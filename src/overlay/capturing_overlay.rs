/*
============================================================================
キャプチャモードオーバーレイモジュール (capturing_overlay.rs)
============================================================================
*/

// GDI+関連のライブラリ（外部機能）をインポート
use windows::Win32::Graphics::GdiPlus::{
    Color, CompositingModeSourceCopy, CompositingModeSourceOver,
    GdipCreateFont, GdipCreateFontFamilyFromName, GdipCreateSolidFill, GdipCreateStringFormat, 
    GdipDeleteBrush, GdipDeleteFont, GdipDeleteFontFamily, GdipDeleteStringFormat, GdipDisposeImage, GdipDrawImageRectI, 
    GdipDrawString, GdipFillRectangleI, GdipSetCompositingMode, GdipSetStringFormatAlign, GdipSetStringFormatLineAlign, 
    GpBitmap, GpFont, GpGraphics, GpSolidFill, GpStringFormat, RectF, Status, StringAlignmentCenter
};
// 必要なライブラリをインポート
use windows::{
    Win32::{
        Foundation::HWND, // 基本的なデータ型
        UI::WindowsAndMessaging::*,
    },
    core::{PCWSTR}, // Windows API用の文字列操作
};

// アプリケーション状態管理構造体
use crate::app_state::*;

// リソースID定数をインポート  
use crate::constants::*;

// オーバーレイ共通機能モジュール
use crate::overlay::*;

// UIユーティリティ群
use crate::ui::ui_utils::*;

// オーバーレイウィンドウのサイズ
const WIN_SIZE: (i32, i32) = (230, 90); // 230x90ピクセル

// アイコンの描画サイズ (constants.rsで定義しても良い)
const ICON_DRAW_SIZE: i32 = 32; 

/// キャプチャモードオーバーレイ構造体
#[derive(Debug)]
pub struct CapturingOverLay {
    hwnd: Option<SafeHWND>,
    font: *mut GpFont,
    transparent_brush: *mut GpSolidFill,
    string_format: *mut GpStringFormat,
    back_ground_brush: *mut GpSolidFill,
    back_orange_brush: *mut GpSolidFill,
    wait_bitmap: *mut GpBitmap,
    processing_bitmap: *mut GpBitmap,
}

/// キャプチャモードオーバーレイ構造体実装
impl CapturingOverLay {
    pub fn new() -> Self {
        let mut overlay = CapturingOverLay { 
            hwnd: None,
            transparent_brush: std::ptr::null_mut(),
            font: std::ptr::null_mut(),
            back_ground_brush: std::ptr::null_mut(),
            back_orange_brush: std::ptr::null_mut(),
            string_format: std::ptr::null_mut(), 
            wait_bitmap: std::ptr::null_mut(),
            processing_bitmap: std::ptr::null_mut(),
        };

        // GDI+リソースの初期化

        // 透明ブラシ作成
        unsafe {
            let transparent_color = Color { Argb: 0x00000000 }; // Alpha=0
            let status = GdipCreateSolidFill(transparent_color.Argb, &mut overlay.transparent_brush);
            if status != Status(0) {
                eprintln!("❌ GdipCreateSolidFill for transparent_brush failed with status {:?}", status);
            }
        }

        // フォント作成
        let font_family_name: Vec<u16> = "Yu Gothic UI".encode_utf16().chain(std::iter::once(0)).collect();

        unsafe {
            // フォント作成
            let mut font_family: *mut _ = std::ptr::null_mut();
            let status =  GdipCreateFontFamilyFromName(PCWSTR(font_family_name.as_ptr()), 
                std::ptr::null_mut(),
                &mut font_family);

            if status != Status(0) {
                eprintln!("❌ GdipCreateFontFamilyFromName failed in CapturingOverLay::new() with status: {:?}", status);
            }

            // フォントサイズ13でフォント作成
            let status =GdipCreateFont( font_family, 16.0, Default::default(), Default::default(),  &mut overlay.font);
            if status != Status(0) {
                eprintln!("❌ GdipCreateFont failed in CapturingOverLay::new() with status: {:?}", status);
            }
            // フォントファミリーの削除
            GdipDeleteFontFamily(font_family);
        }

        unsafe {
            // ラベル背景用のブラシ作成
            let orange_color = Color { Argb: 0xFFDEB887 }; // 不透明 (burlywood)            
            let status = GdipCreateSolidFill(orange_color.Argb, &mut overlay.back_orange_brush);
            if status != Status(0) {
                eprintln!("❌ GdipCreateSolidFill for orange background failed in CapturingOverLay::new() with status: {:?}", status);
            }

            // ラベル文字用の黒ブラシ作成
            let black_color = Color { Argb: 0xFF000000 }; // 不透明な黒
            let status = GdipCreateSolidFill(black_color.Argb, &mut overlay.back_ground_brush);
            if status != Status(0) {
                eprintln!("❌ GdipCreateSolidFill for black background failed in CapturingOverLay::new() with status: {:?}", status);
            }

            let status = GdipCreateStringFormat(0, 0, &mut overlay.string_format);
            if status != Status(0) {
                eprintln!("❌ GdipCreateStringFormat failed in CapturingOverLay::new() with status: {:?}", status);
            }
        }

        // ビットマップの読み込み
        if let Ok(bitmap) = load_png_from_resource(PCWSTR(IDP_CAPTURE_WAITING as usize as *const u16)) {
            overlay.wait_bitmap = bitmap;
        } else {
            eprintln!("❌ Failed to load PNG resource: IDP_CAPTURE_WAITING");
        }

        if let Ok(bitmap) = load_png_from_resource(PCWSTR(IDP_CAPTURE_PROCESSING as usize as *const u16)) {
            overlay.processing_bitmap = bitmap;
        } else {
            eprintln!("❌ Failed to load PNG resource: IDP_CAPTURE_PROCESSING");
        }

        overlay


    }
}


