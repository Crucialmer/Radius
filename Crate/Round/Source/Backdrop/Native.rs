//! Ultra-lightweight native Win32 transparent backdrop scrims (zero WebView2).
//!
//! Covers every monitor's work area with a click-catching `WS_EX_LAYERED`
//! (`alpha = 1/255`, invisible to the human eye) `WS_EX_TOPMOST | WS_EX_TOOLWINDOW
//! | WS_EX_NOACTIVATE` window while the tray control panel is open. Clicking
//! anywhere outside the panel immediately hides all scrims and invokes the
//! registered dismiss callback without spawning or tearing down any WebView2
//! processes.

use std::sync::{
	Mutex,
	OnceLock,
	atomic::{AtomicIsize, Ordering},
};

use tauri::{AppHandle, Manager, Runtime};

use crate::{Bounds::Monitor as Area, Menu};

#[derive(Clone, Copy)]
struct ScrimBounds {
	X:i32,
	Y:i32,
	W:i32,
	H:i32,
}

static ScrimRects:OnceLock<Mutex<Vec<ScrimBounds>>> = OnceLock::new();

static LiveScrims:OnceLock<Mutex<Vec<isize>>> = OnceLock::new();

static ControlHwnd:AtomicIsize = AtomicIsize::new(0);

type DismissFn = Box<dyn Fn() + Send + Sync>;

static DismissCallback:OnceLock<Mutex<Option<DismissFn>>> = OnceLock::new();

const WM_APP_OPEN:u32 = 0x8000 + 11;

const WM_APP_CLOSE:u32 = 0x8000 + 12;

fn RectsLock() -> &'static Mutex<Vec<ScrimBounds>> { ScrimRects.get_or_init(|| Mutex::new(Vec::new())) }

fn LiveLock() -> &'static Mutex<Vec<isize>> { LiveScrims.get_or_init(|| Mutex::new(Vec::new())) }

fn DismissLock() -> &'static Mutex<Option<DismissFn>> { DismissCallback.get_or_init(|| Mutex::new(None)) }

fn EnsureStarted() {
	if ControlHwnd.load(Ordering::Acquire) != 0 {
		return;
	}

	let (ReadyTx, ReadyRx) = std::sync::mpsc::sync_channel::<()>(1);

	let _ = std::thread::Builder::new().name("radius-win32-backdrop".into()).spawn(move || {
		RunLoop(ReadyTx);
	});

	let _ = ReadyRx.recv_timeout(std::time::Duration::from_secs(2));
}

