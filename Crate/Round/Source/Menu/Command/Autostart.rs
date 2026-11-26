//! Toggling Windows startup launch.

use tauri::{AppHandle, Runtime};

use crate::{
	Autostart::Set as AutostartSet,
	Menu::{Command::State as CommandState, State::Struct as State},
};

/// Register or remove Radius from Windows startup (`HKCU\...\Run`).
#[tauri::command(rename = "set_autostart")]
pub fn Autostart<R:Runtime>(App:AppHandle<R>, Enabled:bool) -> Result<State, String> {
	crate::Trace!("click set_autostart enabled={Enabled}");

	AutostartSet::Fn(Enabled)?;

	CommandState::State(App)
}
