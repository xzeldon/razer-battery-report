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

    /// Pre-parsed ksni::Icon (ARGB32) for Linux.
    #[cfg(target_os = "linux")]
    pub white: ksni::Icon,
    #[cfg(target_os = "linux")]
    pub yellow: ksni::Icon,
    #[cfg(target_os = "linux")]
    pub red: ksni::Icon,
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
                white: parse_to_ksni(white_png).context("Failed to parse white icon")?,
                yellow: parse_to_ksni(yellow_png).context("Failed to parse yellow icon")?,
                red: parse_to_ksni(red_png).context("Failed to parse red icon")?,
            })
        }
    }

    /// Select icon based on battery status (Linux).
    #[cfg(target_os = "linux")]
    pub fn get_icon(
        &self,
        status: &librazer::BatteryStatus,
        low_threshold: u8,
        critical_threshold: u8,
    ) -> &ksni::Icon {
        match status {
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

    /// Select icon based on battery status (Windows/macOS).
    #[cfg(not(target_os = "linux"))]
    pub fn get_icon(
        &self,
        status: &librazer::BatteryStatus,
        low_threshold: u8,
        critical_threshold: u8,
    ) -> &Icon {
        match status {
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
}

/// Converts PNG bytes to `ksni::Icon` (ARGB32 format).
#[cfg(target_os = "linux")]
fn parse_to_ksni(png_bytes: &[u8]) -> anyhow::Result<ksni::Icon> {
    let img = image::load_from_memory(png_bytes)?.into_rgba8();
    let (width, height) = img.dimensions();
    let mut data = img.into_vec();

    // Convert RGBA to ARGB32 (network byte order)
    for pixel in data.chunks_exact_mut(4) {
        pixel.rotate_right(1); // R,G,B,A -> A,R,G,B
    }

    Ok(ksni::Icon {
        width: width as i32,
        height: height as i32,
        data,
    })
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
