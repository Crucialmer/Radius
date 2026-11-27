//! WinEvent-driven watcher and hidden `WM_DISPLAYCHANGE` / `WM_SETTINGCHANGE`
//! listener that keeps overlays above the taskbar, dynamically reconciles
//! monitor hotplugs (connecting/unplugging external displays without crashes or
//! shifted taskbar rims), updates taskbar rounding + anti-aliased rims, and
//! tracks maximized and fullscreen windows with minimal idle overhead.

use std::{
	collections::HashMap,
	sync::{
		Mutex,
		OnceLock,
		atomic::{AtomicBool, AtomicU8, Ordering},
	},
};

use tauri::{AppHandle, Manager, Runtime};

use crate::{
	Backdrop::Owns,
	Menu::Label as MenuLabel,
	Overlay::{
		Layer::Trait::Trait as _,
		Platform::{Maximize, Struct::Struct as Platform, Taskbar},
		Sync as OverlaySync,
	},
	Screen::Registry,
	Settings::Struct::Struct as Settings,
};

static LatestSettings:Mutex<Option<Settings>> = Mutex::new(None);

static LastReports:Mutex<Option<HashMap<String, Maximize::MonitorReport>>> = Mutex::new(None);

#[cfg(target_os = "windows")]
type DisplayTuple = (i32, i32, i32, i32, u32);

#[cfg(target_os = "windows")]
static LastDisplaySig:Mutex<Option<Vec<DisplayTuple>>> = Mutex::new(None);

static SyncCallback:OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

static ReconcileCallback:OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

static Started:AtomicBool = AtomicBool::new(false);

static NeedsZOrder:AtomicBool = AtomicBool::new(true);

static NeedsMaximize:AtomicBool = AtomicBool::new(true);

static NeedsTaskbar:AtomicBool = AtomicBool::new(true);

static NeedsReconcile:AtomicBool = AtomicBool::new(false);

static DisplayDebounceTicks:AtomicU8 = AtomicU8::new(0);

static BurstTicks:AtomicU8 = AtomicU8::new(0);

/// Update the in-memory settings snapshot and immediately synchronize Windows
/// taskbar rounding, Z-order, and maximized-window corners.
pub fn Sync<R:Runtime>(App:&AppHandle<R>, Preferences:&Settings) {
	*LatestSettings.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner()) = Some(Preferences.clone());

	NeedsZOrder.store(true, Ordering::Release);
	NeedsMaximize.store(true, Ordering::Release);
	NeedsTaskbar.store(true, Ordering::Release);

	RunOnce(App);
}

/// Re-emit current maximized/work-area/taskbar state for one overlay window
/// after its webview finishes attaching event listeners or resizes across DPIs.
pub fn Resync<R:Runtime>(App:&AppHandle<R>, Label:&str) {
	Maximize::Invalidate(Label);
	Taskbar::InvalidateFrame(Label);

	NeedsMaximize.store(true, Ordering::Release);
	NeedsTaskbar.store(true, Ordering::Release);

	RunOnce(App);
}

/// Schedule a debounced main-thread display reconciliation (safe to call from
/// Win32 message handlers or watcher checks).
pub fn ScheduleReconcile() {
	NeedsReconcile.store(true, Ordering::Release);
	DisplayDebounceTicks.store(4, Ordering::Release);
}

/// Start the background WinEvent hook, display-change listener, and lightweight
/// Z-order/window watcher once.
pub fn Start<R:Runtime>(App:AppHandle<R>, Initial:&Settings) {
	*LatestSettings.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner()) = Some(Initial.clone());

	#[cfg(target_os = "windows")]
	{
		*LastDisplaySig.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner()) = Some(DisplaySignature());
	}

	let SyncHandle = App.clone();

	let _ = SyncCallback.set(Box::new(move || {
		RunOnce(&SyncHandle);
	}));

	let ReconcileHandle = App.clone();

	let _ = ReconcileCallback.set(Box::new(move || {
		let MainHandle = ReconcileHandle.clone();

		let _ = ReconcileHandle.run_on_main_thread(move || {
			OverlaySync::Reconcile(&MainHandle);
		});
	}));

	RunOnce(&App);

	if Started.swap(true, Ordering::AcqRel) {
		return;
	}

	#[cfg(target_os = "windows")]
	std::thread::spawn(move || {
		RunMessageLoop();
	});
}

