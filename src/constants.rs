/*
============================================================================
アプリケーション定数定義モジュール (constants.rs)
============================================================================

【ファイル概要】
アプリケーション全体で使用する定数を一元管理するモジュール。
Windows APIリソースID、UIコントロールID、アイコンリソースIDを定義し、
RustコードとWindowsリソースファイル（dialog.rc）間の整合性を保証する。

【重要な制約】
src/resource.h との完全同期が必須
リソースファイル(dialog.rc)とRustコード間でID値を手動で同期管理
ID値の変更時は以下のファイルを同時更新：
1. src/constants.rs (このファイル)
2. src/resource.h (Cヘッダーファイル)
3. src/dialog.rc (Windowsリソースファイル)

【定数カテゴリ】
1. ダイアログID：メインダイアログウィンドウの識別子
2. UIコントロールID：ボタン、エディットボックス、コンボボックス等の識別子
3. アイコンリソースID：埋め込みアイコンファイルの識別子
4. RCオーバーレイID：キャプチャモードオーバーレイ関連の識別子

【ID値の範囲設計】
- ダイアログID：100番台（101〜）
- UIコントロールID：1000番台（1001〜）
- アイコンリソースID：2000番台（2001〜）
- RCオーバーレイID：2010番台（2011〜2013）

【RCベース統合】
- IDD_CAPTURE_OVERLAY: キャプチャモードオーバーレイダイアログ
- IDC_ICON_MODE: 待機中アイコンコントロール
- IDC_ICON_PROCESSING: 処理中アイコンコントロール
- capture_overlay.rc: レイアウト定義ファイル

【技術仕様】
- 型：i32/u16（Windows API仕様に準拠）
- スコープ：pub const（モジュール外からアクセス可能）
- 命名規則：Windows標準プレフィックス（IDD_, IDC_, IDI_）

【AI解析用：依存関係】
constants.rs → main.rs (UI処理でID参照)
constants.rs → ui_controls.rs (ボタン描画でアイコンID参照)
constants.rs → screen_capture.rs (モード切替でボタンID参照)
constants.rs → captureing_overlay.rs (RCベースオーバーレイID参照)
constants.rs ↔ resource.h (ID値同期)
constants.rs ↔ dialog.rc (リソース定義)
constants.rs ↔ capture_overlay.rc (オーバーレイレイアウト)

【メンテナンス指針】
- ID追加時：連番で新しい番号を割り当て
- ID削除時：欠番は埋めず、将来の拡張のため保持
- ID変更時：全関連ファイルの同時更新を確実に実行
- コンフリクト回避：各カテゴリの番号範囲を厳密に分離

============================================================================
*/

/*
============================================================================
定数定義セクション
============================================================================
*/

// ===== ダイアログリソースID =====
// メインアプリケーションダイアログの識別子
// DialogBoxParamW()でリソースを読み込む際に使用
pub const IDD_DIALOG1: u16 = 101;

// ===== UIコントロール識別子 =====
// GetDlgItem()、WM_COMMANDメッセージ処理で使用される一意ID
//
// フォルダー参照ボタン：保存先フォルダー選択ダイアログを開く
pub const IDC_BROWSE_BUTTON: i32 = 1001;
// パス表示エディットボックス：選択された保存先フォルダーパスを表示
pub const IDC_PATH_EDIT: i32 = 1002;
// エリア選択ボタン：マウスドラッグによる矩形領域選択モードを開始
pub const IDC_AREA_SELECT_BUTTON: i32 = 1005;
// キャプチャ開始ボタン：左クリック画面保存モードを開始
pub const IDC_CAPTURE_START_BUTTON: i32 = 1006;
// アプリケーション終了ボタン：全リソースを解放してプログラム終了
pub const IDC_CLOSE_BUTTON: i32 = 1007;
// PFG変換ボタン：JPEGをPDFファイルに変換する
pub const IDC_EXPORT_PDF_BUTTON: i32 = 1008;
// スケール調整コンボボックス：画像サイズ調整率選択（100%〜50%、5%刻み）
pub const IDC_SCALE_COMBO: i32 = 1009;
// JPEG品質コンボボックス：画像品質選択（100%〜70%、5%刻み）
pub const IDC_QUALITY_COMBO: i32 = 1010;
// PDFサイズ上限コンボボックス：PDFファイル最大サイズ選択（500MB〜1000MB、100MB刻み）
pub const IDC_PDF_SIZE_COMBO: i32 = 1011;
// ログ表示エディットボックス：システムメッセージとステータス情報を表示（1行、読み取り専用）
pub const IDC_LOG_EDIT: i32 = 1012;
// 連続クリック有効化チェックボックス：キャプチャ中の自動クリックを有効/無効にする
pub const IDC_AUTO_CLICK_CHECKBOX: i32 = 1013;
// 連続クリック間隔コンボボックス：自動クリックの間隔を選択（1秒〜10秒、1秒刻み）
pub const IDC_AUTO_CLICK_INTERVAL_COMBO: i32 = 1014;
// 連続クリック回数エディットボックス：自動クリックの回数を指定
pub const IDC_AUTO_CLICK_COUNT_EDIT: i32 = 1015;

