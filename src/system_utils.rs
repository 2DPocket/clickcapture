/*
============================================================================
システム統合ユーティリティモジュール (system_utils.rs)
============================================================================

【ファイル概要】
WindowsシステムAPIとの連携を担う、アプリケーション全体で共通のヘルパー関数を
提供するモジュールです。

【主要機能】
1.  **アプリケーションアイコン設定 (`set_application_icon`)**:
    -   実行ファイルに埋め込まれたアイコンリソースを読み込み、メインダイアログのタイトルバーとタスクバーに設定します。
2.  **統合ログ表示 (`app_log`)**:
    -   メッセージをコンソール（デバッグ用）とUI上のログ表示ボックスの両方に同期して出力します。
3.  **メッセージボックス表示 (`show_message_box`)**:
    -   Windows標準のメッセージボックスを簡単に表示するためのラッパー関数。UTF-8からUTF-16への文字列変換を内部で処理します。

【技術仕様】
-   **API連携**: `LoadIconW`, `SendMessageW`, `MessageBoxW` などの基本的なWin32 APIを使用。
-   **状態アクセス**: `AppState` からダイアログハンドル (`dialog_hwnd`) を取得してUIを操作。
-   **文字列処理**: `encode_utf16` を使用して、Rustの `&str` をWindows APIが要求するUTF-16形式のワイド文字列に変換。

【AI解析用：依存関係】
- `app_state.rs`: ダイアログハンドルを取得するために使用。
- `constants.rs`: `IDI_APP_ICON` などのリソースID定義。
- `main.rs`: `WM_INITDIALOG` 内で `set_application_icon` を呼び出す。
- プロジェクト内のほぼ全てのモジュール: ログ出力のために `app_log` を、ユーザーへの通知のために `show_message_box` を呼び出す。
 */

use crate::{
    app_state::*,
    constants::{IDC_LOG_EDIT, IDI_APP_ICON},
};
use windows::{
    Win32::{
        Foundation::{HINSTANCE, LPARAM, WPARAM},
        Graphics::Gdi::{InvalidateRect, UpdateWindow},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            GetDlgItem, ICON_BIG, ICON_SMALL, LoadIconW, MESSAGEBOX_RESULT, MESSAGEBOX_STYLE,
            MessageBoxW, SendMessageW, SetWindowTextW, WM_SETICON,
        },
    },
    core::PCWSTR,
};

/**
 * アプリケーションのウィンドウアイコンを設定する
 *
 * 実行ファイルに埋め込まれたリソースからアプリケーションアイコンを読み込み、
 * ダイアログウィンドウのタイトルバーとタスクバーに表示されるアイコンを設定します。
 *
 * # 処理内容
 * 1. `LoadIconW` APIを使用して、リソースID (`IDI_APP_ICON`) に基づいてアイコンを読み込みます。
 * 2. `SendMessageW` APIと `WM_SETICON` メッセージを使用して、ウィンドウにアイコンを設定します。
 *    - `ICON_BIG` (32x32): タスクバーやAlt+Tab切り替え画面で使用されます。
 *    - `ICON_SMALL` (16x16): ウィンドウのタイトルバーで使用されます。
 *
 * # リソース管理
 * `LoadIconW` で読み込んだ標準アイコンリソースはシステムによって管理されるため、
 * `DestroyIcon` などで明示的に解放する必要はありません。
 */
pub fn set_application_icon() {
    unsafe {
        // AppStateからダイアログハンドルを取得
        let app_state = AppState::get_app_state_ref();
        let dialog_hwnd = app_state
            .dialog_hwnd
            .expect("ダイアログハンドルが無効です。");

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
 * 統合ログ表示を行う
 *
 * メッセージを標準出力（コンソール）と
 * ダイアログのログ表示テキストボックス（IDC_LOG_EDIT）の両方に同時出力します。
 *
 * # 使用例
 * ```rust
 * app_log("キャプチャを開始しました");
 * app_log(&format!("画像を保存しました: {}", filename));
 * ```
 */
pub fn app_log(message: &str) {
    // 出力1: 標準出力へのログ出力（デバッグ・開発用）
    println!("{}", message);

    // 出力2: UIテキストボックスへの表示（ユーザー向け）
    unsafe {
        let app_state = AppState::get_app_state_ref();

        if let Some(dialog_hwnd) = app_state.dialog_hwnd {
            // ログ表示用テキストボックスコントロールを取得
            if let Ok(log_edit) = GetDlgItem(Some(*dialog_hwnd), IDC_LOG_EDIT) {
                // UTF-8からUTF-16へ変換し、null終端を追加
                let message_wide: Vec<u16> =
                    message.encode_utf16().chain(std::iter::once(0)).collect();

                // テキストボックスにメッセージを設定（最新メッセージで上書き）
                let _ = SetWindowTextW(log_edit, PCWSTR(message_wide.as_ptr()));

                // 強制的な再描画を実行してUI更新を確実にする
                let _ = InvalidateRect(Some(log_edit), None, true); // コントロールを無効化
                let _ = UpdateWindow(log_edit); // 即座に再描画を実行
            }
        } else {
            // ダイアログハンドルが無効な場合はデフォルト
            eprintln!("❌ メッセージボックス表示エラー: ダイアログハンドルが無効です。");
        }
    }
}

/**
 * Windows標準のメッセージボックスを表示する
 *
 * この関数は、`MessageBoxW` APIのラッパーとして機能し、
 * Rustの `&str` をAPIが要求するUTF-16形式に自動的に変換します。
 *
 * # 引数
 * * `message_text` - メッセージボックスに表示する本文。
 * * `title_text` - メッセージボックスのタイトル。
 * * `style` - メッセージボックスのスタイル（ボタンの種類やアイコンなど）。
 *
 * # 戻り値
 * * `MESSAGEBOX_RESULT` - ユーザーがクリックしたボタンを示す値。
 */
pub fn show_message_box(
    message_text: &str,
    title_text: &str,
    style: MESSAGEBOX_STYLE,
) -> MESSAGEBOX_RESULT {
    unsafe {
        let app_state = AppState::get_app_state_ref();

        if let Some(hwnd) = app_state.dialog_hwnd {
            // UTF-8からUTF-16へ変換し、null終端を追加
            let message_wide: Vec<u16> = message_text
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let message = PCWSTR(message_wide.as_ptr());

            let title_wide: Vec<u16> = title_text
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let title = PCWSTR(title_wide.as_ptr());

            MessageBoxW(Some(*hwnd), message, title, style)
        } else {
            // ダイアログハンドルが無効な場合はデフォルト
            eprintln!("❌ メッセージボックス表示エラー: ダイアログハンドルが無効です。");
            MESSAGEBOX_RESULT(0)
        }
    }
}
