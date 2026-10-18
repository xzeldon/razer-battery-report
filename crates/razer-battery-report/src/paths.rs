//! The per-user directories of the app, resolved through the OS's standard locations.

use std::path::PathBuf;

use directories::BaseDirs;

/// Name of the app's directory inside the per-user config and data directories.
const APP_NAME: &str = "razer-battery-report";
/// File name of the config inside the config directory.
const CONFIG_FILE: &str = "config.toml";
/// Name of the log directory inside the local data directory.
const LOG_DIR: &str = "logs";

/// Location of the config file, or `None` when the OS reports no home directory.
pub fn config_file() -> Option<PathBuf> {
    BaseDirs::new().map(|dirs| dirs.config_dir().join(APP_NAME).join(CONFIG_FILE))
}

/// Per-user local state of the app (logs, lock file, notification icon, licence notices), or
/// `None` when the OS
/// reports no home directory.
pub fn data_dir() -> Option<PathBuf> {
    BaseDirs::new().map(|dirs| dirs.data_local_dir().join(APP_NAME))
}

/// Directory that holds the log files, or `None` when the OS reports no home directory.
pub fn log_dir() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join(LOG_DIR))
}
