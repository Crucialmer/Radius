/**
 * @module Settings
 */

/** The two inks a corner can be drawn in - the words the Rust `Mode` enum serialises to. */
export type Mode = "dark" | "light";

/** Which monitor edge an independent rounded taskbar currently occupies. */
export type TaskbarEdge = "none" | "bottom" | "top" | "left" | "right";

/**
 * Exact physical-pixel taskbar overlay frame relative to the overlay client
 * viewport. Dividing physical pixels by WebView2's own live `devicePixelRatio`
 * eliminates Chromium's integer-DIP `100vh` / `%` rounding drift on fractional
 * and custom Windows DPI scales (e.g. 110%, 131 DPI / 136%, 175%) across 1080p–8K.
 */
export type TaskbarFrame = {
	Visible: boolean;
	Edge?: TaskbarEdge;
	LeftPx: number;
	TopPx: number;
	WidthPx: number;
	HeightPx: number;
	OverlayW: number;
	OverlayH: number;
	RadiusPx: number;
	Scale: number;
};

/** What the Rust side injects as `window.settings` before the first paint. */
export type Settings = {
	size: number;

	mode: Mode;

	screenVisible?: boolean;

	maximizeCorners: boolean;

	maximized: boolean;

	overlayW?: number;

	overlayH?: number;

	workLeftPx: number;

	workRightPx: number;

	workBottomPx: number;

	taskbarFrame?: TaskbarFrame;
};

/** `listen("size", ...)` payload: `Message::Size` is externally tagged. */
export type SizeMessage = {
	message: { Size: number };
};

/** `listen("mode", ...)` payload: `Message::Mode` is externally tagged. */
export type ModeMessage = {
	message: { Mode: Mode };
};

/** `listen("screen_visible", ...)` payload: `Message::ScreenVisible`. */
export type ScreenVisibleMessage = {
	message: { ScreenVisible: boolean };
};

/** `listen("maximize_corners", ...)` payload: `Message::MaximizeCorners`. */
export type MaximizeCornersMessage = {
	message: { MaximizeCorners: boolean };
};

/** `listen("maximized", ...)` payload: `Message::Maximized`. */
export type MaximizedMessage = {
	message: { Maximized: boolean };
};

/** `listen("work_inset", ...)` payload: `Message::WorkInset` (exact physical pixels). */
export type WorkInsetMessage = {
	message: {
		WorkInset: {
			LeftPx: number;
			RightPx: number;
			BottomPx: number;
			OverlayW: number;
			OverlayH: number;
		};
	};
};

/** `listen("taskbar_frame", ...)` payload: `Message::TaskbarFrame`. */
export type TaskbarFrameMessage = {
	message: { TaskbarFrame: TaskbarFrame };
};

declare global {
	interface Window {
		settings: Settings;
	}
}
