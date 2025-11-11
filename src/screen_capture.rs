/*
============================================================================
画面キャプチャ機能モジュール (screen_capture.rs)
============================================================================

【ファイル概要】
画面キャプチャ機能の中核を担うモジュールです。選択された領域のキャプチャ、
JPEG画像としての保存、連番ファイル名の生成、キャプチャモードの制御、
そして自動連続クリック機能との連携を提供します。

【主要機能】
1.  **キャプチャモード制御 (`toggle_capture_mode`)**:
    -   キャプチャモードの開始と終了を切り替え、関連リソース（フック、オーバーレイ）を管理します。
2.  **画面領域キャプチャと保存 (`capture_screen_area_with_counter`)**:
    -   `BitBlt` APIを使用して指定領域のピクセルデータを高速に取得します。
    -   取得したデータをユーザー設定のスケールと品質でJPEG画像としてエンコードし、保存します。
3.  **連番ファイル名生成**:
    -   保存するファイル名を `0001.jpg`, `0002.jpg` のように自動でインクリメントします。
4.  **自動クリック連携**:
    -   自動クリックモードが有効な場合、最初のクリックをトリガーに `auto_clicker` を起動し、連続キャプチャを実行します。

【技術仕様】
-   **画面取得**: `GetDC` + `BitBlt` による高速なピクセルデータ取得。
-   **画像処理**: `image` クレートによるJPEGエンコード。`StretchBlt` と `HALFTONE` モードによる高品質な画像縮小。
-   **ファイルI/O**: `std::fs` と `std::io::BufWriter` による効率的なファイル書き込み。
-   **オーバーレイ**: `capturing_overlay` を使用して、キャプチャ待機中や処理中の状態をユーザーにフィードバック。

【処理フロー】
1.  **[UI]** 「キャプチャ開始」ボタンクリック
2.  **`toggle_capture_mode()`**:
    -   エリアが選択済みか、自動クリック設定が妥当かなどを検証します。
    -   検証OKならモードを開始し、フックをインストールして `capturing_overlay` を表示します。
3.  **[マウスフック]** ユーザーが画面を左クリック
4.  **`low_level_mouse_proc` (in `mouse.rs`)**:
    -   **自動クリック有効時**: `auto_clicker.start()` を呼び出します。`auto_clicker` は内部ループで `perform_mouse_click` を実行し、それが再度このマウスフックに捕捉され、結果的に `capture_screen_area_with_counter` が繰り返し呼ばれます。
    -   **自動クリック無効時**: `capture_screen_area_with_counter()` を一度だけ呼び出します。
5.  **`capture_screen_area_with_counter()`**:
    -   `BitBlt` で画面をキャプチャし、`StretchBlt` でリサイズします。
    -   `image` クレートでJPEGにエンコードし、連番ファイル名で保存します。
6.  **モード終了**:
    -   ESCキー押下、または「キャプチャ開始」ボタンの再クリックで `toggle_capture_mode()` が呼ばれ、フックとオーバーレイを解放します。
    -   自動クリック完了時も `WM_AUTO_CLICK_COMPLETE` を経由して `toggle_capture_mode()` が呼ばれます。

============================================================================
*/

use windows::Win32::UI::WindowsAndMessaging::{
    IDOK, MB_ICONQUESTION, MB_ICONWARNING, MB_OK, MB_OKCANCEL,
};
// 必要なライブラリ（外部機能）をインポート
use windows::Win32::{
    Graphics::Gdi::*, // グラフィック描画機能
};
// 画像処理ライブラリ（JPEGキャプチャ保存専用）
use image::{ImageBuffer, Rgb};

use std::fs;

use crate::{
    app_state::*,
    hook::*,
    overlay::Overlay,
    system_utils::*,
    ui::{
        dialog_handler::{bring_dialog_to_back, bring_dialog_to_front},
        folder_manager::*,
        input_control_handlers::update_input_control_states,
    },
};

/**
 * キャプチャモードの開始/終了を切り替える
 *
 * この関数は、キャプチャモードを開始または終了するためのトグルとして機能します。
 * モード開始前には、キャプチャエリアが選択されているか、自動クリック設定が
 * 妥当かなどの前提条件を検証します。
 *
 * # 状態遷移
 * - **OFF -> ON**:
 *   1. 前提条件（エリア選択、自動クリック設定）を検証します。
 *   2. 検証に失敗した場合、エラーメッセージを表示して中断します。
 *   3. `AppState` の `is_capture_mode` を `true` に設定します。
 *   4. マウスとキーボードのフックをインストールし、`capturing_overlay` を表示します。
 *   5. メインダイアログを最小化します。
 *
 * - **ON -> OFF**:
 *   1. `AppState` の `is_capture_mode` を `false` に設定します。
 *   2. フックをアンインストールし、`capturing_overlay` を非表示にします。
 *   3. 実行中の自動クリック処理があれば停止させます。
 *   4. メインダイアログを復元し、最前面に表示します。
 *
 * どちらの場合でも、最後に `update_input_control_states` を呼び出してUIの状態を更新します。
 */