fn RunOnce<R:Runtime>(App:&AppHandle<R>) {
	// Never race against Windows while a display plug/unplug transition is settling.
	if DisplayDebounceTicks.load(Ordering::Acquire) > 0 {
		return;
	}

	let Preferences = LatestSettings
		.lock()
		.unwrap_or_else(|Poisoned| Poisoned.into_inner())
		.clone()
		.unwrap_or_default();

	let Screens = App
		.try_state::<Registry::Struct>()
		.map(|Held| Held.Snapshot())
		.unwrap_or_default();

	#[cfg(target_os = "windows")]
	if NeedsZOrder.swap(false, Ordering::AcqRel) || BurstTicks.load(Ordering::Acquire) > 0 {
		EnsureOverlaysAboveTaskbar(App, &Screens);
	}

	let Reports = if NeedsMaximize.swap(false, Ordering::AcqRel) {
		let (Next, Drifted) = Maximize::Refresh(App, &Screens, Preferences.MaximizeCorners, Preferences.TaskbarRounded);

		if Drifted {
			ScheduleReconcile();
		}

		let mut Guard = LastReports.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner());

		if Guard.as_ref() != Some(&Next) {
			NeedsTaskbar.store(true, Ordering::Release);
			*Guard = Some(Next.clone());
		}

		Next
	} else {
		LastReports
			.lock()
			.unwrap_or_else(|Poisoned| Poisoned.into_inner())
			.clone()
			.unwrap_or_default()
	};

	if NeedsTaskbar.swap(false, Ordering::AcqRel) {
		Taskbar::Apply(App, Preferences.TaskbarRounded, Preferences.TaskbarRadius, &Screens, &Reports);
	}
}

#[cfg(target_os = "windows")]
fn DisplaySignature() -> Vec<DisplayTuple> {
	use std::ptr;

	use windows_sys::Win32::{
		Foundation::{LPARAM, RECT},
		Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO},
		UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
	};

	unsafe extern "system" fn Collect(Hmon:HMONITOR, _Hdc:HDC, _Rect:*mut RECT, Param:LPARAM) -> i32 {
		unsafe {
			let Out = &mut *(Param as *mut Vec<DisplayTuple>);

			let mut Info:MONITORINFO = std::mem::zeroed();

			Info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;

			if GetMonitorInfoW(Hmon, &mut Info) != 0 {
				let mut DpiX:u32 = 96;

				let mut DpiY:u32 = 96;

				let _ = GetDpiForMonitor(Hmon, MDT_EFFECTIVE_DPI, &mut DpiX, &mut DpiY);

				Out.push((
					Info.rcMonitor.left,
					Info.rcMonitor.top,
					Info.rcMonitor.right,
					Info.rcMonitor.bottom,
					DpiX,
				));
			}

			1
		}
	}

	let mut List:Vec<DisplayTuple> = Vec::with_capacity(4);

	unsafe {
		let _ = EnumDisplayMonitors(ptr::null_mut(), ptr::null(), Some(Collect), &mut List as *mut _ as LPARAM);
	}

	List.sort_unstable();

	List
}

