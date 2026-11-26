/**
 * @module Settings
 */

/** The two inks a corner can be drawn in - the words the Rust `Mode` enum serialises to. */
export type Mode = "dark" | "light";

/** Anti-aliased taskbar overlay frame in logical pixels relative to this monitor. */
export type TaskbarFrame = {
	Visible: boolean;
	Left: number;
	Top: number;
	Width: number;
	Height: number;
	Radius: number;
};

/** What the Rust side injects as `window.settings` before the first paint. */
export type Settings = {
	size: number;

	mode: Mode;

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

/** `listen("maximize_corners", ...)` payload: `Message::MaximizeCorners`. */
export type MaximizeCornersMessage = {
	message: { MaximizeCorners: boolean };
};

/** `listen("maximized", ...)` payload: `Message::Maximized`. */
export type MaximizedMessage = {
	message: { Maximized: boolean };
};

/** `listen("work_inset", ...)` payload: `Message::WorkInset`. */
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
