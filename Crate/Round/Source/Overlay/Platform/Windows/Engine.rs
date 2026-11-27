//! Native hardware-accelerated Direct3D 11 + Direct2D + DirectComposition
//! transparent overlay engine for Windows.
//!
//! Renders Apple G2 continuous curvature (Squircle) screen corners, maximized
//! window bottom corners, and concentric macOS 26 Liquid Glass taskbar rims
//! directly in 1:1 physical monitor pixels (`SetDpi(96, 96)`) with
//! `DXGI_ALPHA_MODE_PREMULTIPLIED` and `WS_EX_NOREDIRECTIONBITMAP`:
//! - Zero WebView2 processes for screen overlays (sub-8 MB resident memory).
//! - Exact 0px error across 1080p–8K and all fractional/custom DPI scales
//!   (including 110%, 131 DPI / 136.46%, 140%, 175%, 200%–400%).
//! - Retained-mode GPU rendering: only redraws when visual state changes.
//! - Automatic WARP fallback and `DXGI_ERROR_DEVICE_REMOVED` recovery.

use std::{
	collections::HashMap,
	sync::{
		Mutex,
		OnceLock,
		atomic::{AtomicBool, AtomicIsize, Ordering},
	},
};

use crate::{
	Mode::Enum::Enum as Mode,
	Per::Of as PerOf,
	Screen::Struct::Struct as Screen,
	Settings::Struct::Struct as Settings,
	Visibility::Enum::Enum as Visibility,
};

/// Exact physical-pixel taskbar Liquid Glass rim state for one monitor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TaskbarRimState {
	pub Visible:bool,
	pub Edge:&'static str,
	pub LeftPx:i32,
	pub TopPx:i32,
	pub WidthPx:i32,
	pub HeightPx:i32,
	pub RadiusPx:i32,
	pub ScaleMilli:i32,
}

/// Exact physical-pixel work-area inset state for maximized-window bottom corners.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorkInsetState {
	pub Maximized:bool,
	pub LeftPx:i32,
	pub RightPx:i32,
	pub BottomPx:i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MonitorConfig {
	Label:String,
	PhysicalX:i32,
	PhysicalY:i32,
	WidthPx:i32,
	HeightPx:i32,
	ScaleMilli:i32,
	RadiusDp:u32,
	LightInk:bool,
	ScreenVisible:bool,
	MaximizeCorners:bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MonitorVisualKey {
	WidthPx:i32,
	HeightPx:i32,
	ScaleMilli:i32,
	RadiusDp:u32,
	LightInk:bool,
	ScreenVisible:bool,
	MaximizeCorners:bool,
	Work:WorkInsetState,
	Rim:TaskbarRimState,
	DarkTheme:bool,
}

#[derive(Default)]
struct SharedState {
	Monitors:Vec<MonitorConfig>,
	WorkInsets:HashMap<String, WorkInsetState>,
	TaskbarRims:HashMap<String, TaskbarRimState>,
	DarkTheme:bool,
	HiddenAll:bool,
}

static Desired:OnceLock<Mutex<SharedState>> = OnceLock::new();

type LiveHwndMap = HashMap<String, (isize, i32, i32, i32, i32)>;

static LiveHwnds:OnceLock<Mutex<LiveHwndMap>> = OnceLock::new();

static ControlHwnd:AtomicIsize = AtomicIsize::new(0);

static Stopping:AtomicBool = AtomicBool::new(false);

const WM_APP_SYNC:u32 = 0x8000 + 1;

const WM_APP_HIDE_ALL:u32 = 0x8000 + 2;

const WM_APP_SHUTDOWN:u32 = 0x8000 + 3;

fn StateLock() -> &'static Mutex<SharedState> {
	Desired.get_or_init(|| Mutex::new(SharedState { DarkTheme:ReadSystemDarkTheme(), ..SharedState::default() }))
}

fn HwndLock() -> &'static Mutex<LiveHwndMap> { LiveHwnds.get_or_init(|| Mutex::new(HashMap::new())) }

fn Wake(Msg:u32) {
	let Hwnd = ControlHwnd.load(Ordering::Acquire);

	if Hwnd != 0 {
		unsafe {
			let _ = windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
				Hwnd as windows_sys::Win32::Foundation::HWND,
				Msg,
				0,
				0,
			);
		}
	}
}

/// Read Windows' `SystemUsesLightTheme` registry value (`0` => Dark theme).
pub fn ReadSystemDarkTheme() -> bool {
	use windows_sys::Win32::System::Registry::{
		HKEY,
		HKEY_CURRENT_USER,
		KEY_READ,
		REG_DWORD,
		RegCloseKey,
		RegOpenKeyExW,
		RegQueryValueExW,
	};

	let SubKey:Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0"
		.encode_utf16()
		.collect();

	let ValueName:Vec<u16> = "SystemUsesLightTheme\0".encode_utf16().collect();

	let mut Key:HKEY = std::ptr::null_mut();

	unsafe {
		if RegOpenKeyExW(HKEY_CURRENT_USER, SubKey.as_ptr(), 0, KEY_READ, &mut Key) != 0 {
			return false;
		}

		let mut Data:u32 = 1;

		let mut DataSize = std::mem::size_of::<u32>() as u32;

		let mut Kind:u32 = 0;

		let Found = RegQueryValueExW(
			Key,
			ValueName.as_ptr(),
			std::ptr::null(),
			&mut Kind,
			&mut Data as *mut u32 as *mut u8,
			&mut DataSize,
		) == 0
			&& Kind == REG_DWORD;

		let _ = RegCloseKey(Key);

		Found && Data == 0
	}
}

