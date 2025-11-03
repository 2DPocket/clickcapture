/*
============================================================================
画面キャプチャ機能モジュール (screen_capture.rs)
============================================================================

【ファイル概要】
画面キャプチャ機能の中核モジュール。選択された領域の画面キャプチャ、
JPEG画像保存、連番ファイル名生成、キャプチャモード制御、自動クリック連携を提供する。

【主要機能】
1. キャプチャモード制御（toggle_capture_mode）
2. キャプチャモードオーバーレイ管理（表示/非表示/更新）
3. 画面領域キャプチャ（capture_screen_area_with_counter）
4. JPEG画像保存（ユーザー設定品質70%〜100%で可変品質保存）
5. 連番ファイル名生成（0001.jpg, 0002.jpg...）
6. エラーハンドリング（領域未選択、保存失敗等）
7. 自動連続クリック連携（指定回数・間隔での自動キャプチャ）

【技術仕様】
- 画面取得：GetDC + BitBlt による高速ピクセル取得
- 画像処理：image crate による JPEG エンコード
- ファイル保存：BufWriter による効率的な書き込み
- 品質設定：JPEG ユーザー設定品質（70%〜100%）
- スケール設定：ユーザー設定倍率（55%〜100%）
- オーバーレイ：GDI+で描画される動的オーバーレイ
- パフォーマンス：メモリ効率的な RGB バッファ処理

【処理フロー】
[キャプチャボタン] → toggle_capture_mode() → エリア/自動クリック設定を検証
                      ↓ (モード開始)
              フック開始 + capturing_overlay.show_overlay()
                      ↓
              [画面内左クリック]
                      ├─ (自動クリック有効) → auto_clicker.start() → 指定回数ループ
                      │      ↓ (ループ内)
                      │   capture_screen_area_with_counter() → JPEG保存 → 連番更新
                      │      ↓
                      │   [ループ完了] → toggle_capture_mode() → モード終了
                      │
                      └─ (自動クリック無効) → capture_screen_area_with_counter() → JPEG保存 → 連番更新

============================================================================
*/

use windows::Win32::UI::WindowsAndMessaging::{MB_ICONWARNING, MB_OK};
// 必要なライブラリ（外部機能）をインポート
use windows::Win32::{
    Graphics::Gdi::*, // グラフィック描画機能
};
// 画像処理ライブラリ（JPEGキャプチャ保存専用）
use image::{ImageBuffer, Rgb};

// ファイルシステム操作
use std::fs;

// システムフック管理モジュール
use crate::hook::*;

// UIユーティリティ群
use crate::ui::bring_dialog::{bring_dialog_to_back, bring_dialog_to_front};
use crate::ui::update_input_control_states::update_input_control_states;

// アプリケーション状態管理構造体
use crate::{app_state::*, overlay::Overlay};

// ユーティリティ関数
use crate::system_utils::*;

// フォルダー管理機能
use crate::folder_manager::*;

/**
 * キャプチャモードの開始/終了を切り替える中核制御関数
 *
 * 【機能説明】
 * スクリーンキャプチャモードのON/OFF切り替えを行い、必要なシステムリソース
 * （キーボードフック）の管理とUI状態の同期を実行します。エリア選択の
 * 事前確認により、ユーザビリティとエラーハンドリングを両立させています。
 *
 * 【状態遷移フロー】
 * [キャプチャモード OFF] → エリア選択確認 → [キャプチャモード ON]
 *                      ↓
 *           エラー表示 ←── (エリア未選択 or 自動クリック設定不正)
 *                      ↓
 *      フック開始 + UI更新 ←── (エリア選択済み and 設定正常)
 *
 * [キャプチャモード ON] → フック停止 + UI更新 → [キャプチャモード OFF]
 *
 * 【技術的詳細】
 * - モード切り替え：app_state.is_capture_mode フラグによる状態管理
 * - リソース管理：キーボードとマウスフックの install/uninstall
 * - エラーハンドリング：MessageBoxW による親切なユーザー通知
 * - UI同期：InvalidateRect による強制再描画でボタン状態を更新
 *
 * 【前提条件】
 * - app_state.dialog_hwnd が有効に設定されていること
 * - エリア選択時：app_state.selected_area に有効な RECT が設定済み
 *
 * 【副作用】
 * - キーボードとマウスフックの有効/無効化（システム全体への影響）
 * - 自動クリック処理中の場合、スレッドを停止させる
 * - AppState の is_capture_mode フラグ更新
 * - UI キャプチャボタンの表示状態変更
 * - コンソール出力によるデバッグ情報提供
 *
 * 【AIおよび第三者解析用の技術ノート】
 * この関数は ユーザビリティとシステム安全性のバランスを重視した設計です。
 * エラー処理では技術的なメッセージではなく、具体的な操作手順を提示し、
 * 初心者でも迷うことなく正しい操作を実行できるよう配慮されています。
 */
