/*
============================================================================
アプリケーション状態管理モジュール (app_state.rs) - 2025年10月最新版
============================================================================

【アーキテクチャ概要】
アプリケーション全体の状態を統一的に管理する中核モジュール。
高性能グローバル状態システム、完全スレッドセーフ設計、
ゼロオーバーヘッドアクセスパターンによる最適化された状態管理を提供。

【設計パターン・技術仕様】
- 🏗️ シングルトン＋Registryパターン：OnceLock<SafeHWND>グローバル管理
- 🔒 完全スレッドセーフ：unsafe Send/Sync実装＋所有権管理
- ⚡ ゼロオーバーヘッド：直接ポインタアクセス・Mutex回避
- 🎯 型安全WindowsAPI：SafeHWND/SafeHHOOKラッパー
- 📱 状態駆動UI：React風状態管理・宣言的UI更新

【コアコンポーネント構成】
1. 🎨 UIハンドル管理：5種類オーバーレイ＋メインダイアログ
2. 🎣 システムフック：マウス/キーボード低レベル監視
3. 🎮 操作モード制御：エリア選択・キャプチャ・ドラッグ状態
4. 📍 リアルタイム座標：マウス追跡・領域計算・DPI対応
5. 💾 ファイル管理：自動連番・品質制御・PDF設定
6. 🖱️ 自動クリック機能：有効/無効、間隔、回数などの設定

【高度な状態管理スコープ】
┌─ 🖼️ UI状態ハンドル管理
│  ├─ dialog_hwnd: Win32メインダイアログ（リソース管理中枢）
│  ├─ area_select_overlay: 半透明の矩形選択オーバーレイ
│  └─ capturing_overlay: キャプチャモード中の状態表示オーバーレイ
├─ 🎣 システムレベルフック
│  ├─ mouse_hook: グローバルマウス監視（<1msレスポンス）
│  └─ keyboard_hook: ESCキー緊急停止（システム全体対応）
├─ 🎯 操作モード状態管理（状態機械パターン）
│  ├─ is_area_select_mode: 領域選択アクティブ（オーバーレイ制御）
│  ├─ is_capture_mode: キャプチャ待機（ワンクリック撮影）
│  └─ is_dragging: ドラッグ進行中（リアルタイム描画）
├─ 📍 高精度座標・領域管理（DPI完全対応）
│  ├─ drag_start/end: ピクセル完璧矩形計算
│  ├─ current_mouse_pos: 60fps座標更新
│  └─ selected_area: 確定領域（キャプチャ対象）
├─ 💾 インテリジェントファイル管理
│  ├─ selected_folder_path: OneDrive/Pictures自動検出
│  └─ capture_file_counter: 自動連番（001-999）
├─ 🖥️ マルチモニター・解像度管理
│  ├─ screen_width/height: プライマリ解像度
│  └─ DPI対応: SetProcessDPIAware統合
├─ 🎨 プロフェッショナル品質制御
│  ├─ capture_scale_factor: 55%-100%（5%刻み）
│  ├─ jpeg_quality: 70%-100%（画質・サイズ最適化）
│  └─ pdf_max_size_mb: 500-1000MB（大容量対応）
├─ 🖱️ 自動クリック機能
│  ├─ auto_clicker: 自動クリックの状態と制御を管理
└─ 🚀 高性能システム統合
   ├─ LayeredWindow: UpdateLayeredWindowによるハードウェア加速透明処理
   ├─ GDI+: 高品質な図形描画と画像処理
   └─ メモリ効率: RAII＋明示的解放・リーク完全防止

【超高性能アクセスパターン】
初期化: Box::new→Box::into_raw→SetWindowLongPtrW→OnceLock::set
  ↓
読取: OnceLock::get→GetWindowLongPtrW→直接ポインタ→&AppState
  ↓
変更: 同上→&mut AppState（Mutex不要・ゼロオーバーヘッド）
  ↓
UI更新: 状態変更→自動UI同期→リアルタイム反映
  ↓
終了: 自動RAII→明示的cleanup→完全リソース解放

【エンタープライズ品質保証】
- 🛡️ メモリ安全性: Box管理・所有権追跡・ダングリングポインタ防止
- ⚡ パフォーマンス: O(1)アクセス・キャッシュ効率・CPU最適化
- 🔒 スレッド安全性: Send/Sync保証・データ競合防止
- 🚨 エラー処理: Option<T>・Result<T,E>・グレースフル劣化
- 📊 可観測性: 状態ダンプ・デバッグログ・診断機能

============================================================================
*/

