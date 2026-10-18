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

#[cfg(feature = "self-update")]
use std::io;
use std::path::Path;

use anyhow::Context as _;
use auto_launch::{AutoLaunch, AutoLaunchBuilder, WindowsEnableMode};
use notify_rust::Notification;

/// Name of the app as the user sees it, for example in toast notifications.
pub const APP_DISPLAY_NAME: &str = "Razer Battery Report";
/// The reverse-DNS id of the app. On Windows it is the toast AUMID and the name of the instance
/// mutex; later the macOS bundle id and the Linux desktop id.
#[cfg(windows)]
const APP_ID: &str = "xzeldon.RazerBatteryReport";

/// How the running app got onto the computer, which decides how an update replaces it.
#[cfg(feature = "self-update")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallKind {
    Installer,
    Portable,
}

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

/// Marks the process as a running tray, so an installer can detect it. Call it once at tray
/// start; the mark lasts until the process exits.
pub fn announce_running() {
    imp::announce_running();
}

/// Removes what [`register_notifications`] registered with the OS. Succeeds when nothing is
/// registered.
pub fn unregister_notifications() -> anyhow::Result<()> {
    imp::unregister_notifications()
}

/// A notification builder carrying the app's identity.
pub fn notification() -> Notification {
    let mut notification = Notification::new();
    #[cfg(windows)]
    notification.app_id(APP_ID);
    #[cfg(not(windows))]
    notification.appname(APP_DISPLAY_NAME);
    notification
}

#[cfg(feature = "self-update")]
pub fn install_kind() -> InstallKind {
    imp::install_kind()
}

/// The end of the release file name that holds the app for `kind`, for example `-setup.exe`.
/// Empty when there are no release files for this OS yet.
#[cfg(feature = "self-update")]
pub fn asset_suffix(kind: InstallKind) -> &'static str {
    match kind {
        InstallKind::Installer => imp::INSTALLER_SUFFIX,
        InstallKind::Portable => imp::PORTABLE_SUFFIX,
    }
}

/// Starts the downloaded installer without a window and detached from this process, which has
/// to exit afterwards. The installer writes its log to `log`.
#[cfg(feature = "self-update")]
pub fn launch_installer(setup: &Path, log: &Path) -> io::Result<()> {
    imp::launch_installer(setup, log)
}

/// Starts a new instance of the running executable, detached from this process.
#[cfg(feature = "self-update")]
pub fn relaunch() -> io::Result<()> {
    imp::relaunch()
}

/// Whether a toast can carry buttons that report back to the app.
#[cfg(feature = "self-update")]
pub fn toast_actions_supported() -> bool {
    imp::TOAST_ACTIONS_SUPPORTED
}

/// Shows a toast with buttons given as `(id, label)`. `on_action` gets the id of the clicked
/// button, or the first id when the toast itself is clicked; it runs on another thread and
/// may be called any time while the app runs. Without button support only the text is shown.
#[cfg(feature = "self-update")]
pub fn show_toast(
    title: &str,
    body: &str,
    actions: &[(&str, &str)],
    on_action: impl Fn(String) + Send + 'static,
) -> anyhow::Result<()> {
    imp::show_toast(title, body, actions, on_action)
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
