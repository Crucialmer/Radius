//! Where the panel goes, with strict work-area clamping and atomic Win32 resize+reposition.

use std::sync::Mutex;

use tauri::{AppHandle, Rect, Runtime, WebviewWindow};

use crate::{
	Bounds::{Monitor as Area, Position, Rect as AnchorBounds, Size, Struct::Struct as Bounds},
	Menu::{Animate, Gap, Height, Width},
};

/// Computed physical popup target placement and opening direction.
#[derive(Clone, Copy, Debug)]
pub struct Placement {
	pub Left:i32,
	pub Top:i32,
	pub WidthPx:i32,
	pub HeightPx:i32,
	pub Scale:f64,
	pub OpensAbove:bool,
}

static LastAnchor:Mutex<Option<Rect>> = Mutex::new(None);

static LastLogicalSize:Mutex<Option<(f64, f64)>> = Mutex::new(None);

/// Pure placement calculator that always keeps `(Left, Top, WidthPx, HeightPx)`
/// strictly clamped inside `Area`.
pub fn ComputeRect(Area:&Bounds, Icon:&Bounds, DesiredW:f64, DesiredH:f64, GapPx:f64) -> (i32, i32, i32, i32, bool) {
	let WidthPx = DesiredW.min(Area.Width).max(1.0);

	let HeightPx = DesiredH.min(Area.Height).max(1.0);

	let Left = (Icon.X + Icon.Width / 2.0 - WidthPx / 2.0).clamp(Area.X, (Area.X + Area.Width - WidthPx).max(Area.X));

	let Below = Icon.Y + Icon.Height + GapPx;

	let FitsBelow = Below + HeightPx <= Area.Y + Area.Height;

	let RawTop = if FitsBelow { Below } else { Icon.Y - HeightPx - GapPx };

	let Top = RawTop.clamp(Area.Y, (Area.Y + Area.Height - HeightPx).max(Area.Y));

	(
		Left.round() as i32,
		Top.round() as i32,
		WidthPx.round() as i32,
		HeightPx.round() as i32,
		!FitsBelow,
	)
}

fn ComputeForLogicalSize<R:Runtime>(
	App:&AppHandle<R>,
	Window:&WebviewWindow<R>,
	Anchor:Rect,
	LogicalW:f64,
	LogicalH:f64,
) -> Placement {
	// Resolve the monitor containing the tray icon first so cross-DPI setups use
	// the destination display's scale factor rather than the hidden panel's old one.
	let FallbackScale = Window.scale_factor().unwrap_or(1.0);

	let Probe = AnchorBounds::Fn(&Anchor, FallbackScale);

	let TargetMonitor = App
		.monitor_from_point(Probe.X, Probe.Y)
		.ok()
		.flatten()
		.or_else(|| Window.current_monitor().ok().flatten())
		.or_else(|| Window.primary_monitor().ok().flatten());

	let Scale = TargetMonitor.as_ref().map_or(FallbackScale, |Monitor| Monitor.scale_factor());

	let Icon = AnchorBounds::Fn(&Anchor, Scale);

	// No monitor under the icon is no bound at all: `f64::MAX` makes both clamps pass through cleanly.
	let MonArea = TargetMonitor.map_or(Bounds { X:0.0, Y:0.0, Width:f64::MAX, Height:f64::MAX }, |Monitor| {
		Area::Fn(&Monitor)
	});

	let (Left, Top, WidthPx, HeightPx, OpensAbove) =
		ComputeRect(&MonArea, &Icon, LogicalW * Scale, LogicalH * Scale, Gap::Const * Scale);

	Placement { Left, Top, WidthPx, HeightPx, Scale, OpensAbove }
}

/// Apply `(Left, Top, WidthPx, HeightPx)` atomically to `Window`.
pub fn ApplyBounds<R:Runtime>(Window:&WebviewWindow<R>, Left:i32, Top:i32, WidthPx:i32, HeightPx:i32) {
	#[cfg(target_os = "windows")]
	if let Ok(Native) = Window.hwnd()
		&& !Native.0.is_null()
	{
		use windows_sys::Win32::{
			Foundation::HWND,
			UI::WindowsAndMessaging::{HWND_TOPMOST, SWP_NOACTIVATE, SetWindowPos},
		};

		unsafe {
			if SetWindowPos(
				Native.0 as HWND,
				HWND_TOPMOST,
				Left,
				Top,
				WidthPx.max(1),
				HeightPx.max(1),
				SWP_NOACTIVATE,
			) != 0
			{
				return;
			}
		}
	}

	let _ = Window.set_size(Size::Fn(f64::from(WidthPx.max(1)), f64::from(HeightPx.max(1))));
	let _ = Window.set_position(Position::Fn(f64::from(Left), f64::from(Top)));
}

/// Compute target physical placement under `Anchor`, or above it when there is no room below,
/// remembering `Anchor` for subsequent layout-fit passes.
pub fn Compute<R:Runtime>(App:&AppHandle<R>, Window:&WebviewWindow<R>, Anchor:Rect) -> Placement {
	*LastAnchor.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner()) = Some(Anchor);

	let (LogicalW, LogicalH) = LastLogicalSize
		.lock()
		.unwrap_or_else(|Poisoned| Poisoned.into_inner())
		.unwrap_or((Width::Const, Height::Const));

	ComputeForLogicalSize(App, Window, Anchor, LogicalW, LogicalH)
}

