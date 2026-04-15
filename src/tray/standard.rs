use std::collections::HashMap;

use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    Icon, TrayIcon, TrayIconBuilder,
};

use razer_battery_report as librazer;

use crate::icon::IconSet;
use crate::state::DeviceState;
#[allow(unused_imports)]
use crate::tray::TrayEvent;

/// Windows/macOS tray implementation using tray-icon.
pub struct AppTray {
    tray_icon: Option<TrayIcon>,
    tray_menu: Menu,
    device_items: HashMap<String, CheckMenuItem>,
    quit_item: MenuItem,
}

impl AppTray {
    pub fn new(_autostart: bool, _notifications: bool) -> anyhow::Result<Self> {
        let tray_menu = Menu::new();
        tray_menu.append(&PredefinedMenuItem::separator())?;

        let quit_item = MenuItem::new("Exit", true, None);
        tray_menu.append(&quit_item)?;

        Ok(Self {
            tray_icon: None,
            tray_menu,
            device_items: HashMap::new(),
            quit_item,
        })
    }

    pub fn init(&mut self, icons: &IconSet) -> anyhow::Result<()> {
        if self.tray_icon.is_some() {
            return Ok(());
        }

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(self.tray_menu.clone()))
            .with_tooltip("Razer Battery Report: Initializing...")
            .with_icon(icons.white.clone())
            .build()
            .context("Failed to build tray icon")?;

        self.tray_icon = Some(tray_icon);
        Ok(())
    }

    pub fn update(
        &mut self,
        devices: &[DeviceState],
        active_device_path: &str,
        icons: &IconSet,
        low_threshold: u8,
        critical_threshold: u8,
    ) {
        let device_map: HashMap<_, _> = devices
            .iter()
            .map(|d| (d.path.clone(), (d.device_type, d.status)))
            .collect();

        self.sync_menu_items(&device_map);

        // Update radio buttons
        for (path, item) in &self.device_items {
            let is_active = path == active_device_path;
            if item.is_checked() != is_active {
                item.set_checked(is_active);
            }

            if let Some((dtype, status)) = device_map.get(path) {
                item.set_text(format!("{}  [{}]", dtype, status));
            }
        }

        // Update icon and tooltip
        if let Some(tray) = self.tray_icon.as_mut() {
            if let Some(device) = devices.iter().find(|d| d.path == active_device_path) {
                let new_icon = icons.get_icon(&device.status, low_threshold, critical_threshold);

                if let Err(e) = tray.set_icon(Some(new_icon.clone())) {
                    log::warn!("Failed to update tray icon: {}", e);
                }

                let tooltip = format!("{}: {}", device.device_type, device.status);
                if let Err(e) = tray.set_tooltip(Some(tooltip)) {
                    log::warn!("Failed to update tooltip: {}", e);
                }
            }
        }
    }

    pub fn set_no_devices_state(&mut self, icons: &IconSet) {
        if let Some(tray) = self.tray_icon.as_mut() {
            let _ = tray.set_tooltip(Some("No Razer devices connected".to_string()));
            let _ = tray.set_icon(Some(icons.white.clone()));
        }
    }

    /// No-op on this platform -- event forwarding is handled by the global MenuEvent channel.
    pub fn spawn_command_receiver(
        &mut self,
        _proxy: tao::event_loop::EventLoopProxy<crate::AppEvent>,
    ) {
    }

    #[allow(unused)]
    pub fn set_autostart(&self, _enabled: bool) {}

    #[allow(unused)]
    pub fn set_notifications(&self, _enabled: bool) {}

    pub fn is_quit_event(&self, event_id: &str) -> bool {
        self.quit_item.id() == event_id
    }

    pub fn handle_menu_click(&self, event_id: &str) -> Option<String> {
        for (path, item) in &self.device_items {
            if item.id() == event_id {
                return Some(path.clone());
            }
        }
        None
    }

    fn sync_menu_items(
        &mut self,
        devices: &HashMap<String, (librazer::DeviceType, librazer::BatteryStatus)>,
    ) {
        // Remove items for devices that are gone
        self.device_items.retain(|path, item| {
            if !devices.contains_key(path) {
                let _ = self.tray_menu.remove(item);
                return false;
            }
            true
        });

        // Add items for new devices
        for (path, (dtype, _)) in devices {
            if !self.device_items.contains_key(path) {
                let item = CheckMenuItem::new(format!("{}", dtype), true, false, None);
                let _ = self.tray_menu.prepend(&item);
                self.device_items.insert(path.clone(), item);
            }
        }
    }
}
