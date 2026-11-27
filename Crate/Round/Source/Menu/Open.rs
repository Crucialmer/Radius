//! Open the scrims, then the panel above them.

use tauri::{AppHandle, Emitter, Rect, Runtime, WebviewWindow};

use crate::{
	Backdrop::{Close as BackdropClose, Open as BackdropOpen},
	Menu::{Animate, Place},
	Overlay::{Layer::Trait::Trait as _, Platform::Struct::Struct as Platform},
};

/// Show the panel over a fresh set of scrims with a 240Hz GPU entrance spring.
///
/// A scrim that cannot be built is not a reason to refuse the panel - the tray
/// still closes it - so the failure is logged and the panel shows. A panel that
/// failed to appear, on the other hand, must not leave a scrim behind: a scrim
/// is a window eating clicks with nothing over it, which is the one state worse
/// than a panel that will not close.
pub fn Fn<R:Runtime>(App:&AppHandle<R>, Window:&WebviewWindow<R>, Anchor:Rect) -> tauri::Result<()> {
	Animate::Cancel();

	let Target = Place::Compute(App, Window, Anchor);

	let Offset = (8.0 * Target.Scale).round() as i32;

	let StartY = if Target.OpensAbove { Target.Top + Offset } else { Target.Top - Offset };

	Place::ApplyBounds(Window, Target.Left, StartY, Target.WidthPx, Target.HeightPx);

	let _ = BackdropOpen::Fn(App).inspect_err(|Error| eprintln!("radius: the backdrop could not be opened: {Error}"));

	Window.show().inspect_err(|_| BackdropClose::Fn(App))?;

	// The panel is above the scrim by construction, not by having been shown
	// later: a level on macOS, a re-insert at the top of the topmost band on
	// Windows, a raise on X11. See [`Overlay`](crate::Overlay).
	Platform::Panel(Window);

	let _ = Window.emit("panel-open", Target.OpensAbove);

	#[cfg(target_os = "windows")]
	if let Ok(Native) = Window.hwnd() {
		Animate::SpringOpen(Native.0 as isize, Target.Left, StartY, Target.Top);
	}

	// `show` already orders the window front and makes it key; this is what asks
	// the app to activate. If macOS refuses - it refuses an activation that did
	// not come from a user gesture - the panel is still visible and nothing
	// dismisses it for that.
	Window.set_focus()
}
