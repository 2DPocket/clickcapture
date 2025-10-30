/*
============================================================================
オーバーレイウィンドウ共通処理モジュール (overlay.rs)
============================================================================

【ファイル概要】
アプリケーションで使用される全てのオーバーレイウィンドウ（例: エリア選択、キャプチャモード表示）
の共通基盤を提供するモジュール。Win32 APIのLayered Window機能とGDI+を組み合わせ、
高性能かつ透過的な描画を実現します。

【主要機能】
1.  **`Overlay` トレイト**: 各オーバーレイウィンドウが実装すべき共通インターフェースを定義。
    -   ウィンドウの作成、表示、非表示、再描画、位置設定などの基本操作を抽象化。
2.  **動的なウィンドウクラス登録と作成**:
    -   オーバーレイの種類ごとにユニークなウィンドウクラスを登録し、Layered Windowを作成。
    -   `WM_CREATE` メッセージで各オーバーレイ固有のプロシージャポインタをウィンドウに関連付け。
3.  **高性能な透過描画 (`paint_by_update_layered_window`)**:
    -   `UpdateLayeredWindow` を使用し、ハードウェアアクセラレーションによる高速な透過描画を実現。
    -   32bpp DIB (Device Independent Bitmap) をメモリDCに作成し、GDI+で描画後、一括で画面に転送。
    -   アンチエイリアシング (`SmoothingModeAntiAlias`) を適用し、高品質な描画を提供。
4.  **共通メッセージディスパッチ (`overlay_dispatch_proc`)**:
    -   `WM_CREATE`, `WM_PAINT`, `WM_DESTROY` などの主要なウィンドウメッセージを処理。
    -   各オーバーレイ固有の描画ロジックを呼び出すためのフックポイントを提供。
5.  **堅牢なリソース管理**:
    -   ウィンドウ、ウィンドウクラス、GDI+オブジェクトなどのリソースを適切に作成・破棄。
    -   `WM_DESTROY` 時に `Box::from_raw` を使用して、`WM_CREATE` でポインタ化した構造体の所有権を安全に回収し、メモリリークを防止。

【技術仕様】
-   **設計パターン**:
    -   **トレイトベースのポリモーフィズム**: `Overlay` トレイトにより、異なるオーバーレイが共通のインターフェースで扱える。
    -   **RAII (Resource Acquisition Is Initialization)**: `WM_DESTROY` 処理での `Box::from_raw` によるリソースの安全な解放。

【処理フロー】
1.  **オーバーレイ作成 (`create_overlay`)**:
    -   `Overlay` トレイトを実装する構造体（例: `AreaSelectOverLay`）が `create_overlay()` を呼び出す。
    -   ユニークなウィンドウクラスを登録し、`overlay_dispatch_proc` をウィンドウプロシージャとして設定。
    -   `CreateWindowExW` で Layered Window を作成。この際、`OverlayWindowProc` のポインタを `lpCreateParams` として渡す。
2.  **ウィンドウメッセージ処理 (`overlay_dispatch_proc`)**:
    -   **`WM_CREATE`**: `lpCreateParams` から `OverlayWindowProc` のポインタを取得し、`GWLP_USERDATA` に保存。
    -   **`WM_PAINT`**: `GWLP_USERDATA` から `OverlayWindowProc` を取得し、`paint_by_update_layered_window` を呼び出す。
    -   **`WM_DESTROY`**: `GWLP_USERDATA` から `OverlayWindowProc` を取得し、`Box::from_raw` で所有権を回収してメモリを解放。
3.  **描画処理 (`paint_by_update_layered_window`)**:
    -   32bppのDIB SectionをメモリDCに作成。
    -   このメモリDCからGDI+の `GpGraphics` オブジェクトを作成。
    -   各オーバーレイ固有の `paint` 関数（例: `AreaSelectOverLay::overlay_window_paint`）を呼び出し、DIBに描画。
    -   `UpdateLayeredWindow` を使用して、描画されたDIBの内容を透過的に画面に転送。
    -   GDIリソースを解放。

============================================================================
*/

use core::str;

use windows::{
    Win32::{
        Foundation::{COLORREF, ERROR_CLASS_ALREADY_EXISTS, GetLastError, HMODULE, HWND, LPARAM, LRESULT, RECT, WPARAM}, 
        Graphics::{Gdi::*, GdiPlus::{GdipCreateFromHDC, GdipDeleteGraphics, GdipSetSmoothingMode, GpGraphics, SmoothingModeAntiAlias, Status}}, 
        System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::*
    },
    core::{Error, PCWSTR}, // Windows API用の文字列操作
};

