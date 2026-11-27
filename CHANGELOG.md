# 更新日志

本文件记录 **Radius** 的版本变更。

---

## 2.0.0

### 架构级重塑：全面升级至原生 Direct3D 11 + Direct2D + DirectComposition GPU 渲染引擎与 Win11 原生 DWM Acrylic 毛玻璃

- **屏幕四角 / 最大化底部圆角 / 任务栏玻璃描边全面迁移至 Rust 原生 GPU 引擎（零 WebView2 覆盖层）**：
  - 新增 `Overlay::Platform::Engine`：使用 Direct3D 11 + Direct2D（`D2D1_ANTIALIAS_MODE_PER_PRIMITIVE`）+ DirectComposition（`IDXGISwapChain1` + `DXGI_ALPHA_MODE_PREMULTIPLIED` + `WS_EX_NOREDIRECTIONBITMAP`）直接以 **1:1 物理像素（`SetDpi(96, 96)`）** 绘制所有屏幕圆角与任务栏玻璃描边。
  - 彻底移除每个显示器常驻的 WebView2 覆盖层进程：常驻内存直降 80%+，彻底根除 Chromium 分数 DPI（如 `131 DPI / 136%`、`110%`、`175%`）视口舍入偏移与多 WebView2 退出卡顿。
  - 原生硬件抗锯齿绘制 Apple G2 连续曲率 Squircle 屏幕圆角与最大化底部圆角，以及严格同心的 macOS 26 Liquid Glass 任务栏外暗影 + 对角线镜面高光折射描边（实时跟随 Windows 系统明暗主题切换）；支持 WARP 自动回退与 `DXGI_ERROR_DEVICE_REMOVED` 设备丢失无感自愈。
- **空白点击拦截层（`Backdrop`）迁移至超轻量原生 Win32 透明窗口（零 WebView2）**：
  - 移除每块显示器的 WebView2 透明遮罩进程，改为原生 `WS_EX_LAYERED` 极轻量 Win32 窗口，面板开启与点击收起毫秒级响应，零内存残留、零卸载阻塞。
- **控制面板升级为 Windows 11 原生 DWM Acrylic 实时毛玻璃材质**：
  - 启用 Windows 11 原生 `DWMWA_SYSTEMBACKDROP_TYPE = DWMSBT_TRANSIENTWINDOW` 实时背景亚克力模糊、`DWMWCP_ROUND` 系统原生圆角阴影与 `DWMWA_USE_IMMERSIVE_DARK_MODE` 明暗模式同步，配合高通透 Liquid Glass 前端着色层与对角线折射描边，真正实时透出并模糊下方壁纸与窗口内容（不支持 Acrylic 的旧系统自动平滑降级）。

---

## 1.2.3

### 修复（彻底解决应用内自动更新时电脑卡顿/卡住的问题）

- **解除更新退出阶段的全屏遮罩与线程竞态死锁**：
  - 新增 `Watch::Stop()`（立即置位停机标志并向后台消息循环发送 `WM_QUIT`，即时卸载所有 `SetWinEventHook` 钩子），避免退出/更新期间后台监控线程继续对任务栏和覆盖层窗口执行 `SetWindowPos` / `SetWindowRgn` 与主线程 `App.exit(0)` 互相抢锁。
  - 在准备重启安装前及点击「退出 Radius」时，立即在主线程隐藏全屏透明点击拦截层（`Backdrop`）、控制面板及全部覆盖层，再恢复任务栏并注销托盘图标，彻底杜绝退出期间透明置顶层卡在桌面拦截鼠标点击导致“电脑卡住”的问题；同时增加 1.5s 兜底退出保护，防止 WebView2 销毁卡死。
