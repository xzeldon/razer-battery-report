//! Defaults for the OSes without a dedicated implementation yet.

use super::Theme;

/// Edge length of the tray icon in pixels.
const TRAY_ICON_SIZE: u32 = 32;

/// Nothing to do: only Windows attaches a console to a tray app.
pub fn detach_console() {}

pub fn taskbar_theme() -> Theme {
    Theme::Dark
}

pub fn tray_icon_size() -> u32 {
    TRAY_ICON_SIZE
}

/// Nothing to register: notifications carry the app name directly.
pub fn register_notifications(_icon_png: &[u8]) {}
