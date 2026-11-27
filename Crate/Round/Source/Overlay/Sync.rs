//! Creating, repositioning, and reconciling overlay windows across display
//! hotplugs (connecting/unplugging external monitors, resolution or DPI changes).

use tauri::{
	AppHandle,
	Manager,
	PhysicalPosition,
	PhysicalSize,
	Runtime,
	WebviewUrl,
	WebviewWindow,
	WebviewWindowBuilder,
};
use tauri_plugin_store::StoreExt;

use crate::{
	Backdrop::{Close as BackdropClose, Owns as BackdropOwns},
	Menu::Label as MenuLabel,
	Overlay::{
		Layer::Trait::Trait as _,
		Platform::{Maximize, Struct::Struct as Platform, Taskbar, Watch},
	},
	Per::Of as PerOf,
	Screen::{All as ScreenAll, Registry, Struct::Struct as Screen},
	Settings::{Broadcast, Load as SettingsLoad, Script, Store::Fn as StorePath, Struct::Struct as Settings},
	Visibility::{Apply as VisibilityApply, Enum::Enum as Visibility},
	Window::Chrome,
};

/// Build and position one transparent, click-through overlay window for `Screen`.
pub fn BuildOverlay<R:Runtime>(
	App:&AppHandle<R>,
	Preferences:&Settings,
	Screen:&Screen,
) -> tauri::Result<WebviewWindow<R>> {
	crate::Trace!(
		"window {} radius={:?} ink={:?}",
		Screen.Label,
		PerOf::Fn(&Preferences.Radius, &Screen.Label),
		PerOf::Fn(&Preferences.Ink, &Screen.Label)
	);

	let InitScript = Script::Fn(Preferences, Screen);

	let Width = u32::try_from(Screen.Pixels.0).unwrap_or(0);

	let Height = u32::try_from(Screen.Pixels.1).unwrap_or(0);

	let Builder = Chrome::Fn(
		WebviewWindowBuilder::new(App, &Screen.Label, WebviewUrl::App("index.html".into())),
		"",
	)
	.accept_first_mouse(false)
	.focused(false)
	.fullscreen(false)
	.initialization_script(&InitScript)
	.inner_size(Screen.Width.into(), Screen.Height.into())
	.maximized(false)
	.position(Screen.X.into(), Screen.Y.into())
	.disable_drag_drop_handler();

	// Windows draws its own shadow under a transparent window, which is a grey
	// halo around corners that are meant to be the only edge on screen.
	#[cfg(target_os = "windows")]
	let Builder = Builder.shadow(false);

	let Window = Builder.build()?;

	// Snap to exact physical monitor coordinates and dimensions to eliminate
	// fractional-DPI rounding gaps up to 8K displays.
	let _ = Window.set_position(PhysicalPosition::new(Screen.PhysicalX, Screen.PhysicalY));

	let _ = Window.set_size(PhysicalSize::new(Width, Height));

	Platform::Art(&Window);

	let WindowVis = match PerOf::Fn(&Preferences.Visible, &Screen.Label) {
		Visibility::Shown => Visibility::Shown,
		Visibility::Hidden if Preferences.TaskbarRounded => Visibility::Shown,
		Visibility::Hidden => Visibility::Hidden,
	};

	VisibilityApply::Fn(WindowVis, &Window)?;

	Ok(Window)
}

/// Reconcile overlay windows, registry state, and taskbar rims with the current
/// live monitor topology. Must run on the main thread.
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

	// Hide and close orphaned overlay windows belonging to disconnected monitors
	// so their WebView2 surfaces and stale taskbar rims cannot linger or crash.
	for Window in App.webview_windows().into_values() {
		let Label = Window.label().to_owned();

		if Label == MenuLabel::Const || BackdropOwns::Fn(&Label) {
			continue;
		}

		if !NextScreens.iter().any(|Screen| Screen.Label == Label) {
			let _ = Window.hide();
			let _ = Window.close();
			Maximize::Invalidate(&Label);
			Taskbar::InvalidateFrame(&Label);
		}
	}

	// Reposition/resize surviving overlays to exact new physical bounds and
	// create overlays for newly connected monitors.
	for Screen in &NextScreens {
		let Width = u32::try_from(Screen.Pixels.0).unwrap_or(0);

		let Height = u32::try_from(Screen.Pixels.1).unwrap_or(0);

		if let Some(Window) = App.get_webview_window(&Screen.Label) {
			let _ = Window.set_position(PhysicalPosition::new(Screen.PhysicalX, Screen.PhysicalY));
			let _ = Window.set_size(PhysicalSize::new(Width, Height));
			Platform::Art(&Window);
		} else if let Err(Error) = BuildOverlay(App, &Preferences, Screen) {
			eprintln!("radius: overlay window {} could not be rebuilt: {Error}", Screen.Label);
		}
	}

	Broadcast::Fn(&Preferences, App);

	Maximize::InvalidateAll();
	Taskbar::InvalidateAll();

	Watch::Sync(App, &Preferences);
}
