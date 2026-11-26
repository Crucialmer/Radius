//! Rounding all 4 corners of the Windows taskbar (`Shell_TrayWnd` / `Shell_SecondaryTrayWnd`)
//! and publishing per-monitor taskbar geometry so the overlay can wrap the 1-bit GDI clip
//! seam in a hardware-anti-aliased Liquid Glass rim.

use std::{collections::HashMap, sync::Mutex};

use tauri::{AppHandle, Emitter, Runtime};

use crate::{
	Message::{Enum::Enum as Message, Payload::Struct as Payload},
	Radius::Struct::Struct as Radius,
	Screen::Struct::Struct as Screen,
};

/// Cached taskbar geometry & applied ellipse diameter: `(hwnd, width, height, diameter)`.
static Cache:Mutex<Vec<(isize, i32, i32, i32)>> = Mutex::new(Vec::new());

/// Last emitted overlay rim frame per monitor, stored in physical pixels for exact diffing.
#[derive(Clone, Copy, PartialEq, Eq)]
struct FrameKey {
	Visible:bool,
	Left:i32,
	Top:i32,
	Width:i32,
	Height:i32,
	Radius:i32,
}

static FrameCache:Mutex<Option<HashMap<String, FrameKey>>> = Mutex::new(None);

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
const PrimaryClass:[u16; 14] = EncodeWide(b"Shell_TrayWnd\0");

#[cfg(target_os = "windows")]
const SecondaryClass:[u16; 23] = EncodeWide(b"Shell_SecondaryTrayWnd\0");

/// Locate every visible primary and secondary Windows taskbar in O(1) without
/// enumerating all desktop windows.
#[cfg(target_os = "windows")]
pub fn FindVisible() -> Vec<windows_sys::Win32::Foundation::HWND> {
	use std::ptr;

	use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowExW, FindWindowW, IsWindowVisible};

	let mut Result = Vec::with_capacity(4);

	unsafe {
		let Primary = FindWindowW(PrimaryClass.as_ptr(), ptr::null());

		if !Primary.is_null() && IsWindowVisible(Primary) != 0 {
			Result.push(Primary);
		}

		let mut Current = FindWindowExW(ptr::null_mut(), ptr::null_mut(), SecondaryClass.as_ptr(), ptr::null());

		while !Current.is_null() {
			if IsWindowVisible(Current) != 0 {
				Result.push(Current);
			}

			Current = FindWindowExW(ptr::null_mut(), Current, SecondaryClass.as_ptr(), ptr::null());
		}
	}

	Result
}

/// Compute the initial anti-aliased taskbar rim frame for one monitor at window
/// creation time so the first paint already aligns with the clipped taskbar.
#[cfg(target_os = "windows")]
pub fn InitialFrame(ScreenItem:&Screen, Enabled:bool, TaskbarRadius:Radius) -> (bool, f64, f64, f64, f64, f64) {
	use windows_sys::Win32::{Foundation::RECT, UI::WindowsAndMessaging::GetWindowRect};

	if !Enabled || TaskbarRadius.Value() <= 0 {
		return (false, 0.0, 0.0, 0.0, 0.0, 0.0);
	}

	let Scale = ScreenItem.ScaleFactor.max(1.0);

	for Hwnd in FindVisible() {
		let mut Rect = RECT { left:0, top:0, right:0, bottom:0 };

		if unsafe { GetWindowRect(Hwnd, &mut Rect) } == 0 {
			continue;
		}

		let Width = Rect.right - Rect.left;

		let Height = Rect.bottom - Rect.top;

		if Width <= 0 || Height <= 0 {
			continue;
		}

		let CenterX = Rect.left + Width / 2;

		let CenterY = Rect.top + Height / 2;

		if CenterX >= ScreenItem.PhysicalX
			&& CenterX < ScreenItem.PhysicalX + ScreenItem.Pixels.0
			&& CenterY >= ScreenItem.PhysicalY
			&& CenterY < ScreenItem.PhysicalY + ScreenItem.Pixels.1
		{
			let RadiusPx = ((TaskbarRadius.Value() as f64) * Scale).round() as i32;

			let Clamped = RadiusPx.clamp(0, (Height / 2).min(Width / 2));

			return (
				Clamped > 0,
				f64::from(Rect.left - ScreenItem.PhysicalX) / Scale,
				f64::from(Rect.top - ScreenItem.PhysicalY) / Scale,
				f64::from(Width) / Scale,
				f64::from(Height) / Scale,
				f64::from(Clamped) / Scale,
			);
		}
	}

	(false, 0.0, 0.0, 0.0, 0.0, 0.0)
}

/// Non-Windows fallback.
#[cfg(not(target_os = "windows"))]
pub fn InitialFrame(_ScreenItem:&Screen, _Enabled:bool, _TaskbarRadius:Radius) -> (bool, f64, f64, f64, f64, f64) {
	(false, 0.0, 0.0, 0.0, 0.0, 0.0)
}