/// Atomically resize the panel to `(LogicalW, LogicalH)` and re-anchor or clamp it inside
/// its monitor work area after canceling any in-flight entrance spring.
pub fn Fit<R:Runtime>(App:&AppHandle<R>, Window:&WebviewWindow<R>, LogicalW:f64, LogicalH:f64) {
	Animate::Cancel();

	let CleanW = LogicalW.max(1.0);

	let CleanH = LogicalH.max(1.0);

	*LastLogicalSize.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner()) = Some((CleanW, CleanH));

	if let Some(Anchor) = *LastAnchor.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner()) {
		let Target = ComputeForLogicalSize(App, Window, Anchor, CleanW, CleanH);

		ApplyBounds(Window, Target.Left, Target.Top, Target.WidthPx, Target.HeightPx);
		return;
	}

	let TargetMonitor = Window
		.current_monitor()
		.ok()
		.flatten()
		.or_else(|| Window.primary_monitor().ok().flatten());

	let Scale = TargetMonitor
		.as_ref()
		.map_or_else(|| Window.scale_factor().unwrap_or(1.0), |Monitor| Monitor.scale_factor());

	let MonArea = TargetMonitor.map_or(Bounds { X:0.0, Y:0.0, Width:f64::MAX, Height:f64::MAX }, |Monitor| {
		Area::Fn(&Monitor)
	});

	let Outer = Window.outer_position().ok();

	let DesiredW = (CleanW * Scale).min(MonArea.Width).max(1.0);

	let DesiredH = (CleanH * Scale).min(MonArea.Height).max(1.0);

	let Left = Outer
		.map_or(MonArea.X, |Pt| f64::from(Pt.x))
		.clamp(MonArea.X, (MonArea.X + MonArea.Width - DesiredW).max(MonArea.X));

	let Top = Outer
		.map_or(MonArea.Y, |Pt| f64::from(Pt.y))
		.clamp(MonArea.Y, (MonArea.Y + MonArea.Height - DesiredH).max(MonArea.Y));

	ApplyBounds(
		Window,
		Left.round() as i32,
		Top.round() as i32,
		DesiredW.round() as i32,
		DesiredH.round() as i32,
	);
}

/// Move and size the panel under `Anchor`, or above it when there is no room below.
pub fn Fn<R:Runtime>(App:&AppHandle<R>, Window:&WebviewWindow<R>, Anchor:Rect) {
	let Target = Compute(App, Window, Anchor);

	ApplyBounds(Window, Target.Left, Target.Top, Target.WidthPx, Target.HeightPx);
}

#[cfg(test)]
mod tests {
	use super::ComputeRect;
	use crate::Bounds::Struct::Struct as Bounds;

	fn RectContains(Area:&Bounds, Left:i32, Top:i32, Width:i32, Height:i32) {
		assert!(Width > 0 && Height > 0);
		assert!(f64::from(Left) >= Area.X, "Left {Left} < Area.X {}", Area.X);
		assert!(f64::from(Top) >= Area.Y, "Top {Top} < Area.Y {}", Area.Y);
		assert!(
			f64::from(Left + Width) <= Area.X + Area.Width,
			"Right {} > Area.Right {}",
			Left + Width,
			Area.X + Area.Width
		);
		assert!(
			f64::from(Top + Height) <= Area.Y + Area.Height,
			"Bottom {} > Area.Bottom {}",
			Top + Height,
			Area.Y + Area.Height
		);
	}

	#[test]
	fn ClampsBottomRightTrayAt131DpiStrictlyInsideWorkArea() {
		let Area = Bounds { X:0.0, Y:0.0, Width:3840.0, Height:2118.0 };
		let Icon = Bounds { X:3750.0, Y:2124.0, Width:32.0, Height:36.0 };
		let Scale = 131.0 / 96.0;
		let (L, T, W, H, Above) = ComputeRect(&Area, &Icon, 340.0 * Scale, 590.0 * Scale, 6.0 * Scale);
		assert!(Above);
		RectContains(&Area, L, T, W, H);
	}

	#[test]
	fn ClampsTallPanelOnShortHighDpiScreenToWorkArea() {
		let Area = Bounds { X:0.0, Y:0.0, Width:1920.0, Height:1020.0 };
		let Icon = Bounds { X:1820.0, Y:1030.0, Width:32.0, Height:40.0 };
		let Scale = 2.0;
		let (L, T, W, H, Above) = ComputeRect(&Area, &Icon, 340.0 * Scale, 590.0 * Scale, 6.0 * Scale);
		assert!(Above);
		assert_eq!(T, 0);
		assert_eq!(H, 1020);
		RectContains(&Area, L, T, W, H);
	}

	#[test]
	fn PlacesBelowTopTaskbarAndClampsOnSecondaryNegativeMonitor() {
		let TopArea = Bounds { X:0.0, Y:48.0, Width:2560.0, Height:1392.0 };
		let TopIcon = Bounds { X:2490.0, Y:8.0, Width:32.0, Height:32.0 };
		let (L, T, W, H, Above) = ComputeRect(&TopArea, &TopIcon, 340.0, 590.0, 6.0);
		assert!(!Above);
		RectContains(&TopArea, L, T, W, H);

		let NegArea = Bounds { X:-1920.0, Y:0.0, Width:1920.0, Height:1032.0 };
		let NegIcon = Bounds { X:-1910.0, Y:1040.0, Width:32.0, Height:40.0 };
		let (L2, T2, W2, H2, Above2) = ComputeRect(&NegArea, &NegIcon, 340.0, 590.0, 6.0);
		assert!(Above2);
		RectContains(&NegArea, L2, T2, W2, H2);
	}
}
