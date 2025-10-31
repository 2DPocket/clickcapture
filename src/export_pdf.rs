/*
============================================================================
JPEGからPDF変換モジュール (export_pdf.rs)
============================================================================

このモジュールは、指定されたフォルダー内のJPEGファイルを順次読み込み、
1つまたは複数のPDFファイルに変換して保存します。

仕様（実装）：
- 対象はAppState.selected_folder_pathに設定されたフォルダー内の全JPEGファイル
- ファイル名は昇順で処理
- PDFサイズ上限はAppState.pdf_max_size_mb（ユーザー設定可能：500MB〜1000MB）
- サイズ上限を超過した場合は新しいPDFファイルへ自動切替
- PDF名は4桁ゼロパディングの連番（0001.pdf, 0002.pdf, ...）
- 進捗はprintln!でターミナルに出力

実装メモ：
- lopdfクレートでPDFを作成
- imageクレートでJPEGを読み込み、PDFに画像として埋め込む
- サイズ閾値はAppStateから動的取得（バイト単位比較）
*/

use crate::app_state::*;
use crate::system_utils::app_log;
use image::io::Reader as ImageReader;
use image::GenericImageView;
use lopdf::{Document, Object, Stream, Dictionary, ObjectId};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use num_format::{Locale, ToFormattedString};

// 削除：PDFサイズ制限はAppStateから取得するため定数は不要

/// PDF文書を管理する構造体 - Pagesツリーとリソース管理を含む
struct PdfBuilder {
    doc: Document,
    pages: Vec<ObjectId>,
    current_image_counter: u32,
}

impl PdfBuilder {
    fn new() -> Self {
        Self {
            doc: Document::with_version("1.5"),
            pages: Vec::new(),
            current_image_counter: 1,
        }
    }

    /// JPEGをページとして追加し、適切なXObjectリソース名を生成
    fn add_jpeg_page(&mut self, jpeg_bytes: Vec<u8>, width: u32, height: u32) -> Result<(), Box<dyn std::error::Error>> {
        // JPEGサイズの事前検証
        if jpeg_bytes.is_empty() {
            return Err("空のJPEGデータが渡されました".into());
        }
        
        if width == 0 || height == 0 {
            return Err(format!("無効な画像サイズ: {}x{}", width, height).into());
        }

        // 画像XObjectを作成（DCTDecode filter使用で元のJPEG品質を保持）
        let mut xobject = Dictionary::new();
        xobject.set("Type", "XObject");
        xobject.set("Subtype", "Image");
        xobject.set("Width", Object::Integer(width as i64));
        xobject.set("Height", Object::Integer(height as i64));
        xobject.set("ColorSpace", "DeviceRGB");
        xobject.set("BitsPerComponent", Object::Integer(8));
        xobject.set("Filter", "DCTDecode");
        
        // 🔧 修正：元のJPEGデータを直接使用（追加圧縮なし・品質劣化なし）
        let stream = Stream::new(xobject, jpeg_bytes);
        // stream.compress() を削除 - JPEGは既に最適圧縮済み
        
        let image_id = self.doc.add_object(stream);

        // ユニークなリソース名を生成（衝突回避）
        let resource_name = format!("Image{}", self.current_image_counter);
        self.current_image_counter += 1;

        // ページサイズをポイント単位で計算（OCR最適化のため高DPI設定）
        let dpi = 300.0; // OCR品質向上のため300 DPIを使用
        let px_to_pt = |px: u32| -> f64 { (px as f64) * 72.0 / dpi };
        let page_width = px_to_pt(width);
        let page_height = px_to_pt(height);

        // ページコンテンツストリーム（画像をページ全体に配置）
        let contents = format!(
            "q\n{0} 0 0 {1} 0 0 cm\n/{2} Do\nQ\n",
            page_width, page_height, resource_name
        );

        // 🔧 コンテンツストリームも無圧縮で高品質保持
        let contents_stream = Stream::new(Dictionary::new(), contents.into_bytes());
        // コンテンツストリーム圧縮を削除 - 画質優先
        let contents_id = self.doc.add_object(contents_stream);

        // リソース辞書の作成（XObjectを含む）
        let mut resources = Dictionary::new();
        let mut xobj_map = Dictionary::new();
        xobj_map.set(resource_name, image_id);
        resources.set("XObject", xobj_map);

        // ページ辞書の作成
        let mut page = Dictionary::new();
        page.set("Type", "Page");
        page.set("MediaBox", vec![
            Object::Integer(0), 
            Object::Integer(0), 
            Object::Real(page_width), 
            Object::Real(page_height)
        ]);
        page.set("Resources", resources);
        page.set("Contents", contents_id);

        let page_id = self.doc.add_object(page);
        self.pages.push(page_id);

        Ok(())
    }

