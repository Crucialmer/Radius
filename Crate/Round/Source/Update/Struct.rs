//! The update-check and in-app download state surfaced to the tray panel.

use serde::Serialize;

/// Release update status reported by the background GitHub checker and downloader.
#[derive(Clone, Debug, Serialize)]
pub struct Struct {
	#[serde(rename = "status")]
	pub Status:String,
	#[serde(rename = "current_version")]
	pub CurrentVersion:String,
	#[serde(rename = "latest_version")]
	pub LatestVersion:Option<String>,
	#[serde(rename = "release_url")]
	pub ReleaseUrl:Option<String>,
	#[serde(rename = "download_url")]
	pub DownloadUrl:Option<String>,
	#[serde(rename = "asset_name")]
	pub AssetName:Option<String>,
	#[serde(rename = "downloaded_bytes")]
	pub DownloadedBytes:u64,
	#[serde(rename = "total_bytes")]
	pub TotalBytes:u64,
	#[serde(rename = "progress")]
	pub Progress:u8,
	#[serde(rename = "message")]
	pub Message:Option<String>,
}

impl Default for Struct {
	fn default() -> Self {
		Self {
			Status:"idle".to_owned(),
			CurrentVersion:env!("CARGO_PKG_VERSION").to_owned(),
			LatestVersion:None,
			ReleaseUrl:None,
			DownloadUrl:None,
			AssetName:None,
			DownloadedBytes:0,
			TotalBytes:0,
			Progress:0,
			Message:None,
		}
	}
}
