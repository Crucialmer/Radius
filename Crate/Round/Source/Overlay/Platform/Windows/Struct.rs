//! Windows: a topmost tool window per layer.

use tauri::{Runtime, WebviewWindow};
use windows_sys::Win32::{
	Foundation::HWND,
	Graphics::Dwm::{
		DWMWA_BORDER_COLOR,
		DWMWA_COLOR_NONE,
		DWMWA_WINDOW_CORNER_PREFERENCE,
		DWMWCP_DONOTROUND,
		DwmSetWindowAttribute,
	},
	UI::WindowsAndMessaging::{
		GWL_EXSTYLE,
		GWL_STYLE,
		GetWindowLongPtrW,
		HWND_TOPMOST,
		SWP_FRAMECHANGED,
		SWP_NOACTIVATE,
		SWP_NOMOVE,
		SWP_NOSIZE,
		SetWindowLongPtrW,
		SetWindowPos,
		WS_BORDER,
		WS_CAPTION,
		WS_DLGFRAME,
		WS_EX_CLIENTEDGE,
		WS_EX_DLGMODALFRAME,
		WS_EX_NOACTIVATE,
		WS_EX_STATICEDGE,
		WS_EX_TOOLWINDOW,
		WS_EX_TRANSPARENT,
		WS_EX_WINDOWEDGE,
		WS_THICKFRAME,
	},
};

use crate::{Overlay::Layer::Trait::Trait, Visibility::Enum::Enum as Visibility};

/// The topmost tool window.
pub struct Struct;

/// The window's `HWND`, or `None` when the window does not exist yet.
fn handle<R:Runtime>(Window:&WebviewWindow<R>) -> Option<HWND> {
	let Native = Window.hwnd().ok()?.0 as HWND;

	(!Native.is_null()).then_some(Native)
}

impl Trait for Struct {
	/// Above the taskbar, out of the taskbar list, out of the alt-tab cycle, and
	/// out of the mouse's way. Strips any Win32 non-client frame/border bits and
	/// suppresses Windows 11's DWM corner rounding, drop shadow, and 1px window
	/// border so re-plugging an external monitor never leaves a rectangular border
	/// behind the overlay corners.
	fn Art<R:Runtime>(Window:&WebviewWindow<R>) {
		let _ = Window.set_shadow(false);

		let Some(Native) = handle(Window) else {
			return;
		};

		unsafe {
			let Style = GetWindowLongPtrW(Native, GWL_STYLE);

			let FrameMask = (WS_CAPTION | WS_THICKFRAME | WS_BORDER | WS_DLGFRAME) as isize;

			if (Style & FrameMask) != 0 {
				SetWindowLongPtrW(Native, GWL_STYLE, Style & !FrameMask);
			}

			let ExStyle = GetWindowLongPtrW(Native, GWL_EXSTYLE);

			let EdgeMask = (WS_EX_DLGMODALFRAME | WS_EX_WINDOWEDGE | WS_EX_CLIENTEDGE | WS_EX_STATICEDGE) as isize;

			let Extra = (WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT) as isize;

			SetWindowLongPtrW(Native, GWL_EXSTYLE, (ExStyle & !EdgeMask) | Extra);

			let Corner:i32 = DWMWCP_DONOTROUND;

			let _ = DwmSetWindowAttribute(
				Native,
				DWMWA_WINDOW_CORNER_PREFERENCE as u32,
				&Corner as *const i32 as *const core::ffi::c_void,
				std::mem::size_of::<i32>() as u32,
			);

			let NoBorder:u32 = DWMWA_COLOR_NONE;

			let _ = DwmSetWindowAttribute(
				Native,
				DWMWA_BORDER_COLOR as u32,
				&NoBorder as *const u32 as *const core::ffi::c_void,
				std::mem::size_of::<u32>() as u32,
			);

			SetWindowPos(
				Native,
				HWND_TOPMOST,
				0,
				0,
				0,
				0,
				SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
			);
		}
	}

	/// In the topmost band, out of the taskbar and the alt-tab cycle, and never
	/// activated by the click it takes.
	///
	/// `WS_EX_NOACTIVATE` is the Windows answer to "must not be focusable for
	/// typing": the click arrives, the keyboard does not move.
	/// `WS_EX_TRANSPARENT` is deliberately absent - that is what the art has and
	/// the scrim must not.
	fn Scrim<R:Runtime>(Window:&WebviewWindow<R>) {
		let Some(Native) = handle(Window) else {
			return;
		};

		unsafe {
			let Style = GetWindowLongPtrW(Native, GWL_EXSTYLE);

			let Extra = (WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE) as isize;

			SetWindowLongPtrW(Native, GWL_EXSTYLE, Style | Extra);

			SetWindowPos(Native, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
		}
	}

	/// Above every scrim.
	///
	/// A window created after another one enters the topmost band above it, so
	/// the scrim would otherwise sit over the panel. `HWND_TOPMOST` with no size
	/// or move is a re-insert at the top of the band, which is the whole of the
	/// ordering here - and it is applied after the scrims are up, so the panel is
	/// on top however many times the panel has been opened.
	fn Panel<R:Runtime>(Window:&WebviewWindow<R>) {
		let Some(Native) = handle(Window) else {
			return;
		};

		unsafe {
			SetWindowPos(Native, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);

			let Corner:i32 = DWMWCP_DONOTROUND;

			DwmSetWindowAttribute(
				Native,
				DWMWA_WINDOW_CORNER_PREFERENCE as u32,
				&Corner as *const i32 as *const core::ffi::c_void,
				std::mem::size_of::<i32>() as u32,
			);
		}
	}

	/// The art and the scrim both carry `WS_EX_NOACTIVATE`, so showing either
	/// cannot take the keyboard and the portable call is the whole of it here:
	/// the no-keying order-front that macOS needs has no Windows equivalent to
	/// ask for.
	fn Front<R:Runtime>(Window:&WebviewWindow<R>) -> tauri::Result<()> { Window.show() }

	/// Windows has no dock icon to hide; the setting exists so the menu and the
	/// store keep one shape on every platform.
	fn Dock(_Visible:Visibility) {}
}