#[cfg(target_os = "windows")]
fn EnsureOverlaysAboveTaskbar<R:Runtime>(App:&AppHandle<R>, Screens:&[crate::Screen::Struct::Struct]) {
	use windows_sys::Win32::{
		Foundation::{HWND, RECT},
		UI::WindowsAndMessaging::{
			GW_HWNDPREV,
			GetWindow,
			GetWindowRect,
			HWND_TOPMOST,
			IsWindow,
			IsWindowVisible,
			SWP_NOACTIVATE,
			SWP_NOMOVE,
			SWP_NOSIZE,
			SetWindowPos,
		},
	};

	let Taskbars = Taskbar::FindVisible();

	let Windows = App.webview_windows();

	let mut Reordered = false;

	for Window in Windows.values() {
		let Label = Window.label();

		if Label == MenuLabel::Const || Owns::Fn(Label) {
			continue;
		}

		// Skip orphaned windows whose monitor has already been removed.
		let Some(Screen) = Screens.iter().find(|Item| Item.Label == Label) else {
			continue;
		};

		let Ok(Raw) = Window.hwnd() else {
			continue;
		};

		let OverlayHwnd = Raw.0 as HWND;

		if OverlayHwnd.is_null() {
			continue;
		}

		unsafe {
			if IsWindow(OverlayHwnd) == 0 || IsWindowVisible(OverlayHwnd) == 0 {
				continue;
			}

			// Self-heal if a cross-DPI `WM_DPICHANGED` transition silently shifted or
			// resized this overlay away from its monitor's exact physical bounds.
			let mut WinRect = RECT { left:0, top:0, right:0, bottom:0 };

			if GetWindowRect(OverlayHwnd, &mut WinRect) != 0 {
				let CurW = WinRect.right - WinRect.left;

				let CurH = WinRect.bottom - WinRect.top;

				if (WinRect.left - Screen.PhysicalX).abs() > 2
					|| (WinRect.top - Screen.PhysicalY).abs() > 2
					|| (CurW - Screen.Pixels.0).abs() > 2
					|| (CurH - Screen.Pixels.1).abs() > 2
				{
					let SnapApp = App.clone();

					let SnapLabel = Label.to_owned();

					let _ = App.run_on_main_thread(move || {
						OverlaySync::Snap(&SnapApp, &SnapLabel);
					});
				}
			}

			if Taskbars.is_empty() {
				continue;
			}

			let mut AboveTaskbar = true;

			let mut Prev = GetWindow(OverlayHwnd, GW_HWNDPREV);

			while !Prev.is_null() {
				if Taskbars.contains(&Prev) {
					AboveTaskbar = false;
					break;
				}

				Prev = GetWindow(Prev, GW_HWNDPREV);
			}

			if !AboveTaskbar {
				SetWindowPos(OverlayHwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);

				Reordered = true;
			}
		}
	}

	if Reordered
		&& let Some(Menu) = Windows.get(MenuLabel::Const)
		&& let Ok(Raw) = Menu.hwnd()
		&& !Raw.0.is_null()
		&& unsafe { IsWindow(Raw.0 as HWND) != 0 && IsWindowVisible(Raw.0 as HWND) != 0 }
	{
		for Window in Windows.values() {
			if Owns::Fn(Window.label())
				&& let Ok(ScrimRaw) = Window.hwnd()
				&& !ScrimRaw.0.is_null()
				&& unsafe { IsWindow(ScrimRaw.0 as HWND) != 0 && IsWindowVisible(ScrimRaw.0 as HWND) != 0 }
			{
				Platform::Scrim(Window);
			}
		}

		Platform::Panel(Menu);
	}
}