/// Read Windows' `AppsUseLightTheme` registry value (`0` => Dark mode for apps/panel).
pub fn ReadAppsDarkTheme() -> bool {
	use windows_sys::Win32::System::Registry::{
		HKEY,
		HKEY_CURRENT_USER,
		KEY_READ,
		REG_DWORD,
		RegCloseKey,
		RegOpenKeyExW,
		RegQueryValueExW,
	};

	let SubKey:Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0"
		.encode_utf16()
		.collect();

	let ValueName:Vec<u16> = "AppsUseLightTheme\0".encode_utf16().collect();

	let mut Key:HKEY = std::ptr::null_mut();

	unsafe {
		if RegOpenKeyExW(HKEY_CURRENT_USER, SubKey.as_ptr(), 0, KEY_READ, &mut Key) != 0 {
			return false;
		}

		let mut Data:u32 = 1;

		let mut DataSize = std::mem::size_of::<u32>() as u32;

		let mut Kind:u32 = 0;

		let Found = RegQueryValueExW(
			Key,
			ValueName.as_ptr(),
			std::ptr::null(),
			&mut Kind,
			&mut Data as *mut u32 as *mut u8,
			&mut DataSize,
		) == 0
			&& Kind == REG_DWORD;

		let _ = RegCloseKey(Key);

		Found && Data == 0
	}
}

/// Start the dedicated Direct3D 11 + Direct2D + DirectComposition overlay thread once.
pub fn Start() {
	if ControlHwnd.load(Ordering::Acquire) != 0 {
		return;
	}

	Stopping.store(false, Ordering::Release);

	let (ReadyTx, ReadyRx) = std::sync::mpsc::sync_channel::<()>(1);

	let _ = std::thread::Builder::new().name("radius-gpu-overlay".into()).spawn(move || {
		RunRenderLoop(ReadyTx);
	});

	let _ = ReadyRx.recv_timeout(std::time::Duration::from_secs(3));
}

/// Immediately hide every native overlay window (safe from any thread).
pub fn HideAll() {
	{
		let mut Guard = StateLock().lock().unwrap_or_else(|Poisoned| Poisoned.into_inner());
		Guard.HiddenAll = true;
	}

	// Hide synchronously via Win32 `ShowWindow(SW_HIDE)` so shutdown/update never
	// waits for the message loop to drain.
	let Hwnds:Vec<isize> = HwndLock()
		.lock()
		.unwrap_or_else(|Poisoned| Poisoned.into_inner())
		.values()
		.map(|(Hwnd, ..)| *Hwnd)
		.collect();

	for Raw in Hwnds {
		if Raw != 0 {
			unsafe {
				windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow(
					Raw as windows_sys::Win32::Foundation::HWND,
					windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE,
				);
			}
		}
	}

	Wake(WM_APP_HIDE_ALL);
}

/// Stop the native GPU overlay thread and release all Win32 & DirectComposition resources.
pub fn Stop() {
	Stopping.store(true, Ordering::Release);
	HideAll();
	Wake(WM_APP_SHUTDOWN);
}

/// Synchronize per-monitor geometry, screen corner radius/ink/visibility, and
/// maximize-corner toggle with the native GPU renderer (no-op if not started).
pub fn SyncSettings(Preferences:&Settings, Screens:&[Screen]) {
	if ControlHwnd.load(Ordering::Acquire) == 0 {
		return;
	}

	let Next:Vec<MonitorConfig> = Screens
		.iter()
		.map(|Item| MonitorConfig {
			Label:Item.Label.clone(),
			PhysicalX:Item.PhysicalX,
			PhysicalY:Item.PhysicalY,
			WidthPx:Item.Pixels.0.max(1),
			HeightPx:Item.Pixels.1.max(1),
			ScaleMilli:(Item.ScaleFactor.max(1.0) * 1000.0).round() as i32,
			RadiusDp:u32::try_from(PerOf::Fn(&Preferences.Radius, &Item.Label).Value()).unwrap_or(0),
			LightInk:matches!(PerOf::Fn(&Preferences.Ink, &Item.Label), Mode::Light),
			ScreenVisible:matches!(PerOf::Fn(&Preferences.Visible, &Item.Label), Visibility::Shown),
			MaximizeCorners:Preferences.MaximizeCorners,
		})
		.collect();

	{
		let mut Guard = StateLock().lock().unwrap_or_else(|Poisoned| Poisoned.into_inner());
		Guard.HiddenAll = false;
		Guard.Monitors = Next;
		Guard.DarkTheme = ReadSystemDarkTheme();
	}

	Wake(WM_APP_SYNC);
}

/// Update one monitor's maximized-window work-area insets and trigger a redraw if changed.
pub fn UpdateWorkInset(Label:&str, Next:WorkInsetState) {
	let Changed = {
		let mut Guard = StateLock().lock().unwrap_or_else(|Poisoned| Poisoned.into_inner());

		if Guard.WorkInsets.get(Label).copied() == Some(Next) {
			false
		} else {
			Guard.WorkInsets.insert(Label.to_owned(), Next);
			true
		}
	};

	if Changed {
		Wake(WM_APP_SYNC);
	}
}

