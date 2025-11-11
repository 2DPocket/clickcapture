/*
============================================================================
UIコントロールイベントハンドラ (input_control_handlers.rs)
============================================================================

【ファイル概要】
メインダイアログのUIコントロール（ボタン、コンボボックス、チェックボックスなど）から
発生するイベント（`WM_COMMAND`）を処理する関数群を提供します。
`main.rs`のダイアログプロシージャから呼び出され、ユーザーの操作に応じて
アプリケーションの状態 (`AppState`) を更新したり、特定の機能を実行したりします。

【主要機能】
1.  **PDF変換処理 (`handle_pdf_export_button`)**:
    -   ユーザーに確認ダイアログを表示し、同意が得られればPDF変換プロセスを開始します。
    -   処理中は砂時計カーソルを表示し、他のUI操作を無効化します。
2.  **設定コンボボックスの変更処理**:
    -   画像スケール、JPEG品質、PDF最大サイズなどのコンボボックスの選択内容を `AppState` に反映させます。
3.  **自動クリック関連のUI処理**:
    -   自動クリックの有効/無効チェックボックス、間隔、回数の設定を `AppState` に同期させます。
4.  **`update_input_control_states`**:
    -   `AppState` から現在のモードフラグ（`is_area_select_mode`, `is_capture_mode`など）を読み取ります。
    -   モードに応じて、各UIコントロールが有効であるべきか無効であるべきかを決定します。
    -   `EnableWindow` API を使用して、各コントロールの状態を実際に変更します。
    -   ユーザーが状況に応じて適切な操作のみを行えるようにUIを動的に制御します。
5.  **`update_auto_click_controls_state`**:
    -   自動クリック機能に関連するコントロール（間隔コンボボックス、回数エディットボックス）の
        有効/無効状態を、自動クリックチェックボックスの状態に同期させます。


【AI解析用：依存関係】
- `main.rs`: `dialog_proc` 内の `WM_COMMAND` メッセージハンドラからこのモジュールの関数を呼び出す。
- `app_state.rs`: ユーザーの選択に応じて `AppState` の各フィールドを更新する。
- `export_pdf.rs`: PDF変換ボタンが押されたときに `export_selected_folder_to_pdf` を呼び出す。
- `system_utils.rs`: 確認ダイアログや結果通知のメッセージボックスを表示するために使用。
- `update_input_control_states.rs`: UIコントロールの有効/無効状態を更新するために使用。
 */

// 必要なライブラリ（外部機能）をインポート
use windows::Win32::{
    Foundation::HWND,
    Graphics::Gdi::{InvalidateRect, UpdateWindow},
    UI::{Input::KeyboardAndMouse::EnableWindow, WindowsAndMessaging::*},
};

use crate::{
    app_state::AppState, constants::*,
    ui::auto_click_checkbox_handler::update_auto_click_controls_state,
};

/// オーナードローボタンの初期化
///
/// すべてのオーナードローボタン（アイコンボタン）に対して、
/// マウスカーソルが乗ったときに標準の矢印カーソルから手のひらカーソルに
/// 変わるように設定します。これにより、クリック可能であることが視覚的に分かりやすくなります。
///
/// # 引数
/// * `hwnd` - メインダイアログのウィンドウハンドル。
///
/// # 処理内容
/// 1. `LoadCursorW` で `IDC_HAND`（手のひら）カーソルを読み込みます。
/// 2. `GetDlgItem` で各ボタンのハンドルを取得します。
/// 3. `SetClassLongPtrW` を使用して、各ボタンのウィンドウクラスに `GCLP_HCURSOR` を設定します。
pub fn initialize_icon_button(hwnd: HWND) {
    unsafe {
        // 手のひらカーソルを読み込み
        let hand_cursor = LoadCursorW(None, IDC_HAND).unwrap_or_default();

        // 各アイコンボタンにカスタムカーソルを設定
        if let Ok(button) = GetDlgItem(Some(hwnd), IDC_CAPTURE_START_BUTTON) {
            let _ = InvalidateRect(Some(button), None, true);
            let _ = SetClassLongPtrW(button, GET_CLASS_LONG_INDEX(-12), hand_cursor.0 as isize);
        }
        if let Ok(button) = GetDlgItem(Some(hwnd), IDC_AREA_SELECT_BUTTON) {
            let _ = InvalidateRect(Some(button), None, true);
            let _ = SetClassLongPtrW(button, GET_CLASS_LONG_INDEX(-12), hand_cursor.0 as isize);
        }
        if let Ok(button) = GetDlgItem(Some(hwnd), IDC_BROWSE_BUTTON) {
            let _ = InvalidateRect(Some(button), None, true);
            let _ = SetClassLongPtrW(button, GET_CLASS_LONG_INDEX(-12), hand_cursor.0 as isize);
        }
        if let Ok(button) = GetDlgItem(Some(hwnd), IDC_CLOSE_BUTTON) {
            let _ = InvalidateRect(Some(button), None, true);
            let _ = SetClassLongPtrW(button, GET_CLASS_LONG_INDEX(-12), hand_cursor.0 as isize);
        }
        if let Ok(button) = GetDlgItem(Some(hwnd), IDC_EXPORT_PDF_BUTTON) {
            let _ = InvalidateRect(Some(button), None, true);
            let _ = SetClassLongPtrW(button, GET_CLASS_LONG_INDEX(-12), hand_cursor.0 as isize);
        }
    }
}

