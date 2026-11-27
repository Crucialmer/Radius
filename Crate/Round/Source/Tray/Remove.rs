//! Removing the system tray icon cleanly before exiting so Windows never
//! retains stale ghost icons after quit or self-update restarts.

use tauri::{AppHandle, Runtime};

use crate::Tray::Label;

/// Hide and unregister Radius's tray icon (`Shell_NotifyIconW(NIM_DELETE)` on
/// Windows). Safe and idempotent to call multiple times during shutdown.
pub fn Fn<R:Runtime>(App:&AppHandle<R>) {
	if let Some(Tray) = App.tray_by_id(Label::Const) {
		let _ = Tray.set_visible(false);
	}

	let _ = App.remove_tray_by_id(Label::Const);
}