/// Update one monitor's taskbar Liquid Glass rim frame and trigger a redraw if changed.
pub fn UpdateTaskbarRim(Label:&str, Next:TaskbarRimState) {
	let Changed = {
		let mut Guard = StateLock().lock().unwrap_or_else(|Poisoned| Poisoned.into_inner());

		if Guard.TaskbarRims.get(Label).copied() == Some(Next) {
			false
		} else {
			Guard.TaskbarRims.insert(Label.to_owned(), Next);
			true
		}
	};

	if Changed {
		Wake(WM_APP_SYNC);
	}
}

/// Re-read the live Windows system Light/Dark theme and repaint taskbar rims if changed.
pub fn RefreshTheme() {
	let Dark = ReadSystemDarkTheme();

	let Changed = {
		let mut Guard = StateLock().lock().unwrap_or_else(|Poisoned| Poisoned.into_inner());

		if Guard.DarkTheme == Dark {
			false
		} else {
			Guard.DarkTheme = Dark;
			true
		}
	};

	if Changed {
		Wake(WM_APP_SYNC);
	}
}

/// Return the live physical screen rect of one monitor's native overlay window.
pub fn OverlayRect(Label:&str) -> Option<windows_sys::Win32::Foundation::RECT> {
	let Guard = HwndLock().lock().unwrap_or_else(|Poisoned| Poisoned.into_inner());

	let &(_, Left, Top, Right, Bottom) = Guard.get(Label)?;

	(Right > Left && Bottom > Top).then_some(windows_sys::Win32::Foundation::RECT {
		left:Left,
		top:Top,
		right:Right,
		bottom:Bottom,
	})
}

/// Return all live `(Label, HWND)` native overlay windows for Z-order maintenance.
pub fn AllHwnds() -> Vec<(String, isize)> {
	HwndLock()
		.lock()
		.unwrap_or_else(|Poisoned| Poisoned.into_inner())
		.iter()
		.map(|(Label, &(Hwnd, ..))| (Label.clone(), Hwnd))
		.collect()
}

// ============================================================================
// Native Win32 + Direct3D 11 + Direct2D + DirectComposition implementation
// ============================================================================

struct GpuCore {
	_D3dDevice:windows::Win32::Graphics::Direct3D11::ID3D11Device,
	DxgiFactory:windows::Win32::Graphics::Dxgi::IDXGIFactory2,
	D2dFactory:windows::Win32::Graphics::Direct2D::ID2D1Factory1,
	_D2dDevice:windows::Win32::Graphics::Direct2D::ID2D1Device,
	D2dCtx:windows::Win32::Graphics::Direct2D::ID2D1DeviceContext,
	DComp:windows::Win32::Graphics::DirectComposition::IDCompositionDevice,
}

impl GpuCore {
	fn New() -> windows::core::Result<Self> {
		use windows::{
			Win32::Graphics::{
				Direct2D::{
					D2D1_DEVICE_CONTEXT_OPTIONS_NONE,
					D2D1_FACTORY_TYPE_SINGLE_THREADED,
					D2D1CreateFactory,
					ID2D1Factory1,
				},
				Direct3D::{
					D3D_DRIVER_TYPE,
					D3D_DRIVER_TYPE_HARDWARE,
					D3D_DRIVER_TYPE_WARP,
					D3D_FEATURE_LEVEL_10_0,
					D3D_FEATURE_LEVEL_10_1,
					D3D_FEATURE_LEVEL_11_0,
					D3D_FEATURE_LEVEL_11_1,
				},
				Direct3D11::{D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device},
				DirectComposition::{DCompositionCreateDevice, IDCompositionDevice},
				Dxgi::{CreateDXGIFactory2, DXGI_CREATE_FACTORY_FLAGS, IDXGIDevice, IDXGIFactory2},
			},
			core::Interface,
		};

		let Levels = [
			D3D_FEATURE_LEVEL_11_1,
			D3D_FEATURE_LEVEL_11_0,
			D3D_FEATURE_LEVEL_10_1,
			D3D_FEATURE_LEVEL_10_0,
		];

		let TryCreate = |Driver:D3D_DRIVER_TYPE| -> windows::core::Result<ID3D11Device> {
			let mut Device:Option<ID3D11Device> = None;

			unsafe {
				D3D11CreateDevice(
					None,
					Driver,
					windows::Win32::Foundation::HMODULE::default(),
					D3D11_CREATE_DEVICE_BGRA_SUPPORT,
					Some(&Levels),
					D3D11_SDK_VERSION,
					Some(&mut Device),
					None,
					None,
				)?;
			}

			Device.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))
		};

		let D3dDevice = TryCreate(D3D_DRIVER_TYPE_HARDWARE).or_else(|_| TryCreate(D3D_DRIVER_TYPE_WARP))?;

		let DxgiDevice:IDXGIDevice = D3dDevice.cast()?;

		let DxgiFactory:IDXGIFactory2 = unsafe { CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))? };

		let D2dFactory:ID2D1Factory1 = unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)? };

		let D2dDevice = unsafe { D2dFactory.CreateDevice(&DxgiDevice)? };

		let D2dCtx = unsafe { D2dDevice.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)? };

		unsafe {
			D2dCtx.SetDpi(96.0, 96.0);
			D2dCtx.SetAntialiasMode(windows::Win32::Graphics::Direct2D::D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
		}

		let DComp:IDCompositionDevice = unsafe { DCompositionCreateDevice(&DxgiDevice)? };

		Ok(Self {
			_D3dDevice:D3dDevice,
			DxgiFactory,
			D2dFactory,
			_D2dDevice:D2dDevice,
			D2dCtx,
			DComp,
		})
	}
}

