//! Hiding every scrim.

use tauri::{AppHandle, Runtime};

/// Hide every scrim immediately.
#[cfg(target_os = "windows")]
pub fn Fn<R:Runtime>(_App:&AppHandle<R>) { crate::Backdrop::Native::Close(); }

/// Hide every scrim.
#[cfg(not(target_os = "windows"))]
pub fn Fn<R:Runtime>(App:&AppHandle<R>) {
	use tauri::Manager;

	use crate::Backdrop::Owns;

	App.webview_windows()
		.into_values()
		.filter(|Window| Owns::Fn(Window.label()))
		.for_each(|Window| {
			let _ = Window.hide();
		});
}
