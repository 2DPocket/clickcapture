/*
============================================================================
UIコントロール初期化モジュール (initialize_controls.rs)
============================================================================

【ファイル概要】
メインダイアログのUIコントロール（ボタン、コンボボックス、エディットボックス等）を
初期化するための関数群を提供します。
ダイアログの `WM_INITDIALOG` メッセージハンドラから呼び出され、
各コントロールにデフォルト値を設定し、ユーザーが操作可能な状態にします。

【主要機能】
1.  **アイコンボタンの初期化**: オーナードローボタンにカスタムカーソル（手のひら）を設定。
2.  **パスエディットボックスの初期化**: 最適な保存先フォルダを自動検出し、表示。
3.  **各種コンボボックスの初期化**:
    -   画像スケール (55%～100%)
    -   JPEG品質 (70%～100%)
    -   PDFファイルサイズ (20MB～1GB)
    -   自動クリック間隔 (1秒～5秒)
    各項目に選択肢とデフォルト値を設定し、`AppState` と同期します。
4.  **自動クリック関連コントロールの初期化**: チェックボックス、回数エディットボックスの初期状態を設定。

【AI解析用：依存関係】
- `main.rs`: `WM_INITDIALOG` 内でこのモジュールの各関数を呼び出す。
- `app_state.rs`: 各コントロールの初期値を `AppState` から読み取り、または `AppState` に設定する。
- `folder_manager.rs`: デフォルトの保存先フォルダを取得するために使用。
- `constants.rs`: UIコントロールのID定義。
 */

// 必要なライブラリ（外部機能）をインポート
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM}, // 基本的なデータ型
        Graphics::
            Gdi::*
        , // グラフィック描画機能
        UI::{
            Controls::{
                BST_CHECKED, BST_UNCHECKED, CheckDlgButton,
            },
            Input::KeyboardAndMouse::EnableWindow,
            WindowsAndMessaging::*, // ウィンドウとメッセージ処理
        },
    },
    core::PCWSTR, // Windows API用の文字列操作
};

// アプリケーション状態管理構造体
use crate::app_state::*;

// フォルダ管理機能
use crate::folder_manager::get_pictures_folder;

// 定数群インポート
use crate::constants::*;


/// オーナードローボタンの初期化
///
/// すべてのオーナードローボタン（アイコンボタン）に対して、
/// マウスカーソルが乗ったときに標準の矢印カーソルから手のひらカーソルに
/// 変わるように設定します。これにより、クリック可能であることが視覚的に分かりやすくなります。
///
/// # 引数
/// * `hwnd` - メインダイアログのウィンドウハンドル。
///
/// # 処理内容
/// 1. `LoadCursorW` で `IDC_HAND`（手のひら）カーソルを読み込みます。
/// 2. `GetDlgItem` で各ボタンのハンドルを取得します。
/// 3. `SetClassLongPtrW` を使用して、各ボタンのウィンドウクラスに `GCLP_HCURSOR` を設定します。
pub fn initialize_icon_button(hwnd: HWND) {
    unsafe {
        // 手のひらカーソルを読み込み
        let hand_cursor = LoadCursorW(None, IDC_HAND).unwrap_or_default();

        // 各アイコンボタンにカスタムカーソルを設定
        if let Ok(button) = GetDlgItem(Some(hwnd), IDC_CAPTURE_START_BUTTON) {
            let _ = InvalidateRect(Some(button), None, true);
            let _ = SetClassLongPtrW(button, GET_CLASS_LONG_INDEX(-12), hand_cursor.0 as isize);
        }
        if let Ok(button) = GetDlgItem(Some(hwnd), IDC_AREA_SELECT_BUTTON) {
            let _ = InvalidateRect(Some(button), None, true);
            let _ = SetClassLongPtrW(button, GET_CLASS_LONG_INDEX(-12), hand_cursor.0 as isize);
        }
        if let Ok(button) = GetDlgItem(Some(hwnd), IDC_BROWSE_BUTTON) {
            let _ = InvalidateRect(Some(button), None, true);
            let _ = SetClassLongPtrW(button, GET_CLASS_LONG_INDEX(-12), hand_cursor.0 as isize);
        }
        if let Ok(button) = GetDlgItem(Some(hwnd), IDC_CLOSE_BUTTON) {
            let _ = InvalidateRect(Some(button), None, true);
            let _ = SetClassLongPtrW(button, GET_CLASS_LONG_INDEX(-12), hand_cursor.0 as isize);
        }
        if let Ok(button) = GetDlgItem(Some(hwnd), IDC_EXPORT_PDF_BUTTON) {
            let _ = InvalidateRect(Some(button), None, true);
            let _ = SetClassLongPtrW(button, GET_CLASS_LONG_INDEX(-12), hand_cursor.0 as isize);
        }
    }
}