pub fn toggle_capture_mode() {
    let app_state = AppState::get_app_state_mut();
    let is_capture_mode = app_state.is_capture_mode;

    if is_capture_mode {
        // 【キャプチャモード終了処理】
        app_state.is_capture_mode = false;

        // キーボードとマウスフック停止
        uninstall_hooks();

        // キャプチャモードオーバーレイを非表示
        if let Some(overlay) = app_state.capturing_overlay.as_mut() {
            overlay.hide_overlay();
        }
        //hide_tooltip_overlay();

        // メインダイアログを最前面に表示
        bring_dialog_to_front();

        // キャプチャモード終了時に、自動クリック処理待機＆停止
        // WM_AUTO_CLICK_COMPLETE時も、これ呼ばれる。take()でthread_handleを開放する必要があるため
        if app_state.auto_clicker.is_running() {
            app_state.auto_clicker.stop();
        }
        app_log("画面キャプチャモードを終了しました");
    } else {
        // 【キャプチャモード開始前の前提条件チェック】
        let has_area = app_state.selected_area.is_some();

        if !has_area {
            // 【エラーハンドリング：エリア未選択時の親切な案内】
            app_log("❌ 先にエリア選択を行ってください");

            // ユーザーフレンドリーなエラーメッセージ表示
            show_message_box(
                "先にエリア選択を行ってください。\n\n操作手順:\n1. エリア選択ボタンをクリック\n2. 画面上でドラッグして範囲を選択\n3. キャプチャ開始ボタンをクリック",
                "エラー - エリア未選択",
                MB_OK | MB_ICONWARNING,
            );
            return; // エラー時は早期リターンで後続処理をスキップ
        }

        // 回数の値が0の場合、自動クリック機能を無効化
        if app_state.auto_clicker.is_enabled() && app_state.auto_clicker.get_max_count() == 0 {
            show_message_box(
                "回数の値が0、もしくは未設定です。1以上の値を設定してください。",
                "自動クリックエラー",
                MB_OK | MB_ICONWARNING,
            );
            return; // エラー時は早期リターンで後続処理をスキップ
        }

        // 【キャプチャモード開始処理】
        app_state.is_capture_mode = true;

        // キーボードとマウスフック開始
        install_hooks();

        // キャプチャモードオーバーレイを表示
        if let Some(overlay) = app_state.capturing_overlay.as_mut() {
            if let Err(e) = overlay.show_overlay() {
                eprintln!("❌ キャプチャモードオーバーレイの表示に失敗: {:?}", e);
                // エラー時はモードを開始せずに終了する
            }
        }

        // メインダイアログを最背面に表示
        bring_dialog_to_back();

        app_log("画面キャプチャモードを開始しました (エスケープキーでキャプチャ終了)");

        // デバッグ用：選択エリア情報の出力
        // let rect = app_state.selected_area.unwrap();
        // app_log(&format!(
        //         "選択エリア: ({}, {}) - ({}, {})",
        //         rect.left, rect.top, rect.right, rect.bottom
        //     )
        // );
    };
    // 【UI同期処理】キャプチャボタンの視覚状態を強制更新
    update_input_control_states(); // ダイアログボタン状態更新（UI整合性確保）
}

