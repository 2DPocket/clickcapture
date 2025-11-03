/*
============================================================================
UI部品描画、管理関数
============================================================================
 */

// 必要なライブラリ（外部機能）をインポート
use windows::{
    Win32::{
        Foundation::{HINSTANCE, COLORREF, RECT}, // 基本的なデータ型
        Graphics::Gdi::*,             // GDIグラフィック描画機能
        UI::{
            Controls::DRAWITEMSTRUCT, WindowsAndMessaging::*, // ウィンドウとメッセージ処理
            Shell::SHCreateMemStream, // メモリストリーム作成
        },
        System:: {
            Com::IStream,
            LibraryLoader::{FindResourceW, GetModuleHandleW, LoadResource, LockResource, SizeofResource},
        },
        Media::KernelStreaming::RT_RCDATA, // リソースタイプ定義
    },
    core::PCWSTR, // Windows API用の文字列操作
};

// GDI+機能群のインポート
use windows::Win32::Graphics::GdiPlus::{
    GdipCreateBitmapFromStream, GpBitmap, Status
};

use std::slice;

// アプリケーション状態管理構造体
use crate::app_state::AppState;

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

/// リソースからPNGを読み込み、GDI+ビットマップを作成するヘルパー関数
pub fn load_png_from_resource(resource_id: PCWSTR) -> Result<*mut GpBitmap, String> {
    unsafe {

        let hinstance = GetModuleHandleW(None).map_err(|e| e.to_string())?;

        // 1. リソースを検索 (同じ)
        let resource_handle = FindResourceW(Some(hinstance), resource_id, RT_RCDATA);
        if resource_handle.0 == std::ptr::null_mut() {
            return Err("FindResourceW failed".to_string());
        }

        // 2. リソースをロード (同じ)
        let loaded_resource = LoadResource(Some(hinstance), resource_handle)
            .map_err(|e| format!("LoadResource failed: {}", e))?;

        // 3. ポインタを取得 (同じ)
        let resource_ptr = LockResource(loaded_resource);
        if resource_ptr.is_null() {
            return Err("LockResource failed".to_string());
        }

        // 4. サイズを取得 (同じ)
        let resource_size = SizeofResource(Some(hinstance), resource_handle);
        if resource_size == 0 {
            return Err("SizeofResource returned 0".to_string());
        }

        // 5. ポインタとサイズからRustのスライスを作成
        //    (unsafe ブロック内にいる前提です)
        let data_slice: &[u8] = slice::from_raw_parts(
            resource_ptr as *const u8,
            resource_size as usize, // u32 を usize にキャスト
        );

        // 6. スライスを渡して IStream を作成 (引数は1つ)
        // SHCreateMemStream は内部でメモリを確保・コピーし、
        // 解放時に自動でメモリを解放するストリームを作成します。
        let stream: Option<IStream> = SHCreateMemStream(Some(data_slice));

        if stream.is_none() {
            return Err("SHCreateMemStream failed".to_string());
        }

        // 6. IStreamからGDI+ビットマップを作成 (同じ)
        let mut bitmap: *mut GpBitmap = std::ptr::null_mut();
        let stream_ref = stream.as_ref().expect("IStream is None");
        let status = GdipCreateBitmapFromStream(stream_ref, &mut bitmap);

        if status != Status(0) {
            return Err(format!("GdipCreateBitmapFromStream failed with status {:?}", status));
        }
        
        Ok(bitmap)

    }
}


