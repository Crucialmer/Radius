//! The script an overlay window runs before the frontend does.

use serde_json::json;

use crate::{
	Per::Of as PerOf,
	Screen::Struct::Struct as Screen,
	Settings::Struct::Struct,
	Visibility::Enum::Enum as Visibility,
};

/// The `window.settings` initialisation script one overlay window runs before
/// the frontend does, so the first paint already has the right size, ink,
/// screen-corner visibility, work-area insets and taskbar rim state.
pub fn Fn(State:&Struct, Target:&Screen) -> String {
	let Frame = crate::Overlay::Platform::Taskbar::InitialFrame(Target, State.TaskbarRounded, State.TaskbarRadius);

	let ScreenVisible = matches!(PerOf::Fn(&State.Visible, &Target.Label), Visibility::Shown);

	format!(
		"window.settings = {};",
		json!({
			"size": PerOf::Fn(&State.Radius, &Target.Label),
			"mode": PerOf::Fn(&State.Ink, &Target.Label),
			"screenVisible": ScreenVisible,
			"maximizeCorners": State.MaximizeCorners,
			"maximized": false,
			"workLeft": Target.WorkLeft,
			"workRight": Target.WorkRight,
			"workBottom": Target.WorkBottom,
			"taskbarFrame": {
				"Visible": Frame.Visible,
				"Edge": Frame.Edge,
				"Left": Frame.Left,
				"Top": Frame.Top,
				"Width": Frame.Width,
				"Height": Frame.Height,
				"Radius": Frame.Radius,
			},
		})
	)
}
