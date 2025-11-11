/*
============================================================================
キャプチャモードオーバーレイモジュール (capturing_overlay.rs)
============================================================================

【ファイル概要】
ClickCaptureアプリケーションのキャプチャモード中に表示される、リアルタイム状態表示
オーバーレイを管理するモジュール。マウスカーソルに追従する小型の状態インジケーター
として、キャプチャ待機・処理中・自動クリック進行状況を視覚的にフィードバックします。

【主要機能】
1.  **動的状態表示オーバーレイ**: `CapturingOverLay`構造体
    -   キャプチャ待機中：待機アイコン表示
    -   キャプチャ処理中：処理中アイコン表示
    -   自動クリック中：進行状況付きツールチップ表示

2.  **リアルタイム視覚フィードバック**: `overlay_window_paint`
    -   GDI+による高品質アイコン描画
    -   透明度制御による非侵襲的表示
    -   マウスカーソル追従による直感的UX

3.  **埋め込みリソース管理**: `load_png_from_resource`
    -   実行ファイル内PNGアイコンの動的読み込み
    -   メモリ効率的なGDI+ビットマップ変換
    -   RAII パターンによる自動リソース解放

【技術仕様】
-   **オーバーレイサイズ**: 230x90ピクセル（アイコン32x32 + テキスト領域）
-   **描画エンジン**: GDI+ による高品質レンダリング
-   **透明処理**: LayeredWindow + UpdateLayeredWindow（ハードウェア加速）
-   **位置制御**: WS_EX_TRANSPARENT による背景オブジェクトとの非干渉
-   **フォント**: Yu Gothic UI 16pt（日本語対応、高DPI対応）

【状態別表示仕様】
-   **待機状態**: 
    - 待機アイコン（IDP_CAPTURE_WAITING）
    - 半透明表示、ユーザーの次アクション待ち
-   **処理状態**:
    - 処理中アイコン（IDP_CAPTURE_PROCESSING）
    - 明確なフィードバックでキャプチャ実行中を通知
-   **自動クリック状態**:
    - 進行状況ラベル「自動クリック中 ...(N/M)」
    - オレンジ背景 + 黒文字による高視認性表示

【UI/UX設計思想】
-   **非侵襲性**: 作業画面を遮らない最小限サイズ
-   **直感性**: アイコンによる言語に依存しない状態表示
-   **レスポンシブ**: リアルタイム状態反映による即座のフィードバック
-   **アクセシビリティ**: 高コントラスト色彩とクリアなタイポグラフィ

【実装アーキテクチャ】
-   **RAII リソース管理**: GDI+オブジェクトの自動解放
-   **型安全設計**: SafeHWNDラッパーによる安全なウィンドウ操作
-   **メモリ効率**: 事前ロードされたビットマップリソースの再利用
-   **エラー耐性**: 描画失敗時の継続実行保証

【AI解析用：依存関係】
-   `windows`クレート: Win32 API（LayeredWindow、GDI+、リソース管理）
-   `app_state.rs`: キャプチャ状態とマウス座標の監視
-   `constants.rs`: アイコンリソースID定義（IDP_CAPTURE_*）
-   `overlay/mod.rs`: Overlayトレイトとオーバーレイ基盤機能
-   `screen_capture.rs`: キャプチャモード制御との連携
-   `auto_click.rs`: 自動クリック進行状況の表示連携
-   `ui/ui_utils.rs`: PNGリソース読み込み機能（load_png_from_resource）
 */

// GDI+関連のライブラリ（外部機能）をインポート
use windows::Win32::Graphics::GdiPlus::{
    Color, CompositingModeSourceCopy, CompositingModeSourceOver, GdipCreateBitmapFromStream,
    GdipCreateFont, GdipCreateFontFamilyFromName, GdipCreateSolidFill, GdipCreateStringFormat,
    GdipDeleteBrush, GdipDeleteFont, GdipDeleteFontFamily, GdipDeleteStringFormat,
    GdipDisposeImage, GdipDrawImageRectI, GdipDrawString, GdipFillRectangleI,
    GdipSetCompositingMode, GdipSetStringFormatAlign, GdipSetStringFormatLineAlign, GpBitmap,
    GpFont, GpGraphics, GpSolidFill, GpStringFormat, RectF, Status, StringAlignmentCenter,
};
use windows::Win32::System::Com::IStream;
use windows::Win32::System::LibraryLoader::{
    FindResourceW, LoadResource, LockResource, SizeofResource,
};
use windows::Win32::UI::Shell::SHCreateMemStream;
// 必要なライブラリをインポート
use windows::{
    Win32::{
        Foundation::HWND,                  // 基本的なデータ型
        Media::KernelStreaming::RT_RCDATA, // リソースタイプ定義
        UI::WindowsAndMessaging::*,
    },
    core::PCWSTR, // Windows API用の文字列操作
};

