//! Leaving.

use tauri::{AppHandle, Runtime};

/// Restore the native taskbar region, remove the tray icon cleanly so Windows
/// does not leave a ghost icon behind, and exit.
#[tauri::command(rename = "quit")]
pub fn Quit<R:Runtime>(App:AppHandle<R>) {
	crate::Overlay::Platform::Taskbar::Restore();
	crate::Tray::Remove::Fn(&App);

	App.exit(0);
}
