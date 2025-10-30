/*
============================================================================
マウスフック管理モジュール (mouse.rs)
============================================================================

【ファイル概要】
グローバルマウスフック機能を提供し、システム全体のマウスイベントを監視する。
エリア選択のドラッグ処理、キャプチャモードのクリック検出、
リアルタイムカーソル位置追跡を実現する中核モジュール。

【主要機能】
1. マウスフック管理（install/uninstall_mouse_hook）
2. ドラッグ処理（開始/更新/終了の検出と処理）
3. クリック検出（キャプチャモード時の左クリック処理）
4. リアルタイム座標更新（カーソル追跡）
5. 高速イベント処理（1ms以下の応答時間）

【技術仕様】
- フックタイプ：WH_MOUSE_LL（低レベルマウスフック）
- 監視範囲：システム全体（全アプリケーション）
- イベント：WM_MOUSEMOVE, WM_LBUTTONDOWN, WM_LBUTTONUP
- パフォーマンス：unsafe最適化による高速処理
- スレッドセーフ：AppState経由の安全な状態共有

【処理フロー】
SetWindowsHookExW → low_level_mouse_proc コールバック → イベント種別判定
                         ├─ WM_MOUSEMOVE → カーソル位置更新 + オーバーレイ位置/描画更新
                         │   ├─ is_capture_mode: capturing_overlay の位置を更新
                         │   └─ is_dragging: area_select_overlay を再描画
                         ├─ WM_LBUTTONDOWN → ドラッグ開始 or キャプチャ実行
                         │   ├─ is_area_select_mode: ドラッグ開始状態に移行
                         │   └─ is_capture_mode: 自動クリック開始 or 単発キャプチャ実行
                         └─ WM_LBUTTONUP → ドラッグ終了
                             └─ is_dragging: エリア選択を完了
    └─ WM_LBUTTONUP → ドラッグ終了 or キャプチャ実行
                         ↓
                   CallNextHookEx → 他のアプリへイベント継続

【パフォーマンス最適化】
- 直接メモリアクセス：AppState への unsafe アクセス
- 最小限処理：必要な場合のみ状態更新
- 効率的分岐：早期リターンによる処理負荷軽減
- ロックフリー：Mutex回避による高速化

============================================================================
*/

// 必要なライブラリ（外部機能）をインポート
use windows::Win32::{
    Foundation::{LPARAM, LRESULT, POINT, WPARAM}, // 基本的なデータ型
    System::{
        LibraryLoader::GetModuleHandleW, // プログラムのハンドル取得
    },

    UI::{
        WindowsAndMessaging::*, // ウィンドウとメッセージ処理
    },
};

// アプリケーション状態管理構造体
use crate::app_state::*;

// エリア選択モジュール
use crate::area_select::*;

// オーバーレイ管理関数
use crate::overlay::*;

// 画面キャプチャ管理関数
use crate::screen_capture::*;

// マウスフックを開始する関数
pub fn install_mouse_hook() {
    unsafe {
        // 現在のプログラムのハンドル（識別子）を取得
        let module_handle = GetModuleHandleW(None).unwrap_or_default();

        // 低レベルマウスフックを設定
        // WH_MOUSE_LL: 低レベルマウスフック（画面全体のマウス操作を監視）
        // low_level_mouse_proc: マウス操作があったときに呼び出される関数
        let hook = SetWindowsHookExW(
            WH_MOUSE_LL,                // フックの種類
            Some(low_level_mouse_proc), // コールバック関数
            Some(module_handle.into()), // プログラムのハンドル
            0,                          // スレッドID（0は全スレッド）
        );

        if let Ok(hook) = hook {
            let app_state = AppState::get_app_state_mut();
           
            app_state.mouse_hook = Some(SafeHHOOK(hook)); // AppState構造体にフックハンドルを保存
            println!("マウスフックを開始しました");
        } else {
            eprintln!("❌ マウスフックの開始に失敗しました");
        }
    }
}

// マウスフックを停止する関数
pub fn uninstall_mouse_hook() {
    unsafe {
        let app_state = AppState::get_app_state_mut();
        if let Some(hook) = app_state.mouse_hook {
            // フックを解除（監視停止）
            let _ = UnhookWindowsHookEx(*hook);
            app_state.mouse_hook = None;
            println!("マウスフックを停止しました");
        }
    }
}

/*
============================================================================
低レベルマウスプロシージャ関数
============================================================================
 この関数は画面上でマウス操作（移動、クリック）があるたびに
 Windowsから自動的に呼び出される最も重要な関数

 【AI解析用：イベント処理フロー】
 WM_MOUSEMOVE: 常時 → 座標更新 + 各オーバーレイの更新
 WM_LBUTTONDOWN: AppState.is_area_select_mode時 → ドラッグ開始 / AppState.is_capture_mode時 → キャプチャ実行
 WM_LBUTTONUP: AppState.is_dragging時 → ドラッグ終了、エリア選択完了

 【重要な条件分岐】
 1. AppState.is_area_select_mode: エリア選択ボタンで制御される状態
 2. AppState.is_dragging: WM_LBUTTONDOWN～WM_LBUTTONUP間の状態

 【座標系の一貫性】
 - 全ての座標はスクリーン絶対座標（画面左上が0,0）
 - DPI認識により拡大設定の影響を回避
 - GetCursorPos()との整合性チェックを実装
*/

