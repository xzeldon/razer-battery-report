use anyhow::Context;

use razer_battery_report as librazer;
use tray_icon::Icon;

/// Holds the pre-loaded icons to avoid reloading them from RAM every update.
pub struct IconSet {
    pub white: Icon,
    pub yellow: Icon,
    pub red: Icon,
    // TODO: Charging icon?
}

impl IconSet {
    /// Loads all icons from the embedded binary assets.
    pub fn load() -> anyhow::Result<Self> {
        Ok(Self {
            white: load_icon(include_bytes!("../assets/mouse_white.png"))
                .context("Failed to load white icon")?,
            yellow: load_icon(include_bytes!("../assets/mouse_yellow.png"))
                .context("Failed to load yellow icon")?,
            red: load_icon(include_bytes!("../assets/mouse_red.png"))
                .context("Failed to load red icon")?,
        })
    }

    /// Select icon based on battery status
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
                if *level <= critical_threshold {
                    &self.red
                } else if *level <= low_threshold {
                    &self.yellow
                } else {
                    &self.white
                }
            }

            librazer::BatteryStatus::Unknown => &self.white,
        }
    }
}

/// Helper to parse PNG bytes into a Tray Icon.
fn load_icon(bytes: &[u8]) -> anyhow::Result<Icon> {
    let image = image::load_from_memory(bytes)
        .context("Failed to parse image bytes")?
        .into_rgba8();

    let (width, height) = image.dimensions();
    let rgba = image.into_raw();

    Icon::from_rgba(rgba, width, height).context("Failed to create tray icon")
}