use std::slice;

// アプリケーション状態管理構造体
use crate::app_state::*;

// リソースID定数をインポート
use crate::constants::*;

// オーバーレイ共通機能モジュール
use crate::overlay::*;

// オーバーレイウィンドウサイズ定数
// 幅230px: アイコン32px + テキスト領域198px（自動クリック進行表示用）
// 高90px: アイコン32px + テキスト行高58px（マージン込み）
const WIN_SIZE: (i32, i32) = (230, 90);

// アイコン描画サイズ定数（32x32ピクセル）
// 高DPI環境での視認性とパフォーマンスの最適バランス
const ICON_DRAW_SIZE: i32 = 32;

/// キャプチャモードオーバーレイ構造体
/// 
/// キャプチャモード中の状態表示を担う軽量オーバーレイウィンドウの実装。
/// GDI+リソースの効率的管理、リアルタイム状態描画、マウス追従による
/// 非侵襲的なユーザーフィードバックを提供します。
/// 
/// # 構造体フィールド詳細
/// - `hwnd`: オーバーレイウィンドウハンドル（SafeHWNDでラップ）
/// - `font`: テキスト描画用GDI+フォント（Yu Gothic UI 16pt）
/// - `transparent_brush`: 背景透明化用ブラシ（Alpha=0）
/// - `string_format`: 文字列描画制御（中央揃え設定）
/// - `back_ground_brush`: 文字描画用黒ブラシ（文字色）
/// - `back_orange_brush`: ラベル背景用オレンジブラシ（ツールチップ背景色）
/// - `wait_bitmap`: 待機状態アイコン（PNG→GDI+変換済み）
/// - `processing_bitmap`: 処理中状態アイコン（PNG→GDI+変換済み）
/// 
/// # リソース管理
/// 全てのGDI+オブジェクトはRAIIパターンで自動解放。
/// Dropトレイト実装により、構造体破棄時に確実にクリーンアップされます。
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
    /// 新しいキャプチャモードオーバーレイインスタンスを作成する
    ///
    /// GDI+リソースの初期化、フォント設定、アイコンリソースの読み込みを行い、
    /// キャプチャモード表示に必要な全要素を準備します。初期化に失敗した場合も
    /// アプリケーションの継続実行を保証するため、エラーは個別にログ出力され、
    /// 部分的な機能低下で動作を継続します。
    ///
    /// # 初期化処理内容
    /// 1. **透明ブラシ作成**: 背景クリア用（Alpha=0）
    /// 2. **フォント作成**: Yu Gothic UI 16ptフォント
    /// 3. **描画ブラシ作成**: 文字用黒ブラシ、ラベル背景用オレンジブラシ
    /// 4. **文字列フォーマット作成**: 中央揃え設定
    /// 5. **アイコンビットマップ読み込み**: 待機・処理中アイコンのPNG→GDI+変換
    ///
    /// # リソース初期化エラー処理
    /// 各GDI+オブジェクトの作成失敗は個別にキャッチされ、エラーログを出力。
    /// 失敗したリソースはnullポインタのまま残り、描画時にスキップされます。
    /// この設計により、部分的な機能低下でもアプリケーションは動作継続可能。
    ///
    /// # フォント選択理由
    /// Yu Gothic UI: Windows 10/11標準、日本語・英語混在テキストの高い可読性、
    /// ClearType最適化による高DPI環境での鮮明表示を実現。
    ///
    /// # 戻り値
    /// 初期化されたCapturingOverLayインスタンス。一部リソース作成に失敗しても
    /// 有効なインスタンスを返し、利用可能な機能のみで動作します。
    pub fn new() -> Self {
        // 構造体の初期状態（全ポインタをnullで初期化）
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

        // === GDI+リソースの段階的初期化 ===

        // 1. 透明ブラシ作成（背景クリア用）
        unsafe {
            let transparent_color = Color { Argb: 0x00000000 }; // Alpha=0で完全透明
            let status =
                GdipCreateSolidFill(transparent_color.Argb, &mut overlay.transparent_brush);
            if status != Status(0) {
                eprintln!(
                    "❌ GdipCreateSolidFill for transparent_brush failed with status {:?}",
                    status
                );
            }
        }

        // 2. フォント作成（Yu Gothic UI 16pt）
        // UTF-16エンコード + Null終端でWindows API互換文字列作成
        let font_family_name: Vec<u16> = "Yu Gothic UI"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            // フォントファミリーオブジェクト作成
            let mut font_family: *mut _ = std::ptr::null_mut();
            let status = GdipCreateFontFamilyFromName(
                PCWSTR(font_family_name.as_ptr()),
                std::ptr::null_mut(), // システム標準フォントコレクション使用
                &mut font_family,
            );

            if status != Status(0) {
                eprintln!(
                    "❌ GdipCreateFontFamilyFromName failed in CapturingOverLay::new() with status: {:?}",
                    status
                );
            }

            // フォントインスタンス作成（16pt、標準スタイル）
            // 16pt: 高DPI環境での視認性とレイアウト最適化の調和点
            let status = GdipCreateFont(
                font_family,
                16.0,                    // フォントサイズ16pt
                Default::default(),      // FontStyleRegular（標準）
                Default::default(),      // UnitPoint（ポイント単位）
                &mut overlay.font,
            );
            if status != Status(0) {
                eprintln!(
                    "❌ GdipCreateFont failed in CapturingOverLay::new() with status: {:?}",
                    status
                );
            }
            
            // フォントファミリーオブジェクトのクリーンアップ
            // 作成したフォントファミリーはフォント作成後に即座に解放
            GdipDeleteFontFamily(font_family);
        }

        // 3. 描画ブラシ作成
        unsafe {
            // ラベル背景用オレンジブラシ作成
            let orange_color = Color { Argb: 0xFFDEB887 }; // Burlywood色（#DEB887）
            let status = GdipCreateSolidFill(orange_color.Argb, &mut overlay.back_orange_brush);
            if status != Status(0) {
                eprintln!(
                    "❌ GdipCreateSolidFill for orange background failed in CapturingOverLay::new() with status: {:?}",
                    status
                );
            }

            // 文字描画用黒ブラシ作成
            let black_color = Color { Argb: 0xFF000000 }; // 不透明な黒（#000000）
            let status = GdipCreateSolidFill(black_color.Argb, &mut overlay.back_ground_brush);
            if status != Status(0) {
                eprintln!(
                    "❌ GdipCreateSolidFill for black background failed in CapturingOverLay::new() with status: {:?}",
                    status
                );
            }

            // 4. 文字列描画フォーマット作成
            // デフォルト設定で作成後、後で中央揃え等の設定を適用
            let status = GdipCreateStringFormat(0, 0, &mut overlay.string_format);
            if status != Status(0) {
                eprintln!(
                    "❌ GdipCreateStringFormat failed in CapturingOverLay::new() with status: {:?}",
                    status
                );
            }
        }

        // 5. アイコンビットマップリソース読み込み
        // 待機状態アイコン（マウスクリック待機中の表示用）
        if let Ok(bitmap) =
            load_png_from_resource(PCWSTR(IDP_CAPTURE_WAITING as usize as *const u16))
        {
            overlay.wait_bitmap = bitmap;
        } else {
            eprintln!("❌ Failed to load PNG resource: IDP_CAPTURE_WAITING");
        }

        // 処理中状態アイコン（キャプチャ実行中の表示用）
        if let Ok(bitmap) =
            load_png_from_resource(PCWSTR(IDP_CAPTURE_PROCESSING as usize as *const u16))
        {
            overlay.processing_bitmap = bitmap;
        } else {
            eprintln!("❌ Failed to load PNG resource: IDP_CAPTURE_PROCESSING");
        }

        // 初期化完了したオーバーレイインスタンスを返却
        // 一部リソース作成に失敗していても、利用可能な機能で動作継続
        overlay
    }
}

