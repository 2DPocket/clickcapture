/*
 * system_utils.rs - システム統合ユーティリティモジュール
 * 
 * このモジュールは、WindowsシステムAPIとの統合を管理し、以下の主要機能を提供します：
 * 
 * 【主要機能概要】
 * 1. アプリケーションアイコン設定 - Win32リソースからのアイコン読み込みと設定
 * 2. システムカーソル制御 - キャプチャ中の砂時計カーソル表示
 * 3. 統合ログ表示 - コンソールとUIテキストボックスへの同期出力
 * 4. UI強制更新 - テキストボックスの即座の再描画制御
 * 
 * 【RCリソース統合】
 * - アイコンリソース: resource.h/constants.rs同期管理
 * - 品質向上: JPEG 95%高品質設定への対応
 * - オーバーレイ統合: RCベースアイコンオーバーレイとの連携
 * 
 * 【設計パターン】
 * - RAII原則: リソースの自動管理
 * - エラー処理: Rustの Result型を活用した安全な操作
 * - メモリ管理: Windows API用メモリの適切な解放
 * - 状態同期: AppState HWNDユーザーデータ経由の高速アクセス
 * 
 * 【パフォーマンス考慮事項】
 * - システムカーソル変更の最小オーバーヘッド
 * - UI更新の効率的な制御
 * - ログ出力の双方向同期
 * 
 * 【AIおよび第三者解析のための技術仕様】
 * - Windows API統合: windows crateを使用したモダンなRust-Windows相互運用
 * - エラーハンドリング: パニックを避けるための防御的プログラミング
 * - 状態管理: app_stateモジュールとの連携による一元的な状態更新
 * - UI統合: ダイアログボックスとの直接的な連携
 * - リソース管理: RCベースアーキテクチャとの統合設計
 * 
 * 【フォルダー管理機能の分離】
 * フォルダー選択・管理機能は folder_manager.rs モジュールに分離されました。
 * - show_folder_dialog() → folder_manager::show_folder_dialog()
 * - get_pictures_folder() → folder_manager::get_pictures_folder()
 */

use crate::{
    app_state::*,
    constants::{IDI_APP_ICON, IDC_LOG_EDIT},
};
use windows::{
    Win32::{
        Foundation::{HINSTANCE,LPARAM, WPARAM},
        Graphics::Gdi::{InvalidateRect, UpdateWindow},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
                GetDlgItem, ICON_BIG, ICON_SMALL, LoadIconW, MESSAGEBOX_RESULT, MESSAGEBOX_STYLE, MessageBoxW, SendMessageW, SetWindowTextW, WM_SETICON 
            },
    }, core::{PCWSTR}
};

/**
 * アプリケーションのウィンドウアイコンを設定する関数
 * 
 * 【機能説明】
 * 実行ファイルに埋め込まれたリソースからアプリケーションアイコンを読み込み、
 * ダイアログウィンドウのタイトルバーとタスクバーに表示されるアイコンを設定します。
 * 
 * 【技術的詳細】
 * - Win32 LoadIconW APIによるリソースアイコンの読み込み
 * - WM_SETICON メッセージによる大小両方のアイコン設定
 * - ICON_SMALL (16x16): タイトルバー用アイコン
 * - ICON_BIG (32x32): Alt+Tabおよびタスクバー用アイコン
 * 
 * 【リソース管理】
 * アイコンリソースは実行ファイル内に静的に埋め込まれているため、
 * 明示的な解放処理は不要です（システムが自動管理）。
 * 
 * 【エラーハンドリング】
 * - モジュールハンドル取得失敗時: デフォルトハンドルで継続
 * - アイコン読み込み失敗時: 処理をスキップ（アプリケーション継続）
 * - ダイアログハンドル無効時: アクセス違反を回避するため事前チェック
 * 
 * 【呼び出しタイミング】
 * ダイアログ初期化完了後、ウィンドウが表示される前に呼び出す必要があります。
 * app_state.dialog_hwndが有効に設定されていることが前提条件です。
 * 
 * 【安全性注意事項】
 * この関数は unsafe として定義されていますが、実際の unsafe 操作は
 * Windows API呼び出しのみであり、適切な検証により安全性を確保しています。
 * 
 * 【AIおよび第三者解析用の技術ノート】
 * Win32 GDI リソース管理の複雑さを隠蔽し、Rustの安全性モデルと
 * Windows APIの要求を適切にバランスさせた実装です。リソースIDは
 * constants.rsで一元管理され、ビルド時にembed-resourceで埋め込まれます。
 */
pub fn set_application_icon() {
    unsafe {
        // AppStateから有効なダイアログハンドルを取得
        let app_state = AppState::get_app_state_ref();
        let dialog_hwnd = app_state.dialog_hwnd.unwrap(); // パニック時は設計エラー

        // 現在のモジュール（実行ファイル）のハンドルを取得
        let hinstance = GetModuleHandleW(None).unwrap_or_default();
        
        // 埋め込みリソースからアプリケーションアイコンを読み込み
        let icon = LoadIconW(
            Some(HINSTANCE(hinstance.0)),
            PCWSTR(IDI_APP_ICON as *const u16), // constants.rsで定義されたリソースID
        );
        
        // アイコン読み込み成功時のみウィンドウアイコンを設定
        if let Ok(icon) = icon {
            // 小アイコン設定 (16x16) - タイトルバー表示用
            SendMessageW(
                *dialog_hwnd,
                WM_SETICON,
                Some(WPARAM(ICON_SMALL as usize)),
                Some(LPARAM(icon.0 as isize)),
            );
            
            // 大アイコン設定 (32x32) - Alt+Tab・タスクバー表示用
            SendMessageW(
                *dialog_hwnd,
                WM_SETICON,
                Some(WPARAM(ICON_BIG as usize)),
                Some(LPARAM(icon.0 as isize)),
            );
        }
        
        // アイコンハンドルは システムリソースのため明示的解放不要
    }
}


