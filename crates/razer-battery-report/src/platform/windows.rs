//! The Windows implementation of the platform API.

#[cfg(feature = "self-update")]
use std::io;
#[cfg(feature = "self-update")]
use std::os::windows::process::CommandExt as _;
use std::path::Path;
#[cfg(feature = "self-update")]
use std::process::{Command, Stdio};
use std::{fs, ptr};

use anyhow::Context as _;

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Console::{FreeConsole, GetConsoleProcessList};
#[cfg(feature = "self-update")]
use windows_sys::Win32::System::Registry::RRF_RT_REG_SZ;
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_DWORD, RegDeleteTreeW, RegGetValueW, RegSetKeyValueW,
};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
use windows_sys::Win32::UI::WindowsAndMessaging::SM_CXSMICON;

#[cfg(feature = "self-update")]
use super::InstallKind;
use super::{APP_DISPLAY_NAME, APP_ID, Theme};
use crate::paths;

/// Registry key (under `HKCU`) with the taskbar theme settings.
const PERSONALIZE_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
/// Value of the personalize key that is 1 when the taskbar is light.
const LIGHT_THEME_VALUE: &str = "SystemUsesLightTheme";
/// Registry key (under `HKCU`) that holds the app's id registrations for toasts.
const AUMID_KEY_PREFIX: &str = r"Software\Classes\AppUserModelId";
/// Value with the name shown on the toast.
const DISPLAY_NAME_VALUE: &str = "DisplayName";
/// Value with the path of the icon shown on the toast.
const ICON_URI_VALUE: &str = "IconUri";
/// Registry key (under `HKCU`) in which the Inno Setup installer records the per-user
/// installation. The id is `AppId` of the installer script.
#[cfg(feature = "self-update")]
const INSTALLER_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{8EC7685F-05CE-4DE1-B33E-262D8A56D8C3}_is1";
#[cfg(feature = "self-update")]
const INSTALL_LOCATION_VALUE: &str = "InstallLocation";
#[cfg(feature = "self-update")]
pub const PORTABLE_SUFFIX: &str = ".exe";
#[cfg(feature = "self-update")]
pub const INSTALLER_SUFFIX: &str = "-setup.exe";
#[cfg(feature = "self-update")]
pub const TOAST_ACTIONS_SUPPORTED: bool = true;
#[cfg(feature = "self-update")]
const DETACHED_PROCESS: u32 = 0x0000_0008;
#[cfg(feature = "self-update")]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
#[cfg(feature = "self-update")]
const INSTALLER_ARGS: [&str; 4] = ["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/UPDATE"];
/// Tray icon size in pixels when the system metric is unavailable.
const FALLBACK_ICON_SIZE: u32 = 16;
/// Process count of a console that only this process uses.
const OWN_CONSOLE_PROCESS_COUNT: u32 = 1;

/// Frees the console when this process is its only user, so a tray started from Explorer or
/// autostart shows no window. A console shared with a shell is kept.
pub fn detach_console() {
    let mut processes = [0_u32; 1];
    // SAFETY: the pointer and length describe the live one-element array.
    let count = unsafe { GetConsoleProcessList(processes.as_mut_ptr(), 1) };
    if count != OWN_CONSOLE_PROCESS_COUNT {
        return;
    }
    // SAFETY: no arguments; the app does not use the console afterwards.
    if unsafe { FreeConsole() } == 0 {
        log::warn!("platform: cannot free the console");
    }
}

/// Reads `SystemUsesLightTheme` of the current user: 1 is light, anything else is dark.
pub fn taskbar_theme() -> Theme {
    let key = wide(PERSONALIZE_KEY);
    let value = wide(LIGHT_THEME_VALUE);
    let mut data = 0_u32;
    let mut size = u32::try_from(size_of::<u32>()).unwrap_or_default();
    // SAFETY: both names are NUL-terminated and `data` and `size` are valid for the call.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            ptr::null_mut(),
            ptr::from_mut(&mut data).cast(),
            &raw mut size,
        )
    };
    match status == ERROR_SUCCESS && data == 1 {
        true => Theme::Light,
        false => Theme::Dark,
    }
}

/// The small icon width for the system DPI.
pub fn tray_icon_size() -> u32 {
    // SAFETY: both functions take no pointers.
    let size = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem()) };
    u32::try_from(size)
        .ok()
        .filter(|size| *size > 0)
        .unwrap_or(FALLBACK_ICON_SIZE)
}

/// See [`super::register_notifications`].
pub fn register_notifications(icon_png: &[u8]) {
    if let Err(error) = register(icon_png) {
        log::warn!("platform: cannot register notifications: {error:#}");
    }
}

/// The fallible part of [`register_notifications`].
fn register(icon_png: &[u8]) -> anyhow::Result<()> {
    let icon = paths::notification_icon().context("no per-user data directory")?;
    if let Some(dir) = icon.parent() {
        fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }
    fs::write(&icon, icon_png).with_context(|| format!("cannot write {}", icon.display()))?;
    let key = wide(&format!(r"{AUMID_KEY_PREFIX}\{APP_ID}"));
    set_string(&key, DISPLAY_NAME_VALUE, APP_DISPLAY_NAME)?;
    set_string(&key, ICON_URI_VALUE, &path_text(&icon))
}

