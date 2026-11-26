import { LogicalPosition, LogicalSize } from "@tauri-apps/api/dpi";
import type { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

import Locate from "./Locate.js";

/** The width the panel is drawn at, in logical pixels - `Menu::Width`. */
const Width = 356;

/** The transparent margin the window keeps around the panel, on each side,
 *  giving the macOS 26 Liquid Glass elevation shadow room without clipping. */
const Margin = 10;

/** Logical pixels of the work area kept clear of the panel: the gap it hangs
 *  below the tray icon by, plus the margin above and below it. */
const Reserve = 6 + 2 * Margin;

/** Size the window to the panel, and keep the panel inside the work area. */
export default async (Element:HTMLDivElement, Window:ReturnType<typeof getCurrentWebviewWindow>) => {
	const Area = await Locate();

	// The cap goes on before the measurement, not after it: the panel scrolls
	// inside the window, so the height it measures is the height it is drawn at.
	Element.style.maxHeight = Area ? `${Math.max(Area.Height - Reserve, 2 * Margin)}px` : "";

	const Height = Math.ceil(Element.getBoundingClientRect().height) + 2 * Margin;

	await Window.setSize(new LogicalSize(Width, Height)).catch((error) =>
		console.warn("radius: the panel could not be resized", error),
	);

	if (!Area) return;

	const Position = await Window.outerPosition().catch(() => null);

	if (!Position) return;

	const Y = Position.y / Area.Scale;
	const Top = Math.min(Math.max(Y, Area.Top), Math.max(Area.Top, Area.Top + Area.Height - Height));

	if (Math.abs(Top - Y) < 1) return;

	await Window.setPosition(new LogicalPosition(Position.x / Area.Scale, Top)).catch((error) =>
		console.warn("radius: the panel could not be moved back on screen", error),
	);
};
