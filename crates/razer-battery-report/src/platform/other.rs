//! Defaults for the OSes without a dedicated implementation yet.

#[cfg(feature = "self-update")]
use std::io;
#[cfg(feature = "self-update")]
use std::path::Path;

use super::Theme;
#[cfg(feature = "self-update")]
use super::{InstallKind, notification};

/// Edge length of the tray icon in pixels.
const TRAY_ICON_SIZE: u32 = 32;
/// Ends of the release file names; there are no release files for these OSes yet, so nothing
/// can match.
#[cfg(feature = "self-update")]
pub const PORTABLE_SUFFIX: &str = "";
#[cfg(feature = "self-update")]
pub const INSTALLER_SUFFIX: &str = "";
#[cfg(feature = "self-update")]
pub const TOAST_ACTIONS_SUPPORTED: bool = false;

/// Nothing to do: only Windows attaches a console to a tray app.
pub fn detach_console() {}

pub fn taskbar_theme() -> Theme {
    Theme::Dark
}

/// Nothing to watch: the theme here is always dark.
pub fn watch_taskbar_theme(_on_change: impl Fn(Theme) -> bool + Send + 'static) {}

pub fn tray_icon_size() -> u32 {
    TRAY_ICON_SIZE
}

/// Nothing to register: notifications carry the app name directly.
pub fn register_notifications(_icon_png: &[u8]) {}

/// Nothing to announce: only the Windows installer looks for a running tray.
pub fn announce_running() {}

/// Nothing was registered.
pub fn unregister_notifications() -> anyhow::Result<()> {
    Ok(())
}

#[cfg(feature = "self-update")]
pub fn install_kind() -> InstallKind {
    InstallKind::Portable
}

/// No installer exists for these OSes yet.
#[cfg(feature = "self-update")]
pub fn launch_installer(_setup: &Path, _log: &Path) -> io::Result<()> {
    Err(io::Error::from(io::ErrorKind::Unsupported))
}

#[cfg(feature = "self-update")]
pub fn relaunch() -> io::Result<()> {
    std::process::Command::new(std::env::current_exe()?).spawn()?;
    Ok(())
}

#[cfg(feature = "self-update")]
pub fn show_toast(
    title: &str,
    body: &str,
    _actions: &[(&str, &str)],
    _on_action: impl Fn(String) + Send + 'static,
) -> anyhow::Result<()> {
    notification().summary(title).body(body).show()?;
    Ok(())
}
