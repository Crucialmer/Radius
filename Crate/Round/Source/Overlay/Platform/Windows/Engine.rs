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
// Native Win32 + Direct3D 11 HLSL + Direct2D + DirectComposition implementation
// ============================================================================

const LIQUID_GLASS_HLSL:&str = include_str!("LiquidGlass.hlsl");

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
struct ShaderUniforms {
	Screen:[f32; 4],
	Style:[f32; 4],
	CornerMask:[f32; 4],
	MaxWork:[f32; 4],
	RimRect:[f32; 4],
	RimParams:[f32; 4],
	RimDetail:[f32; 4],
	Extra:[f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SpringScalar {
	Value:f32,
	Velocity:f32,
	Target:f32,
}

impl SpringScalar {
	fn New(Initial:f32) -> Self { Self { Value:Initial, Velocity:0.0, Target:Initial } }

	fn Snap(&mut self, Value:f32) {
		self.Value = Value;
		self.Target = Value;
		self.Velocity = 0.0;
	}

	fn SetTarget(&mut self, Target:f32) { self.Target = Target; }

	fn Step(&mut self, DtSec:f32, Stiffness:f32, Damping:f32, ClampUnit:bool) -> bool {
		let Dt = DtSec.clamp(0.0005, 0.05);

		if (self.Value - self.Target).abs() < 0.002 && self.Velocity.abs() < 0.01 {
			self.Value = self.Target;
			self.Velocity = 0.0;
			return false;
		}

		let Steps = ((Dt * 240.0).ceil() as usize).max(1);

		let H = Dt / (Steps as f32);

		for _ in 0..Steps {
			let Acc = Stiffness * (self.Target - self.Value) - Damping * self.Velocity;

			self.Velocity += Acc * H;
			self.Value += self.Velocity * H;

			if ClampUnit {
				if self.Value <= 0.0 {
					self.Value = 0.0;
					self.Velocity = self.Velocity.max(0.0);
				} else if self.Value >= 1.0 {
					self.Value = 1.0;
					self.Velocity = self.Velocity.min(0.0);
				}
			}
		}

		if (self.Value - self.Target).abs() < 0.002 && self.Velocity.abs() < 0.01 {
			self.Value = self.Target;
			self.Velocity = 0.0;
			false
		} else {
			true
		}
	}
}

struct MonitorSprings {
	Initialized:bool,
	ScreenRadiusPx:SpringScalar,
	ScreenOpacity:SpringScalar,
	MaxOpacity:SpringScalar,
	MaxBottomPx:SpringScalar,
	WorkLeftPx:SpringScalar,
	WorkRightPx:SpringScalar,
	RimRadiusPx:SpringScalar,
	RimOpacity:SpringScalar,
	RimLeftPx:SpringScalar,
	RimTopPx:SpringScalar,
	RimWidthPx:SpringScalar,
	RimHeightPx:SpringScalar,
	InkMix:SpringScalar,
	ThemeMix:SpringScalar,
	Scale:f32,
	WidthPx:f32,
	HeightPx:f32,
	DrawMask:[f32; 4],
	RimIsHorizontal:bool,
	LastRendered:Option<ShaderUniforms>,
}

impl Default for MonitorSprings {
	fn default() -> Self {
		Self {
			Initialized:false,
			ScreenRadiusPx:SpringScalar::New(0.0),
			ScreenOpacity:SpringScalar::New(0.0),
			MaxOpacity:SpringScalar::New(0.0),
			MaxBottomPx:SpringScalar::New(0.0),
			WorkLeftPx:SpringScalar::New(0.0),
			WorkRightPx:SpringScalar::New(0.0),
			RimRadiusPx:SpringScalar::New(0.0),
			RimOpacity:SpringScalar::New(0.0),
			RimLeftPx:SpringScalar::New(0.0),
			RimTopPx:SpringScalar::New(0.0),
			RimWidthPx:SpringScalar::New(0.0),
			RimHeightPx:SpringScalar::New(0.0),
			InkMix:SpringScalar::New(0.0),
			ThemeMix:SpringScalar::New(0.0),
			Scale:1.0,
			WidthPx:1.0,
			HeightPx:1.0,
			DrawMask:[1.0; 4],
			RimIsHorizontal:true,
			LastRendered:None,
		}
	}
}

impl MonitorSprings {
	fn ApplyKey(&mut self, Key:&MonitorVisualKey, SnapAll:bool) {
		let Scale = ((Key.ScaleMilli as f32) / 1000.0).max(1.0);

		self.Scale = Scale;
		self.WidthPx = Key.WidthPx.max(1) as f32;
		self.HeightPx = Key.HeightPx.max(1) as f32;

		let ScreenOn = Key.ScreenVisible && Key.RadiusDp > 0;

		let TargetScreenR = (Key.RadiusDp as f32) * Scale;

		let TargetScreenOp = if ScreenOn { 1.0 } else { 0.0 };

		let TargetMaxOn = ScreenOn && Key.MaximizeCorners && Key.Work.Maximized && Key.Work.BottomPx > 1;

		let TargetMaxOp = if TargetMaxOn { 1.0 } else { 0.0 };

		let TargetMaxBottom = Key.Work.BottomPx.max(0) as f32;

		let TargetWorkLeft = Key.Work.LeftPx.max(0) as f32;

		let TargetWorkRight = Key.Work.RightPx.max(0) as f32;

		let Edge = Key.Rim.Edge;

		self.DrawMask = [
			if Edge != "top" && Edge != "left" { 1.0 } else { 0.0 },
			if Edge != "top" && Edge != "right" { 1.0 } else { 0.0 },
			if Edge != "bottom" && Edge != "left" { 1.0 } else { 0.0 },
			if Edge != "bottom" && Edge != "right" { 1.0 } else { 0.0 },
		];

		let RimOn = Key.Rim.Visible && Key.Rim.RadiusPx > 0 && Key.Rim.WidthPx > 2 && Key.Rim.HeightPx > 2;

		let TargetRimOp = if RimOn { 1.0 } else { 0.0 };

		let TargetRimR = if RimOn { Key.Rim.RadiusPx as f32 } else { 0.0 };

		let TargetInk = if Key.LightInk { 1.0 } else { 0.0 };

		let TargetTheme = if Key.DarkTheme { 1.0 } else { 0.0 };

		if SnapAll || !self.Initialized {
			self.ScreenRadiusPx.Snap(TargetScreenR);
			self.ScreenOpacity.Snap(TargetScreenOp);
			self.MaxOpacity.Snap(TargetMaxOp);
			self.MaxBottomPx.Snap(TargetMaxBottom);
			self.WorkLeftPx.Snap(TargetWorkLeft);
			self.WorkRightPx.Snap(TargetWorkRight);
			self.RimRadiusPx.Snap(TargetRimR);
			self.RimOpacity.Snap(TargetRimOp);
			self.RimLeftPx.Snap(Key.Rim.LeftPx as f32);
			self.RimTopPx.Snap(Key.Rim.TopPx as f32);
			self.RimWidthPx.Snap(Key.Rim.WidthPx.max(0) as f32);
			self.RimHeightPx.Snap(Key.Rim.HeightPx.max(0) as f32);
			self.InkMix.Snap(TargetInk);
			self.ThemeMix.Snap(TargetTheme);
			self.RimIsHorizontal = Key.Rim.WidthPx >= Key.Rim.HeightPx;
			self.Initialized = true;
			return;
		}

		// When re-appearing from hidden, snap spatial frame bounds so corners/rims do
		// not fly in from (0, 0); spring only the opacity and radius smoothly.
		if self.ScreenOpacity.Value <= 0.002 && TargetScreenOp > 0.5 {
			self.ScreenRadiusPx.Snap(TargetScreenR * 0.72);
			self.InkMix.Snap(TargetInk);
		} else {
			self.ScreenRadiusPx.SetTarget(TargetScreenR);
			self.InkMix.SetTarget(TargetInk);
		}
		self.ScreenOpacity.SetTarget(TargetScreenOp);

		if self.MaxOpacity.Value <= 0.002 && TargetMaxOp > 0.5 {
			self.MaxBottomPx.Snap(TargetMaxBottom);
			self.WorkLeftPx.Snap(TargetWorkLeft);
			self.WorkRightPx.Snap(TargetWorkRight);
		} else if TargetMaxOn {
			self.MaxBottomPx.SetTarget(TargetMaxBottom);
			self.WorkLeftPx.SetTarget(TargetWorkLeft);
			self.WorkRightPx.SetTarget(TargetWorkRight);
		}
		self.MaxOpacity.SetTarget(TargetMaxOp);

		if self.RimOpacity.Value <= 0.002 && RimOn {
			self.RimLeftPx.Snap(Key.Rim.LeftPx as f32);
			self.RimTopPx.Snap(Key.Rim.TopPx as f32);
			self.RimWidthPx.Snap(Key.Rim.WidthPx as f32);
			self.RimHeightPx.Snap(Key.Rim.HeightPx as f32);
			self.RimRadiusPx.Snap(TargetRimR * 0.65);
			self.RimIsHorizontal = Key.Rim.WidthPx >= Key.Rim.HeightPx;
		} else if RimOn {
			self.RimLeftPx.SetTarget(Key.Rim.LeftPx as f32);
			self.RimTopPx.SetTarget(Key.Rim.TopPx as f32);
			self.RimWidthPx.SetTarget(Key.Rim.WidthPx as f32);
			self.RimHeightPx.SetTarget(Key.Rim.HeightPx as f32);
			self.RimIsHorizontal = Key.Rim.WidthPx >= Key.Rim.HeightPx;
		}

		if ScreenOn || TargetScreenR > 0.0 {
			self.ScreenRadiusPx.SetTarget(TargetScreenR);
		}
		self.RimRadiusPx.SetTarget(TargetRimR);
		self.RimOpacity.SetTarget(TargetRimOp);
		self.ThemeMix.SetTarget(TargetTheme);
	}

	fn Step(&mut self, DtSec:f32) -> bool {
		// Radius/spatial springs: crisp underdamped Apple spring (~260ms settle)
		const MOTION_K:f32 = 420.0;
		const MOTION_C:f32 = 34.0;
		// Opacity & color mix: critically damped (zero overshoot past [0, 1])
		const FADE_K:f32 = 480.0;
		const FADE_C:f32 = 43.8;

		let mut Moving = false;
		Moving |= self.ScreenRadiusPx.Step(DtSec, MOTION_K, MOTION_C, false);
		Moving |= self.MaxBottomPx.Step(DtSec, MOTION_K, MOTION_C, false);
		Moving |= self.WorkLeftPx.Step(DtSec, MOTION_K, MOTION_C, false);
		Moving |= self.WorkRightPx.Step(DtSec, MOTION_K, MOTION_C, false);
		Moving |= self.RimRadiusPx.Step(DtSec, MOTION_K, MOTION_C, false);
		Moving |= self.RimLeftPx.Step(DtSec, MOTION_K, MOTION_C, false);
		Moving |= self.RimTopPx.Step(DtSec, MOTION_K, MOTION_C, false);
		Moving |= self.RimWidthPx.Step(DtSec, MOTION_K, MOTION_C, false);
		Moving |= self.RimHeightPx.Step(DtSec, MOTION_K, MOTION_C, false);

		Moving |= self.ScreenOpacity.Step(DtSec, FADE_K, FADE_C, true);
		Moving |= self.MaxOpacity.Step(DtSec, FADE_K, FADE_C, true);
		Moving |= self.RimOpacity.Step(DtSec, FADE_K, FADE_C, true);
		Moving |= self.InkMix.Step(DtSec, FADE_K, FADE_C, true);
		Moving |= self.ThemeMix.Step(DtSec, FADE_K, FADE_C, true);
		Moving
	}

	fn IsVisibleNow(&self) -> bool { self.ScreenOpacity.Value > 0.002 || self.RimOpacity.Value > 0.002 }

	fn BuildUniforms(&self) -> ShaderUniforms {
		let W = self.WidthPx.max(1.0);

		let H = self.HeightPx.max(1.0);

		let ScreenSpan = (self.ScreenRadiusPx.Value * 1.528_665).min(W * 0.5).min(H * 0.5).max(0.0);

		let BottomSpan =
			if self.MaxOpacity.Value > 0.002 { ScreenSpan.min(self.MaxBottomPx.Value.max(0.0)) } else { ScreenSpan };

		let WorkBottomY = (H - self.MaxBottomPx.Value.max(0.0)).clamp(0.0, H);

		let WorkLeftX = self.WorkLeftPx.Value.clamp(0.0, W);

		let WorkRightX = (W - self.WorkRightPx.Value.max(0.0)).clamp(0.0, W);

		let RimW = self.RimWidthPx.Value.max(0.0);

		let RimH = self.RimHeightPx.Value.max(0.0);

		let RimR = self.RimRadiusPx.Value.max(0.0);

		let RimSpan = (RimR * 1.528_665).min(RimW * 0.5).min(RimH * 0.5).max(0.0);

		let Layout = ComputeRimLayout(RimSpan, self.Scale);

		ShaderUniforms {
			Screen:[W, H, self.Scale, ScreenSpan],
			Style:[
				self.ScreenOpacity.Value.clamp(0.0, 1.0),
				self.InkMix.Value.clamp(0.0, 1.0),
				self.ThemeMix.Value.clamp(0.0, 1.0),
				BottomSpan,
			],
			CornerMask:self.DrawMask,
			MaxWork:[self.MaxOpacity.Value.clamp(0.0, 1.0), WorkLeftX, WorkRightX, WorkBottomY],
			RimRect:[self.RimLeftPx.Value, self.RimTopPx.Value, RimW, RimH],
			RimParams:[RimSpan, self.RimOpacity.Value.clamp(0.0, 1.0), Layout.OuterW, 0.0],
			RimDetail:[0.0, 0.0, if self.RimIsHorizontal { 1.0 } else { 0.0 }, 0.0],
			Extra:[0.0; 4],
		}
	}
}

struct ShaderPipeline {
	VertexShader:windows::Win32::Graphics::Direct3D11::ID3D11VertexShader,
	PixelShader:windows::Win32::Graphics::Direct3D11::ID3D11PixelShader,
	ConstantBuffer:windows::Win32::Graphics::Direct3D11::ID3D11Buffer,
	RasterState:windows::Win32::Graphics::Direct3D11::ID3D11RasterizerState,
}

impl ShaderPipeline {
	fn New(Device:&windows::Win32::Graphics::Direct3D11::ID3D11Device) -> windows::core::Result<Self> {
		use std::mem::size_of;

		use windows::{
			Win32::Graphics::{
				Direct3D::{Fxc::D3DCompile, ID3DBlob, ID3DInclude},
				Direct3D11::{
					D3D11_BIND_CONSTANT_BUFFER,
					D3D11_BUFFER_DESC,
					D3D11_CPU_ACCESS_WRITE,
					D3D11_CULL_NONE,
					D3D11_FILL_SOLID,
					D3D11_RASTERIZER_DESC,
					D3D11_USAGE_DYNAMIC,
				},
			},
			core::PCSTR,
		};

		let Compile = |Entry:&core::ffi::CStr, Target:&core::ffi::CStr| -> windows::core::Result<ID3DBlob> {
			let mut Code:Option<ID3DBlob> = None;

			let mut Errors:Option<ID3DBlob> = None;

			let Source = LIQUID_GLASS_HLSL.as_bytes();

			let NoInclude:Option<&ID3DInclude> = None;

			// D3DCOMPILE_ENABLE_STRICTNESS (1 << 11) | D3DCOMPILE_OPTIMIZATION_LEVEL3 (1 << 15)
			let Flags:u32 = (1 << 11) | (1 << 15);

			let Res = unsafe {
				D3DCompile(
					Source.as_ptr() as *const core::ffi::c_void,
					Source.len(),
					PCSTR(c"LiquidGlass.hlsl".as_ptr() as *const u8),
					None,
					NoInclude,
					PCSTR(Entry.as_ptr() as *const u8),
					PCSTR(Target.as_ptr() as *const u8),
					Flags,
					0,
					&mut Code,
					Some(&mut Errors),
				)
			};

			if let Err(Error) = Res {
				if let Some(Blob) = Errors {
					let Msg = unsafe {
						let Ptr = Blob.GetBufferPointer() as *const u8;
						let Len = Blob.GetBufferSize();
						String::from_utf8_lossy(std::slice::from_raw_parts(Ptr, Len)).into_owned()
					};
					eprintln!("radius: HLSL compile error: {Msg}");
				}
				return Err(Error);
			}

			Code.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))
		};

		let VsBlob = Compile(c"VSMain", c"vs_4_0")?;

		let PsBlob = Compile(c"PSMain", c"ps_4_0")?;

		let VsBytes =
			unsafe { std::slice::from_raw_parts(VsBlob.GetBufferPointer() as *const u8, VsBlob.GetBufferSize()) };

		let PsBytes =
			unsafe { std::slice::from_raw_parts(PsBlob.GetBufferPointer() as *const u8, PsBlob.GetBufferSize()) };

		let mut VertexShader = None;

		let mut PixelShader = None;

		let BufDesc = D3D11_BUFFER_DESC {
			ByteWidth:size_of::<ShaderUniforms>() as u32,
			Usage:D3D11_USAGE_DYNAMIC,
			BindFlags:D3D11_BIND_CONSTANT_BUFFER.0 as u32,
			CPUAccessFlags:D3D11_CPU_ACCESS_WRITE.0 as u32,
			MiscFlags:0,
			StructureByteStride:0,
		};

		let mut ConstantBuffer = None;

		let RastDesc = D3D11_RASTERIZER_DESC {
			FillMode:D3D11_FILL_SOLID,
			CullMode:D3D11_CULL_NONE,
			FrontCounterClockwise:false.into(),
			DepthBias:0,
			DepthBiasClamp:0.0,
			SlopeScaledDepthBias:0.0,
			DepthClipEnable:false.into(),
			ScissorEnable:false.into(),
			MultisampleEnable:false.into(),
			AntialiasedLineEnable:false.into(),
		};

		let mut RasterState = None;

		unsafe {
			Device.CreateVertexShader(VsBytes, None, Some(&mut VertexShader))?;
			Device.CreatePixelShader(PsBytes, None, Some(&mut PixelShader))?;
			Device.CreateBuffer(&BufDesc, None, Some(&mut ConstantBuffer))?;
			Device.CreateRasterizerState(&RastDesc, Some(&mut RasterState))?;
		}

		Ok(Self {
			VertexShader:VertexShader.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?,
			PixelShader:PixelShader.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?,
			ConstantBuffer:ConstantBuffer
				.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?,
			RasterState:RasterState.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?,
		})
	}

