use serde::{Deserialize, Serialize};

/// Application name used for configuration directory resolution.
const APP_NAME: &str = "razer-battery-report";

/// Configuration structure holding user preferences.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    /// Whether desktop notifications are enabled.
    pub notifications_enabled: bool,

    /// Battery polling interval in seconds.
    pub polling_interval_secs: u64,

    /// Whether the application should start automatically on login.
    pub autostart_enabled: bool,

    /// Battery percentage threshold for low battery warning.
    pub low_battery_threshold: u8,
}

/// Default values for the configuration.
impl Default for AppConfig {
    fn default() -> Self {
        Self {
            notifications_enabled: true,
            polling_interval_secs: 60, // 1 minute
            autostart_enabled: false,
            low_battery_threshold: 15,
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
    pub fn save(&self) -> anyhow::Result<()> {
        confy::store(APP_NAME, "default", self)?;
        Ok(())
    }
}