struct MonitorSurface {
	Hwnd:windows::Win32::Foundation::HWND,
	SwapChain:windows::Win32::Graphics::Dxgi::IDXGISwapChain1,
	_Target:windows::Win32::Graphics::DirectComposition::IDCompositionTarget,
	_Visual:windows::Win32::Graphics::DirectComposition::IDCompositionVisual,
	PhysicalX:i32,
	PhysicalY:i32,
	WidthPx:i32,
	HeightPx:i32,
	Visible:bool,
	LastKey:Option<MonitorVisualKey>,
}

impl Drop for MonitorSurface {
	fn drop(&mut self) {
		if !self.Hwnd.0.is_null() {
			unsafe {
				let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(self.Hwnd);
			}
		}
	}
}

fn CreateOverlayWindow(
	ClassName:&[u16],
	X:i32,
	Y:i32,
	W:i32,
	H:i32,
) -> windows::core::Result<windows::Win32::Foundation::HWND> {
	use windows::{
		Win32::{
			Graphics::Dwm::{
				DWMWA_BORDER_COLOR,
				DWMWA_COLOR_NONE,
				DWMWA_WINDOW_CORNER_PREFERENCE,
				DWMWCP_DONOTROUND,
				DwmSetWindowAttribute,
			},
			System::LibraryLoader::GetModuleHandleW,
			UI::WindowsAndMessaging::{
				CreateWindowExW,
				WS_CLIPSIBLINGS,
				WS_EX_LAYERED,
				WS_EX_NOACTIVATE,
				WS_EX_NOREDIRECTIONBITMAP,
				WS_EX_TOOLWINDOW,
				WS_EX_TOPMOST,
				WS_EX_TRANSPARENT,
				WS_POPUP,
			},
		},
		core::PCWSTR,
	};

	let HInstance = unsafe { GetModuleHandleW(None)? };

	let ExStyle = WS_EX_TOPMOST
		| WS_EX_TOOLWINDOW
		| WS_EX_NOACTIVATE
		| WS_EX_TRANSPARENT
		| WS_EX_LAYERED
		| WS_EX_NOREDIRECTIONBITMAP;

	let Hwnd = unsafe {
		CreateWindowExW(
			ExStyle,
			PCWSTR(ClassName.as_ptr()),
			PCWSTR::null(),
			WS_POPUP | WS_CLIPSIBLINGS,
			X,
			Y,
			W,
			H,
			None,
			None,
			Some(HInstance.into()),
			None,
		)?
	};

	unsafe {
		let Corner = DWMWCP_DONOTROUND;

		let _ = DwmSetWindowAttribute(
			Hwnd,
			DWMWA_WINDOW_CORNER_PREFERENCE,
			&Corner as *const _ as *const core::ffi::c_void,
			std::mem::size_of_val(&Corner) as u32,
		);

		let NoBorder = DWMWA_COLOR_NONE;

		let _ = DwmSetWindowAttribute(
			Hwnd,
			DWMWA_BORDER_COLOR,
			&NoBorder as *const _ as *const core::ffi::c_void,
			std::mem::size_of_val(&NoBorder) as u32,
		);
	}

	Ok(Hwnd)
}

