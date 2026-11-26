import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { createEffect, createSignal, Index, onCleanup, onMount, Show } from "solid-js";

import Context from "../Context.js";
import Describe from "../Fn/Describe.js";
import Fit from "../Fn/Fit.js";
import Name from "../Fn/Name.js";
import type { State, UpdateInfo } from "../Type/Panel.js";

import Row from "./Row.jsx";
import Setting from "./Setting.jsx";
import Slider from "./Slider.jsx";

/** 托盘左键/右键弹出的 macOS 26 Liquid Glass 风格简体中文控制面板。 */
export default () => {
	const Window = getCurrentWebviewWindow();

	const [State, Sync] = createSignal<State>();

	const [Selected, Select] = createSignal("");

	const Scope = () => Selected() || undefined;

	const call = async (command:string, args?:Record<string, unknown>) => {
		try {
			Sync(await invoke<State>(command, args));
		} catch (Error) {
			console.error(`${command} failed`, Error);
		}
	};

	let Element:HTMLDivElement | undefined;
	let Picker:HTMLSelectElement | undefined;

	createEffect(() => {
		const label = Scope();

		if (label !== undefined && !(State()?.monitors ?? []).some((monitor) => monitor.label === label)) {
			Select("");
		}
	});

	createEffect(() => {
		const value = Selected();

		State();

		if (Picker) Picker.value = value;
	});

	onMount(() => {
		void call("state");

		let Disposed = false;
		let LastHeight = -1;
		let Unlisten:(() => void) | undefined;

		const ScheduleFit = () => {
			const Measured = Element;

			if (!Measured) return;

			const NextHeight = Math.ceil(Measured.getBoundingClientRect().height);

			if (NextHeight === LastHeight) return;

			LastHeight = NextHeight;

			requestAnimationFrame(() => {
				if (!Disposed && Element) void Fit(Element, Window);
			});
		};

		const Observer = new ResizeObserver(ScheduleFit);

		if (Element) {
			Observer.observe(Element);
			ScheduleFit();
		}

		const OnKeyDown = (event:KeyboardEvent) => {
			if (event.key === "Escape") {
				void invoke("dismiss");
			}
		};

		const OnFocus = () => {
			void call("state");
		};

		window.addEventListener("keydown", OnKeyDown);
		window.addEventListener("focus", OnFocus);

		void listen<UpdateInfo>(
			"update",
			({ payload }) => {
				Sync((prev) => (prev ? { ...prev, update:payload } : prev));
			},
			{ target: Window.label },
		).then((Off) => {
			if (Disposed) {
				Off();
			} else {
				Unlisten = Off;
			}
		});

		onCleanup(() => {
			Disposed = true;
			Observer.disconnect();
			window.removeEventListener("keydown", OnKeyDown);
			window.removeEventListener("focus", OnFocus);
			Unlisten?.();
		});
	});

	const IsBusyUpdate = (info:UpdateInfo) =>
		info.status === "checking" || info.status === "downloading" || info.status === "installing";

	const UpdateText = (info:UpdateInfo) => {
		if (info.message) return info.message;
		if (info.status === "checking") return "正在检查更新...";
		if (info.status === "available") return `发现新版本 v${info.latest_version ?? ""}`;
		if (info.status === "downloading") return `正在下载更新 · ${info.progress}%`;
		if (info.status === "installing") return "正在重启并安装更新...";
		if (info.status === "up_to_date") return "当前已是最新版本";
		if (info.status === "error") return "检查或下载更新失败";
		return "点击右侧按钮检查 GitHub 新版本";
	};

	return (
		<Context.Provider value={{ State, call, Scope }}>
			<div class="Panel" ref={Element}>
				<Show when={State()} fallback={<p class="Empty">正在读取配置...</p>}>
					{(state) => (
						<>
							<div class="Group">
								<h2 class="Title">
									<svg class="SectionIcon" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
										<path d="M1.5 2.75A1.75 1.75 0 0 1 3.25 1h9.5A1.75 1.75 0 0 1 14.5 2.75v7.5A1.75 1.75 0 0 1 12.75 12H9.6l.45 1.75h1.2a.625.625 0 0 1 0 1.25h-6.5a.625.625 0 0 1 0-1.25h1.2L6.4 12H3.25A1.75 1.75 0 0 1 1.5 10.25v-7.5zm1.75-.5a.5.5 0 0 0-.5.5v7.5a.5.5 0 0 0 .5.5h9.5a.5.5 0 0 0 .5-.5v-7.5a.5.5 0 0 0-.5-.5h-9.5z" />
									</svg>
									显示器范围
								</h2>

								<div class="PickerWrap">
									<select
										class="Picker"
										ref={Picker}
										title="选择下方设置应用到的显示器范围"
										value={Selected()}
										onChange={(event) => Select(event.currentTarget.value)}>
										<option value="">全部显示器</option>

										<Index each={state().monitors}>
											{(monitor, index) => (
												<option value={monitor().label}>{Name(monitor(), index)}</option>
											)}
										</Index>
									</select>
									<svg class="PickerChevron" viewBox="0 0 12 12" fill="none" aria-hidden="true">
										<path
											d="M3.5 4.75L6 2.25L8.5 4.75M3.5 7.25L6 9.75L8.5 7.25"
											stroke="currentColor"
											stroke-width="1.35"
											stroke-linecap="round"
											stroke-linejoin="round"
										/>
									</svg>
								</div>

								<p class="Where">{Describe(state(), Scope())}</p>
							</div>

							<div class="Group">
								<h2 class="Title">
									<svg class="SectionIcon" viewBox="0 0 16 16" fill="none" aria-hidden="true">
										<path
											d="M2.5 11.5V6.25A3.75 3.75 0 0 1 6.25 2.5h5.25"
											stroke="currentColor"
											stroke-width="1.5"
											stroke-linecap="round"
										/>
										<circle cx="11.5" cy="11.5" r="2" fill="currentColor" opacity="0.55" />
									</svg>
									屏幕圆角
								</h2>

								<Setting
									title="圆角大小"
									word="radius"
									value={state().radius}
									monitors={state().monitors}
									scope={Scope()}
									pick={(monitor) => monitor.radius}
									set={(label, value) =>
										call("set_size", label === undefined ? { value } : { label, value })
									}
									control={(value, commit) => (
										<Slider value={value()} maximum={state().maximum} Commit={commit} />
									)}
								/>

								<Setting
									title="边角颜色"
									word="ink"
									value={state().ink}
									monitors={state().monitors}
									scope={Scope()}
									pick={(monitor) => monitor.ink}
									set={(label, mode) =>
										call("set_mode", label === undefined ? { mode } : { label, mode })
									}
									control={(value, commit) => (
										<div class="Segmented">
											<button
												type="button"
												classList={{ On:value() === "dark" }}
												onClick={() => commit("dark")}>
												黑色
											</button>
											<button
												type="button"
												classList={{ On:value() === "light" }}
												onClick={() => commit("light")}>
												白色
											</button>
										</div>
									)}
								/>

								<Setting
									title="圆角显示"
									word="hidden"
									value={state().hidden}
									monitors={state().monitors}
									scope={Scope()}
									pick={(monitor) => monitor.hidden}
									set={(label, hidden) =>
										call("set_hidden", label === undefined ? { hidden } : { label, hidden })
									}
									control={(value, commit) => (
										<div class="Segmented">
											<button
												type="button"
												classList={{ On:!value() }}
												onClick={() => commit(false)}>
												显示
											</button>
											<button
												type="button"
												classList={{ On:value() }}
												onClick={() => commit(true)}>
												隐藏
											</button>
										</div>
									)}
								/>

								<p class="Note">Apple G2 连续曲率，默认 15px；点击右侧 ↺ 可恢复默认</p>

								<hr class="GroupDivider" />

								<div class="ToggleItem">
									<div class="ToggleHeader">
										<span class="ToggleLabel">最大化底部圆角</span>
										<label class="Switch" title="窗口最大化时为其底部紧邻任务栏的两角添加与屏幕一致的圆角">
											<input
												type="checkbox"
												checked={state().maximize_corners}
												onChange={(event) =>
													void call("set_maximize_corners", {
														enabled: event.currentTarget.checked,
													})
												}
											/>
											<span class="SwitchTrack">
												<span class="SwitchThumb" />
											</span>
										</label>
									</div>
									<p class="Note">跟随上方屏幕圆角的大小、颜色与显隐，为最大化窗口底部补齐圆角</p>
								</div>
							</div>

							<div class="Group">
								<h2 class="Title">
									<svg class="SectionIcon" viewBox="0 0 16 16" fill="none" aria-hidden="true">
										<rect
											x="1.75"
											y="5.5"
											width="12.5"
											height="5"
											rx="2.5"
											stroke="currentColor"
											stroke-width="1.4"
										/>
										<circle cx="5.25" cy="8" r="0.9" fill="currentColor" />
										<circle cx="8" cy="8" r="0.9" fill="currentColor" />
										<circle cx="10.75" cy="8" r="0.9" fill="currentColor" />
									</svg>
									任务栏圆角
								</h2>

								<div class="ToggleItem">
									<div class="ToggleHeader">
										<span class="ToggleLabel">任务栏四角圆角</span>
										<label class="Switch" title="独立裁切任务栏四角并叠加抗锯齿玻璃描边，不参与屏幕圆角设置">
											<input
												type="checkbox"
												checked={state().taskbar_rounded}
												onChange={(event) =>
													void call("set_taskbar_rounded", {
														enabled: event.currentTarget.checked,
													})
												}
											/>
											<span class="SwitchTrack">
												<span class="SwitchThumb" />
											</span>
										</label>
									</div>
									<p class="Note">独立于屏幕圆角；开启后任务栏处单独裁切并叠加抗锯齿玻璃描边</p>
								</div>

								<Row
									label="圆角大小"
									hint="所有显示器的任务栏共用此独立圆角值，默认 8px，不与屏幕圆角联动"
									disabled={!state().taskbar_rounded}
									reset="恢复默认值（8）"
									Reset={() => void call("reset", { setting:"taskbar_radius" })}>
									<Slider
										value={state().taskbar_radius}
										maximum={state().taskbar_maximum}
										disabled={!state().taskbar_rounded}
										Commit={(value) => void call("set_taskbar_size", { value })}
									/>
								</Row>
							</div>

							<div class="Group">
								<h2 class="Title">
									<svg class="SectionIcon" viewBox="0 0 16 16" fill="none" aria-hidden="true">
										<path
											d="M8 2.25v4.5m0 0L6 4.75m2 2 2-2M3.25 10.25A4.75 4.75 0 0 0 8 13.75a4.75 4.75 0 0 0 4.75-3.5"
											stroke="currentColor"
											stroke-width="1.35"
											stroke-linecap="round"
											stroke-linejoin="round"
										/>
									</svg>
									系统与更新
								</h2>

								<div class="ToggleItem">
									<div class="ToggleHeader">
										<span class="ToggleLabel">开机自动启动</span>
										<label class="Switch" title="登录 Windows 时自动在后台启动 Radius">
											<input
												type="checkbox"
												checked={state().autostart}
												onChange={(event) =>
													void call("set_autostart", {
														enabled: event.currentTarget.checked,
													})
												}
											/>
											<span class="SwitchTrack">
												<span class="SwitchThumb" />
											</span>
										</label>
									</div>
									<p class="Note">登录系统后自动加载屏幕与任务栏圆角</p>
								</div>

								<div class="UpdateCard">
									<div class="UpdateBox">
										<div class="UpdateInfo">
											<span class="ToggleLabel">软件更新</span>
											<span
												class="Note"
												classList={{
													UpdateHighlight:
														state().update.status === "available" ||
														state().update.status === "downloading" ||
														state().update.status === "installing",
													UpdateError: state().update.status === "error",
												}}
												title={UpdateText(state().update)}>
												{UpdateText(state().update)}
											</span>
										</div>

										<div class="UpdateActions">
											<Show when={state().update.status === "available" && state().update.download_url}>
												<button
													type="button"
													class="ActionBtn Primary"
													onClick={() => void call("download_update")}>
													立即更新
												</button>
											</Show>

											<Show
												when={
													state().update.status === "available" ||
													state().update.status === "error"
												}>
												<button
													type="button"
													class="ActionBtn"
													title="在浏览器中打开 GitHub 发布页"
													onClick={() => void invoke("open_release")}>
													发布页
												</button>
											</Show>

											<Show when={state().update.status !== "available"}>
												<button
													type="button"
													class="ActionBtn"
													disabled={IsBusyUpdate(state().update)}
													onClick={() => void call("check_update")}>
													{state().update.status === "checking"
														? "检查中..."
														: state().update.status === "downloading"
															? `${state().update.progress}%`
															: state().update.status === "installing"
																? "安装中..."
																: "检查更新"}
												</button>
											</Show>
										</div>
									</div>

									<Show
										when={
											state().update.status === "downloading" ||
											state().update.status === "installing"
										}>
										<div class="UpdateProgress" aria-label="更新下载进度">
											<div
												class="UpdateBarFill"
												classList={{
													Indeterminate:
														state().update.status === "installing" ||
														(state().update.status === "downloading" &&
															state().update.total_bytes === 0),
												}}
												style={{
													width:
														state().update.status === "installing"
															? "100%"
															: `${Math.max(4, state().update.progress)}%`,
												}}
											/>
										</div>
									</Show>
								</div>
							</div>

							<div class="Footer">
								<span class="Version">Radius v{state().version}</span>
								<button type="button" class="Quit" onClick={() => void invoke("quit")}>
									退出 Radius
								</button>
							</div>
						</>
					)}
				</Show>
			</div>
		</Context.Provider>
	);
};
