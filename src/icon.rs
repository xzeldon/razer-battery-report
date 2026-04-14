#[cfg(not(target_os = "linux"))]
use anyhow::Context;

use razer_battery_report as librazer;
#[cfg(not(target_os = "linux"))]
use tray_icon::Icon;

/// Holds the pre-loaded icons to avoid reloading them from RAM every update.
#[derive(Clone)]
pub struct IconSet {
    #[cfg(not(target_os = "linux"))]
    pub white: Icon,
    #[cfg(not(target_os = "linux"))]
    pub yellow: Icon,
    #[cfg(not(target_os = "linux"))]
    pub red: Icon,
    // TODO: Charging icon?

    /// Raw PNG bytes for Linux ksni conversion (ARGB32).
    #[cfg(target_os = "linux")]
    pub white_png: &'static [u8],
    #[cfg(target_os = "linux")]
    pub yellow_png: &'static [u8],
    #[cfg(target_os = "linux")]
    pub red_png: &'static [u8],
}

impl IconSet {
    /// Loads all icons from the embedded binary assets.
    pub fn load() -> anyhow::Result<Self> {
        let white_png = include_bytes!("../assets/mouse_white.png");
        let yellow_png = include_bytes!("../assets/mouse_yellow.png");
        let red_png = include_bytes!("../assets/mouse_red.png");

        #[cfg(not(target_os = "linux"))]
        {
            Ok(Self {
                white: load_icon(white_png).context("Failed to load white icon")?,
                yellow: load_icon(yellow_png).context("Failed to load yellow icon")?,
                red: load_icon(red_png).context("Failed to load red icon")?,
            })
        }

        #[cfg(target_os = "linux")]
        {
            Ok(Self {
                white_png,
                yellow_png,
                red_png,
            })
        }
    }

    /// Select icon based on battery status (Windows/macOS only).
    #[cfg(not(target_os = "linux"))]
    pub fn get_icon(
        &self,
        status: &librazer::BatteryStatus,
        low_threshold: u8,
        critical_threshold: u8,
    ) -> &Icon {
        match status {
            // Charging always shows white for now
            // TODO: Charging icon?
            librazer::BatteryStatus::Charging(_) => &self.white,

            librazer::BatteryStatus::Level(level) => {
                let val = level.value();
                if val <= critical_threshold {
                    &self.red
                } else if val <= low_threshold {
                    &self.yellow
                } else {
                    &self.white
                }
            }

            librazer::BatteryStatus::Unknown => &self.white,
        }
    }

    /// Get raw PNG bytes for the icon matching the current status (Linux only).
    #[cfg(target_os = "linux")]
    pub fn get_icon_png(
        &self,
        status: &librazer::BatteryStatus,
        low_threshold: u8,
        critical_threshold: u8,
    ) -> &'static [u8] {
        match status {
            librazer::BatteryStatus::Charging(_) => self.white_png,
            librazer::BatteryStatus::Level(level) => {
                let val = level.value();
                if val <= critical_threshold {
                    self.red_png
                } else if val <= low_threshold {
                    self.yellow_png
                } else {
                    self.white_png
                }
            }
            librazer::BatteryStatus::Unknown => self.white_png,
        }
    }
}

/// Helper to parse PNG bytes into a Tray Icon (Windows/macOS only).
#[cfg(not(target_os = "linux"))]
fn load_icon(bytes: &[u8]) -> anyhow::Result<Icon> {
    let image = image::load_from_memory(bytes)
        .context("Failed to parse image bytes")?
        .into_rgba8();

    let (width, height) = image.dimensions();
    let rgba = image.into_raw();

    Icon::from_rgba(rgba, width, height).context("Failed to create tray icon")
}