	fn RenderToRtv(
		&self,
		Ctx:&windows::Win32::Graphics::Direct3D11::ID3D11DeviceContext,
		Rtv:&windows::Win32::Graphics::Direct3D11::ID3D11RenderTargetView,
		WidthPx:f32,
		HeightPx:f32,
		Uniforms:&ShaderUniforms,
	) -> windows::core::Result<()> {
		use windows::Win32::Graphics::{
			Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
			Direct3D11::{D3D11_MAP_WRITE_DISCARD, D3D11_MAPPED_SUBRESOURCE, D3D11_VIEWPORT},
		};

		unsafe {
			let mut Mapped = D3D11_MAPPED_SUBRESOURCE::default();

			Ctx.Map(&self.ConstantBuffer, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut Mapped))?;
			std::ptr::copy_nonoverlapping(Uniforms, Mapped.pData as *mut ShaderUniforms, 1);
			Ctx.Unmap(&self.ConstantBuffer, 0);

			let ClearColor = [0.0_f32; 4];

			Ctx.ClearRenderTargetView(Rtv, &ClearColor);

			let Viewport = D3D11_VIEWPORT {
				TopLeftX:0.0,
				TopLeftY:0.0,
				Width:WidthPx.max(1.0),
				Height:HeightPx.max(1.0),
				MinDepth:0.0,
				MaxDepth:1.0,
			};

			Ctx.RSSetViewports(Some(&[Viewport]));
			Ctx.RSSetState(&self.RasterState);
			Ctx.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
			Ctx.VSSetShader(&self.VertexShader, None);
			Ctx.PSSetShader(&self.PixelShader, None);

			let Cbs = [Some(self.ConstantBuffer.clone())];

			Ctx.PSSetConstantBuffers(0, Some(&Cbs));

			let Rtvs = [Some(Rtv.clone())];

			Ctx.OMSetRenderTargets(Some(&Rtvs), None);
			Ctx.OMSetBlendState(None, None, 0xFFFF_FFFF);
			Ctx.Draw(3, 0);

			let Empty:[Option<windows::Win32::Graphics::Direct3D11::ID3D11RenderTargetView>; 1] = [None];

			Ctx.OMSetRenderTargets(Some(&Empty), None);
		}