fn CreateSurface(Core:&GpuCore, ClassName:&[u16], Config:&MonitorConfig) -> windows::core::Result<MonitorSurface> {
	use windows::Win32::Graphics::Dxgi::{
		Common::{DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
		DXGI_SCALING_STRETCH,
		DXGI_SWAP_CHAIN_DESC1,
		DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
		DXGI_USAGE_RENDER_TARGET_OUTPUT,
	};

	let W = Config.WidthPx.max(1);

	let H = Config.HeightPx.max(1);

	let Hwnd = CreateOverlayWindow(ClassName, Config.PhysicalX, Config.PhysicalY, W, H)?;

	let Desc = DXGI_SWAP_CHAIN_DESC1 {
		Width:W as u32,
		Height:H as u32,
		Format:DXGI_FORMAT_B8G8R8A8_UNORM,
		Stereo:false.into(),
		SampleDesc:DXGI_SAMPLE_DESC { Count:1, Quality:0 },
		BufferUsage:DXGI_USAGE_RENDER_TARGET_OUTPUT,
		BufferCount:2,
		Scaling:DXGI_SCALING_STRETCH,
		SwapEffect:DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
		AlphaMode:DXGI_ALPHA_MODE_PREMULTIPLIED,
		Flags:0,
	};

	let SwapChain = unsafe { Core.DxgiFactory.CreateSwapChainForComposition(&Core._D3dDevice, &Desc, None)? };

	let Target = unsafe { Core.DComp.CreateTargetForHwnd(Hwnd, true)? };

	let Visual = unsafe { Core.DComp.CreateVisual()? };

	unsafe {
		Visual.SetContent(&SwapChain)?;
		Target.SetRoot(&Visual)?;
		Core.DComp.Commit()?;
	}

	Ok(MonitorSurface {
		Hwnd,
		SwapChain,
		_Target:Target,
		_Visual:Visual,
		PhysicalX:Config.PhysicalX,
		PhysicalY:Config.PhysicalY,
		WidthPx:W,
		HeightPx:H,
		Visible:false,
		LastKey:None,
	})
}

/// Add one authentic Apple G2 continuous curvature (Squircle) corner cutout to
/// a Direct2D path sink in exact physical pixel coordinates.
///
/// `(OriginX, OriginY)` is the sharp outer corner vertex; `(DirX, DirY)` are
/// unit direction signs (`+1.0` or `-1.0`) pointing inward along X and Y.
fn DrawSquircleCorner(
	Core:&GpuCore,
	Brush:&windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
	OriginX:f32,
	OriginY:f32,
	DirX:f32,
	DirY:f32,
	SpanPx:f32,
) -> windows::core::Result<()> {
	use windows::{
		Win32::Graphics::Direct2D::{
			Common::{D2D1_BEZIER_SEGMENT, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED},
			ID2D1Geometry,
		},
		core::Interface,
	};
	use windows_numerics::Vector2;

	if SpanPx <= 0.5 {
		return Ok(());
	}

	let Pt = |U:f32, V:f32| Vector2 { X:OriginX + U * SpanPx * DirX, Y:OriginY + V * SpanPx * DirY };

	let Path = unsafe { Core.D2dFactory.CreatePathGeometry()? };

	let Sink = unsafe { Path.Open()? };

	unsafe {
		Sink.BeginFigure(Pt(0.0, 0.0), D2D1_FIGURE_BEGIN_FILLED);
		Sink.AddLine(Pt(1.0, 0.0));
		Sink.AddBezier(&D2D1_BEZIER_SEGMENT {
			point1:Pt(0.7120, 0.0),
			point2:Pt(0.5681, 0.0),
			point3:Pt(0.4131, 0.0490),
		});
		Sink.AddBezier(&D2D1_BEZIER_SEGMENT {
			point1:Pt(0.2439, 0.1106),
			point2:Pt(0.1106, 0.2439),
			point3:Pt(0.0490, 0.4131),
		});
		Sink.AddBezier(&D2D1_BEZIER_SEGMENT { point1:Pt(0.0, 0.5681), point2:Pt(0.0, 0.7120), point3:Pt(0.0, 1.0) });
		Sink.EndFigure(D2D1_FIGURE_END_CLOSED);
		Sink.Close()?;

		let Geom:ID2D1Geometry = Path.cast()?;

		Core.D2dCtx.FillGeometry(&Geom, Brush, None);
	}

	Ok(())
}

/// Draw a concentric hardware-anti-aliased macOS 26 Liquid Glass rim aligned 1:1
/// in physical pixels over the clipped Windows taskbar boundary.
fn DrawTaskbarRim(Core:&GpuCore, Rim:&TaskbarRimState, DarkTheme:bool) -> windows::core::Result<()> {
	use windows::Win32::Graphics::Direct2D::{
		Common::{D2D_RECT_F, D2D1_COLOR_F, D2D1_GRADIENT_STOP},
		D2D1_EXTEND_MODE_CLAMP,
		D2D1_GAMMA_2_2,
		D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES,
		D2D1_ROUNDED_RECT,
		ID2D1RenderTarget,
	};
	use windows_numerics::Vector2;

	if !Rim.Visible || Rim.RadiusPx <= 0 || Rim.WidthPx <= 2 || Rim.HeightPx <= 2 {
		return Ok(());
	}

	let Left = Rim.LeftPx as f32;

	let Top = Rim.TopPx as f32;

	let W = Rim.WidthPx as f32;

	let H = Rim.HeightPx as f32;

	let Scale = ((Rim.ScaleMilli as f32) / 1000.0).max(1.0);

	let Radius = (Rim.RadiusPx as f32).min(W * 0.5).min(H * 0.5);

	// Layer 1: Exact flush concentric outer contact shadow covering the GDI clip seam.
	let OuterW = (1.0 * Scale).max(1.0);

	let OuterInset = OuterW * 0.5;

	let OuterR = (Radius - OuterInset).max(0.0);

	let OuterColor = if DarkTheme {
		D2D1_COLOR_F { r:0.0, g:0.0, b:0.0, a:0.34 }
	} else {
		D2D1_COLOR_F { r:15.0 / 255.0, g:18.0 / 255.0, b:26.0 / 255.0, a:0.15 }
	};

	let OuterBrush = unsafe { Core.D2dCtx.CreateSolidColorBrush(&OuterColor, None)? };

	let OuterRect = D2D1_ROUNDED_RECT {
		rect:D2D_RECT_F {
			left:Left + OuterInset,
			top:Top + OuterInset,
			right:Left + W - OuterInset,
			bottom:Top + H - OuterInset,
		},
		radiusX:OuterR,
		radiusY:OuterR,
	};

	unsafe {
		Core.D2dCtx.DrawRoundedRectangle(&OuterRect, &OuterBrush, OuterW, None);
	}

	// Layer 2: Concentric diagonal macOS 26 Liquid Glass specular refraction rim.
	let InnerW = (0.85 * Scale).max(0.75);

	let InnerInset = OuterW - InnerW * 0.5 + 0.18 * Scale;

	let InnerR = (Radius - InnerInset).max(0.0);

	if W > InnerInset * 2.0 + 1.0 && H > InnerInset * 2.0 + 1.0 {
		let (TopA, MidA, BotA) = if DarkTheme { (0.32, 0.10, 0.18) } else { (0.68, 0.22, 0.36) };

		let Stops = [
			D2D1_GRADIENT_STOP { position:0.0, color:D2D1_COLOR_F { r:1.0, g:1.0, b:1.0, a:TopA } },
			D2D1_GRADIENT_STOP { position:0.46, color:D2D1_COLOR_F { r:1.0, g:1.0, b:1.0, a:MidA } },
			D2D1_GRADIENT_STOP { position:1.0, color:D2D1_COLOR_F { r:1.0, g:1.0, b:1.0, a:BotA } },
		];

		let Collection = unsafe {
			ID2D1RenderTarget::CreateGradientStopCollection(
				&Core.D2dCtx,
				&Stops,
				D2D1_GAMMA_2_2,
				D2D1_EXTEND_MODE_CLAMP,
			)?
		};

		let GradProps = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
			startPoint:Vector2 { X:Left, Y:Top },
			endPoint:Vector2 { X:Left + W * 0.18, Y:Top + H },
		};

		let SpecBrush = unsafe { Core.D2dCtx.CreateLinearGradientBrush(&GradProps, None, &Collection)? };

		let InnerRect = D2D1_ROUNDED_RECT {
			rect:D2D_RECT_F {
				left:Left + InnerInset,
				top:Top + InnerInset,
				right:Left + W - InnerInset,
				bottom:Top + H - InnerInset,
			},
			radiusX:InnerR,
			radiusY:InnerR,
		};

		unsafe {
			Core.D2dCtx.DrawRoundedRectangle(&InnerRect, &SpecBrush, InnerW, None);
		}
	}

	Ok(())
}

