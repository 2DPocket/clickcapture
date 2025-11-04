/*
============================================================================
自動連続クリック機能モジュール (auto_click.rs)
============================================================================

【機能概要】
キャプチャモード中に、指定された回数と間隔で自動的にマウスクリックを発生させ、
連続的なスクリーンショット撮影を実現します。
メインUIスレッドをブロックしないように、バックグラウンドスレッドで実行されます。

【主要機能】
1.  **`AutoClicker` 構造体**: 自動クリック機能の状態（有効/無効、間隔、回数など）を管理します。
2.  **バックグラウンド実行**: `std::thread` を使用して、クリック処理を別スレッドで実行し、UIの応答性を維持します。
3.  **安全なスレッド制御**:
    -   `Arc<AtomicBool>` を使用した停止フラグにより、外部から安全にスレッドを停止させることができます。
    -   `Drop` トレイトを実装し、`AutoClicker` インスタンスが破棄される際にスレッドが確実に終了するように保証します。
4.  **メインスレッドへの通知**: 処理完了後、`PostMessageW` を使用してメインダイアログに非同期メッセージ (`WM_AUTO_CLICK_COMPLETE`) を送信し、後処理を促します。

【技術仕様】
-   **クリックシミュレーション**: `SendInput` API を使用して、物理的なマウスクリックイベントを生成します。
-   **スレッド同期**: `Arc` と `Atomic*` 型（`AtomicBool`, `AtomicU32`）を使用して、スレッド間で安全に状態を共有・変更します。

【処理フロー】
1.  **[UI]** ユーザーが自動クリックを有効にし、キャプチャモードを開始します。
2.  **[マウスフック]** ユーザーが画面を初めてクリックすると、`hook/mouse.rs` が `AutoClicker::start()` を呼び出します。
3.  **`AutoClicker::start()`**:
    -   停止フラグをリセットし、新しいバックグラウンドスレッドを生成します。
    -   スレッド内で `auto_click_loop` 関数が実行されます。
4.  **`auto_click_loop()`**:
    -   指定された間隔で待機します。
    -   `perform_mouse_click()` を呼び出して、`start`時に指定された座標でクリックをシミュレートします。
    -   このシミュレートされたクリックは `hook/mouse.rs` に捕捉され、`capture_screen_area_with_counter()` が実行されます。
    -   指定回数に達するか、停止フラグが立てられるまで上記を繰り返します。
5.  **[ループ終了後]**:
    -   `PostMessageW` でメインダイアログに `WM_AUTO_CLICK_COMPLETE` メッセージを送信します。
6.  **[main.rs]**: `WM_AUTO_CLICK_COMPLETE` を受信し、キャプチャモードを終了するなどの後処理を実行します。

【AI解析用：依存関係】
- `hook/mouse.rs`: ユーザーの最初のクリックをトリガーとして `AutoClicker::start` を呼び出す。
- `main.rs`: `WM_AUTO_CLICK_COMPLETE` メッセージを受信して後処理を行う。
- `app_state.rs`: `AppState` に `AutoClicker` インスタンスを保持する。
*/

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread;
use std::time::Duration;

use windows::Win32::UI::WindowsAndMessaging::{MB_ICONWARNING, MB_OK, PostMessageW};
use windows::Win32::{
    Foundation::{LPARAM, POINT, WPARAM},
    UI::Input::KeyboardAndMouse::*,
};

use crate::app_state::AppState;
use crate::constants::WM_AUTO_CLICK_COMPLETE;
use crate::overlay::Overlay;
use crate::system_utils::{app_log, show_message_box};

const MAX_CAPTURE_COUNT: u32 = 999; // 最大連続クリック数制限

/// 自動連続クリック機能の状態と制御を管理する
#[derive(Debug)]
pub struct AutoClicker {
    enabled: bool,                                 // 機能がUI上で有効かどうかのフラグ
    stop_flag: Arc<AtomicBool>,                    // バックグラウンドスレッドを停止させるためのフラグ
    interval_ms: u64,                              // クリック実行間隔（ミリ秒）
    progress_count: Arc<AtomicU32>,                // 現在の実行回数
    max_count: Arc<AtomicU32>,                     // 設定された最大実行回数
    thread_handle: Option<thread::JoinHandle<()>>, // バックグラウンドスレッドのハンドル
}