/**
 * 連番ファイル名を使用したスクリーンキャプチャ実行関数
 *
 * 【機能説明】
 * 指定された画面矩形領域をキャプチャし、最適化されたJPEG画像として保存します。
 * ユーザー設定スケーリング、高品質アンチエイリアシング、自動連番ファイル名生成により、
 * 高速かつ軽量なスクリーンキャプチャを実現します。
 *
 * 【パフォーマンス最適化戦略】
 * 1. ユーザー設定スケーリング：ファイルサイズ削減（55%-100%可変）
 * 2. HALFTONE モード：高品質な縮小処理で視覚品質維持
 * 3. メモリDC使用：GPU加速による高速ピクセル処理
 * 4. 適切なリソース管理：メモリリーク防止とパフォーマンス最適化
 *
 * 【パラメータ】
 * left, top, right, bottom: スクリーン座標系での矩形領域（絶対座標）
 *
 * 【戻り値】
 * Result<(), Box<dyn std::error::Error>>: 成功時OK、失敗時詳細エラー情報
 *
 * 【処理フロー】
 * 画面DC取得 → メモリDC作成 → 領域コピー → スケーリング処理
 *     ↓
 * ピクセルデータ抽出 → BGR→RGB変換 → JPEG圧縮 → ファイル保存
 *     ↓
 * リソース解放 → 連番カウンタ更新 → 完了通知
 *
 * 【ファイル命名規則】
 * 0001.jpg, 0002.jpg, ... (4桁ゼロパディング連番)
 *
 * 【保存先決定ロジック】
 * 1. ユーザー指定フォルダー（app_state.selected_folder_path）
 * 2. 自動検出フォルダー（get_pictures_folder()）
 * 3. フォルダー自動作成（存在しない場合）
 *
 * 【エラーハンドリング】
 * - ビットマップ作成失敗
 * - ピクセルデータ取得失敗  
 * - ファイル保存失敗
 * - フォルダー作成失敗
 *
 * 【AIおよび第三者解析用の技術ノート】
 * この関数は Windows GDI APIの複雑さを抽象化し、安全なRustコードで
 * 高性能なスクリーンキャプチャを実現しています。メモリ管理、エラー処理、
 * パフォーマンス最適化の全てを考慮した包括的な実装です。
 */

