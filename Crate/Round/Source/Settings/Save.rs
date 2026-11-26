//! Writing the state back.

use serde_json::json;
use tauri::Runtime;
use tauri_plugin_store::Store;

use crate::Settings::{Schema, Struct::Struct};

/// Write every setting, then flush the store to disk.
///
/// The three per-monitor settings are written as the shape
/// [`Per`](crate::Per::Struct::Struct) serialises to (`{"default": ...,
/// "monitor": {...}}`), so the reader and the writer agree by construction instead
/// of by two lists of keys kept in step by hand.
pub fn Fn<R:Runtime>(State:&Struct, Storage:&Store<R>) -> tauri_plugin_store::Result<()> {
	// The shape marker travels with every write, so a store this build has touched
	// is never read back as one that predates the current defaults.
	Storage.set("schema", json!(Schema::Const));

	Storage.set("radius", json!(State.Radius));

	Storage.set("taskbar_radius", json!(State.TaskbarRadius));

	Storage.set("ink", json!(State.Ink));

	Storage.set("hidden", json!(State.Visible));

	Storage.set("dock_hidden", json!(State.Dock));

	Storage.set("taskbar_rounded", json!(State.TaskbarRounded));

	Storage.set("maximize_corners", json!(State.MaximizeCorners));

	Storage.save()
}
