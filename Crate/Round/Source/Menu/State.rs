//! What the panel renders.

use serde::Serialize;

use crate::{
	Menu::Monitor::Struct as Monitor,
	Mode::Enum::Enum as Ink,
	Radius::Struct::Struct as Radius,
	Update::Struct::Struct as UpdateInfo,
	Visibility::Enum::Enum as Visible,
};

/// The whole of what the panel draws, returned by every command.
///
/// Each per-monitor setting travels twice: its default, so the panel can draw
/// the "All monitors" row, and every monitor's own value with its origin, so a
/// monitor row can be drawn without the panel holding a copy of the override
/// map. `Maximum` travels with them because the radius slider's range is a
/// property of the radius, not of a monitor.
///
/// Every key is pinned with a per-field `rename` rather than a container
/// attribute: the field names are PascalCase by house style and the wire keys
/// are the lowercase words `Menu.tsx` reads, so deriving one from the other is
/// the one thing this struct cannot do.
#[derive(Clone, Debug, Serialize)]
pub struct Struct {
	#[serde(rename = "radius")]
	pub Radius:Radius,
	#[serde(rename = "maximum")]
	pub Maximum:i64,
	#[serde(rename = "taskbar_radius")]
	pub TaskbarRadius:Radius,
	#[serde(rename = "taskbar_maximum")]
	pub TaskbarMaximum:i64,
	#[serde(rename = "ink")]
	pub Ink:Ink,
	#[serde(rename = "hidden")]
	pub Hidden:Visible,
	#[serde(rename = "dock_hidden")]
	pub Dock:Visible,
	#[serde(rename = "taskbar_rounded")]
	pub TaskbarRounded:bool,
	#[serde(rename = "maximize_corners")]
	pub MaximizeCorners:bool,
	#[serde(rename = "autostart")]
	pub Autostart:bool,
	#[serde(rename = "version")]
	pub Version:String,
	#[serde(rename = "update")]
	pub Update:UpdateInfo,
	#[serde(rename = "monitors")]
	pub Monitors:Vec<Monitor>,
}