impl Drop for CapturingOverLay {
    fn drop(&mut self) {
        self.destroy_overlay();

        // GDI+リソースの解放
        unsafe {
            GdipDeleteBrush(self.transparent_brush as *mut _);
            GdipDeleteFont(self.font);
            GdipDeleteStringFormat(self.string_format);
            GdipDeleteBrush(self.back_ground_brush as *mut _);
            GdipDeleteBrush(self.back_orange_brush as *mut _);

            GdipDisposeImage(self.wait_bitmap as *mut _);
            GdipDisposeImage(self.processing_bitmap as *mut _);
        }
    }
}

/// Overlayトレイト実装
impl Overlay for CapturingOverLay {

    fn set_hwnd(&mut self, hwnd: Option<SafeHWND>) {
        self.hwnd = hwnd;
    }
    fn get_hwnd(&self) -> Option<SafeHWND> {
        self.hwnd.clone()
    }
    fn get_overlay_name(&self) -> &str {
        "Capturing"
    }
    fn get_description(&self) -> &str {
        "キャプチャモードオーバーレイ"
    }
    fn get_window_proc(&self) -> OverlayWindowProc {
        OverlayWindowProc {
            create: None,
            paint: Some(overlay_window_paint),
            destroy: None,
        }
    } 

    fn get_class_params(&self) -> OverlayWindowClassParams {
        OverlayWindowClassParams::default()
    }

    fn get_window_params(&self) -> OverlayWindowParams {
        // オーバーレイウィンドウを作成（WS_EX_TRANSPARENTを削除、マウスイベントを背後に通さないため）
        let mut params = OverlayWindowParams::default();
        params = OverlayWindowParams
        {
            dwex_style: WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT,
            width:  WIN_SIZE.0,
            height: WIN_SIZE.1,
            ..params
        };
        params
    }

    // オーバーレイウィンドウの位置設定
    fn set_window_pos(&self) {
        unsafe {
            let app_state = AppState::get_app_state_mut();

            let size = WIN_SIZE;
            // let offset = size / 2;
            let offset = ICON_DRAW_SIZE;
            let screen_x = app_state.current_mouse_pos.x;
            let screen_y = app_state.current_mouse_pos.y;

            if let Some(hwnd) = self.hwnd {
                let _ = SetWindowPos(
                    *hwnd,
                    Some(HWND_TOPMOST),
                    screen_x - offset,
                    screen_y - offset,
                    size.0, size.1,
                    SWP_NOACTIVATE,
                );
            }
        }
    }    

}

