import type { State } from "../Type/Panel.js";

import Measure from "./Measure.js";

/** 以简体中文完整展示当前所选的作用域与显示器规格说明。 */
export default (state:State, label:string | undefined) => {
	const index = label === undefined ? -1 : state.monitors.findIndex((monitor) => monitor.label === label);

	const monitor = index < 0 ? undefined : state.monitors[index];

	return monitor === undefined
		? `全部显示器 · 共连接 ${state.monitors.length} 台`
		: `第 ${index + 1}/${state.monitors.length} 台 · ${monitor.name} · ${Measure(monitor)}`;
};
