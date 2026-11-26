/// The taskbar radius 1.1.2 shipped as its default.
///
/// Kept only so a store still holding this value can tell "never touched the
/// slider" from "chose 10 px myself": the first adopts the current default, the
/// second keeps what it asked for.
pub const Const:i64 = 10;
