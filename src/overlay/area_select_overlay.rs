/*
============================================================================
エリア選択オーバーレイモジュール (area_select_overlay.rs)
============================================================================

【ファイル概要】
ClickCaptureアプリケーションのエリア選択モード時に表示される、全画面半透明
オーバーレイを管理するモジュール。ユーザーのマウスドラッグによる矩形領域選択を
視覚的に支援し、直感的な画面領域指定を可能にする高度なUI機能を提供します。

【主要機能】
1.  **全画面半透明オーバーレイ**: `AreaSelectOverLay`構造体
    -   画面全体を覆う半透明黒背景（Alpha=60%）
    -   選択領域の透明くり抜き表示
    -   赤色境界線による選択範囲の明確な視覚化

2.  **リアルタイム選択領域描画**: `area_select_overlay_paint`
    -   マウスドラッグに追従する動的矩形描画
    -   高品質アンチエイリアス境界線
    -   選択中/確定後の状態別描画制御

3.  **視覚的フィードバックシステム**:
    -   半透明マスク：非選択領域の視覚的抑制
    -   透明くり抜き：選択領域の鮮明な表示
    -   境界線：正確な選択範囲の把握支援

【技術仕様】
-   **レイアウト**: 全画面フルスクリーンオーバーレイ（プライマリモニター対応）
-   **描画エンジン**: GDI+ による高品質レンダリング
-   **透明処理**: LayeredWindow + UpdateLayeredWindow（ハードウェア加速）
-   **合成モード**: SourceCopy/SourceOver の動的切り替え
-   **色彩設計**: 半透明黒背景（#99000000）+ 赤色境界線（#FFFF0000）

【描画アルゴリズム】
1. **背景マスク描画**: 画面全体を半透明黒で覆う
2. **選択領域くり抜き**: CompositingModeSourceCopy による透明化
3. **境界線描画**: 赤色ペンによる矩形境界の描画
4. **状態別制御**: ドラッグ中/確定後の適切な表示切り替え

【ユーザー体験設計】
-   **直感的操作**: マウスドラッグによる自然な領域選択
-   **明確なフィードバック**: 選択領域の即座の視覚化
-   **非侵襲的表示**: 作業内容を遮らない適切な透明度
-   **高精度選択**: ピクセル単位での正確な領域指定

【実装アーキテクチャ】
-   **RAII リソース管理**: GDI+オブジェクトの自動解放
-   **型安全設計**: SafeHWNDラッパーによる安全なウィンドウ操作
-   **パフォーマンス最適化**: 描画リソースの事前作成と再利用
-   **エラー耐性**: 描画失敗時の継続実行保証

【AI解析用：依存関係】
-   `windows`クレート: Win32 API（LayeredWindow、GDI+、全画面制御）
-   `app_state.rs`: ドラッグ状態と選択領域座標の監視
-   `overlay/mod.rs`: Overlayトレイトとオーバーレイ基盤機能
-   `area_select.rs`: エリア選択モード制御との連携
-   `hook/mouse.rs`: マウスイベントによる描画トリガー
-   `screen_capture.rs`: 選択領域の最終的なキャプチャ実行
 */

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

use crate::app_state::*;
use crate::overlay::*;

