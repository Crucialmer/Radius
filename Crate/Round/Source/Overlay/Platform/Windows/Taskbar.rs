//! Rounding all 4 corners of the Windows taskbar (`Shell_TrayWnd` / `Shell_SecondaryTrayWnd`)
//! and publishing per-monitor taskbar geometry and screen-edge ownership so the overlay
//! can wrap the 1-bit GDI clip seam in a hardware-anti-aliased Liquid Glass rim and keep
//! taskbar corners completely independent from screen corner settings.

use std::{collections::HashMap, sync::Mutex};

use tauri::{AppHandle, Emitter, Runtime};

use crate::{
	Message::{Enum::Enum as Message, Payload::Struct as Payload},
	Overlay::Platform::Maximize::MonitorReport,
	Radius::Struct::Struct as Radius,
	Screen::Struct::Struct as Screen,
};

/// Logical taskbar overlay snapshot used for initial window script injection.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
	pub Visible:bool,
	pub Edge:&'static str,
	pub Left:f64,
	pub Top:f64,
	pub Width:f64,
	pub Height:f64,
	pub Radius:f64,
}

impl Default for Frame {
	fn default() -> Self { Self { Visible:false, Edge:"none", Left:0.0, Top:0.0, Width:0.0, Height:0.0, Radius:0.0 } }
}

/// Cached taskbar clip region geometry and ellipse diameter.
#[derive(Clone, Copy, PartialEq, Eq)]
struct ClipEntry {
	Hwnd:isize,
	OffsetX:i32,
	OffsetY:i32,
	Width:i32,
	Height:i32,
	Diameter:i32,
}

static Cache:Mutex<Vec<ClipEntry>> = Mutex::new(Vec::new());

/// Last emitted overlay rim frame per monitor, stored in physical pixels plus milli-scale for exact diffing.
#[derive(Clone, Copy, PartialEq, Eq)]
struct FrameKey {
	Visible:bool,
	Edge:&'static str,
	Left:i32,
	Top:i32,
	Width:i32,
	Height:i32,
	Radius:i32,
	ScaleMilli:i32,
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

#[cfg(target_os = "windows")]
const BridgeClass:[u16; 50] = EncodeWide(b"Windows.UI.Composition.DesktopWindowContentBridge\0");

/// Clear the cached rim frame for one monitor so the next sync re-emits its
/// current geometry (used when an overlay finishes mounting or is removed).
pub fn InvalidateFrame(Label:&str) {
	let mut Guard = FrameCache.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner());

	if let Some(Map) = Guard.as_mut() {
		Map.remove(Label);
	}
}

/// Clear all cached taskbar clip entries and rim frames so display hotplugs
/// cleanly re-clip surviving taskbars and push fresh rim coordinates.
pub fn InvalidateAll() {
	let mut ClipGuard = Cache.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner());
	ClipGuard.clear();
	drop(ClipGuard);

	let mut FrameGuard = FrameCache.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner());
	if let Some(Map) = FrameGuard.as_mut() {
		Map.clear();
	}
}

/// Query live physical `(rcMonitor, rcWork, ScaleFactor)` for `ScreenItem` using
/// `MONITOR_DEFAULTTONULL` and `GetDpiForMonitor` so a disconnected external
/// monitor returns `None` instead of aliasing onto the surviving display, and
/// per-monitor DPI is always exact even during cross-DPI window creation.
#[cfg(target_os = "windows")]
pub fn MonitorRects(
	ScreenItem:&Screen,
) -> Option<(windows_sys::Win32::Foundation::RECT, windows_sys::Win32::Foundation::RECT, f64)> {
	use windows_sys::Win32::{
		Foundation::POINT,
		Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONULL, MONITORINFO, MonitorFromPoint},
		UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
	};

	let Center = POINT {
		x:ScreenItem.PhysicalX + ScreenItem.Pixels.0 / 2,
		y:ScreenItem.PhysicalY + ScreenItem.Pixels.1 / 2,
	};

	unsafe {
		let Hmon = MonitorFromPoint(Center, MONITOR_DEFAULTTONULL);

		if Hmon.is_null() {
			return None;
		}

		let mut Info:MONITORINFO = std::mem::zeroed();

		Info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;

		if GetMonitorInfoW(Hmon, &mut Info) == 0 {
			return None;
		}

		// Verify the live monitor still matches this screen's position & dimensions;
		// if a display hotplug shifted coordinates or changed resolution, return None
		// so the watcher triggers a clean main-thread reconcile instead of painting
		// shifted rims into a stale-sized overlay window.
		let MonW = Info.rcMonitor.right - Info.rcMonitor.left;

		let MonH = Info.rcMonitor.bottom - Info.rcMonitor.top;

		if (Info.rcMonitor.left - ScreenItem.PhysicalX).abs() > 2
			|| (Info.rcMonitor.top - ScreenItem.PhysicalY).abs() > 2
			|| (MonW - ScreenItem.Pixels.0).abs() > 2
			|| (MonH - ScreenItem.Pixels.1).abs() > 2
		{
			return None;
		}

		let mut DpiX:u32 = 96;

		let mut DpiY:u32 = 96;

		let Scale = if GetDpiForMonitor(Hmon, MDT_EFFECTIVE_DPI, &mut DpiX, &mut DpiY) == 0 && DpiX >= 96 {
			(f64::from(DpiX) / 96.0).max(1.0)
		} else {
			ScreenItem.ScaleFactor.max(1.0)
		};

		Some((Info.rcMonitor, Info.rcWork, Scale))
	}
}

