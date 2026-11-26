//! WinEvent-driven watcher that keeps overlays above the taskbar (fixing the Win-key
//! Z-order bug), updates taskbar rounding + anti-aliased rims, and tracks maximized
//! and fullscreen windows with minimal idle overhead.

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
	},
	Screen::Registry,
	Settings::Struct::Struct as Settings,
};

static LatestSettings:Mutex<Option<Settings>> = Mutex::new(None);

static LastReports:Mutex<Option<HashMap<String, Maximize::MonitorReport>>> = Mutex::new(None);

static SyncCallback:OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

static Started:AtomicBool = AtomicBool::new(false);

static NeedsZOrder:AtomicBool = AtomicBool::new(true);

static NeedsMaximize:AtomicBool = AtomicBool::new(true);

static NeedsTaskbar:AtomicBool = AtomicBool::new(true);

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
/// after its webview finishes attaching event listeners.
pub fn Resync<R:Runtime>(App:&AppHandle<R>, Label:&str) {
	Maximize::Invalidate(Label);
	Taskbar::InvalidateFrame(Label);

	NeedsMaximize.store(true, Ordering::Release);
	NeedsTaskbar.store(true, Ordering::Release);

	RunOnce(App);
}

/// Start the background WinEvent hook and lightweight Z-order/window watcher once.
pub fn Start<R:Runtime>(App:AppHandle<R>, Initial:&Settings) {
	*LatestSettings.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner()) = Some(Initial.clone());

	let Handle = App.clone();

	let _ = SyncCallback.set(Box::new(move || {
		RunOnce(&Handle);
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
	let Preferences = LatestSettings
		.lock()
		.unwrap_or_else(|Poisoned| Poisoned.into_inner())
		.clone()
		.unwrap_or_default();

	let Screens = App
		.try_state::<Registry::Struct>()
		.map(|Held| Held.0.clone())
		.unwrap_or_default();

	#[cfg(target_os = "windows")]
	if NeedsZOrder.swap(false, Ordering::AcqRel) || BurstTicks.load(Ordering::Acquire) > 0 {
		EnsureOverlaysAboveTaskbar(App);
	}

	let Reports = if NeedsMaximize.swap(false, Ordering::AcqRel) {
		let Next = Maximize::Refresh(App, &Screens, Preferences.MaximizeCorners, Preferences.TaskbarRounded);

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
fn EnsureOverlaysAboveTaskbar<R:Runtime>(App:&AppHandle<R>) {
	use windows_sys::Win32::{
		Foundation::HWND,
		UI::WindowsAndMessaging::{
			GW_HWNDPREV,
			GetWindow,
			HWND_TOPMOST,
			SWP_NOACTIVATE,
			SWP_NOMOVE,
			SWP_NOSIZE,
			SetWindowPos,
		},
	};

	let Taskbars = Taskbar::FindVisible();

	if Taskbars.is_empty() {
		return;
	}

	let Windows = App.webview_windows();

	let mut Reordered = false;

	for Window in Windows.values() {
		let Label = Window.label();

		if Label == MenuLabel::Const || Owns::Fn(Label) {
			continue;
		}

		if !Window.is_visible().unwrap_or(false) {
			continue;
		}

		let Ok(Raw) = Window.hwnd() else {
			continue;
		};

		let OverlayHwnd = Raw.0 as HWND;

		if OverlayHwnd.is_null() {
			continue;
		}

		let mut AboveTaskbar = true;

		unsafe {
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
		&& Menu.is_visible().unwrap_or(false)
	{
		for Window in Windows.values() {
			if Owns::Fn(Window.label()) && Window.is_visible().unwrap_or(false) {
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
		Foundation::HWND,
		UI::{
			Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent},
			WindowsAndMessaging::{
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
				SetTimer,
				TranslateMessage,
				WINEVENT_OUTOFCONTEXT,
				WINEVENT_SKIPOWNPROCESS,
			},
		},
	};

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
		let Remaining = BurstTicks.load(Ordering::Acquire);

		let ShouldRun = if Remaining > 0 {
			BurstTicks.store(Remaining - 1, Ordering::Release);
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

			// Periodic ~4.8 s reconciliation (40 * 120 ms) to catch Explorer restarts
			// or shell resets while keeping idle CPU wakeups negligible.
			if Next >= 40 {
				IdleTicks.store(0, Ordering::Release);
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
	}
}
