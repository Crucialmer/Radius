import { currentMonitor } from "@tauri-apps/api/window";

import type { Work } from "../Type/Panel.js";

/** The work area of the monitor the panel is on, in exact physical pixels plus
 *  logical height and DPI scale so mixed-DPI and 8K multi-monitor clamping never
 *  drifts across scale factors. */
export default async ():Promise<Work | undefined> => {
	const Monitor = await currentMonitor().catch(() => null);

	if (!Monitor) {
		const Scale = window.devicePixelRatio || 1;
		const Width = window.screen.availWidth;
		const Height = window.screen.availHeight;

		return Height > 0
			? {
					LeftPx: 0,
					TopPx: 0,
					WidthPx: Math.round(Width * Scale),
					HeightPx: Math.round(Height * Scale),
					Height: Math.floor(Height),
					Scale,
				}
			: undefined;
	}

	const Scale = Math.max(1, Monitor.scaleFactor || 1);
	const WidthPx = Monitor.workArea.size.width;
	const HeightPx = Monitor.workArea.size.height;
	const Height = HeightPx / Scale;

	return Height > 0
		? {
				LeftPx: Monitor.workArea.position.x,
				TopPx: Monitor.workArea.position.y,
				WidthPx,
				HeightPx,
				Height: Math.floor(Height),
				Scale,
			}
		: undefined;
};
