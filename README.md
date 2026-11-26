<p align="center">
  <img width="88" height="88" alt="Radius" src="Crate/Round/icons/128x128@2x.png">
</p>

<h1 align="center">Radius</h1>

<p align="center">
  轻量、纯净、绿色便携的 Windows 屏幕圆角与任务栏美化工具<br />
  采用 <strong>macOS 26 Liquid Glass</strong> 设计美学 · 支持多屏混合 DPI 与 8K 显示器
</p>

<p align="center">
  <a href="https://github.com/Crucialmer/Radius/releases"><img src="https://img.shields.io/github/v/release/Crucialmer/Radius?label=最新版本&color=ff3a80" alt="最新版本"></a>
  <img src="https://img.shields.io/static/v1?label=平台&message=Windows%2010%2F11&color=007aff" alt="Windows 10/11">
  <img src="https://img.shields.io/static/v1?label=体积&message=~1.9MB%20便携版&color=34c759" alt="便携版">
  <a href="LICENSE"><img src="https://img.shields.io/static/v1?label=协议&message=CC0-1.0&color=lightgrey" alt="CC0-1.0"></a>
</p>

---

## 核心特性

- **Apple G2 连续曲率圆角（Squircle）**：采用 Apple 规范三段三次贝塞尔连续圆角算法（`1.528665 × R` 融合过渡，边界曲率从 0 平滑起坡），配合高精度矢量 SVG 渲染，告别机械圆弧的衔接生硬感。
- **任务栏四角圆角 + 抗锯齿玻璃描边**：对 Windows 主副屏任务栏进行四角圆角裁切（独立 0–24px 半径），并在点击穿透覆盖层上叠加硬件抗锯齿 **Liquid Glass 流光描边**，光学消除 GDI 裁切锯齿；遇到全屏视频或游戏自动隐退。
- **最大化窗口底部补齐圆角**：应用窗口最大化时自动检测所在显示器工作区，为紧贴任务栏上方的两角补齐流畅圆角。
- **macOS 26 Liquid Glass 控制面板**：点击托盘图标即可弹出同心圆角玻璃面板（`22px / 12px / 胶囊`），具备对角线镜面高光、分组玻璃卡片、实时同步数值的液态胶囊滑块、深浅色自适应、`Esc` 快捷关闭及减少透明/减弱动态无障碍支持。
- **多屏混合缩放 & 8K（7680×4320）适配**：支持全局或逐显示器独立配置圆角半径、黑/白边角颜色与显隐状态；覆盖层按物理像素精确对齐，配合 CSS 图层隔离（`contain: layout paint size`）保持极低资源占用。
- **应用内一键检查更新与自动重启安装**：通过原生 WinHTTP 检查 GitHub Releases 新版本，支持在面板内直接下载、实时显示进度条与下载字节，自动校验并无缝重启完成替换。
- **纯净绿色便携运行**：配置文件（`settings.json`）、更新缓存（`./data/update`）与 WebView2 运行数据（`./data/webview`）全部收纳在 `Radius.exe` 同级目录，不污染 `%APPDATA%` 或 `%LOCALAPPDATA%`（仅在手动开启「开机自动启动」时写入当前用户的自启项）；删除目录即可彻底卸载。

---

## 快速使用

1. 前往 [Releases 页面](https://github.com/Crucialmer/Radius/releases) 下载最新版 **`Radius.zip`**；
2. 解压得到单文件 **`Radius.exe`**，放到任意文件夹后双击运行（无主窗口、无任务栏占位，仅驻留系统托盘）；
3. 左键或右键点击系统托盘图标，打开控制面板按需调节：
   - **显示器范围**：选择「全部显示器」或单独针对某一台显示器微调；
   - **屏幕圆角**：调整圆角大小（0–200px，默认 15px）、边角颜色（黑/白）、显隐状态以及跟随屏幕圆角设置的「最大化底部圆角」；
   - **任务栏圆角**：独立控制任务栏四角圆角开关与圆角大小（0–24px，默认 8px）；开启后任务栏所在边缘不再参与屏幕圆角，并自动叠加抗锯齿玻璃描边；
   - **系统与更新**：开关开机自动启动，检查并一键下载安装新版本。

---

## 从源码构建（Windows）

环境要求：Rust Nightly 工具链（见 [rust-toolchain.toml](rust-toolchain.toml)）、Node.js ≥ 22、pnpm 12。

```powershell
git clone https://github.com/Crucialmer/Radius.git
cd Radius
pnpm install
pnpm run Build
cargo build --release
.\Target\release\Radius.exe
```

常用校验命令：

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test
```

---

## 项目结构

- **`Crate/Round`** — Rust 核心与桌面入口（`Radius.exe`），包含多屏覆盖层管理、WinEvent 置顶守护、任务栏圆角裁切、开机自启与原生 WinHTTP 更新下载器。
- **`Package/Web`** — Solid 1.9 + Vite 8 前端面板与矢量圆角覆盖层，编译后直接嵌入二进制文件。
- **`Crate/Maintain`** — 版本号同步与发布辅助工具。

---

## 更新日志

见 [CHANGELOG.md](CHANGELOG.md)。

## 开源协议

本项目基于 [CC0-1.0](LICENSE) 公有领域协议开源。
