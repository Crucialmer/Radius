//! Opening the GitHub release page in the default browser.

/// Default GitHub releases URL used when no release URL was cached.
pub const DefaultUrl:&str = "https://github.com/Crucialmer/Radius/releases/latest";

/// Open a validated Crucialmer/Radius GitHub URL in the user's browser.
#[cfg(target_os = "windows")]
pub fn Fn(Url:Option<&str>) -> Result<(), String> {
	use std::ptr;

	use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};

	let Target = Url
		.filter(|Candidate| Candidate.starts_with("https://github.com/Crucialmer/Radius/"))
		.unwrap_or(DefaultUrl);

	let Verb:Vec<u16> = "open\0".encode_utf16().collect();

	let WideUrl:Vec<u16> = format!("{Target}\0").encode_utf16().collect();

	unsafe {
		let Result = ShellExecuteW(
			ptr::null_mut(),
			Verb.as_ptr(),
			WideUrl.as_ptr(),
			ptr::null(),
			ptr::null(),
			SW_SHOWNORMAL,
		);

		if (Result as isize) <= 32 {
			return Err("无法调用系统默认浏览器打开更新页面".to_owned());
		}
	}

	Ok(())
}

/// Non-Windows fallback.
#[cfg(not(target_os = "windows"))]
pub fn Fn(_Url:Option<&str>) -> Result<(), String> { Ok(()) }
