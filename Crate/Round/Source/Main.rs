#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// House style: locals and parameters are PascalCase, the same as the library's.
#![allow(non_snake_case)]

//! The `round` binary: one overlay webview per monitor, then the tray icon.

use pround::{
	Overlay::{Layer::Trait::Trait as _, Platform::Struct::Struct as Platform, Sync as OverlaySync},
	Screen::{All as ScreenAll, Registry},
	Settings::{Load as SettingsLoad, Save as SettingsSave, Store::Fn as StorePath},
	Tray::{Install, Remove as TrayRemove},
};
use tauri::Manager;
use tauri_plugin_store::StoreExt;

/// Session-scoped Windows named mutex guard ensuring only one `Radius.exe`
/// instance runs per user session, preventing duplicate tray icons and
/// competing overlay windows.
#[cfg(target_os = "windows")]
struct SingleInstance(windows_sys::Win32::Foundation::HANDLE);

#[cfg(target_os = "windows")]
impl Drop for SingleInstance {
	fn drop(&mut self) {
		if !self.0.is_null() {
			unsafe {
				let _ = windows_sys::Win32::Foundation::CloseHandle(self.0);
			}
		}
	}
}

#[cfg(target_os = "windows")]
fn AcquireSingleInstance() -> Option<SingleInstance> {
	use std::ptr;

	use windows_sys::Win32::{
		Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError},
		System::Threading::CreateMutexW,
	};

	let Name:Vec<u16> = "Local\\Crucialmer.Radius.SingleInstance\0".encode_utf16().collect();

	unsafe {
		let Handle = CreateMutexW(ptr::null(), 1, Name.as_ptr());

		if Handle.is_null() {
			// If mutex creation unexpectedly fails, allow launch rather than locking out the user.
			return Some(SingleInstance(ptr::null_mut()));
		}

		if GetLastError() == ERROR_ALREADY_EXISTS {
			let _ = CloseHandle(Handle);
			return None;
		}

		Some(SingleInstance(Handle))
	}
}

fn main() {
	// Ensure at most one Radius instance runs in the current Windows session.
	#[cfg(target_os = "windows")]
	let Some(_InstanceGuard) = AcquireSingleInstance() else {
		return;
	};

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
		.run(|App, Event| match Event {
			// Closing an orphaned overlay window during external-monitor unplug
			// must never terminate the tray application; only explicit `App.exit(0)`
			// (`code.is_some()`) from Quit or the self-updater exits the process.
			tauri::RunEvent::ExitRequested { api, code, .. } if code.is_none() => {
				api.prevent_exit();
			}
			tauri::RunEvent::Exit => {
				pround::Overlay::Platform::Taskbar::Restore();
				TrayRemove::Fn(App);
			}
			_ => {}
		});
}