- **彻底告别更新时双 PowerShell 进程卡顿与进度事件风暴**：
  - 解压更新包改用 Windows 系统原生 C 工具 `System32\tar.exe`（实测耗时仅 ~29ms，零 .NET CLR 启动与 AMSI 扫描卡顿，仅在极少数无 `tar.exe` 环境下回退 PowerShell）。
  - 自替换重启脚本改用极轻量原生 `System32\cmd.exe`（~130ms，支持中文/空格路径与全语言系统 `tasklist /FO CSV` 精确等待），不再拉起第二个重型 `powershell.exe`。
  - 对下载进度推送增加严格的 `120ms` 节流，消除高速网络下 0.15 秒内连续触发近百次 WebView2 IPC 与面板重绘引发的瞬时卡顿。

---

## 1.2.2

### 核心修复（彻底根治非整倍/自定义缩放下圆角描边向下偏移 1–2px）

- **定位并根治 Chromium / WebView2 分数缩放视口舍入偏移（如 `DPI=131 / 136%`、`110%`、`140%`、`175%`）**：
  - 通过实机坐标诊断与 Edge/Chromium 引擎级复现确认：在非二进制分数 DPI（例如 4K `3840×2160 @ DPI=131 / 136.46%`）下，Chromium 会将 `100vh` / `100%` / `bottom: 0` 的视口高度取整到整数 CSS DIP，导致视口底部实际超出物理屏幕 `1–2px`，从而把原先依赖 `top: TopPct%` 的任务栏玻璃描边整框向下推偏 `1–2px`（顶边露缝、底边高光溢出裁切圆角外）。
  - 彻底改用**物理像素 ÷ 实时 `window.devicePixelRatio`**从精确 `(0, 0)` 原点定位覆盖层容器（`.Window`）、任务栏玻璃描边（`.TaskbarRim`）与最大化底部圆角（`--WorkBottom`），并动态监听 `matchMedia(dppx)` 与 `resize` 实时响应缩放切换，同时移除 `.TaskbarRim` 的 `transform: translateZ(0)` 避免合成层整数 DIP 吸附。经 Edge 无头矩阵实测，在 `100% / 110% / 125% / 131 DPI (136%) / 150% / 175% / 200%` 全档位下顶边与底边误差均为 **`0 px`**。
- **修复前端 `setIgnoreCursorEvents` 覆写覆盖层 Win32 样式的问题**：
  - 将 `set_ignore_cursor_events(true)` 前置到 Rust 侧 `BuildOverlay`（`Platform::Art` 之前）并在 `overlay_ready` 重新稳固无边框工具窗样式，避免 Tao `WindowState::apply_diff` 在启动后将 `WS_CAPTION` / `WS_EX_WINDOWEDGE` 写回并覆盖 `WS_EX_TOOLWINDOW` / `WS_EX_NOACTIVATE`。
  - 将 `Watch` 覆盖层位置自愈阈值由 `> 2px` 收紧至 `!= 0px`，确保任意 `1px` 偏移都能立即校准。

---

## 1.2.1

### 视觉优化与修复（消除圆角阴影向下偏移）

- **修复任务栏圆角玻璃描边（`TaskbarRim`）圆心向下偏移的问题**：
  - 修复 `TaskbarRim-Outer` 与 `TaskbarRim-Specular` 内缩矩形时未同步减去内缩量导致圆角弧心向内/向下偏移（高分屏 150%–400% 下尤为明显）的几何偏差；改为严格同心公式（`OuterRadius = Radius - OuterInset`、`InnerRadius = Radius - InnerInset`，外边沿精确齐平 Win32 `CreateRoundRectRgn` 裁切边界）。
  - 收紧内层镜面高光间距并取消 `.TaskbarRim` 的 `contain: paint` 硬裁切，避免描边外半侧被截断导致视觉重心下沉，同时柔和化明暗模式描边不透明度。
- **强化覆盖层客户区精确对齐与任务栏顶部保护**：
  - 将 `Taskbar::OverlayRect` 升级为 Win32 真实物理客户区屏幕坐标（`GetClientRect` + `ClientToScreen`），消除非客户区边距引起 CSS 视口纵向偏移的可能。
  - 为 `WorkRect` 裁剪增加 `≥ 6dp` 最小步进阈值，避免系统工作区 1–4px 舍入误差误切正常任务栏顶部导致圆角整体下移，同时继续完整支持小任务栏。
