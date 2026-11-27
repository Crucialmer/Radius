//! Creating, repositioning, and reconciling overlay surfaces across display
//! hotplugs (connecting/unplugging external monitors, resolution or DPI changes).

use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_store::StoreExt;

use crate::{
	Backdrop::Close as BackdropClose,
	Menu::Label as MenuLabel,
	Overlay::Platform::{Maximize, Taskbar, Watch},
	Screen::{All as ScreenAll, Registry},
	Settings::{Broadcast, Load as SettingsLoad, Store::Fn as StorePath},
};

/// Reconcile native Direct2D/DirectComposition overlay surfaces, registry state,
/// and taskbar rims with the current live monitor topology.
pub fn Reconcile<R:Runtime>(App:&AppHandle<R>) {
	let Ok(Devices) = App.available_monitors() else {
		return;
	};

	// During HDMI/USB-C unplug transitions Windows can transiently report zero
	// monitors while switching display outputs; wait for the next settle tick.
	if Devices.is_empty() {
		return;
	}

	let NextScreens = ScreenAll::Fn(&Devices);

	if let Some(Held) = App.try_state::<Registry::Struct>() {
		Held.Replace(NextScreens.clone());
	}

	let Preferences = App
		.store(StorePath())
		.map(|Storage| SettingsLoad::Fn(&Storage, &NextScreens))
		.unwrap_or_default();

	// Dismiss open menu/scrims so they are never stranded off-screen after a
	// display unplug or layout shift.
	BackdropClose::Fn(App);

	if let Some(Menu) = App.get_webview_window(MenuLabel::Const) {
		let _ = Menu.hide();
	}

	#[cfg(target_os = "windows")]
	crate::Overlay::Platform::Engine::SyncSettings(&Preferences, &NextScreens);

	Broadcast::Fn(&Preferences, App);

	Maximize::InvalidateAll();
	Taskbar::InvalidateAll();

	Watch::Sync(App, &Preferences);
}
