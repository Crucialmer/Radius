//! Leaving.

use tauri::{AppHandle, Runtime};

/// Restore the native taskbar region if modified, then exit cleanly.
#[tauri::command(rename = "quit")]
pub fn Quit<R:Runtime>(App:AppHandle<R>) {
	crate::Overlay::Platform::Taskbar::Restore();

	App.exit(0);
}
