//! Building the panel window.

use tauri::{AppHandle, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::{
	Menu::{Height, Label, Width},
	Overlay::{Layer::Trait::Trait as _, Platform::Struct::Struct as Platform},
	Window::Chrome,
};

/// The panel window, built hidden and ordered above the scrims.
///
/// Built once and re-used: `Toggle` builds it on the first click, so a launch
/// that never opens the panel never pays for it.
pub fn Fn<R:Runtime>(App:&AppHandle<R>) -> tauri::Result<WebviewWindow<R>> {
	let Builder = Chrome::Fn(
		WebviewWindowBuilder::new(App, Label::Const, WebviewUrl::App("menu.html".into())),
		"Radius",
	)
	.inner_size(Width::Const, Height::Const)
	.shadow(true);

	let Window = Builder.build()?;

	Platform::Panel(&Window);

	Ok(Window)
}
