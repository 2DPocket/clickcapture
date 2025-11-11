/*
============================================================================
エリア選択機能モジュール (area_select.rs)
============================================================================

【ファイル概要】
マウスドラッグによる画面領域選択機能を提供するモジュール。
エリア選択モードの開始・終了、オーバーレイ表示制御を管理し、
`area_select_overlay.rs` と連携して直感的な領域選択UXを実現します。
実際のマウスイベント処理は `hook/mouse.rs` が担当します。

【主要機能】
1.  **エリア選択モード制御 (`start_area_select_mode`, `cancel_area_select_mode`)**:
    -   モードの開始/終了を管理し、関連リソース（フック、オーバーレイ）を制御します。
2.  **領域確定処理 (`end_area_select_mode`)**:
    -   ドラッグ操作で選択された矩形領域を `AppState` に保存します。
3.  **オーバーレイ連携**:
    -   `area_select_overlay` を表示/非表示にし、ユーザーに視覚的なフィードバックを提供します。

【処理フロー】
1.  **[UI]** 「エリア選択」ボタンクリック
2.  **`start_area_select_mode()`**:
    -   `AppState` の `is_area_select_mode` を `true` に設定。
    -   マウスとキーボードのフックをインストール (`install_hooks`)。
    -   `area_select_overlay` を表示。
3.  **[マウスフック]** `WM_LBUTTONDOWN` でドラッグ開始 (`is_dragging = true`)。
4.  **[マウスフック]** `WM_MOUSEMOVE` でドラッグ中の矩形をオーバーレイに再描画。
5.  **[マウスフック]** `WM_LBUTTONUP` で `end_area_select_mode()` を呼び出し。
6.  **`end_area_select_mode()`**:
    -   選択された `RECT` を `AppState` に保存。
    -   `cancel_area_select_mode()` を呼び出してモードを終了。
7.  **`cancel_area_select_mode()`** (完了またはESCキーでのキャンセル時):
    -   フックをアンインストールし、オーバーレイを非表示にする。

【技術仕様】
-   **オーバーレイ**: `area_select_overlay` が `LayeredWindow` を使用して半透明描画。
-   **イベント監視**: `WH_MOUSE_LL` と `WH_KEYBOARD_LL` フックを利用してシステム全体のマウス・キーボードイベントを監視。
-   **状態管理**: `AppState` を介してモードフラグや選択領域を安全に共有。

============================================================================
*/

use windows::Win32::{
    Foundation::{POINT, RECT},
    UI::WindowsAndMessaging::{GetCursorPos, MB_ICONERROR, MB_OK},
};

use crate::{
    app_state::*,
    hook::*,
    overlay::*,
    system_utils::*,
    ui::{
        dialog_handler::{bring_dialog_to_back, bring_dialog_to_front},
        input_control_handlers::update_input_control_states,
    },
};

/**
 * エリア選択モードを開始する
 *
 * マウスドラッグによる画面領域選択モードを開始し、必要な視覚効果（オーバーレイ）と
 * システムフック（マウス・キーボード）を初期化します。
 *
 * # 処理フロー
 * 1. 重複起動をチェックし、モード中でなければ続行します。
 * 2. `AppState` の `is_area_select_mode` フラグを `true` に設定します。
 * 3. マウスとキーボードのグローバルフックをインストールします (`install_hooks`)。
 * 4. `area_select_overlay` を表示し、全画面を半透明に覆います。
 * 5. UIコントロールの状態を「エリア選択モード」に合わせて更新します。
 * 6. メインダイアログを最小化し、画面操作の邪魔にならないようにします。
 *
 * # エラーハンドリング
 * - 既にエリア選択モードの場合は、メッセージボックスを表示して処理を中断します。
 * - オーバーレイの表示に失敗した場合は、モードを即座にキャンセルしてクリーンアップします。
 *
 * # 副作用
 * - システム全体のマウス・キーボードフックが有効になります。
 * - 全画面を覆うオーバーレイウィンドウが表示されます。
 * - `AppState` の `is_area_select_mode` フラグが `true` になります。
 *
 */