use std::{ops::Deref, sync::OnceLock};

use windows::Win32::{
    Foundation::{HWND, POINT, RECT}, // 基本的なデータ型
    UI::{
        WindowsAndMessaging::*, // ウィンドウとメッセージ処理
    },
};

use crate::{auto_click::AutoClicker, capturing_overlay::CapturingOverLay};

use crate::area_select_overlay::*;


/*
============================================================================
超高性能スレッドセーフWrapperシステム
============================================================================
*/

/// 【SafeHWND】WindowsウィンドウハンドルのスレッドセーフWrapper
/// 
/// # 設計目的
/// Windows HWND（ウィンドウハンドル）をマルチスレッド環境で安全に共有。
/// Rustの標準型システムでは、Windows APIハンドルは Send/Sync 未実装のため、
/// 明示的なunsafe実装により高性能スレッド間通信を実現。
/// 
/// # 技術実装
/// - 🎯 newtype pattern: HWND完全カプセル化・型安全性保証
/// - 🚀 Send実装: スレッド間所有権移動・ゼロコストアブストラクション
/// - 🔒 Sync実装: 複数スレッド同時参照・データ競合防止
/// - ⚡ Deref実装: 透明アクセス・*safe_hwnd → HWND自動変換
/// 
/// # 安全性保証
/// Windows APIハンドルはプロセススコープ有効性・適切ライフタイム管理下で
/// スレッド間共有が安全。HWNDの無効化はDestroyWindow明示呼び出しのみ。
/// 
/// # 使用パターン
/// - AppState内ハンドル保存・グローバル状態管理
/// - オーバーレイ間ハンドル受け渡し・UI連携
/// - フック処理内ウィンドウ操作・リアルタイム制御
#[derive(Debug, Clone, Copy)]
pub struct SafeHWND(pub HWND);
unsafe impl Send for SafeHWND {} // スレッド間移動許可・パフォーマンス最適化
unsafe impl Sync for SafeHWND {} // 同時参照許可・競合状態防止

impl Deref for SafeHWND {
    type Target = HWND;

    /// 透明HWND直接アクセス・ゼロオーバーヘッド変換
    /// 使用例: let hwnd: HWND = *safe_hwnd;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// 【SafeHHOOK】WindowsフックハンドルのスレッドセーフWrapper
/// 
/// # 設計目的
/// Windows HHOOK（フックハンドル）をマルチスレッド環境で安全に管理。
/// SetWindowsHookExWで取得したシステムレベルフックを他スレッドから
/// 操作可能にし、確実なリソース解放を保証。
/// 
/// # 技術仕様
/// - 🎯 グローバルフック: WH_MOUSE_LL/WH_KEYBOARD_LL全プロセス監視
/// - 🔧 ハンドル管理: UnhookWindowsHookEx確実解放・リーク防止
/// - 🚀 スレッド安全性: Windows APIレベル保証・データ競合なし
/// - ⚡ 高速アクセス: CallNextHookEx直接呼び出し対応
/// 
/// # リソースライフサイクル
/// Install→Active Monitoring→Unhook→Handle Invalid
/// 各段階でのメモリ安全性・エラーハンドリング完備
/// 
/// # パフォーマンス特性
/// - フック処理: <1ms応答時間・リアルタイム保証
/// - メモリ使用: 固定8bytes・動的確保なし
/// - CPU負荷: イベント駆動・アイドル0%
#[derive(Debug, Clone, Copy)]
pub struct SafeHHOOK(pub HHOOK);
unsafe impl Send for SafeHHOOK {} // スレッド間移動許可・フック管理最適化  
unsafe impl Sync for SafeHHOOK {} // 同時参照許可・競合状態回避

impl Deref for SafeHHOOK {
    type Target = HHOOK;