#[cfg(target_os = "windows")]
fn DetectEdge(
	Rect:&windows_sys::Win32::Foundation::RECT,
	MonRect:&windows_sys::Win32::Foundation::RECT,
) -> &'static str {
	let Width = Rect.right - Rect.left;

	let Height = Rect.bottom - Rect.top;

	let MonW = (MonRect.right - MonRect.left).max(1);

	let MonH = (MonRect.bottom - MonRect.top).max(1);

	if Width >= Height {
		// Narrow floating docks do not occupy the monitor's left/right corners.
		if Width < (MonW * 9) / 10 {
			return "none";
		}

		if Rect.top <= MonRect.top + 4 && Rect.bottom < MonRect.bottom - 4 {
			"top"
		} else if Rect.bottom >= MonRect.bottom - 4 {
			"bottom"
		} else {
			"none"
		}
	} else {
		if Height < (MonH * 9) / 10 {
			return "none";
		}

		if Rect.left <= MonRect.left + 4 {
			"left"
		} else if Rect.right >= MonRect.right - 4 {
			"right"
		} else {
			"none"
		}
	}
}

/// Measure a taskbar window's raw Win32 rect and tightened visible rect on
/// `MonRect`, accounting for DWM frame bounds, Win11 XAML content bridge
/// sizing, compact/small taskbar `rcWork` bounds, and optional maximized-window
/// bottom alignment.
#[cfg(target_os = "windows")]
fn MeasureBounds(
	Hwnd:windows_sys::Win32::Foundation::HWND,
	MonRect:&windows_sys::Win32::Foundation::RECT,
	WorkRect:&windows_sys::Win32::Foundation::RECT,
	MaxBottom:Option<i32>,
	Scale:f64,
) -> Option<(windows_sys::Win32::Foundation::RECT, windows_sys::Win32::Foundation::RECT)> {
	use std::{ffi::c_void, ptr};

	use windows_sys::Win32::{
		Foundation::RECT,
		Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute},
		UI::WindowsAndMessaging::{FindWindowExW, GetWindowRect, IsWindowVisible},
	};

	let mut Raw = RECT { left:0, top:0, right:0, bottom:0 };

	if unsafe { GetWindowRect(Hwnd, &mut Raw) } == 0 {
		return None;
	}

	let RawW = Raw.right - Raw.left;

	let RawH = Raw.bottom - Raw.top;

	if RawW <= 0 || RawH <= 0 {
		return None;
	}

	let mut Vis = Raw;

	let mut DwmRect = RECT { left:0, top:0, right:0, bottom:0 };

	if unsafe {
		DwmGetWindowAttribute(
			Hwnd,
			DWMWA_EXTENDED_FRAME_BOUNDS as u32,
			&mut DwmRect as *mut RECT as *mut c_void,
			std::mem::size_of::<RECT>() as u32,
		)
	} == 0
		&& DwmRect.right > DwmRect.left
		&& DwmRect.bottom > DwmRect.top
	{
		Vis.left = Vis.left.max(DwmRect.left);
		Vis.top = Vis.top.max(DwmRect.top);
		Vis.right = Vis.right.min(DwmRect.right);
		Vis.bottom = Vis.bottom.min(DwmRect.bottom);
	}

	// Clamp to the monitor so auto-hidden taskbars slid off-screen are filtered out.
	Vis.left = Vis.left.max(MonRect.left);
	Vis.top = Vis.top.max(MonRect.top);
	Vis.right = Vis.right.min(MonRect.right);
	Vis.bottom = Vis.bottom.min(MonRect.bottom);

	let mut VisW = Vis.right - Vis.left;

	let mut VisH = Vis.bottom - Vis.top;

	if VisW < 12 || VisH < 12 {
		return None;
	}

	// On Windows 11, check if the full-span XAML content bridge is strictly tighter
	// than the outer `Shell_TrayWnd` host (common with small-taskbar mods).
	let Horizontal = VisW >= VisH;

	unsafe {
		let mut Child = FindWindowExW(Hwnd, ptr::null_mut(), BridgeClass.as_ptr(), ptr::null());

		while !Child.is_null() {
			if IsWindowVisible(Child) != 0 {
				let mut Bridge = RECT { left:0, top:0, right:0, bottom:0 };

				if GetWindowRect(Child, &mut Bridge) != 0 {
					let BridgeW = Bridge.right - Bridge.left;

					let BridgeH = Bridge.bottom - Bridge.top;

					if Horizontal && BridgeW >= (VisW * 9) / 10 && BridgeH >= 16 && BridgeH < VisH {
						let NextTop = Vis.top.max(Bridge.top);

						let NextBottom = Vis.bottom.min(Bridge.bottom);

						if NextBottom - NextTop >= 16 {
							Vis.top = NextTop;
							Vis.bottom = NextBottom;
							VisH = Vis.bottom - Vis.top;
						}
					} else if !Horizontal && BridgeH >= (VisH * 9) / 10 && BridgeW >= 16 && BridgeW < VisW {
						let NextLeft = Vis.left.max(Bridge.left);

						let NextRight = Vis.right.min(Bridge.right);

						if NextRight - NextLeft >= 16 {
							Vis.left = NextLeft;
							Vis.right = NextRight;
							VisW = Vis.right - Vis.left;
						}
					}
				}
			}

			Child = FindWindowExW(Hwnd, Child, BridgeClass.as_ptr(), ptr::null());
		}
	}

	// Reconcile with `rcWork`: when a small taskbar shrinks the reserved work area
	// while the outer `Shell_TrayWnd` host keeps its default 48dp height, trim the
	// transparent dead strip above/beside the small taskbar.
	if Horizontal {
		if Vis.bottom >= MonRect.bottom - 8 && Vis.top > MonRect.top + 8 {
			if WorkRect.bottom < MonRect.bottom - 4 && WorkRect.bottom > Vis.top && WorkRect.bottom < Vis.bottom - 12 {
				Vis.top = WorkRect.bottom;
			}

			let BleedTol = (4.0 * Scale).round() as i32;

			if let Some(Bottom) = MaxBottom
				&& Bottom > Vis.top + BleedTol
				&& Bottom < Vis.bottom - 12
			{
				Vis.top = Bottom;
			}
		} else if Vis.top <= MonRect.top + 8
			&& Vis.bottom < MonRect.bottom - 8
			&& WorkRect.top > MonRect.top + 4
			&& WorkRect.top < Vis.bottom
			&& WorkRect.top > Vis.top + 12
		{
			Vis.bottom = WorkRect.top;
		}
	} else if Vis.left <= MonRect.left + 8
		&& WorkRect.left > MonRect.left + 4
		&& WorkRect.left < Vis.right
		&& WorkRect.left > Vis.left + 12
	{
		Vis.right = WorkRect.left;
	} else if Vis.right >= MonRect.right - 8
		&& WorkRect.right < MonRect.right - 4
		&& WorkRect.right > Vis.left
		&& WorkRect.right < Vis.right - 12
	{
		Vis.left = WorkRect.right;
	}

	if Vis.right - Vis.left < 12 || Vis.bottom - Vis.top < 12 {
		return None;
	}

	Some((Raw, Vis))
}

