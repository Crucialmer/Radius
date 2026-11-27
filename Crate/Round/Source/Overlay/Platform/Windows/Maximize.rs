//! Per-monitor maximized & fullscreen window detection and live work-area inset
//! synchronization with zero heap allocations in the Win32 enumeration callback.

use std::{collections::HashMap, sync::Mutex};

use tauri::{AppHandle, Emitter, Runtime};

use crate::{
	Message::{Enum::Enum as Message, Payload::Struct as Payload},
	Overlay::Platform::Taskbar,
	Screen::Struct::Struct as Screen,
};

/// Per-monitor detection summary shared with [`Taskbar::Apply`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MonitorReport {
	pub Fullscreen:bool,
	pub MaxBottom:Option<i32>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct MonitorState {
	Maximized:bool,
	Left:i32,
	Right:i32,
	Bottom:i32,
	ScaleMilli:i32,
}

static Cache:Mutex<Option<HashMap<String, MonitorState>>> = Mutex::new(None);

/// Clear cached state for one monitor so the next refresh re-emits its
/// live work-area insets and maximized state.
pub fn Invalidate(Label:&str) {
	let mut Guard = Cache.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner());

	if let Some(Map) = Guard.as_mut() {
		Map.remove(Label);
	}
}

/// Clear all cached per-monitor state after a display topology change.
pub fn InvalidateAll() {
	let mut Guard = Cache.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner());

	if let Some(Map) = Guard.as_mut() {
		Map.clear();
	}
}

#[cfg(target_os = "windows")]
const fn EncodeWide<const N: usize>(Bytes:&[u8; N]) -> [u16; N] {
	let mut Out = [0u16; N];

	let mut Index = 0;

	while Index < N {
		Out[Index] = Bytes[Index] as u16;
		Index += 1;
	}

	Out
}

#[cfg(target_os = "windows")]
const IgnoredClasses:&[&[u16]] = &[
	&EncodeWide(b"Shell_TrayWnd"),
	&EncodeWide(b"Shell_SecondaryTrayWnd"),
	&EncodeWide(b"Progman"),
	&EncodeWide(b"WorkerW"),
	&EncodeWide(b"Windows.UI.Core.CoreWindow"),
];

