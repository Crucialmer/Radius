//! Atomically resizing and re-anchoring the open panel inside its monitor work area.

use tauri::{AppHandle, Runtime, WebviewWindow};

/// Resize the tray panel to `(width, height)` logical pixels and re-anchor it cleanly
/// inside the monitor work area without racing the entrance spring.
#[tauri::command(rename = "fit_panel")]
pub fn FitPanel<R:Runtime>(App:AppHandle<R>, Window:WebviewWindow<R>, width:f64, height:f64) {
	if width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0 {
		crate::Menu::Place::Fit(&App, &Window, width, height);
	}
}
