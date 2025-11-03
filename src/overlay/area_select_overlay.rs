// GDI+関連のライブラリ（外部機能）をインポート
use windows::Win32::Graphics::GdiPlus::{
    Color, CompositingModeSourceCopy, CompositingModeSourceOver, GdipCreatePen1,
    GdipCreateSolidFill, GdipDeleteBrush, GdipDeletePen, GdipDrawRectangleI, GdipFillRectangleI,
    GdipSetCompositingMode, GpGraphics, GpPen, GpSolidFill, Rect as GpRect, Status, UnitPixel,
};

// 必要なライブラリ（外部機能）をインポート
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::*, // グラフィック描画機能
};

// アプリケーション状態管理構造体
use crate::app_state::*;

// オーバーレイ管理関数
use crate::overlay::*;

/// エリア選択オーバーレイ構造体
#[derive(Debug)]
pub struct AreaSelectOverLay {
    hwnd: Option<SafeHWND>,
    semi_transparent_black_brush: *mut GpSolidFill, // 半透明黒背景ブラシ
    transparent_brush: *mut GpSolidFill,            // くり抜き用の透明ブラシ
    red_pen: *mut GpPen,                            // 赤色境界線ペン
    resize_handles_brush: *mut GpSolidFill,         // リサイズハンドル用のブラシ
    resize_handles_pen: *mut GpPen,                 // リサイズハンドル用ペン
}

/// エリア選択オーバーレイ構造体実装
impl AreaSelectOverLay {
    pub fn new() -> Self {
        let mut overlay = AreaSelectOverLay {
            hwnd: None,
            semi_transparent_black_brush: std::ptr::null_mut(),
            transparent_brush: std::ptr::null_mut(),
            red_pen: std::ptr::null_mut(),
            resize_handles_brush: std::ptr::null_mut(),
            resize_handles_pen: std::ptr::null_mut(),
        };

        // GDI+リソースの初期化
        unsafe {
            // 半透明黒背景ブラシ作成
            let semi_transparent_black_color = Color { Argb: 0x99000000 }; // Alpha=128
            let status = GdipCreateSolidFill(
                semi_transparent_black_color.Argb,
                &mut overlay.semi_transparent_black_brush,
            );
            if status != Status(0) {
                eprintln!(
                    "❌ GdipCreateSolidFill for semi_transparent_black_brush failed with status {:?}",
                    status
                );
            }

            // 透明ブラシ作成
            let transparent_color = Color { Argb: 0x00000000 }; // Alpha=0
            let status =
                GdipCreateSolidFill(transparent_color.Argb, &mut overlay.transparent_brush);
            if status != Status(0) {
                eprintln!(
                    "❌ GdipCreateSolidFill for transparent_brush failed with status {:?}",
                    status
                );
            }

            // 赤色境界線ペン作成
            let red_color = Color { Argb: 0xFFFF0000 }; // Alpha
            let status = GdipCreatePen1(red_color.Argb, 2.0, UnitPixel, &mut overlay.red_pen);
            if status != Status(0) {
                eprintln!(
                    "❌ GdipCreatePen1 for red_pen failed with status {:?}",
                    status
                );
            }

            // リサイズハンドル用ブラシ作成（薄い赤）
            let handle_fill_color = Color { Argb: 0x80FF0000 }; // Alpha=128
            let status =
                GdipCreateSolidFill(handle_fill_color.Argb, &mut overlay.resize_handles_brush);
            if status != Status(0) {
                eprintln!(
                    "❌ GdipCreateSolidFill for resize_handles_brush failed with status {:?}",
                    status
                );
            }

            // リサイズハンドル用ペン作成（不透明な赤）
            let handle_border_color = Color { Argb: 0xFFFF0000 }; // Alpha=255
            let status = GdipCreatePen1(
                handle_border_color.Argb,
                1.0,
                UnitPixel,
                &mut overlay.resize_handles_pen,
            );
            if status != Status(0) {
                eprintln!(
                    "❌ GdipCreatePen1 for resize_handles_pen failed with status {:?}",
                    status
                );
            }
        }

        overlay
    }
}

impl Drop for AreaSelectOverLay {
    fn drop(&mut self) {
        self.destroy_overlay();

        // GDI+リソースの解放
        unsafe {
            GdipDeleteBrush(self.semi_transparent_black_brush as *mut _);
            GdipDeleteBrush(self.transparent_brush as *mut _);
            GdipDeletePen(self.red_pen);
            GdipDeleteBrush(self.resize_handles_brush as *mut _);
            GdipDeletePen(self.resize_handles_pen);
        }
    }
}

