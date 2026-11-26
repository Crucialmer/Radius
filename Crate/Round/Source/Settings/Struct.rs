//! The persisted state of the overlay.

use crate::{
	Mode::Enum::Enum as Ink,
	Per::Struct::Struct as Per,
	Radius::Struct::Struct as Radius,
	Visibility::Enum::Enum as Visible,
};

/// Everything Round persists between launches.
///
/// The three per-monitor settings are [`Per`] rather than a default and a map
/// written out three times: each holds the value every monitor takes and the
/// monitors tuned on their own, so a monitor with no entry follows the default
/// and one gesture changes all of them at once.
///
/// `Dock` is the one setting with no per-monitor dimension: the dock icon
/// belongs to the application, not to a monitor, so there is nothing to override
/// per screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Struct {
	pub Radius:Per<Radius>,
	pub Ink:Per<Ink>,
	pub Visible:Per<Visible>,
	/// macOS only in the UI; kept on every platform so the store shape and this
	/// struct never diverge by target.
	pub Dock:Visible,
	/// The corner radius the taskbar itself is clipped with, in logical pixels.
	///
	/// Its own setting rather than the screen's [`Radius`]: the bar is a strip a few
	/// dozen pixels tall, and a radius that reads as a gentle rounding on a 1440 px
	/// screen turns it into a capsule. It is global rather than per-monitor for the
	/// same reason `Dock` is - one look for every taskbar, scaled by each display's
	/// own DPI when the clip is applied.
	pub TaskbarRadius:Radius,
	/// Whether the Windows taskbar (`Shell_TrayWnd` / `Shell_SecondaryTrayWnd`) is
	/// clipped into a 4-corner rounded rectangle.
	pub TaskbarRounded:bool,
	/// Whether maximized windows receive rounded bottom corners above the taskbar.
	pub MaximizeCorners:bool,
}

/// A fresh install: the default radius, corners shown, and both Windows-only
/// roundings on.
///
/// The two booleans are the one thing this cannot derive: `false` is the zero
/// value, and these start `true` because the rounding they switch on is what the
/// build is for. A store written before they defaulted on is handled by
/// [`Load`](crate::Settings::Load), not here.
impl Default for Struct {
	fn default() -> Self {
		Self {
			Radius:Per::default(),
			Ink:Per::default(),
			Visible:Per::default(),
			Dock:Visible::default(),
			TaskbarRadius:Radius::from(crate::Taskbar::Default::Const),
			TaskbarRounded:true,
			MaximizeCorners:true,
		}
	}
}
