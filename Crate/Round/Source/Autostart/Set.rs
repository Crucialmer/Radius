//! Enabling or disabling Windows startup launch in `HKCU\...\Run`.

/// Register or remove Radius under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
#[cfg(target_os = "windows")]
pub fn Fn(Enabled:bool) -> Result<(), String> {
	use std::{env, ptr};

	use windows_sys::Win32::{
		Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS},
		System::Registry::{
			HKEY,
			HKEY_CURRENT_USER,
			KEY_CREATE_SUB_KEY,
			KEY_SET_VALUE,
			REG_OPTION_NON_VOLATILE,
			REG_SZ,
			RegCloseKey,
			RegCreateKeyExW,
			RegDeleteValueW,
			RegOpenKeyExW,
			RegSetValueExW,
		},
	};

	let SubKey:Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Run\0".encode_utf16().collect();

	let Name:Vec<u16> = "Radius\0".encode_utf16().collect();

	unsafe {
		if Enabled {
			let Exe = env::current_exe().map_err(|Error| format!("无法获取当前程序路径: {Error}"))?;

			let Command = format!("\"{}\"\0", Exe.display());

			let Wide:Vec<u16> = Command.encode_utf16().collect();

			let mut Key:HKEY = ptr::null_mut();

			let Status = RegCreateKeyExW(
				HKEY_CURRENT_USER,
				SubKey.as_ptr(),
				0,
				ptr::null(),
				REG_OPTION_NON_VOLATILE,
				KEY_SET_VALUE | KEY_CREATE_SUB_KEY,
				ptr::null(),
				&mut Key,
				ptr::null_mut(),
			);

			if Status != ERROR_SUCCESS {
				return Err(format!("无法打开开机自启注册表项 (错误码 {Status})"));
			}

			let Bytes = (Wide.len() * std::mem::size_of::<u16>()) as u32;

			let WriteStatus = RegSetValueExW(Key, Name.as_ptr(), 0, REG_SZ, Wide.as_ptr() as *const u8, Bytes);

			let _ = RegCloseKey(Key);

			if WriteStatus != ERROR_SUCCESS {
				return Err(format!("无法写入开机自启注册表项 (错误码 {WriteStatus})"));
			}
		} else {
			let mut Key:HKEY = ptr::null_mut();

			let Status = RegOpenKeyExW(HKEY_CURRENT_USER, SubKey.as_ptr(), 0, KEY_SET_VALUE, &mut Key);

			if Status == ERROR_FILE_NOT_FOUND {
				return Ok(());
			}

			if Status != ERROR_SUCCESS {
				return Err(format!("无法打开开机自启注册表项 (错误码 {Status})"));
			}

			let DeleteStatus = RegDeleteValueW(Key, Name.as_ptr());

			let _ = RegCloseKey(Key);

			if DeleteStatus != ERROR_SUCCESS && DeleteStatus != ERROR_FILE_NOT_FOUND {
				return Err(format!("无法移除开机自启注册表项 (错误码 {DeleteStatus})"));
			}
		}
	}

	Ok(())
}

/// Non-Windows fallback.
#[cfg(not(target_os = "windows"))]
pub fn Fn(_Enabled:bool) -> Result<(), String> { Ok(()) }
