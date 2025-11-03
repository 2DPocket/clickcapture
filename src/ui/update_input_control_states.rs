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

/// 各モードに応じて全ボタンの有効/無効を動的制御する関数
///
/// # モード別動作
/// - **通常モード**: エリア選択有効、キャプチャは選択エリア有無で判定
/// - **エリア選択モード**: エリア選択のみ有効（キャンセル用）、他は無効
/// - **キャプチャモード**: キャプチャのみ有効（キャンセル用）、他は無効
/// - **ドラッグ中**: 全ボタン無効（操作完了待ち）
///
/// # 呼び出しタイミング
/// - エリア選択モード開始・終了時
/// - キャプチャモード開始・終了時  
/// - PDF変換開始・終了時
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
        // エリア選択モード中：エリア選択ボタンと閉じるボタンのみ表示
        (true, false, false, false, true, false, false)
    } else if app_state.is_capture_mode {
        // キャプチャモード中：キャプチャボタンと閉じるボタンのみ表示
        (false, true, false, false, true, false, false)
    } else if app_state.is_exporting_to_pdf {
        // PDF変換中：全てのボタンを無効化
        (false, false, false, false, false, false, false)
    } else {
        // 通常モード：エリア選択済みならキャプチャ表示、他は全て表示
        (true, true, true, true, true, true, true)
    };

    // ボタン表示制御関数
    fn set_input_control_status(hwnd: HWND, button_id: i32, enabled: bool) {
        unsafe {
            if let Ok(button) = GetDlgItem(Some(hwnd), button_id) {
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

/// 連続クリック関連コントロールの有効/無効状態を更新
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