    /// Pagesツリーとカタログを適切に構築して完了
    fn finalize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.pages.is_empty() {
            return Ok(()); // 空文書は何もしない
        }

        // Pagesツリーの構築
        let pages_kids: Vec<Object> = self.pages.iter().map(|id| Object::Reference(*id)).collect();
        let mut pages_dict = Dictionary::new();
        pages_dict.set("Type", "Pages");
        pages_dict.set("Kids", Object::Array(pages_kids));
        pages_dict.set("Count", Object::Integer(self.pages.len() as i64));

        // 各ページのParent参照を設定
        let pages_obj_id = self.doc.add_object(pages_dict);
        for &page_id in &self.pages {
            if let Ok(page_obj) = self.doc.get_object_mut(page_id) {
                if let Object::Dictionary(page_dict) = page_obj {
                    page_dict.set("Parent", pages_obj_id);
                }
            }
        }

        // カタログの作成
        let mut catalog = Dictionary::new();
        catalog.set("Type", "Catalog");
        catalog.set("Pages", pages_obj_id);
        let catalog_id = self.doc.add_object(catalog);

        // トレーラーにルート参照を設定
        self.doc.trailer.set("Root", catalog_id);

        Ok(())
    }

    /// 現在の文書サイズを計算（メモリ効率を考慮）
    fn estimate_size(&mut self) -> Result<usize, Box<dyn std::error::Error>> {
        self.finalize()?;
        let mut buffer = Vec::new();
        self.doc.save_to(&mut buffer)?;
        Ok(buffer.len())
    }

    /// 文書をファイルに保存
    fn save_to_file(&mut self, path: &Path) -> Result<usize, Box<dyn std::error::Error>> {
        self.finalize()?;
        let mut buffer = Vec::new();
        self.doc.save_to(&mut buffer)?;
        File::create(path)?.write_all(&buffer)?;
        Ok(buffer.len())
    }
}

