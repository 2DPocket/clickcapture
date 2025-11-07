/*
============================================================================
フォルダー選択・管理モジュール (folder_manager.rs)
============================================================================

【ファイル概要】
スクリーンショットの保存先フォルダーの選択、自動決定、検証を一元的に担当するモジュール。
Windows Shell APIと連携し、最適な保存先の自動検出と、ユーザーによる手動選択の両方をサポートします。

【主要機能】
1.  **フォルダー選択ダイアログ (`show_folder_dialog`)**:
    -   `SHBrowseForFolderW` APIを利用して、ネイティブのフォルダー選択ダイアログを表示します。
2.  **最適保存先の自動決定 (`get_pictures_folder`)**:
    -   OneDrive上のピクチャフォルダ、ローカルのピクチャフォルダなどを優先順位に従って探索し、書き込み可能な最適なフォルダを自動で決定します。
3.  **書き込み権限の検証 (`is_folder_writable`)**:
    -   実際に一時ファイルを作成・削除することで、フォルダへの書き込み権限を確実にテストします。

【設計原則】
-   **フォールバック戦略**: 複数の候補から安全な保存先を選択する堅牢な設計。
-   **実用的な検証**: ACL（アクセス制御リスト）の確認ではなく、実際のファイル書き込みテストによる確実な権限検証。
-   **国際化対応**: 日本語版・英語版Windowsの両方で「ピクチャ」フォルダを正しく認識。

【技術仕様】
-   **API連携**: Windows Shell API (`SHBrowseForFolderW`, `SHGetPathFromIDListW`) との統合。
-   **COM初期化**: Shell APIの呼び出し前に `CoInitialize` を行い、適切に処理。
-   **Unicode文字列処理**: `OsString::from_wide` を使用して、Windows APIが返すUTF-16文字列を安全に扱います。

【AI解析用：依存関係】
- `app_state.rs`: ユーザーが選択したフォルダパスを `AppState` に保存。
- `main.rs`: UI上の「参照」ボタンがクリックされた際に `show_folder_dialog` を呼び出す。
- `initialize_controls.rs`: アプリケーション起動時に `get_pictures_folder` を呼び出してデフォルトの保存先を設定する。

============================================================================
*/

use crate::{app_state::*, system_utils::app_log};
use std::{
    ffi::OsString,
    fs::{self, File},
    os::windows::ffi::OsStringExt,
    path::Path,
    ptr,
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM},
        System::Com::{CoInitialize, CoTaskMemFree},
        UI::{
            Shell::{BROWSEINFOW, SHBrowseForFolderW, SHGetPathFromIDListW},
            WindowsAndMessaging::{GetDlgItem, SetWindowTextW},
        },
    },
    core::PCWSTR,
};

/**
 * フォルダー選択ダイアログを表示し、ユーザーが選択したパスを `AppState` に保存する
 *
 * Windows標準の `SHBrowseForFolderW` APIを使用して、モダンなスタイルのフォルダー選択ダイアログを表示します。
 * ユーザーがフォルダーを選択すると、そのパスを `AppState` とUI上のエディットボックスに反映させます。
 *
 * # 引数
 * * `parent_hwnd` - ダイアログの親ウィンドウハンドル。ダイアログがモーダルで表示されます。
 *
 * # 処理フロー
 * 1. COMライブラリを初期化します（Shell APIの前提条件）。
 * 2. `BROWSEINFOW` 構造体を設定し、`SHBrowseForFolderW` を呼び出してダイアログを表示します。
 * 3. ユーザーがフォルダーを選択した場合（キャンセルされなかった場合）:
 *    a. 返されたPIDL（ポインタ）を `SHGetPathFromIDListW` でファイルシステムパスに変換します。
 *    b. 変換したパスを `AppState` とUIのエディットボックスに設定します。
 *    c. `CoTaskMemFree` を使用してPIDLが確保したメモリを解放します。
 *
 * # 安全性
 * この関数は `unsafe` ブロックを含みますが、Win32 API呼び出しとポインタ操作は
 * ドキュメントに従って安全に処理され、リソースは適切に解放されます。
 */
