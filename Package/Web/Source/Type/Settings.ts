/**
 * @module Settings
 */

/** The two inks a corner can be drawn in - the words the Rust `Mode` enum serialises to. */
export type Mode = "dark" | "light";

/** Which monitor edge an independent rounded taskbar currently occupies. */
export type TaskbarEdge = "none" | "bottom" | "top" | "left" | "right";

/**
 * Anti-aliased taskbar overlay frame in DPI-independent viewport percentages
 * (`LeftPct/TopPct/WidthPct/HeightPct`) paired with exact physical pixel
 * geometry (`WidthPx/HeightPx/RadiusPx`) so the SVG rim stays locked 1:1 to
 * the Win32 `CreateRoundRectRgn` clip across all DPIs, text scales, and hotplugs.
 */
export type TaskbarFrame = {
	Visible: boolean;
	Edge?: TaskbarEdge;
	LeftPct: number;
	TopPct: number;
	WidthPct: number;
	HeightPct: number;
	WidthPx: number;
	HeightPx: number;
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

	workLeft: number;

	workRight: number;

	workBottom: number;

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

/** `listen("work_inset", ...)` payload: `Message::WorkInset` (viewport percentages). */
export type WorkInsetMessage = {
	message: { WorkInset: { Left: number; Right: number; Bottom: number } };
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
