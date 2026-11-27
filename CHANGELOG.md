# 更新日志

本文件记录 **Radius** 的版本变更。

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
