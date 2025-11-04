/*
============================================================================
UIコントロール状態更新モジュール (update_input_control_states.rs)
============================================================================

【ファイル概要】
アプリケーションの現在のモード（通常、エリア選択、キャプチャ、PDF変換中など）に応じて、
メインダイアログ上のUIコントロール（ボタンやコンボボックス）の有効/無効状態を
一括で更新する機能を提供します。

【主要機能】
1.  **`update_input_control_states`**:
    -   `AppState` から現在のモードフラグ（`is_area_select_mode`, `is_capture_mode`など）を読み取ります。
    -   モードに応じて、各UIコントロールが有効であるべきか無効であるべきかを決定します。
    -   `EnableWindow` API を使用して、各コントロールの状態を実際に変更します。
    -   ユーザーが状況に応じて適切な操作のみを行えるようにUIを動的に制御します。

2.  **`update_auto_click_controls_state`**:
    -   自動クリック機能に関連するコントロール（間隔コンボボックス、回数エディットボックス）の
        有効/無効状態を、自動クリックチェックボックスの状態に同期させます。

【呼び出しタイミング】
-   モードが切り替わるタイミング（エリア選択開始/終了、キャプチャ開始/終了など）。
-   PDF変換処理の開始時および終了時。
-   自動クリックチェックボックスの状態が変更された時。

【AI解析用：依存関係】
- `app_state.rs`: アプリケーションの現在のモードを読み取るために使用。
- `constants.rs`: UIコントロールのID定義。
- `area_select.rs`, `screen_capture.rs`, `input_control_handlers.rs`: モード変更時にこのモジュールの関数を呼び出す。
 */

use windows::Win32::{
    Foundation::HWND, // 基本的なデータ型
    Graphics::Gdi::*, // グラフィック描画機能
    UI::{
        Input::KeyboardAndMouse::EnableWindow,
        WindowsAndMessaging::*, // ウィンドウとメッセージ処理
    },
};

use crate::app_state::AppState;

use crate::constants::*;

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

/// 自動連続クリック関連コントロールの有効/無効状態を更新する
///
/// 自動クリックチェックボックスの状態（有効/無効）に合わせて、
/// 関連するコントロール（間隔コンボボックス、回数エディットボックス）の
/// 有効/無効状態を同期させます。
///
/// # 引数
/// * `hwnd` - メインダイアログのウィンドウハンドル。
pub fn update_auto_click_controls_state(hwnd: HWND) {
    unsafe {
        let app_state = AppState::get_app_state_ref();
        let is_enabled = app_state.auto_clicker.is_enabled();

        // 関連コントロールの有効/無効を切り替え
        let _ = EnableWindow(
            GetDlgItem(Some(hwnd), IDC_AUTO_CLICK_INTERVAL_COMBO).unwrap(),
            is_enabled,
        );
        let _ = EnableWindow(
            GetDlgItem(Some(hwnd), IDC_AUTO_CLICK_COUNT_EDIT).unwrap(),
            is_enabled,
        );
    }
}