// ===== アイコンリソース識別子 =====
// LoadIconW()で.icoファイルを読み込む際の識別子
// オーナードローボタンでアイコン表示に使用
//
// キャプチャモード非アクティブ時のカメラアイコン
pub const IDI_CAMERA_OFF: i32 = 2001;
// キャプチャモードアクティブ時のカメラアイコン（録画中表示）
pub const IDI_CAMERA_ON: i32 = 2002;
// エリア選択モードアクティブ時の選択アイコン
pub const IDI_SELECT_AREA_ON: i32 = 2003;
// エリア選択モード非アクティブ時の選択アイコン
pub const IDI_SELECT_AREA_OFF: i32 = 2004;
// フォルダー参照ボタン用のフォルダーアイコン
pub const IDI_SELECT_FOLDER: i32 = 2005;
// アプリケーション終了ボタン用の×アイコン
pub const IDI_CLOSE: i32 = 2006;
// PDF変換ボタン用の×アイコン
pub const IDI_EXPORT_PDF: i32 = 2007;
// アプリケーションメインアイコン（タスクバー、ウィンドウタイトル用）
pub const IDI_APP_ICON: i32 = 2008;

// キャプチャオーバーレイ用画像リソース識別子
pub const IDP_CAPTURE_PROCESSING: i32 = 2009;
pub const IDP_CAPTURE_WAITING: i32 = 2010;

// ===== カスタムウィンドウメッセージ =====
// WM_APP (0x8000) 以降はアプリケーション定義メッセージとして使用可能
// 自動クリック処理完了をメインスレッドに通知する
pub const WM_AUTO_CLICK_COMPLETE: u32 = 0x8000 + 1;

/*
============================================================================
定数使用例とベストプラクティス（AI解析用）
============================================================================

【典型的な使用パターン】

1. ダイアログリソース読み込み：
```rust
// main.rs の main() より
let dialog_id = PCWSTR(IDD_DIALOG1 as *const u16);
DialogBoxParamW(None, dialog_id, None, Some(dialog_proc), LPARAM(0));
```

2. UIコントロール取得：
```rust
if let Ok(button) = GetDlgItem(Some(hwnd), IDC_CAPTURE_START_BUTTON) {
    // ボタン操作処理
}
```

3. メッセージ処理での分岐：
```rust
match wparam.0 as i32 {
    IDC_BROWSE_BUTTON => show_folder_dialog(hwnd),
    IDC_CAPTURE_START_BUTTON => toggle_capture_mode(hwnd),
    _ => {}
}
```

4. アイコン読み込み：
```rust
let icon = LoadIconW(hinstance, PCWSTR(IDI_CAMERA_OFF as *const u16));
```

【定数管理のガイドライン】

- 新規追加：既存の番号範囲を維持し、連番で追加
- 削除時：定数は残し、使用箇所のみ削除（後方互換性）
- 変更時：resource.h、dialog.rcとの三重チェック実施
- テスト：定数変更後は必ずビルドテストを実行

【トラブルシューティング】

よくあるエラーパターン：
1. "Resource not found"：resource.hとの不整合
2. "Control not found"：dialog.rcでのID定義ミス
3. "Icon not loaded"：.icoファイルパスエラー
4. コンパイルエラー：型の不整合（i32 vs u16）

============================================================================
*/