/// 選択フォルダ内のJPEGをPDFへ変換して保存する（改善版）
pub fn export_selected_folder_to_pdf() -> Result<(), Box<dyn std::error::Error>> {
    let app_state = AppState::get_app_state_ref();
    let folder = match &app_state.selected_folder_path {
        Some(p) => p.clone(),
        None => {
            app_log("⚠️ PDF変換エラー: 保存フォルダーが選択されていません");
            return Ok(());
        }
    };

    app_log(&format!("PDF変換開始: フォルダー = {}", folder));

    // フォルダーの存在確認
    let folder_path = Path::new(&folder);
    if !folder_path.exists() {
        return Err(format!("❌ 指定されたフォルダーが存在しません: {}", folder).into());
    }

    // JPEG ファイルを収集してソート
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

    // AppStateからPDFサイズ上限を取得
    let app_state = AppState::get_app_state_ref();
    let max_pdf_size_bytes = (app_state.pdf_max_size_mb as u64) * 1024 * 1024;
    println!("PDFサイズ上限: {} Byte", max_pdf_size_bytes.to_formatted_string(&Locale::ja));

    for entry in entries {
        let path = entry.path();
        let filename = path.file_name()
            .expect("ファイル名の取得に失敗しました")
            .to_string_lossy().to_string();
        
        total_processed += 1;
        app_log(&format!("⏳ 処理中のJPEG: {} ({}/{})", filename, total_processed, total_files));

        // JPEG画像情報を取得（エラーハンドリング強化）
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
        
        // ファイルサイズチェックと品質情報表示
        let jpeg_bytes = match fs::read(&path) {
            Ok(bytes) => {
                // 🔧 追加：JPEG品質情報の表示
                let file_size_mb = bytes.len() as f64 / 1024.0 / 1024.0;
                let bytes_per_pixel = bytes.len() as f64 / (width * height) as f64;
                
                println!("  {} x {} px, {:.1}MB, {:.3}バイト/ピクセル", 
                        width, height, file_size_mb, bytes_per_pixel);
                
                if bytes.len() > 50 * 1024 * 1024 { // 50MB以上の画像は警告
                    println!("⚠️ 警告: 大きな画像ファイル ({:.1}MB)", file_size_mb);
                }
                
                if bytes_per_pixel < 0.1 {
                    println!("⚠️ 警告: 低品質JPEG ({:.3}バイト/ピクセル)", bytes_per_pixel);
                } else if bytes_per_pixel > 1.0 {
                    println!("✅ 高品質JPEG ({:.3}バイト/ピクセル)", bytes_per_pixel);
                }
                
                bytes
            },
            Err(e) => {
                eprintln!("ファイル読み込みエラー ({}): {}", filename, e);
                return Err(e.into());
            }
        };

        // 画像をPDFに追加
        if let Err(e) = current_builder.add_jpeg_page(jpeg_bytes.clone(), width, height) {
            eprintln!("❌ PDF追加エラー ({}): {}", filename, e);
            return Err(e.into());
        }
        
        files_in_current_pdf += 1;
        
        // サイズチェック（メモリ効率を考慮してバッチ処理）
        if files_in_current_pdf % 10 == 0 || files_in_current_pdf > 1 {
            let estimated_size = match current_builder.estimate_size() {
                Ok(size) => size,
                Err(e) => {
                    eprintln!("❌ PDFサイズ推定エラー: {}", e);
                    return Err(e);
                }
            };

            println!("推定PDFサイズ: {} Byte", estimated_size.to_formatted_string(&Locale::ja));

            if estimated_size > max_pdf_size_bytes as usize && files_in_current_pdf > 1 {
                app_log(&format!("➡️ PDFサイズ制限到達 ({:.1}MB)。現在のPDFを保存して新しいPDFを開始します。", 
                        estimated_size as f64 / 1024.0 / 1024.0));
                
                // 最後の画像を除いて現在のPDFを保存
                current_builder.pages.pop(); // 最後の画像ページを削除
                
                if !current_builder.pages.is_empty() {
                    let output_path = Path::new(&folder).join(format!("{:04}.pdf", pdf_index));
                    match current_builder.save_to_file(&output_path) {
                        Ok(file_size) => {
                            app_log(&format!("✅ PDF完了: {} ({:.1}MB)", 
                                    output_path.display(), file_size as f64 / 1024.0 / 1024.0));
                            pdf_index += 1;
                        },
                        Err(e) => {
                            eprintln!("❌ PDF保存エラー: {}", e);
                            return Err(e);  
                        }
                    }
                }
                
                // 新しいビルダーで現在の画像から開始
                current_builder = PdfBuilder::new();
                if let Err(e) = current_builder.add_jpeg_page(jpeg_bytes, width, height) {
                    eprintln!("❌ 新PDF開始エラー ({}): {}", filename, e);
                    return Err(e);  
                }
                files_in_current_pdf = 1;
            }
        }
    }

    // 最後のPDFを保存
    if !current_builder.pages.is_empty() {
        let output_path = Path::new(&folder).join(format!("{:04}.pdf", pdf_index));
        match current_builder.save_to_file(&output_path) {
            Ok(file_size) => {
                app_log(&format!("✅ PDF完了: {} ({:.1}MB)", 
                        output_path.display(), file_size as f64 / 1024.0 / 1024.0));
            },
            Err(e) => {
                eprintln!("❌ 最終PDF保存エラー: {}", e);
                return Err(e);
            }
        }
    }

    app_log(&format!("✅ 全JPEGからのPDF変換処理が完了しました。処理ファイル数: {}", total_processed));
    Ok(())
}