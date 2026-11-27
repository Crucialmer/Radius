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

	/** Logical pixel insets between the monitor rect and the work area. */
	const [WorkInset, setWorkInset] = createSignal({
		Left: window.settings.workLeft ?? 0,
		Right: window.settings.workRight ?? 0,
		Bottom: window.settings.workBottom ?? 0,
	});

	/** Independent taskbar rim frame and occupied edge pushed by the Windows watcher. */
	const [Taskbar, setTaskbar] = createSignal<TaskbarFrame>(
		window.settings.taskbarFrame ?? {
			Visible: false,
			Edge: "none",
			LeftPct: 0,
			TopPct: 0,
			WidthPct: 0,
			HeightPct: 0,
			WidthPx: 0,
			HeightPx: 0,
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
		ScreenVisible() && Radius() > 0 && MaximizeCorners() && Maximized() && WorkInset().Bottom > 0.1;

	const ShowTaskbarRim = () =>
		Taskbar().Visible && Taskbar().RadiusPx > 0 && Taskbar().WidthPx > 2 && Taskbar().HeightPx > 2;

	onMount(() => {
		void Current.setIgnoreCursorEvents(true);

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
		Register<WorkInsetMessage>("work_inset", (payload) => setWorkInset(payload.message.WorkInset));
		Register<TaskbarFrameMessage>("taskbar_frame", (payload) => setTaskbar(payload.message.TaskbarFrame));

		void Promise.all(Ready).then(() => {
			if (!Disposed) {
				void invoke("overlay_ready");
			}
		});

		let ResizeTimer: number | undefined;

		const OnResize = () => {
			if (ResizeTimer !== undefined) {
				window.clearTimeout(ResizeTimer);
			}

			ResizeTimer = window.setTimeout(() => {
				if (!Disposed) {
					void invoke("overlay_ready");
				}
			}, 150);
		};

		window.addEventListener("resize", OnResize);

		onCleanup(() => {
			Disposed = true;
			if (ResizeTimer !== undefined) {
				window.clearTimeout(ResizeTimer);
			}
			window.removeEventListener("resize", OnResize);
			Unlisten.forEach((Off) => Off());
		});
	});

	return (
		<div
			class="Window"
			data-label={Current.label}
			data-mode={Mode()}
			data-max-corners={ShowMaxCorners() ? "true" : "false"}
			style={`--Corner: ${Radius()}px; --WorkLeft: ${WorkInset().Left}vw; --WorkRight: ${WorkInset().Right}vw; --WorkBottom: ${WorkInset().Bottom}vh`}>
			<Show when={ShowScreenCorners()}>
				<For each={ActiveScreenCorners()}>{(corner) => <Corner id={corner} />}</For>
			</Show>
			<Show when={ShowMaxCorners()}>
				<For each={WorkCorners}>{(corner) => <Corner id={corner} />}</For>
			</Show>
			<Show when={ShowTaskbarRim()}>
				<TaskbarRim
					leftPct={Taskbar().LeftPct}
					topPct={Taskbar().TopPct}
					widthPct={Taskbar().WidthPct}
					heightPct={Taskbar().HeightPct}
					widthPx={Taskbar().WidthPx}
					heightPx={Taskbar().HeightPx}
					radiusPx={Taskbar().RadiusPx}
					scale={Taskbar().Scale}
				/>
			</Show>
		</div>
	);
}

render(() => <Overlay />, document.getElementById("window") as HTMLElement);
