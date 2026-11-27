//! 240Hz damped harmonic oscillator spring motion for the native tray popup window.

#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(target_os = "windows")]
static Generation:AtomicU64 = AtomicU64::new(0);

/// Cancel any active popup entrance spring immediately.
pub fn Cancel() {
	#[cfg(target_os = "windows")]
	{
		Generation.fetch_add(1, Ordering::AcqRel);
	}
}

/// Animate the popup window vertically from `StartY` to `TargetY` at 240Hz
/// using a snappy underdamped Apple-style spring (`k = 560, c = 36`).
#[cfg(target_os = "windows")]
pub fn SpringOpen(HwndRaw:isize, TargetX:i32, StartY:i32, TargetY:i32) {
	use std::time::{Duration, Instant};

	use windows_sys::Win32::{
		Foundation::{CloseHandle, HWND},
		System::Threading::{
			CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
			CreateWaitableTimerExW,
			SetWaitableTimer,
			WaitForSingleObject,
		},
		UI::WindowsAndMessaging::{IsWindow, IsWindowVisible, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos},
	};

	if HwndRaw == 0 || StartY == TargetY {
		return;
	}

	let Token = Generation.fetch_add(1, Ordering::AcqRel) + 1;

	let _ = std::thread::Builder::new().name("radius-menu-spring".into()).spawn(move || {
		let Hwnd = HwndRaw as HWND;

		unsafe {
			let Timer = {
				let HighRes = CreateWaitableTimerExW(
					std::ptr::null(),
					std::ptr::null(),
					CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
					0x001F_0003,
				);

				if !HighRes.is_null() {
					HighRes
				} else {
					CreateWaitableTimerExW(std::ptr::null(), std::ptr::null(), 0, 0x001F_0003)
				}
			};

			let Stiffness:f64 = 560.0;

			let Damping:f64 = 36.0;

			let Target = f64::from(TargetY);

			let mut Value = f64::from(StartY);

			let mut Velocity:f64 = 0.0;

			let Started = Instant::now();

			let mut Last = Started;

			let MaxDuration = Duration::from_millis(320);

			let (DueTime, FallbackMs) = crate::Overlay::Platform::Engine::DisplayFrameInterval100ns();

			let WaitMs = FallbackMs.max(16);

			while Generation.load(Ordering::Acquire) == Token {
				if IsWindow(Hwnd) == 0 || IsWindowVisible(Hwnd) == 0 {
					break;
				}

				if Started.elapsed() >= MaxDuration {
					let _ = SetWindowPos(
						Hwnd,
						std::ptr::null_mut(),
						TargetX,
						TargetY,
						0,
						0,
						SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
					);
					break;
				}

				if !Timer.is_null() {
					if SetWaitableTimer(Timer, &DueTime, 0, None, std::ptr::null(), 0) != 0 {
						WaitForSingleObject(Timer, WaitMs);
					} else {
						std::thread::sleep(Duration::from_millis(u64::from(FallbackMs)));
					}
				} else {
					std::thread::sleep(Duration::from_millis(u64::from(FallbackMs)));
				}

				if Generation.load(Ordering::Acquire) != Token {
					break;
				}

				let Now = Instant::now();

				let Dt = (Now - Last).as_secs_f64().clamp(0.0005, 0.05);

				Last = Now;

				let Steps = ((Dt * 240.0).ceil() as usize).max(1);

				let StepDt = Dt / (Steps as f64);

				for _ in 0..Steps {
					let Acc = Stiffness * (Target - Value) - Damping * Velocity;

					Velocity += Acc * StepDt;
					Value += Velocity * StepDt;
				}

				let Settled = (Value - Target).abs() < 0.25 && Velocity.abs() < 1.0;

				let NextY = if Settled { TargetY } else { Value.round() as i32 };

				let _ = SetWindowPos(
					Hwnd,
					std::ptr::null_mut(),
					TargetX,
					NextY,
					0,
					0,
					SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
				);

				if Settled {
					break;
				}
			}

			if !Timer.is_null() {
				let _ = CloseHandle(Timer);
			}
		}
	});
}

/// Non-Windows no-op.
#[cfg(not(target_os = "windows"))]
pub fn SpringOpen(_HwndRaw:isize, _TargetX:i32, _StartY:i32, _TargetY:i32) {}
