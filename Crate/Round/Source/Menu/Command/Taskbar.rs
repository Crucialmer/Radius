//! Toggling 4-corner rounded taskbar clipping.

use tauri::{AppHandle, Runtime};

use crate::{
	Menu::{
		State::Struct as State,
		Store::{Commit, Load},
	},
	Settings::Struct::Struct as Settings,
};

/// Enable or disable 4-corner rounded rectangle clipping on the Windows taskbar.
#[tauri::command(rename = "set_taskbar_rounded")]
pub fn Taskbar<R:Runtime>(App:AppHandle<R>, Enabled:bool) -> Result<State, String> {
	crate::Trace!("click set_taskbar_rounded enabled={Enabled}");

	let mut Preferences:Settings = Load::Fn(&App)?;

	Preferences.TaskbarRounded = Enabled;

	Commit::Fn(&App, &Preferences)
}