fn PaintSurface(Core:&GpuCore, Surface:&mut MonitorSurface, Key:&MonitorVisualKey) -> windows::core::Result<()> {
	use windows::Win32::Graphics::{
		Direct2D::{
			Common::{D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT},
			D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
			D2D1_BITMAP_OPTIONS_TARGET,
			D2D1_BITMAP_PROPERTIES1,
		},
		Dxgi::{Common::DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_PRESENT, IDXGISurface},
	};

	let BackBuffer:IDXGISurface = unsafe { Surface.SwapChain.GetBuffer(0)? };

	let BitmapProps = D2D1_BITMAP_PROPERTIES1 {
		pixelFormat:D2D1_PIXEL_FORMAT { format:DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode:D2D1_ALPHA_MODE_PREMULTIPLIED },
		dpiX:96.0,
		dpiY:96.0,
		bitmapOptions:D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
		colorContext:std::mem::ManuallyDrop::new(None),
	};

	let TargetBitmap = unsafe { Core.D2dCtx.CreateBitmapFromDxgiSurface(&BackBuffer, Some(&BitmapProps))? };

	unsafe {
		Core.D2dCtx.SetTarget(&TargetBitmap);
		Core.D2dCtx.BeginDraw();
		Core.D2dCtx.Clear(Some(&D2D1_COLOR_F { r:0.0, g:0.0, b:0.0, a:0.0 }));
	}

	let W = Key.WidthPx.max(1) as f32;

	let H = Key.HeightPx.max(1) as f32;

	let Scale = ((Key.ScaleMilli as f32) / 1000.0).max(1.0);

	let ShowScreen = Key.ScreenVisible && Key.RadiusDp > 0;

	let ShowMax = ShowScreen && Key.MaximizeCorners && Key.Work.Maximized && Key.Work.BottomPx > 1;

	let DrawResult:windows::core::Result<()> = (|| {
		if ShowScreen {
			let Ink = if Key.LightInk {
				D2D1_COLOR_F { r:1.0, g:1.0, b:1.0, a:1.0 }
			} else {
				D2D1_COLOR_F { r:0.0, g:0.0, b:0.0, a:1.0 }
			};

			let Brush = unsafe { Core.D2dCtx.CreateSolidColorBrush(&Ink, None)? };

			// Apple continuous corner blend span = 1.528665 * R_px, clamped to half-dimension.
			let Span = ((Key.RadiusDp as f32) * Scale * 1.528_665).min(W * 0.5).min(H * 0.5);

			let Edge = Key.Rim.Edge;

			let DrawTopLeft = Edge != "top" && Edge != "left";

			let DrawTopRight = Edge != "top" && Edge != "right";

			let DrawBottomLeft = Edge != "bottom" && Edge != "left";

			let DrawBottomRight = Edge != "bottom" && Edge != "right";

			if DrawTopLeft {
				DrawSquircleCorner(Core, &Brush, 0.0, 0.0, 1.0, 1.0, Span)?;
			}

			if DrawTopRight {
				DrawSquircleCorner(Core, &Brush, W, 0.0, -1.0, 1.0, Span)?;
			}

			let BottomSpan = if ShowMax { Span.min(Key.Work.BottomPx.max(0) as f32) } else { Span };

			if DrawBottomLeft {
				DrawSquircleCorner(Core, &Brush, 0.0, H, 1.0, -1.0, BottomSpan)?;
			}

			if DrawBottomRight {
				DrawSquircleCorner(Core, &Brush, W, H, -1.0, -1.0, BottomSpan)?;
			}

			if ShowMax {
				let WorkBottomY = (H - (Key.Work.BottomPx.max(0) as f32)).clamp(0.0, H);

				let WorkLeftX = (Key.Work.LeftPx.max(0) as f32).clamp(0.0, W);

				let WorkRightX = (W - (Key.Work.RightPx.max(0) as f32)).clamp(0.0, W);

				DrawSquircleCorner(Core, &Brush, WorkLeftX, WorkBottomY, 1.0, -1.0, Span)?;
				DrawSquircleCorner(Core, &Brush, WorkRightX, WorkBottomY, -1.0, -1.0, Span)?;
			}
		}

		DrawTaskbarRim(Core, &Key.Rim, Key.DarkTheme)?;

		Ok(())
	})();

	let EndResult = unsafe { Core.D2dCtx.EndDraw(None, None) };

	unsafe {
		Core.D2dCtx.SetTarget(None);
	}

	DrawResult?;
	EndResult?;

	unsafe {
		Surface.SwapChain.Present(0, DXGI_PRESENT(0)).ok()?;
		Core.DComp.Commit()?;
	}

	Surface.LastKey = Some(*Key);

	Ok(())
}