/// Apply 4-corner rounded rectangle clipping to every Windows taskbar and sync
/// each monitor's anti-aliased Liquid Glass rim, or restore native rectangular
/// geometry when disabled.
#[cfg(target_os = "windows")]
pub fn Apply<R:Runtime>(
	App:&AppHandle<R>,
	Enabled:bool,
	TaskbarRadius:Radius,
	Screens:&[Screen],
	Fullscreen:&[String],
) {
	use std::ptr;

	use windows_sys::Win32::{
		Foundation::RECT,
		Graphics::Gdi::{CreateRoundRectRgn, DeleteObject, ERROR, GetWindowRgnBox, NULLREGION, SetWindowRgn},
		UI::WindowsAndMessaging::GetWindowRect,
	};

	if !Enabled {
		Restore();
		SyncFrames(App, Screens, &HashMap::new());
		return;
	}

	let Taskbars = FindVisible();

	let mut Guard = Cache.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner());

	let mut NextCache:Vec<(isize, i32, i32, i32)> = Vec::with_capacity(Taskbars.len());

	let mut NextFrames:HashMap<String, FrameKey> = HashMap::with_capacity(Taskbars.len());

	for Hwnd in Taskbars {
		let mut Rect = RECT { left:0, top:0, right:0, bottom:0 };

		if unsafe { GetWindowRect(Hwnd, &mut Rect) } == 0 {
			continue;
		}

		let Width = Rect.right - Rect.left;

		let Height = Rect.bottom - Rect.top;

		if Width <= 0 || Height <= 0 {
			continue;
		}

		let CenterX = Rect.left + Width / 2;

		let CenterY = Rect.top + Height / 2;

		let MatchedScreen = Screens.iter().find(|Item| {
			CenterX >= Item.PhysicalX
				&& CenterX < Item.PhysicalX + Item.Pixels.0
				&& CenterY >= Item.PhysicalY
				&& CenterY < Item.PhysicalY + Item.Pixels.1
		});

		let Scale = MatchedScreen.map_or(1.0, |Item| Item.ScaleFactor);

		let RadiusPx = ((TaskbarRadius.Value() as f64) * Scale).round() as i32;

		let Clamped = RadiusPx.clamp(0, (Height / 2).min(Width / 2));

		let Diameter = Clamped * 2;

		let Key = Hwnd as isize;

		let Cached = Guard.iter().any(|&(CachedHwnd, CachedW, CachedH, CachedD)| {
			CachedHwnd == Key && CachedW == Width && CachedH == Height && CachedD == Diameter
		});

		// Verify Explorer or a DPI transition has not silently cleared our region.
		let RegionActive = if Diameter == 0 {
			true
		} else {
			let mut BoxRect = RECT { left:0, top:0, right:0, bottom:0 };

			let Kind = unsafe { GetWindowRgnBox(Hwnd, &mut BoxRect) };

			Kind != ERROR && Kind != NULLREGION
		};

		if !Cached || !RegionActive {
			unsafe {
				if Diameter == 0 {
					let _ = SetWindowRgn(Hwnd, ptr::null_mut(), 1);
				} else {
					let Region = CreateRoundRectRgn(0, 0, Width + 1, Height + 1, Diameter, Diameter);

					if !Region.is_null() && SetWindowRgn(Hwnd, Region, 1) == 0 {
						DeleteObject(Region);
					}
				}
			}
		}

		NextCache.push((Key, Width, Height, Diameter));

		if let Some(Item) = MatchedScreen {
			let Covered = Fullscreen.iter().any(|Label| Label == &Item.Label);

			let Visible = Clamped > 0 && !Covered;

			NextFrames.insert(
				Item.Label.clone(),
				FrameKey {
					Visible,
					Left:Rect.left - Item.PhysicalX,
					Top:Rect.top - Item.PhysicalY,
					Width,
					Height,
					Radius:Clamped,
				},
			);
		}
	}

	*Guard = NextCache;

	drop(Guard);

	SyncFrames(App, Screens, &NextFrames);
}

#[cfg(target_os = "windows")]
fn SyncFrames<R:Runtime>(App:&AppHandle<R>, Screens:&[Screen], Next:&HashMap<String, FrameKey>) {
	let mut Guard = FrameCache.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner());

	let Map = Guard.get_or_insert_with(HashMap::new);

	let Hidden = FrameKey { Visible:false, Left:0, Top:0, Width:0, Height:0, Radius:0 };

	for Item in Screens {
		let Target = Next.get(&Item.Label).copied().unwrap_or(Hidden);

		if Map.get(&Item.Label).copied() == Some(Target) {
			continue;
		}

		let Scale = Item.ScaleFactor.max(1.0);

		let _ = App.emit_to(
			Item.Label.as_str(),
			"taskbar_frame",
			Payload {
				Message:Message::TaskbarFrame {
					Visible:Target.Visible,
					Left:f64::from(Target.Left) / Scale,
					Top:f64::from(Target.Top) / Scale,
					Width:f64::from(Target.Width) / Scale,
					Height:f64::from(Target.Height) / Scale,
					Radius:f64::from(Target.Radius) / Scale,
				},
			},
		);

		Map.insert(Item.Label.clone(), Target);
	}
}

/// Restore every previously clipped taskbar back to its native rectangular region.
#[cfg(target_os = "windows")]
pub fn Restore() {
	use std::ptr;

	use windows_sys::Win32::{Foundation::HWND, Graphics::Gdi::SetWindowRgn, UI::WindowsAndMessaging::IsWindow};

	let Entries:Vec<(isize, i32, i32, i32)> = {
		let mut Guard = Cache.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner());

		std::mem::take(&mut *Guard)
	};

	for (HwndRaw, ..) in Entries {
		let Hwnd = HwndRaw as HWND;

		unsafe {
			if IsWindow(Hwnd) != 0 {
				let _ = SetWindowRgn(Hwnd, ptr::null_mut(), 1);
			}
		}
	}
}

/// Non-Windows fallback.
#[cfg(not(target_os = "windows"))]
pub fn Apply<R:Runtime>(
	_App:&AppHandle<R>,
	_Enabled:bool,
	_TaskbarRadius:Radius,
	_Screens:&[Screen],
	_Fullscreen:&[String],
) {
}

/// Non-Windows fallback.
#[cfg(not(target_os = "windows"))]
pub fn Restore() {}
