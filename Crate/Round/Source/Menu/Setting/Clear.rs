//! Dropping the overrides one section of the panel names.

use crate::{
	Per::Clear as PerClear,
	Radius::Struct::Struct as Radius,
	Settings::Struct::Struct,
	Taskbar::Default as TaskbarDefault,
};

/// Drop the overrides `Label` names, in `Setting` when one is named and in every
/// per-monitor setting otherwise.
///
/// `Setting` is the wire word for a panel section, so the words are the
/// contract: `radius`, `ink`, `hidden` and `taskbar_radius`. A word this build
/// does not know is refused rather than guessed, because clearing the wrong
/// setting is not something pressing the button again puts right. `Label` absent
/// is the "All monitors" row, which resets the default and drops every monitor's
/// override in the same gesture.
pub fn Fn(Preferences:&mut Struct, Setting:Option<&str>, Label:Option<&str>) -> Result<(), String> {
	match Setting {
		Some("radius") => PerClear::Fn(&mut Preferences.Radius, Label),
		Some("ink") => PerClear::Fn(&mut Preferences.Ink, Label),
		Some("hidden") => PerClear::Fn(&mut Preferences.Visible, Label),
		Some("taskbar_radius") => Preferences.TaskbarRadius = Radius::from(TaskbarDefault::Const),
		Some(Word) => return Err(format!("radius: `{Word}` is not a per-monitor setting")),
		None => {
			PerClear::Fn(&mut Preferences.Radius, Label);

			PerClear::Fn(&mut Preferences.Ink, Label);

			PerClear::Fn(&mut Preferences.Visible, Label);
		}
	}

	Ok(())
}
