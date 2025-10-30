/*
============================================================================
キャプチャモードオーバーレイモジュール (capturing_overlay.rs) - RCベース実装
============================================================================

*/

// GDI+関連のライブラリ（外部機能）をインポート
use windows::Win32::Graphics::GdiPlus::{
    Color, CompositingModeSourceCopy, CompositingModeSourceOver,
    GdipCreateBitmapFromStream, GdipCreateFont, GdipCreateFontFamilyFromName, GdipCreateSolidFill, GdipCreateStringFormat, 
    GdipDeleteBrush, GdipDeleteFont, GdipDeleteFontFamily, GdipDeleteStringFormat, GdipDisposeImage, GdipDrawImageRectI, 
    GdipDrawString, GdipFillRectangleI, GdipSetCompositingMode, GdipSetStringFormatAlign, GdipSetStringFormatLineAlign, 
    GpBitmap, GpFont, GpGraphics, GpSolidFill, GpStringFormat, RectF, Status, StringAlignmentCenter
};
// 必要なライブラリをインポート
use windows::{
    Win32::{
        Foundation::{HGLOBAL, HWND}, // 基本的なデータ型
        System::{
            Com::{IStream},
            LibraryLoader::{FindResourceW, GetModuleHandleW, LoadResource, LockResource, SizeofResource},
            Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE},
        },
        Media::KernelStreaming::RT_RCDATA, // リソースタイプ定義
        UI::WindowsAndMessaging::*,
    },
    core::{PCWSTR}, // Windows API用の文字列操作
};

// アプリケーション状態管理構造体
use crate::{app_state::*, overlay};

// リソースID定数をインポート  
use crate::constants::*;

// オーバーレイ共通機能モジュール
use crate::overlay::*;


// オーバーレイウィンドウのサイズ
const WIN_SIZE: (i32, i32) = (200, 200); // 100x200ピクセル

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
                println!("❌ GdipCreateSolidFill for transparent_brush failed with status {:?}", status);
            }
        }

        // フォント作成
        let font_family_name: Vec<u16> = "MS UI Gothic".encode_utf16().chain(std::iter::once(0)).collect();

        unsafe {
            // フォント作成
            let mut font_family: *mut _ = std::ptr::null_mut();
            let status =  GdipCreateFontFamilyFromName(PCWSTR(font_family_name.as_ptr()), 
                std::ptr::null_mut(),
                &mut font_family);

            if status != Status(0) {
                println!("❌ GdipCreateFontFamilyFromName failed in CapturingOverLay::new() with status: {:?}", status);
            }

            // フォントサイズ10でフォント作成
            let status =GdipCreateFont( font_family, 10.0, Default::default(), Default::default(),  &mut overlay.font);
            if status != Status(0) {
                println!("❌ GdipCreateFont failed in CapturingOverLay::new() with status: {:?}", status);
            }
            // フォントファミリーの削除
            GdipDeleteFontFamily(font_family);
        }

        unsafe {
            let orange_color = Color { Argb: 0xFFFF8C00 }; // 不透明なオレンジ (DarkOrange)            
            let status = GdipCreateSolidFill(orange_color.Argb, &mut overlay.back_orange_brush);
            if status != Status(0) {
                println!("❌ GdipCreateSolidFill for orange background failed in CapturingOverLay::new() with status: {:?}", status);
            }

            let black_color = Color { Argb: 0xFF000000 }; // 不透明な黒
            let status = GdipCreateSolidFill(black_color.Argb, &mut overlay.back_ground_brush);
            if status != Status(0) {
                println!("❌ GdipCreateSolidFill for black background failed in CapturingOverLay::new() with status: {:?}", status);
            }

            let status = GdipCreateStringFormat(0, 0, &mut overlay.string_format);
            if status != Status(0) {
                println!("❌ GdipCreateStringFormat failed in CapturingOverLay::new() with status: {:?}", status);
            }
        }

        // ビットマップの読み込み
        unsafe {
            if let Ok(bitmap) = load_png_from_resource(PCWSTR(IDP_CAPTURE_WAITING as usize as *const u16)) {
                overlay.wait_bitmap = bitmap;
            } else {
                println!("❌ Failed to load PNG resource: IDP_CAPTURE_WAITING");
            }

            if let Ok(bitmap) = load_png_from_resource(PCWSTR(IDP_CAPTURE_PROCESSING as usize as *const u16)) {
                overlay.processing_bitmap = bitmap;
            } else {
                println!("❌ Failed to load PNG resource: IDP_CAPTURE_PROCESSING");
            }
        }

        overlay


    }
}

