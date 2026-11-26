//! Thread-safe storage for the latest update check result.

use std::sync::Mutex;

use tauri::{AppHandle, Manager, Runtime};

use crate::Update::Struct::Struct as Info;

/// Managed Tauri state holding the latest release check result.
#[derive(Default)]
pub struct Struct(pub Mutex<Info>);

/// Read a snapshot of the current update state.
pub fn Read<R:Runtime>(App:&AppHandle<R>) -> Info {
	App.try_state::<Struct>()
		.map(|Held| Held.0.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner()).clone())
		.unwrap_or_default()
}