- **优化控制面板（`.Panel`）圆角阴影均衡度与零底边裁切**：
  - 将面板外边距由 `14px` 扩大至 `16px`（窗口逻辑宽度同步调整为 `368dp`，内容区仍为 `336dp`），并将环境悬浮阴影由 `0 4px 12px` 精调为更居中均衡的 `0 2px 10px`，彻底消除明暗模式下圆角阴影视觉下坠与底部边缘轻微裁切的问题。

---

## 1.2.0

### 全面升级与优化（macOS 26 美学 · 8K 全缩放 · 多尺寸任务栏适配）

- **macOS 26 (Tahoe) Liquid Glass 控制面板精修**：
  - 新增紧凑型控制中心品牌顶栏（连续曲率圆角徽标 + `Radius` + 版本胶囊标签），优化视觉层级。
  - 将面板透明外边距从 `10px` 扩大至 `14px` 并重新调校外层环境阴影衰减范围，彻底消除浅色壁纸下弹出面板底部阴影被窗口边缘硬裁切的问题；同时为控制面板窗口补齐 `DWMWA_BORDER_COLOR = DWMWA_COLOR_NONE` 与 `set_shadow(false)`，杜绝 Windows 11 矩形 1px DWM 边框。
  - 精细化内嵌圆角阶梯（`22px / 12px / 8px / 胶囊`），补充矮屏滚动细滑条与 `prefers-contrast: more` 高对比度无障碍支持。
  - 升级任务栏玻璃描边为 macOS 26 对角线镜面高光折射层，明暗模式下质感更细腻通透。
- **8K（7680×4320）与 100%–400% 全缩放比例精确适配**：
  - 将任务栏边缘判定、XAML 子桥接紧贴容差、全屏窗口检测容差及自动隐藏任务栏底边下限全部按实时显示器 `Scale` 等比换算，解决 4K/8K（200%–400% 缩放）下自动隐藏任务栏 2dp 激活条（4–8px）误触发最大化底部圆角的问题。
  - 修复多屏混合 DPI / 负坐标左侧屏 / 8K 屏下控制面板边缘贴合逻辑（`Fit.ts` / `Locate.ts`），改用精确物理像素（`PhysicalPosition`）同时约束 X/Y 轴在当前显示器工作区内。
  - 为屏幕圆角添加 `min(calc(var(--Corner) * 1.528665), 50vw, 50vh)` 防重叠保护，并在圆角半径为 `0` 时跳过 SVG/GPU 图层挂载，适配紧凑屏、竖屏到 8K 超高分屏。

---

## 1.1.6

### 修复

- **修复系统托盘出现多个重复图标的 Bug**：
  - 新增 Windows 会话级单实例互斥锁（`CreateMutexW` / `Local\Crucialmer.Radius.SingleInstance`），防止开机自启、重复双击或并发启动导致多个 `Radius.exe` 同时运行并重复注册托盘图标。
  - 新增退出前显式注销托盘图标（`Tray::Remove::Fn`，触发 `Shell_NotifyIconW(NIM_DELETE)`），在点击「退出 Radius」、应用内自动更新重启及 `RunEvent::Exit` 退出前先隐藏并移除旧托盘图标，彻底消除退出或连续自动升级后托盘残留的“幽灵图标”。

---

## 1.1.5

### 修复

- **修复任务栏圆角与玻璃描边位置偏移的 Bug**：
  - 将任务栏描边与最大化工作区定位从依赖 CSS 像素换算（`px / Scale`）彻底升级为**视口归一化比例坐标（`%` / `vw` / `vh`）+ 物理像素 SVG `viewBox`**，直接以覆盖层窗口真实物理矩形（`GetWindowRect`）为基准定位，彻底消除跨 DPI 多屏热插拔、分数缩放（125%/150%/175%）或系统字体缩放下 `devicePixelRatio` 差异导致的任务栏圆角框错位。
  - 将 `.Window` 覆盖层容器锁定为 `position: fixed; inset: 0; width: 100vw; height: 100vh; overflow: hidden`，消除子像素溢出引起的偏移。
  - 收紧任务栏 `DesktopWindowContentBridge` 与 `MaxBottom` 裁切阈值：仅在紧贴屏幕外边缘且存在真实小任务栏高度差（`≥8dp` / `≥12dp`）时才收紧可视边界，避免 Windows 11 内部 XAML 1–6px 内边距或无边框最大化窗口微溢出把任务栏顶部圆角向下错切。

