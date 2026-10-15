//! The Windows implementation of the platform API.

use std::path::Path;
use std::{fs, ptr};

use anyhow::Context as _;

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Console::{FreeConsole, GetConsoleProcessList};
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_DWORD, RegGetValueW, RegSetKeyValueW,
};
use windows_sys::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
use windows_sys::Win32::UI::WindowsAndMessaging::SM_CXSMICON;

use super::{APP_DISPLAY_NAME, AUMID, Theme};
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
/// File name of the toast icon inside the data directory.
const NOTIFICATION_ICON_FILE: &str = "notification.png";
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
    let dir = paths::data_dir().context("no per-user data directory")?;
    fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let icon = dir.join(NOTIFICATION_ICON_FILE);
    fs::write(&icon, icon_png).with_context(|| format!("cannot write {}", icon.display()))?;
    let key = wide(&format!(r"{AUMID_KEY_PREFIX}\{AUMID}"));
    set_string(&key, DISPLAY_NAME_VALUE, APP_DISPLAY_NAME)?;
    set_string(&key, ICON_URI_VALUE, &path_text(&icon))
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

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// UTF-16 with a trailing NUL, as the wide Win32 functions expect.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
