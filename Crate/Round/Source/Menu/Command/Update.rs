//! Checking, downloading, and opening releases from the tray panel.

use tauri::{AppHandle, Runtime};

use crate::{
	Menu::{Command::State as CommandState, State::Struct as State},
	Update::{Check, Download, Open, Store},
};

/// Trigger a background check against GitHub Releases.
#[tauri::command(rename = "check_update")]
pub fn CheckUpdate<R:Runtime>(App:AppHandle<R>) -> Result<State, String> {
	crate::Trace!("click check_update");

	Check::Fn(&App);

	CommandState::State(App)
}

/// Download the latest release asset, report live progress, and restart into it.
#[tauri::command(rename = "download_update")]
pub fn DownloadUpdate<R:Runtime>(App:AppHandle<R>) -> Result<State, String> {
	crate::Trace!("click download_update");

	Download::Fn(&App)?;

	CommandState::State(App)
}

/// Open the latest GitHub release page in the default browser.
#[tauri::command(rename = "open_release")]
pub fn OpenRelease<R:Runtime>(App:AppHandle<R>) -> Result<(), String> {
	crate::Trace!("click open_release");

	let Info = Store::Read(&App);

	Open::Fn(Info.ReleaseUrl.as_deref())
}
