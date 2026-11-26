/// The taskbar's corner radius, in logical pixels, on a fresh install.
///
/// Smaller than the screen's own default, and deliberately its own setting: the
/// taskbar is a strip a few dozen pixels tall, so a radius that reads as a gentle
/// rounding on a 1440 px screen reads as a capsule on the bar. 8 px rounds the
/// four corners visibly while keeping the bar looking like a bar.
pub const Const:i64 = 8;
