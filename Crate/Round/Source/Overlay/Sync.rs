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

/// Snap an overlay window to exact physical monitor coordinates & dimensions
/// while stripping DWM shadows/borders and syncing WebView2 controller bounds
/// even across cross-DPI `WM_DPICHANGED` transitions.
fn SnapWindow<R:Runtime>(Window:&WebviewWindow<R>, PhysicalX:i32, PhysicalY:i32, WidthPx:i32, HeightPx:i32) {
	let Width = u32::try_from(WidthPx).unwrap_or(0);

	let Height = u32::try_from(HeightPx).unwrap_or(0);

	if Width == 0 || Height == 0 {
		return;
	}

	let _ = Window.set_shadow(false);

	let _ = Window.set_position(PhysicalPosition::new(PhysicalX, PhysicalY));

	let _ = Window.set_size(PhysicalSize::new(Width, Height));

	Platform::Art(Window);

	#[cfg(target_os = "windows")]
	if let Ok(Raw) = Window.hwnd() {
		use windows_sys::Win32::{
			Foundation::HWND,
			UI::WindowsAndMessaging::{HWND_TOPMOST, SWP_FRAMECHANGED, SWP_NOACTIVATE, SetWindowPos},
		};

		let Hwnd = Raw.0 as HWND;

		if !Hwnd.is_null() {
			unsafe {
				SetWindowPos(
					Hwnd,
					HWND_TOPMOST,
					PhysicalX,
					PhysicalY,
					WidthPx,
					HeightPx,
					SWP_NOACTIVATE | SWP_FRAMECHANGED,
				);
			}
		}
	}

	// Re-apply physical size after cross-DPI `WM_DPICHANGED` has landed so Wry's
	// WebView2 controller bounds match the exact monitor dimensions.
	let _ = Window.set_size(PhysicalSize::new(Width, Height));
}

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

	SnapWindow(&Window, Screen.PhysicalX, Screen.PhysicalY, Screen.Pixels.0, Screen.Pixels.1);

	let WindowVis = match PerOf::Fn(&Preferences.Visible, &Screen.Label) {
		Visibility::Shown => Visibility::Shown,
		Visibility::Hidden if Preferences.TaskbarRounded => Visibility::Shown,
		Visibility::Hidden => Visibility::Hidden,
	};

	VisibilityApply::Fn(WindowVis, &Window)?;

	// Re-verify bounds and frame suppression once shown so Windows 11 DWM never
	// attaches a rectangular border or shadow after visibility transitions.
	SnapWindow(&Window, Screen.PhysicalX, Screen.PhysicalY, Screen.Pixels.0, Screen.Pixels.1);

	Ok(Window)
}

/// Re-snap a single live overlay window whose Win32 rect drifted from its
/// monitor (for example due to a delayed `WM_DPICHANGED` after monitor plug-in).
/// Must run on the main thread.
pub fn Snap<R:Runtime>(App:&AppHandle<R>, Label:&str) {
	let Some(Window) = App.get_webview_window(Label) else {
		return;
	};

	let Screens = App
		.try_state::<Registry::Struct>()
		.map(|Held| Held.Snapshot())
		.unwrap_or_default();

	let Some(Screen) = Screens.iter().find(|Item| Item.Label == Label) else {
		return;
	};

	SnapWindow(&Window, Screen.PhysicalX, Screen.PhysicalY, Screen.Pixels.0, Screen.Pixels.1);

	Watch::Resync(App, Label);
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

	// Immediately hide and destroy orphaned overlay windows belonging to
	// disconnected monitors so their labels, HWNDs, and WebView2 surfaces are
	// released cleanly before the monitor can be re-plugged.
	for Window in App.webview_windows().into_values() {
		let Label = Window.label().to_owned();

		if Label == MenuLabel::Const || BackdropOwns::Fn(&Label) {
			continue;
		}

		if !NextScreens.iter().any(|Screen| Screen.Label == Label) {
			let _ = Window.hide();
			let _ = Window.destroy();
			Maximize::Invalidate(&Label);
			Taskbar::InvalidateFrame(&Label);
		}
	}

	// Reposition/resize surviving overlays to exact new physical bounds and
	// create overlays for newly connected monitors.
	for Screen in &NextScreens {
		if let Some(Window) = App.get_webview_window(&Screen.Label) {
			SnapWindow(&Window, Screen.PhysicalX, Screen.PhysicalY, Screen.Pixels.0, Screen.Pixels.1);
		} else if let Err(Error) = BuildOverlay(App, &Preferences, Screen) {
			eprintln!("radius: overlay window {} could not be rebuilt: {Error}", Screen.Label);
		}
	}

	Broadcast::Fn(&Preferences, App);

	Maximize::InvalidateAll();
	Taskbar::InvalidateAll();

	Watch::Sync(App, &Preferences);
}