/// See [`super::announce_running`]. The handle is never closed: the mutex must live as long as
/// the process.
pub fn announce_running() {
    let name = wide(APP_ID);
    // SAFETY: the name is NUL-terminated; null security attributes select the defaults.
    let handle = unsafe { CreateMutexW(ptr::null(), 0, name.as_ptr()) };
    if handle.is_null() {
        log::warn!("platform: cannot create the instance mutex");
    }
}

/// See [`super::install_kind`]. The app counts as installed when it runs from the folder the
/// installer recorded.
#[cfg(feature = "self-update")]
pub fn install_kind() -> InstallKind {
    let location = registry_string(INSTALLER_KEY, INSTALL_LOCATION_VALUE);
    let exe = std::env::current_exe().ok();
    let dir = exe.as_deref().and_then(Path::parent);
    match (location, dir) {
        (Some(location), Some(dir)) if same_dir(&location, dir) => InstallKind::Installer,
        _ => InstallKind::Portable,
    }
}

#[cfg(feature = "self-update")]
pub fn launch_installer(setup: &Path, log: &Path) -> io::Result<()> {
    let mut command = Command::new(setup);
    command
        .args(INSTALLER_ARGS)
        .arg(format!("/LOG={}", log.display()));
    spawn_detached(&mut command)
}

#[cfg(feature = "self-update")]
pub fn relaunch() -> io::Result<()> {
    spawn_detached(&mut Command::new(std::env::current_exe()?))
}

/// See [`super::show_toast`]. Uses the toast API directly: notify-rust stops listening after
/// the first event, which is the banner timing out, and would lose later clicks in the
/// Action Center.
#[cfg(feature = "self-update")]
pub fn show_toast(
    title: &str,
    body: &str,
    actions: &[(&str, &str)],
    on_action: impl Fn(String) + Send + 'static,
) -> anyhow::Result<()> {
    let first = actions.first().map(|(id, _)| (*id).to_owned());
    let mut toast = tauri_winrt_notification::Toast::new(APP_ID)
        .title(title)
        .text1(body);
    for (id, label) in actions {
        toast = toast.add_button(label, id);
    }
    toast
        .on_activated(move |id| {
            // A click on the toast itself carries no id.
            if let Some(id) = id.or_else(|| first.clone()) {
                on_action(id);
            }
            Ok(())
        })
        .show()
        .context("cannot show the toast")
}

/// See [`super::unregister_notifications`].
pub fn unregister_notifications() -> anyhow::Result<()> {
    let key = wide(&format!(r"{AUMID_KEY_PREFIX}\{APP_ID}"));
    // SAFETY: the key is NUL-terminated.
    let status = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, key.as_ptr()) };
    match status {
        ERROR_SUCCESS | ERROR_FILE_NOT_FOUND => Ok(()),
        _ => anyhow::bail!("cannot delete the notification registration: error {status}"),
    }
}

/// Sets a string value under an `HKCU` key, creating the key when needed.
fn set_string(key: &[u16], name: &str, text: &str) -> anyhow::Result<()> {
    let name_wide = wide(name);
    let text_wide = wide(text);
    let bytes =
        u32::try_from(text_wide.len() * size_of::<u16>()).context("registry value too long")?;
    // SAFETY: key, name and text are NUL-terminated and `bytes` is the size of the text
    // including its terminator.
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name_wide.as_ptr(),
            REG_SZ,
            text_wide.as_ptr().cast(),
            bytes,
        )
    };
    match status == ERROR_SUCCESS {
        true => Ok(()),
        false => anyhow::bail!("cannot write registry value {name}: error {status}"),
    }
}

#[cfg(feature = "self-update")]
fn spawn_detached(command: &mut Command) -> io::Result<()> {
    command
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
}

#[cfg(feature = "self-update")]
fn registry_string(key: &str, name: &str) -> Option<String> {
    let key = wide(key);
    let name = wide(name);
    let mut bytes = 0_u32;
    // SAFETY: key and name are NUL-terminated; without a data pointer the call only reports
    // the size in `bytes`.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            ptr::null_mut(),
            ptr::null_mut(),
            &raw mut bytes,
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let mut buffer = vec![0_u16; usize::try_from(bytes).ok()?.div_ceil(size_of::<u16>())];
    bytes = u32::try_from(buffer.len() * size_of::<u16>()).ok()?;
    // SAFETY: as above, and `buffer` is valid for the `bytes` bytes the call may write.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &raw mut bytes,
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let text = String::from_utf16_lossy(&buffer);
    Some(text.trim_end_matches('\0').to_owned())
}

/// Whether the recorded folder is `dir`, ignoring case and a trailing separator.
#[cfg(feature = "self-update")]
fn same_dir(recorded: &str, dir: &Path) -> bool {
    let normal = |text: &str| text.trim_end_matches(['\\', '/']).to_lowercase();
    normal(recorded) == normal(&dir.to_string_lossy())
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// UTF-16 with a trailing NUL, as the wide Win32 functions expect.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