pub fn toggle_capture_mode() {
    let app_state = AppState::get_app_state_mut();
    let is_capture_mode = app_state.is_capture_mode;

    if is_capture_mode {
        // キャプチャモードを終了する
        app_state.is_capture_mode = false;

        // キーボードとマウスフック停止
        uninstall_hooks();

        // キャプチャモードオーバーレイを非表示
        if let Some(overlay) = app_state.capturing_overlay.as_mut() {
            overlay.hide_overlay();
        }

        // メインダイアログを最前面に表示
        bring_dialog_to_front();

        // 実行中の自動クリック処理があれば停止させる
        if app_state.auto_clicker.is_running() {
            app_state.auto_clicker.stop();
        }
        app_log("画面キャプチャモードを終了しました");
    } else {
        // キャプチャモードを開始する（開始前に前提条件をチェック）
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
            return;
        }

        // 回数の値が0の場合、自動クリック機能を無効化
        if app_state.auto_clicker.is_enabled() && app_state.auto_clicker.get_max_count() == 0 {
            show_message_box(
                "回数の値が0、もしくは未設定です。1以上の値を設定してください。",
                "自動クリックエラー",
                MB_OK | MB_ICONWARNING,
            );
            return;
        }

        // 確認ダイアログを表示
        if app_state.auto_clicker.is_enabled() {
            let result = show_message_box(
                "自動クリックモードでキャプチャを開始します。\n\n\
                【開始方法】\n\
                キャプチャしたい場所（例：「次へ」ボタン）を1回クリックしてください。\n\n\
                【動作】\n\
                設定された回数・間隔で、同じ場所へのクリックとキャプチャを自動で繰り返します。\n\n\
                【停止方法】\n\
                いつでも ESC キーで中断できます。",
                "自動クリックモードの開始確認",
                MB_OKCANCEL | MB_ICONQUESTION,
            );

            if result.0 != IDOK.0 {
                app_log("自動クリックモードがキャンセルされました。");
                return;
            }
        }

        // 前提条件をクリアしたので、モードを開始
        app_state.is_capture_mode = true;

        // キーボードとマウスフック開始
        install_hooks();

        // キャプチャモードオーバーレイを表示
        if let Some(overlay) = app_state.capturing_overlay.as_mut() {
            if let Err(e) = overlay.show_overlay() {
                eprintln!("❌ キャプチャモードオーバーレイの表示に失敗: {:?}", e);
                // TODO: エラー時はモードを開始せずに終了するべき
            }
        }

        // メインダイアログを最背面に表示
        bring_dialog_to_back();

        app_log("画面キャプチャモードを開始しました (エスケープキーでキャプチャ終了)");
    };
    // UIコントロールの状態を更新
    update_input_control_states();
}

