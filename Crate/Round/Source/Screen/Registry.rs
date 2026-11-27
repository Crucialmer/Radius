//! The live monitor set kept in sync across display hotplugs for every command
//! and watcher to read.
//!
//! A label is unique only within a set, so commands read from this shared
//! snapshot rather than re-enumerating ad hoc; when Windows notifies us of a
//! display change, [`crate::Overlay::Sync::Reconcile`] replaces the set
//! atomically alongside rebuilding/repositioning the overlay windows.

use std::sync::RwLock;

use crate::Screen::Struct::Struct as Screen;

/// Thread-safe live set of resolved monitors.
#[derive(Default)]
pub struct Struct(RwLock<Vec<Screen>>);

impl Struct {
	/// Create a registry seeded with `Screens`.
	pub fn New(Screens:Vec<Screen>) -> Self { Self(RwLock::new(Screens)) }

	/// Return an owned snapshot of the current monitor list.
	pub fn Snapshot(&self) -> Vec<Screen> { self.0.read().unwrap_or_else(|Poisoned| Poisoned.into_inner()).clone() }

	/// Replace the active monitor list atomically after a display topology change.
	pub fn Replace(&self, Screens:Vec<Screen>) {
		*self.0.write().unwrap_or_else(|Poisoned| Poisoned.into_inner()) = Screens;
	}
}
