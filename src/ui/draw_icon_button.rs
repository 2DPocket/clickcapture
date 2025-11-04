/*
============================================================================
アイコンボタン描画機能群
============================================================================
 */

// 必要なライブラリ（外部機能）をインポート
use windows::{
    Win32::{
        Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, RECT, WPARAM}, Graphics::Gdi::*, System:: 
            LibraryLoader::GetModuleHandleW, UI::{
            Controls::DRAWITEMSTRUCT, WindowsAndMessaging::*, // メモリストリーム作成
        } // リソースタイプ定義
    },
    core::PCWSTR, // Windows API用の文字列操作
};

// アプリケーション状態管理構造体
use crate::app_state::*;

// 定数群インポート
use crate::constants::*;


// アイコンボタン描画制御ハンドラ
pub fn draw_icon_button_handler(_hwnd: HWND, _wparam: WPARAM, lparam: LPARAM) {
    unsafe {
        let draw_item = lparam.0 as *const DRAWITEMSTRUCT;
        if draw_item.is_null() {
            return;
        }

        let draw_struct = &*draw_item;

        // ボタンのIDに応じて処理を分岐
        let app_state = AppState::get_app_state_ref();
        match draw_struct.CtlID {
            id if id == IDC_CAPTURE_START_BUTTON as u32 => {
                // キャプチャ開始ボタンの描画
                let is_capture_mode = app_state.is_capture_mode;
                draw_icon_button(draw_struct, is_capture_mode, IDI_CAMERA_ON, IDI_CAMERA_OFF);
            }
            id if id == IDC_AREA_SELECT_BUTTON as u32 => {
                // エリア選択ボタンの描画
                let is_area_select_mode = app_state.is_area_select_mode;
                draw_icon_button(
                    draw_struct,
                    is_area_select_mode,
                    IDI_SELECT_AREA_ON,
                    IDI_SELECT_AREA_OFF,
                );
            }
            id if id == IDC_BROWSE_BUTTON as u32 => {
                // 参照ボタンの描画（常にIDI_SELECT_FOLDERアイコンを表示）
                draw_icon_button(draw_struct, false, IDI_SELECT_FOLDER, IDI_SELECT_FOLDER);
            }
            id if id == IDC_EXPORT_PDF_BUTTON as u32 => {
                // PDF変換ボタンの描画（常にIDI_EXPORT_PFGアイコンを表示）
                draw_icon_button(draw_struct, false, IDI_EXPORT_PDF, IDI_EXPORT_PDF);
            }
            id if id == IDC_CLOSE_BUTTON as u32 => {
                // 閉じるボタンの描画（常にIDI_CLOSEアイコンを表示）
                draw_icon_button(draw_struct, false, IDI_CLOSE, IDI_CLOSE);
            }
            _ => {} // その他のコントロールは処理しない
        }
    }
}

// アイコンボタンを描画する共通関数
pub fn draw_icon_button(
    draw_struct: &DRAWITEMSTRUCT,
    is_active: bool,
    active_icon_id: i32,
    inactive_icon_id: i32,
) {
    unsafe {
        let hdc = draw_struct.hDC;
        let rect = draw_struct.rcItem;

        // 1. ボタン背景を描画
        let bg_color = if is_active {
            COLORREF(0xE0E0E0) // 押下状態
        } else {
            COLORREF(0xF0F0F0) // 通常状態
        };

        let bg_brush = CreateSolidBrush(bg_color);
        FillRect(hdc, &rect, bg_brush);
        let _ = DeleteObject(bg_brush.into());

        // 2. アイコンを直接描画（変換処理不要）
        let icon_id = if is_active {
            active_icon_id
        } else {
            inactive_icon_id
        };

        if let Some(hicon) = load_icon_from_resource(icon_id) {
            let icon_size = 32;
            let x = rect.left + (rect.right - rect.left - icon_size) / 2;
            let y = rect.top + (rect.bottom - rect.top - icon_size) / 2;

            // アイコンを直接描画（これだけ！）
            let _ = DrawIconEx(hdc, x, y, hicon, icon_size, icon_size, 0, None, DI_NORMAL);

            // アイコンリソースを解放
            let _ = DestroyIcon(hicon);
        }

        // 3. 境界線を描画
        draw_button_border(hdc, &rect);
    }
}

// リソースからビットマップをHBITMAPとして読み込む関数
// アイコンリソースからHBITMAPとして読み込む関数
pub fn load_icon_from_resource(resource_id: i32) -> Option<HICON> {
    unsafe {
        let hmodule = GetModuleHandleW(None).ok()?;

        LoadImageW(
            Some(HINSTANCE(hmodule.0)),
            PCWSTR(resource_id as usize as *const u16),
            IMAGE_ICON, // アイコンとして直接読み込み
            32,
            32,
            LR_DEFAULTCOLOR,
        )
        .ok()
        .map(|handle| HICON(handle.0))
    }
}

// 境界線描画（共通処理）
pub fn draw_button_border(hdc: HDC, rect: &RECT) {
    unsafe {
        let pen = CreatePen(PS_SOLID, 1, COLORREF(0xacacac));
        let old_pen = SelectObject(hdc, pen.into());
        let old_brush = SelectObject(hdc, GetStockObject(NULL_BRUSH));

        let _ = Rectangle(hdc, rect.left, rect.top, rect.right, rect.bottom);

        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_brush);
        let _ = DeleteObject(pen.into());
    }
}