#[cfg(target_os = "windows")]
fn RunMessageLoop() {
	use std::{ptr, sync::atomic::AtomicU16};

	use windows_sys::Win32::{
		Foundation::{HWND, LPARAM, LRESULT, WPARAM},
		UI::{
			Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent},
			WindowsAndMessaging::{
				CreateWindowExW,
				DefWindowProcW,
				DestroyWindow,
				DispatchMessageW,
				EVENT_OBJECT_CLOAKED,
				EVENT_OBJECT_HIDE,
				EVENT_OBJECT_LOCATIONCHANGE,
				EVENT_OBJECT_SHOW,
				EVENT_OBJECT_UNCLOAKED,
				EVENT_SYSTEM_FOREGROUND,
				EVENT_SYSTEM_MINIMIZEEND,
				GetMessageW,
				MSG,
				RegisterClassExW,
				SetTimer,
				TranslateMessage,
				WINEVENT_OUTOFCONTEXT,
				WINEVENT_SKIPOWNPROCESS,
				WM_DISPLAYCHANGE,
				WM_SETTINGCHANGE,
				WNDCLASSEXW,
				WS_EX_NOACTIVATE,
				WS_EX_TOOLWINDOW,
				WS_POPUP,
			},
		},
	};

	unsafe extern "system" fn OnDisplayWindowProc(Hwnd:HWND, Msg:u32, WParam:WPARAM, LParam:LPARAM) -> LRESULT {
		match Msg {
			WM_DISPLAYCHANGE => {
				ScheduleReconcile();
			}
			WM_SETTINGCHANGE => {
				NeedsMaximize.store(true, Ordering::Release);
				NeedsTaskbar.store(true, Ordering::Release);
				BurstTicks.store(4, Ordering::Release);
			}
			_ => {}
		}

		unsafe { DefWindowProcW(Hwnd, Msg, WParam, LParam) }
	}

	unsafe extern "system" fn OnWinEvent(
		_Hook:HWINEVENTHOOK,
		Event:u32,
		_Hwnd:HWND,
		IdObject:i32,
		IdChild:i32,
		_Thread:u32,
		_Time:u32,
	) {
		// Ignore non-window sub-element events (OBJID_WINDOW == 0, CHILDID_SELF == 0).
		if Event >= EVENT_OBJECT_SHOW && (IdObject != 0 || IdChild != 0) {
			return;
		}

		NeedsMaximize.store(true, Ordering::Release);

		if Event == EVENT_OBJECT_LOCATIONCHANGE {
			NeedsTaskbar.store(true, Ordering::Release);
			return;
		}

		NeedsZOrder.store(true, Ordering::Release);
		NeedsTaskbar.store(true, Ordering::Release);

		if Event <= EVENT_SYSTEM_MINIMIZEEND {
			BurstTicks.store(4, Ordering::Release);
		}
	}

	static IdleTicks:AtomicU16 = AtomicU16::new(0);

	unsafe extern "system" fn OnTimer(_Hwnd:HWND, _Msg:u32, _Id:usize, _Time:u32) {
		// First check if a display hotplug / DPI transition debounce is counting down.
		let Debounce = DisplayDebounceTicks.load(Ordering::Acquire);

		if Debounce > 0 {
			let Next = Debounce - 1;

			DisplayDebounceTicks.store(Next, Ordering::Release);

			if Next == 0 && NeedsReconcile.swap(false, Ordering::AcqRel) {
				let Sig = DisplaySignature();

				if Sig.is_empty() {
					// Still mid-transition; wait another short debounce cycle.
					ScheduleReconcile();
					return;
				}

				*LastDisplaySig.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner()) = Some(Sig);

				if let Some(Callback) = ReconcileCallback.get() {
					Callback();
				}

				// Follow up for ~2.4 s so delayed Explorer taskbar creation and
				// secondary EDID/DPI settlement after `WM_DISPLAYCHANGE` are caught.
				BurstTicks.store(20, Ordering::Release);
				IdleTicks.store(0, Ordering::Release);
			}

			return;
		}

		let Remaining = BurstTicks.load(Ordering::Acquire);

		let ShouldRun = if Remaining > 0 {
			BurstTicks.store(Remaining - 1, Ordering::Release);

			if Remaining == 1 {
				let Sig = DisplaySignature();

				let Changed = {
					let Guard = LastDisplaySig.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner());
					!Sig.is_empty() && Guard.as_ref() != Some(&Sig)
				};

				if Changed {
					ScheduleReconcile();
					return;
				}
			}

			NeedsZOrder.store(true, Ordering::Release);
			NeedsMaximize.store(true, Ordering::Release);
			NeedsTaskbar.store(true, Ordering::Release);
			IdleTicks.store(0, Ordering::Release);
			true
		} else if NeedsZOrder.load(Ordering::Acquire)
			|| NeedsMaximize.load(Ordering::Acquire)
			|| NeedsTaskbar.load(Ordering::Acquire)
		{
			IdleTicks.store(0, Ordering::Release);
			true
		} else {
			let Next = IdleTicks.fetch_add(1, Ordering::AcqRel) + 1;

			// Periodic ~4.8 s reconciliation (40 * 120 ms) to catch Explorer restarts,
			// shell resets, or missed display topology changes with negligible CPU.
			if Next >= 40 {
				IdleTicks.store(0, Ordering::Release);

				let Sig = DisplaySignature();

				let Changed = {
					let Guard = LastDisplaySig.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner());
					!Sig.is_empty() && Guard.as_ref() != Some(&Sig)
				};

				if Changed {
					ScheduleReconcile();
					return;
				}

				NeedsZOrder.store(true, Ordering::Release);
				NeedsMaximize.store(true, Ordering::Release);
				NeedsTaskbar.store(true, Ordering::Release);
				true
			} else {
				false
			}
		};

		if ShouldRun && let Some(Callback) = SyncCallback.get() {
			Callback();
		}
	}

	unsafe {
		// Hidden top-level listener window receives broadcast `WM_DISPLAYCHANGE`
		// and `WM_SETTINGCHANGE` notifications immediately on monitor plug/unplug.
		let ClassName:Vec<u16> = "RadiusDisplayWatch\0".encode_utf16().collect();

		let Class = WNDCLASSEXW {
			cbSize:std::mem::size_of::<WNDCLASSEXW>() as u32,
			style:0,
			lpfnWndProc:Some(OnDisplayWindowProc),
			cbClsExtra:0,
			cbWndExtra:0,
			hInstance:ptr::null_mut(),
			hIcon:ptr::null_mut(),
			hCursor:ptr::null_mut(),
			hbrBackground:ptr::null_mut(),
			lpszMenuName:ptr::null(),
			lpszClassName:ClassName.as_ptr(),
			hIconSm:ptr::null_mut(),
		};

		let _ = RegisterClassExW(&Class);

		let ListenerHwnd = CreateWindowExW(
			WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
			ClassName.as_ptr(),
			ptr::null(),
			WS_POPUP,
			0,
			0,
			0,
			0,
			ptr::null_mut(),
			ptr::null_mut(),
			ptr::null_mut(),
			ptr::null(),
		);

		let HookSystem = SetWinEventHook(
			EVENT_SYSTEM_FOREGROUND,
			EVENT_SYSTEM_MINIMIZEEND,
			ptr::null_mut(),
			Some(OnWinEvent),
			0,
			0,
			WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
		);

		let HookShowHide = SetWinEventHook(
			EVENT_OBJECT_SHOW,
			EVENT_OBJECT_HIDE,
			ptr::null_mut(),
			Some(OnWinEvent),
			0,
			0,
			WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
		);

		let HookLocation = SetWinEventHook(
			EVENT_OBJECT_LOCATIONCHANGE,
			EVENT_OBJECT_LOCATIONCHANGE,
			ptr::null_mut(),
			Some(OnWinEvent),
			0,
			0,
			WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
		);

		let HookCloak = SetWinEventHook(
			EVENT_OBJECT_CLOAKED,
			EVENT_OBJECT_UNCLOAKED,
			ptr::null_mut(),
			Some(OnWinEvent),
			0,
			0,
			WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
		);

		let _Timer = SetTimer(ptr::null_mut(), 0, 120, Some(OnTimer));

		let mut Msg:MSG = std::mem::zeroed();

		while GetMessageW(&mut Msg, ptr::null_mut(), 0, 0) > 0 {
			TranslateMessage(&Msg);
			DispatchMessageW(&Msg);
		}

		if !HookSystem.is_null() {
			UnhookWinEvent(HookSystem);
		}

		if !HookShowHide.is_null() {
			UnhookWinEvent(HookShowHide);
		}

		if !HookLocation.is_null() {
			UnhookWinEvent(HookLocation);
		}

		if !HookCloak.is_null() {
			UnhookWinEvent(HookCloak);
		}

		if !ListenerHwnd.is_null() {
			let _ = DestroyWindow(ListenerHwnd);
		}
	}
}
