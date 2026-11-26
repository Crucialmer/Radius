//! Clearing the session when the panel closes.

use tauri::{AppHandle, Runtime};
#[cfg(target_os = "macos")]
use tauri::tray::MouseButton;

#[cfg(target_os = "macos")]
use crate::Tray::Reset;
use crate::Tray::Session::Store;

/// Forget the session, and unpress the icon when the platform left it pressed.
///
/// The read takes the record, so the reset happens at most once per panel close:
/// a second `dismiss` - the scrim's second click, or one that arrives after the
/// tray already closed the panel - finds no session and touches nothing.
///
/// A right or middle click is the only one worth repairing. A left click's own
/// `mouseUp` clears the highlight inside the platform, and a close with no
/// session behind it has no highlight to clear, so neither is worth making the
/// icon blink.
pub fn Fn<R:Runtime>(App:&AppHandle<R>) {
	let Session = Store::Const.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner()).take();

	// Rebuilding the status item clears macOS `NSStatusItem`'s stuck right-click
	// highlight. On Windows `Shell_NotifyIcon(NIM_DELETE/NIM_ADD)` makes the tray
	// icon visibly blink and can demote a pinned icon back into the overflow tray.
	#[cfg(target_os = "macos")]
	match Session {
		Some(MouseButton::Right | MouseButton::Middle) => Reset::Fn(App),
		Some(MouseButton::Left) | None => (),
	}

	#[cfg(not(target_os = "macos"))]
	let _ = (App, Session);
}