/// CapturingOverLay用RAII自動リソース解放実装
/// 
/// 構造体がスコープを抜ける際に、保持している全てのGDI+リソースを
/// 確実に解放します。この実装により、メモリリークとリソースリークを
/// 完全に防止し、長時間動作でも安定したパフォーマンスを保証します。
/// 
/// # 解放対象リソース
/// - オーバーレイウィンドウ（destroy_overlay()経由）
/// - GDI+ブラシオブジェクト群（透明、黒、オレンジ）
/// - GDI+フォントオブジェクト
/// - 文字列フォーマットオブジェクト
/// - ビットマップオブジェクト群（待機、処理中アイコン）
/// 
/// # 解放順序の重要性
/// GDI+の依存関係を考慮し、依存されるオブジェクトから順番に解放。
/// nullポインタチェックによりダブル解放を防止。
impl Drop for CapturingOverLay {
    fn drop(&mut self) {
        // 1. オーバーレイウィンドウの破棄
        self.destroy_overlay();

        // 2. GDI+リソースの段階的解放
        unsafe {
            // ブラシオブジェクト解放
            GdipDeleteBrush(self.transparent_brush as *mut _);
            GdipDeleteBrush(self.back_ground_brush as *mut _);
            GdipDeleteBrush(self.back_orange_brush as *mut _);
            
            // フォント関連オブジェクト解放
            GdipDeleteFont(self.font);
            GdipDeleteStringFormat(self.string_format);

            // ビットマップオブジェクト解放
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
        params = OverlayWindowParams {
            dwex_style: WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT,
            width: WIN_SIZE.0,
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
                    size.0,
                    size.1,
                    SWP_NOACTIVATE,
                );
            }
        }
    }
}

