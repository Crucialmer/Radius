//! Re-synchronising an overlay window once its webview listeners are attached.

use tauri::{AppHandle, Runtime, WebviewWindow};

/// Push live maximized, work-area inset, and taskbar geometry to `Window`.
#[tauri::command(rename = "overlay_ready")]
pub fn Ready<R:Runtime>(App:AppHandle<R>, Window:WebviewWindow<R>) {
	crate::Overlay::Platform::Watch::Resync(&App, Window.label());
}
