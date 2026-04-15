//! Unified system tray module.
//!
//! - Linux: Uses `ksni` crate (StatusNotifierItem spec)
//! - Windows/macOS: Uses `tray-icon` crate

#[cfg(target_os = "linux")]
mod linux;
#[cfg(not(target_os = "linux"))]
mod standard;

#[cfg(target_os = "linux")]
pub use linux::AppTray;
#[cfg(not(target_os = "linux"))]
pub use standard::AppTray;

/// Events originating from tray menu interactions.
#[derive(Debug)]
pub enum TrayEvent {
    SelectDevice(String),
    ToggleAutostart(bool),
    ToggleNotifications(bool),
    Restart,
    ShowAbout,
    Quit,
}