/// Return the physical bottom inset occupied by a full-width visible bottom
/// taskbar on `MonRect`, or `0` if none is docked there (auto-hidden, narrow
/// floating dock, or on another edge).
#[cfg(target_os = "windows")]
pub fn BottomInsetOn(
	MonRect:&windows_sys::Win32::Foundation::RECT,
	WorkRect:&windows_sys::Win32::Foundation::RECT,
	Scale:f64,
) -> i32 {
	let MonW = (MonRect.right - MonRect.left).max(1);

	for Hwnd in FindVisible() {
		let Some((_, Vis)) = MeasureBounds(Hwnd, MonRect, WorkRect, None, Scale) else {
			continue;
		};

		let VisW = Vis.right - Vis.left;

		let VisH = Vis.bottom - Vis.top;

		if VisW >= VisH && VisW >= (MonW * 85) / 100 && Vis.bottom >= MonRect.bottom - 8 && Vis.top > MonRect.top + 8 {
			return (MonRect.bottom - Vis.top).max(0);
		}
	}

	0
}

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

/// Compute the initial anti-aliased taskbar rim frame and occupied edge for one
/// monitor at window creation time so the first paint already aligns cleanly.
#[cfg(target_os = "windows")]
pub fn InitialFrame(ScreenItem:&Screen, Enabled:bool, TaskbarRadius:Radius) -> Frame {
	if !Enabled {
		return Frame::default();
	}

	let Some((MonRect, WorkRect, Scale)) = MonitorRects(ScreenItem) else {
		return Frame::default();
	};

	for Hwnd in FindVisible() {
		let Some((_, Vis)) = MeasureBounds(Hwnd, &MonRect, &WorkRect, None, Scale) else {
			continue;
		};

		let Width = Vis.right - Vis.left;

		let Height = Vis.bottom - Vis.top;

		let RadiusPx = ((TaskbarRadius.Value() as f64) * Scale).round() as i32;

		let Clamped = RadiusPx.clamp(0, (Height / 2).min(Width / 2));

		return Frame {
			Visible:Clamped > 0,
			Edge:DetectEdge(&Vis, &MonRect),
			Left:f64::from(Vis.left - MonRect.left) / Scale,
			Top:f64::from(Vis.top - MonRect.top) / Scale,
			Width:f64::from(Width) / Scale,
			Height:f64::from(Height) / Scale,
			Radius:f64::from(Clamped) / Scale,
		};
	}

	Frame::default()
}

