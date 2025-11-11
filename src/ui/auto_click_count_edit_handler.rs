/*
============================================================================
自動クリック回数エディットボックスハンドラモジュール
============================================================================
*/

use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::*, // ウィンドウとメッセージ処理
};

use crate::{app_state::AppState, constants::*};

/// 自動クリック回数エディットボックスの変更を処理する
///
/// # 引数
/// * `hwnd` - ダイアログウィンドウハンドル
///
/// # 処理内容
/// エディットボックスからフォーカスが外れた（`EN_KILLFOCUS`）際に、入力されたテキストを数値に変換し、`AppState` の `auto_clicker` に最大実行回数として設定します。
pub fn handle_auto_click_count_edit_change(hwnd: HWND) {
    unsafe {
        if let Ok(edit_hwnd) = GetDlgItem(Some(hwnd), IDC_AUTO_CLICK_COUNT_EDIT) {
            // テキストを取得
            let mut buffer: [u16; 16] = [0; 16];
            let text_length = GetWindowTextW(edit_hwnd, &mut buffer);
            if text_length == 0 {
                return; // テキストが空の場合は何もしない
            }

            let text = String::from_utf16_lossy(&buffer[..text_length as usize]);
            // 数値に変換
            if let Ok(count) = text.trim().parse::<u32>() {
                let app_state = AppState::get_app_state_mut();
                app_state.auto_clicker.set_max_count(count);
                println!("自動クリック回数設定変更: {}", count);
            }
        }
    }
}
