//! Checking GitHub Releases in the background using native WinHTTP.

use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::{
	Menu::Label as MenuLabel,
	Tray::Label as TrayLabel,
	Update::{Store::Struct as Store, Struct::Struct as Info, Version},
};

struct ReleaseMeta {
	Tag:String,
	HtmlUrl:String,
	DownloadUrl:Option<String>,
	AssetName:Option<String>,
	AssetSize:u64,
}

/// Spawn a non-blocking update check against GitHub Releases.
pub fn Fn<R:Runtime>(App:&AppHandle<R>) {
	if let Some(Held) = App.try_state::<Store>() {
		let mut Guard = Held.0.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner());

		if matches!(Guard.Status.as_str(), "checking" | "downloading" | "installing") {
			return;
		}

		Guard.Status = "checking".to_owned();
		Guard.DownloadedBytes = 0;
		Guard.Progress = 0;
		Guard.Message = Some("正在检查更新...".to_owned());

		let Snapshot = Guard.clone();

		drop(Guard);

		let _ = App.emit_to(MenuLabel::Const, "update", Snapshot);
	}

	let Handle = App.clone();

	std::thread::spawn(move || {
		let Current = env!("CARGO_PKG_VERSION");

		let Next = match FetchLatest() {
			Ok(Meta) => {
				let Latest = Version::Clean(&Meta.Tag);

				if Version::IsNewer(Current, &Latest) {
					if let Some(Tray) = Handle.tray_by_id(TrayLabel::Const) {
						let _ = Tray.set_tooltip(Some(&format!("Radius — 发现新版本 v{Latest}")));
					}

					Info {
						Status:"available".to_owned(),
						CurrentVersion:Current.to_owned(),
						LatestVersion:Some(Latest.clone()),
						ReleaseUrl:Some(Meta.HtmlUrl),
						DownloadUrl:Meta.DownloadUrl,
						AssetName:Meta.AssetName,
						DownloadedBytes:0,
						TotalBytes:Meta.AssetSize,
						Progress:0,
						Message:Some(format!("发现新版本 v{Latest}")),
					}
				} else {
					if let Some(Tray) = Handle.tray_by_id(TrayLabel::Const) {
						let _ = Tray.set_tooltip(Some("Radius — 屏幕圆角工具"));
					}

					Info {
						Status:"up_to_date".to_owned(),
						CurrentVersion:Current.to_owned(),
						LatestVersion:Some(Latest),
						ReleaseUrl:Some(Meta.HtmlUrl),
						DownloadUrl:None,
						AssetName:None,
						DownloadedBytes:0,
						TotalBytes:0,
						Progress:0,
						Message:Some("当前已是最新版本".to_owned()),
					}
				}
			}
			Err(Error) => Info {
				Status:"error".to_owned(),
				CurrentVersion:Current.to_owned(),
				LatestVersion:None,
				ReleaseUrl:Some(crate::Update::Open::DefaultUrl.to_owned()),
				DownloadUrl:None,
				AssetName:None,
				DownloadedBytes:0,
				TotalBytes:0,
				Progress:0,
				Message:Some(Error),
			},
		};

		if let Some(Held) = Handle.try_state::<Store>() {
			*Held.0.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner()) = Next.clone();
		}

		let _ = Handle.emit_to(MenuLabel::Const, "update", Next);
	});
}