fn Post(Msg:u32) {
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

/// Show native transparent scrims over every monitor's work area.
pub fn Open<R:Runtime>(App:&AppHandle<R>) -> tauri::Result<()> {
	EnsureStarted();

	let AppForDismiss = App.clone();

	*DismissLock().lock().unwrap_or_else(|Poisoned| Poisoned.into_inner()) = Some(Box::new(move || {
		let Handle = AppForDismiss.clone();

		let _ = AppForDismiss.run_on_main_thread(move || {
			if let Some(Panel) = Handle.get_webview_window(Menu::Label::Const) {
				let _ = Menu::Hide::Fn(&Handle, &Panel);
			} else {
				Close();
			}
		});
	}));

	let Monitors = App.available_monitors()?;

	let Rects:Vec<ScrimBounds> = Monitors
		.iter()
		.map(|Monitor| {
			let BoxArea = Area::Fn(Monitor);

			ScrimBounds {
				X:BoxArea.X.round() as i32,
				Y:BoxArea.Y.round() as i32,
				W:(BoxArea.Width.round() as i32).max(1),
				H:(BoxArea.Height.round() as i32).max(1),
			}
		})
		.collect();

	*RectsLock().lock().unwrap_or_else(|Poisoned| Poisoned.into_inner()) = Rects;

	Post(WM_APP_OPEN);

	Ok(())
}

/// Immediately hide every native scrim window (safe & non-blocking from any thread).
pub fn Close() {
	let Hwnds:Vec<isize> = LiveLock().lock().unwrap_or_else(|Poisoned| Poisoned.into_inner()).clone();

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

	Post(WM_APP_CLOSE);
}

/// Re-raise all currently visible scrim windows into the topmost band (called
/// before `Platform::Panel` when overlays reorder above the taskbar).
pub fn RaiseVisible() {
	use windows_sys::Win32::{
		Foundation::HWND,
		UI::WindowsAndMessaging::{
			HWND_TOPMOST,
			IsWindow,
			IsWindowVisible,
			SWP_NOACTIVATE,
			SWP_NOMOVE,
			SWP_NOSIZE,
			SetWindowPos,
		},
	};

	let Hwnds:Vec<isize> = LiveLock().lock().unwrap_or_else(|Poisoned| Poisoned.into_inner()).clone();

	for Raw in Hwnds {
		let Hwnd = Raw as HWND;

		unsafe {
			if !Hwnd.is_null() && IsWindow(Hwnd) != 0 && IsWindowVisible(Hwnd) != 0 {
				SetWindowPos(Hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
			}
		}
	}
}

fn RunLoop(ReadyTx:std::sync::mpsc::SyncSender<()>) {
	use std::ptr;

	use windows_sys::Win32::{
		Foundation::{HWND, LPARAM, LRESULT, WPARAM},
		UI::WindowsAndMessaging::{
			CreateWindowExW,
			DefWindowProcW,
			DestroyWindow,
			DispatchMessageW,
			GetMessageW,
			HWND_TOPMOST,
			LWA_ALPHA,
			MSG,
			RegisterClassExW,
			SW_HIDE,
			SW_SHOWNOACTIVATE,
			SWP_NOACTIVATE,
			SetLayeredWindowAttributes,
			SetWindowPos,
			ShowWindow,
			TranslateMessage,
			WM_LBUTTONDOWN,
			WM_MBUTTONDOWN,
			WM_RBUTTONDOWN,
			WNDCLASSEXW,
			WS_EX_LAYERED,
			WS_EX_NOACTIVATE,
			WS_EX_TOOLWINDOW,
			WS_EX_TOPMOST,
			WS_POPUP,
		},
	};

	unsafe extern "system" fn ScrimProc(Hwnd:HWND, Msg:u32, WParam:WPARAM, LParam:LPARAM) -> LRESULT {
		const WM_POINTERDOWN:u32 = 0x0246;

		if matches!(Msg, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_POINTERDOWN) {
			Close();

			if let Some(Cb) = DismissLock().lock().unwrap_or_else(|Poisoned| Poisoned.into_inner()).as_ref() {
				Cb();
			}

			return 0;
		}

		unsafe { DefWindowProcW(Hwnd, Msg, WParam, LParam) }
	}

	let ScrimClass:Vec<u16> = "RadiusNativeScrim\0".encode_utf16().collect();

	let CtrlClass:Vec<u16> = "RadiusScrimControl\0".encode_utf16().collect();

	unsafe {
		let WcScrim = WNDCLASSEXW {
			cbSize:std::mem::size_of::<WNDCLASSEXW>() as u32,
			style:0,
			lpfnWndProc:Some(ScrimProc),
			cbClsExtra:0,
			cbWndExtra:0,
			hInstance:ptr::null_mut(),
			hIcon:ptr::null_mut(),
			hCursor:ptr::null_mut(),
			hbrBackground:ptr::null_mut(),
			lpszMenuName:ptr::null(),
			lpszClassName:ScrimClass.as_ptr(),
			hIconSm:ptr::null_mut(),
		};
		let _ = RegisterClassExW(&WcScrim);

		let WcCtrl = WNDCLASSEXW {
			cbSize:std::mem::size_of::<WNDCLASSEXW>() as u32,
			style:0,
			lpfnWndProc:Some(DefWindowProcW),
			cbClsExtra:0,
			cbWndExtra:0,
			hInstance:ptr::null_mut(),
			hIcon:ptr::null_mut(),
			hCursor:ptr::null_mut(),
			hbrBackground:ptr::null_mut(),
			lpszMenuName:ptr::null(),
			lpszClassName:CtrlClass.as_ptr(),
			hIconSm:ptr::null_mut(),
		};
		let _ = RegisterClassExW(&WcCtrl);

		let Ctrl = CreateWindowExW(
			WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
			CtrlClass.as_ptr(),
			ptr::null(),
			WS_POPUP,
			0,
			0,
			0,
			0,
			ptr::null_mut(),
			ptr::null_mut(),
			ptr::null_mut(),
			ptr::null(),
		);

		ControlHwnd.store(Ctrl as isize, Ordering::Release);

		let _ = ReadyTx.send(());

		let mut Pool:Vec<HWND> = Vec::new();

		let mut Msg:MSG = std::mem::zeroed();

		while GetMessageW(&mut Msg, ptr::null_mut(), 0, 0) > 0 {
			match Msg.message {
				WM_APP_OPEN => {
					let Rects = RectsLock().lock().unwrap_or_else(|Poisoned| Poisoned.into_inner()).clone();

					while Pool.len() < Rects.len() {
						let Hwnd = CreateWindowExW(
							WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED,
							ScrimClass.as_ptr(),
							ptr::null(),
							WS_POPUP,
							0,
							0,
							1,
							1,
							ptr::null_mut(),
							ptr::null_mut(),
							ptr::null_mut(),
							ptr::null(),
						);

						if Hwnd.is_null() {
							break;
						}

						// Alpha = 1/255: completely invisible to the eye while remaining
						// hit-testable for mouse clicks outside the control panel.
						let _ = SetLayeredWindowAttributes(Hwnd, 0, 1, LWA_ALPHA);

						Pool.push(Hwnd);
					}

					for (Index, &Hwnd) in Pool.iter().enumerate() {
						if let Some(Rect) = Rects.get(Index) {
							let _ = SetWindowPos(Hwnd, HWND_TOPMOST, Rect.X, Rect.Y, Rect.W, Rect.H, SWP_NOACTIVATE);
							let _ = ShowWindow(Hwnd, SW_SHOWNOACTIVATE);
						} else {
							let _ = ShowWindow(Hwnd, SW_HIDE);
						}
					}

					*LiveLock().lock().unwrap_or_else(|Poisoned| Poisoned.into_inner()) =
						Pool.iter().map(|&H| H as isize).collect();
				}
				WM_APP_CLOSE => {
					for &Hwnd in &Pool {
						let _ = ShowWindow(Hwnd, SW_HIDE);
					}
				}
				_ => {
					TranslateMessage(&Msg);
					DispatchMessageW(&Msg);
				}
			}
		}

		for Hwnd in Pool {
			let _ = DestroyWindow(Hwnd);
		}

		let _ = DestroyWindow(Ctrl);
	}
}
