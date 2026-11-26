import { Show, type JSX } from "solid-js";

/** One row: a label, that row's controls, and the reset that undoes them. */
export default (properties:{
	label:string;
	hint:string;
	mark?:string | undefined;
	reset?:string | undefined;
	disabled?:boolean | undefined;
	Reset?:(() => void) | undefined;
	children:JSX.Element;
}) => (
	<div class="Row" classList={{ Disabled: Boolean(properties.disabled) }}>
		<span title={properties.hint}>
			<span class="Name">{properties.label}</span>
			<Show when={properties.mark}>
				{(mark) => (
					<span class="Mark" title={mark()}>
						•
					</span>
				)}
			</Show>
		</span>
		{properties.children}
		<Show when={properties.reset}>
			{(reset) => (
				<button
					type="button"
					class="Reset"
					title={reset()}
					disabled={properties.disabled}
					onClick={() => properties.Reset?.()}>
					↺
				</button>
			)}
		</Show>
	</div>
);
