/*
============================================================================
UI統合モジュール (ui.rs)
============================================================================

【ファイル概要】
アプリケーションのUI関連機能を統括するメインモジュールです。
UIの各関心事（初期化、イベント処理、状態更新、描画など）を個別の
サブモジュールに分割し、それらを一つのインターフェースとして公開します。
これにより、UIコードのモジュール性と保守性を高めています。

【公開サブモジュールと責務】
-   **`dialog_handlers`**:
    メインダイアログの表示状態（最小化/復元）を制御します。

-   **`draw_icon_button`**:
    オーナードローボタン（アイコン付きボタン）のカスタム描画処理を担当します。

-   **`initialize_controls`**:
    ダイアログ起動時（`WM_INITDIALOG`）に、各UIコントロール（コンボボックス、エディットボックス等）を初期化します。

-   **`input_control_handlers`**:
    ユーザーによるUI操作（ボタンクリック、選択変更など）のイベントを処理し、`AppState` を更新します。

-   **`update_input_control_states`**:
    アプリケーションの現在のモードに応じて、UIコントロールの有効/無効状態を動的に更新します。

-   **`ui_utils`**:
    UI関連の共通ヘルパー関数（例: リソースからのPNG画像読み込み）を提供します。

【設計意図】
-   **関心の分離**: UIの各機能を専門のモジュールに分けることで、コードの可読性と再利用性を向上させます。
-   **凝集度の向上**: 関連する機能が同じモジュールに集まることで、変更時の影響範囲が特定しやすくなります。

【AI解析用：依存関係】
- `main.rs`: このモジュールを `use crate::ui::{...}` として利用し、ダイアログプロシージャ内で各サブモジュールの関数を呼び出します。

 */

 pub mod input_control_handlers;
pub mod path_edit_handler;
pub mod scale_combo_handler;
pub mod pdf_size_combo_handler;
pub mod auto_click_checkbox_handler;
pub mod auto_click_interval_combo_handler;
pub mod auto_click_count_edit_handler;
pub mod pdf_export_button_handler;
pub mod quality_combo_handler;
pub mod dialog_handler;
pub mod icon_button;
pub mod folder_manager;