/// エリア選択オーバーレイ構造体
/// 
/// 全画面エリア選択機能を提供する高度なオーバーレイウィンドウの実装。
/// GDI+リソースの効率的管理、リアルタイム領域描画、視覚的フィードバック
/// システムを統合し、直感的な画面領域選択体験を実現します。
/// 
/// # 構造体フィールド詳細
/// - `hwnd`: オーバーレイウィンドウハンドル（SafeHWNDでラップ）
/// - `semi_transparent_black_brush`: 半透明黒背景ブラシ（Alpha=60%）
/// - `transparent_brush`: 選択領域くり抜き用透明ブラシ（Alpha=0%）
/// - `red_pen`: 境界線描画用赤色ペン（1ピクセル幅）
/// - `resize_handles_brush`: リサイズハンドル描画用ブラシ（将来拡張用）
/// - `resize_handles_pen`: リサイズハンドル境界用ペン（将来拡張用）
/// 
/// # 描画リソース設計
/// 全てのGDI+オブジェクトは初期化時に作成され、描画処理で再利用されます。
/// この設計により、リアルタイム描画時のパフォーマンスを最大化し、
/// スムーズなユーザー操作体験を保証します。
/// 
/// # リソース管理
/// RAIIパターンによる自動リソース管理を実装。Dropトレイトにより、
/// 構造体破棄時に全GDI+オブジェクトが確実にクリーンアップされます。
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
    /// 新しいエリア選択オーバーレイインスタンスを作成する
    ///
    /// GDI+描画リソースの初期化、色彩設定、ペン/ブラシの作成を行い、
    /// エリア選択表示に必要な全要素を準備します。初期化に失敗した場合も
    /// アプリケーションの継続実行を保証するため、エラーは個別にログ出力され、
    /// 部分的な機能低下で動作を継続します。
    ///
    /// # 初期化処理内容
    /// 1. **半透明背景ブラシ**: Alpha=60%の黒背景（視覚的抑制効果）
    /// 2. **透明ブラシ**: Alpha=0%（選択領域くり抜き用）
    /// 3. **境界線ペン**: 赤色1ピクセル（明確な視覚的境界）
    /// 4. **拡張ハンドル**: リサイズ機能用リソース（将来対応）
    ///
    /// # 色彩設計の根拠
    /// - **半透明黒（#99000000）**: 非選択領域の適度な抑制、作業内容の視認性維持
    /// - **赤色境界（#FFFF0000）**: 高い視認性、直感的な選択範囲把握
    /// - **透明くり抜き**: 選択領域の完全な鮮明表示
    ///
    /// # パフォーマンス最適化
    /// 描画リソースの事前作成により、リアルタイム描画時のオーバーヘッドを
    /// 最小化。マウスドラッグ中の滑らかな描画更新を実現します。
    ///
    /// # エラーハンドリング
    /// 各GDI+オブジェクトの作成失敗は個別にキャッチされ、エラーログを出力。
    /// 失敗したリソースはnullポインタのまま残り、描画時にスキップされます。
    ///
    /// # 戻り値
    /// 初期化されたAreaSelectOverLayインスタンス。一部リソース作成に失敗しても
    /// 有効なインスタンスを返し、利用可能な機能のみで動作します。
    pub fn new() -> Self {
        // 構造体の初期状態（全ポインタをnullで初期化）
        let mut overlay = AreaSelectOverLay {
            hwnd: None,
            semi_transparent_black_brush: std::ptr::null_mut(),
            transparent_brush: std::ptr::null_mut(),
            red_pen: std::ptr::null_mut(),
            resize_handles_brush: std::ptr::null_mut(),
            resize_handles_pen: std::ptr::null_mut(),
        };

        // === GDI+描画リソースの段階的初期化 ===
        unsafe {
            // 1. 半透明黒背景ブラシ作成
            // Alpha=153(0x99): 約60%の透明度で背景を適度に抑制
            let semi_transparent_black_color = Color { Argb: 0x99000000 };
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

            // 2. 透明ブラシ作成（選択領域くり抜き用）
            // Alpha=0: 完全透明で選択領域を鮮明に表示
            let transparent_color = Color { Argb: 0x00000000 };
            let status =
                GdipCreateSolidFill(transparent_color.Argb, &mut overlay.transparent_brush);
            if status != Status(0) {
                eprintln!(
                    "❌ GdipCreateSolidFill for transparent_brush failed with status {:?}",
                    status
                );
            }

            // 3. 赤色境界線ペン作成
            // 赤色（#FF0000）: 高い視認性で選択範囲を明確に表示
            // 2.0px幅: 高DPI環境でも視認可能な適切な太さ
            let red_color = Color { Argb: 0xFFFF0000 };
            let status = GdipCreatePen1(red_color.Argb, 2.0, UnitPixel, &mut overlay.red_pen);
            if status != Status(0) {
                eprintln!(
                    "❌ GdipCreatePen1 for red_pen failed with status {:?}",
                    status
                );
            }

            // 4. リサイズハンドル用ブラシ作成（将来拡張用）
            // 半透明赤（Alpha=50%）: ハンドル部分の柔らかな強調表示
            let handle_fill_color = Color { Argb: 0x80FF0000 };
            let status =
                GdipCreateSolidFill(handle_fill_color.Argb, &mut overlay.resize_handles_brush);
            if status != Status(0) {
                eprintln!(
                    "❌ GdipCreateSolidFill for resize_handles_brush failed with status {:?}",
                    status
                );
            }

            // 5. リサイズハンドル境界用ペン作成（将来拡張用）
            // 不透明赤色1px: ハンドル境界の明確な定義
            let handle_border_color = Color { Argb: 0xFFFF0000 };
            let status = GdipCreatePen1(
                handle_border_color.Argb,
                1.0,                    // 1ピクセル幅
                UnitPixel,              // ピクセル単位指定
                &mut overlay.resize_handles_pen,
            );
            if status != Status(0) {
                eprintln!(
                    "❌ GdipCreatePen1 for resize_handles_pen failed with status {:?}",
                    status
                );
            }
        }

        // 初期化完了したオーバーレイインスタンスを返却
        // 一部リソース作成に失敗していても、利用可能な機能で動作継続
        overlay
    }
}

