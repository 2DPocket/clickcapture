/*
============================================================================
保存先パスエディットボックスハンドラモジュール
============================================================================
*/

use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::*, // ウィンドウとメッセージ処理
};
use windows::core::PCWSTR;

use crate::{app_state::AppState, constants::*, ui::folder_manager::get_pictures_folder};

/// 保存先パスのエディットボックスを初期化
///
/// アプリケーションの初回起動時に、スクリーンショットのデフォルト保存先フォルダを決定し、
/// `AppState` とUI上のエディットボックスに設定します。
///
/// # 引数
/// * `hwnd` - メインダイアログのウィンドウハンドル。
///
/// # 処理内容
/// 1. `folder_manager::get_pictures_folder` を呼び出し、最適な保存先（例: OneDrive/ピクチャ, ローカルのピクチャ）を自動検出します。
/// 2. 検出したパスを `AppState` の `selected_folder_path` に保存します。
/// 3. `SetWindowTextW` を使用して、UIのエディットボックス（`IDC_PATH_EDIT`）にパスを表示します。
pub fn init_path_edit_control(hwnd: HWND) {
    unsafe {
        let app_state = AppState::get_app_state_mut();
        let default_folder = get_pictures_folder();
        app_state.selected_folder_path = Some(default_folder.clone());

        // パステキストボックスに初期値を設定
        if let Ok(path_edit) = GetDlgItem(Some(hwnd), IDC_PATH_EDIT) {
            let default_path = format!("{}\0", default_folder);
            let path_wide: Vec<u16> = default_path.encode_utf16().collect();
            let _ = SetWindowTextW(path_edit, PCWSTR(path_wide.as_ptr()));
        }
    }
}
