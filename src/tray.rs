use std::collections::HashMap;

use crate::icon::IconSet;
use anyhow::Context;
use log::warn;
use razer_battery_report as librazer;
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder,
};

/// Handles creation of the tray icon, the menu, and updates to the icon/tooltip.
pub struct AppTray {
    tray_icon: Option<TrayIcon>,
    tray_menu: Menu,
    device_items: HashMap<librazer::DeviceType, CheckMenuItem>,
    pub quit_item: MenuItem,
}

impl AppTray {
    /// Creates the tray icon and menu.
    pub fn new(icons: &IconSet) -> anyhow::Result<Self> {
        let tray_menu = Menu::new();
        tray_menu.append(&PredefinedMenuItem::separator())?;
        let quit_item = MenuItem::new("Exit", true, None);
        // TODO: Add Settings, etc.
        tray_menu.append(&quit_item)?;

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu.clone()))
            .with_tooltip("Razer Battery Report: Initializing...")
            .with_icon(icons.white.clone())
            .build()
            .context("Failed to build tray icon")?;

        Ok(Self {
            tray_icon: Some(tray_icon),
            tray_menu,
            device_items: HashMap::new(),
            quit_item,
        })
    }

    /// Updates the tray icon, tooltip, and menu based on the device list and active selection.
    pub fn update(
        &mut self,
        devices_status: &HashMap<librazer::DeviceType, librazer::BatteryStatus>,
        active_device: &librazer::DeviceType,
        icons: &IconSet,
        low_threshold: u8,
        critical_threshold: u8,
    ) {
        self.sync_menu_items(devices_status);

        // Update radio buttons in menu and tooltip text
        for (device_type, item) in &self.device_items {
            let is_active = device_type == active_device;
            // Ensure only the active item is checked
            if item.is_checked() != is_active {
                item.set_checked(is_active);
            }

            // Update text to include current percentage
            if let Some(status) = devices_status.get(device_type) {
                item.set_text(format!("{}  [{}]", device_type, status));
            }
        }

        if let Some(tray) = self.tray_icon.as_mut() {
            if let Some(status) = devices_status.get(active_device) {
                let new_icon = icons.get_icon(status, low_threshold, critical_threshold);
                if let Err(e) = tray.set_icon(Some(new_icon.clone())) {
                    warn!("Failed to update tray icon: {}", e);
                }

                let tooltip = format!("{}: {}", active_device, status);
                if let Err(e) = tray.set_tooltip(Some(tooltip)) {
                    warn!("Failed to update tooltip: {}", e);
                }
            }
        }
    }

    /// Helper to add/remove menu items dynamically
    fn sync_menu_items(
        &mut self,
        devices: &HashMap<librazer::DeviceType, librazer::BatteryStatus>,
    ) {
        // Remove items for devices that are gone
        self.device_items.retain(|device_type, item| {
            if !devices.contains_key(device_type) {
                // Remove from the UI
                if let Err(e) = self.tray_menu.remove(item) {
                    warn!("Failed to remove menu item: {}", e);
                }
                // Remove from HashMap
                return false;
            }
            true
        });

        // Add items for new devices
        for device_type in devices.keys() {
            if !self.device_items.contains_key(device_type) {
                let item = CheckMenuItem::new(format!("{}", device_type), true, false, None);

                if let Err(e) = self.tray_menu.prepend(&item) {
                    warn!("Failed to add menu item: {}", e);
                } else {
                    self.device_items.insert(*device_type, item);
                }
            }
        }
    }

    /// Checks if a menu event corresponds to one of the device selection items.
    pub fn handle_menu_click(&self, event_id: &str) -> Option<librazer::DeviceType> {
        for (device_type, item) in &self.device_items {
            if item.id() == event_id {
                return Some(*device_type);
            }
        }
        None
    }
}