/// Non-Windows fallback.
#[cfg(not(target_os = "windows"))]
pub fn InitialFrame(_ScreenItem:&Screen, _Enabled:bool, _TaskbarRadius:Radius) -> Frame { Frame::default() }

/// Apply 4-corner rounded rectangle clipping to every Windows taskbar and sync
/// each monitor's anti-aliased Liquid Glass rim and independent edge status.
#[cfg(target_os = "windows")]
pub fn Apply<R:Runtime>(
	App:&AppHandle<R>,
	Enabled:bool,
	TaskbarRadius:Radius,
	Screens:&[Screen],
	Reports:&HashMap<String, MonitorReport>,
) {
	use std::ptr;

	use windows_sys::Win32::{
		Foundation::RECT,
		Graphics::Gdi::{COMPLEXREGION, CreateRoundRectRgn, DeleteObject, GetWindowRgnBox, SetWindowRgn},
		UI::WindowsAndMessaging::GetWindowRect,
	};

	if !Enabled {
		Restore();
		SyncFrames(App, Screens, &HashMap::new());
		return;
	}

	let Taskbars = FindVisible();

	// Only include monitors that still exist at their registered bounds so an
	// unplugged monitor can never alias onto a surviving display's taskbar.
	let MonitorInfo:Vec<(&Screen, RECT, RECT, f64)> = Screens
		.iter()
		.filter_map(|Item| {
			let (MonRect, WorkRect, Scale) = MonitorRects(Item)?;

			Some((Item, MonRect, WorkRect, Scale))
		})
		.collect();

	let mut Guard = Cache.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner());

	let mut NextCache:Vec<ClipEntry> = Vec::with_capacity(Taskbars.len());

	let mut NextFrames:HashMap<String, FrameKey> = HashMap::with_capacity(Taskbars.len());

	for Hwnd in Taskbars {
		let mut Raw = RECT { left:0, top:0, right:0, bottom:0 };

		if unsafe { GetWindowRect(Hwnd, &mut Raw) } == 0 {
			continue;
		}

		let RawW = Raw.right - Raw.left;

		let RawH = Raw.bottom - Raw.top;

		if RawW <= 0 || RawH <= 0 {
			continue;
		}

		let CenterX = Raw.left + RawW / 2;

		let CenterY = Raw.top + RawH / 2;

		let Some(&(Item, MonRect, WorkRect, Scale)) = MonitorInfo.iter().find(|(_, MonRect, ..)| {
			CenterX >= MonRect.left && CenterX < MonRect.right && CenterY >= MonRect.top && CenterY < MonRect.bottom
		}) else {
			// Taskbar belongs to a monitor not yet reconciled into `Screens`; wait
			// for `Reconcile` rather than clipping at a guessed 1.0 DPI scale.
			continue;
		};

		let MaxBottom = Reports.get(&Item.Label).and_then(|Report| Report.MaxBottom);

		let Some((RawBox, Vis)) = MeasureBounds(Hwnd, &MonRect, &WorkRect, MaxBottom, Scale) else {
			continue;
		};

		Raw = RawBox;

		let Width = Vis.right - Vis.left;

		let Height = Vis.bottom - Vis.top;

		let OffsetX = (Vis.left - Raw.left).max(0);

		let OffsetY = (Vis.top - Raw.top).max(0);

		let RadiusPx = ((TaskbarRadius.Value() as f64) * Scale).round() as i32;

		let Clamped = RadiusPx.clamp(0, (Height / 2).min(Width / 2));

		let Diameter = Clamped * 2;

		let Entry = ClipEntry { Hwnd:Hwnd as isize, OffsetX, OffsetY, Width, Height, Diameter };

		let Cached = Guard.contains(&Entry);

		// A rounded-rect region always reports `COMPLEXREGION` whose bounding box
		// matches our clip rect. Checking strictly catches Windows/Explorer
		// resetting a taskbar back to a rectangular `SIMPLEREGION` or wrong size
		// when an external monitor is re-plugged.
		let RegionActive = if Diameter == 0 {
			true
		} else {
			let mut BoxRect = RECT { left:0, top:0, right:0, bottom:0 };

			let Kind = unsafe { GetWindowRgnBox(Hwnd, &mut BoxRect) };

			Kind == COMPLEXREGION
				&& (BoxRect.left - OffsetX).abs() <= 1
				&& (BoxRect.top - OffsetY).abs() <= 1
				&& (BoxRect.right - (OffsetX + Width)).abs() <= 1
				&& (BoxRect.bottom - (OffsetY + Height)).abs() <= 1
		};

		if !Cached || !RegionActive {
			unsafe {
				if Diameter == 0 {
					let _ = SetWindowRgn(Hwnd, ptr::null_mut(), 1);
				} else {
					let Region = CreateRoundRectRgn(
						OffsetX,
						OffsetY,
						OffsetX + Width + 1,
						OffsetY + Height + 1,
						Diameter,
						Diameter,
					);

					if !Region.is_null() && SetWindowRgn(Hwnd, Region, 1) == 0 {
						DeleteObject(Region);
					}
				}
			}
		}

		NextCache.push(Entry);

		let Covered = Reports.get(&Item.Label).is_some_and(|Report| Report.Fullscreen);

		let Edge = if Covered { "none" } else { DetectEdge(&Vis, &MonRect) };

		let Visible = Clamped > 0 && !Covered;

		NextFrames.insert(
			Item.Label.clone(),
			FrameKey {
				Visible,
				Edge,
				Left:Vis.left - MonRect.left,
				Top:Vis.top - MonRect.top,
				Width,
				Height,
				Radius:Clamped,
				ScaleMilli:(Scale * 1000.0).round() as i32,
			},
		);
	}

	*Guard = NextCache;

	drop(Guard);

	SyncFrames(App, Screens, &NextFrames);
}

