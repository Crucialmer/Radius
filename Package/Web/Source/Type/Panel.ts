import type { Accessor } from "solid-js";

/** One setting as one monitor sees it: the value it is drawn with, and whether
 *  that value is its own. */
export type Value<T> = { value:T; origin:"inherit" | "override" };

/** One monitor, with every setting already resolved for it. */
export type Monitor = {
	label:string;
	name:string;
	width:number;
	height:number;
	radius:Value<number>;
	ink:Value<"dark" | "light">;
	hidden:Value<boolean>;
};

/** GitHub release update check and in-app download snapshot. */
export type UpdateInfo = {
	status:"idle" | "checking" | "up_to_date" | "available" | "downloading" | "installing" | "error";
	current_version:string;
	latest_version?:string | null;
	release_url?:string | null;
	download_url?:string | null;
	asset_name?:string | null;
	downloaded_bytes:number;
	total_bytes:number;
	progress:number;
	message?:string | null;
};

/** Everything the panel draws, returned by every command. */
export type State = {
	radius:number;
	maximum:number;
	taskbar_radius:number;
	taskbar_maximum:number;
	ink:"dark" | "light";
	hidden:boolean;
	dock_hidden:boolean;
	taskbar_rounded:boolean;
	maximize_corners:boolean;
	autostart:boolean;
	version:string;
	update:UpdateInfo;
	monitors:Monitor[];
};

/** What every row in the panel shares: the Rust state, the command that writes
 *  it, and the scope the rows apply to. */
export type Shared = {
	State:Accessor<State | undefined>;
	call:(command:string, args?:Record<string, unknown>) => Promise<void>;
	Scope:() => string | undefined;
};

/** A work area in physical pixels plus its monitor DPI scale factor. */
export type Work = {
	LeftPx:number;
	TopPx:number;
	WidthPx:number;
	HeightPx:number;
	Height:number;
	Scale:number;
};