---

## 1.1.4

### 修复

- **修复重新插入外接屏幕后圆角异常、圆角阴影后面出现矩形边框的 Bug**：
  - 强化覆盖层窗口样式净化（[Struct.rs](Crate/Round/Source/Overlay/Platform/Windows/Struct.rs)）：强制关闭阴影（`set_shadow(false)`）、清除非客户区边框标志（`WS_BORDER` / `WS_THICKFRAME` / `WS_CAPTION` / `WS_EX_WINDOWEDGE` 等）、显式设置 Windows 11 `DWMWA_BORDER_COLOR = DWMWA_COLOR_NONE` 与 `DWMWCP_DONOTROUND` 并通过 `SWP_FRAMECHANGED` 立即刷新，彻底消除外接屏重插后 DWM 残留的矩形边框与阴影。
  - 严格校验任务栏裁切区域类型与边界（[Taskbar.rs](Crate/Round/Source/Overlay/Platform/Windows/Taskbar.rs)）：必须为 `COMPLEXREGION` 且与当前可视矩形一致，避免外接屏重插时资源管理器将任务栏重置为矩形 `SIMPLEREGION` 后从玻璃圆角阴影背后露出直角矩形框。
  - 直接通过 `GetDpiForMonitor` 按显示器读取精确 DPI（不再受新窗口跨 DPI 启动瞬间缩放漂移影响），并在 `SnapWindow` 及监控巡检中自动校准因 `WM_DPICHANGED` 偏移/缩放的覆盖层窗口与 WebView2 视图尺寸，将热插拔后跟踪窗口延长至 2.4 秒并立即销毁已拔出屏幕的旧窗口（`destroy()`）。

---

## 1.1.3

### 修复

- **修复拔下外接显示器后软件崩溃与任务栏圆角框虚影移位的 Bug**：
  - 新增隐藏顶层窗口实时监听 Windows 显示拓扑变更广播（`WM_DISPLAYCHANGE` / `WM_SETTINGCHANGE`）及显示器几何+DPI 指纹巡检，拔插外接显示器、切换扩展/仅主屏或更改缩放时自动防抖并在主线程动态同步（`Overlay::Sync::Reconcile`）：干净销毁已断开显示器的覆盖层窗口、为新接入显示器自动创建覆盖层，并将保留的显示器覆盖层精确重定位到最新物理坐标与分辨率。
  - 修复 `MonitorFromPoint` 使用 `MONITOR_DEFAULTTONEAREST` 导致已拔掉的外接屏错误映射到笔记本主屏、产生重影/移位任务栏圆角描边框的问题，改为严格校验 `MONITOR_DEFAULTTONULL` 与实时显示器尺寸一致性。
  - 将后台监听线程中的跨线程 Tauri `scale_factor` / `is_visible` 调用改为纯 Win32 `GetDpiForWindow` / `IsWindow` / `IsWindowVisible`，并在 `WM_DISPLAYCHANGE` 过渡期间暂停窗口操作、拦截非主动退出的 `ExitRequested` 事件，彻底消除外接屏热拔插崩溃。
  - 覆盖层前端新增 `resize` 防抖回拉（`overlay_ready`），显示器尺寸或 DPI 变更后自动刷新任务栏玻璃描边与工作区圆角坐标。

---

## 1.1.2

### 修复

