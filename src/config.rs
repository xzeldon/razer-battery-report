use clap::ValueEnum;
use razer_battery_report::DeviceType;
use serde::{Deserialize, Serialize};

/// Application name used for configuration directory resolution.
const APP_NAME: &str = "razer-battery-report";

/// Log levels supported by the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            LogLevel::Off => "off",
            LogLevel::Error => "error",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Trace => "trace",
        };
        write!(f, "{}", s)
    }
}

/// Configuration structure holding user preferences.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct AppConfig {
    /// Whether desktop notifications are enabled.
    pub notifications_enabled: bool,

    /// Battery polling interval in seconds.
    pub polling_interval_secs: u64,

    /// Whether the application should start automatically on login.
    pub autostart_enabled: bool,

    /// Battery percentage threshold for low battery warning.
    pub low_battery_threshold: u8,

    /// Battery percentage threshold for critical battery warning.
    pub critical_battery_threshold: u8,

    /// The device manually selected by the user to be shown in the tray.
    pub preferred_device: Option<DeviceType>,

    /// The logging level (off, error, warn, info, debug, trace).
    pub log_level: LogLevel,
}

/// Default values for the configuration.
impl Default for AppConfig {
    fn default() -> Self {
        Self {
            notifications_enabled: true,
            polling_interval_secs: 60, // 1 minute
            autostart_enabled: false,
            low_battery_threshold: 15,
            critical_battery_threshold: 5,
            preferred_device: None,
            log_level: LogLevel::Info,
        }
    }
}

impl AppConfig {
    /// Loads the configuration from disk or creates a default one if not found.
    /// Uses the system's standard configuration directory (XDG on Linux, AppData on Windows).
    pub fn load() -> anyhow::Result<Self> {
        let cfg: AppConfig = confy::load(APP_NAME, "default")?;
        Ok(cfg)
    }

    /// Saves the current configuration to disk.
    #[allow(dead_code)]
    pub fn save(&self) -> anyhow::Result<()> {
        confy::store(APP_NAME, "default", self)?;
        Ok(())
    }
}
