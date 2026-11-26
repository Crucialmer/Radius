import { useContext, type JSX } from "solid-js";

import Context from "../Context.js";
import type { Monitor, Value } from "../Type/Panel.js";

import Row from "./Row.jsx";

/** 单个可按显示器独立覆盖的设置行。 */
export default <T,>(properties:{
	title:string;
	value:T;
	monitors:Monitor[];
	scope:string | undefined;
	pick:(monitor:Monitor) => Value<T>;
	control:(value:() => T, commit:(next:T) => void) => JSX.Element;
	set:(label:string | undefined, next:T) => void;
	word:string;
}) => {
	const Shared = useContext(Context);

	const Shown = () => {
		const label = properties.scope;

		return label === undefined ? undefined : properties.monitors.find((monitor) => monitor.label === label);
	};

	const Drawn = () => {
		const monitor = Shown();

		return monitor === undefined ? properties.value : properties.pick(monitor).value;
	};

	const Mark = () => {
		const monitor = Shown();

		if (monitor !== undefined) {
			return properties.pick(monitor).origin === "override"
				? "当前显示器已单独设置；点击右侧重置可恢复跟随全局默认"
				: undefined;
		}

		const count = properties.monitors.filter((each) => properties.pick(each).origin === "override").length;

		if (count === 0) return undefined;

		return `有 ${count} 台显示器已单独设置；在此修改或重置将清除这些独立设置`;
	};

	return (
		<Row
			label={properties.title}
			hint={
				Shown() === undefined
					? "全局默认配置，未单独设置的显示器将跟随此值"
					: "当前显示器的独立配置，或跟随全局默认"
			}
			mark={Mark()}
			reset={
				Shown() === undefined
					? "恢复默认值并清除所有显示器的独立设置"
					: "恢复默认值，仅清除当前显示器的独立设置"
			}
			Reset={() =>
				Shared?.call(
					"reset",
					Shown() === undefined
						? { setting:properties.word }
						: { setting:properties.word, label:properties.scope },
				)
			}>
			{properties.control(Drawn, (next) => properties.set(properties.scope, next))}
		</Row>
	);
};