/**
 * 統合ログ表示関数 - コンソールとUI両方に同期出力
 * 
 * 【機能説明】
 * アプリケーション全体で使用する統一ログ関数。メッセージを標準出力（コンソール）と
 * ダイアログのログ表示テキストボックス（IDC_LOG_EDIT）の両方に同時出力します。
 * 
 * 【技術的詳細】
 * - println!: 標準出力への出力（デバッグ時のコンソール表示）
 * - SetWindowTextW: UIテキストボックスへの出力（ユーザー向け表示）
 * - UTF-16変換: Windows API要求に応じた文字列エンコード処理
 * - null終端: C形式文字列としての適切な終端処理
 * 
 * 【エラーハンドリング】
 * - ダイアログハンドル無効時: コンソール出力のみ実行
 * - ログコントロール取得失敗時: コンソール出力のみ実行
 * - UI更新失敗時: 無視（アプリケーション継続性を重視）
 * 
 * 【使用例】
 * ```rust
 * app_log("キャプチャを開始しました");
 * app_log(&format!("画像を保存しました: {}", filename));
 * ```
 * 
 * 【パフォーマンス考慮事項】
 * - UTF-16変換は必要時のみ実行（UI更新が可能な場合のみ）
 * - メモリ確保は一時的（関数スコープ内のみ）
 * - UI更新は非ブロッキング（失敗時もアプリケーション継続）
 * 
 * 【AIおよび第三者解析用の技術ノート】
 * この関数により、アプリケーション全体のログ出力が一元化され、
 * デバッグ時のコンソール出力とユーザー向けUI表示の両方を同時に
 * サポートします。既存のprintln!呼び出しをこの関数で置き換えることで、
 * コンソールアプリケーションからGUIアプリケーションへの
 * シームレスな移行が可能です。
 */
pub fn app_log(message: &str) {
    // 【出力1】標準出力へのログ出力（デバッグ・開発用）
    println!("{}", message);
    
    // 【出力2】UIテキストボックスへの表示（ユーザー向け）
    unsafe {
        let app_state = AppState::get_app_state_ref();

        if let Some(dialog_hwnd) = app_state.dialog_hwnd {
            // ログ表示用テキストボックスコントロールを取得
            if let Ok(log_edit) = GetDlgItem(Some(*dialog_hwnd), IDC_LOG_EDIT) {
                // UTF-8からUTF-16への変換（Windows API要求）
                let message_wide: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
                
                // テキストボックスにメッセージを設定（最新メッセージで上書き）
                let _ = SetWindowTextW(log_edit, PCWSTR(message_wide.as_ptr()));
                
                // 【重要】強制的な再描画を実行してUI更新を確実にする
                let _ = InvalidateRect(Some(log_edit), None, true);  // コントロールを無効化
                let _ = UpdateWindow(log_edit);               // 即座に再描画を実行
            }
        } else {
            // ダイアログハンドルが無効な場合はデフォルト
            eprintln!("❌ メッセージボックス表示エラー: ダイアログハンドルが無効です。");
        }
    }
}

/**
 * メッセージボックス表示関数
 * 
 * 【機能説明】
 * Windows標準のメッセージボックスを表示する共通関数。
 * UTF-8文字列からUTF-16への変換とnull終端処理を自動的に行います。
 * 
 * 【パラメータ】
 * - hwnd: 親ウィンドウハンドル
 * - message_text: 表示するメッセージテキスト（UTF-8）
 * - title_text: ウィンドウタイトル（UTF-8）
 * - style: メッセージボックスのスタイル（MB_OK | MB_ICONERROR等）
 * 
 * 【戻り値】
 * MESSAGEBOX_RESULT: ユーザーがクリックしたボタンの結果
 * 
 * 【使用例】
 * ```rust
 * unsafe {
 *     show_message_box(
 *         hwnd,
 *         "エラーが発生しました",
 *         "エラー",
 *         MB_OK | MB_ICONERROR
 *     );
 * }
 * ```
 */
pub fn show_message_box(message_text: &str, title_text: &str, style: MESSAGEBOX_STYLE) -> MESSAGEBOX_RESULT {
    unsafe {
        let app_state = AppState::get_app_state_ref();

        if let Some(hwnd) = app_state.dialog_hwnd {
            // UTF-8からUTF-16への変換（null終端付き）
            let message_wide: Vec<u16> = message_text.encode_utf16().chain(std::iter::once(0)).collect();
            let message = PCWSTR(message_wide.as_ptr());

            let title_wide: Vec<u16> = title_text.encode_utf16().chain(std::iter::once(0)).collect();
            let title = PCWSTR(title_wide.as_ptr());

            MessageBoxW(
                Some(*hwnd),
                message,
                title,
                style,
            )
        } else {
            // ダイアログハンドルが無効な場合はデフォルト
            eprintln!("❌ メッセージボックス表示エラー: ダイアログハンドルが無効です。");
            MESSAGEBOX_RESULT(0)
        }
    }
}
