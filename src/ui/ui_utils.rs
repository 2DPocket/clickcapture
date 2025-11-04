/*
============================================================================
UIユーティリティモジュール (ui_utils.rs)
============================================================================

【ファイル概要】
UI関連の共通ヘルパー関数を提供するモジュール。
現在は、実行ファイルに埋め込まれたPNGリソースをGDI+ビットマップとして
安全に読み込む機能を提供します。

【主要機能】
1.  **PNGリソースの読み込み**: `load_png_from_resource`
    -   Win32 APIを駆使して、リソースセクションからバイナリデータを取得。
    -   取得したデータをインメモリの`IStream`に変換。
    -   `IStream`からGDI+の`GpBitmap`オブジェクトを生成。

【技術仕様】
-   **リソースタイプ**: `RT_RCDATA` を使用して、任意のバイナリデータ（この場合はPNG）を埋め込み。
-   **メモリ管理**: `SHCreateMemStream` を利用して、リソースデータをCOMの`IStream`に安全にラップ。`IStream`が解放される際に自動的にメモリがクリーンアップされるため、手動でのメモリ管理が不要。
-   **GDI+連携**: `GdipCreateBitmapFromStream` を使用して、ストリームから直接GDI+ビットマップを生成。これにより、中間ファイルへの書き出しが不要となり、パフォーマンスが向上。
-   **エラーハンドリング**: 各API呼び出しの結果を`Result`型で返し、エラー発生時の原因特定を容易に。

【AI解析用：依存関係】
-   `windows`クレート: Win32 APIおよびGDI+ APIへのアクセス。
-   `constants.rs`: `IDP_CAPTURE_PROCESSING`などのリソースID定義。
-   `capturing_overlay.rs`: このモジュールの関数を呼び出して、オーバーレイに表示する画像を取得。
 */

// 必要なライブラリ（外部機能）をインポート
use windows::{
    Win32::{
        UI::
            Shell::SHCreateMemStream // メモリストリーム作成
        ,
        System:: {
            Com::IStream,
            LibraryLoader::{FindResourceW, GetModuleHandleW, LoadResource, LockResource, SizeofResource},
        },
        Media::KernelStreaming::RT_RCDATA, // リソースタイプ定義
    },
    core::PCWSTR, // Windows API用の文字列操作
};

// GDI+機能群のインポート
use windows::Win32::Graphics::GdiPlus::{
    GdipCreateBitmapFromStream, GpBitmap, Status
};

use std::slice;

/// 埋め込みリソースからPNG画像を読み込み、GDI+ビットマップを作成する
///
/// 実行ファイルに`RT_RCDATA`として埋め込まれたPNGリソースを、
/// GDI+で描画可能な`GpBitmap`オブジェクトに変換します。
///
/// # 引数
/// * `resource_id` - `resource.h`で定義されたリソースID (`PCWSTR`形式)。
///
/// # 戻り値
/// * `Ok(*mut GpBitmap)` - 成功した場合、GDI+ビットマップへのポインタ。
/// * `Err(String)` - 失敗した場合、エラーメッセージ。
///
/// # 処理フロー
/// 1.  `FindResourceW`: 実行ファイル内のリソースを検索。
/// 2.  `LoadResource`: リソースをメモリにロード。
/// 3.  `LockResource`: リソースデータへのポインタを取得。
/// 4.  `SizeofResource`: リソースのサイズを取得。
/// 5.  `slice::from_raw_parts`: ポインタとサイズからRustの`&[u8]`スライスを安全に作成。
/// 6.  `SHCreateMemStream`: メモリスライスからCOMの`IStream`オブジェクトを作成。
///     これにより、リソースデータをファイルのようにストリームとして扱える。
/// 7.  `GdipCreateBitmapFromStream`: `IStream`からGDI+ビットマップを生成。
///
/// # 安全性
/// この関数は`unsafe`ブロックを含みますが、Win32 API呼び出しは適切に処理され、
/// メモリ管理は`IStream`のRAIIパターンによって自動的に行われるため安全です。
/// 呼び出し元は、返された`GpBitmap`ポインタを`GdipDisposeImage`で解放する責任があります。
pub fn load_png_from_resource(resource_id: PCWSTR) -> Result<*mut GpBitmap, String> {
    unsafe {
        let hinstance = GetModuleHandleW(None).map_err(|e| e.to_string())?;

        // 1. 実行ファイルからリソースを検索
        let resource_handle = FindResourceW(Some(hinstance), resource_id, RT_RCDATA);
        if resource_handle.0 == std::ptr::null_mut() {
            return Err("リソースの検索に失敗しました (FindResourceW)".to_string());
        }

        // 2. リソースをメモリにロード
        let loaded_resource = LoadResource(Some(hinstance), resource_handle)
            .map_err(|e| format!("リソースのロードに失敗しました (LoadResource): {}", e))?;

        // 3. リソースデータへのポインタを取得
        let resource_ptr = LockResource(loaded_resource);
        if resource_ptr.is_null() {
            return Err("リソースポインタの取得に失敗しました (LockResource)".to_string());
        }

        // 4. リソースデータのサイズを取得
        let resource_size = SizeofResource(Some(hinstance), resource_handle);
        if resource_size == 0 {
            return Err("リソースサイズが0です (SizeofResource)".to_string());
        }

        // 5. ポインタとサイズからRustのバイトスライスを作成
        let data_slice: &[u8] = slice::from_raw_parts(
            resource_ptr as *const u8,
            resource_size as usize,
        );

        // 6. バイトスライスからインメモリのCOMストリーム(`IStream`)を作成
        // `SHCreateMemStream`は、渡されたデータを内部でコピーし、
        // ストリームオブジェクトが解放されるときに自動的にメモリを解放します。
        let stream: Option<IStream> = SHCreateMemStream(Some(data_slice));

        let stream = match stream {
            Some(s) => s,
            None => return Err("メモリストリームの作成に失敗しました (SHCreateMemStream)".to_string()),
        };

        // 7. `IStream`からGDI+ビットマップオブジェクトを作成
        let mut bitmap: *mut GpBitmap = std::ptr::null_mut();
        let status = GdipCreateBitmapFromStream(&stream, &mut bitmap);

        if status != Status(0) {
            return Err(format!(
                "ストリームからのビットマップ作成に失敗しました (GdipCreateBitmapFromStream): {:?}",
                status
            ));
        }

        // ポインタがnullでないことを確認
        if bitmap.is_null() {
            return Err(
                "ビットマップは正常に作成されましたが、ポインタがnullです".to_string(),
            );
        }

        Ok(bitmap)
    }
}
