//! The per-user directories of the app, resolved through the OS's standard locations.

use std::path::PathBuf;

use directories::BaseDirs;

/// Name of the app's directory inside the per-user config and data directories.
const APP_NAME: &str = "razer-battery-report";
/// File name of the config inside the config directory.
const CONFIG_FILE: &str = "config.toml";
/// Name of the log directory inside the local data directory.
const LOG_DIR: &str = "logs";
/// File name of the lock that keeps a second tray from starting.
const LOCK_FILE: &str = "tray.lock";
/// File name of the toast icon.
const NOTIFICATION_ICON_FILE: &str = "notification.png";
/// File name of the licence notices of this build.
const LICENSES_FILE: &str = "THIRD-PARTY.md";

/// Location of the config file, or `None` when the OS reports no home directory.
pub fn config_file() -> Option<PathBuf> {
    BaseDirs::new().map(|dirs| dirs.config_dir().join(APP_NAME).join(CONFIG_FILE))
}

/// Per-user local state of the app (logs, lock file, notification icon, licence notices), or
/// `None` when the OS reports no home directory.
pub fn data_dir() -> Option<PathBuf> {
    BaseDirs::new().map(|dirs| dirs.data_local_dir().join(APP_NAME))
}

/// Directory that holds the log files, or `None` when the OS reports no home directory.
pub fn log_dir() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join(LOG_DIR))
}

/// The lock file of the running tray, or `None` when the OS reports no home directory.
pub fn lock_file() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join(LOCK_FILE))
}

/// The toast icon, or `None` when the OS reports no home directory.
pub fn notification_icon() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join(NOTIFICATION_ICON_FILE))
}

/// The licence notices written for the user, or `None` when the OS reports no home directory.
pub fn licenses_file() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join(LICENSES_FILE))
}