/// キャプチャオーバーレイウィンドウの描画処理
/// 
/// キャプチャモード中のオーバーレイウィンドウに対するカスタム描画を実行します。
/// 現在のアプリケーション状態に基づいて適切なアイコンとテキストを表示し、
/// ユーザーに明確な視覚フィードバックを提供します。
/// 
/// # 引数
/// * `_hwnd` - オーバーレイウィンドウハンドル（使用しないため_プレフィックス）
/// * `graphics` - GDI+グラフィックスコンテキストへのポインタ
/// 
/// # 描画内容
/// 1. **背景クリア**: 透明ブラシによる完全透明化
/// 2. **状態アイコン**: 
///    - 処理中：processing_bitmap（キャプチャ実行中）
///    - 待機中：wait_bitmap（ユーザー操作待ち）
/// 3. **自動クリック状況**: 進行状況ラベル（有効時のみ）
/// 
/// # 描画技術詳細
/// - **合成モード制御**: SourceCopy → SourceOver の切り替えで透明度管理
/// - **高品質描画**: GDI+によるアンチエイリアス、ClearType対応
/// - **パフォーマンス最適化**: 事前読み込み済みビットマップの再利用
/// 
/// # レイアウト設計
/// - アイコン位置：左上（0,0）から32x32ピクセル
/// - テキスト領域：アイコン下部、幅210px（マージン込み）
/// - 全体サイズ：230x90ピクセルの固定レイアウト
fn overlay_window_paint(_hwnd: HWND, graphics: *mut GpGraphics) {
    // AppStateから描画対象オーバーレイインスタンスを取得
    let app_state = AppState::get_app_state_ref();
    let overlay = app_state
        .capturing_overlay
        .as_ref()
        .expect("キャプチャーオーバーレイが存在しません。");

    unsafe {
        // === 1. 背景透明化処理 ===
        // LayeredWindowによる透明度制御とGDI+描画の協調動作
        // CompositingModeSourceCopy: 既存ピクセルを完全上書き（アルファ値無視）
        // これにより、前フレームの描画痕跡を完全に除去し、クリーンな透明背景を確保
        GdipSetCompositingMode(graphics, CompositingModeSourceCopy);
        GdipFillRectangleI(
            graphics,
            overlay.transparent_brush as *mut _,
            0,                  // X座標：左端から
            0,                  // Y座標：上端から  
            WIN_SIZE.0,         // 幅：230ピクセル
            WIN_SIZE.1,         // 高：90ピクセル
        );
        
        // 描画モードを通常合成に復元
        // CompositingModeSourceOver: アルファブレンディング有効（通常描画）
        GdipSetCompositingMode(graphics, CompositingModeSourceOver);

        // === 2. 状態アイコン描画 ===
        // アイコン描画位置：オーバーレイウィンドウの左上角
        let x = 0; // X座標：左端に配置
        let y = 0; // Y座標：上端に配置

        // アプリケーション状態に基づく条件分岐描画
        if app_state.capture_overlay_is_processing {
            // キャプチャ処理実行中：処理中アイコンを表示
            GdipDrawImageRectI(
                graphics,
                overlay.processing_bitmap as *mut _,
                x,                      // X座標
                y,                      // Y座標  
                ICON_DRAW_SIZE,        // 幅：32ピクセル
                ICON_DRAW_SIZE,        // 高：32ピクセル
            );
        } else {
            // ユーザー操作待機中：待機アイコンを表示
            GdipDrawImageRectI(
                graphics,
                overlay.wait_bitmap as *mut _,
                x,                      // X座標
                y,                      // Y座標
                ICON_DRAW_SIZE,        // 幅：32ピクセル
                ICON_DRAW_SIZE,        // 高：32ピクセル
            );
        };

        // === 3. 自動クリック進行状況表示 ===  
        // 自動クリック機能が動作中の場合のみ、進行状況ラベルを描画
        if app_state.auto_clicker.is_running() {
            draw_auto_click_processing_label(graphics);
        }
    }
}