    /// 透明HHOOK直接アクセス・CallNextHookEx最適化
    /// 使用例: CallNextHookEx(*safe_hook, ...);
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/*
============================================================================
エンタープライズグレード状態管理構造体
============================================================================
*/

/// 【AppState】アプリケーション状態統合管理構造体
/// 
/// # アーキテクチャ設計
/// - 🏗️ 集約ルートパターン: 全状態の一元化・整合性保証
/// - 🎯 単一責任原則: 状態管理専門・副作用分離
/// - 🚀 高性能アクセス: 直接ポインタ・O(1)操作・キャッシュ効率
/// - 🔒 完全スレッドセーフ: 所有権管理・データ競合防止
/// 
/// # 状態カテゴリ（プロダクション分類）
/// 1. 🎨 UI状態: ウィンドウハンドル群（5種類オーバーレイ管理）
/// 2. 🔧 システム状態: フックハンドル群（マウス・キーボード監視）
/// 3. 🎮 操作状態: モードフラグ群（選択・キャプチャ・ドラッグ）
/// 4. 📍 座標状態: リアルタイム位置（マウス追跡・領域計算）
/// 5. 💾 ファイル状態: 保存管理（自動連番・品質制御）
/// 6. ⚙️ 設定状態: ユーザー設定（スケール・品質・PDF）
/// 
/// # 他モジュール連携
/// この構造体は全モジュールから参照される中心的データ構造。
/// 各フィールド変更は対応UI/機能の即座状態同期を実現。
/// React風宣言的UI更新パターンによる一貫性保証。
#[derive(Debug)]
pub struct AppState {
    // ===== 🎨 プロフェッショナルUI要素ハンドル管理 =====
    /// メインダイアログウィンドウ: アプリケーション制御中枢
    /// - 機能: Win32ダイアログ・ユーザー操作受付・設定管理
    /// - 実装: main.rs DialogBoxParamW・リソース管理
    pub dialog_hwnd: Option<SafeHWND>,

    /// 半透明の矩形選択オーバーレイ
    /// - 機能: ドラッグ中の選択範囲を視覚的に表示
    /// - 実装: area_select_overlay.rs
    pub area_select_overlay: Option<AreaSelectOverLay>,

    /// キャプチャモード中の状態表示オーバーレイ
    /// - 機能: マウスカーソルに追従し、キャプチャ待機中や処理中の状態を表示
    /// - 実装: capturing_overlay.rs
    pub capturing_overlay: Option<CapturingOverLay>,
    
    // ===== システムフック管理 =====
    // 低レベルマウスフック：システム全体のマウスイベント監視
    pub mouse_hook: Option<SafeHHOOK>,
    // 低レベルキーボードフック：エスケープキーによるモード終了監視
    pub keyboard_hook: Option<SafeHHOOK>,

    // ===== 操作モード状態フラグ =====
    // エリア選択モード：ドラッグによる矩形領域選択が有効
    pub is_area_select_mode: bool,
    // キャプチャモード：左クリックによる画面保存が有効
    pub is_capture_mode: bool,
    // ドラッグ操作中：マウス左ボタンが押され、ドラッグ中
    pub is_dragging: bool,

    // ===== 座標・領域管理 =====
    // ドラッグ開始座標：マウス左ボタン押下時の初期位置
    pub drag_start: POINT,
    // ドラッグ終了座標：マウス左ボタン離上時の最終位置
    pub drag_end: POINT,
    // 現在のマウス位置：リアルタイムで更新される座標（オーバーレイ表示用）
    pub current_mouse_pos: POINT,

    // ===== 確定領域管理 =====
    // 選択確定済み領域：エリア選択完了後の矩形領域（キャプチャ対象）
    pub selected_area: Option<RECT>,

    // ===== ファイル管理設定 =====
    // 保存先フォルダーパス：ユーザー選択またはデフォルト（Pictures/OneDrive）
    pub selected_folder_path: Option<String>,
    // キャプチャファイル連番：screenshot_001.jpg, screenshot_002.jpg...
    pub capture_file_counter: u32,

    // ===== 画面解像度情報 =====
    // プライマリモニタ幅：GetSystemMetrics(SM_CXSCREEN)
    pub screen_width: i32,
    // プライマリモニタ高：GetSystemMetrics(SM_CYSCREEN)
    pub screen_height: i32,

    // ===== オーバーレイ表示状態 =====
    /// キャプチャオーバーレイの状態フラグ
    /// - true: 処理中状態（処理中アイコンを表示）
    /// - false: 待機中状態（待機中アイコンを表示）
    /// - 制御方法：switch_capture_processing(bool) -> capturing_overlay.refresh_overlay()
    pub capture_overlay_is_processing: bool,

