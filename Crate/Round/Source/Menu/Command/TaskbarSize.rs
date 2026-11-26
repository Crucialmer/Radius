//! Setting the radius the taskbar is clipped with.

use tauri::{AppHandle, Runtime};

use crate::{
	Menu::{
		State::Struct as State,
		Store::{Commit, Load},
	},
	Radius::Struct::Struct as Radius,
	Settings::Struct::Struct as Settings,
	Taskbar::Maximum as TaskbarMaximum,
};

/// Set the corner radius every taskbar is clipped with.
///
/// A command of its own rather than a second meaning for `set_size`: the screen's
/// corners and the bar's clip are two looks, and a gesture that changed one should
/// not drag the other with it - which is exactly what the first release did and
/// what made a 15 px screen radius a capsule-shaped taskbar.
#[tauri::command(rename = "set_taskbar_size")]
pub fn TaskbarSize<R:Runtime>(App:AppHandle<R>, Value:i64) -> Result<State, String> {
	crate::Trace!("click set_taskbar_size value={Value}");

	let mut Preferences:Settings = Load::Fn(&App)?;

	Preferences.TaskbarRadius = Radius::from(Value.clamp(0, TaskbarMaximum::Const));

	Commit::Fn(&App, &Preferences)
}
