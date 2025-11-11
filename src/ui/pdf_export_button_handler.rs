/*
============================================================================
PDF変換ボタンハンドラモジュール
============================================================================
*/

use windows::Win32::UI::WindowsAndMessaging::*;

use crate::{
    app_state::AppState,
    export_pdf::export_selected_folder_to_pdf,
    system_utils::{app_log, show_message_box},
    ui::input_control_handlers::update_input_control_states,
};

/// PDF変換ボタンのクリックイベントを処理する
///
/// ユーザーに確認ダイアログを表示し、同意が得られた場合にJPEGからPDFへの変換プロセスを開始します。
/// 処理中は、他のUI操作を無効化し、マウスカーソルを砂時計に変更して処理中であることを示します。
///
/// # 処理フロー
/// 1. `show_message_box` でユーザーに実行の意思を確認します。
/// 2. ユーザーが「OK」をクリックした場合:
///    a. `AppState` の `is_exporting_to_pdf` フラグを `true` に設定し、UIコントロールを無効化します。
///    b. マウスカーソルを砂時計（`IDC_WAIT`）に変更します。
///    c. `export_selected_folder_to_pdf` を呼び出して変換処理を実行します。
///    d. 処理完了後、カーソルを元に戻し、`is_exporting_to_pdf` フラグを `false` にしてUIを再度有効化します。
///    e. 処理結果（成功または失敗）をメッセージボックスでユーザーに通知します。
/// 3. ユーザーが「キャンセル」をクリックした場合は、ログを出力して処理を中断します。
pub fn handle_pdf_export_button() -> isize {
    unsafe {
        // 確認ダイアログを表示
        let result = show_message_box(
            "PDF変換を開始してもよろしいでしょうか？\n\n選択されたフォルダー内のJPEG画像を\nPDFファイルに変換します。",
            "PDF変換確認",
            MB_OKCANCEL | MB_ICONQUESTION,
        );

        if result.0 == IDOK.0 {
            app_log("PDF変換を開始します...");

            // カーソルを砂時計に変更
            let wait_cursor = LoadCursorW(None, IDC_WAIT).unwrap_or_default();
            let original_cursor = SetCursor(Some(wait_cursor));

            // PDF変換実行（RAIIパターンでカーソー復元を保証）
            let conversion_result = {
                let app_state = AppState::get_app_state_mut();

                app_state.is_exporting_to_pdf = true;
                update_input_control_states();
                let result = export_selected_folder_to_pdf();
                app_state.is_exporting_to_pdf = false;
                update_input_control_states();
                SetCursor(Some(original_cursor));
                result
            };

            // 結果処理
            match conversion_result {
                Err(e) => {
                    app_log(&format!("PDF変換エラー: {}", e));
                    let error_message = format!("PDF変換中にエラーが発生しました：\n\n{}", e);
                    show_message_box(&error_message, "PDF変換エラー", MB_OK | MB_ICONERROR);
                }
                Ok(_) => {
                    show_message_box(
                        "PDF変換が正常に完了しました。",
                        "PDF変換完了",
                        MB_OK | MB_ICONINFORMATION,
                    );
                }
            }
        } else {
            app_log("PDF変換がキャンセルされました。");
        }
    }
    1
}
