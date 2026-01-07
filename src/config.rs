use clap::ValueEnum;
use razer_battery_report::DeviceType;
use serde::{Deserialize, Serialize};

/// Application name used for configuration directory resolution.
const APP_NAME: &str = "razer-battery-report";

// Defaults
const DEFAULT_NOTIFICATIONS: bool = true;
const DEFAULT_AUTOSTART: bool = false;

const DEFAULT_POLLING_INTERVAL_SECS: u64 = 60;
const MIN_POLLING_INTERVAL_SECS: u64 = 30;

const DEFAULT_LOW_BATTERY_THRESHOLD: u8 = 15;
const DEFAULT_CRITICAL_BATTERY_THRESHOLD: u8 = 5;
const MAX_BATTERY_PERCENTAGE: u8 = 100;

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
        write!(f, "{}", format!("{:?}", self).to_lowercase())
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
            notifications_enabled: DEFAULT_NOTIFICATIONS,
            polling_interval_secs: DEFAULT_POLLING_INTERVAL_SECS,
            autostart_enabled: DEFAULT_AUTOSTART,
            low_battery_threshold: DEFAULT_LOW_BATTERY_THRESHOLD,
            critical_battery_threshold: DEFAULT_CRITICAL_BATTERY_THRESHOLD,
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
    pub fn save(&self) -> anyhow::Result<()> {
        confy::store(APP_NAME, "default", self)?;
        Ok(())
    }

    /// Validates and corrects configuration values.
    pub fn sanitize(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();

        // Validate Polling Interval to prevent busy loops.
        // If the user sets polling interval to 0, it causes 100% CPU usage or HID spam.
        self.polling_interval_secs.ensure_min(
            MIN_POLLING_INTERVAL_SECS,
            "polling_interval_secs",
            &mut warnings,
        );

        // Validate Battery Thresholds (Clamp to 100%)
        self.low_battery_threshold.ensure_max(
            MAX_BATTERY_PERCENTAGE,
            "low_battery_threshold",
            &mut warnings,
        );
        self.critical_battery_threshold.ensure_max(
            MAX_BATTERY_PERCENTAGE,
            "critical_battery_threshold",
            &mut warnings,
        );

        // Validate Logic (Critical must be <= Low)
        if self.critical_battery_threshold > self.low_battery_threshold {
            warnings.push(format!(
                    "Config warning: 'critical_battery_threshold' ({}%) is higher than 'low_battery_threshold' ({}%). Swapping values.",
                    self.critical_battery_threshold, self.low_battery_threshold
                ));
            std::mem::swap(
                &mut self.critical_battery_threshold,
                &mut self.low_battery_threshold,
            );
        }

        warnings
    }
}

/// A helper trait to add validation methods to primitive types.
trait ValidateProperty<T> {
    fn ensure_min(&mut self, min: T, name: &str, warnings: &mut Vec<String>);
    fn ensure_max(&mut self, max: T, name: &str, warnings: &mut Vec<String>);
}

/// Validation for any type that can be compared, copied, and printed.
impl<T> ValidateProperty<T> for T
where
    T: PartialOrd + Copy + std::fmt::Display,
{
    fn ensure_min(&mut self, min: T, name: &str, warnings: &mut Vec<String>) {
        if *self < min {
            warnings.push(format!(
                "Config warning: '{}' ({}) is too low. Resetting to safe minimum ({}).",
                name, self, min
            ));
            *self = min;
        }
    }

    fn ensure_max(&mut self, max: T, name: &str, warnings: &mut Vec<String>) {
        if *self > max {
            warnings.push(format!(
                "Config warning: '{}' ({}) is too high. Clamping to maximum ({}).",
                name, self, max
            ));
            *self = max;
        }
    }
}
