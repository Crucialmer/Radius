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
		Left:f64,
		Right:f64,
		Bottom:f64,
	},
	TaskbarFrame {
		Visible:bool,
		Edge:&'static str,
		LeftPct:f64,
		TopPct:f64,
		WidthPct:f64,
		HeightPct:f64,
		WidthPx:i32,
		HeightPx:i32,
		RadiusPx:i32,
		Scale:f64,
	},
}