/// 保存先パスのエディットボックスを初期化
///
/// アプリケーションの初回起動時に、スクリーンショットのデフォルト保存先フォルダを決定し、
/// `AppState` とUI上のエディットボックスに設定します。
///
/// # 引数
/// * `hwnd` - メインダイアログのウィンドウハンドル。
///
/// # 処理内容
/// 1. `folder_manager::get_pictures_folder` を呼び出し、最適な保存先（例: OneDrive/ピクチャ, ローカルのピクチャ）を自動検出します。
/// 2. 検出したパスを `AppState` の `selected_folder_path` に保存します。
/// 3. `SetWindowTextW` を使用して、UIのエディットボックス（`IDC_PATH_EDIT`）にパスを表示します。
pub fn init_path_edit_control(hwnd: HWND) {
    unsafe {
        let app_state = AppState::get_app_state_mut();
        let default_folder = get_pictures_folder();
        app_state.selected_folder_path = Some(default_folder.clone());

        // パステキストボックスに初期値を設定
        if let Ok(path_edit) = GetDlgItem(Some(hwnd), IDC_PATH_EDIT) {
            let default_path = format!("{}\0", default_folder);
            let path_wide: Vec<u16> = default_path.encode_utf16().collect();
            let _ = SetWindowTextW(path_edit, PCWSTR(path_wide.as_ptr()));
        }
    }
}

/// スケールコンボボックスを初期化（100%〜55%、5%刻み）
///
/// キャプチャ画像の縮小率を設定するコンボボックスに、55%から100%までの選択肢を5%刻みで追加します。
/// デフォルト値として、画質とファイルサイズのバランスが良い65%を選択状態にします。
///
/// # 引数
/// * `hwnd` - ダイアログウィンドウハンドル。
///
/// # 処理内容
/// - `CB_ADDSTRING` で表示テキストを追加し、`CB_SETITEMDATA` で実際のスケール値（`u8`）を各項目に関連付けます。
/// - `CB_SETCURSEL` でデフォルトの項目を選択します。`AppState` の `capture_scale_factor` のデフォルト値と一致させます。
pub fn initialize_scale_combo(hwnd: HWND) {
    if let Ok(combo_hwnd) = unsafe { GetDlgItem(Some(hwnd), IDC_SCALE_COMBO) } {
        // 55%から100%まで5%刻みで項目を追加
        let scales: Vec<u8> = (55..=100).step_by(5).collect();

        for &scale in scales.iter().rev() {
            let text = format!("{}%\0", scale);
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
            // 各項目に実際のスケール値をデータとして設定
            unsafe {
                SendMessageW(
                    combo_hwnd,
                    CB_SETITEMDATA,
                    Some(WPARAM(index)),
                    Some(LPARAM(scale as isize)),
                );
            }
        }

        // デフォルト値（65%）を選択
        // 65%は (100-65)/5 = 7番目のインデックス（0ベース）
        let default_index = (100 - 65) / 5;
        unsafe {
            SendMessageW(
                combo_hwnd,
                CB_SETCURSEL,
                Some(WPARAM(default_index as usize)),
                Some(LPARAM(0)),
            );
        }
    }
}

/// JPEG品質コンボボックスを初期化（100%〜70%、5%刻み）
///
/// 保存するJPEG画像の品質を設定するコンボボックスに、70%から100%までの選択肢を5%刻みで追加します。
/// デフォルト値として、高画質を維持できる95%を選択状態にします。
///
/// # 引数
/// * `hwnd` - ダイアログウィンドウハンドル。
///
/// # 処理内容
/// - `CB_ADDSTRING` と `CB_SETITEMDATA` を使用して、表示テキストと実際の品質値（`u8`）を関連付けます。
/// - `CB_SETCURSEL` でデフォルトの項目を選択します。`AppState` の `jpeg_quality` のデフォルト値と一致させます。
pub fn initialize_quality_combo(hwnd: HWND) {
    if let Ok(combo_hwnd) = unsafe { GetDlgItem(Some(hwnd), IDC_QUALITY_COMBO) } {
        // 100%から70%まで5%刻みで項目を追加
        let qualities: Vec<u8> = (70..=100).step_by(5).collect();
        for &quality in qualities.iter().rev() {
            let text = format!("{}%\0", quality);
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
            // 各項目に実際の品質値をデータとして設定
            unsafe {
                SendMessageW(
                    combo_hwnd,
                    CB_SETITEMDATA,
                    Some(WPARAM(index)),
                    Some(LPARAM(quality as isize)),
                );
            }
        }

        // デフォルト値（95%）を選択
        // 95%は (100-95)/5 = 1番目のインデックス（0ベース）
        let default_index = (100 - 95) / 5;
        unsafe {
            SendMessageW(
                combo_hwnd,
                CB_SETCURSEL,
                Some(WPARAM(default_index as usize)),
                Some(LPARAM(0)),
            );
        }
    }
}