/**
 * 選択された画面領域をキャプチャし、連番ファイル名でJPEGとして保存する
 *
 * # パフォーマンス最適化
 * - `BitBlt` APIによる高速なピクセルデータコピー。
 * - `StretchBlt` APIと `HALFTONE` モードによる高品質な画像縮小。
 * - メモリDC（オフスクリーンバッファ）を使用し、GPUアクセラレーションを活用。
 *
 * 【戻り値】
 * * `Ok(())` - 成功した場合。
 * * `Err(Box<dyn std::error::Error>)` - 失敗した場合、エラー情報。
 *
 * 【処理フロー】
 * 1. `AppState` から選択領域 (`selected_area`) を取得します。
 * 2. `GetDC` で画面全体のデバイスコンテキストを取得し、`CreateCompatibleDC` でメモリDCを作成します。
 * 3. `BitBlt` を使用して、画面の指定領域をメモリ上のビットマップにコピーします。
 * 4. `StretchBlt` を使用して、ユーザー設定のスケールに合わせて画像をリサイズします。
 * 5. `GetDIBits` でリサイズされたビットマップからピクセルデータを抽出します。
 * 6. 抽出したBGR形式のピクセルデータをRGB形式に変換し、`image` クレートの `ImageBuffer` に格納します。
 * 7. `JpegEncoder` を使用して、ユーザー設定の品質でJPEGにエンコードし、連番ファイル名で保存します。
 * 8. 使用したGDIリソースを全て解放します。
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

        // キャプチャ処理開始時にオーバーレイアイコンを「処理中」に切り替え
        set_capture_overlay_processing_state(true);

        // デバイスコンテキストの準備
        let screen_dc = GetDC(None);
        let memory_dc = CreateCompatibleDC(Some(screen_dc));

        // キャプチャ領域のサイズ計算
        let width = (right - left).abs();
        let height = (bottom - top).abs();

        // ユーザー設定のスケール値に基づいて、リサイズ後のサイズを計算
        let scale_factor = (app_state.capture_scale_factor as f32) / 100.0;
        let scaled_width = ((width as f32) * scale_factor) as i32;
        let scaled_height = ((height as f32) * scale_factor) as i32;

        // 原寸サイズのビットマップを作成し、画面の指定領域をコピー
        let hbitmap = CreateCompatibleBitmap(screen_dc, width, height);
        let old_bitmap = SelectObject(memory_dc, hbitmap.into());

        // キャプチャの瞬間だけオーバーレイを非表示にし、BitBltを実行後、再表示する
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

            if let Err(e) = overlay.show_overlay() {
                return Err(format!("❌ キャプチャアイコンの再表示に失敗: {}", e).into());
            }
        }

        // スケーリング用のデバイスコンテキストとビットマップを準備
        let scaled_dc = CreateCompatibleDC(Some(screen_dc));
        let hbitmap_scaled = CreateCompatibleBitmap(screen_dc, scaled_width, scaled_height);
        let old_bitmap_scaled = SelectObject(scaled_dc, hbitmap_scaled.into());

        // 高品質な縮小処理を行うためにHALFTONEモードを設定
        let _ = SetStretchBltMode(scaled_dc, HALFTONE);
        let _ = SetBrushOrgEx(scaled_dc, 0, 0, None);

        // `StretchBlt` を使用して、原寸ビットマップを縮小ビットマップにコピー
        let _ = StretchBlt(
            scaled_dc,
            0,
            0,
            scaled_width,
            scaled_height,
            Some(memory_dc),
            0,
            0,
            width,
            height,  // 縮小元サイズ
            SRCCOPY, // 転送モード
        );

        // ピクセルデータ抽出の準備
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

        // `GetDIBits` を使用して、縮小ビットマップからピクセルデータを抽出
        let result = GetDIBits(
            scaled_dc,                               // ソースDC
            hbitmap_scaled,                          // ソースビットマップ
            0,                                       // 開始スキャンライン
            scaled_height as u32,                    // スキャンライン数
            Some(pixel_data.as_mut_ptr() as *mut _), // 出力バッファ
            &mut bitmap_info,                        // ビットマップ情報
            DIB_RGB_COLORS,                          // カラーテーブル形式
        );

        // Windows GDIリソースを解放
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
            set_capture_overlay_processing_state(false);
            return Err("ビットマップデータの取得に失敗".into());
        }

        // `image` クレート用の `ImageBuffer` を作成し、ピクセルデータを変換
        let mut img_buffer =
            ImageBuffer::<Rgb<u8>, Vec<u8>>::new(scaled_width as u32, scaled_height as u32);

        // Windows GDIのBGR形式から、標準的なRGB形式にピクセル単位で変換
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

        // 保存先ディレクトリを決定
        let save_dir_path: String = {
            if let Some(selected_path) = app_state.selected_folder_path.as_ref() {
                selected_path.clone() // ユーザー指定フォルダー優先
            } else {
                get_pictures_folder() // 自動検出フォルダー（OneDrive対応）
            }
        };

        // フォルダが存在しない場合は作成
        let save_dir = std::path::Path::new(&save_dir_path);
        if !save_dir.exists() {
            fs::create_dir_all(save_dir)?; // 親ディレクトリも含めて再帰作成
        }

        // 連番ファイル名を生成（4桁ゼロパディング）
        let current_counter = app_state.capture_file_counter;
        let file_path = save_dir.join(format!("{:04}.jpg", current_counter));

        // JPEGとして保存
        use image::codecs::jpeg::JpegEncoder;
        use std::fs::File;
        use std::io::BufWriter;

        let save_result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let output_file = File::create(&file_path)?;
            let mut writer = BufWriter::new(output_file);
            let encoder = JpegEncoder::new_with_quality(&mut writer, app_state.jpeg_quality);
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

                // 成功時のみ連番カウンタをインクリメント
                app_state.capture_file_counter += 1;

                // 処理成功時にアイコンを待機中に戻す
                set_capture_overlay_processing_state(false);

                Ok(()) // 全処理成功
            }
            Err(e) => {
                // ファイル保存エラー時にもアイコンを待機中に戻す
                set_capture_overlay_processing_state(false);
                Err(e)
            }
        }
    }
}

/**
 * キャプチャオーバーレイの表示状態（待機中/処理中）を切り替える
 *
 * `AppState` のフラグを更新し、`capturing_overlay` に再描画を要求することで、
 * マウスカーソルに追従するアイコンの表示を「待機中」と「処理中」で切り替えます。
 *
 * # 引数
 * * `is_processing` - `true` であれば「処理中」アイコン、`false` であれば「待機中」アイコンを表示します。
 *
 * # 呼び出し箇所
 * - `capture_screen_area_with_counter` の処理開始時に `true` で呼び出されます。
 * - `capture_screen_area_with_counter` の処理終了時に `false` で呼び出されます。
 */
pub fn set_capture_overlay_processing_state(is_processing: bool) {
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