/// リソースからPNGを読み込み、GDI+ビットマップを作成するヘルパー関数
fn load_png_from_resource(resource_id: PCWSTR) -> Result<*mut GpBitmap, String> {
    unsafe {


        let hinstance = GetModuleHandleW(None).map_err(|e| e.to_string())?;

        // 1. リソースを検索
        let resource_handle = FindResourceW(Some(hinstance.into()), resource_id, RT_RCDATA);
        if resource_handle.0 == std::ptr::null_mut() {
            return Err("FindResourceW failed".to_string());
        }

        // 2. リソースをメモリにロード
        let loaded_resource = LoadResource(Some(hinstance.into()), resource_handle)
            .map_err(|e| format!("LoadResource failed: {}", e))?;

        // 3. リソースへのポインタを取得
        let resource_ptr = LockResource(loaded_resource);
        if resource_ptr.is_null() {
            return Err("LockResource failed".to_string());
        }

        // 4. リソースのサイズを取得
        let resource_size = SizeofResource(Some(hinstance.into()), resource_handle);
        if resource_size == 0 {
            return Err("SizeofResource returned 0".to_string());
        }

        // 5. リソースデータをグローバルメモリにコピー
        let hglobal: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, resource_size as usize)
            .map_err(|e| format!("GlobalAlloc failed: {}", e))?;
        let global_ptr = GlobalLock(hglobal);
        if global_ptr.is_null() {
            return Err("GlobalLock failed".to_string());
        }

        std::ptr::copy_nonoverlapping(resource_ptr, global_ptr, resource_size as usize);
        let _ = GlobalUnlock(hglobal);

        // 6. メモリからIStreamを作成
        let mut stream: Option<IStream> = None;
        CreateStreamOnHGlobal(hglobal, true, &mut stream)
            .map_err(|e| format!("CreateStreamOnHGlobal failed: {}", e))?;

        // 7. IStreamからGDI+ビットマップを作成
        let mut bitmap: *mut GpBitmap = std::ptr::null_mut();
        let status = GdipCreateBitmapFromStream(stream.as_ref().unwrap(), &mut bitmap);

        if status != Status(0) {
            // hglobalはIStreamに所有権が移っているので解放不要
            return Err(format!("GdipCreateBitmapFromStream failed with status {:?}", status));
        }
        Ok(bitmap)
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

/// OverLayトレイト実装
impl OverLay for CapturingOverLay {

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
    fn get_window_proc(&self) -> OverLayWindowProc {
        OverLayWindowProc {
            create: None,
            paint: Some(overlay_window_paint),
            destroy: None,
        }
    } 

    fn get_class_params(&self) -> OverLayWindowClassParams {
        OverLayWindowClassParams::default()
    }

    fn get_window_params(&self) -> OverLayWindowParams {
        // オーバーレイウィンドウを作成（WS_EX_TRANSPARENTを削除、マウスイベントを背後に通さないため）
        let mut params = OverLayWindowParams::default();
        params = OverLayWindowParams 
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
    let overlay = app_state.capturing_overlay.as_ref().unwrap();

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
        if app_state.capture_overlay_is_processing {
            let text = "キャプチャ中。。。";
            let text_rect_y = ICON_DRAW_SIZE;
            let text_rect_height = WIN_SIZE.1 - ICON_DRAW_SIZE;

            // 4-1. オレンジ色の不透明な背景を描画
            // CompositingModeSourceCopyでアルファを無視して完全に上書き
            GdipSetCompositingMode(graphics, CompositingModeSourceCopy);
            GdipFillRectangleI(graphics, overlay.back_orange_brush as *mut _, 0, text_rect_y, WIN_SIZE.0, text_rect_height);
            GdipSetCompositingMode(graphics, CompositingModeSourceOver); // モードを元に戻す

            // 4-2. 黒色のテキストを描画
            // テキストを中央揃えに設定
            GdipSetStringFormatAlign(overlay.string_format, StringAlignmentCenter);
            GdipSetStringFormatLineAlign(overlay.string_format, StringAlignmentCenter);

            let text_utf16: Vec<u16> = text.encode_utf16().collect();
            let layout_rect = RectF {
                X: 0.0,
                Y: text_rect_y as f32,
                Width: WIN_SIZE.0 as f32,
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
}
