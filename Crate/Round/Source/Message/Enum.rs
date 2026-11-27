//! What the tray broadcasts to the corner windows.

use serde::Serialize;

use crate::{Mode::Enum::Enum as Mode, Radius::Struct::Struct as Radius};

/// A state change pushed to every window.
///
/// The shape is the frontend contract: `listen("mode", ({ payload }) =>
/// payload.message.Mode)`. Externally tagged, lowercase mode, integer size -
/// changing either arm changes the webview.
#[derive(Clone, Copy, Debug, Serialize)]
pub enum Enum {
	Mode(Mode),
	Size(Radius),
	ScreenVisible(bool),
	MaximizeCorners(bool),
	Maximized(bool),
	WorkInset {
		LeftPx:i32,
		RightPx:i32,
		BottomPx:i32,
		OverlayW:i32,
		OverlayH:i32,
	},
	TaskbarFrame {
		Visible:bool,
		Edge:&'static str,
		LeftPx:i32,
		TopPx:i32,
		WidthPx:i32,
		HeightPx:i32,
		OverlayW:i32,
		OverlayH:i32,
		RadiusPx:i32,
		Scale:f64,
	},
}