fn ShouldShowSurface(Key:&MonitorVisualKey) -> bool {
	(Key.ScreenVisible && Key.RadiusDp > 0) || (Key.Rim.Visible && Key.Rim.RadiusPx > 0)
}

fn SyncSurfaces(CoreOpt:&mut Option<GpuCore>, Surfaces:&mut HashMap<String, MonitorSurface>, OverlayClass:&[u16]) {
	use windows::Win32::{
		Graphics::Dxgi::{
			Common::DXGI_FORMAT_B8G8R8A8_UNORM,
			DXGI_ERROR_DEVICE_REMOVED,
			DXGI_ERROR_DEVICE_RESET,
			DXGI_SWAP_CHAIN_FLAG,
		},
		UI::WindowsAndMessaging::{HWND_TOPMOST, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SetWindowPos, ShowWindow},
	};

	let Snapshot = {
		let Guard = StateLock().lock().unwrap_or_else(|Poisoned| Poisoned.into_inner());
		(
			Guard.HiddenAll,
			Guard.DarkTheme,
			Guard.Monitors.clone(),
			Guard.WorkInsets.clone(),
			Guard.TaskbarRims.clone(),
		)
	};

	let (HiddenAll, DarkTheme, Monitors, WorkInsets, TaskbarRims) = Snapshot;

	if HiddenAll || Stopping.load(Ordering::Acquire) {
		for Surface in Surfaces.values_mut() {
			if Surface.Visible {
				unsafe {
					let _ = ShowWindow(Surface.Hwnd, SW_HIDE);
				}
				Surface.Visible = false;
			}
		}
		return;
	}

	if CoreOpt.is_none() {
		match GpuCore::New() {
			Ok(Core) => *CoreOpt = Some(Core),
			Err(Error) => {
				eprintln!("radius: Direct2D/DirectComposition init failed: {Error}");
				return;
			}
		}
	}

	let Some(Core) = CoreOpt.as_ref() else {
		return;
	};

	// Destroy orphaned surfaces belonging to disconnected monitors immediately.
	Surfaces.retain(|Label, _| Monitors.iter().any(|Item| Item.Label == *Label));

	let mut DeviceLost = false;

	for Config in &Monitors {
		let W = Config.WidthPx.max(1);

		let H = Config.HeightPx.max(1);

		let NeedCreate = !Surfaces.contains_key(&Config.Label);

		if NeedCreate {
			match CreateSurface(Core, OverlayClass, Config) {
				Ok(Surface) => {
					Surfaces.insert(Config.Label.clone(), Surface);
				}
				Err(Error) => {
					eprintln!("radius: native overlay surface {} failed: {Error}", Config.Label);
					if Error.code() == DXGI_ERROR_DEVICE_REMOVED || Error.code() == DXGI_ERROR_DEVICE_RESET {
						DeviceLost = true;
						break;
					}
					continue;
				}
			}
		}

		let Some(Surface) = Surfaces.get_mut(&Config.Label) else {
			continue;
		};

		let Moved = Surface.PhysicalX != Config.PhysicalX || Surface.PhysicalY != Config.PhysicalY;

		let Resized = Surface.WidthPx != W || Surface.HeightPx != H;

		if Moved || Resized {
			unsafe {
				let _ = SetWindowPos(
					Surface.Hwnd,
					Some(HWND_TOPMOST),
					Config.PhysicalX,
					Config.PhysicalY,
					W,
					H,
					SWP_NOACTIVATE,
				);
			}

			Surface.PhysicalX = Config.PhysicalX;
			Surface.PhysicalY = Config.PhysicalY;

			if Resized {
				let ResizeRes = unsafe {
					Surface.SwapChain.ResizeBuffers(
						2,
						W as u32,
						H as u32,
						DXGI_FORMAT_B8G8R8A8_UNORM,
						DXGI_SWAP_CHAIN_FLAG(0),
					)
				};

				if let Err(Error) = ResizeRes {
					eprintln!("radius: swapchain resize failed: {Error}");
					if Error.code() == DXGI_ERROR_DEVICE_REMOVED || Error.code() == DXGI_ERROR_DEVICE_RESET {
						DeviceLost = true;
						break;
					}
					continue;
				}

				Surface.WidthPx = W;
				Surface.HeightPx = H;
				Surface.LastKey = None;
			}
		}

		let Work = WorkInsets.get(&Config.Label).copied().unwrap_or_default();

		let Rim = TaskbarRims.get(&Config.Label).copied().unwrap_or_default();

		let Key = MonitorVisualKey {
			WidthPx:W,
			HeightPx:H,
			ScaleMilli:Config.ScaleMilli,
			RadiusDp:Config.RadiusDp,
			LightInk:Config.LightInk,
			ScreenVisible:Config.ScreenVisible,
			MaximizeCorners:Config.MaximizeCorners,
			Work,
			Rim,
			DarkTheme,
		};

		let WantVisible = ShouldShowSurface(&Key);

		if !WantVisible {
			if Surface.Visible {
				unsafe {
					let _ = ShowWindow(Surface.Hwnd, SW_HIDE);
				}
				Surface.Visible = false;
			}
			continue;
		}

		if Surface.LastKey.as_ref() != Some(&Key)
			&& let Err(Error) = PaintSurface(Core, Surface, &Key)
		{
			eprintln!("radius: Direct2D paint failed: {Error}");
			if Error.code() == DXGI_ERROR_DEVICE_REMOVED || Error.code() == DXGI_ERROR_DEVICE_RESET {
				DeviceLost = true;
				break;
			}
			continue;
		}

		if !Surface.Visible {
			unsafe {
				let _ = ShowWindow(Surface.Hwnd, SW_SHOWNOACTIVATE);
				let _ = SetWindowPos(
					Surface.Hwnd,
					Some(HWND_TOPMOST),
					Surface.PhysicalX,
					Surface.PhysicalY,
					Surface.WidthPx,
					Surface.HeightPx,
					SWP_NOACTIVATE,
				);
			}
			Surface.Visible = true;
		}
	}

	if DeviceLost {
		Surfaces.clear();
		*CoreOpt = None;
		Wake(WM_APP_SYNC);
	}

	// Publish live HWNDs & physical rects for `Watch` and `Taskbar`.
	let mut Published = HwndLock().lock().unwrap_or_else(|Poisoned| Poisoned.into_inner());
	Published.clear();
	for (Label, Surface) in Surfaces.iter() {
		Published.insert(
			Label.clone(),
			(
				Surface.Hwnd.0 as isize,
				Surface.PhysicalX,
				Surface.PhysicalY,
				Surface.PhysicalX + Surface.WidthPx,
				Surface.PhysicalY + Surface.HeightPx,
			),
		);
	}
}

