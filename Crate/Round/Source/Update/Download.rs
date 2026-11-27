//! Streaming in-app release download with live progress reporting and
//! automatic self-replacement + restart on Windows.

use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::{
	Menu::Label as MenuLabel,
	Update::{Store::Struct as Store, Struct::Struct as Info},
};

/// Download the cached release asset in the background, emit live progress to
/// the panel, extract `Radius.exe`, and atomically swap & relaunch.
pub fn Fn<R:Runtime>(App:&AppHandle<R>) -> Result<(), String> {
	let Some(Held) = App.try_state::<Store>() else {
		return Err("更新状态未初始化".to_owned());
	};

	let (DownloadUrl, AssetName, LatestVersion, ExpectedBytes) = {
		let mut Guard = Held.0.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner());

		if matches!(Guard.Status.as_str(), "downloading" | "installing") {
			return Ok(());
		}

		let Some(Url) = Guard
			.DownloadUrl
			.clone()
			.filter(|Candidate| Candidate.starts_with("https://github.com/Crucialmer/Radius/"))
		else {
			return Err("未找到可用的更新安装包链接，请前往发布页手动下载".to_owned());
		};

		let Name = Guard.AssetName.clone().unwrap_or_else(|| "Radius.zip".to_owned());

		let Version = Guard.LatestVersion.clone().unwrap_or_default();

		let Total = Guard.TotalBytes;

		Guard.Status = "downloading".to_owned();
		Guard.DownloadedBytes = 0;
		Guard.Progress = 0;
		Guard.Message = Some(format!("正在连接下载 v{Version}..."));

		let Snapshot = Guard.clone();

		drop(Guard);

		let _ = App.emit_to(MenuLabel::Const, "update", Snapshot);

		(Url, Name, Version, Total)
	};

	let Handle = App.clone();

	std::thread::spawn(move || {
		if let Err(Error) = RunUpdate(&Handle, &DownloadUrl, &AssetName, &LatestVersion, ExpectedBytes) {
			Publish(&Handle, |State| {
				State.Status = "error".to_owned();
				State.Message = Some(Error.clone());
			});
		}
	});

	Ok(())
}

fn Publish<R:Runtime, F:FnOnce(&mut Info)>(App:&AppHandle<R>, Mutate:F) {
	if let Some(Held) = App.try_state::<Store>() {
		let mut Guard = Held.0.lock().unwrap_or_else(|Poisoned| Poisoned.into_inner());

		Mutate(&mut Guard);

		let Snapshot = Guard.clone();

		drop(Guard);

		let _ = App.emit_to(MenuLabel::Const, "update", Snapshot);
	}
}

#[cfg(target_os = "windows")]
fn FormatSize(Bytes:u64) -> String {
	const MB:f64 = 1024.0 * 1024.0;

	const KB:f64 = 1024.0;

	if Bytes >= 1024 * 1024 {
		format!("{:.1} MB", (Bytes as f64) / MB)
	} else {
		format!("{:.0} KB", (Bytes as f64) / KB)
	}
}

#[cfg(target_os = "windows")]
fn SplitHttpsUrl(Url:&str) -> Result<(String, u16, String), String> {
	let Rest = Url.strip_prefix("https://").ok_or_else(|| "仅支持 HTTPS 更新下载".to_owned())?;

	let (Authority, Path) = match Rest.find('/') {
		Some(Index) => (&Rest[..Index], &Rest[Index..]),
		None => (Rest, "/"),
	};

	let (Host, Port) = match Authority.rsplit_once(':') {
		Some((HostPart, PortPart)) => (HostPart, PortPart.parse::<u16>().unwrap_or(443)),
		None => (Authority, 443),
	};

	if Host.is_empty() {
		return Err("更新下载地址无效".to_owned());
	}

	Ok((Host.to_owned(), Port, Path.to_owned()))
}