pub fn start_area_select_mode() {
    unsafe {
        // 重複起動を防止
        let app_state = AppState::get_app_state_mut();
        if app_state.is_area_select_mode {
            show_message_box(
                "既にエリア選択モード中です",
                "エリア選択エラー",
                MB_OK | MB_ICONERROR,
            );
            return;
        }

        app_log("エリア選択モードを開始しました (エスケープキーでキャンセル可能)");

        // 現在のマウス位置を取得して状態を初期化
        let mut current_pos = POINT { x: 0, y: 0 };
        if GetCursorPos(&mut current_pos).is_ok() {
            println!("現在のマウス位置: ({}, {})", current_pos.x, current_pos.y);

            // AppState状態更新
            app_state.is_area_select_mode = true;
            app_state.current_mouse_pos = current_pos; // 初期位置設定

            // システムフックを開始（ESCキーでのキャンセルとマウス操作の監視）
            install_hooks();
        }

        // エリア選択用のオーバーレイを表示
        if let Some(overlay) = app_state.area_select_overlay.as_mut() {
            if let Err(e) = overlay.show_overlay() {
                eprintln!("❌ エリア選択オーバーレイの表示に失敗: {:?}", e);
                cancel_area_select_mode(); // エラー時はモードをキャンセル
            }
        }

        // UIコントロールの状態を更新
        update_input_control_states();

        // メインダイアログを最小化
        bring_dialog_to_back();
    }
}

/**
 * エリア選択を完了し、選択領域を確定する
 *
 * ユーザーがマウスドラッグで選択した領域を `AppState` に保存します。
 * この関数は、ドラッグ操作が完了したとき（`WM_LBUTTONUP`）に `hook/mouse.rs` から呼び出されます。
 * 処理完了後、`cancel_area_select_mode` を呼び出してモードを終了し、リソースを解放します。
 *
 * # 処理フロー
 * 1. `AppState` からドラッグの開始点と終了点を取得し、正規化された `RECT` を作成します。
 * 2. 作成した `RECT` を `AppState` の `selected_area` に保存します。
 * 3. `cancel_area_select_mode` を呼び出して、クリーンアップ処理を実行します。
 *
 * # 保存される状態
 * - `app_state.selected_area`: 後続のキャプチャ処理でこの領域が使用されます。
 */
pub fn end_area_select_mode() {
    let app_state = AppState::get_app_state_mut();

    // 選択矩形の座標を取得
    let (left, top, right, bottom) = {
        let left = app_state.drag_start.x.min(app_state.drag_end.x);
        let top = app_state.drag_start.y.min(app_state.drag_end.y);
        let right = app_state.drag_start.x.max(app_state.drag_end.x);
        let bottom = app_state.drag_start.y.max(app_state.drag_end.y);
        (left, top, right, bottom)
    };

    let rect = RECT {
        left,
        top,
        right,
        bottom,
    };

    app_log(&format!(
        "✅ エリア選択完了: ({}, {}) - ({}, {})",
        rect.left, rect.top, rect.right, rect.bottom
    ));

    // 選択領域をAppStateに保存
    app_state.selected_area = Some(rect);

    // 共通の終了処理を呼び出す
    cancel_area_select_mode();
}

/**
 * エリア選択モードを終了（キャンセル）する
 *
 * エリア選択モードを安全に終了し、関連するシステムリソース（フック、オーバーレイ）を解放します。
 * この関数は、領域選択が完了したとき (`end_area_select_mode` から) または
 * ESCキーでキャンセルされたとき (`hook/keyboard.rs` から) に呼び出されます。
 *
 * # クリーンアップ処理
 * 1. `AppState` の `is_area_select_mode` と `is_dragging` フラグを `false` にリセットします。
 * 2. `area_select_overlay` を非表示にします。
 * 3. マウスとキーボードのフックをアンインストールします (`uninstall_hooks`)。
 * 4. UIコントロールの状態を通常モードに戻します。
 * 5. メインダイアログを復元し、最前面に表示します。
 */
pub fn cancel_area_select_mode() {
    let app_state = AppState::get_app_state_mut();

    // 【Step 1】AppState フラグの安全な初期化
    app_state.is_area_select_mode = false; // エリア選択モード終了

    // ドラッグ中だった場合もフラグをリセット
    if app_state.is_dragging {
        app_state.is_dragging = false;
    }

    // オーバーレイを非表示にする
    if let Some(overlay) = app_state.area_select_overlay.as_mut() {
        overlay.hide_overlay();
    }

    // システムフックを停止
    uninstall_hooks();
    // UIコントロールの状態を更新
    update_input_control_states();

    // メインダイアログを復元して最前面に表示
    bring_dialog_to_front();

    println!("エリア選択モードを終了します");
}