/// アプリケーションのモードに応じて、全てのUIコントロールの有効/無効状態を更新する
///
/// # モード別の状態
/// - **通常モード**: ほとんどのコントロールが有効になります。
/// - **エリア選択モード**: 「エリア選択」ボタン（キャンセルとして機能）と「閉じる」ボタンのみ有効になります。
/// - **キャプチャモード**: 「キャプチャ開始」ボタン（キャンセルとして機能）と「閉じる」ボタンのみ有効になります。
/// - **PDF変換中**: 全てのコントロールが無効になり、処理に集中させます。
///
/// # 呼び出しタイミング
/// モードが変更されるたびに呼び出され、UIの状態をアプリケーションの内部状態と同期させます。
/// 例えば、エリア選択モードの開始/終了時や、キャプチャモードの開始/終了時などです。
pub fn update_input_control_states() {
    let app_state = AppState::get_app_state_ref();

    // ダイアログハンドルを取得
    let hwnd = match app_state.dialog_hwnd {
        Some(safe_hwnd) => *safe_hwnd,
        None => return, // ダイアログが初期化されていない場合は何もしない
    };

    // モード判定とボタン状態決定
    let (
        area_select_enable,
        capture_enable,
        browse_enable,
        export_pdf_enable,
        close_enable,
        auto_click_enable,
        property_combobox_enable,
    ) = if app_state.is_area_select_mode {
        // エリア選択モード中：「エリア選択」ボタン（キャンセル用）と「閉じる」ボタンのみ有効
        (true, false, false, false, true, false, false)
    } else if app_state.is_capture_mode {
        // キャプチャモード中：「キャプチャ開始」ボタン（キャンセル用）と「閉じる」ボタンのみ有効
        (false, true, false, false, true, false, false)
    } else if app_state.is_exporting_to_pdf {
        // PDF変換中：全てのコントロールを無効化
        (false, false, false, false, false, false, false)
    } else {
        // 通常モード：エリア選択済みならキャプチャ表示、他は全て表示
        (true, true, true, true, true, true, true)
    };

    // ボタン表示制御関数
    fn set_input_control_status(hwnd: HWND, control_id: i32, enabled: bool) {
        unsafe {
            if let Ok(button) = GetDlgItem(Some(hwnd), control_id) {
                let _ = EnableWindow(button, enabled);
                // InvalidateRectはオーナードローボタンには有効だが、標準コントロールの
                // グレーアウト状態を即座に反映させるにはUpdateWindowで強制的に再描画を促すのが確実。
                let _ = InvalidateRect(Some(button), None, true); // オーナードローボタンのために残す
                let _ = UpdateWindow(button); // 標準コントロールのために追加
            }
        }
    }

    // 各ボタンの表示制御
    set_input_control_status(hwnd, IDC_AREA_SELECT_BUTTON, area_select_enable);
    set_input_control_status(hwnd, IDC_CAPTURE_START_BUTTON, capture_enable);
    set_input_control_status(hwnd, IDC_BROWSE_BUTTON, browse_enable);
    set_input_control_status(hwnd, IDC_EXPORT_PDF_BUTTON, export_pdf_enable);
    set_input_control_status(hwnd, IDC_CLOSE_BUTTON, close_enable);
    set_input_control_status(hwnd, IDC_AUTO_CLICK_CHECKBOX, auto_click_enable);

    // プロパティコンボボックス群の有効/無効制御
    set_input_control_status(hwnd, IDC_SCALE_COMBO, property_combobox_enable);
    set_input_control_status(hwnd, IDC_QUALITY_COMBO, property_combobox_enable);
    set_input_control_status(hwnd, IDC_PDF_SIZE_COMBO, property_combobox_enable);

    // 自動クリックの設定が有効な場合、関連コントロールを有効化
    if auto_click_enable {
        update_auto_click_controls_state(hwnd);
    } else {
        set_input_control_status(hwnd, IDC_AUTO_CLICK_INTERVAL_COMBO, false);
        set_input_control_status(hwnd, IDC_AUTO_CLICK_COUNT_EDIT, false);
    }

    // デバッグログ出力
    println!(
        "ボタン表示状態更新: エリア選択={}, キャプチャ={}, 参照(フォルダー選択)={}, PDF={}, 閉じる={}, 自動クリック={}",
        area_select_enable,
        capture_enable,
        browse_enable,
        export_pdf_enable,
        close_enable,
        auto_click_enable
    );
}
