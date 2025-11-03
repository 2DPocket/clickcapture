/*
============================================================================
システムフック管理モジュール (hook.rs)
============================================================================

【ファイル概要】
マウスフックとキーボードフックの管理を統合するモジュール。
フックのインストールとアンインストールを一つのインターフェースで提供し、
コードの重複を削減し、保守性を向上させる。

【主要機能】
1. 統合フックインストール (install_hooks)
2. 統合フックアンインストール (uninstall_hooks)
3. サブモジュールの公開 (keyboard, mouse)

*/

pub mod keyboard;
pub mod mouse;

/// マウスフックとキーボードフックの両方をインストールします。
pub fn install_hooks() {
    keyboard::install_keyboard_hook();
    mouse::install_mouse_hook();
}

/// マウスフックとキーボードフックの両方をアンインストールします。
pub fn uninstall_hooks() {
    keyboard::uninstall_keyboard_hook();
    mouse::uninstall_mouse_hook();
}
