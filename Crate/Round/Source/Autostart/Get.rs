//! Querying whether Radius is registered to launch on Windows startup.

/// Returns `true` when the current user's `Run` registry key contains a `Radius`
/// entry pointing to the currently running executable.
#[cfg(target_os = "windows")]
pub fn Fn() -> bool {
	use std::{env, ptr};

	use windows_sys::Win32::{
		Foundation::ERROR_SUCCESS,
		System::Registry::{
			HKEY,
			HKEY_CURRENT_USER,
			KEY_QUERY_VALUE,
			REG_SZ,
			RegCloseKey,
			RegOpenKeyExW,
			RegQueryValueExW,
		},
	};

	let SubKey:Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Run\0".encode_utf16().collect();

	let Name:Vec<u16> = "Radius\0".encode_utf16().collect();

	unsafe {
		let mut Key:HKEY = ptr::null_mut();

		if RegOpenKeyExW(HKEY_CURRENT_USER, SubKey.as_ptr(), 0, KEY_QUERY_VALUE, &mut Key) != ERROR_SUCCESS {
			return false;
		}

		let mut Kind:u32 = 0;

		let mut Bytes:u32 = 0;

		let Status = RegQueryValueExW(Key, Name.as_ptr(), ptr::null(), &mut Kind, ptr::null_mut(), &mut Bytes);

		if Status != ERROR_SUCCESS || Kind != REG_SZ || Bytes <= 2 {
			let _ = RegCloseKey(Key);
			return false;
		}

		let mut Buffer = vec![0u16; (Bytes as usize).div_ceil(2)];

		let ReadStatus = RegQueryValueExW(
			Key,
			Name.as_ptr(),
			ptr::null(),
			&mut Kind,
			Buffer.as_mut_ptr() as *mut u8,
			&mut Bytes,
		);

		let _ = RegCloseKey(Key);

		if ReadStatus != ERROR_SUCCESS {
			return false;
		}

		let End = Buffer.iter().position(|&Unit| Unit == 0).unwrap_or(Buffer.len());

		let Stored = String::from_utf16_lossy(&Buffer[..End]);

		let StoredPath = Stored.trim().trim_matches('"');

		let Ok(Current) = env::current_exe() else {
			return !StoredPath.is_empty();
		};

		StoredPath.eq_ignore_ascii_case(&Current.to_string_lossy())
	}
}

/// Non-Windows fallback.
#[cfg(not(target_os = "windows"))]
pub fn Fn() -> bool { false }
