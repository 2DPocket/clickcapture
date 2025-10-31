
/*
============================================================================
連続自動クリック機能モジュール (auto_click.rs)
============================================================================
 
【機能概要】
キャプチャモード中に指定間隔で現在のマウス位置にクリックイベントを
自動発生させる連続クリック機能を提供
 
【主要機能】
1. 🖱️ 自動マウスクリック：SendInput APIによる物理クリック生成
2. ⏱️ 間隔制御：1秒間隔（設定可能）での連続実行
3. 🔄 スレッド管理：非同期実行・メインスレッド非ブロック
4. 🛑 即座停止：キャプチャモード終了時の安全な停止
 
【技術仕様】
- API使用：SendInput（物理マウスイベント生成）
- スレッド：std::thread（バックグラウンド実行）
- 同期制御：AtomicBool（スレッド間通信）
- 座標取得：GetCursorPos（リアルタイム位置）
*/

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use windows::Win32::UI::WindowsAndMessaging::{MB_ICONWARNING, MB_OK, PostMessageW};
use windows::Win32::{
    Foundation::{LPARAM, POINT, WPARAM},
    UI::Input::KeyboardAndMouse::*,
};

use crate::constants::WM_AUTO_CLICK_COMPLETE;
use crate::app_state::AppState;
use crate::overlay::Overlay;
use crate::system_utils::{app_log, show_message_box};

const MAX_CAPTURE_COUNT: u32 = 999; // 最大連続クリック数制限

/// 自動連続クリック機能の状態と制御を管理する構造体
#[derive(Debug)]
pub struct AutoClicker {
    enabled: bool,                                  // 連続クリック機能有効フラグ       
    stop_flag: Arc<AtomicBool>,                     // 停止フラグ（スレッド間共有）
    interval_ms: u64,                               // クリック間隔（ミリ秒）
    progress_count: Arc<AtomicU32>,                 // 現在のクリック回数進捗
    max_count: Arc<AtomicU32>,                      // 最大クリック回数設定
    thread_handle: Option<thread::JoinHandle<()>>,  // バックグラウンドスレッドハンドル
}

impl AutoClicker {
    /// 新しい連続クリッカーを作成
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

    /// `enabled`フィールドのゲッター
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// `enabled`フィールドのセッター
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// スレッドが実行中か確認するゲッター
    pub fn is_running(&self) -> bool {
        self.thread_handle.is_some()
    }

    /// 間隔を更新するセッター
    pub fn set_interval(&mut self, interval_ms: u64) {
        self.interval_ms = interval_ms;
    }

    /// 現在の進捗カウントのゲッター
    pub fn get_progress_count(&self) -> u32 {
        self.progress_count.load(Ordering::Relaxed)
    }

    // 最大数を更新するセッター
    pub fn set_max_count(&mut self, count: u32) {
        self.max_count.store(count, Ordering::Relaxed);
    }

    /// 最大数のゲッター
    pub fn get_max_count(&self) -> u32 {
        self.max_count.load(Ordering::Relaxed)
    }

    /// 連続クリックを開始
    pub fn start(&mut self,  position: POINT) -> Result<(), String> {
        if self.thread_handle.is_some() {
            return Err("連続クリックは既に開始されています".to_string());
        }


        // 停止フラグをリセット
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
        app_log(&format!("🖱️ 連続クリックを開始しました（{}ms間隔, {}回クリック）", interval, self.max_count.load(Ordering::Relaxed)));

        Ok(())
    }

    /// 連続クリックを停止
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
    fn drop(&mut self) {
        self.stop();
    }
}

/// 自動クリックをバックグラウンドで実行するループ関数
fn auto_click_loop(stop_flag: Arc<AtomicBool>, interval_ms: u64, progress_count_boxed: Arc<AtomicU32>, 
    max_count_boxed: Arc<AtomicU32>, position: POINT) {
    let max_count = max_count_boxed.load(Ordering::Relaxed);
    let mut progress_count = progress_count_boxed.load(Ordering::Relaxed);

    let app_state = AppState::get_app_state_ref();

    while !stop_flag.load(Ordering::Relaxed) {

        // オーバーレイを最新状態に更新
        let overlay = app_state.capturing_overlay.as_ref().expect("キャプチャーオーバーレイが存在しません。");
        overlay.refresh_overlay();

        // 指定間隔で待機（停止チェック付き）
        // 100ms毎に停止フラグを確認しながらスリープ
        let sleep_duration = Duration::from_millis(interval_ms);    // 実際のスリープ時間    
        let check_interval = Duration::from_millis(100);            // 100ms毎に停止チェック
        let mut remaining = sleep_duration;                         // 残り時間管理 

        // check_interval毎に停止フラグを確認しつつ実際のスリープ時間に達するまでスリープ
        while remaining > Duration::from_millis(0) && !stop_flag.load(Ordering::Relaxed) {
            let sleep_time = remaining.min(check_interval);
            // 指定時間スリープ
            thread::sleep(sleep_time);
            remaining = remaining.saturating_sub(sleep_time);
        }

        // 停止フラグが立っていればクリックをスキップしてループ終了
        // （スリープ中に停止要求（エスケープキー）があった場合の即時対応）
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        // 最大クリック数到達チェック
        // キャプチャモード終了も兼ねる
        if progress_count >= MAX_CAPTURE_COUNT || progress_count >= max_count {
            if progress_count >= MAX_CAPTURE_COUNT {
                show_message_box(&format!("⚠️ 連続クリックが最大クリック数({})に達しました。連続クリックを停止します。", MAX_CAPTURE_COUNT)
                    , "自動クリック警告", 
                    MB_OK | MB_ICONWARNING);
            }
            break;
        }

        // 開始したマウス位置で連続クリック
        progress_count += 1;
        app_log(&format!("🖱️ 自動クリック実行: マウス位置({}, {}) {}/{}回目", position.x, position.y, progress_count, max_count));

        // マウスクリックを実行
        if let Err(e) = perform_mouse_click(position) {
            app_log(&format!("❌ クリック実行エラー: {}", e));
            break;
        }
        progress_count_boxed.store(progress_count, Ordering::Relaxed);
    }

    // メインスレッドに処理完了を通知する
    let app_state = AppState::get_app_state_ref();
    if let Some(hwnd) = app_state.dialog_hwnd {
        unsafe {
            // カスタムメッセージ（WM_AUTO_CLICK_COMPLETE）をダイアログのメッセージキューに送信
            if let Err(e) = PostMessageW(Some(*hwnd), WM_AUTO_CLICK_COMPLETE, WPARAM(0), LPARAM(0)) {
                app_log(&format!("❌ メッセージ送信エラー: {}", e));
            }
        }
    }
}

/// SendInput APIを使用して、指定されたスクリーン座標でマウスクリック（左ボタンダウン→アップ）をシミュレートする
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