		Ok(())
	}
}

struct GpuCore {
	D3dDevice:windows::Win32::Graphics::Direct3D11::ID3D11Device,
	D3dCtx:windows::Win32::Graphics::Direct3D11::ID3D11DeviceContext,
	DxgiFactory:windows::Win32::Graphics::Dxgi::IDXGIFactory2,
	D2dFactory:windows::Win32::Graphics::Direct2D::ID2D1Factory1,
	_D2dDevice:windows::Win32::Graphics::Direct2D::ID2D1Device,
	D2dCtx:windows::Win32::Graphics::Direct2D::ID2D1DeviceContext,
	DComp:windows::Win32::Graphics::DirectComposition::IDCompositionDevice,
	Shader:Option<ShaderPipeline>,
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
				Direct3D11::{
					D3D11_CREATE_DEVICE_BGRA_SUPPORT,
					D3D11_SDK_VERSION,
					D3D11CreateDevice,
					ID3D11Device,
					ID3D11DeviceContext,
				},
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

		let TryCreate = |Driver:D3D_DRIVER_TYPE| -> windows::core::Result<(ID3D11Device, ID3D11DeviceContext)> {
			let mut Device:Option<ID3D11Device> = None;

			let mut Ctx:Option<ID3D11DeviceContext> = None;

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
					Some(&mut Ctx),
				)?;
			}

			Ok((
				Device.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?,
				Ctx.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?,
			))
		};

		let (D3dDevice, D3dCtx) = TryCreate(D3D_DRIVER_TYPE_HARDWARE).or_else(|_| TryCreate(D3D_DRIVER_TYPE_WARP))?;

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

		let Shader = ShaderPipeline::New(&D3dDevice).ok();

		Ok(Self {
			D3dDevice,
			D3dCtx,
			DxgiFactory,
			D2dFactory,
			_D2dDevice:D2dDevice,
			D2dCtx,
			DComp,
			Shader,
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
	Springs:MonitorSprings,
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

	let SwapChain = unsafe { Core.DxgiFactory.CreateSwapChainForComposition(&Core.D3dDevice, &Desc, None)? };

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
		Springs:MonitorSprings::default(),
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

struct RimLayout {
	OuterW:f32,
	OuterInset:f32,
	OuterR:f32,
}

/// Compute uniform matte outer hairline geometry for taskbar edge antialiasing.
fn ComputeRimLayout(Radius:f32, Scale:f32) -> RimLayout {
	let OuterW = (1.0 * Scale).clamp(1.0, 2.0);

	let OuterInset = OuterW * 0.5;

	let OuterR = (Radius - OuterInset).max(0.0);

	RimLayout { OuterW, OuterInset, OuterR }
}

/// Draw a single matte hardware-anti-aliased border aligned 1:1 in physical
/// pixels over the clipped Windows taskbar boundary (fallback when HLSL is unavailable).
fn DrawTaskbarRim(Core:&GpuCore, Rim:&TaskbarRimState, DarkTheme:bool) -> windows::core::Result<()> {
	use windows::Win32::Graphics::Direct2D::{
		Common::{D2D_RECT_F, D2D1_COLOR_F},
		D2D1_ROUNDED_RECT,
	};

	if !Rim.Visible || Rim.RadiusPx <= 0 || Rim.WidthPx <= 2 || Rim.HeightPx <= 2 {
		return Ok(());
	}

	let Left = Rim.LeftPx as f32;

	let Top = Rim.TopPx as f32;

	let W = Rim.WidthPx as f32;

	let H = Rim.HeightPx as f32;

	let Scale = ((Rim.ScaleMilli as f32) / 1000.0).max(1.0);

	let Radius = (Rim.RadiusPx as f32).min(W * 0.5).min(H * 0.5);

	let Layout = ComputeRimLayout(Radius, Scale);

	let OuterColor = if DarkTheme {
		D2D1_COLOR_F { r:0.0, g:0.0, b:0.0, a:0.28 }
	} else {
		D2D1_COLOR_F { r:15.0 / 255.0, g:18.0 / 255.0, b:26.0 / 255.0, a:0.14 }
	};

	let OuterBrush = unsafe { Core.D2dCtx.CreateSolidColorBrush(&OuterColor, None)? };

	let OuterRect = D2D1_ROUNDED_RECT {
		rect:D2D_RECT_F {
			left:Left + Layout.OuterInset,
			top:Top + Layout.OuterInset,
			right:Left + W - Layout.OuterInset,
			bottom:Top + H - Layout.OuterInset,
		},
		radiusX:Layout.OuterR,
		radiusY:Layout.OuterR,
	};

	unsafe {
		Core.D2dCtx.DrawRoundedRectangle(&OuterRect, &OuterBrush, Layout.OuterW, None);
	}

	Ok(())
}

fn PaintSurfaceD3d11(
	Core:&GpuCore,
	Pipeline:&ShaderPipeline,
	Surface:&mut MonitorSurface,
	Uniforms:&ShaderUniforms,
) -> windows::core::Result<()> {
	use windows::Win32::Graphics::{Direct3D11::ID3D11Texture2D, Dxgi::DXGI_PRESENT};

	let BackBuffer:ID3D11Texture2D = unsafe { Surface.SwapChain.GetBuffer(0)? };

	let mut Rtv = None;

	unsafe {
		Core.D3dDevice.CreateRenderTargetView(&BackBuffer, None, Some(&mut Rtv))?;
	}

	let Rtv = Rtv.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?;

	Pipeline.RenderToRtv(
		&Core.D3dCtx,
		&Rtv,
		Surface.WidthPx.max(1) as f32,
		Surface.HeightPx.max(1) as f32,
		Uniforms,
	)?;

	drop(Rtv);
	drop(BackBuffer);

	unsafe {
		Surface.SwapChain.Present(0, DXGI_PRESENT(0)).ok()?;
		Core.DComp.Commit()?;
	}

	Surface.Springs.LastRendered = Some(*Uniforms);

	Ok(())
}

fn RenderSurface(Core:&GpuCore, Surface:&mut MonitorSurface, Key:&MonitorVisualKey) -> windows::core::Result<()> {
	let Uniforms = Surface.Springs.BuildUniforms();

	if Surface.Springs.LastRendered.as_ref() == Some(&Uniforms) {
		return Ok(());
	}

	if let Some(Pipeline) = Core.Shader.as_ref() {
		return PaintSurfaceD3d11(Core, Pipeline, Surface, &Uniforms);
	}

	PaintSurfaceD2dFallback(Core, Surface, Key)?;
	Surface.Springs.LastRendered = Some(Uniforms);
	Ok(())
}

fn PaintSurfaceD2dFallback(
	Core:&GpuCore,
	Surface:&mut MonitorSurface,
	Key:&MonitorVisualKey,
) -> windows::core::Result<()> {
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

	let Resized = Surface.LastKey.is_none();

	unsafe {
		Surface.SwapChain.Present(0, DXGI_PRESENT(0)).ok()?;
		Core.DComp.Commit()?;
	}

	let _ = Resized;

	Surface.LastKey = Some(*Key);

	Ok(())
}

fn ShouldShowSurface(Key:&MonitorVisualKey) -> bool {
	(Key.ScreenVisible && Key.RadiusDp > 0) || (Key.Rim.Visible && Key.Rim.RadiusPx > 0)
}

fn ShowOrHideSurface(Surface:&mut MonitorSurface, Show:bool) {
	use windows::Win32::UI::WindowsAndMessaging::{
		HWND_TOPMOST,
		SW_HIDE,
		SW_SHOWNOACTIVATE,
		SWP_NOACTIVATE,
		SetWindowPos,
		ShowWindow,
	};

	if Show && !Surface.Visible {
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
	} else if !Show && Surface.Visible {
		unsafe {
			let _ = ShowWindow(Surface.Hwnd, SW_HIDE);
		}
		Surface.Visible = false;
	}
}

fn SyncSurfaces(
	CoreOpt:&mut Option<GpuCore>,
	Surfaces:&mut HashMap<String, MonitorSurface>,
	OverlayClass:&[u16],
) -> bool {
	use windows::Win32::{
		Graphics::Dxgi::{
			Common::DXGI_FORMAT_B8G8R8A8_UNORM,
			DXGI_ERROR_DEVICE_REMOVED,
			DXGI_ERROR_DEVICE_RESET,
			DXGI_SWAP_CHAIN_FLAG,
		},
		UI::WindowsAndMessaging::{HWND_TOPMOST, SW_HIDE, SWP_NOACTIVATE, SetWindowPos, ShowWindow},
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
			Surface.Springs.Initialized = false;
		}
		return false;
	}

	if CoreOpt.is_none() {
		match GpuCore::New() {
			Ok(Core) => *CoreOpt = Some(Core),
			Err(Error) => {
				eprintln!("radius: Direct3D/DirectComposition init failed: {Error}");
				return false;
			}
		}
	}

	let Some(Core) = CoreOpt.as_ref() else {
		return false;
	};

	// Destroy orphaned surfaces belonging to disconnected monitors immediately.
	Surfaces.retain(|Label, _| Monitors.iter().any(|Item| Item.Label == *Label));

	let mut DeviceLost = false;

	let mut AnyAnimating = false;

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
				Surface.Springs.LastRendered = None;
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

		let KeyChanged = Surface.LastKey.as_ref() != Some(&Key);

		if KeyChanged || !Surface.Springs.Initialized {
			Surface.Springs.ApplyKey(&Key, Resized);
			Surface.LastKey = Some(Key);
		}

		let Moving = Surface.Springs.Step(1.0 / 240.0);

		AnyAnimating |= Moving;

		let WantShow = ShouldShowSurface(&Key) || Surface.Springs.IsVisibleNow();

		if !WantShow {
			ShowOrHideSurface(Surface, false);
			continue;
		}

		if let Err(Error) = RenderSurface(Core, Surface, &Key) {
			eprintln!("radius: GPU paint failed: {Error}");
			if Error.code() == DXGI_ERROR_DEVICE_REMOVED || Error.code() == DXGI_ERROR_DEVICE_RESET {
				DeviceLost = true;
				break;
			}
			continue;
		}

		ShowOrHideSurface(Surface, true);
	}

	if DeviceLost {
		Surfaces.clear();
		*CoreOpt = None;
		Wake(WM_APP_SYNC);
		return false;
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

	AnyAnimating
}

fn TickAnimations(CoreOpt:&mut Option<GpuCore>, Surfaces:&mut HashMap<String, MonitorSurface>, DtSec:f32) -> bool {
	use windows::Win32::Graphics::Dxgi::{DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET};

	if Stopping.load(Ordering::Acquire) {
		return false;
	}

	let Some(Core) = CoreOpt.as_ref() else {
		return false;
	};

	let mut AnyAnimating = false;

	let mut DeviceLost = false;

	for Surface in Surfaces.values_mut() {
		let Some(Key) = Surface.LastKey else {
			continue;
		};

		let Moving = Surface.Springs.Step(DtSec);

		AnyAnimating |= Moving;

		let WantShow = ShouldShowSurface(&Key) || Surface.Springs.IsVisibleNow();

		if !WantShow {
			ShowOrHideSurface(Surface, false);
			continue;
		}

		if let Err(Error) = RenderSurface(Core, Surface, &Key) {
			eprintln!("radius: GPU frame step failed: {Error}");
			if Error.code() == DXGI_ERROR_DEVICE_REMOVED || Error.code() == DXGI_ERROR_DEVICE_RESET {
				DeviceLost = true;
				break;
			}
			continue;
		}

		ShowOrHideSurface(Surface, true);
	}

	if DeviceLost {
		Surfaces.clear();
		*CoreOpt = None;
		Wake(WM_APP_SYNC);
		return false;
	}

	AnyAnimating
}

fn RunRenderLoop(ReadyTx:std::sync::mpsc::SyncSender<()>) {
	use std::time::Instant;

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
				PM_REMOVE,
				PeekMessageW,
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

	let TimerHandle = unsafe {
		let HighRes = windows_sys::Win32::System::Threading::CreateWaitableTimerExW(
			std::ptr::null(),
			std::ptr::null(),
			windows_sys::Win32::System::Threading::CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
			0x001F_0003,
		);

		if !HighRes.is_null() {
			HighRes
		} else {
			windows_sys::Win32::System::Threading::CreateWaitableTimerExW(
				std::ptr::null(),
				std::ptr::null(),
				0,
				0x001F_0003,
			)
		}
	};

	let mut CoreOpt:Option<GpuCore> = GpuCore::New().ok();

	let mut Surfaces:HashMap<String, MonitorSurface> = HashMap::new();

	let mut Animating = SyncSurfaces(&mut CoreOpt, &mut Surfaces, &OverlayClass);

	let mut LastTick = Instant::now();

	let mut Msg = MSG::default();

	let mut ShutdownRequested = false;

	let HandleMsg = |Msg:&MSG,
	                 CoreOpt:&mut Option<GpuCore>,
	                 Surfaces:&mut HashMap<String, MonitorSurface>,
	                 Animating:&mut bool,
	                 LastTick:&mut Instant,
	                 ShutdownRequested:&mut bool| {
		match Msg.message {
			WM_APP_SYNC => {
				let WasAnimating = *Animating;

				*Animating = SyncSurfaces(CoreOpt, Surfaces, &OverlayClass);

				if !WasAnimating && *Animating {
					*LastTick = Instant::now();
				}
			}
			WM_APP_HIDE_ALL => {
				for Surface in Surfaces.values_mut() {
					ShowOrHideSurface(Surface, false);
					Surface.Springs.Initialized = false;
				}
				*Animating = false;
			}
			WM_APP_SHUTDOWN => {
				*ShutdownRequested = true;
			}
			_ => unsafe {
				let _ = TranslateMessage(Msg);
				DispatchMessageW(Msg);
			},
		}
	};

	unsafe {
		while !ShutdownRequested {
			if !Animating {
				if !GetMessageW(&mut Msg, None, 0, 0).as_bool() {
					break;
				}

				HandleMsg(
					&Msg,
					&mut CoreOpt,
					&mut Surfaces,
					&mut Animating,
					&mut LastTick,
					&mut ShutdownRequested,
				);

				while !ShutdownRequested && PeekMessageW(&mut Msg, None, 0, 0, PM_REMOVE).as_bool() {
					HandleMsg(
						&Msg,
						&mut CoreOpt,
						&mut Surfaces,
						&mut Animating,
						&mut LastTick,
						&mut ShutdownRequested,
					);
				}

				continue;
			}

			if !TimerHandle.is_null() {
				// 4.1667 ms one-shot pacing = 240 Hz GPU spring refresh (0% CPU when idle).
				let DueTime:i64 = -41_667;

				let Armed = windows_sys::Win32::System::Threading::SetWaitableTimer(
					TimerHandle,
					&DueTime,
					0,
					None,
					std::ptr::null(),
					0,
				) != 0;

				if Armed {
					let Handles = [TimerHandle];

					let _ = windows_sys::Win32::UI::WindowsAndMessaging::MsgWaitForMultipleObjects(
						1,
						Handles.as_ptr(),
						0,
						16,
						windows_sys::Win32::UI::WindowsAndMessaging::QS_ALLINPUT,
					);
				} else {
					std::thread::sleep(std::time::Duration::from_millis(4));
				}
			} else {
				std::thread::sleep(std::time::Duration::from_millis(4));
			}

			let mut HadSync = false;

			while !ShutdownRequested && PeekMessageW(&mut Msg, None, 0, 0, PM_REMOVE).as_bool() {
				if Msg.message == windows_sys::Win32::UI::WindowsAndMessaging::WM_QUIT {
					ShutdownRequested = true;
					break;
				}

				if Msg.message == WM_APP_SYNC {
					HadSync = true;
					continue;
				}

				HandleMsg(
					&Msg,
					&mut CoreOpt,
					&mut Surfaces,
					&mut Animating,
					&mut LastTick,
					&mut ShutdownRequested,
				);
			}

			if ShutdownRequested {
				break;
			}

			let Now = Instant::now();

			let DtSec = (Now - LastTick).as_secs_f32().clamp(0.0005, 0.05);

			LastTick = Now;

			if HadSync {
				Animating = SyncSurfaces(&mut CoreOpt, &mut Surfaces, &OverlayClass);
			}

			Animating |= TickAnimations(&mut CoreOpt, &mut Surfaces, DtSec);
		}
	}

	if !TimerHandle.is_null() {
		unsafe {
			let _ = windows_sys::Win32::Foundation::CloseHandle(TimerHandle);
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

#[cfg(test)]
mod tests {
	use super::{
		ComputeRimLayout,
		GpuCore,
		MonitorSprings,
		MonitorVisualKey,
		ShaderPipeline,
		SpringScalar,
		TaskbarRimState,
		WorkInsetState,
	};

	#[test]
	fn RimLayoutScalesMatteHairlineCleanly() {
		for Scale in [1.0_f32, 1.365, 1.5, 2.0] {
			let Layout = ComputeRimLayout(15.0, Scale);

			assert!((Layout.OuterW - (1.0 * Scale).clamp(1.0, 2.0)).abs() < 1e-4);
			assert!((Layout.OuterInset - Layout.OuterW * 0.5).abs() < 1e-4);
			assert!(Layout.OuterR > 0.0 && Layout.OuterR < 15.0);
		}
	}

	#[test]
	fn SpringScalarSettlesSmoothlyAt240Hz() {
		// Critically damped fade spring: zero overshoot past [0, 1].
		let mut Fade = SpringScalar::New(0.0);
		Fade.SetTarget(1.0);
		let mut Frames = 0;
		while Fade.Step(1.0 / 240.0, 480.0, 43.8, true) && Frames < 240 {
			assert!(Fade.Value >= 0.0 && Fade.Value <= 1.0);
			Frames += 1;
		}
		assert!(Frames < 120, "fade took too long: {Frames} frames");
		assert_eq!(Fade.Value, 1.0);
		assert_eq!(Fade.Velocity, 0.0);

		// Coarse dt = 0.05s sub-stepping also converges cleanly.
		let mut Radius = SpringScalar::New(0.0);
		Radius.SetTarget(20.0);
		let mut Steps = 0;
		while Radius.Step(0.05, 420.0, 34.0, false) && Steps < 40 {
			Steps += 1;
		}
		assert!(Steps < 20, "coarse spring took too many steps: {Steps}");
		assert_eq!(Radius.Value, 20.0);
	}

	#[test]
	fn HlslLiquidGlassShaderCompilesAndRendersUniformHorizontalRim() {
		use windows::Win32::Graphics::{
			Direct3D11::{
				D3D11_BIND_RENDER_TARGET,
				D3D11_CPU_ACCESS_READ,
				D3D11_MAP_READ,
				D3D11_MAPPED_SUBRESOURCE,
				D3D11_TEXTURE2D_DESC,
				D3D11_USAGE_DEFAULT,
				D3D11_USAGE_STAGING,
				ID3D11Texture2D,
			},
			Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
		};

		let Core = GpuCore::New().expect("D3D11 device creation");
		let Pipeline = ShaderPipeline::New(&Core.D3dDevice).expect("LiquidGlass.hlsl compile");

		let W = 128_u32;
		let H = 64_u32;

		let TexDesc = D3D11_TEXTURE2D_DESC {
			Width:W,
			Height:H,
			MipLevels:1,
			ArraySize:1,
			Format:DXGI_FORMAT_B8G8R8A8_UNORM,
			SampleDesc:DXGI_SAMPLE_DESC { Count:1, Quality:0 },
			Usage:D3D11_USAGE_DEFAULT,
			BindFlags:D3D11_BIND_RENDER_TARGET.0 as u32,
			CPUAccessFlags:0,
			MiscFlags:0,
		};

		let mut Tex:Option<ID3D11Texture2D> = None;
		unsafe {
			Core.D3dDevice.CreateTexture2D(&TexDesc, None, Some(&mut Tex)).unwrap();
		}
		let Tex = Tex.unwrap();

		let mut Rtv = None;
		unsafe {
			Core.D3dDevice.CreateRenderTargetView(&Tex, None, Some(&mut Rtv)).unwrap();
		}
		let Rtv = Rtv.unwrap();

		let Key = MonitorVisualKey {
			WidthPx:W as i32,
			HeightPx:H as i32,
			ScaleMilli:1500,
			RadiusDp:0,
			LightInk:false,
			ScreenVisible:false,
			MaximizeCorners:false,
			Work:WorkInsetState::default(),
			Rim:TaskbarRimState {
				Visible:true,
				Edge:"bottom",
				LeftPx:0,
				TopPx:0,
				WidthPx:W as i32,
				HeightPx:H as i32,
				RadiusPx:16,
				ScaleMilli:1500,
			},
			DarkTheme:false,
		};

		let mut Springs = MonitorSprings::default();
		Springs.ApplyKey(&Key, true);
		let Uniforms = Springs.BuildUniforms();

		Pipeline.RenderToRtv(&Core.D3dCtx, &Rtv, W as f32, H as f32, &Uniforms).unwrap();

		let StageDesc = D3D11_TEXTURE2D_DESC {
			Usage:D3D11_USAGE_STAGING,
			BindFlags:0,
			CPUAccessFlags:D3D11_CPU_ACCESS_READ.0 as u32,
			..TexDesc
		};

		let mut Stage:Option<ID3D11Texture2D> = None;
		unsafe {
			Core.D3dDevice.CreateTexture2D(&StageDesc, None, Some(&mut Stage)).unwrap();
		}
		let Stage = Stage.unwrap();

		let Pixel = |X:usize, Y:usize, Ptr:*const u8, Pitch:usize| -> [u8; 4] {
			unsafe {
				let Row = Ptr.add(Y * Pitch);
				let P = Row.add(X * 4);
				[*P.add(0), *P.add(1), *P.add(2), *P.add(3)]
			}
		};

		unsafe {
			Core.D3dCtx.CopyResource(&Stage, &Tex);
			let mut Mapped = D3D11_MAPPED_SUBRESOURCE::default();
			Core.D3dCtx.Map(&Stage, 0, D3D11_MAP_READ, 0, Some(&mut Mapped)).unwrap();

			let Ptr = Mapped.pData as *const u8;
			let Pitch = Mapped.RowPitch as usize;

			// Outside squircle top-left (0, 0), inner strip (64, 5), and interior center (64, 32)
			// must be completely transparent (zero inward glow).
			assert_eq!(
				Pixel(0, 0, Ptr, Pitch)[3],
				0,
				"pixel outside squircle corner must be transparent"
			);
			assert_eq!(Pixel(64, 5, Ptr, Pitch)[3], 0, "inward strip must have zero glow");
			assert_eq!(Pixel(64, 32, Ptr, Pitch)[3], 0, "interior pixel must be transparent");

			// Top (x=32, 64, 96, y=0), bottom (64, 63), left (0, 32), right (127, 32) must all share
			// identical uniform matte color/alpha with zero directional glow or RGB dispersion.
			let Samples = [
				Pixel(32, 0, Ptr, Pitch),
				Pixel(64, 0, Ptr, Pitch),
				Pixel(96, 0, Ptr, Pitch),
				Pixel(64, 63, Ptr, Pitch),
				Pixel(0, 32, Ptr, Pitch),
				Pixel(127, 32, Ptr, Pitch),
			];

			for P in Samples {
				assert!(P[3] > 10, "edge hairline must be visible: {P:?}");
				assert!(
					P[0] <= P[3] && P[1] <= P[3] && P[2] <= P[3],
					"output must be valid premultiplied BGRA: {P:?}"
				);
				let MaxDiff = (P[0] as i32 - Samples[0][0] as i32)
					.abs()
					.max((P[1] as i32 - Samples[0][1] as i32).abs())
					.max((P[2] as i32 - Samples[0][2] as i32).abs())
					.max((P[3] as i32 - Samples[0][3] as i32).abs());
				assert!(
					MaxDiff <= 2,
					"edge color/alpha must stay uniform across all 4 sides: {Samples:?}"
				);
			}

			Core.D3dCtx.Unmap(&Stage, 0);
		}
	}
}