/// PDFサイズコンボボックスを初期化（20MB〜100MB、20MB刻み）
///
/// # 引数
/// * `hwnd` - ダイアログウィンドウハンドル
///
/// # 機能
/// 1. コンボボックスに選択肢（20, 40, 60, 80, 100）と「最大(1GB)」を追加
/// 2. デフォルト値（20MB）を選択状態に設定
/// 3. AppStateのpdf_max_size_mbと同期
const PDF_FILE_MIN_SIZE_MB: u16 = 20;
const PDF_FILE_MAX_SIZE_MB: u16 = 100;
const PDF_FILE_SIZE_STEP_MB: u16 = 20;
pub fn initialize_pdf_size_combo(hwnd: HWND) {
    if let Ok(combo_hwnd) = unsafe { GetDlgItem(Some(hwnd), IDC_PDF_SIZE_COMBO) } {
        // 20MBから100MBまで20MB刻みで項目を追加
        for &size_mb in (PDF_FILE_MIN_SIZE_MB..=PDF_FILE_MAX_SIZE_MB)
            .step_by(PDF_FILE_SIZE_STEP_MB as usize)
            .collect::<Vec<u16>>()
            .iter()
        {
            let text = format!("{}MB\0", size_mb);
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
                    Some(LPARAM(size_mb as isize)),
                );
            }
        }

        // 無制限オプションを追加
        let unlimited_text = "最大(1GB)\0";
        let unlimited_wide: Vec<u16> = unlimited_text.encode_utf16().collect();
        let index = unsafe {
            SendMessageW(
                combo_hwnd,
                CB_ADDSTRING,
                Some(WPARAM(0)),
                Some(LPARAM(unlimited_wide.as_ptr() as isize)),
            )
        }
        .0 as usize;
        // 1GBをMB単位で設定
        unsafe {
            SendMessageW(
                combo_hwnd,
                CB_SETITEMDATA,
                Some(WPARAM(index)),
                Some(LPARAM(1024)),
            );
        }

        // デフォルト値（20MB）を選択
        // 20MBは最初の項目（インデックス0）
        unsafe {
            SendMessageW(combo_hwnd, CB_SETCURSEL, Some(WPARAM(0)), Some(LPARAM(0)));
        }
    }
}

/*
============================================================================
自動クリック機能関連コントロール初期化
============================================================================
*/

/// 連続クリックチェックボックスを初期化
///
/// 自動連続クリック機能の有効/無効を切り替えるチェックボックスを初期化します。
/// `AppState` のデフォルト状態（通常は無効）に合わせて、チェックボックスの初期状態を設定し、
/// 関連するコントロール（間隔コンボボックス、回数エディットボックス）の有効/無効状態も同期させます。
///
/// # 引数
/// * `hwnd` - ダイアログウィンドウハンドル。
pub fn initialize_auto_click_checkbox(hwnd: HWND) {
    unsafe {
        let app_state = AppState::get_app_state_ref();
        let is_checked = app_state.auto_clicker.is_enabled();
        let _ = CheckDlgButton(
            hwnd,
            IDC_AUTO_CLICK_CHECKBOX,
            if is_checked {
                BST_CHECKED
            } else {
                BST_UNCHECKED
            },
        );

        // 関連コントロールの有効/無効を初期状態で設定
        if let Ok(interval_combo) = GetDlgItem(Some(hwnd), IDC_AUTO_CLICK_INTERVAL_COMBO) {
            let _ = EnableWindow(interval_combo, is_checked);
        }
        if let Ok(count_edit) = GetDlgItem(Some(hwnd), IDC_AUTO_CLICK_COUNT_EDIT) {
            let _ = EnableWindow(count_edit, is_checked);
        }
    }
}

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