#[cfg(target_os = "windows")]
fn RunUpdate<R:Runtime>(
	App:&AppHandle<R>,
	DownloadUrl:&str,
	AssetName:&str,
	LatestVersion:&str,
	ExpectedBytes:u64,
) -> Result<(), String> {
	use std::{
		env,
		fs::{self, File},
		io::Read,
		os::windows::process::CommandExt,
		path::{Path, PathBuf},
		process::{Command, Stdio},
	};

	const CREATE_NO_WINDOW:u32 = 0x0800_0000;

	// Must NOT be combined with `DETACHED_PROCESS` (0x8): Windows treats
	// `CREATE_NO_WINDOW | DETACHED_PROCESS` as mutually exclusive, causing
	// `powershell.exe` to terminate immediately before executing `apply.ps1`.
	const CREATE_NEW_PROCESS_GROUP:u32 = 0x0000_0200;

	let CurrentExe = StripVerbatim(env::current_exe().map_err(|Error| format!("无法定位当前程序路径: {Error}"))?);

	let UpdateDir = StripVerbatim(crate::Settings::Store::Dir().join("data").join("update"));

	let _ = fs::remove_dir_all(&UpdateDir);

	fs::create_dir_all(&UpdateDir).map_err(|Error| format!("无法创建更新缓存目录: {Error}"))?;

	let SafeName = Path::new(AssetName)
		.file_name()
		.and_then(|Part| Part.to_str())
		.unwrap_or("Radius.zip");

	let PartPath = UpdateDir.join(format!("{SafeName}.part"));

	let ArchivePath = UpdateDir.join(SafeName);

	StreamDownload(App, DownloadUrl, LatestVersion, ExpectedBytes, &PartPath)?;

	fs::rename(&PartPath, &ArchivePath).map_err(|Error| format!("无法完成更新包写入: {Error}"))?;

	Publish(App, |State| {
		State.Status = "installing".to_owned();
		State.Progress = 100;
		State.Message = Some("下载完成，正在解压并校验...".to_owned());
	});

	let StagedExe:PathBuf = if SafeName.to_ascii_lowercase().ends_with(".zip") {
		let StageDir = UpdateDir.join("stage");

		fs::create_dir_all(&StageDir).map_err(|Error| format!("无法创建解压目录: {Error}"))?;

		// Prefer Windows 10/11 built-in native `System32\tar.exe` (~25ms, zero .NET
		// CLR / AMSI overhead); fall back to PowerShell only on older environments.
		let TarPath = System32Tool("tar.exe");

		let TarOk = Command::new(&TarPath)
			.arg("-xf")
			.arg(&ArchivePath)
			.arg("-C")
			.arg(&StageDir)
			.stdin(Stdio::null())
			.stdout(Stdio::null())
			.stderr(Stdio::null())
			.creation_flags(CREATE_NO_WINDOW)
			.status()
			.is_ok_and(|Status| Status.success());

		if !TarOk {
			let Output = Command::new("powershell.exe")
				.args([
					"-NoProfile",
					"-NonInteractive",
					"-WindowStyle",
					"Hidden",
					"-Command",
					"Expand-Archive -LiteralPath $env:RADIUS_ZIP -DestinationPath $env:RADIUS_STAGE -Force",
				])
				.env("RADIUS_ZIP", &ArchivePath)
				.env("RADIUS_STAGE", &StageDir)
				.stdin(Stdio::null())
				.creation_flags(CREATE_NO_WINDOW)
				.output()
				.map_err(|Error| format!("无法调用系统解压工具: {Error}"))?;

			if !Output.status.success() {
				return Err("解压更新压缩包失败".to_owned());
			}
		}

		FindExe(&StageDir).ok_or_else(|| "更新包中未找到 Radius.exe".to_owned())?
	} else {
		ArchivePath.clone()
	};

	// Verify the staged binary is a valid Windows PE executable before swapping.
	let Meta = fs::metadata(&StagedExe).map_err(|Error| format!("无法读取更新文件: {Error}"))?;

	if Meta.len() < 64 * 1024 {
		return Err("更新文件体积异常，已中止安装".to_owned());
	}

	let mut Header = [0u8; 2];

	File::open(&StagedExe)
		.and_then(|mut Handle| Handle.read_exact(&mut Header))
		.map_err(|Error| format!("无法校验更新文件头: {Error}"))?;

	if &Header != b"MZ" {
		return Err("更新文件不是有效的可执行程序".to_owned());
	}

	Publish(App, |State| {
		State.Status = "installing".to_owned();
		State.Progress = 100;
		State.Message = Some("校验通过，正在重启并安装更新...".to_owned());
	});

	let WorkDir = CurrentExe
		.parent()
		.map(Path::to_path_buf)
		.unwrap_or_else(crate::Settings::Store::Dir);

	let ScriptPath = UpdateDir.join("apply.cmd");

	// Ultra-lightweight native cmd.exe swapper (~100ms, zero .NET CLR / AMSI
	// stall, UTF-16 env vars for Chinese & space paths, locale-independent CSV
	// tasklist check).
	let ScriptBody = "@echo off\r\nsetlocal EnableExtensions\r\nset /a TRIES=0\r\n:waitloop\r\ntasklist /FI \"PID eq \
	                  %RADIUS_PID%\" /FO CSV /NH 2>nul | findstr /C:\",\\\"%RADIUS_PID%\\\",\" >nul\r\nif errorlevel \
	                  1 goto swap\r\nset /a TRIES+=1\r\nif %TRIES% GEQ 20 goto swap\r\nping -n 2 127.0.0.1 \
	                  >nul\r\ngoto waitloop\r\n:swap\r\nset /a STRIES=0\r\n:swaploop\r\nif exist \
	                  \"%RADIUS_TARGET%.bak\" del /f /q \"%RADIUS_TARGET%.bak\" >nul 2>&1\r\nif exist \
	                  \"%RADIUS_TARGET%\" move /y \"%RADIUS_TARGET%\" \"%RADIUS_TARGET%.bak\" >nul 2>&1\r\nmove /y \
	                  \"%RADIUS_STAGED%\" \"%RADIUS_TARGET%\" >nul 2>&1\r\nif not errorlevel 1 (\r\nif exist \
	                  \"%RADIUS_TARGET%.bak\" del /f /q \"%RADIUS_TARGET%.bak\" >nul 2>&1\r\ngoto launch\r\n)\r\nif \
	                  not exist \"%RADIUS_TARGET%\" if exist \"%RADIUS_TARGET%.bak\" move /y \"%RADIUS_TARGET%.bak\" \
	                  \"%RADIUS_TARGET%\" >nul 2>&1\r\nset /a STRIES+=1\r\nif %STRIES% GEQ 20 goto launch\r\nping -n \
	                  2 127.0.0.1 >nul\r\ngoto swaploop\r\n:launch\r\nif exist \"%RADIUS_TARGET%\" start \"\" /d \
	                  \"%RADIUS_WORKDIR%\" \"%RADIUS_TARGET%\"\r\nexit /b 0\r\n";

	fs::write(&ScriptPath, ScriptBody.as_bytes()).map_err(|Error| format!("无法生成安装脚本: {Error}"))?;

	// 1. Stop the background WinEvent watcher and hide native GPU overlays
	//    immediately so nothing fights window teardown or `Taskbar::Restore()`.
	crate::Overlay::Platform::Watch::Stop();
	crate::Overlay::Platform::Engine::HideAll();
	crate::Backdrop::Close::Fn(App);

	// 2. Hide the panel on the main thread (with a bounded wait so shutdown can
	//    never deadlock), restore the taskbar region, and delete the tray icon.
	let (DoneTx, DoneRx) = std::sync::mpsc::sync_channel::<()>(1);

	let MainApp = App.clone();

	let _ = App.run_on_main_thread(move || {
		crate::Backdrop::Close::Fn(&MainApp);

		for Window in MainApp.webview_windows().into_values() {
			let _ = Window.hide();
		}

		crate::Overlay::Platform::Taskbar::Restore();
		crate::Tray::Remove::Fn(&MainApp);

		let _ = DoneTx.send(());
	});

	let _ = DoneRx.recv_timeout(std::time::Duration::from_millis(800));

	crate::Overlay::Platform::Engine::Stop();

	let CmdPath = System32Tool("cmd.exe");

	Command::new(&CmdPath)
		.args(["/d", "/c"])
		.arg(&ScriptPath)
		.env("RADIUS_PID", std::process::id().to_string())
		.env("RADIUS_TARGET", &CurrentExe)
		.env("RADIUS_STAGED", StripVerbatim(StagedExe))
		.env("RADIUS_WORKDIR", &WorkDir)
		.stdin(Stdio::null())
		.stdout(Stdio::null())
		.stderr(Stdio::null())
		.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP)
		.spawn()
		.map_err(|Error| format!("无法启动更新安装程序: {Error}"))?;

	// Guarantee the old process exits promptly even if a WebView2 controller
	// stalls during teardown, so the swapper replaces `Radius.exe` in ~150ms.
	std::thread::spawn(|| {
		std::thread::sleep(std::time::Duration::from_millis(1500));
		std::process::exit(0);
	});

	let ExitApp = App.clone();

	let _ = App.run_on_main_thread(move || {
		ExitApp.exit(0);
	});

	Ok(())
}