/// Overlayトレイト実装
impl Overlay for AreaSelectOverLay {
    fn set_hwnd(&mut self, hwnd: Option<SafeHWND>) {
        self.hwnd = hwnd;
    }
    fn get_hwnd(&self) -> Option<SafeHWND> {
        self.hwnd.clone()
    }
    fn get_overlay_name(&self) -> &str {
        "AreaSelect"
    }
    fn get_description(&self) -> &str {
        "エリア選択オーバーレイ"
    }
    fn get_window_proc(&self) -> OverlayWindowProc {
        OverlayWindowProc {
            create: None,
            paint: Some(overlay_window_paint),
            destroy: None,
        }
    }

    fn get_class_params(&self) -> OverlayWindowClassParams {
        let mut params = OverlayWindowClassParams::default();
        unsafe {
            params = OverlayWindowClassParams {
                h_cursor: LoadCursorW(None, IDC_CROSS).unwrap_or_default(),
                ..params
            };
        }
        params
    }

    fn get_window_params(&self) -> OverlayWindowParams {
        let app_state = AppState::get_app_state_mut();

        // // オーバーレイウィンドウを作成（WS_EX_TRANSPARENTを削除、マウスイベントを背後に通さないため）
        let mut params = OverlayWindowParams::default();
        params = OverlayWindowParams {
            dwex_style: WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            width: app_state.screen_width,
            height: app_state.screen_height,
            ..params
        };
        params
    }
}

/// オーバーレイウィンドウの描画処理
fn overlay_window_paint(_hwnd: HWND, graphics: *mut GpGraphics) {
    // この関数は paint_by_update_layered_window の 32bpp DIB 上で呼ばれることを前提とする
    let app_state = AppState::get_app_state_ref();
    let (is_dragging, screen_width, screen_height) = (
        app_state.is_dragging,
        app_state.screen_width,
        app_state.screen_height,
    );

    let overlay = app_state
        .area_select_overlay
        .as_ref()
        .expect("エリア選択オーバーレイが存在しません。");

    // 背景を半透明の黒で塗りつぶす
    unsafe {
        GdipFillRectangleI(
            graphics,
            overlay.semi_transparent_black_brush as *mut _,
            0,
            0,
            screen_width,
            screen_height,
        );
    }

    if is_dragging {
        // ドラッグ中の選択範囲を取得
        let (left, top, right, bottom) = {
            let left = app_state.drag_start.x.min(app_state.drag_end.x);
            let top = app_state.drag_start.y.min(app_state.drag_end.y);
            let right = app_state.drag_start.x.max(app_state.drag_end.x);
            let bottom = app_state.drag_start.y.max(app_state.drag_end.y);
            (left, top, right, bottom)
        };
        let width = right - left;
        let height = bottom - top;

        // 選択範囲を完全に透明でクリア（くり抜き）
        unsafe {
            GdipSetCompositingMode(graphics, CompositingModeSourceCopy);
            GdipFillRectangleI(
                graphics,
                overlay.transparent_brush as *mut _,
                left,
                top,
                width,
                height,
            );
            GdipSetCompositingMode(graphics, CompositingModeSourceOver);
        }

        // 選択範囲の境界線を不透明の赤で描画
        unsafe {
            GdipDrawRectangleI(graphics, overlay.red_pen, left, top, width, height);
        }

        // 選択範囲の四隅にリサイズハンドルを描画
        let border_rect = GpRect {
            X: left,
            Y: top,
            Width: width,
            Height: height,
        };
        draw_resize_handles(overlay, graphics, border_rect);
    }
}

/// エリア選択枠の四隅にリサイズハンドルを描画する
fn draw_resize_handles(
    overlay: &AreaSelectOverLay,
    graphics: *mut GpGraphics,
    border_rect: GpRect,
) {
    const HANDLE_SIZE: i32 = 16;
    let handle_half_size = HANDLE_SIZE / 2;

    // 4つの角の座標を計算
    let corners = [
        (border_rect.X, border_rect.Y),                      // 左上
        (border_rect.X + border_rect.Width, border_rect.Y),  // 右上
        (border_rect.X, border_rect.Y + border_rect.Height), // 左下
        (
            border_rect.X + border_rect.Width,
            border_rect.Y + border_rect.Height,
        ), // 右下
    ];

    // 各角にハンドルを描画
    for (cx, cy) in corners.iter() {
        let handle_rect = GpRect {
            X: cx - handle_half_size,
            Y: cy - handle_half_size,
            Width: HANDLE_SIZE,
            Height: HANDLE_SIZE,
        };
        unsafe {
            GdipFillRectangleI(
                graphics,
                overlay.resize_handles_brush as *mut _,
                handle_rect.X,
                handle_rect.Y,
                handle_rect.Width,
                handle_rect.Height,
            );
            GdipDrawRectangleI(
                graphics,
                overlay.resize_handles_pen,
                handle_rect.X,
                handle_rect.Y,
                handle_rect.Width,
                handle_rect.Height,
            );
        }
    }
}