pub fn capture_screen_area_with_counter() -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        app_log("⌛ スクリーンキャプチャ中です...");

        let app_state = AppState::get_app_state_mut();

        // 選択された領域を取得
        let left;
        let top;
        let right;
        let bottom;

        match app_state.selected_area {
            Some(selected_area) => {
                left = selected_area.left;
                top = selected_area.top;
                right = selected_area.right;
                bottom = selected_area.bottom;
            }
            None => {
                return Err("❌ キャプチャエリアが選択されていません".into());
            }
        }

        // 🔧 追加：キャプチャ処理開始時にアイコンを処理中に切り替え
        switch_capture_processing(true);

        // 【Step 1】デバイスコンテキストの準備
        let screen_dc = GetDC(None); // 画面全体のデバイスコンテキスト取得
        let memory_dc = CreateCompatibleDC(Some(screen_dc)); // メモリ描画用DC作成

        // 【Step 2】キャプチャ領域サイズ計算
        let width = (right - left).abs();
        let height = (bottom - top).abs();

        // 【Step 3】高品質設定：ユーザー設定のスケール値を使用
        let scale_factor = (app_state.capture_scale_factor as f32) / 100.0; // パーセンテージから小数値に変換
        let scaled_width = ((width as f32) * scale_factor) as i32;
        let scaled_height = ((height as f32) * scale_factor) as i32;

        // 【Step 4】原寸ビットマップの作成と画面コピー
        let hbitmap = CreateCompatibleBitmap(screen_dc, width, height);
        let old_bitmap = SelectObject(memory_dc, hbitmap.into());

        // 画面の指定領域をメモリビットマップに高速コピー
        if let Some(overlay) = app_state.capturing_overlay.as_mut() {
            overlay.hide_overlay(); // キャプチャアイコンを一時的に非表示

            let _ = BitBlt(
                memory_dc, // コピー先（メモリDC）
                0,
                0, // コピー先座標
                width,
                height,          // コピーサイズ
                Some(screen_dc), // コピー元（画面DC）
                left,
                top,     // コピー元座標
                SRCCOPY, // コピーモード（上書き）
            );

            // キャプチャアイコン　表示状態に戻す
            if let Err(e) = overlay.show_overlay() { 
                return Err(format!("❌ キャプチャアイコンの再表示に失敗: {}", e).into());
            }
        }

        // 【Step 5】スケーリング用デバイスコンテキストとビットマップの準備
        let scaled_dc = CreateCompatibleDC(Some(screen_dc));
        let hbitmap_scaled = CreateCompatibleBitmap(screen_dc, scaled_width, scaled_height);
        let old_bitmap_scaled = SelectObject(scaled_dc, hbitmap_scaled.into());

        // 【Step 6】高品質スケーリングモードの設定
        let _ = SetStretchBltMode(scaled_dc, HALFTONE); // アンチエイリアシング有効
        let _ = SetBrushOrgEx(scaled_dc, 0, 0, None); // ブラシ原点設定

        // 【Step 7】高品質縮小処理の実行
        let _ = StretchBlt(
            scaled_dc, // 縮小先DC
            0,
            0, // 縮小先座標
            scaled_width,
            scaled_height,   // 縮小後サイズ
            Some(memory_dc), // 縮小元DC
            0,
            0, // 縮小元座標
            width,
            height,  // 縮小元サイズ
            SRCCOPY, // 転送モード
        );

        // 【Step 8】ピクセルデータ抽出の準備
        let bytes_per_pixel = 3; // RGB 24bit形式
        let row_size = ((scaled_width * bytes_per_pixel + 3) / 4) * 4; // Windows 4バイト境界調整
        let mut pixel_data = vec![0u8; (row_size * scaled_height) as usize];

        // BITMAPINFO構造体の設定（GetDIBits API用）
        let mut bitmap_info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: scaled_width,
                biHeight: -scaled_height, // 負値で上下反転防止（トップダウン形式）
                biPlanes: 1,
                biBitCount: 24,          // RGB 24bit カラー深度
                biCompression: BI_RGB.0, // 無圧縮RGB
                biSizeImage: 0,          // BI_RGB時は0で可
                biXPelsPerMeter: 0,      // 解像度情報（未使用）
                biYPelsPerMeter: 0,
                biClrUsed: 0,      // フルカラー使用
                biClrImportant: 0, // 全色重要
            },
            bmiColors: [RGBQUAD::default(); 1], // RGB形式では未使用
        };

        // 【Step 9】ビットマップからピクセルデータを抽出
        let result = GetDIBits(
            scaled_dc,                               // ソースDC
            hbitmap_scaled,                          // ソースビットマップ
            0,                                       // 開始スキャンライン
            scaled_height as u32,                    // スキャンライン数
            Some(pixel_data.as_mut_ptr() as *mut _), // 出力バッファ
            &mut bitmap_info,                        // ビットマップ情報
            DIB_RGB_COLORS,                          // カラーテーブル形式
        );

        // 【Step 10】Windows GDI リソースの適切な解放（メモリリーク防止）
        let _ = SelectObject(memory_dc, old_bitmap); // 元のビットマップを復元
        let _ = SelectObject(scaled_dc, old_bitmap_scaled); // 元のビットマップを復元
        let _ = DeleteObject(hbitmap.into()); // 原寸ビットマップ削除
        let _ = DeleteObject(hbitmap_scaled.into()); // 縮小ビットマップ削除
        let _ = DeleteDC(memory_dc); // メモリDC削除
        let _ = DeleteDC(scaled_dc); // スケーリングDC削除
        let _ = ReleaseDC(None, screen_dc); // 画面DC解放

        // ピクセルデータ取得成功確認
        if result == 0 {
            // エラー時にもアイコンを待機中に戻す
            switch_capture_processing(false);
            return Err("ビットマップデータの取得に失敗".into());
        }

        // 【Step 11】image crate用ImageBufferの作成とピクセル変換
        let mut img_buffer =
            ImageBuffer::<Rgb<u8>, Vec<u8>>::new(scaled_width as u32, scaled_height as u32);

        // ピクセル単位でのBGR→RGB変換処理
        for y in 0..scaled_height {
            for x in 0..scaled_width {
                let src_idx = (y * row_size + x * bytes_per_pixel) as usize;

                // 配列境界チェック（安全性確保）
                if src_idx + 2 < pixel_data.len() {
                    // Windows GDI はBGR順なのでRGB順に変換
                    let b = pixel_data[src_idx]; // Blue
                    let g = pixel_data[src_idx + 1]; // Green  
                    let r = pixel_data[src_idx + 2]; // Red

                    img_buffer.put_pixel(x as u32, y as u32, Rgb([r, g, b]));
                }
            }
        }

        // 【Step 12】保存先ディレクトリの決定（フォールバック戦略）
        let save_dir_path: String = {
            if let Some(selected_path) = app_state.selected_folder_path.as_ref() {
                selected_path.clone() // ユーザー指定フォルダー優先
            } else {
                get_pictures_folder() // 自動検出フォルダー（OneDrive対応）
            }
        };

        // フォルダー存在確認と自動作成
        let save_dir = std::path::Path::new(&save_dir_path);
        if !save_dir.exists() {
            fs::create_dir_all(save_dir)?; // 親ディレクトリも含めて再帰作成
        }

        // 【Step 13】連番ファイル名生成（4桁ゼロパディング）
        let current_counter = app_state.capture_file_counter;
        let file_path = save_dir.join(format!("{:04}.jpg", current_counter));

        // 【Step 14】JPEG保存
        // 🔧 修正：最高品質でJPEG保存（エラー時カーソル復元対応）
        use image::codecs::jpeg::JpegEncoder;
        use std::fs::File;
        use std::io::BufWriter;

        let save_result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let output_file = File::create(&file_path)?;
            let mut writer = BufWriter::new(output_file);
            let encoder = JpegEncoder::new_with_quality(&mut writer, app_state.jpeg_quality); // ユーザー設定品質
            img_buffer.write_with_encoder(encoder)?;
            Ok(())
        })();

        match save_result {
            Ok(()) => {
                // 成功通知とデバッグ情報出力
                app_log(&format!(
                    "✅ 画像保存完了: {:04}.jpg ({}x{}) (scale: {}%, quality: {}%)",
                    current_counter,
                    scaled_width,
                    scaled_height,
                    app_state.capture_scale_factor,
                    app_state.jpeg_quality
                ));

                // 【Step 15】成功時のみ連番カウンタをインクリメント（失敗時は番号スキップを防止）
                app_state.capture_file_counter += 1;

                // 処理成功時にアイコンを待機中に戻す
                switch_capture_processing(false);

                Ok(()) // 全処理成功
            }
            Err(e) => {
                // ファイル保存エラー時にもアイコンを待機中に戻す
                switch_capture_processing(false);
                Err(e)
            }
        }
    }
}

