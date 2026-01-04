use crate::icon::IconSet;
use anyhow::Context;
use log::warn;
use razer_battery_report as librazer;
use tray_icon::{
    menu::{Menu, MenuItem},
    TrayIcon, TrayIconBuilder,
};

/// Handles creation of the tray icon, the menu, and updates to the icon/tooltip.
pub struct AppTray {
    tray_icon: Option<TrayIcon>,
    pub quit_item: MenuItem,
}

impl AppTray {
    /// Creates the tray icon and menu.
    pub fn new(icons: &IconSet) -> anyhow::Result<Self> {
        let tray_menu = Menu::new();
        let quit_item = MenuItem::new("Exit", true, None);
        // TODO: Add Settings, etc. in future steps
        tray_menu.append(&quit_item)?;

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu))
            .with_tooltip("Razer Battery Report: Initializing...")
            .with_icon(icons.white.clone())
            .build()
            .context("Failed to build tray icon")?;

        Ok(Self {
            tray_icon: Some(tray_icon),
            quit_item,
        })
    }

    /// Updates the tray icon and tooltip based on the primary device state.
    pub fn update(
        &mut self,
        device_type: &librazer::DeviceType,
        status: &librazer::BatteryStatus,
        icons: &IconSet,
        low_threshold: u8,
        critical_threshold: u8,
    ) {
        if let Some(tray) = self.tray_icon.as_mut() {
            let new_icon = icons.get_icon(status, low_threshold, critical_threshold);

            if let Err(e) = tray.set_icon(Some(new_icon.clone())) {
                warn!("Failed to update tray icon: {}", e);
            }

            let tooltip = format!("{}: {}", device_type, status);
            if let Err(e) = tray.set_tooltip(Some(tooltip)) {
                warn!("Failed to update tooltip: {}", e);
            }
        }
    }
}