impl AutoClicker {
    /// `AutoClicker` の新しいインスタンスをデフォルト値で作成する
    pub fn new() -> Self {
        Self {
            enabled: false,
            stop_flag: Arc::new(AtomicBool::new(true)),
            interval_ms: 1000, // デフォルト1秒
            progress_count: Arc::new(AtomicU32::new(0)),
            max_count: Arc::new(AtomicU32::new(0)),
            thread_handle: None,
        }
    }

    /// 機能が有効化されているかを取得する
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// 機能の有効/無効を設定する
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// バックグラウンドスレッドが実行中かを確認する
    pub fn is_running(&self) -> bool {
        self.thread_handle.is_some()
    }

    /// クリック間隔（ミリ秒）を設定する
    pub fn set_interval(&mut self, interval_ms: u64) {
        self.interval_ms = interval_ms;
    }

    /// 現在の実行回数を取得する
    pub fn get_progress_count(&self) -> u32 {
        self.progress_count.load(Ordering::Relaxed)
    }

    /// 最大実行回数を設定する
    pub fn set_max_count(&mut self, count: u32) {
        self.max_count.store(count, Ordering::Relaxed);
    }

    /// 設定された最大実行回数を取得する
    pub fn get_max_count(&self) -> u32 {
        self.max_count.load(Ordering::Relaxed)
    }

    /// 自動連続クリック処理をバックグラウンドスレッドで開始する
    ///
    /// # 引数
    /// * `position` - クリックをシミュレートする画面上の座標。
    pub fn start(&mut self, position: POINT) -> Result<(), String> {
        if self.thread_handle.is_some() {
            return Err("連続クリックは既に開始されています".to_string());
        }

        // スレッドを開始する前に停止フラグをリセット
        self.stop_flag.store(false, Ordering::Relaxed);
        let stop_flag = Arc::clone(&self.stop_flag);

        let interval = self.interval_ms;

        let max_count = Arc::clone(&self.max_count);

        self.progress_count.store(0, Ordering::Relaxed);
        let progress_count = Arc::clone(&self.progress_count);

        // バックグラウンドスレッドで連続クリック実行
        let handle = thread::spawn(move || {
            auto_click_loop(stop_flag, interval, progress_count, max_count, position);
        });

        self.thread_handle = Some(handle);
        app_log(&format!(
            "🖱️ 連続クリックを開始しました（{}ms間隔, {}回クリック）",
            interval,
            self.max_count.load(Ordering::Relaxed)
        ));

        Ok(())
    }

    /// 実行中の自動連続クリック処理を安全に停止する
    pub fn stop(&mut self) {
        if self.stop_flag.load(Ordering::Relaxed) {
            return; // 既に停止している場合は何もしない
        }

        // 停止フラグをセット
        self.stop_flag.store(true, Ordering::Relaxed);

        // スレッドの終了を待機
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
        app_log("🛑 自動連続クリック処理（スレッド）を停止しました");
    }
}

impl Drop for AutoClicker {
    /// `AutoClicker` インスタンスが破棄される際に、実行中のスレッドを確実に停止させる
    fn drop(&mut self) {
        self.stop();
    }
}

