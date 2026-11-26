//! The script an overlay window runs before the frontend does.

use serde_json::json;

use crate::{Per::Of as PerOf, Screen::Struct::Struct as Screen, Settings::Struct::Struct};

/// The `window.settings` initialisation script one overlay window runs before
/// the frontend does, so the first paint already has the right size, ink,
/// and work-area insets instead of flashing the defaults.
///
/// Both values are resolved for this window's label, so a monitor with an
/// override paints as itself before the first event arrives.
pub fn Fn(State:&Struct, Target:&Screen) -> String {
	let (TbVisible, TbLeft, TbTop, TbWidth, TbHeight, TbRadius) =
		crate::Overlay::Platform::Taskbar::InitialFrame(Target, State.TaskbarRounded, State.TaskbarRadius);

	format!(
		"window.settings = {};",
		json!({
			"size": PerOf::Fn(&State.Radius, &Target.Label),
			"mode": PerOf::Fn(&State.Ink, &Target.Label),
			"maximizeCorners": State.MaximizeCorners,
			"maximized": false,
			"workLeft": Target.WorkLeft,
			"workRight": Target.WorkRight,
			"workBottom": Target.WorkBottom,
			"taskbarFrame": {
				"Visible": TbVisible,
				"Left": TbLeft,
				"Top": TbTop,
				"Width": TbWidth,
				"Height": TbHeight,
				"Radius": TbRadius,
			},
		})
	)
}
