//! Leaving.

use tauri::{AppHandle, Manager, Runtime};

/// Stop the background watcher, immediately hide all topmost scrims/overlays so
/// the desktop stays responsive, restore the native taskbar region, remove the
/// tray icon cleanly, and exit.
#[tauri::command(rename = "quit")]
pub fn Quit<R:Runtime>(App:AppHandle<R>) {
	crate::Overlay::Platform::Watch::Stop();
	crate::Backdrop::Close::Fn(&App);

	for Window in App.webview_windows().into_values() {
		let _ = Window.hide();
	}

	crate::Overlay::Platform::Taskbar::Restore();
	crate::Tray::Remove::Fn(&App);

	std::thread::spawn(|| {
		std::thread::sleep(std::time::Duration::from_millis(1500));
		std::process::exit(0);
	});

	App.exit(0);
}