/// 自動クリックをバックグラウンドで実行するループ処理
///
/// # 引数
/// * `stop_flag` - ループを外部から停止させるためのフラグ。
/// * `interval_ms` - クリックを実行する間隔（ミリ秒）。
/// * `progress_count_boxed` - 実行回数をカウントするためのアトミックなカウンタ。
/// * `max_count_boxed` - 実行回数の上限。
/// * `position` - クリックをシミュレートする座標。
fn auto_click_loop(
    stop_flag: Arc<AtomicBool>,
    interval_ms: u64,
    progress_count_boxed: Arc<AtomicU32>,
    max_count_boxed: Arc<AtomicU32>,
    position: POINT,
) {
    let max_count = max_count_boxed.load(Ordering::Relaxed);
    let mut progress_count = progress_count_boxed.load(Ordering::Relaxed);

    let app_state = AppState::get_app_state_ref();

    while !stop_flag.load(Ordering::Relaxed) {
        // オーバーレイを最新状態に更新
        let overlay = app_state
            .capturing_overlay
            .as_ref()
            .expect("キャプチャーオーバーレイが存在しません。");
        overlay.refresh_overlay();

        // 指定された間隔で待機する。
        // ただし、長い待機時間中に停止要求があった場合に即座に応答できるよう、
        // 100ミリ秒ごとに短いスリープを繰り返し、その都度停止フラグを確認する。
        let sleep_duration = Duration::from_millis(interval_ms);
        let check_interval = Duration::from_millis(100);
        let mut remaining = sleep_duration;

        // `check_interval` ごとに停止フラグを確認しつつ、指定された `sleep_duration` に達するまで待機
        while remaining > Duration::from_millis(0) && !stop_flag.load(Ordering::Relaxed) {
            let sleep_time = remaining.min(check_interval);
            // 指定時間スリープ
            thread::sleep(sleep_time);
            remaining = remaining.saturating_sub(sleep_time);
        }

        // スリープ中に停止要求（例: ESCキー押下）があった場合、クリックを実行せずにループを抜ける
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        // 最大クリック数に到達したかチェック
        // `MAX_CAPTURE_COUNT` は暴走を防ぐための安全装置
        if progress_count >= MAX_CAPTURE_COUNT || progress_count >= max_count {
            if progress_count >= MAX_CAPTURE_COUNT {
                show_message_box(
                    &format!(
                        "⚠️ 連続クリックが最大クリック数({})に達しました。連続クリックを停止します。",
                        MAX_CAPTURE_COUNT
                    ),
                    "自動クリック警告",
                    MB_OK | MB_ICONWARNING,
                );
            }
            break;
        }

        // 実行回数をインクリメントし、クリックを実行
        progress_count += 1;
        app_log(&format!(
            "🖱️ 自動クリック実行: マウス位置({}, {}) {}/{}回目",
            position.x, position.y, progress_count, max_count
        ));

        // マウスクリックを実行
        if let Err(e) = perform_mouse_click(position) {
            app_log(&format!("❌ クリック実行エラー: {}", e));
            break;
        }
        progress_count_boxed.store(progress_count, Ordering::Relaxed);
    }

    // ループ終了後、メインスレッドに処理完了を非同期で通知する
    let app_state = AppState::get_app_state_ref();
    if let Some(hwnd) = app_state.dialog_hwnd {
        unsafe {
            // カスタムメッセージ（WM_AUTO_CLICK_COMPLETE）をダイアログのメッセージキューに送信
            if let Err(e) = PostMessageW(Some(*hwnd), WM_AUTO_CLICK_COMPLETE, WPARAM(0), LPARAM(0))
            {
                app_log(&format!("❌ メッセージ送信エラー: {}", e));
            }
        }
    }
}

/// `SendInput` APIを使用してマウスクリックをシミュレートする
///
/// 指定されたスクリーン座標で、マウスの左ボタンダウンと左ボタンアップの
/// イベントを連続して発生させる。
fn perform_mouse_click(position: POINT) -> Result<(), String> {
    unsafe {
        // マウス入力構造体を作成
        let mut inputs = [
            INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: position.x,
                        dy: position.y,
                        mouseData: 0,
                        dwFlags: MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_LEFTDOWN,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: position.x,
                        dy: position.y,
                        mouseData: 0,
                        dwFlags: MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_LEFTUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
        ];

        // 左クリック（押下→離上）を送信
        let result = SendInput(&mut inputs, std::mem::size_of::<INPUT>() as i32);

        if result == 2 {
            Ok(())
        } else {
            Err(format!("SendInput failed: expected 2, got {}", result))
        }
    }
}
