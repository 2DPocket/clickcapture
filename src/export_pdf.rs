/*
============================================================================
JPEGからPDFへの変換モジュール (export_pdf.rs)
============================================================================

【ファイル概要】
指定されたフォルダ内のJPEGファイルを読み込み、1つまたは複数のPDFファイルに変換して
保存する機能を提供します。
ユーザーが設定したファイルサイズの上限を超えた場合、自動的に新しいPDFファイルを作成して
分割保存する機能を持ちます。

【主要機能】
1.  **JPEGファイルの収集とソート**:
    -   `AppState` から指定されたフォルダを読み取り、`jpg`または`jpeg`拡張子のファイルを収集します。
    -   ファイル名を昇順にソートして、ページ順序を保証します。
2.  **高品質なPDF変換 (`PdfBuilder`)**:
    -   `lopdf` クレートを利用してPDFドキュメントを構築します。
    -   JPEGデータを再圧縮せずに `DCTDecode` フィルタを使用してそのまま埋め込むことで、画質の劣化を防ぎます。
3.  **ファイルサイズの自動分割**:
    -   `AppState` で設定された最大ファイルサイズ (`pdf_max_size_mb`) を超えないように、PDFの推定サイズを監視します。
    -   上限を超えた場合、現在のPDFを保存し、新しいPDFファイルを作成して処理を継続します。
4.  **連番ファイル名**:
    -   生成されるPDFファイルには `0001.pdf`, `0002.pdf` のような4桁の連番が付与されます。

【処理フロー】
1.  `export_selected_folder_to_pdf` が呼び出されます。
2.  指定フォルダからJPEGファイルを収集・ソートします。
3.  `PdfBuilder` の新しいインスタンスを作成します。
4.  ファイルリストをループ処理:
    a. JPEGファイルを読み込み、`PdfBuilder::add_jpeg_page` でPDFページとして追加します。
    b. 一定数のファイルを追加するごとに `PdfBuilder::estimate_size` で現在のPDFサイズを推定します。
    c. 推定サイズが上限を超えた場合:
        i.  現在の `PdfBuilder` を（最後に追加したページを除いて）ファイルに保存します。
        ii. 新しい `PdfBuilder` を作成し、最後に追加したページを最初のページとして新しいPDFの構築を開始します。
5.  ループ終了後、最後の `PdfBuilder` をファイルに保存します。

【技術仕様】
-   **PDFライブラリ**: `lopdf` を使用して、低レベルなPDFオブジェクトを直接操作。
-   **画像ライブラリ**: `image` を使用して、JPEGの寸法（幅・高さ）を取得。
-   **ファイルI/O**: `std::fs` を使用してファイルとディレクトリを操作。

【AI解析用：依存関係】
- `app_state.rs`: 保存先フォルダパスやPDF最大サイズ設定を取得。
- `system_utils.rs`: `app_log` を使用して処理の進捗をログに出力。
- `lopdf`, `image`: PDF生成と画像解析のための外部クレート。
*/

use crate::app_state::*;
use crate::system_utils::app_log;
use image::GenericImageView;
use image::io::Reader as ImageReader;
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use num_format::{Locale, ToFormattedString};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

/// PDFドキュメントの構築を管理するヘルパー構造体
///
/// `lopdf` を使用して、JPEG画像からPDFページを作成し、
/// ドキュメント全体の構造（Pagesツリー、Catalogなど）を管理します。
struct PdfBuilder {
    /// `lopdf` のドキュメントオブジェクト。全てのPDFオブジェクト（ディクショナリ、ストリーム等）を保持します。
    doc: Document,
    /// 作成された各ページの `ObjectId` を保持するベクター。最終的に `Pages` ツリーの構築に使用されます。
    pages: Vec<ObjectId>,
    /// PDF内で画像リソース（XObject）にユニークな名前を付けるためのカウンター。
    current_image_counter: u32,
}

impl PdfBuilder {
    /// 新しい `PdfBuilder` インスタンスを作成します。
    fn new() -> Self {
        Self {
            doc: Document::with_version("1.5"),
            pages: Vec::new(),
            current_image_counter: 1,
        }
    }

