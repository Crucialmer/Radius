use std::{env, path::PathBuf};

/// The store file name.
pub const Const:&str = "settings.json";

/// The portable base directory alongside the executable so Radius runs purely
/// green without writing to OS-wide config directories.
pub fn Dir() -> PathBuf {
	if let Ok(Exe) = env::current_exe()
		&& let Some(Parent) = Exe.parent()
	{
		return Parent.to_path_buf();
	}

	PathBuf::from(".")
}

/// The portable store path, resolved inside [`Dir`].
pub fn Fn() -> PathBuf { Dir().join(Const) }