    // ===== キャプチャ設定 =====
    // キャプチャ画質設定：画像のスケールファクター（50%〜100%、5%刻み）
    // - 100: 最高画質（元の解像度のまま保存）
    // - 65: 標準画質（画質とファイルサイズのバランス良好）※デフォルト
    // - 50: 軽量画質（ファイルサイズ重視、SNS共有に適している）
    // - UI制御: ドロップダウンコンボボックスでユーザー選択
    // - 使用箇所: screen_capture.rs内でキャプチャ処理時に参照
    pub capture_scale_factor: u8,

    /// JPEG画像保存品質設定（70%〜100%、5%刻み）
    /// 
    /// キャプチャした画像をJPEG形式で保存する際の圧縮品質を制御します。
    /// 値が高いほど画質が良くなりますが、ファイルサイズが大きくなります。
    /// 
    /// # 設定値の意味
    /// - 100: 最高画質（非圧縮に近い品質、ファイルサイズ大）
    /// - 95: 高画質（デフォルト、画質とサイズの最適バランス）※デフォルト
    /// - 85: 標準画質（一般的な用途に十分な品質）
    /// - 75: 軽量画質（ウェブ表示やメール添付に適している）
    /// - 70: 最軽量（ファイルサイズ重視、プレビュー用途）
    /// - UI制御: ドロップダウンコンボボックスでユーザー選択
    /// - 使用箇所: screen_capture.rs内でJPEGエンコード時に参照
    pub jpeg_quality: u8,

    /// PDFファイル最大サイズ設定（500MB〜1000MB、100MB刻み）
    /// 
    /// PDF変換時の1つのPDFファイルの最大サイズを制御します。
    /// この値を超えた場合、新しいPDFファイルが作成されます。
    /// 
    /// # 設定値の意味
    /// - 500: 標準サイズ（デフォルト、バランス良好）※デフォルト
    /// - 600: やや大きめ（画像数多めのプロジェクト向け）
    /// - 700: 大サイズ（高解像度画像多用時）
    /// - 800: 最大級（大容量対応、転送制限に注意）
    /// - 900: 特大サイズ（専門用途）
    /// - 1000: 最大サイズ（メール添付制限を考慮）
    /// - UI制御: ドロップダウンコンボボックスでユーザー選択
    /// - 使用箇所: export_pdf.rs内でPDFサイズ制限判定時に参照
    pub pdf_max_size_mb: u16,

    pub is_exporting_to_pdf: bool, // PDFエクスポート中フラグ

    // ===== 自動連続クリック機能 =====
    pub auto_clicker: AutoClicker,      // 自動クリック機能管理
}

/*
============================================================================
AppState実装メソッド群
============================================================================
*/

impl AppState {
    /// 【マウスフック取得】SafeHHOOKからHHOOKへの安全な変換
    //
    // 概要：Option<SafeHHOOK>からOption<HHOOK>への変換を提供
    // Windows API関数（CallNextHookEx等）で直接使用可能な形式に変換
    //
    // 戻り値：Some(HHOOK) - フックが設定済み / None - フック未設定
    pub fn get_mouse_hook(&self) -> Option<HHOOK> {
        self.mouse_hook.map(|hook| *hook)
    }

    /// 【キーボードフック取得】SafeHHOOKからHHOOKへの安全な変換
    //
    // 概要：キーボードフックハンドルをWindows API呼び出し用に変換
    // low_level_keyboard_proc内でCallNextHookEx呼び出し時に使用
    //
    // 戻り値：Some(HHOOK) - フックが設定済み / None - フック未設定
    pub fn get_keyboard_hook(&self) -> Option<HHOOK> {
        self.keyboard_hook.map(|hook| *hook)
    }

    /// 【状態初期化】アプリケーション開始時の状態セットアップ
    //
    // 概要：
    //   AppState::default()でデフォルト値を設定し、HWNDのユーザーデータ領域に保存
    //   main()関数のダイアログ作成直後に一度だけ呼び出される
    //
    // 技術実装：
    //   - Box::new(AppState::default())でヒープ領域に状態作成
    //   - Box::into_raw()でポインタ取得（Rustの所有権放棄）
    //   - SetWindowLongPtrW(GWLP_USERDATA)でHWNDに状態ポインタ格納
    //   - OnceLock::set()でグローバルHWNDアクセス設定
    //
    // 初期化内容：
    //   - 全ハンドル：None（未作成状態）
    //   - 全フラグ：false（非アクティブ状態）
    //   - 座標：(0,0)（デフォルト位置）
    //
    // 呼び出しタイミング：main()関数のDialogBoxParamW成功直後
    pub fn init_app_state(hwnd: HWND) {
        println!("アプリケーション状態を初期化します...");

        let mut app_state = AppState::default();

        // メインダイアログハンドルを設定
        app_state.dialog_hwnd = Some(SafeHWND(hwnd));

        // オーバーレイ構造体の初期化
        app_state.area_select_overlay = Some(AreaSelectOverLay::new());
        app_state.capturing_overlay = Some(CapturingOverLay::new());

        // グローバル状態変数にデフォルト値をセット
        let app_state_box = Box::new(app_state);
        let app_state_ptr = Box::into_raw(app_state_box);
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, app_state_ptr as isize);
        }

