//! Toggling rounded bottom corners for maximized windows.

use tauri::{AppHandle, Runtime};

use crate::{
	Menu::{
		State::Struct as State,
		Store::{Commit, Load},
	},
	Settings::Struct::Struct as Settings,
};

/// Enable or disable bottom rounded corners on maximized windows above the taskbar.
#[tauri::command(rename = "set_maximize_corners")]
pub fn Maximize<R:Runtime>(App:AppHandle<R>, Enabled:bool) -> Result<State, String> {
	crate::Trace!("click set_maximize_corners enabled={Enabled}");

	let mut Preferences:Settings = Load::Fn(&App)?;

	Preferences.MaximizeCorners = Enabled;

	Commit::Fn(&App, &Preferences)
}
