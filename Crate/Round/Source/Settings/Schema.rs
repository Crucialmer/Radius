//! The `settings.json` shape this build writes.

/// The store shape, recorded under the `schema` key.
///
/// Bumped when a stored key changes meaning, or when a default moves in a way an
/// older store cannot express. 1.1.0 and earlier wrote no marker at all, which
/// reads as 0: their `taskbar_rounded` / `maximize_corners` were `false` because
/// that was the default then, so those stores adopt the current defaults once
/// instead of having a choice they never made read back at them. 3 moves the
/// taskbar radius default from 10 px to 8 px for the same reason.
pub const Const:i64 = 3;