fn RunRenderLoop(ReadyTx:std::sync::mpsc::SyncSender<()>) {
	use windows::{
		Win32::{
			Foundation::{HWND, LPARAM, LRESULT, WPARAM},
			System::{
				Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize},
				LibraryLoader::GetModuleHandleW,
			},
			UI::WindowsAndMessaging::{
				CreateWindowExW,
				DefWindowProcW,
				DestroyWindow,
				DispatchMessageW,
				GetMessageW,
				MSG,
				RegisterClassExW,
				TranslateMessage,
				WNDCLASSEXW,
				WS_EX_NOACTIVATE,
				WS_EX_TOOLWINDOW,
				WS_POPUP,
			},
		},
		core::PCWSTR,
	};

	unsafe extern "system" fn OverlayWndProc(Hwnd:HWND, Msg:u32, WParam:WPARAM, LParam:LPARAM) -> LRESULT {
		unsafe { DefWindowProcW(Hwnd, Msg, WParam, LParam) }
	}

	unsafe {
		let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
	}

	let HInstance = match unsafe { GetModuleHandleW(None) } {
		Ok(Mod) => Mod,
		Err(_) => {
			let _ = ReadyTx.send(());
			return;
		}
	};

	let OverlayClass:Vec<u16> = "RadiusNativeOverlay\0".encode_utf16().collect();

	let ControlClass:Vec<u16> = "RadiusGpuControl\0".encode_utf16().collect();

	unsafe {
		let WcOverlay = WNDCLASSEXW {
			cbSize:std::mem::size_of::<WNDCLASSEXW>() as u32,
			lpfnWndProc:Some(OverlayWndProc),
			hInstance:HInstance.into(),
			lpszClassName:PCWSTR(OverlayClass.as_ptr()),
			..Default::default()
		};
		let _ = RegisterClassExW(&WcOverlay);

		let WcControl = WNDCLASSEXW {
			cbSize:std::mem::size_of::<WNDCLASSEXW>() as u32,
			lpfnWndProc:Some(OverlayWndProc),
			hInstance:HInstance.into(),
			lpszClassName:PCWSTR(ControlClass.as_ptr()),
			..Default::default()
		};
		let _ = RegisterClassExW(&WcControl);
	}

	let CtrlHwnd = match unsafe {
		CreateWindowExW(
			WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
			PCWSTR(ControlClass.as_ptr()),
			PCWSTR::null(),
			WS_POPUP,
			0,
			0,
			0,
			0,
			None,
			None,
			Some(HInstance.into()),
			None,
		)
	} {
		Ok(Hwnd) => Hwnd,
		Err(_) => {
			let _ = ReadyTx.send(());
			return;
		}
	};

	ControlHwnd.store(CtrlHwnd.0 as isize, Ordering::Release);

	let _ = ReadyTx.send(());

	let mut CoreOpt:Option<GpuCore> = GpuCore::New().ok();

	let mut Surfaces:HashMap<String, MonitorSurface> = HashMap::new();

	SyncSurfaces(&mut CoreOpt, &mut Surfaces, &OverlayClass);

	let mut Msg = MSG::default();

	unsafe {
		while GetMessageW(&mut Msg, None, 0, 0).as_bool() {
			match Msg.message {
				WM_APP_SYNC => {
					SyncSurfaces(&mut CoreOpt, &mut Surfaces, &OverlayClass);
				}
				WM_APP_HIDE_ALL => {
					for Surface in Surfaces.values_mut() {
						if Surface.Visible {
							let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
								Surface.Hwnd,
								windows::Win32::UI::WindowsAndMessaging::SW_HIDE,
							);
							Surface.Visible = false;
						}
					}
				}
				WM_APP_SHUTDOWN => {
					break;
				}
				_ => {
					let _ = TranslateMessage(&Msg);
					DispatchMessageW(&Msg);
				}
			}
		}
	}

	Surfaces.clear();
	drop(CoreOpt);

	HwndLock().lock().unwrap_or_else(|Poisoned| Poisoned.into_inner()).clear();

	ControlHwnd.store(0, Ordering::Release);

	unsafe {
		let _ = DestroyWindow(CtrlHwnd);
		CoUninitialize();
	}
}
