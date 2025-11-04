/*
============================================================================
ダイアログ制御ハンドラ (dialog_handlers.rs)
============================================================================

【ファイル概要】
メインダイアログウィンドウの表示状態（Zオーダー、最小化/復元）を制御する
ヘルパー関数群を提供します。
エリア選択モードやキャプチャモード時に、メインダイアログが他のウィンドウの
邪魔にならないように最小化し、モード終了時に元の状態に復元して最前面に
表示するために使用されます。

【主要機能】
1.  **ダイアログの最小化 (`bring_dialog_to_back`)**:
    -   `ShowWindow` API (SW_MINIMIZE) を使用して、メインダイアログをタスクバーに最小化します。
    -   オーバーレイ表示時に、メインダイアログがキャプチャ対象の邪魔にならないようにします。

2.  **ダイアログの復元と最前面表示 (`bring_dialog_to_front`)**:
    -   `ShowWindow` API (SW_RESTORE) で最小化状態から復元します。
    -   `SetWindowPos` API (HWND_TOP) でウィンドウをZオーダーの最前面に移動させ、ユーザーがすぐに操作できるようにします。

【技術仕様】
-   `AppState` からグローバルなダイアログハンドルを取得して操作します。
-   Win32 APIを直接呼び出して、ウィンドウの状態を効率的に変更します。

【AI解析用：依存関係】
-   `app_state.rs`: `AppState` からダイアログハンドルを取得。
-   `area_select.rs`, `screen_capture.rs`: モードの開始/終了時にこのモジュールの関数を呼び出す。
 */

use windows::Win32::{
    Graphics::Gdi::UpdateWindow,
    UI::WindowsAndMessaging::{
        HWND_TOP, SW_MINIMIZE, SW_RESTORE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos, ShowWindow,
    },
};

// アプリケーション状態管理構造体
use crate::app_state::AppState;

/// メインダイアログを最小化して背面に送る
///
/// エリア選択モードやキャプチャモードが開始される際に呼び出され、
/// メインダイアログがオーバーレイ表示や画面操作の邪魔にならないように
/// タスクバーへ最小化します。
///
/// # 処理内容
/// - `AppState` からダイアログハンドルを取得します。
/// - `ShowWindow` APIに `SW_MINIMIZE` フラグを渡してウィンドウを最小化します。
pub fn bring_dialog_to_back() {
    unsafe {
        let app_state = AppState::get_app_state_ref();
        if let Some(safe_hwnd) = app_state.dialog_hwnd {
            let _ = ShowWindow(*safe_hwnd, SW_MINIMIZE);
        }
    }
}

/// ダイアログを復元して最前面に表示する
///
/// エリア選択モードやキャプチャモードが終了した際に呼び出され、
/// 最小化されていたメインダイアログを元のサイズに戻し、
/// ユーザーがすぐに操作できるようにZオーダーの最前面に配置します。
///
/// # 処理内容
/// 1. `AppState` からダイアログハンドルを取得します。
/// 2. `ShowWindow` APIに `SW_RESTORE` フラグを渡し、ウィンドウを最小化前の状態に復元します。
/// 3. `UpdateWindow` を呼び出し、ウィンドウの再描画を促します。
/// 4. `SetWindowPos` APIに `HWND_TOP` フラグを渡し、ウィンドウをZオーダーの最前面に移動させます。
///    `SWP_NOMOVE | SWP_NOSIZE` を指定することで、位置やサイズは変更せずにZオーダーのみを更新します。
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
