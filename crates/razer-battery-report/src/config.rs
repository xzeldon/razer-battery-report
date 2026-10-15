//! The user's settings, read from the config file.

use std::fs;
use std::io::ErrorKind;

use anyhow::Context as _;
use serde::Deserialize;

use crate::paths;

/// Threshold used when the file has none or an unsupported one.
const DEFAULT_LOW_THRESHOLD: u8 = 15;
/// The thresholds the tray menu offers.
const ALLOWED_LOW_THRESHOLDS: [u8; 4] = [10, 15, 20, 25];

/// Settings of the app. Missing keys take their defaults and unknown keys are ignored.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Whether battery events are shown as desktop notifications.
    pub notifications: bool,
    /// Battery percentage at or below which the "low battery" event fires.
    pub low_threshold: u8,
    /// Whether the app starts with the user's session.
    pub autostart: bool,
    /// Serial of the mouse shown in the tray.
    pub selected: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            notifications: true,
            low_threshold: DEFAULT_LOW_THRESHOLD,
            autostart: false,
            selected: None,
        }
    }
}

impl Config {
    /// Reads the config file. A missing file gives the defaults silently; an unreadable or
    /// corrupt one gives the defaults and a warning in the log.
    pub fn load() -> Self {
        let Some(path) = paths::config_file() else {
            log::warn!("config: no per-user config directory, using defaults");
            return Self::default();
        };
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => return Self::default(),
            Err(error) => {
                log::warn!("config: cannot read {}: {error}", path.display());
                return Self::default();
            }
        };
        Self::parse(&text).unwrap_or_else(|error| {
            log::warn!("config: {}: {error:#}", path.display());
            Self::default()
        })
    }

    /// Parses the text of a config file. An unsupported threshold is replaced by the default.
    fn parse(text: &str) -> anyhow::Result<Self> {
        let mut config: Self = toml::from_str(text).context("cannot parse config")?;
        if !ALLOWED_LOW_THRESHOLDS.contains(&config.low_threshold) {
            log::warn!(
                "config: low_threshold {} is not one of {ALLOWED_LOW_THRESHOLDS:?}, using {DEFAULT_LOW_THRESHOLD}",
                config.low_threshold
            );
            config.low_threshold = DEFAULT_LOW_THRESHOLD;
        }
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_file_keeps_other_defaults() {
        let config = Config::parse("autostart = true\n").unwrap();
        assert_eq!(
            config,
            Config {
                autostart: true,
                ..Config::default()
            }
        );
    }

    #[test]
    fn invalid_threshold_becomes_default() {
        assert_eq!(
            Config::parse("low_threshold = 7\n").unwrap().low_threshold,
            15
        );
        assert_eq!(
            Config::parse("low_threshold = 200\n")
                .unwrap()
                .low_threshold,
            15
        );
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(Config::parse("this is = = not toml").is_err());
        assert!(Config::parse("low_threshold = \"high\"").is_err());
    }
}