- **修复小任务栏场景下最大化底部圆角错位、出现长条矩形的 Bug**：
  - 将最大化窗口底部圆角锚点从仅依赖系统工作区 `MONITORINFO.rcWork` 改为实时跟踪顶层最大化窗口的实际 DWM 可视边界（`DWMWA_EXTENDED_FRAME_BOUNDS`），并结合实际任务栏边缘与实时 DPI 缩放校准；即使在注册表小任务栏、Windhawk / StartAllBack / ExplorerPatcher 紧凑任务栏或自动隐藏/悬浮任务栏导致 `rcWork` 未同步时，圆角也能精确贴合最大化窗口底边与小任务栏顶边。
  - 优化任务栏可视区域检测（综合 `DWMWA_EXTENDED_FRAME_BOUNDS`、Windows 11 XAML `DesktopWindowContentBridge`、`rcWork` 与最大化窗口底边校准），消除小任务栏上方多余的裁切区域与描边矩形；非满宽悬浮任务栏不再错误隐藏屏幕底角。
  - 限制开启最大化底部圆角时底部屏幕圆角不超出任务栏高度，避免大圆角半径在小任务栏上向上溢出。
  - 新增覆盖层挂载完成握手同步（`overlay_ready`），防止启动瞬间丢失首帧工作区与最大化状态事件。

---

## 1.1.1

### 修复

- **修复应用内自动更新只退出不重启的 Bug**：移除互斥的 Win32 进程创建标志组合 `CREATE_NO_WINDOW | DETACHED_PROCESS`（该组合会导致隐藏启动的 PowerShell 替换脚本在执行前直接退出），改用 `CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP` 并分离标准 IO 句柄；同时去除路径 `\\?\` 前缀、解除下载文件锁定（`Unblock-File`）并在替换失败时兜底拉起原程序，确保自动替换与重启稳定生效。

---

## 1.1.0

### 优化与变更

- **任务栏圆角彻底独立**：开启「任务栏四角圆角」时，任务栏所在屏幕边缘自动排除在屏幕圆角之外，不再被屏幕圆角遮挡；即使将屏幕圆角设为隐藏，任务栏圆角与抗锯齿玻璃描边依然保持独立生效（全屏视频/游戏覆盖任务栏时自动恢复四角屏幕圆角）。
- **最大化底部圆角并入屏幕圆角**：将「最大化底部圆角」合并至「屏幕圆角」分组，完整跟随屏幕圆角的大小、颜色与显隐设置。
- **控制面板分组重构**：将原先混排的「任务栏与窗口」拆分为清晰的「屏幕圆角（含最大化底部圆角）」与独立的「任务栏圆角」分组，任务栏总开关关闭时其半径滑块平滑呈现禁用状态。

---

## 1.0.0

首个正式整理发布版本：

- **全新视觉图标**：以圆角弧线与点阵几何为核心设计 macOS 26 Liquid Glass 质感应用图标与高对比度系统托盘图标。
- **macOS 26 Liquid Glass 控制面板**：同心圆角（`22px / 12px / 胶囊`）分组玻璃卡片、对角线镜面高光、深浅色自适应、实时同步滑块、`Esc` 关闭及无障碍支持。
- **Apple G2 连续曲率屏幕圆角**：三段三次贝塞尔连续圆角（`1.528665 × R`），支持多屏混合 DPI 与 8K 显示器及逐屏独立设置。
- **任务栏圆角与抗锯齿美化**：独立任务栏圆角半径（0–24px，默认 8px），叠加硬件抗锯齿 Liquid Glass 玻璃描边平滑裁切边缘，并在全屏应用时自动隐藏。
- **最大化窗口底部圆角**：自动检测最大化窗口并补齐紧邻任务栏上方的两角圆角。
- **应用内自动更新**：支持原生 WinHTTP 检查更新、实时进度条直下 `Radius.zip`、PE 校验、原子替换并自动重启。
- **纯净绿色便携 & 极致轻量**：数据全量收敛于程序同级目录；采用 O(1) 任务栏定位与零堆分配窗口检测，开启 `lto = "thin"` 与 `strip = true` 编译优化。