#[cfg(target_os = "windows")]
fn FetchLatest() -> Result<ReleaseMeta, String> {
	use std::{ffi::c_void, ptr};

	use windows_sys::Win32::Networking::WinHttp::{
		WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
		WINHTTP_FLAG_SECURE,
		WINHTTP_QUERY_FLAG_NUMBER,
		WINHTTP_QUERY_STATUS_CODE,
		WinHttpCloseHandle,
		WinHttpConnect,
		WinHttpOpen,
		WinHttpOpenRequest,
		WinHttpQueryDataAvailable,
		WinHttpQueryHeaders,
		WinHttpReadData,
		WinHttpReceiveResponse,
		WinHttpSendRequest,
		WinHttpSetTimeouts,
	};

	struct HandleGuard(*mut c_void);

	impl Drop for HandleGuard {
		fn drop(&mut self) {
			if !self.0.is_null() {
				unsafe { WinHttpCloseHandle(self.0) };
			}
		}
	}

	let Agent:Vec<u16> = format!("Radius/{}\0", env!("CARGO_PKG_VERSION")).encode_utf16().collect();

	let Host:Vec<u16> = "api.github.com\0".encode_utf16().collect();

	let Method:Vec<u16> = "GET\0".encode_utf16().collect();

	let Path:Vec<u16> = "/repos/Crucialmer/Radius/releases/latest\0".encode_utf16().collect();

	let Headers:Vec<u16> = "Accept: application/vnd.github+json\r\n\0".encode_utf16().collect();

	unsafe {
		let Session = WinHttpOpen(Agent.as_ptr(), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, ptr::null(), ptr::null(), 0);

		if Session.is_null() {
			return Err("无法初始化网络会话".to_owned());
		}

		let _SessionGuard = HandleGuard(Session);

		let _ = WinHttpSetTimeouts(Session, 5000, 5000, 5000, 8000);

		let Connect = WinHttpConnect(Session, Host.as_ptr(), 443, 0);

		if Connect.is_null() {
			return Err("连接 GitHub 服务器失败".to_owned());
		}

		let _ConnectGuard = HandleGuard(Connect);

		let Request = WinHttpOpenRequest(
			Connect,
			Method.as_ptr(),
			Path.as_ptr(),
			ptr::null(),
			ptr::null(),
			ptr::null(),
			WINHTTP_FLAG_SECURE,
		);

		if Request.is_null() {
			return Err("创建更新请求失败".to_owned());
		}

		let _RequestGuard = HandleGuard(Request);

		if WinHttpSendRequest(Request, Headers.as_ptr(), (Headers.len() - 1) as u32, ptr::null(), 0, 0, 0) == 0 {
			return Err("发送更新检查请求失败".to_owned());
		}

		if WinHttpReceiveResponse(Request, ptr::null_mut()) == 0 {
			return Err("接收 GitHub 更新响应超时".to_owned());
		}

		let mut StatusCode:u32 = 0;

		let mut StatusSize:u32 = std::mem::size_of::<u32>() as u32;

		if WinHttpQueryHeaders(
			Request,
			WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
			ptr::null(),
			&mut StatusCode as *mut u32 as *mut c_void,
			&mut StatusSize,
			ptr::null_mut(),
		) == 0
		{
			return Err("无法读取更新响应状态码".to_owned());
		}

		if StatusCode != 200 {
			return Err(format!("GitHub API 返回状态码 {StatusCode}"));
		}

		let mut Body:Vec<u8> = Vec::new();

		let mut Chunk = [0u8; 16 * 1024];

		const MAX_BODY:usize = 512 * 1024;

		loop {
			let mut Available:u32 = 0;

			if WinHttpQueryDataAvailable(Request, &mut Available) == 0 || Available == 0 {
				break;
			}

			let Remaining = MAX_BODY.saturating_sub(Body.len());

			if Remaining == 0 {
				return Err("更新响应体过大".to_owned());
			}

			let ToRead = (Available as usize).min(Chunk.len()).min(Remaining);

			let mut Read:u32 = 0;

			if WinHttpReadData(Request, Chunk.as_mut_ptr() as *mut c_void, ToRead as u32, &mut Read) == 0 || Read == 0 {
				break;
			}

			Body.extend_from_slice(&Chunk[..Read as usize]);
		}

		let Json:serde_json::Value = serde_json::from_slice(&Body).map_err(|_| "解析更新信息失败".to_owned())?;

		let Tag = Json
			.get("tag_name")
			.and_then(|Value| Value.as_str())
			.ok_or_else(|| "未找到发布版本标签".to_owned())?
			.to_owned();

		let HtmlUrl = Json
			.get("html_url")
			.and_then(|Value| Value.as_str())
			.unwrap_or(crate::Update::Open::DefaultUrl)
			.to_owned();

		let (DownloadUrl, AssetName, AssetSize) = PickAsset(&Json);

		Ok(ReleaseMeta { Tag, HtmlUrl, DownloadUrl, AssetName, AssetSize })
	}
}

#[cfg(target_os = "windows")]
fn PickAsset(Json:&serde_json::Value) -> (Option<String>, Option<String>, u64) {
	let Some(Assets) = Json.get("assets").and_then(|Value| Value.as_array()) else {
		return (None, None, 0);
	};

	let Score = |Name:&str| -> u8 {
		let Lower = Name.to_ascii_lowercase();

		if Lower == "radius.zip" {
			return 3;
		}

		if Lower.ends_with(".zip") && Lower.contains("radius") {
			return 2;
		}

		if Lower.ends_with(".exe") && Lower.contains("radius") {
			return 1;
		}

		0
	};

	let Best = Assets
		.iter()
		.filter_map(|Asset| {
			let Name = Asset.get("name")?.as_str()?;

			let Rank = Score(Name);

			if Rank == 0 {
				return None;
			}

			let Url = Asset.get("browser_download_url")?.as_str()?;

			if !Url.starts_with("https://github.com/Crucialmer/Radius/") {
				return None;
			}

			let Size = Asset.get("size").and_then(|Value| Value.as_u64()).unwrap_or(0);

			Some((Rank, Url.to_owned(), Name.to_owned(), Size))
		})
		.max_by_key(|(Rank, ..)| *Rank);

	match Best {
		Some((_, Url, Name, Size)) => (Some(Url), Some(Name), Size),
		None => (None, None, 0),
	}
}

#[cfg(not(target_os = "windows"))]
fn FetchLatest() -> Result<ReleaseMeta, String> { Err("仅支持 Windows 平台自动检查更新".to_owned()) }
