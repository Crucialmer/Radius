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
	SizeMessage,
	TaskbarFrame,
	TaskbarFrameMessage,
	WorkInsetMessage,
} from "./Type/Settings.js";

const Corners = ["BottomLeft", "BottomRight", "TopLeft", "TopRight"];
const WorkCorners = ["WorkBottomLeft", "WorkBottomRight"];

/**
 * One overlay: four G2 Squircle screen corners, optional maximized bottom
 * corners above the taskbar, and an anti-aliased Liquid Glass taskbar rim.
 */
function Overlay() {
	const Current = getCurrentWebviewWindow();

	/** The ink this monitor's corners are drawn in. */
	const [Mode, setMode] = createSignal(window.settings.mode);

	/** The corner radius this monitor is drawn with, in logical pixels. */
	const [Radius, setRadius] = createSignal(window.settings.size);

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

	/** Anti-aliased taskbar rim frame pushed by the Windows watcher. */
	const [Taskbar, setTaskbar] = createSignal<TaskbarFrame>(
		window.settings.taskbarFrame ?? {
			Visible: false,
			Left: 0,
			Top: 0,
			Width: 0,
			Height: 0,
			Radius: 0,
		},
	);

	const ShowMaxCorners = () => MaximizeCorners() && Maximized() && WorkInset().Bottom > 1;

	const ShowTaskbarRim = () => Taskbar().Visible && Taskbar().Radius > 0 && Taskbar().Width > 2 && Taskbar().Height > 2;

	onMount(() => {
		void Current.setIgnoreCursorEvents(true);

		const Target = { target: Current.label };

		let Disposed = false;

		const Unlisten: Array<() => void> = [];

		const Register = <T,>(Event: string, Handler: (Payload: T) => void) => {
			void listen<T>(Event, ({ payload }) => Handler(payload), Target).then((Off) => {
				if (Disposed) {
					Off();
				} else {
					Unlisten.push(Off);
				}
			});
		};

		Register<ModeMessage>("mode", (payload) => setMode(payload.message.Mode));
		Register<SizeMessage>("size", (payload) => setRadius(payload.message.Size));
		Register<MaximizeCornersMessage>("maximize_corners", (payload) =>
			setMaximizeCorners(payload.message.MaximizeCorners),
		);
		Register<MaximizedMessage>("maximized", (payload) => setMaximized(payload.message.Maximized));
		Register<WorkInsetMessage>("work_inset", (payload) => setWorkInset(payload.message.WorkInset));
		Register<TaskbarFrameMessage>("taskbar_frame", (payload) => setTaskbar(payload.message.TaskbarFrame));

		onCleanup(() => {
			Disposed = true;
			Unlisten.forEach((Off) => Off());
		});
	});

	return (
		<div
			class="Window"
			data-label={Current.label}
			data-mode={Mode()}
			style={`--Corner: ${Radius()}px; --WorkLeft: ${WorkInset().Left}px; --WorkRight: ${WorkInset().Right}px; --WorkBottom: ${WorkInset().Bottom}px`}>
			<For each={Corners}>{(corner) => <Corner id={corner} />}</For>
			<Show when={ShowMaxCorners()}>
				<For each={WorkCorners}>{(corner) => <Corner id={corner} />}</For>
			</Show>
			<Show when={ShowTaskbarRim()}>
				<TaskbarRim
					left={Taskbar().Left}
					top={Taskbar().Top}
					width={Taskbar().Width}
					height={Taskbar().Height}
					radius={Taskbar().Radius}
				/>
			</Show>
		</div>
	);
}

render(() => <Overlay />, document.getElementById("window") as HTMLElement);