    /// JPEG画像を新しいページとしてPDFドキュメントに追加する
    ///
    /// JPEGデータを再圧縮せずに `DCTDecode` フィルタを用いてそのまま埋め込むことで、
    /// 画質の劣化を防ぎます。
    ///
    /// # 引数
    /// * `jpeg_bytes` - JPEGファイルの生データ。
    /// * `width` - 画像の幅（ピクセル）。
    /// * `height` - 画像の高さ（ピクセル）。
    fn add_jpeg_page(
        &mut self,
        jpeg_bytes: Vec<u8>,
        width: u32,
        height: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // JPEGサイズの事前検証
        if jpeg_bytes.is_empty() {
            return Err("空のJPEGデータが渡されました".into());
        }

        if width == 0 || height == 0 {
            return Err(format!("無効な画像サイズ: {}x{}", width, height).into());
        }

        // 画像XObject（PDF内で画像を表現するオブジェクト）を作成します。
        let mut xobject = Dictionary::new();
        xobject.set("Type", "XObject");
        xobject.set("Subtype", "Image");
        xobject.set("Width", Object::Integer(width as i64));
        xobject.set("Height", Object::Integer(height as i64));
        xobject.set("ColorSpace", "DeviceRGB");
        xobject.set("BitsPerComponent", Object::Integer(8));
        xobject.set("Filter", "DCTDecode");

        // 元のJPEGデータをストリームとしてラップします。`DCTDecode`フィルタが指定されているため、
        // PDFビューアはこれをJPEGとして直接デコードします。
        let stream = Stream::new(xobject, jpeg_bytes);
        let image_id = self.doc.add_object(stream);

        // ページ内で画像を参照するためのユニークなリソース名を生成します。
        let resource_name = format!("Image{}", self.current_image_counter);
        self.current_image_counter += 1;

        // ページサイズをポイント単位で計算します。ここでは300 DPIを基準としています。
        // これにより、印刷時や表示時に適切な解像度が維持されます。
        let dpi = 300.0;
        let px_to_pt = |px: u32| -> f64 { (px as f64) * 72.0 / dpi };
        let page_width = px_to_pt(width);
        let page_height = px_to_pt(height);

        // ページコンテンツストリーム（画像をページ全体に配置）
        let contents = format!(
            "q\n{0} 0 0 {1} 0 0 cm\n/{2} Do\nQ\n",
            page_width, page_height, resource_name
        );

        let contents_stream = Stream::new(Dictionary::new(), contents.into_bytes());
        let contents_id = self.doc.add_object(contents_stream);

        // ページが使用するリソース（この場合は画像XObject）を定義するリソースディクショナリを作成します。
        let mut resources = Dictionary::new();
        let mut xobj_map = Dictionary::new();
        xobj_map.set(resource_name, image_id);
        resources.set("XObject", xobj_map);

        // ページ辞書の作成
        let mut page = Dictionary::new();
        page.set("Type", "Page");
        page.set(
            "MediaBox",
            vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Real(page_width),
                Object::Real(page_height),
            ],
        );
        page.set("Resources", resources);
        page.set("Contents", contents_id);

        let page_id = self.doc.add_object(page);
        self.pages.push(page_id);

        Ok(())
    }

    /// ドキュメントの最終処理を行い、保存可能な状態にする
    ///
    /// `Pages` ツリーと `Catalog` ディクショナリを構築し、ドキュメントのルートを設定します。
    /// この処理は、ドキュメントを保存する直前、またはサイズを推定する前に呼び出す必要があります。
    fn finalize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.pages.is_empty() {
            return Ok(()); // 空文書は何もしない
        }

        let pages_kids: Vec<Object> = self.pages.iter().map(|id| Object::Reference(*id)).collect();
        let mut pages_dict = Dictionary::new();
        pages_dict.set("Type", "Pages");
        pages_dict.set("Kids", Object::Array(pages_kids));
        pages_dict.set("Count", Object::Integer(self.pages.len() as i64));

        // 各ページのParent参照を設定
        let pages_id = self.doc.add_object(pages_dict);
        for &page_id in &self.pages {
            if let Ok(page_obj) = self.doc.get_object_mut(page_id) {
                if let Object::Dictionary(page_dict) = page_obj {
                    page_dict.set("Parent", pages_id);
                }
            }
        }

        // カタログの作成
        let mut catalog = Dictionary::new();
        catalog.set("Type", "Catalog");
        catalog.set("Pages", pages_id);
        let catalog_id = self.doc.add_object(catalog);

        // ドキュメントのルートオブジェクトとしてカタログを設定
        self.doc.trailer.set("Root", catalog_id);

        Ok(())
    }

    /// 現在構築中のPDFの推定ファイルサイズをバイト単位で計算する
    ///
    /// 内部的にドキュメントをメモリ上のバッファに保存してみて、そのサイズを返します。
    /// ファイル分割の判定に使用されます。
    fn estimate_size(&mut self) -> Result<usize, Box<dyn std::error::Error>> {
        self.finalize()?;
        let mut buffer = Vec::new();
        self.doc.save_to(&mut buffer)?;
        Ok(buffer.len())
    }

    /// 構築したPDFドキュメントを指定されたパスに保存する
    fn save_to_file(&mut self, path: &Path) -> Result<usize, Box<dyn std::error::Error>> {
        self.finalize()?;
        let mut buffer = Vec::new();
        self.doc.save_to(&mut buffer)?;
        File::create(path)?.write_all(&buffer)?;
        Ok(buffer.len())
    }
}

