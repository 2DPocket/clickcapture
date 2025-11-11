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


