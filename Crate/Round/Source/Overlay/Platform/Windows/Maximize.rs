//! Per-monitor maximized & fullscreen window detection and live work-area inset
//! synchronization with zero heap allocations in the Win32 enumeration callback.

use std::{collections::HashMap, sync::Mutex};

use tauri::{AppHandle, Emitter, Runtime};

use crate::{
	Message::{Enum::Enum as Message, Payload::Struct as Payload},
	Screen::Struct::Struct as Screen,
};

#[derive(Clone, Copy, PartialEq, Eq)]
struct MonitorState {
	Maximized:bool,
	Left:i32,
	Right:i32,
	Bottom:i32,
}

static Cache:Mutex<Option<HashMap<String, MonitorState>>> = Mutex::new(None);

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

/// Inspect top-level windows per monitor, notify each overlay when a maximized
/// window appears/disappears or work-area insets change, and return the labels of
/// monitors currently covered by a fullscreen window.
#[cfg(target_os = "windows")]
pub fn Refresh<R:Runtime>(
	App:&AppHandle<R>,
	Screens:&[Screen],
	MaximizeEnabled:bool,
	TrackFullscreen:bool,
) -> Vec<String> {
	use std::ffi::c_void;

	use windows_sys::Win32::{
		Foundation::{HWND, LPARAM, POINT, RECT},
		Graphics::{
			Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute},
			Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint},
		},
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
			WS_EX_TOOLWINDOW,
		},
	};

	struct Candidate {
		Rect:RECT,
		Zoomed:bool,
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

			if DwmGetWindowAttribute(
				Hwnd,
				DWMWA_EXTENDED_FRAME_BOUNDS as u32,
				&mut Rect as *mut RECT as *mut c_void,
				std::mem::size_of::<RECT>() as u32,
			) != 0
				&& GetWindowRect(Hwnd, &mut Rect) == 0
			{
				return 1;
			}

			if Rect.right - Rect.left <= 0 || Rect.bottom - Rect.top <= 0 {
				return 1;
			}

			Ctx.List.push(Candidate { Rect, Zoomed:IsZoomed(Hwnd) != 0 });

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

	let mut Fullscreen = Vec::new();

	for Item in Screens {
		let Center = POINT { x:Item.PhysicalX + Item.Pixels.0 / 2, y:Item.PhysicalY + Item.Pixels.1 / 2 };

		let (MonRect, WorkRect) = unsafe {
			let Hmon = MonitorFromPoint(Center, MONITOR_DEFAULTTONEAREST);

			let mut Info:MONITORINFO = std::mem::zeroed();

			Info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;

			if !Hmon.is_null() && GetMonitorInfoW(Hmon, &mut Info) != 0 {
				(Info.rcMonitor, Info.rcWork)
			} else {
				(
					RECT {
						left:Item.PhysicalX,
						top:Item.PhysicalY,
						right:Item.PhysicalX + Item.Pixels.0,
						bottom:Item.PhysicalY + Item.Pixels.1,
					},
					RECT {
						left:Item.PhysicalX,
						top:Item.PhysicalY,
						right:Item.PhysicalX + Item.Pixels.0,
						bottom:Item.PhysicalY + Item.Pixels.1,
					},
				)
			}
		};

		let LeftPx = (WorkRect.left - MonRect.left).max(0);

		let RightPx = (MonRect.right - WorkRect.right).max(0);

		let BottomPx = (MonRect.bottom - WorkRect.bottom).max(0);

		let WorkW = (WorkRect.right - WorkRect.left).max(1);

		let WorkH = (WorkRect.bottom - WorkRect.top).max(1);

		let mut HasMaximized = false;

		let mut IsFullscreen = false;

		if NeedEnum {
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
					break;
				}

				if MaximizeEnabled
					&& BottomPx > 0
					&& Candidate.Zoomed
					&& IntersectW >= (WorkW * 9) / 10
					&& IntersectH >= (WorkH * 9) / 10
				{
					HasMaximized = true;
					break;
				}
			}
		}

		if IsFullscreen {
			Fullscreen.push(Item.Label.clone());
		}

		let NextState = MonitorState { Maximized:HasMaximized, Left:LeftPx, Right:RightPx, Bottom:BottomPx };

		let Previous = Map.get(&Item.Label).copied();

		if Previous.is_none_or(|Prev| {
			Prev.Left != NextState.Left || Prev.Right != NextState.Right || Prev.Bottom != NextState.Bottom
		}) {
			let Scale = Item.ScaleFactor.max(1.0);

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

	Fullscreen
}

/// Non-Windows fallback.
#[cfg(not(target_os = "windows"))]
pub fn Refresh<R:Runtime>(
	_App:&AppHandle<R>,
	_Screens:&[Screen],
	_MaximizeEnabled:bool,
	_TrackFullscreen:bool,
) -> Vec<String> {
	Vec::new()
}
