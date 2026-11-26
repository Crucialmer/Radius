import { createEffect, createSignal } from "solid-js";

/** A macOS 26 Liquid Glass range slider with a live-synced tabular readout.
 *  Updates the numeric badge and accent track on every drag frame while
 *  committing once on release so one drag is one disk write. */
export default (properties:{
	value:number;
	maximum:number;
	disabled?:boolean | undefined;
	Commit:(value:number) => void;
}) => {
	const [value, Write] = createSignal(properties.value);

	createEffect(() => Write(properties.value));

	const Progress = () => `${Math.min(100, Math.max(0, (value() / Math.max(1, properties.maximum)) * 100))}%`;

	return (
		<>
			<input
				type="range"
				min="0"
				max={properties.maximum}
				step="1"
				disabled={properties.disabled}
				value={value()}
				style={{ "--fill": Progress() }}
				onInput={(event) => Write(Number(event.currentTarget.value))}
				onChange={() => properties.Commit(value())}
			/>
			<span class="Number">{value()}</span>
		</>
	);
};
