//! Re-synchronising an overlay window once its webview listeners are attached.

use tauri::{AppHandle, Runtime, WebviewWindow};

use crate::Overlay::{Layer::Trait::Trait as _, Platform::Struct::Struct as Platform};

/// Re-assert cleaned overlay window styles and push live maximized, work-area
/// inset, and taskbar geometry to `Window`.
#[tauri::command(rename = "overlay_ready")]
pub fn Ready<R:Runtime>(App:AppHandle<R>, Window:WebviewWindow<R>) {
	Platform::Art(&Window);
	crate::Overlay::Platform::Watch::Resync(&App, Window.label());
}