/// Inspect top-level windows per monitor, anchor maximized bottom corners to
/// the actual rendered bottom edge of the topmost maximized window (staying
/// exact across standard, small, compact, floating, and auto-hidden taskbars),
/// and return `(Reports, TopologyDrifted)`.
#[cfg(target_os = "windows")]
pub fn Refresh<R:Runtime>(
	App:&AppHandle<R>,
	Screens:&[Screen],
	MaximizeEnabled:bool,
	TrackFullscreen:bool,
) -> (HashMap<String, MonitorReport>, bool) {
	use std::ffi::c_void;

	use windows_sys::Win32::{
		Foundation::{HWND, LPARAM, RECT},
		Graphics::Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute},
		UI::WindowsAndMessaging::{
			EnumWindows,
			GWL_EXSTYLE,
			GetClassNameW,
			GetWindowLongPtrW,
			GetWindowRect,
			GetWindowThreadProcessId,
			IsIconic,
			IsWindowVisible,
			IsZoomed,
			WS_EX_LAYERED,
			WS_EX_TOOLWINDOW,
			WS_EX_TRANSPARENT,
		},
	};

	struct Candidate {
		Rect:RECT,
		Zoomed:bool,
		FromDwm:bool,
	}

	struct EnumContext {
		OwnPid:u32,
		List:Vec<Candidate>,
	}

	unsafe extern "system" fn Collect(Hwnd:HWND, Param:LPARAM) -> i32 {
		unsafe {
			let Ctx = &mut *(Param as *mut EnumContext);

			if IsWindowVisible(Hwnd) == 0 || IsIconic(Hwnd) != 0 {
				return 1;
			}

			let mut Pid:u32 = 0;

			GetWindowThreadProcessId(Hwnd, &mut Pid);

			if Pid == Ctx.OwnPid {
				return 1;
			}

			let ExStyle = GetWindowLongPtrW(Hwnd, GWL_EXSTYLE);

			if (ExStyle & (WS_EX_TOOLWINDOW as isize)) != 0 {
				return 1;
			}

			let ClickThrough = (WS_EX_TRANSPARENT | WS_EX_LAYERED) as isize;

			if (ExStyle & ClickThrough) == ClickThrough {
				return 1;
			}

			let mut Cloaked:u32 = 0;

			if DwmGetWindowAttribute(
				Hwnd,
				DWMWA_CLOAKED as u32,
				&mut Cloaked as *mut u32 as *mut c_void,
				std::mem::size_of::<u32>() as u32,
			) == 0
				&& Cloaked != 0
			{
				return 1;
			}

			let mut Class = [0u16; 64];

			let Len = GetClassNameW(Hwnd, Class.as_mut_ptr(), Class.len() as i32);

			if Len > 0 {
				let Slice = &Class[..Len as usize];

				if IgnoredClasses.contains(&Slice) {
					return 1;
				}
			}

			let mut Rect = RECT { left:0, top:0, right:0, bottom:0 };

			let FromDwm = DwmGetWindowAttribute(
				Hwnd,
				DWMWA_EXTENDED_FRAME_BOUNDS as u32,
				&mut Rect as *mut RECT as *mut c_void,
				std::mem::size_of::<RECT>() as u32,
			) == 0;

			if !FromDwm && GetWindowRect(Hwnd, &mut Rect) == 0 {
				return 1;
			}

			if Rect.right - Rect.left <= 0 || Rect.bottom - Rect.top <= 0 {
				return 1;
			}

			Ctx.List.push(Candidate { Rect, Zoomed:IsZoomed(Hwnd) != 0, FromDwm });

			1
		}
	}

	let NeedEnum = MaximizeEnabled || TrackFullscreen;

	let mut Context = EnumContext {
		OwnPid:std::process::id(),
		List:Vec::with_capacity(if NeedEnum { 32 } else { 0 }),
	};

	let Windows = if NeedEnum {
		unsafe {
			EnumWindows(Some(Collect), &mut Context as *mut _ as LPARAM);
		}
		Context.List
	} else {
		Vec::new()
	};

	let mut Guard = Cache.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner());

	let Map = Guard.get_or_insert_with(HashMap::new);

	let mut Reports = HashMap::with_capacity(Screens.len());

	let mut Drifted = false;

	for Item in Screens {
		let Some((MonRect, WorkRect)) = Taskbar::MonitorRects(Item) else {
			Drifted = true;
			continue;
		};

		let Scale = Taskbar::LiveScale(App, Item);

		let MonW = (MonRect.right - MonRect.left).max(1);

		let MonH = (MonRect.bottom - MonRect.top).max(1);

		let WorkLeftPx = (WorkRect.left - MonRect.left).max(0);

		let WorkRightPx = (MonRect.right - WorkRect.right).max(0);

		let WorkBottomPx = (MonRect.bottom - WorkRect.bottom).max(0);

		let EdgeTaskbarBottomPx = Taskbar::BottomInsetOn(&MonRect, &WorkRect, Scale);

		let mut LeftPx = WorkLeftPx;

		let mut RightPx = WorkRightPx;

		let mut BottomPx = if EdgeTaskbarBottomPx > 0 { EdgeTaskbarBottomPx } else { WorkBottomPx };

		let mut HasMaximized = false;

		let mut IsFullscreen = false;

		let mut MaxBottom = None;

		if NeedEnum {
			let MaxBleed = (4.0 * Scale).round() as i32;

			for Candidate in &Windows {
				let IntersectW =
					(Candidate.Rect.right.min(MonRect.right) - Candidate.Rect.left.max(MonRect.left)).max(0);

				let IntersectH =
					(Candidate.Rect.bottom.min(MonRect.bottom) - Candidate.Rect.top.max(MonRect.top)).max(0);

				if IntersectW == 0 || IntersectH == 0 {
					continue;
				}

				// Fullscreen windows cover the taskbar as well; do not render work-area
				// bottom masks or taskbar rims across fullscreen videos or games.
				let CoversMonitor = Candidate.Rect.left <= MonRect.left + 2
					&& Candidate.Rect.top <= MonRect.top + 2
					&& Candidate.Rect.right >= MonRect.right - 2
					&& Candidate.Rect.bottom >= MonRect.bottom - 2;

				if CoversMonitor {
					IsFullscreen = true;
					HasMaximized = false;
					MaxBottom = None;
					break;
				}

				if MaximizeEnabled
					&& Candidate.Zoomed
					&& IntersectW >= (MonW * 85) / 100
					&& IntersectH >= (MonH * 70) / 100
				{
					if Candidate.FromDwm {
						let WinLeft = (Candidate.Rect.left - MonRect.left).clamp(0, MonW);

						let WinRight = (MonRect.right - Candidate.Rect.right).clamp(0, MonW);

						let WinBottom = (MonRect.bottom - Candidate.Rect.bottom).clamp(0, MonH);

						// Anchor to the maximized window's exact DWM bottom edge so stale
						// default 48dp `rcWork` values never leave a rectangular strip above
						// a small/compact taskbar. Only snap minor <=4dp overlaps when a
						// docked bottom taskbar and `rcWork` both agree on its height.
						let EffectiveBottom = if EdgeTaskbarBottomPx > WinBottom
							&& EdgeTaskbarBottomPx - WinBottom <= MaxBleed
							&& (WorkBottomPx - EdgeTaskbarBottomPx).abs() <= 2
						{
							EdgeTaskbarBottomPx
						} else {
							WinBottom
						};

						LeftPx = WinLeft;
						RightPx = WinRight;
						BottomPx = EffectiveBottom;
						HasMaximized = EffectiveBottom > 2;
						MaxBottom = HasMaximized.then_some(MonRect.bottom - EffectiveBottom);
					} else {
						HasMaximized = BottomPx > 2;
						MaxBottom = HasMaximized.then_some(MonRect.bottom - BottomPx);
					}

					break;
				}
			}
		}

		Reports.insert(Item.Label.clone(), MonitorReport { Fullscreen:IsFullscreen, MaxBottom });

		let NextState = MonitorState {
			Maximized:HasMaximized,
			Left:LeftPx,
			Right:RightPx,
			Bottom:BottomPx,
			ScaleMilli:(Scale * 1000.0).round() as i32,
		};

		let Previous = Map.get(&Item.Label).copied();

		if Previous.is_none_or(|Prev| {
			Prev.Left != NextState.Left
				|| Prev.Right != NextState.Right
				|| Prev.Bottom != NextState.Bottom
				|| Prev.ScaleMilli != NextState.ScaleMilli
		}) {
			let _ = App.emit_to(
				Item.Label.as_str(),
				"work_inset",
				Payload {
					Message:Message::WorkInset {
						Left:f64::from(LeftPx) / Scale,
						Right:f64::from(RightPx) / Scale,
						Bottom:f64::from(BottomPx) / Scale,
					},
				},
			);
		}

		if Previous.is_none_or(|Prev| Prev.Maximized != NextState.Maximized) {
			let _ = App.emit_to(
				Item.Label.as_str(),
				"maximized",
				Payload { Message:Message::Maximized(NextState.Maximized) },
			);
		}

		Map.insert(Item.Label.clone(), NextState);
	}

	(Reports, Drifted)
}

/// Non-Windows fallback.
#[cfg(not(target_os = "windows"))]
pub fn Refresh<R:Runtime>(
	_App:&AppHandle<R>,
	_Screens:&[Screen],
	_MaximizeEnabled:bool,
	_TrackFullscreen:bool,
) -> (HashMap<String, MonitorReport>, bool) {
	(HashMap::new(), false)
}
