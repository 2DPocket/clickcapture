/*
============================================================================
自動クリック間隔コンボボックスハンドラモジュール (auto_click_interval_combo_handler.rs)
============================================================================

【ファイル概要】
ClickCaptureアプリケーションの自動連続クリック機能において、クリック実行間隔を
設定するコンボボックスを管理するモジュール。ユーザーが1秒〜5秒の範囲で
自動キャプチャの実行間隔を直感的に選択できるUIを提供し、
高度な自動化ワークフローの精密な制御を可能にします。

【主要機能】
1.  **間隔コンボボックス初期化**: `initialize_auto_click_interval_combo`
    -   1秒〜5秒の間隔設定を1秒刻みで提供（1秒、2秒、3秒、4秒、5秒）
    -   デフォルト値として実用的な1秒間隔を設定
    -   Win32コンボボックスAPIによるネイティブUI制御

2.  **間隔変更イベント処理**: `handle_auto_click_interval_combo_change`
    -   ユーザーの間隔選択を即座にAutoClickerに反映
    -   リアルタイムでの設定更新によるシームレスな操作体験

【技術仕様】
-   **間隔範囲**: 1秒〜5秒（1秒刻み）
    - 1秒: 高速連続キャプチャ、動的コンテンツ監視に最適
    - 2秒: バランス重視、一般的なスクリーンキャプチャ作業
    - 3秒: 安定重視、システム負荷を抑えた長時間動作
    - 4秒: 低負荷動作、バックグラウンド監視用途
    - 5秒: 最低負荷、定期的なスナップショット取得
-   **UI制御**: Win32 ComboBox API (`CB_ADDSTRING`, `CB_SETITEMDATA`, `CB_GETCURSEL`)
-   **データ管理**: 各項目に間隔値（`u64`秒）を関連付け
-   **状態同期**: AutoClicker経由でアプリケーション全体の間隔設定共有

【自動化用途別推奨設定】
-   **リアルタイム監視**: 1-2秒間隔、動的コンテンツの変化追跡
-   **定期スナップショット**: 3-4秒間隔、安定したドキュメント作成
-   **長時間監視**: 5秒間隔、システム負荷最小化
-   **プレゼンテーション記録**: 2-3秒間隔、適度な詳細度での記録

【実装詳細】
-   コンボボックス項目の表示テキスト（"N秒"）と内部データ（秒数値）の分離管理
-   UTF-16エンコーディングによるWin32 API互換性確保
-   エラーハンドリング付きの安全なWin32 API呼び出し
-   AutoClickerモジュールとの密接な連携

【AI解析用：依存関係】
-   `windows`クレート: Win32 API（ダイアログ制御、コンボボックス管理）
-   `app_state.rs`: AutoClickerインスタンスとの間隔設定同期
-   `constants.rs`: `IDC_AUTO_CLICK_INTERVAL_COMBO`コントロールID定義
-   `auto_click_checkbox_handler.rs`: チェックボックスによる有効/無効制御
-   `auto_click.rs`: 実際の間隔制御を行うAutoClickerロジック
-   メインダイアログ: CBN_SELCHANGE通知メッセージの受信
 */

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::*, // ウィンドウとメッセージ処理
};

use crate::{app_state::AppState, constants::*};

/// 自動クリック間隔コンボボックスを初期化（1秒〜5秒、1秒刻み）
///
/// 自動連続クリックの実行間隔を設定するコンボボックスに、1秒から5秒までの選択肢を1秒刻みで追加します。
/// デフォルト値として1秒を選択状態にします。
///
/// # 引数
/// * `hwnd` - ダイアログウィンドウハンドル。
pub fn initialize_auto_click_interval_combo(hwnd: HWND) {
    if let Ok(combo_hwnd) = unsafe { GetDlgItem(Some(hwnd), IDC_AUTO_CLICK_INTERVAL_COMBO) } {
        // 1秒から5秒まで1秒刻みで項目を追加
        for interval_sec in 1..=5u64 {
            let text = format!("{}秒\0", interval_sec);
            let wide_text: Vec<u16> = text.encode_utf16().collect();
            let index = unsafe {
                SendMessageW(
                    combo_hwnd,
                    CB_ADDSTRING,
                    Some(WPARAM(0)),
                    Some(LPARAM(wide_text.as_ptr() as isize)),
                )
            }
            .0 as usize;
            unsafe {
                SendMessageW(
                    combo_hwnd,
                    CB_SETITEMDATA,
                    Some(WPARAM(index)),
                    Some(LPARAM((interval_sec * 1000) as isize)),
                );
            }
        }

        // デフォルト値（1秒）を選択
        unsafe {
            SendMessageW(combo_hwnd, CB_SETCURSEL, Some(WPARAM(0)), Some(LPARAM(0)));
        }
    }
}

/// 自動クリック間隔コンボボックスの選択変更を処理する
///
/// # 引数
/// * `hwnd` - ダイアログウィンドウハンドル
///
/// # 処理内容
/// コンボボックスで選択された項目から間隔の値（ミリ秒）を取得し、`AppState` の `auto_clicker` に設定します。
pub fn handle_auto_click_interval_combo_change(hwnd: HWND) {
    if let Ok(combo_hwnd) = unsafe { GetDlgItem(Some(hwnd), IDC_AUTO_CLICK_INTERVAL_COMBO) } {
        // 現在選択されているインデックスを取得
        let selected_index =
            unsafe { SendMessageW(combo_hwnd, CB_GETCURSEL, Some(WPARAM(0)), Some(LPARAM(0))).0 }
                as i32;

        if selected_index >= 0 {
            // 選択された項目のデータを直接取得
            let interval_value = unsafe {
                SendMessageW(
                    combo_hwnd,
                    CB_GETITEMDATA,
                    Some(WPARAM(selected_index as usize)),
                    Some(LPARAM(0)),
                )
            }
            .0 as u64;

            // AppStateに保存
            let app_state = AppState::get_app_state_mut();
            app_state.auto_clicker.set_interval(interval_value);

            println!("自動クリック間隔設定変更: {}ms", interval_value);
        }
    }
}