#[cfg(target_os = "windows")]
fn System32Tool(Name:&str) -> std::path::PathBuf {
	let Root = std::env::var_os("SystemRoot")
		.map(std::path::PathBuf::from)
		.unwrap_or_else(|| std::path::PathBuf::from(r"C:\Windows"));

	let Candidate = Root.join("System32").join(Name);

	if Candidate.is_file() { Candidate } else { std::path::PathBuf::from(Name) }
}

#[cfg(target_os = "windows")]
fn StripVerbatim(Path:std::path::PathBuf) -> std::path::PathBuf {
	Path.to_str()
		.and_then(|Text| Text.strip_prefix(r"\\?\"))
		.map(std::path::PathBuf::from)
		.unwrap_or(Path)
}

#[cfg(target_os = "windows")]
fn FindExe(Root:&std::path::Path) -> Option<std::path::PathBuf> {
	let Entries = std::fs::read_dir(Root).ok()?;

	for Entry in Entries.flatten() {
		let Path = Entry.path();

		if Path.is_dir() {
			if let Some(Found) = FindExe(&Path) {
				return Some(Found);
			}
		} else if Path
			.file_name()
			.and_then(|Name| Name.to_str())
			.is_some_and(|Name| Name.eq_ignore_ascii_case("Radius.exe"))
		{
			return Some(Path);
		}
	}

	None
}

#[cfg(target_os = "windows")]
fn StreamDownload<R:Runtime>(
	App:&AppHandle<R>,
	DownloadUrl:&str,
	LatestVersion:&str,
	ExpectedBytes:u64,
	Destination:&std::path::Path,
) -> Result<(), String> {
	use std::{
		ffi::c_void,
		fs::File,
		io::Write,
		ptr,
		time::{Duration, Instant},
	};

	use windows_sys::Win32::Networking::WinHttp::{
		WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
		WINHTTP_FLAG_SECURE,
		WINHTTP_QUERY_CONTENT_LENGTH,
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

	let (HostStr, Port, PathStr) = SplitHttpsUrl(DownloadUrl)?;

	let Agent:Vec<u16> = format!("Radius/{}\0", env!("CARGO_PKG_VERSION")).encode_utf16().collect();

	let Host:Vec<u16> = format!("{HostStr}\0").encode_utf16().collect();

	let Method:Vec<u16> = "GET\0".encode_utf16().collect();

	let Path:Vec<u16> = format!("{PathStr}\0").encode_utf16().collect();

	unsafe {
		let Session = WinHttpOpen(Agent.as_ptr(), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, ptr::null(), ptr::null(), 0);

		if Session.is_null() {
			return Err("无法初始化下载会话".to_owned());
		}

		let _SessionGuard = HandleGuard(Session);

		let _ = WinHttpSetTimeouts(Session, 10_000, 10_000, 15_000, 30_000);

		let Connect = WinHttpConnect(Session, Host.as_ptr(), Port, 0);

		if Connect.is_null() {
			return Err("连接更新服务器失败".to_owned());
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
			return Err("创建下载请求失败".to_owned());
		}

		let _RequestGuard = HandleGuard(Request);

		if WinHttpSendRequest(Request, ptr::null(), 0, ptr::null(), 0, 0, 0) == 0 {
			return Err("发送下载请求失败".to_owned());
		}

		if WinHttpReceiveResponse(Request, ptr::null_mut()) == 0 {
			return Err("等待下载响应超时".to_owned());
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
			return Err("无法读取下载状态码".to_owned());
		}

		if StatusCode != 200 {
			return Err(format!("下载服务器返回状态码 {StatusCode}"));
		}

		let mut LenBuf = [0u16; 32];

		let mut LenBytes = (LenBuf.len() * std::mem::size_of::<u16>()) as u32;

		let HeaderLen = if WinHttpQueryHeaders(
			Request,
			WINHTTP_QUERY_CONTENT_LENGTH,
			ptr::null(),
			LenBuf.as_mut_ptr() as *mut c_void,
			&mut LenBytes,
			ptr::null_mut(),
		) != 0
		{
			let Units = (LenBytes as usize) / std::mem::size_of::<u16>();

			String::from_utf16_lossy(&LenBuf[..Units.min(LenBuf.len())])
				.trim_matches('\0')
				.trim()
				.parse::<u64>()
				.unwrap_or(0)
		} else {
			0
		};

		let TotalBytes = if HeaderLen > 0 { HeaderLen } else { ExpectedBytes };

		let mut Output = File::create(Destination).map_err(|Error| format!("无法创建下载文件: {Error}"))?;

		let mut Buffer = vec![0u8; 32 * 1024];

		let mut Downloaded:u64 = 0;

		let mut LastEmit = Instant::now();

		let mut LastPercent:u8 = 0;

		const MAX_DOWNLOAD:u64 = 128 * 1024 * 1024;

		loop {
			let mut Available:u32 = 0;

			if WinHttpQueryDataAvailable(Request, &mut Available) == 0 {
				return Err("下载连接中断".to_owned());
			}

			if Available == 0 {
				break;
			}

			let ToRead = (Available as usize).min(Buffer.len());

			let mut Read:u32 = 0;

			if WinHttpReadData(Request, Buffer.as_mut_ptr() as *mut c_void, ToRead as u32, &mut Read) == 0 {
				return Err("读取下载数据失败".to_owned());
			}

			if Read == 0 {
				break;
			}

			Output
				.write_all(&Buffer[..Read as usize])
				.map_err(|Error| format!("写入更新包失败: {Error}"))?;

			Downloaded = Downloaded.saturating_add(u64::from(Read));

			if Downloaded > MAX_DOWNLOAD {
				return Err("更新包体积超出安全限制".to_owned());
			}

			let Percent:u8 = Downloaded
				.saturating_mul(100)
				.checked_div(TotalBytes)
				.map_or(0, |Ratio| Ratio.min(100) as u8);

			// Strictly throttle IPC progress updates to >= 120ms (plus final 100%)
			// so high-speed downloads never flood WebView2 & ResizeObserver on the main thread.
			if LastEmit.elapsed() >= Duration::from_millis(120) || (Percent == 100 && LastPercent != 100) {
				LastPercent = Percent;
				LastEmit = Instant::now();

				let Detail = if TotalBytes > 0 {
					format!(
						"正在下载 v{LatestVersion} · {Percent}% ({} / {})",
						FormatSize(Downloaded),
						FormatSize(TotalBytes)
					)
				} else {
					format!("正在下载 v{LatestVersion} · {}", FormatSize(Downloaded))
				};

				Publish(App, |State| {
					State.Status = "downloading".to_owned();
					State.DownloadedBytes = Downloaded;
					State.TotalBytes = TotalBytes;
					State.Progress = Percent;
					State.Message = Some(Detail);
				});
			}
		}

		Output.flush().map_err(|Error| format!("刷新更新包到磁盘失败: {Error}"))?;

		if Downloaded == 0 {
			return Err("下载的更新包为空".to_owned());
		}

		if TotalBytes > 0 && Downloaded < TotalBytes {
			return Err("下载未完整结束，请重试".to_owned());
		}
	}

	Ok(())
}

#[cfg(not(target_os = "windows"))]
fn RunUpdate<R:Runtime>(
	_App:&AppHandle<R>,
	_DownloadUrl:&str,
	_AssetName:&str,
	_LatestVersion:&str,
	_ExpectedBytes:u64,
) -> Result<(), String> {
	Err("仅支持 Windows 平台自动下载并安装更新".to_owned())
}
