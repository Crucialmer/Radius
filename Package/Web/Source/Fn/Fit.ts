import { LogicalSize, PhysicalPosition } from "@tauri-apps/api/dpi";
import type { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

import Locate from "./Locate.js";

/** The width the panel is drawn at, in logical pixels - matches `Menu::Width`. */
const Width = 368;

/** Transparent shadow margin on every side so the macOS 26 Liquid Glass ambient
 *  elevation shadow fades smoothly with zero window-edge clipping. */
const Margin = 16;

/** Logical pixels of work area kept clear of the panel: anchor gap + vertical margins. */
const Reserve = 6 + 2 * Margin;

/** Size the window to the panel and clamp both X and Y cleanly inside the
 *  monitor's exact physical work area across 1080p–8K and mixed-DPI setups. */
export default async (Element:HTMLDivElement, Window:ReturnType<typeof getCurrentWebviewWindow>) => {
	const Area = await Locate();

	// Cap before measurement so scroll height stays strictly within the screen's work area.
	Element.style.maxHeight = Area ? `${Math.max(Area.Height - Reserve, 2 * Margin)}px` : "";

	const Height = Math.ceil(Element.getBoundingClientRect().height) + 2 * Margin;

	await Window.setSize(new LogicalSize(Width, Height)).catch((error) =>
		console.warn("radius: the panel could not be resized", error),
	);

	if (!Area) return;

	const Position = await Window.outerPosition().catch(() => null);

	if (!Position) return;

	const WidthPx = Math.round(Width * Area.Scale);
	const HeightPx = Math.round(Height * Area.Scale);

	const MinX = Area.LeftPx;
	const MaxX = Math.max(MinX, Area.LeftPx + Area.WidthPx - WidthPx);
	const MinY = Area.TopPx;
	const MaxY = Math.max(MinY, Area.TopPx + Area.HeightPx - HeightPx);

	const ClampedX = Math.round(Math.min(Math.max(Position.x, MinX), MaxX));
	const ClampedY = Math.round(Math.min(Math.max(Position.y, MinY), MaxY));

	if (Math.abs(ClampedX - Position.x) < 2 && Math.abs(ClampedY - Position.y) < 2) return;

	await Window.setPosition(new PhysicalPosition(ClampedX, ClampedY)).catch((error) =>
		console.warn("radius: the panel could not be moved back on screen", error),
	);
};