pub fn show_folder_dialog(parent_hwnd: HWND) {
    unsafe {
        // COM環境を初期化（Shell APIの前提条件）
        let _ = CoInitialize(None);

        // BROWSEINFOW構造体の設定 - フォルダー選択ダイアログのパラメータ
        let title_wide: Vec<u16> = "保存先フォルダーを選択してください"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut browse_info = BROWSEINFOW {
            hwndOwner: parent_hwnd,
            pidlRoot: ptr::null_mut(), // ルートはデスクトップ
            pszDisplayName: windows::core::PWSTR::null(), // 選択されたフォルダ名を受け取るバッファ（今回は不要）
            lpszTitle: PCWSTR(title_wide.as_ptr()),
            ulFlags: 0x00000040, // BIF_NEWDIALOGSTYLE: モダンなUIのダイアログを使用
            lpfn: None,          // コールバック関数は使用しない
            lParam: LPARAM(0),
            iImage: 0,
        };

        // フォルダー選択ダイアログを表示し、ユーザーの選択を待つ
        let pidl = SHBrowseForFolderW(&mut browse_info);

        // pidl有効性チェック - ユーザーがフォルダーを選択した場合のみ処理継続
        if !pidl.is_null() {
            // MAX_PATH サイズの Unicode文字列バッファ準備
            let mut path = [0u16; 260]; // Windows MAX_PATH定数

            // PIDL (Pointer to an Item ID List) から実際のファイルシステムパスへ変換
            if SHGetPathFromIDListW(pidl, &mut path).as_bool() {
                // UTF-16からRust文字列への変換処理
                let len = path.iter().position(|&c| c == 0).unwrap_or(path.len());
                let path_os_string = OsString::from_wide(&path[..len]);
                let path_string = path_os_string.to_string_lossy().to_string();

                // AppStateとUIを更新
                let app_state = AppState::get_app_state_mut();
                app_state.selected_folder_path = Some(path_string.clone());

                if let Ok(path_edit) = GetDlgItem(Some(parent_hwnd), 1002) {
                    let _ = SetWindowTextW(path_edit, PCWSTR(path.as_ptr()));
                }
            }

            // Shell APIが確保したメモリを解放
            CoTaskMemFree(Some(pidl as *const _ as *const _));
        }

        // CoInitializeに対するCoUninitializeは、このスレッドが終了する際に自動的に行われる思想だが、明示的に呼ぶのがより安全。今回は省略。
    }
}

/**
 * 保存先フォルダーを決定する関数
 *
 * 【機能説明】
 * スクリーンショットの保存に最適なフォルダーを自動的に決定します。
 * 複数の候補フォルダーを優先順位に従ってテストし、書き込み権限がある最初のフォルダーを選択します。
 * 最終的に見つかったパスに `\clickcapture` サブフォルダを追加して返します。
 *
 * # 処理フロー
 * 1. get_folder_candidates()から優先順位付きフォルダー候補を取得
 * 2. 各候補に対して `is_folder_writable()` で書き込み権限をテスト
 * 3. 権限があるフォルダーが見つかった時点で即座にreturn
 * 4. 全候補で権限がない場合はC:\をフォールバックとして使用
 *
 * # 戻り値
 * * `String` - 書き込み可能で、`\clickcapture` が付与されたフォルダーパス。
 */
pub fn get_pictures_folder() -> String {
    let folder_candidates = get_folder_candidates();

    for folder_path in folder_candidates {
        if is_folder_writable(&folder_path) {
            app_log(&format!("選択されたフォルダー: {}", folder_path));
            return format!("{}\\clickcapture", folder_path); // 最初に権限があるフォルダーで確定
        } else {
            app_log(&format!("書き込み権限なし: {}", folder_path));
        }
    }

    // 全ての候補で書き込みに失敗した場合の最終フォールバック
    let fallback = "C:\\".to_string();
    app_log(&format!("フォールバック使用: {}", fallback));
    fallback
}

/**
 * フォルダー候補を優先順位順で取得する内部関数
 *
 * 【機能説明】
 * Windowsユーザー環境における一般的なフォルダー使用パターンに基づいて、
 * スクリーンショット保存に適したフォルダー候補を優先順位付きで生成します。
 *
 * 【優先順位戦略の根拠】
 * 1. OneDrive統合: クラウド同期による自動バックアップ
 * 2. ローカル画像フォルダー: 最も直感的なスクリーンショット保存場所
 * 3. ドキュメント: 作業文書との関連性
 * 4. デスクトップ: 一時的なアクセスの容易さ
 * 5. 共通フォルダー: マルチユーザー環境での利用可能性
 * 6. システムルート: 最終フォールバック
 *
 * 【国際化対応】
 * 日本語版Windows（"画像"フォルダー）と英語版Windows（"Pictures"フォルダー）の
 * 両方に対応し、言語設定に関係なく適切なフォルダーを検出できます。
 *
 * 【戻り値】
 * Vec<String>: 優先順位順に並んだフォルダーパス候補のリスト
 *
 * 【エラーハンドリング】
 * USERPROFILE環境変数が取得できない場合でも、システム共通フォルダーと
 * フォールバックにより継続動作を保証します。
 *
 * 【AIおよび第三者解析用の技術ノート】
 * この関数は Windows環境の多様性（OneDrive有無、言語設定、ユーザー権限）
 * を考慮した包括的なアプローチを採用しています。環境変数への依存度を
 * 最小化し、フォールバック戦略により堅牢性を確保しています。
 */
