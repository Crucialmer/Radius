/// The largest taskbar radius the panel offers, in logical pixels.
///
/// Half of a standard taskbar: the bar is 48 logical pixels tall on Windows 11 at
/// any scale factor, so anything past 24 would be clipped back to 24 by the
/// per-monitor clamp and the slider would keep moving while the screen did not.
pub const Const:i64 = 24;