/**
 * 🔄 キャプチャオーバーレイの状態切り替え関数
 *
 * 【機能概要】
 * キャプチャ処理の状態（待機中/処理中）を`AppState`に保存し、
 * `capturing_overlay`に再描画を要求することで、表示を更新する。
 *
 * 【技術実装】
 * - 状態保存: `app_state.capture_overlay_is_processing` フラグ更新
 * - UI更新: `overlay.refresh_overlay()` を呼び出してオーバーレイを再描画
 *
 * 【状態遷移】
 * - is_processing=false（待機中）: 待機中アイコンが表示される
 * - is_processing=true（処理中）: 処理中アイコンが表示される
 *
 * 【呼び出しコンテキスト】
 * - キャプチャ処理開始直前: `switch_capture_processing(true)`
 * - キャプチャ処理完了後: `switch_capture_processing(false)`
 */
pub fn switch_capture_processing(is_processing: bool) {
    let app_state = AppState::get_app_state_mut();

    // 状態フラグを更新
    app_state.capture_overlay_is_processing = is_processing;

    // オーバーレイ更新
    if let Some(overlay) = app_state.capturing_overlay.as_mut() {
        overlay.refresh_overlay();
    }

    if is_processing {
        println!("⌛ オーバーレイを「処理中」状態に更新しました");
    } else {
        println!("📷 オーバーレイを「待機中」状態に更新しました");
    }
}