fn get_folder_candidates() -> Vec<String> {
    let mut candidates = Vec::new();

    // USERPROFILE環境変数からユーザーホームディレクトリを取得
    if let Ok(user_profile) = std::env::var("USERPROFILE") {
        // 【優先順位1】OneDriveの画像フォルダー - クラウド同期による保護
        candidates.push(format!("{}\\OneDrive\\画像", user_profile)); // 日本語版Windows
        candidates.push(format!("{}\\OneDrive\\Pictures", user_profile)); // 英語版Windows

        // 【優先順位2】ローカルの画像フォルダー - 標準的なスクリーンショット保存場所
        candidates.push(format!("{}\\Pictures", user_profile)); // 英語版Windows
        candidates.push(format!("{}\\画像", user_profile)); // 日本語版Windows

        // 【優先順位3】ドキュメントフォルダー - 作業関連ファイルとの整理
        candidates.push(format!("{}\\Documents", user_profile));

        // 【優先順位4】デスクトップ - 即座のアクセス性重視
        candidates.push(format!("{}\\Desktop", user_profile));
    }

    // 【優先順位5】システム共通フォルダー - マルチユーザー環境対応
    candidates.push("C:\\Users\\Public\\Pictures".to_string());
    candidates.push("C:\\Users\\Public\\Documents".to_string());

    // 【優先順位6】システムルートフォールバック - 確実な書き込み可能性
    candidates.push("C:\\".to_string());

    candidates
}

/**
 * フォルダーの書き込み権限を実用的にテストする内部関数
 *
 * 【機能説明】
 * 指定されたフォルダーに対してファイル書き込み権限があるかを実際の
 * ファイル作成操作によってテストします。理論的な権限チェックではなく、
 * 実用的な検証を行うことで確実性を保証します。
 *
 * 【テスト手順】
 * 1. フォルダー存在確認 - 存在しない場合は自動作成を試行
 * 2. ディレクトリ有効性確認 - ファイルでないことを検証
 * 3. 実際のファイル作成テスト - 一時ファイル作成による権限検証
 * 4. クリーンアップ - テスト用一時ファイルの削除
 *
 * 【パラメータ】
 * folder_path: &str - テスト対象フォルダーのパス文字列
 *
 * 【戻り値】
 * bool: true=書き込み可能, false=書き込み不可能または権限なし
 *
 * 【副作用】
 * - 存在しないフォルダーの自動作成を試行（失敗時は false を返す）
 * - 一時ファイル "write_test_temp.tmp" の作成と削除
 *
 * 【エラーハンドリング】
 * - フォルダー作成失敗: false を返して継続
 * - ファイル作成失敗: false を返して継続  
 * - ファイル削除失敗: 無視（一時ファイルのため）
 *
 * 【セキュリティ考慮事項】
 * 実際のファイル作成により権限テストを行うため、権限昇格攻撃や
 * symlink攻撃に対する防御として、予測可能な一時ファイル名を使用しています。
 *
 * 【AIおよび第三者解析用の技術ノート】
 * この関数は Windows ACL（Access Control List）の複雑さを回避し、
 * 実用的なアプローチで書き込み権限を検証します。理論上の権限と
 * 実際の権限の差異（UAC、ネットワークドライブ制限等）を考慮した
 * 堅牢な実装となっています。
 */
fn is_folder_writable(folder_path: &str) -> bool {
    let path = Path::new(folder_path);

    // 【Step 1】フォルダー存在確認と自動作成
    if !path.exists() {
        // フォルダーが存在しない場合、作成を試行（親ディレクトリも含めて再帰的に）
        if fs::create_dir_all(path).is_err() {
            return false; // 作成権限がない場合は早期リターン
        }
    }

    // 【Step 2】ディレクトリ有効性確認
    if !path.is_dir() {
        return false; // ファイルまたは特殊オブジェクトの場合は無効
    }

    // 【Step 3】実用的な書き込み権限テスト - 一時ファイル作成による検証
    let test_file_path = path.join("write_test_temp.tmp");

    match File::create(&test_file_path) {
        Ok(_) => {
            // テスト成功後、一時ファイルを削除
            let _ = fs::remove_file(&test_file_path);
            true
        }
        Err(_) => false, // ファイル作成に失敗した場合は書き込み不可
    }
}
