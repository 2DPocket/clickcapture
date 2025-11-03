/*
============================================================================
ダイアログを最小化、復元、最前面に移動する関数
============================================================================
 */

use windows::Win32::{
    Graphics::Gdi::UpdateWindow,
    UI::WindowsAndMessaging::{
        HWND_TOP, SW_MINIMIZE, SW_RESTORE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos, ShowWindow,
    },
};

// アプリケーション状態管理構造体
use crate::app_state::AppState;

// ダイアログを最小化
pub fn bring_dialog_to_back() {
    unsafe {
        let app_state = AppState::get_app_state_ref();
        if let Some(safe_hwnd) = app_state.dialog_hwnd {
            let _ = ShowWindow(*safe_hwnd, SW_MINIMIZE);
        }
    }
}

// ダイアログを復元して最前面に移動
pub fn bring_dialog_to_front() {
    unsafe {
        let app_state = AppState::get_app_state_ref();
        if let Some(safe_hwnd) = app_state.dialog_hwnd {
            // 最小化されている場合は復元
            let _ = ShowWindow(*safe_hwnd, SW_RESTORE);
            let _ = UpdateWindow(*safe_hwnd);

            // 最前面に移動
            let _ = SetWindowPos(
                *safe_hwnd,
                Some(HWND_TOP),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE,
            );
        }
    }
}