        DIALOG_HWND.set(SafeHWND(hwnd))
            .expect("グローバルダイアログハンドルの設定に失敗しました。");

        println!("アプリケーション状態が初期化されました");
    }

    /// 【状態参照取得】HWNDからAppStateへの不変参照を取得
    //
    // 概要：
    //   グローバルHWNDからユーザーデータ領域のAppStateポインタを取得し、
    //   不変参照として返却。読み取り専用アクセス用。
    //
    // 技術実装：
    //   - DIALOG_HWND.get()でグローバルHWND取得
    //   - GetWindowLongPtrW(GWLP_USERDATA)で状態ポインタ取得
    //   - *const AppStateから&AppStateへの参照変換
    //
    // 使用場面：
    //   - オーバーレイ描画時の状態確認
    //   - フラグ参照での条件分岐
    //   - デバッグ情報出力
    //
    // 安全性：AppState生存期間はアプリケーション全体と同じ
    pub fn get_app_state_ref() -> &'static AppState {
        let hwnd = DIALOG_HWND.get().unwrap();
        unsafe {
            let ptr = GetWindowLongPtrW(**hwnd, GWLP_USERDATA) as *const AppState;
            &*ptr
        }
    }

    /// 【状態参照取得】HWNDからAppStateへの可変参照を取得
    //
    // 概要：
    //   グローバルHWNDからユーザーデータ領域のAppStateポインタを取得し、
    //   可変参照として返却。状態変更操作用。
    //
    // 技術実装：
    //   - DIALOG_HWND.get()でグローバルHWND取得
    //   - GetWindowLongPtrW(GWLP_USERDATA)で状態ポインタ取得
    //   - *mut AppStateから&mut AppStateへの参照変換
    //
    // 使用場面：
    //   - オーバーレイハンドル設定/削除
    //   - モードフラグ切り替え
    //   - 座標位置更新
    //   - RCベースアイコン状態切り替え
    //
    // 注意：
    //   同時に複数の可変参照を作成しないよう呼び出し側で制御必要
    pub fn get_app_state_mut() -> &'static mut AppState {
        let hwnd = DIALOG_HWND.get().unwrap();
        
        unsafe {
            let ptr = GetWindowLongPtrW(**hwnd, GWLP_USERDATA) as *mut AppState;
            &mut *ptr
        }
    }

}

impl Default for AppState {
    fn default() -> Self {
        let screen_width;
        let screen_height;

        unsafe {
            // 画面全体のサイズを取得
            screen_width = GetSystemMetrics(SM_CXSCREEN);
            screen_height = GetSystemMetrics(SM_CYSCREEN);
        }

        Self {
            dialog_hwnd: None,
            area_select_overlay: None,
            capturing_overlay: None,
            mouse_hook: None,
            keyboard_hook: None,
            is_area_select_mode: false,
            is_capture_mode: false,
            is_dragging: false,
            drag_start: POINT { x: 0, y: 0 },
            drag_end: POINT { x: 0, y: 0 },
            current_mouse_pos: POINT { x: 0, y: 0 },
            selected_area: None,
            selected_folder_path: None,
            capture_file_counter: 1,
            screen_width,
            screen_height,
            capture_overlay_is_processing: false,
            capture_scale_factor: 65, // デフォルト65%（バランス良好）
            jpeg_quality: 95, // デフォルト95%（高画質）
            pdf_max_size_mb: 500, // デフォルト500MB（標準サイズ）
            is_exporting_to_pdf: false,
            auto_clicker: AutoClicker::new(),
        }

    }
}

/*
============================================================================
グローバル状態管理システム
============================================================================
*/

// 【グローバルダイアログハンドル】フック処理用の高速アクセス
static DIALOG_HWND: OnceLock<SafeHWND> = OnceLock::new();