#[cfg(target_os = "windows")]
fn SyncFrames<R:Runtime>(App:&AppHandle<R>, Screens:&[Screen], Next:&HashMap<String, FrameKey>) {
	let mut Guard = FrameCache.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner());

	let Map = Guard.get_or_insert_with(HashMap::new);

	let Hidden = FrameKey {
		Visible:false,
		Edge:"none",
		Left:0,
		Top:0,
		Width:0,
		Height:0,
		Radius:0,
		ScaleMilli:1000,
	};

	for Item in Screens {
		let Target = Next.get(&Item.Label).copied().unwrap_or(Hidden);

		if Map.get(&Item.Label).copied() == Some(Target) {
			continue;
		}

		let Scale = (f64::from(Target.ScaleMilli) / 1000.0).max(1.0);

		let _ = App.emit_to(
			Item.Label.as_str(),
			"taskbar_frame",
			Payload {
				Message:Message::TaskbarFrame {
					Visible:Target.Visible,
					Edge:Target.Edge,
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

	let Entries:Vec<ClipEntry> = {
		let mut Guard = Cache.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner());

		std::mem::take(&mut *Guard)
	};

	for Entry in Entries {
		let Hwnd = Entry.Hwnd as HWND;

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
	_Reports:&HashMap<String, MonitorReport>,
) {
}

/// Non-Windows fallback.
#[cfg(not(target_os = "windows"))]
pub fn Restore() {}