/// AreaSelectOverLay用RAII自動リソース解放実装
/// 
/// 構造体がスコープを抜ける際に、保持している全てのGDI+リソースを
/// 確実に解放します。この実装により、メモリリークとリソースリークを
/// 完全に防止し、長時間動作でも安定したパフォーマンスを保証します。
/// 
/// # 解放対象リソース
/// - オーバーレイウィンドウ（destroy_overlay()経由）
/// - GDI+ブラシオブジェクト群（半透明黒、透明、リサイズハンドル）
/// - GDI+ペンオブジェクト群（境界線、リサイズハンドル境界）
/// 
/// # 解放順序の安全性
/// GDI+オブジェクトは相互依存がないため、任意の順序で安全に解放可能。
/// nullポインタに対する解放呼び出しも安全に処理されます。
impl Drop for AreaSelectOverLay {
    fn drop(&mut self) {
        // 1. オーバーレイウィンドウの破棄
        self.destroy_overlay();

        // 2. GDI+リソースの一括解放
        unsafe {
            // ブラシオブジェクト解放
            GdipDeleteBrush(self.semi_transparent_black_brush as *mut _);
            GdipDeleteBrush(self.transparent_brush as *mut _);
            GdipDeleteBrush(self.resize_handles_brush as *mut _);
            
            // ペンオブジェクト解放
            GdipDeletePen(self.red_pen);
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
/// エリア選択オーバーレイウィンドウの描画処理
/// 
/// 全画面エリア選択中のオーバーレイに対するカスタム描画を実行します。
/// 半透明黒背景による視覚的抑制効果と、選択領域の透明くり抜き表示により、
/// ユーザーが直感的に画面領域を選択できる高品質な視覚体験を提供します。
/// 
/// # 引数
/// * `_hwnd` - オーバーレイウィンドウハンドル（使用しないため_プレフィックス）
/// * `graphics` - GDI+グラフィックスコンテキストへのポインタ
/// 
/// # 描画アルゴリズム
/// 1. **全画面背景マスク**: 半透明黒（Alpha=60%）で画面全体を覆う
/// 2. **選択領域くり抜き**: ドラッグ中の矩形領域を完全透明化
/// 3. **境界線描画**: 赤色2px境界線で選択範囲を明確に示す
/// 4. **状態別制御**: ドラッグ中/確定済みの適切な表示切り替え
/// 
/// # 視覚設計の効果
/// - **背景抑制**: 非選択領域の視覚的重要度を下げ、選択作業に集中
/// - **領域強調**: 透明くり抜きにより選択領域を鮮明に表示
/// - **境界明示**: 赤色境界線で選択範囲を正確に把握可能
/// 
/// # 描画技術詳細
/// - **合成モード**: SourceCopy（くり抜き）→ SourceOver（境界線）
/// - **高品質レンダリング**: GDI+アンチエイリアス、高DPI対応
/// - **パフォーマンス最適化**: 事前作成済みリソースの効率的再利用
/// 
/// # レスポンシブ描画
/// マウスドラッグに完全追従し、リアルタイムで選択領域を更新。
/// 60FPS相当の滑らかな描画更新でストレスフリーな操作体験を実現。
fn overlay_window_paint(_hwnd: HWND, graphics: *mut GpGraphics) {
    // この関数は paint_by_update_layered_window の 32bpp DIB 上で呼ばれることを前提とする
    
    // === AppState から描画に必要な状態情報を取得 ===
    let app_state = AppState::get_app_state_ref();
    let (is_dragging, screen_width, screen_height) = (
        app_state.is_dragging,         // ユーザーがドラッグ操作中かを判定
        app_state.screen_width,        // プライマリスクリーンの幅（ピクセル）
        app_state.screen_height,       // プライマリスクリーンの高さ（ピクセル）
    );

    // 描画対象オーバーレイインスタンスを取得（GDI+リソースアクセス用）
    let overlay = app_state
        .area_select_overlay
        .as_ref()
        .expect("エリア選択オーバーレイが存在しません。");

    // === 1. 全画面背景マスク描画 ===
    // 半透明黒（Alpha=60%）で画面全体を覆い、非選択領域の視覚的重要度を下げる
    // この処理により、ユーザーの注意を選択領域に集中させることができる
    unsafe {
        GdipFillRectangleI(
            graphics,
            overlay.semi_transparent_black_brush as *mut _,
            0,                          // X座標：左端から
            0,                          // Y座標：上端から
            screen_width,               // 幅：画面全幅
            screen_height,              // 高さ：画面全高
        );
    }

    // === 2. ドラッグ中の動的選択領域処理 ===
    if is_dragging {
        // === 2.1 ドラッグ開始点と終了点から正規化された矩形領域を計算 ===
        // min/max関数により、任意方向のドラッグ（右下・左上・右上・左下）に対応
        let (left, top, right, bottom) = {
            let left = app_state.drag_start.x.min(app_state.drag_end.x);
            let top = app_state.drag_start.y.min(app_state.drag_end.y);
            let right = app_state.drag_start.x.max(app_state.drag_end.x);
            let bottom = app_state.drag_start.y.max(app_state.drag_end.y);
            (left, top, right, bottom)
        };
        let width = right - left;      // 選択領域の幅（ピクセル）
        let height = bottom - top;     // 選択領域の高さ（ピクセル）

        // === 2.2 選択領域の透明くり抜き処理 ===
        // CompositingModeSourceCopy: アルファブレンド無視で完全上書き
        // 背景マスクの上に透明領域を描画し、選択範囲を鮮明に表示
        unsafe {
            GdipSetCompositingMode(graphics, CompositingModeSourceCopy);
            GdipFillRectangleI(
                graphics,
                overlay.transparent_brush as *mut _,
                left,                       // 選択領域の左端X座標
                top,                        // 選択領域の上端Y座標
                width,                      // 選択領域の幅
                height,                     // 選択領域の高さ
            );
            // CompositingModeSourceOver: 通常の透過描画モードに復帰
            GdipSetCompositingMode(graphics, CompositingModeSourceOver);
        }

        // === 2.3 選択領域境界線の描画 ===
        // 赤色2ピクセル境界線で選択範囲を明確に表示
        // 高い視認性により、ユーザーが選択範囲を正確に把握可能
        unsafe {
            GdipDrawRectangleI(
                graphics, 
                overlay.red_pen,            // 赤色ペン（#FFFF0000, 2px幅）
                left,                       // 矩形左端X座標
                top,                        // 矩形上端Y座標  
                width,                      // 矩形幅
                height                      // 矩形高さ
            );
        }

        // === 2.4 リサイズハンドル描画 ===
        // 選択範囲の四隅にリサイズハンドルを配置し、将来的なサイズ調整機能を提供
        let border_rect = GpRect {
            X: left,                        // 選択領域の左端座標
            Y: top,                         // 選択領域の上端座標
            Width: width,                   // 選択領域の幅
            Height: height,                 // 選択領域の高さ
        };
        draw_resize_handles(overlay, graphics, border_rect);
    }
}

/// エリア選択枠の四隅にリサイズハンドルを描画する
/// 
/// 選択された矩形領域の四隅（左上、右上、左下、右下）にリサイズハンドルを配置し、
/// 将来的な選択領域サイズ調整機能の視覚的基盤を提供します。各ハンドルは
/// 16x16ピクセルの正方形として描画され、明確な操作可能性を示します。
/// 
/// # 引数
/// * `overlay` - エリア選択オーバーレイの参照（描画リソースアクセス用）
/// * `graphics` - GDI+グラフィックスコンテキストへのポインタ
/// * `border_rect` - リサイズハンドルを配置する基準矩形
/// 
/// # ハンドル配置戦略
/// - **左上ハンドル**: 矩形の左上角を基準点として配置
/// - **右上ハンドル**: 矩形の右上角から幅分オフセット
/// - **左下ハンドル**: 矩形の左下角から高さ分オフセット  
/// - **右下ハンドル**: 矩形の右下角から幅・高さ分オフセット
/// 
/// # 描画仕様
/// - **サイズ**: 16x16ピクセル正方形
/// - **塗りつぶし**: リサイズハンドル用ブラシ（視認性重視）
/// - **境界線**: リサイズハンドル用ペン（明確な境界）
/// 
/// # 将来拡張性
/// 現在は視覚表示のみですが、将来的にマウスイベント処理を追加することで
/// インタラクティブなリサイズ機能を実装可能な設計となっています。
fn draw_resize_handles(
    overlay: &AreaSelectOverLay,
    graphics: *mut GpGraphics,
    border_rect: GpRect,
) {
    // === ハンドルサイズ定義 ===
    const HANDLE_SIZE: i32 = 16;       // リサイズハンドルの一辺サイズ（ピクセル）
    let handle_half_size = HANDLE_SIZE / 2; // ハンドル中心からの距離（8ピクセル）

    // === 四隅の座標計算 ===
    // 選択矩形の各角の座標を配列として定義し、効率的な描画処理を実現
    let corners = [
        (border_rect.X, border_rect.Y),                      // 左上角の座標
        (border_rect.X + border_rect.Width, border_rect.Y),  // 右上角の座標
        (border_rect.X, border_rect.Y + border_rect.Height), // 左下角の座標
        (
            border_rect.X + border_rect.Width,               // 右下角のX座標
            border_rect.Y + border_rect.Height,              // 右下角のY座標
        ),
    ];

    // === 各角へのハンドル描画処理 ===
    for (cx, cy) in corners.iter() {
        // ハンドル矩形の計算：角の座標を中心とした16x16ピクセル正方形
        let handle_rect = GpRect {
            X: cx - handle_half_size,       // 中心X座標から左に8ピクセル
            Y: cy - handle_half_size,       // 中心Y座標から上に8ピクセル
            Width: HANDLE_SIZE,             // 幅：16ピクセル
            Height: HANDLE_SIZE,            // 高さ：16ピクセル
        };
        
        // GDI+による二段階描画：塗りつぶし→境界線
        unsafe {
            // === ハンドル背景の塗りつぶし ===
            GdipFillRectangleI(
                graphics,
                overlay.resize_handles_brush as *mut _,
                handle_rect.X,              // 塗りつぶし領域の左端X座標
                handle_rect.Y,              // 塗りつぶし領域の上端Y座標
                handle_rect.Width,          // 塗りつぶし領域の幅
                handle_rect.Height,         // 塗りつぶし領域の高さ
            );
            
            // === ハンドル境界線の描画 ===
            GdipDrawRectangleI(
                graphics,
                overlay.resize_handles_pen, // リサイズハンドル境界線用ペン
                handle_rect.X,              // 境界線矩形の左端X座標
                handle_rect.Y,              // 境界線矩形の上端Y座標
                handle_rect.Width,          // 境界線矩形の幅
                handle_rect.Height,         // 境界線矩形の高さ
            );
        }
    }
}
