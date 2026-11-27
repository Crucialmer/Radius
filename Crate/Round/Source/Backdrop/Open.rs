//! Showing one scrim over every monitor.

use tauri::{AppHandle, Runtime};
#[cfg(not(target_os = "windows"))]
use tauri::Manager;

#[cfg(not(target_os = "windows"))]
use crate::Backdrop::{Label, Owns, Show};

/// Show one scrim over every monitor's work area.
///
/// On Windows this uses ultra-lightweight native Win32 layered windows (zero
/// WebView2 processes) for instant <1ms opening and freeze-free dismissal.
#[cfg(target_os = "windows")]
pub fn Fn<R:Runtime>(App:&AppHandle<R>) -> tauri::Result<()> { crate::Backdrop::Native::Open(App) }

/// Show one scrim over every monitor's work area, building the ones that do not
/// exist yet.
#[cfg(not(target_os = "windows"))]
pub fn Fn<R:Runtime>(App:&AppHandle<R>) -> tauri::Result<()> {
	let Monitors = App.available_monitors()?;

	Monitors.iter().enumerate().for_each(|(Index, Monitor)| {
		let _ = Show::Fn(App, Monitor, Index)
			.inspect_err(|Error| eprintln!("radius: the backdrop over monitor {Index} could not be shown: {Error}"));
	});

	let Live:Vec<String> = (0..Monitors.len()).map(Label::Fn).collect();

	App.webview_windows()
		.into_values()
		.filter(|Window| Owns::Fn(Window.label()) && !Live.iter().any(|Label| Label == Window.label()))
		.for_each(|Window| {
			let _ = Window.hide();
		});

	Ok(())
}
