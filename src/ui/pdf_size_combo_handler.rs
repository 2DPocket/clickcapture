/*
============================================================================
PDFサイズコンボボックスハンドラモジュール
============================================================================
*/

// 必要なライブラリ（外部機能）をインポート
use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    UI::WindowsAndMessaging::*,
};

use crate::{app_state::AppState, constants::*};

/// PDFサイズコンボボックスを初期化（20MB〜100MB、20MB刻み）
///
/// # 引数
/// * `hwnd` - ダイアログウィンドウハンドル
///
/// # 機能
/// 1. コンボボックスに選択肢（20, 40, 60, 80, 100）と「最大(1GB)」を追加
/// 2. デフォルト値（20MB）を選択状態に設定
/// 3. AppStateのpdf_max_size_mbと同期
const PDF_FILE_MIN_SIZE_MB: u16 = 20;
const PDF_FILE_MAX_SIZE_MB: u16 = 100;
const PDF_FILE_SIZE_STEP_MB: u16 = 20;
pub fn initialize_pdf_size_combo(hwnd: HWND) {
    if let Ok(combo_hwnd) = unsafe { GetDlgItem(Some(hwnd), IDC_PDF_SIZE_COMBO) } {
        // 20MBから100MBまで20MB刻みで項目を追加
        for &size_mb in (PDF_FILE_MIN_SIZE_MB..=PDF_FILE_MAX_SIZE_MB)
            .step_by(PDF_FILE_SIZE_STEP_MB as usize)
            .collect::<Vec<u16>>()
            .iter()
        {
            let text = format!("{}MB\0", size_mb);
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
            unsafe {
                SendMessageW(
                    combo_hwnd,
                    CB_SETITEMDATA,
                    Some(WPARAM(index)),
                    Some(LPARAM(size_mb as isize)),
                );
            }
        }

        // 無制限オプションを追加
        let unlimited_text = "最大(1GB)\0";
        let unlimited_wide: Vec<u16> = unlimited_text.encode_utf16().collect();
        let index = unsafe {
            SendMessageW(
                combo_hwnd,
                CB_ADDSTRING,
                Some(WPARAM(0)),
                Some(LPARAM(unlimited_wide.as_ptr() as isize)),
            )
        }
        .0 as usize;
        // 1GBをMB単位で設定
        unsafe {
            SendMessageW(
                combo_hwnd,
                CB_SETITEMDATA,
                Some(WPARAM(index)),
                Some(LPARAM(1024)),
            );
        }

        // デフォルト値（20MB）を選択
        // 20MBは最初の項目（インデックス0）
        unsafe {
            SendMessageW(combo_hwnd, CB_SETCURSEL, Some(WPARAM(0)), Some(LPARAM(0)));
        }
    }
}

/// PDF最大サイズコンボボックスの選択変更を処理する
///
/// # 引数
/// * `hwnd` - ダイアログウィンドウハンドル
///
/// # 処理内容
/// 1. `CB_GETCURSEL` で選択された項目のインデックスを取得します。
/// 2. `CB_GETITEMDATA` でその項目に関連付けられたサイズ値（`u16`, MB単位）を取得します。
/// 3. 取得した値を `AppState` の `pdf_max_size_mb` フィールドに保存します。
pub fn handle_pdf_size_combo_change(hwnd: HWND) {
    if let Ok(combo_hwnd) = unsafe { GetDlgItem(Some(hwnd), IDC_PDF_SIZE_COMBO) } {
        // 現在選択されているインデックスを取得
        let selected_index =
            unsafe { SendMessageW(combo_hwnd, CB_GETCURSEL, Some(WPARAM(0)), Some(LPARAM(0))).0 }
                as i32;

        if selected_index >= 0 {
            // 選択された項目のデータを直接取得
            let size_value = unsafe {
                SendMessageW(
                    combo_hwnd,
                    CB_GETITEMDATA,
                    Some(WPARAM(selected_index as usize)),
                    Some(LPARAM(0)),
                )
            }
            .0 as u16;

            // AppStateに保存
            let app_state = AppState::get_app_state_mut();
            app_state.pdf_max_size_mb = size_value as u16;

            println!("PDFサイズ設定変更: {}MB", size_value);
        }
    }
}
