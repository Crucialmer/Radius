#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// House style: locals and parameters are PascalCase, the same as the library's.
#![allow(non_snake_case)]

//! The `round` binary: one overlay webview per monitor, then the tray icon.

use pround::{
	Overlay::{Layer::Trait::Trait as _, Platform::Struct::Struct as Platform, Sync as OverlaySync},
	Screen::{All as ScreenAll, Registry},
	Settings::{Load as SettingsLoad, Save as SettingsSave, Store::Fn as StorePath},
	Tray::Install,
};
use tauri::Manager;
use tauri_plugin_store::StoreExt;

fn main() {
	// Pure green portable runtime on Windows: redirect WebView2's user data
	// folder into `./data/webview` beside the binary before any thread or
	// webview subsystem starts, preventing writes to `%LOCALAPPDATA%`.
	#[cfg(target_os = "windows")]
	{
		let DataDir = pround::Settings::Store::Dir().join("data");

		// Clean up leftover staging files from a completed self-update.
		let _ = std::fs::remove_dir_all(DataDir.join("update"));

		let WebViewDir = DataDir.join("webview");

		// SAFETY: Called at the very beginning of `main` on the initial thread
		// before `tauri::Builder` spawns any worker threads.
		unsafe { std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", WebViewDir) };
	}

	tauri::Builder::default()
		.plugin(tauri_plugin_store::Builder::new().build())
		.invoke_handler(pround::Commands!())
		.setup(|App| {
			let Storage = App.store(StorePath())?;

			// The monitors come before the settings: every override is keyed by
			// the label a monitor is given, and a label is unique only against
			// the rest of the set, so the set is what decides where a stored
			// setting lands.
			let Devices = App.available_monitors()?;

			let Screens = ScreenAll::Fn(&Devices);

			// The one live set every command reads a label from. Updated
			// atomically on display hotplug by `Overlay::Sync::Reconcile`.
			App.manage(Registry::Struct::New(Screens.clone()));

			App.manage(pround::Update::Store::Struct::default());

			// Read, then write back: that round trip is how a fresh install gets
			// its defaults on disk before anything reads them again, and how a
			// store an older build wrote is folded into the per-monitor shape
			// once, at launch, instead of on every command.
			let Preferences = SettingsLoad::Fn(&Storage, &Screens);

			SettingsSave::Fn(&Preferences, &Storage)?;

			// macOS: the activation policy is process-wide, so it is applied
			// before any window exists.
			Platform::Dock(Preferences.Dock);

			Screens
				.iter()
				.try_for_each(|Screen| OverlaySync::BuildOverlay(App.handle(), &Preferences, Screen).map(|_| ()))?;

			Install::Fn(App.handle())?;

			pround::Overlay::Platform::Watch::Start(App.handle().clone(), &Preferences);

			pround::Update::Check::Fn(App.handle());

			Ok(())
		})
		.build(tauri::generate_context!())
		.expect("Cannot launch Radius.")
		.run(|_App, Event| match Event {
			// Closing an orphaned overlay window during external-monitor unplug
			// must never terminate the tray application; only explicit `App.exit(0)`
			// (`code.is_some()`) from Quit or the self-updater exits the process.
			tauri::RunEvent::ExitRequested { api, code, .. } if code.is_none() => {
				api.prevent_exit();
			}
			tauri::RunEvent::Exit => {
				pround::Overlay::Platform::Taskbar::Restore();
			}
			_ => {}
		});
}