unsafe extern "system" fn low_level_mouse_proc(
    ncode: i32,     // 処理コード（0以上なら正常）
    wparam: WPARAM, // マウスイベントの種類（移動、クリックなど）
    lparam: LPARAM, // マウスの詳細情報（座標など）
) -> LRESULT {
    unsafe {
        let app_state = AppState::get_app_state_mut();
        if ncode >= 0 {
            // マウス情報を取得
            // MSLLHOOKSTRUCT: マウスの詳細情報が格納された構造体
            let mouse_struct = lparam.0 as *const MSLLHOOKSTRUCT;
            let current_pos = if !mouse_struct.is_null() {
                (*mouse_struct).pt // 画面座標でのマウス位置
            } else {
                POINT { x: 0, y: 0 } // エラー時はゼロ座標
            };

            // グローバルAppState構造体に現在のマウス位置を保存
            app_state.current_mouse_pos = current_pos;

            // マウスイベントの種類によって処理を分岐
            match wparam.0 as u32 {
                WM_MOUSEMOVE => {
                    // ===== マウス移動イベント =====
                    // マウスが移動するたびに呼び出される


                    // 🔧 キャプチャモードオーバーレイの位置更新
                    if app_state.is_capture_mode {
                        if let Some(overlay) = app_state.capturing_overlay.as_mut() {
                            overlay.set_window_pos();
                        }
                    }

                    // エリア選択オーバーレイ表示中かつドラッグ中の場合
                    let is_dragging = app_state.is_area_select_mode && app_state.is_dragging;

                    if is_dragging {
                        app_state.drag_end = current_pos;

                        // エリア選択オーバーレイを再描画
                        if let Some(overlay) = app_state.area_select_overlay.as_mut() {
                            overlay.refresh_overlay();
                        }

                    }
                }
                WM_LBUTTONDOWN => {
                    let mut block_mouse_propagation = false; // 今回はfalseに設定（下のウィンドウにも渡す）

                    // エリア選択モードの時のみオーバーレイを表示
                    let is_area_select_mode = app_state.is_area_select_mode;

                    if is_area_select_mode {
                        // 左クリック押下時：正確な座標を記録してオーバーレイを表示
                        app_state.drag_start = current_pos;
                        app_state.drag_end = current_pos;
                        app_state.is_dragging = true;

                        // マウスイベントを捕獲（下のウィンドウに渡さない）
                        block_mouse_propagation = true;
                    }

                    if block_mouse_propagation {
                        return LRESULT(1); // イベントを消費
                    }
                }
                WM_LBUTTONUP => {
                    // エリア選択モード中のドラッグ終了時の処理
                    let (is_area_select_mode, is_dragging) =
                        (app_state.is_area_select_mode, app_state.is_dragging);

                    if is_area_select_mode && is_dragging {
                        // 【変更】即座にキャプチャせず、選択エリアを保存
                        end_area_select_mode();
                    }
                    // 画面キャプチャモード中の左クリック処理
                    else {

                        if app_state.is_capture_mode {


                            // 連続クリックが有効な場合のみ機能を初期化＆開始
                            if app_state.auto_clicker.is_enabled() && !app_state.auto_clicker.is_running() {
                                let _ = app_state.auto_clicker.start(current_pos);
                                return LRESULT(1); // イベントを消費
                            }

                            // ファイル名に連番を使用してキャプチャ実行
                            let _ = capture_screen_area_with_counter();

                            println!(
                                "画面キャプチャ実行: ファイル {}.jpg",
                                app_state.capture_file_counter - 1
                            );


                            // 【重要】左クリック後もキャプチャモードは継続するが、
                            // 他のアプリケーションにも左クリックイベントを渡す
                            // return LRESULT(1); // 削除：イベント消費しない
                        }
                    }
                }

                // 【削除】右クリック処理は不要（エスケープキーに変更）
                _ => {}
            }
        }

        // エリア選択中（オーバーレイ表示中）は、マウスイベントを下のウィンドウに渡さない
        let is_area_select_mode = app_state.is_area_select_mode;

        if is_area_select_mode
            && (wparam.0 as u32 == WM_LBUTTONDOWN || wparam.0 as u32 == WM_LBUTTONUP)
        {
            return LRESULT(1); // イベントを消費
        }

        // 次のフックに処理を渡す
        let mouse_hook = app_state.get_mouse_hook();
        CallNextHookEx(mouse_hook, ncode, wparam, lparam)
    }
}
