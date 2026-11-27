import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { render } from "solid-js/web";

import "./Asset/CSS/Window.css";

import Corner from "./Element/Corner.jsx";
import TaskbarRim from "./Element/TaskbarRim.jsx";
import type {
	MaximizeCornersMessage,
	MaximizedMessage,
	ModeMessage,
	ScreenVisibleMessage,
	SizeMessage,
	TaskbarFrame,
	TaskbarFrameMessage,
	WorkInsetMessage,
} from "./Type/Settings.js";

const AllCorners = ["BottomLeft", "BottomRight", "TopLeft", "TopRight"];
const WorkCorners = ["WorkBottomLeft", "WorkBottomRight"];

/**
 * One overlay:
 * - Screen corners (and maximized-window bottom corners) controlled by screen settings;
 *   when independent taskbar rounding is active on an edge, those two taskbar-edge
 *   corners are excluded from screen corner painting.
 * - Independent anti-aliased Liquid Glass taskbar rim, unaffected by screen corner settings.
 */
function Overlay() {
	const Current = getCurrentWebviewWindow();

	/** The ink this monitor's screen & maximized corners are drawn in. */
	const [Mode, setMode] = createSignal(window.settings.mode);

	/** The corner radius this monitor's screen & maximized corners use, in logical pixels. */
	const [Radius, setRadius] = createSignal(window.settings.size);

	/** Whether this monitor's screen & maximized corners are currently visible. */
	const [ScreenVisible, setScreenVisible] = createSignal(window.settings.screenVisible ?? true);

	/** Whether maximized-window bottom corner rounding is enabled. */
	const [MaximizeCorners, setMaximizeCorners] = createSignal(Boolean(window.settings.maximizeCorners));

	/** Whether a maximized window is currently visible on this monitor. */
	const [Maximized, setMaximized] = createSignal(Boolean(window.settings.maximized));

	/** Exact physical pixel work-area insets relative to this overlay window. */
	const [WorkInset, setWorkInset] = createSignal({
		LeftPx: window.settings.workLeftPx ?? 0,
		RightPx: window.settings.workRightPx ?? 0,
		BottomPx: window.settings.workBottomPx ?? 0,
	});

	/** Exact physical overlay client dimensions (avoids Chromium integer-DIP `100vh` overshoot). */
	const [OverlaySize, setOverlaySize] = createSignal({
		Width: window.settings.overlayW ?? 0,
		Height: window.settings.overlayH ?? 0,
	});

	/** Live WebView2 devicePixelRatio for exact `(physicalPx / dpr) * dpr === physicalPx` mapping. */
	const [Dpr, setDpr] = createSignal(window.devicePixelRatio || 1);

	/** Independent taskbar rim frame and occupied edge pushed by the Windows watcher. */
	const [Taskbar, setTaskbar] = createSignal<TaskbarFrame>(
		window.settings.taskbarFrame ?? {
			Visible: false,
			Edge: "none",
			LeftPx: 0,
			TopPx: 0,
			WidthPx: 0,
			HeightPx: 0,
			OverlayW: window.settings.overlayW ?? 0,
			OverlayH: window.settings.overlayH ?? 0,
			RadiusPx: 0,
			Scale: 1,
		},
	);

	/** Screen corners excluding the edge owned by an active independent rounded taskbar. */
	const ActiveScreenCorners = () => {
		const Edge = Taskbar().Edge ?? "none";

		if (Edge === "bottom") return ["TopLeft", "TopRight"];
		if (Edge === "top") return ["BottomLeft", "BottomRight"];
		if (Edge === "left") return ["TopRight", "BottomRight"];
		if (Edge === "right") return ["TopLeft", "BottomLeft"];

		return AllCorners;
	};

	const ShowScreenCorners = () => ScreenVisible() && Radius() > 0;

	const ShowMaxCorners = () =>
		ScreenVisible() && Radius() > 0 && MaximizeCorners() && Maximized() && WorkInset().BottomPx > 1;

	const ShowTaskbarRim = () =>
		Taskbar().Visible && Taskbar().RadiusPx > 0 && Taskbar().WidthPx > 2 && Taskbar().HeightPx > 2;

	onMount(() => {
		const Target = { target: Current.label };

		let Disposed = false;

		const Unlisten: Array<() => void> = [];

		const Ready: Array<Promise<void>> = [];

		const Register = <T,>(Event: string, Handler: (Payload: T) => void) => {
			Ready.push(
				listen<T>(Event, ({ payload }) => Handler(payload), Target).then((Off) => {
					if (Disposed) {
						Off();
					} else {
						Unlisten.push(Off);
					}
				}),
			);
		};

		Register<ModeMessage>("mode", (payload) => setMode(payload.message.Mode));
		Register<SizeMessage>("size", (payload) => setRadius(payload.message.Size));
		Register<ScreenVisibleMessage>("screen_visible", (payload) =>
			setScreenVisible(payload.message.ScreenVisible),
		);
		Register<MaximizeCornersMessage>("maximize_corners", (payload) =>
			setMaximizeCorners(payload.message.MaximizeCorners),
		);
		Register<MaximizedMessage>("maximized", (payload) => setMaximized(payload.message.Maximized));
		Register<WorkInsetMessage>("work_inset", (payload) => {
			const Inset = payload.message.WorkInset;
			setWorkInset({ LeftPx: Inset.LeftPx, RightPx: Inset.RightPx, BottomPx: Inset.BottomPx });
			if (Inset.OverlayW > 0 && Inset.OverlayH > 0) {
				setOverlaySize({ Width: Inset.OverlayW, Height: Inset.OverlayH });
			}
		});
		Register<TaskbarFrameMessage>("taskbar_frame", (payload) => {
			const Frame = payload.message.TaskbarFrame;
			setTaskbar(Frame);
			if (Frame.OverlayW > 0 && Frame.OverlayH > 0) {
				setOverlaySize({ Width: Frame.OverlayW, Height: Frame.OverlayH });
			}
		});

		void Promise.all(Ready).then(() => {
			if (!Disposed) {
				void invoke("overlay_ready");
			}
		});

		let ResizeTimer: number | undefined;

		let RemoveDprWatch: (() => void) | undefined;

		const WatchDpr = () => {
			RemoveDprWatch?.();
			const CurrentDpr = window.devicePixelRatio || 1;
			setDpr(CurrentDpr);
			const Media = window.matchMedia(`(resolution: ${CurrentDpr}dppx)`);
			const OnChange = () => {
				if (!Disposed) {
					WatchDpr();
					void invoke("overlay_ready");
				}
			};
			Media.addEventListener("change", OnChange, { once: true });
			RemoveDprWatch = () => Media.removeEventListener("change", OnChange);
		};

		WatchDpr();

		const OnResize = () => {
			setDpr(window.devicePixelRatio || 1);

			if (ResizeTimer !== undefined) {
				window.clearTimeout(ResizeTimer);
			}

			ResizeTimer = window.setTimeout(() => {
				if (!Disposed) {
					setDpr(window.devicePixelRatio || 1);
					void invoke("overlay_ready");
				}
			}, 150);
		};

		window.addEventListener("resize", OnResize);

		onCleanup(() => {
			Disposed = true;
			RemoveDprWatch?.();
			if (ResizeTimer !== undefined) {
				window.clearTimeout(ResizeTimer);
			}
			window.removeEventListener("resize", OnResize);
			Unlisten.forEach((Off) => Off());
		});
	});

	const WindowStyle = () => {
		const Scale = Math.max(0.5, Dpr());
		const Box = OverlaySize();
		const WidthStyle = Box.Width > 0 ? `width: ${Box.Width / Scale}px;` : "width: 100vw;";
		const HeightStyle = Box.Height > 0 ? `height: ${Box.Height / Scale}px;` : "height: 100vh;";

		return `${WidthStyle} ${HeightStyle} --Corner: ${Radius()}px; --WorkLeft: ${WorkInset().LeftPx / Scale}px; --WorkRight: ${WorkInset().RightPx / Scale}px; --WorkBottom: ${WorkInset().BottomPx / Scale}px`;
	};

	return (
		<div
			class="Window"
			data-label={Current.label}
			data-mode={Mode()}
			data-max-corners={ShowMaxCorners() ? "true" : "false"}
			style={WindowStyle()}>
			<Show when={ShowScreenCorners()}>
				<For each={ActiveScreenCorners()}>{(corner) => <Corner id={corner} />}</For>
			</Show>
			<Show when={ShowMaxCorners()}>
				<For each={WorkCorners}>{(corner) => <Corner id={corner} />}</For>
			</Show>
			<Show when={ShowTaskbarRim()}>
				<TaskbarRim
					leftPx={Taskbar().LeftPx}
					topPx={Taskbar().TopPx}
					widthPx={Taskbar().WidthPx}
					heightPx={Taskbar().HeightPx}
					radiusPx={Taskbar().RadiusPx}
					scale={Taskbar().Scale}
					dpr={Dpr()}
				/>
			</Show>
		</div>
	);
}

render(() => <Overlay />, document.getElementById("window") as HTMLElement);