fn overlay_window_paint(_hwnd: HWND, graphics: *mut GpGraphics) {
    let app_state = AppState::get_app_state_ref();
    let overlay = app_state.capturing_overlay.as_ref()
        .expect("キャプチャーオーバーレイが存在しません。");

    unsafe {
        // 背景を透明でクリア (GDI+で透明な背景を確保)
        // LayeredWindowのLWA_COLORKEYで背景色を透明にしているが、
        // GDI+の描画はDIBSection上で行われるため、明示的にクリアする
        // CompositingModeSourceCopy を使用して、既存のピクセルを上書きし、アルファ値を考慮しない
        // これにより、DIBの該当領域が完全に透明になる
        GdipSetCompositingMode(graphics, CompositingModeSourceCopy);
        GdipFillRectangleI(graphics, overlay.transparent_brush as *mut _, 0, 0, WIN_SIZE.0, WIN_SIZE.1);
        // 描画モードを元に戻す
        GdipSetCompositingMode(graphics, CompositingModeSourceOver);

        // アイコンの描画サイズと位置を決定
        // let x = (WIN_SIZE - ICON_DRAW_SIZE) / 2; // オーバーレイウィンドウの中央に描画
        // let y = (WIN_SIZE - ICON_DRAW_SIZE) / 2;
        let x = 0; // オーバーレイウィンドウの左上に描画
        let y = 0;

        // 描画するアイコンのIDを決定
        // GpBitmapをGpGraphicsに描画 (GDI+関数)
        if app_state.capture_overlay_is_processing {
            GdipDrawImageRectI(graphics, overlay.processing_bitmap as *mut _, x, y, ICON_DRAW_SIZE, ICON_DRAW_SIZE);
        } else {
            GdipDrawImageRectI(graphics, overlay.wait_bitmap as *mut _, x, y, ICON_DRAW_SIZE, ICON_DRAW_SIZE);
        };
        
        // アイコンの下にツールチップ風の矩形とテキストを描画
        if app_state.auto_clicker.is_running() {
            draw_auto_click_processing_label(graphics);
        }

    }
}

/// 連続クリック処理中ラベル描画ヘルパー関数
fn draw_auto_click_processing_label(graphics: *mut GpGraphics) {
    const LABEL_OFFSET_X: i32 = 20;

    let app_state = AppState::get_app_state_ref();
    let overlay = app_state.capturing_overlay.as_ref()
        .expect("キャプチャーオーバーレイが存在しません。");

    let text = format!("自動クリック中 ...({}/{})", 
        app_state.auto_clicker.get_progress_count(),
        app_state.auto_clicker.get_max_count(),
    );
    let text_rect_y = ICON_DRAW_SIZE+1;                 // アイコンの下に配置
    let text_rect_height = WIN_SIZE.1 - text_rect_y;    // 残りの領域を使用
    unsafe {
        // 4-1. オレンジ色の不透明な背景を描画
        // CompositingModeSourceCopyでアルファを無視して完全に上書き
        GdipSetCompositingMode(graphics, CompositingModeSourceCopy);
        GdipFillRectangleI(graphics, overlay.back_orange_brush as *mut _, LABEL_OFFSET_X, text_rect_y, WIN_SIZE.0 - LABEL_OFFSET_X, text_rect_height);
        GdipSetCompositingMode(graphics, CompositingModeSourceOver); // モードを元に戻す

        // 4-2. 黒色のテキストを描画
        // テキストを中央揃えに設定
        GdipSetStringFormatAlign(overlay.string_format, StringAlignmentCenter);
        GdipSetStringFormatLineAlign(overlay.string_format, StringAlignmentCenter);

        let text_utf16: Vec<u16> = text.encode_utf16().collect();
        let layout_rect = RectF {
            X: LABEL_OFFSET_X as f32,
            Y: text_rect_y as f32,
            Width: (WIN_SIZE.0 - LABEL_OFFSET_X) as f32,
            Height: text_rect_height as f32,
        };

        GdipDrawString(
            graphics,
            PCWSTR(text_utf16.as_ptr()),
            text_utf16.len() as i32,
            overlay.font,
            &layout_rect,
            overlay.string_format,
            overlay.back_ground_brush as *mut _,
        );

    }
}
