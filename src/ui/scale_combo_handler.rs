/*
============================================================================
スケールコンボボックスハンドラモジュール
============================================================================
*/
// 必要なライブラリ（外部機能）をインポート
use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    UI::WindowsAndMessaging::*,
};

use crate::{app_state::AppState, constants::*};

/// スケールコンボボックスを初期化（100%〜55%、5%刻み）
///
/// キャプチャ画像の縮小率を設定するコンボボックスに、55%から100%までの選択肢を5%刻みで追加します。
/// デフォルト値として、画質とファイルサイズのバランスが良い65%を選択状態にします。
///
/// # 引数
/// * `hwnd` - ダイアログウィンドウハンドル。
///
/// # 処理内容
/// - `CB_ADDSTRING` で表示テキストを追加し、`CB_SETITEMDATA` で実際のスケール値（`u8`）を各項目に関連付けます。
/// - `CB_SETCURSEL` でデフォルトの項目を選択します。`AppState` の `capture_scale_factor` のデフォルト値と一致させます。
pub fn initialize_scale_combo(hwnd: HWND) {
    if let Ok(combo_hwnd) = unsafe { GetDlgItem(Some(hwnd), IDC_SCALE_COMBO) } {
        // 55%から100%まで5%刻みで項目を追加
        let scales: Vec<u8> = (55..=100).step_by(5).collect();

        for &scale in scales.iter().rev() {
            let text = format!("{}%\0", scale);
            let wide_text: Vec<u16> = text.encode_utf16().collect();
            let index = unsafe {
                SendMessageW(
                    combo_hwnd,
                    CB_ADDSTRING,
                    Some(WPARAM(0)),
                    Some(LPARAM(wide_text.as_ptr() as isize)),
                )
            }
            .0 as usize;
            // 各項目に実際のスケール値をデータとして設定
            unsafe {
                SendMessageW(
                    combo_hwnd,
                    CB_SETITEMDATA,
                    Some(WPARAM(index)),
                    Some(LPARAM(scale as isize)),
                );
            }
        }

        // デフォルト値（65%）を選択
        // 65%は (100-65)/5 = 7番目のインデックス（0ベース）
        let default_index = (100 - 65) / 5;
        unsafe {
            SendMessageW(
                combo_hwnd,
                CB_SETCURSEL,
                Some(WPARAM(default_index as usize)),
                Some(LPARAM(0)),
            );
        }
    }
}

/// 画像スケールコンボボックスの選択変更を処理する
///
/// # 引数
/// * `hwnd` - ダイアログウィンドウハンドル
///
/// # 処理内容
/// 1. `CB_GETCURSEL` で選択された項目のインデックスを取得します。
/// 2. `CB_GETITEMDATA` でその項目に関連付けられたスケール値（`u8`）を取得します。
/// 3. 取得した値を `AppState` の `capture_scale_factor` フィールドに保存します。
pub fn handle_scale_combo_change(hwnd: HWND) {
    if let Ok(combo_hwnd) = unsafe { GetDlgItem(Some(hwnd), IDC_SCALE_COMBO) } {
        // 現在選択されているインデックスを取得
        let selected_index =
            unsafe { SendMessageW(combo_hwnd, CB_GETCURSEL, Some(WPARAM(0)), Some(LPARAM(0))).0 }
                as i32;

        if selected_index >= 0 {
            // 選択された項目のデータを直接取得
            let scale_value = unsafe {
                SendMessageW(
                    combo_hwnd,
                    CB_GETITEMDATA,
                    Some(WPARAM(selected_index as usize)),
                    Some(LPARAM(0)),
                )
            }
            .0 as u8;

            // AppStateに保存
            let app_state = AppState::get_app_state_mut();
            app_state.capture_scale_factor = scale_value as u8;

            println!("スケール設定変更: {}%", scale_value);
        }
    }
}