/// 選択されたフォルダ内のJPEG画像をPDFファイルに変換する
///
/// フォルダ内のJPEGファイルをファイル名順に読み込み、`AppState` で設定された
/// 最大ファイルサイズに基づいて、1つまたは複数のPDFファイルに分割して保存します。
pub fn export_selected_folder_to_pdf() -> Result<(), Box<dyn std::error::Error>> {
    let app_state = AppState::get_app_state_ref();
    let folder = match &app_state.selected_folder_path {
        Some(p) => p.clone(),
        None => {
            app_log("⚠️ PDF変換エラー: 保存フォルダーが選択されていません");
            return Ok(());
        }
    };

    println!("PDF変換開始: フォルダー = {}", folder);

    // フォルダの存在を確認
    let folder_path = Path::new(&folder);
    if !folder_path.exists() {
        return Err(format!("❌ 指定されたフォルダーが存在しません: {}", folder).into());
    }

    // フォルダ内のJPEGファイル（.jpg, .jpeg）を収集してファイル名でソート
    let mut entries: Vec<_> = fs::read_dir(&folder)?
        .filter_map(|r| r.ok())
        .filter(|e| {
            if let Some(ext) = e.path().extension() {
                let s = ext.to_string_lossy().to_lowercase();
                s == "jpg" || s == "jpeg"
            } else {
                false
            }
        })
        .collect();

    entries.sort_by_key(|e| e.path());

    if entries.is_empty() {
        app_log("⚠️ PDF変換: 対象のJPEGファイルが見つかりませんでした。");
        return Ok(());
    }

    println!("処理対象ファイル数: {}", entries.len());

    let mut pdf_index = 1;
    let mut current_builder = PdfBuilder::new();
    let mut files_in_current_pdf = 0;
    let mut total_processed = 0;
    let total_files = entries.len();

    // AppStateからPDFの最大ファイルサイズ（MB単位）を取得し、バイトに変換
    let app_state = AppState::get_app_state_ref();
    let max_pdf_size_bytes = (app_state.pdf_max_size_mb as u64) * 1024 * 1024;
    println!(
        "PDFサイズ上限: {} Byte",
        max_pdf_size_bytes.to_formatted_string(&Locale::ja)
    );

    for entry in entries {
        let path = entry.path();
        let filename = path
            .file_name()
            .expect("ファイル名の取得に失敗しました")
            .to_string_lossy()
            .to_string();

        total_processed += 1;
        app_log(&format!(
            "⏳ 処理中のJPEG: {} ({}/{})",
            filename, total_processed, total_files
        ));

        // `image` クレートを使って画像のデコードと寸法取得を試みる
        let img = match ImageReader::open(&path) {
            Ok(reader) => match reader.decode() {
                Ok(img) => img,
                Err(e) => {
                    eprintln!("❌ 画像デコードエラー ({}): {}", filename, e);
                    return Err(e.into());
                }
            },
            Err(e) => {
                eprintln!("❌ 画像読み込みエラー ({}): {}", filename, e);
                return Err(e.into());
            }
        };

        let (width, height) = img.dimensions();

        // JPEGファイルの生データを読み込む
        let jpeg_bytes = match fs::read(&path) {
            Ok(bytes) => {
                let file_size_mb = bytes.len() as f64 / 1024.0 / 1024.0;
                let bytes_per_pixel = bytes.len() as f64 / (width * height) as f64;

                println!(
                    "  {} x {} px, {:.1}MB, {:.3}バイト/ピクセル",
                    width, height, file_size_mb, bytes_per_pixel
                );

                if bytes.len() > 50 * 1024 * 1024 {
                    // 50MB以上の画像は警告
                    println!("⚠️ 警告: 大きな画像ファイル ({:.1}MB)", file_size_mb);
                }

                if bytes_per_pixel < 0.1 {
                    println!(
                        "⚠️ 警告: 低品質JPEG ({:.3}バイト/ピクセル)",
                        bytes_per_pixel
                    );
                } else if bytes_per_pixel > 1.0 {
                    println!("✅ 高品質JPEG ({:.3}バイト/ピクセル)", bytes_per_pixel);
                }

                bytes
            }
            Err(e) => {
                eprintln!("ファイル読み込みエラー ({}): {}", filename, e);
                return Err(e.into());
            }
        };

        // 読み込んだJPEGデータを現在の `PdfBuilder` にページとして追加
        if let Err(e) = current_builder.add_jpeg_page(jpeg_bytes.clone(), width, height) {
            eprintln!("❌ PDF追加エラー ({}): {}", filename, e);
            return Err(e.into());
        }

        files_in_current_pdf += 1;

        // ファイルサイズをチェックして、必要であればPDFを分割する。
        // 毎回チェックするとパフォーマンスが落ちるため、10ファイルごと、または最初の1ファイル以降にチェック。
        if files_in_current_pdf % 10 == 0 || files_in_current_pdf > 1 {
            let estimated_size = match current_builder.estimate_size() {
                Ok(size) => size,
                Err(e) => {
                    eprintln!("❌ PDFサイズ推定エラー: {}", e);
                    return Err(e);
                }
            };

            println!(
                "推定PDFサイズ: {} Byte",
                estimated_size.to_formatted_string(&Locale::ja)
            );

            if estimated_size > max_pdf_size_bytes as usize && files_in_current_pdf > 1 {
                app_log(&format!(
                    "➡️ PDFサイズ制限到達 ({:.1}MB)。現在のPDFを保存して新しいPDFを開始します。",
                    estimated_size as f64 / 1024.0 / 1024.0
                ));

                // 現在のPDFを保存する。ただし、サイズオーバーの原因となった最後の画像は含めない。
                // その画像は次の新しいPDFの最初のページになる。
                current_builder.pages.pop();

                if !current_builder.pages.is_empty() {
                    let output_path = Path::new(&folder).join(format!("{:04}.pdf", pdf_index));
                    match current_builder.save_to_file(&output_path) {
                        Ok(file_size) => {
                            app_log(&format!(
                                "✅ PDF完了: {} ({:.1}MB)",
                                output_path.display(),
                                file_size as f64 / 1024.0 / 1024.0
                            ));
                            pdf_index += 1;
                        }
                        Err(e) => {
                            eprintln!("❌ PDF保存エラー: {}", e);
                            return Err(e);
                        }
                    }
                }

                // 新しい `PdfBuilder` を作成し、先ほど除外した画像から新しいPDFを開始する
                current_builder = PdfBuilder::new();
                if let Err(e) = current_builder.add_jpeg_page(jpeg_bytes, width, height) {
                    eprintln!("❌ 新PDF開始エラー ({}): {}", filename, e);
                    return Err(e);
                }
                files_in_current_pdf = 1;
            }
        }
    }

    // ループ終了後、残っているページがあれば最後のPDFファイルとして保存
    if !current_builder.pages.is_empty() {
        let output_path = Path::new(&folder).join(format!("{:04}.pdf", pdf_index));
        match current_builder.save_to_file(&output_path) {
            Ok(file_size) => {
                app_log(&format!(
                    "✅ PDF完了: {} ({:.1}MB)",
                    output_path.display(),
                    file_size as f64 / 1024.0 / 1024.0
                ));
            }
            Err(e) => {
                eprintln!("❌ 最終PDF保存エラー: {}", e);
                return Err(e);
            }
        }
    }

    app_log(&format!(
        "✅ 全JPEGからのPDF変換処理が完了しました。処理ファイル数: {}",
        total_processed
    ));
    Ok(())
}