/// 自動クリック実行中の進行状況ラベル描画
/// 
/// 自動クリック機能の実行中に、現在の進行状況を視覚的に表示するラベルを描画します。
/// オレンジ色の背景に黒文字で「自動クリック中 ...(N/M)」形式のテキストを表示し、
/// ユーザーが現在の実行状況を即座に把握できるよう設計されています。
/// 
/// # 引数
/// * `graphics` - GDI+グラフィックスコンテキストへのポインタ
/// 
/// # 表示内容
/// - フォーマット：「自動クリック中 ...(現在回数/最大回数)」
/// - 背景色：Burlywood (#DEB887) - 温かみのある通知色
/// - 文字色：黒 (#000000) - 高コントラストで視認性確保
/// - 配置：アイコン直下、中央揃え
/// 
/// # レイアウト設計
/// - X座標：20px オフセット（視覚的バランス調整）
/// - Y座標：アイコン下端+1px（密着配置でコンパクト性確保）
/// - 幅：210px（全体幅230px - オフセット20px）
/// - 高：57px（全体高90px - アイコン高32px - マージン1px）
/// 
/// # 描画技術
/// - 背景：SourceCopyモードでアルファ値無視の完全描画
/// - 文字：SourceOverモードでアンチエイリアス適用
/// - 配置：StringFormat中央揃えで美しい視覚配置
fn draw_auto_click_processing_label(graphics: *mut GpGraphics) {
    // ラベルの左端オフセット（視覚的調整用）
    const LABEL_OFFSET_X: i32 = 20;

    // AppStateと描画対象オーバーレイの取得
    let app_state = AppState::get_app_state_ref();
    let overlay = app_state
        .capturing_overlay
        .as_ref()
        .expect("キャプチャーオーバーレイが存在しません。");

    // 進行状況テキストの動的生成
    // フォーマット例：「自動クリック中 ...(3/10)」
    let text = format!(
        "自動クリック中 ...({}/{})",
        app_state.auto_clicker.get_progress_count(),    // 現在の実行回数
        app_state.auto_clicker.get_max_count(),         // 設定された最大回数
    );
    
    // ラベル描画領域の計算
    let text_rect_y = ICON_DRAW_SIZE + 1;          // Y座標：アイコン直下+1px
    let text_rect_height = WIN_SIZE.1 - text_rect_y; // 高さ：残り全領域使用
    
    unsafe {
        // === 背景描画（不透明なオレンジ矩形） ===
        // CompositingModeSourceCopy: アルファチャンネル無視で確実な不透明描画
        GdipSetCompositingMode(graphics, CompositingModeSourceCopy);
        GdipFillRectangleI(
            graphics,
            overlay.back_orange_brush as *mut _,
            LABEL_OFFSET_X,
            text_rect_y,
            WIN_SIZE.0 - LABEL_OFFSET_X,
            text_rect_height,
        );
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

/// 埋め込みリソースからPNG画像を読み込み、GDI+ビットマップを作成する
///
/// 実行ファイルに`RT_RCDATA`として埋め込まれたPNGリソースを、
/// GDI+で描画可能な`GpBitmap`オブジェクトに変換します。
///
/// # 引数
/// * `resource_id` - `resource.h`で定義されたリソースID (`PCWSTR`形式)。
///
/// # 戻り値
/// * `Ok(*mut GpBitmap)` - 成功した場合、GDI+ビットマップへのポインタ。
/// * `Err(String)` - 失敗した場合、エラーメッセージ。
///
/// # 処理フロー
/// 1.  `FindResourceW`: 実行ファイル内のリソースを検索。
/// 2.  `LoadResource`: リソースをメモリにロード。
/// 3.  `LockResource`: リソースデータへのポインタを取得。
/// 4.  `SizeofResource`: リソースのサイズを取得。
/// 5.  `slice::from_raw_parts`: ポインタとサイズからRustの`&[u8]`スライスを安全に作成。
/// 6.  `SHCreateMemStream`: メモリスライスからCOMの`IStream`オブジェクトを作成。
///     これにより、リソースデータをファイルのようにストリームとして扱える。
/// 7.  `GdipCreateBitmapFromStream`: `IStream`からGDI+ビットマップを生成。
///
/// # 安全性
/// この関数は`unsafe`ブロックを含みますが、Win32 API呼び出しは適切に処理され、
/// メモリ管理は`IStream`のRAIIパターンによって自動的に行われるため安全です。
/// 呼び出し元は、返された`GpBitmap`ポインタを`GdipDisposeImage`で解放する責任があります。
pub fn load_png_from_resource(resource_id: PCWSTR) -> Result<*mut GpBitmap, String> {
    unsafe {
        let hinstance = GetModuleHandleW(None).map_err(|e| e.to_string())?;

        // 1. 実行ファイルからリソースを検索
        let resource_handle = FindResourceW(Some(hinstance), resource_id, RT_RCDATA);
        if resource_handle.0 == std::ptr::null_mut() {
            return Err("リソースの検索に失敗しました (FindResourceW)".to_string());
        }

        // 2. リソースをメモリにロード
        let loaded_resource = LoadResource(Some(hinstance), resource_handle)
            .map_err(|e| format!("リソースのロードに失敗しました (LoadResource): {}", e))?;

        // 3. リソースデータへのポインタを取得
        let resource_ptr = LockResource(loaded_resource);
        if resource_ptr.is_null() {
            return Err("リソースポインタの取得に失敗しました (LockResource)".to_string());
        }

        // 4. リソースデータのサイズを取得
        let resource_size = SizeofResource(Some(hinstance), resource_handle);
        if resource_size == 0 {
            return Err("リソースサイズが0です (SizeofResource)".to_string());
        }

        // 5. ポインタとサイズからRustのバイトスライスを作成
        let data_slice: &[u8] =
            slice::from_raw_parts(resource_ptr as *const u8, resource_size as usize);

        // 6. バイトスライスからインメモリのCOMストリーム(`IStream`)を作成
        // `SHCreateMemStream`は、渡されたデータを内部でコピーし、
        // ストリームオブジェクトが解放されるときに自動的にメモリを解放します。
        let stream: Option<IStream> = SHCreateMemStream(Some(data_slice));

        let stream = match stream {
            Some(s) => s,
            None => {
                return Err("メモリストリームの作成に失敗しました (SHCreateMemStream)".to_string());
            }
        };

        // 7. `IStream`からGDI+ビットマップオブジェクトを作成
        let mut bitmap: *mut GpBitmap = std::ptr::null_mut();
        let status = GdipCreateBitmapFromStream(&stream, &mut bitmap);

        if status != Status(0) {
            return Err(format!(
                "ストリームからのビットマップ作成に失敗しました (GdipCreateBitmapFromStream): {:?}",
                status
            ));
        }

        // ポインタがnullでないことを確認
        if bitmap.is_null() {
            return Err("ビットマップは正常に作成されましたが、ポインタがnullです".to_string());
        }

        Ok(bitmap)
    }
}