// アプリケーション状態管理構造体
use crate::app_state::*;


/// オーバーレイウィンドウプロシージャ構造体
/// 各オーバーレイウィンドウのメッセージ処理関数ポインタを格納
/// # フィールド
/// - create: ウィンドウ作成時の初期化関数ポインタ
/// - paint: ウィンドウ描画関数ポインタ
/// - destroy: ウィンドウ破棄時のクリーンアップ関数ポインタ
/// 
pub struct OverlayWindowProc {
    pub create: Option<fn (hwnd: HWND)>,
    pub paint: Option<fn (hwnd: HWND, graphics: *mut GpGraphics)>,
    pub destroy: Option<fn (hwnd: HWND)>, 
}

/// オーバーレイウィンドウ作成パラメータ構造体
/// # フィールド
/// - dwex_style: 拡張ウィンドウスタイル
/// - style: ウィンドウスタイル
/// - x: ウィンドウの初期X座標
/// - y: ウィンドウの初期Y座標
/// - width: ウィンドウの幅
/// - height: ウィンドウの高さ
/// - hwnd_parent: 親ウィンドウのHWND
/// 
pub struct OverlayWindowParams {
    pub dwex_style: WINDOW_EX_STYLE,
    pub style: WINDOW_STYLE,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub hwnd_parent: Option<HWND>,
}

/// デフォルトのオーバーレイウィンドウ作成パラメータ
impl Default for OverlayWindowParams {
    fn default() -> Self {
        Self {
            dwex_style: WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT,
            style: WS_POPUP,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            hwnd_parent: None,
        }
    }
}

/// オーバーレイクラス作成パラメータ構造体
/// # フィールド
/// - h_cursor: クラスのカーソルハンドル
/// - hbr_background: クラスの背景ブラシハンドル
///
pub struct OverlayWindowClassParams {
    pub h_cursor: HCURSOR,
    pub hbr_background: HBRUSH,
}

/// デフォルトのオーバーレイクラス作成パラメータ
impl Default for OverlayWindowClassParams {
    fn default() -> Self {
        unsafe {
            Self {
                h_cursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbr_background: HBRUSH::default(),
            }
        }
    }
}   


pub trait Overlay {
    /// HWND管理用セッター
    fn set_hwnd(&mut self, hwnd: Option<SafeHWND>);

    /// HWND管理用ゲッター
    fn get_hwnd(&self) -> Option<SafeHWND>; 

    /// オーバーレイクラス名取得
    fn get_overlay_name(&self) -> &str;

    /// オーバーレイ説明文取得
    fn get_description(&self) -> &str;

    /// オーバーレイウィンドウ名取得
    fn get_windows_name(&self) -> String {
        format!("ClickCapture_{}_Windows", self.get_overlay_name())
    }

    /// オーバーレイウィンドウパラメータ取得
    fn get_window_params(&self) -> OverlayWindowParams;

    /// オーバーレイウィンドウプロシージャ取得
    fn get_window_proc(&self) -> OverlayWindowProc;

    /// オーバーレイクラス名取得
    fn get_class_name(&self) -> String {
        format!("ClickCapture_{}_Class", self.get_overlay_name())
    }
 
    /// オーバーレイクラスパラメータ取得
    fn get_class_params(&self) -> OverlayWindowClassParams;

    /// オーバーレイウィンドウ表示制御
    /// 
    /// # 機能
    /// - 既存オーバーレイチェック・新規作成
    /// - 最前面表示（HWND_TOPMOST）
    /// - 即座表示・ユーザーフィードバック
    /// 
    /// # 処理フロー
    /// 1. オーバーレイウィンドウ未作成時：create_overlay()実行
    /// 2. ShowWindow(SW_SHOW)・即座表示
    /// 3. set_window_pos_when_show・初期ウィンドウの位置調整
    fn show_overlay(&mut self) {
        let overlay_exists = self.get_hwnd().is_some();

        // オーバーレイウィンドウが存在しない場合は作成
        if !overlay_exists {
            self.create_overlay();
        }

        if let Some(hwnd) =  self.get_hwnd() {
            unsafe {
                let _ = ShowWindow(*hwnd, SW_SHOW);
            }
            // 表示の時は描画要求を実行
            // 再表示したときも、最新状態に更新
            self.refresh_overlay();

            // 位置設定
            self.set_window_pos();

        }   
    }

