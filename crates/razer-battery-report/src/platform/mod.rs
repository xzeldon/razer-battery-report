//! Every OS-specific call of the app.

#[cfg(not(windows))]
mod other;
#[cfg(windows)]
// The Windows implementation calls the Win32 API directly; each `unsafe` block carries a
// `// SAFETY:` comment.
#[allow(unsafe_code)]
mod windows;

#[cfg(not(windows))]
use other as imp;
#[cfg(windows)]
use windows as imp;

use std::path::Path;

use anyhow::Context as _;
use auto_launch::{AutoLaunch, AutoLaunchBuilder, WindowsEnableMode};
use notify_rust::Notification;

/// Name of the app as the user sees it, for example in toast notifications.
pub const APP_DISPLAY_NAME: &str = "Razer Battery Report";
/// Application user model id that identifies the app's toasts on Windows.
#[cfg(windows)]
const AUMID: &str = "xzeldon.RazerBatteryReport";

/// Whether the taskbar is dark or light, which decides the colour of the tray icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    /// A dark taskbar: draw light glyphs.
    Dark,
    /// A light taskbar: draw dark glyphs.
    Light,
}

/// Releases the console when the tray was not started from a terminal, so no window stays open.
/// A console shared with a shell is kept. Call it once at tray start.
pub fn detach_console() {
    imp::detach_console();
}

/// The current taskbar theme. Dark when the OS gives no answer.
pub fn taskbar_theme() -> Theme {
    imp::taskbar_theme()
}

/// The edge length in pixels of the tray icon the OS wants. On Windows it must be called after
/// the tao event loop exists, because tao sets the process DPI awareness; before that the
/// answer is always 16.
pub fn tray_icon_size() -> u32 {
    imp::tray_icon_size()
}

/// Registers the app's name and icon for notifications. Call it once at tray start; failures
/// are logged and the notifications still work, only under a generic name.
pub fn register_notifications(icon_png: &[u8]) {
    imp::register_notifications(icon_png);
}

/// A notification builder carrying the app's identity.
pub fn notification() -> Notification {
    let mut notification = Notification::new();
    #[cfg(windows)]
    notification.app_id(AUMID);
    #[cfg(not(windows))]
    notification.appname(APP_DISPLAY_NAME);
    notification
}

/// Starts the app with the user's session when `enabled`. Calling it again repoints a moved
/// executable.
pub fn set_autostart(enabled: bool) -> anyhow::Result<()> {
    let launcher = launcher()?;
    if enabled {
        return launcher.enable().context("cannot enable autostart");
    }
    let registered = launcher
        .is_enabled()
        .context("cannot read autostart state")?;
    match registered {
        true => launcher.disable().context("cannot disable autostart"),
        false => Ok(()),
    }
}

pub fn open(path: &Path) -> anyhow::Result<()> {
    opener::open(path).with_context(|| format!("cannot open {}", path.display()))
}

/// Opens `url` in the default browser.
pub fn open_url(url: &str) -> anyhow::Result<()> {
    opener::open_browser(url).with_context(|| format!("cannot open {url}"))
}

fn launcher() -> anyhow::Result<AutoLaunch> {
    let exe = std::env::current_exe().context("cannot find the executable")?;
    AutoLaunchBuilder::new()
        .set_app_name(APP_DISPLAY_NAME)
        .set_app_path(&command_path(&exe))
        .set_windows_enable_mode(WindowsEnableMode::CurrentUser)
        .build()
        .context("cannot set up autostart")
}

/// The executable path as the autostart entry stores it. The Windows Run key holds a command
/// line, so a path with spaces has to be quoted.
fn command_path(exe: &Path) -> String {
    let text = exe.to_string_lossy();
    match cfg!(windows) {
        true => format!("\"{text}\""),
        false => text.into_owned(),
    }
}
