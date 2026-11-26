//! Reading the persisted state back.

use tauri::Runtime;
use tauri_plugin_store::Store;

use crate::{
	Mode::Of as ModeOf,
	Per::Read as PerRead,
	Radius::{Of as RadiusOf, Struct::Struct as Radius},
	Screen::Struct::Struct as Screen,
	Settings::{Migrate, Schema, Struct::Struct},
	Taskbar::{Default as TaskbarDefault, Previous as TaskbarPrevious},
	Visibility::Of as VisibilityOf,
};

/// Read the settings back, falling back per setting to [`Struct::default`].
///
/// A hand-edited or truncated store can therefore cost one setting, never the
/// launch. The per-monitor keys are read by [`PerRead`], which owns the "default
/// plus overrides" shape; what is spelled out here is the store's own key names,
/// because they are the shape of `settings.json` that an older build of Round
/// wrote and a newer one reads. A key the current shape does not hold is
/// answered by [`Migrate`], which folds the store an older build wrote.
///
/// `Screens` is what every override is keyed by, so it is also what the labels
/// stored under an older build are carried onto: [`Migrate::Label`] is the last
/// step of every per-monitor setting, and it is why this takes the monitors as
/// well as the store.
pub fn Fn<R:Runtime>(Storage:&Store<R>, Screens:&[Screen]) -> Struct {
	let Radius = PerRead::Fn(Storage.get("radius").as_ref(), RadiusOf::Fn)
		.or_else(|| Migrate::Radius::Fn(Storage))
		.unwrap_or_default();

	let Ink = PerRead::Fn(Storage.get("ink").as_ref(), |Value| Value.as_str().map(ModeOf::Fn))
		.or_else(|| Migrate::Ink::Fn(Storage))
		.unwrap_or_default();

	// The old key is the new key: `hidden` held a bare boolean, which `PerRead`
	// already reads as a default with no overrides.
	let Visible =
		PerRead::Fn(Storage.get("hidden").as_ref(), |Value| Value.as_bool().map(VisibilityOf::Fn)).unwrap_or_default();

	// Migrate each default change strictly by the schema version that introduced
	// it so upgrading from schema 2 to schema 3 never clobbers a user's explicit
	// `false` choice for `taskbar_rounded` or `maximize_corners`.
	let StoredSchema = Storage.get("schema").and_then(|Value| Value.as_i64()).unwrap_or(0);

	let AdoptWindowsToggles = StoredSchema < 2;

	let AdoptTaskbarRadius = StoredSchema < Schema::Const;

	// The taskbar radius moved from 10 px to 8 px in schema 3. A store still
	// holding the old default never chose it, so it adopts the new one once; a
	// bar someone deliberately rounded to another value keeps what they asked for.
	let TaskbarRadius = match Storage.get("taskbar_radius").and_then(|Value| Value.as_i64()) {
		Some(Value) if !(AdoptTaskbarRadius && Value == TaskbarPrevious::Const) => {
			Radius::from(Value.clamp(0, crate::Taskbar::Maximum::Const))
		}
		_ => Radius::from(TaskbarDefault::Const),
	};

	Struct {
		Radius:Migrate::Label::Fn(Radius, Screens),
		Ink:Migrate::Label::Fn(Ink, Screens),
		Visible:Migrate::Label::Fn(Visible, Screens),
		Dock:Storage
			.get("dock_hidden")
			.and_then(|Value| Value.as_bool())
			.map(VisibilityOf::Fn)
			.unwrap_or_default(),
		TaskbarRadius,
		TaskbarRounded:AdoptWindowsToggles
			|| Storage.get("taskbar_rounded").and_then(|Value| Value.as_bool()).unwrap_or(true),
		MaximizeCorners:AdoptWindowsToggles
			|| Storage
				.get("maximize_corners")
				.and_then(|Value| Value.as_bool())
				.unwrap_or(true),
	}
}