    // オーバーレイウィンドウの位置設定
    fn set_window_pos(&self) {
        if let Some(hwnd) = self.get_hwnd() {
            unsafe {
                let _ = SetWindowPos(
                    *hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        }
    }

    // オーバーレイ再描画要求
    fn refresh_overlay(&self) {
        unsafe {
            if let Some(hwnd) = self.get_hwnd() {
                let _ = InvalidateRect(Some(*hwnd), None, true);
                let _ = UpdateWindow(*hwnd);    
            }
        }
    }

    /// オーバーレイ高速非表示制御
    /// 
    /// # 効率設計
    /// - ウィンドウ破棄なし・ShowWindow(SW_HIDE)のみ
    /// - 再表示時：即座復帰・初期化不要
    /// 
    /// # 使用場面
    /// - モード切替時・状態変更
    /// - ユーザー操作キャンセル時
    fn hide_overlay(&self) {
        if let Some(hwnd) = self.get_hwnd() {
            unsafe {
                let _ = ShowWindow(*hwnd, SW_HIDE);
            }
        }
    }

    /// オーバーレイウィンドウ作成
    /// 
    /// # 処理フロー
    /// 1. ウィンドウクラス登録（RegisterClassExW）
    /// 2. ウィンドウ作成（CreateWindowExW）
    /// 3. HWND保存・成功/失敗ログ出力
    /// 
    fn create_overlay(&mut self) {
        let class_name_wide: Vec<u16> = self.get_class_name().as_str().encode_utf16().chain(std::iter::once(0)).collect();
        let class_name = PCWSTR(class_name_wide.as_ptr());

        let window_name_wide: Vec<u16> = self.get_windows_name().as_str().encode_utf16().chain(std::iter::once(0)).collect();
        let window_name = PCWSTR(window_name_wide.as_ptr());

        let hinstance;
        unsafe {
            hinstance = GetModuleHandleW(None).unwrap_or_default();
        }
        let class_params = self.get_class_params();

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(overlay_dispatch_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance.into(),
            hIcon: HICON::default(),
            hCursor: class_params.h_cursor,
            hbrBackground: class_params.hbr_background,
            lpszMenuName: PCWSTR::null(),
            lpszClassName: class_name,
            hIconSm: HICON::default(),
        };

        let overlay_result;
        unsafe {
            if RegisterClassExW(&wc) == 0 {
                if GetLastError().0 != ERROR_CLASS_ALREADY_EXISTS.0 {
                    eprintln!("❌ {} オーバーレイのクラス登録に失敗", self.get_description());
                    return;
                }else {
                    println!("ℹ️ {} オーバーレイのクラスは既に登録済み", self.get_description());
                }
            }

            overlay_result = self.create_window(hinstance, class_name, window_name);
        }

        match overlay_result {
            Ok(hwnd) => {
                self.set_hwnd(Some(SafeHWND(hwnd)));
                println!("✅ {} オーバーレイを作成しました({} {})", self.get_description(), self.get_class_name().as_str(), self.get_windows_name().as_str());
            }
            Err(e) => {
                eprintln!("❌ {} オーバーレイの作成に失敗しました({} {})", self.get_description(), self.get_class_name().as_str(), self.get_windows_name().as_str());
                eprintln!("❌ エラー詳細: {:?}", e);
            }
        }
    }

    /// オーバーレイウィンドウ作成
    fn create_window(&self, hinstance: HMODULE, class_name: PCWSTR, window_name: PCWSTR) -> Result<HWND, Error> {
        let params = self.get_window_params();

        // ウィンドウとSelfの関連付けを保存
        let boxed_overlay_window_proc = Box::new(self.get_window_proc());
        let boxed_overlay_window_proc_ptr = Box::into_raw(boxed_overlay_window_proc) as *mut std::ffi::c_void;

        let overlay_result;
        unsafe {
            overlay_result = CreateWindowExW(
                params.dwex_style,
                class_name,
                window_name,
                params.style,
                params.x,
                params.y,
                params.width,
                params.height,
                params.hwnd_parent,
                None,
                Some(hinstance.into()),
                Some(boxed_overlay_window_proc_ptr),
            );
        }            
        overlay_result
   }


    /// オーバーレイ完全クリーンアップ・リソース解放
    /// 
    /// # 解放処理
    /// 1. DestroyWindow：ウィンドウ破棄・OS通知
    /// 2. UnregisterClassW：クラス登録解除・メモリ回収
    /// 
    fn destroy_overlay(&self) {
            if let Some(hwnd) = self.get_hwnd() {
                unsafe {let _ = DestroyWindow(*hwnd);}
                println!("🗑️ {} オーバーレイ・ウィンドウを削除しました", &self.get_description());
            }

            // ウィンドウクラスの登録解除
            let hinstance = unsafe { GetModuleHandleW(None).unwrap_or_default() };

            let class_name_wide: Vec<u16> = self.get_class_name().as_str().encode_utf16().chain(std::iter::once(0)).collect();
            let class_name = PCWSTR(class_name_wide.as_ptr());
            let _ = unsafe { UnregisterClassW(class_name, Some(hinstance.into())) };

            println!("🗑️ {} オーバーレイ・クラスを削除しました", &self.get_description());
    }    
}

/// オーバーレイウィンドウプロシージャ・メッセージ処理
/// 
/// # 処理メッセージ
/// - WM_CREATE：初期化
/// - WM_PAINT：描画処理
/// - WM_DESTROY：クリーンアップ
/// 
/// # WM_PAINT詳細処理
/// - paint_by_update_layered_window を呼び出し、UpdateLayeredWindowを使用した高速描画を行う
///
extern "system" fn overlay_dispatch_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let overlay_window_proc;
            let boxed_overlay_window_proc_ptr;
            unsafe {
                let createstruct = lparam.0 as *const CREATESTRUCTW;
                boxed_overlay_window_proc_ptr = (*createstruct).lpCreateParams as *const OverlayWindowProc;
                overlay_window_proc = &*boxed_overlay_window_proc_ptr;
            }

            if let Some(create) = overlay_window_proc.create.as_ref() {
                create(hwnd);
            }

            unsafe {SetWindowLongPtrW(hwnd, GWLP_USERDATA, boxed_overlay_window_proc_ptr as isize);}
            LRESULT(0)
        }
        WM_PAINT => {
            // 各オーバーレイウィンドウの描画処理を呼び出す
            let overlay_window_proc;
            unsafe {
                let boxed_overlay_window_proc_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const OverlayWindowProc;
                if boxed_overlay_window_proc_ptr.is_null() {
                    return LRESULT(0);
                }
                overlay_window_proc = &*boxed_overlay_window_proc_ptr;
            }

            let mut ps = PAINTSTRUCT::default();
            if let Some(paint) = overlay_window_proc.paint.as_ref() {
                unsafe { 
                    let hdc = BeginPaint(hwnd, &mut ps);
                    paint_by_update_layered_window(hwnd, hdc, paint);
                    let _ = EndPaint(hwnd, &ps);
                }
            }

            LRESULT(0)
        }
        WM_DESTROY => {
            // クリーンアップ処理
            let overlay_window_proc;
            let boxed_overlay_window_proc_ptr;
            unsafe {
                boxed_overlay_window_proc_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const OverlayWindowProc;
                overlay_window_proc = &*boxed_overlay_window_proc_ptr;
            }

            if let Some(destroy) = overlay_window_proc.destroy.as_ref() {
                destroy(hwnd);
            }

            if !boxed_overlay_window_proc_ptr.is_null() {
                unsafe {
                    // WM_CREATEでBox::into_rawによってポインタに変換されたOverlayWindowProcの
                    // 所有権をBoxに戻し、スコープを抜ける際にメモリを安全に解放する。
                    let _ = Box::from_raw(boxed_overlay_window_proc_ptr as *mut OverlayWindowProc);
                }
            }            
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// UpdateLayeredWindowを使用したオーバーレイウィンドウ描画
/// DIBを作成し、GDI+で描画後にUpdateLayeredWindowで反映
/// 
/// # 引数
/// - hwnd: オーバーレイウィンドウのHWND   
/// - hdc: オーバーレイウィンドウのHDC
/// - paint: 描画関数ポインタ (Graphicsオブジェクトを受け取る)
/// # 処理フロー    
/// 1. クライアント領域サイズ取得
/// 2. メモリDCと32bpp DIBセクション作成
/// 3. GDI+ Graphicsオブジェクト作成
/// 4. paint関数呼び出し・DIBに描画
/// 5. GDI+リソース解放
/// 6. UpdateLayeredWindowで画面に反映
/// 7. GDIリソース解放
/// # 注意点
/// - DIBセクションはトップダウン形式で作成（biHeightに負の値を指定）
/// - アンチエイリアシングを有効化（SmoothingModeAntiAlias）
/// - アルファブレンド設定（AC_SRC_ALPHA）
/// # エラー処理
/// - GDI+関数の戻り値をチェックし、エラー発生時はログ出力
/// - Graphicsオブジェクト作成失敗時は早期リターンし、後続処理をスキップ
/// # パフォーマンス
/// - UpdateLayeredWindowを使用することで、高速かつ滑らかな描画を実現
/// - メモリDCとDIBセクションを使用し、描画負荷を軽減
/// # 引用
/// - [UpdateLayeredWindow function - Windows applications | Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-updatelayeredwindow)
/// - [GDI+ Graphics Class - Windows applications | Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/gdiplus/-gdiplus-graphics-class)
/// - [Creating a Layered Window - Windows applications | Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/winmsg/creating-a-layered-window)
/// # 引数の関数ポインタ仕様
/// /// - paint関数はhwndとGpGraphicsポインタを受け取り、voidを返す
/// # 例
/// /// ```rust
/// /// fn my_paint_function(hwnd: HWND, graphics: *mut GpGraphics) {
/// /// ///     // GDI+を使用した描画処理
/// /// /// }
/// /// /// paint_by_update_layered_window(hwnd, hdc, &my_paint_function);
/// /// ```
///
fn paint_by_update_layered_window(hwnd: HWND, hdc: HDC, paint: &fn (hwnd: HWND, graphics: *mut GpGraphics)) {

    // クライアント領域サイズ取得
    let mut client_rect = RECT::default();
    unsafe { 
        let _ = GetClientRect(hwnd, &mut client_rect);
    }

    let width = client_rect.right - client_rect.left;
    let height = client_rect.bottom - client_rect.top;

    // UpdateLayeredWindow用のメモリDCと32bpp DIBを作成
    let mem_dc = unsafe { CreateCompatibleDC(Some(hdc)) };
                    
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height, // トップダウンDIB
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut bits = std::ptr::null_mut();
    
    let mem_bmp;
    let old_bmp;
    unsafe {
        mem_bmp = CreateDIBSection(
            Some(hdc),
            &bmi as *const BITMAPINFO,
            DIB_RGB_COLORS,
            &mut bits,
            None,
            0,
        ).unwrap();
        
        old_bmp = SelectObject(mem_dc, mem_bmp.into());
    }

    // DIBSectionが選択されたメモリDCからGDI+のGraphicsオブジェクトを作成
    let mut graphics: *mut GpGraphics = std::ptr::null_mut();
    unsafe { 
        let status = GdipCreateFromHDC(mem_dc, &mut graphics);
        if status != Status(0) { // Status(0) は Ok
            println!("★ Error: GdipCreateFromHDC failed with status {:?}", status);
            return; // Graphicsオブジェクトが作成できないと後続処理は不可能
        }

        let status = GdipSetSmoothingMode(graphics, SmoothingModeAntiAlias);
        if status != Status(0) {
            println!("★ Warning: GdipSetSmoothingMode failed with status {:?}", status);
        }
    };

    // paint関数を呼び出してメモリDCに描画
    paint(hwnd, graphics);

    // GDI+リソースの解放
    unsafe { 
        GdipDeleteGraphics(graphics); 
    };

    // UpdateLayeredWindowで画面に反映
    let blend_function = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255, // ビットマップのアルファ値を使用
        AlphaFormat: AC_SRC_ALPHA as u8,
    };

    let size = windows::Win32::Foundation::SIZE { cx: width, cy: height };
    let pt_src = windows::Win32::Foundation::POINT { x: 0, y: 0 };

    unsafe {
        let _ = UpdateLayeredWindow(hwnd, Some(hdc), None, Some(&size), Some(mem_dc), Some(&pt_src), COLORREF(0), Some(&blend_function), ULW_ALPHA);
    }

    // GDIリソースの解放
    unsafe {
        SelectObject(mem_dc, old_bmp);
        let _ = DeleteObject(mem_bmp.into());
        let _ = DeleteDC(mem_dc);
    }

}    

        
