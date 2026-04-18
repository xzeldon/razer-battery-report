use std::collections::HashMap;

use anyhow::Context;
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuItem, MenuEvent, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder,
};

use crate::icon::IconSet;
use crate::state::DeviceState;
use crate::tray::TrayEvent;

/// Windows/macOS tray implementation using tray-icon.
pub struct AppTray {
    tray_icon: Option<TrayIcon>,
    tray_menu: Menu,
    device_items: HashMap<String, CheckMenuItem>,
    autostart_item: CheckMenuItem,
    notifications_item: CheckMenuItem,
    restart_item: MenuItem,
    about_item: MenuItem,
    quit_item: MenuItem,
}

impl AppTray {
    pub fn new(autostart: bool, notifications: bool) -> anyhow::Result<Self> {
        let tray_menu = Menu::new();

        // Static menu skeleton (device items inserted at index 0 above the first separator)
        tray_menu.append(&PredefinedMenuItem::separator())?;
        let autostart_item = CheckMenuItem::new("Autostart", true, autostart, None);
        tray_menu.append(&autostart_item)?;
        let notifications_item =
            CheckMenuItem::new("Notifications", true, notifications, None);
        tray_menu.append(&notifications_item)?;
        tray_menu.append(&PredefinedMenuItem::separator())?;
        let restart_item = MenuItem::new("Restart", true, None);
        tray_menu.append(&restart_item)?;
        let about_item = MenuItem::new("About", true, None);
        tray_menu.append(&about_item)?;
        tray_menu.append(&PredefinedMenuItem::separator())?;
        let quit_item = MenuItem::new("Exit", true, None);
        tray_menu.append(&quit_item)?;

        Ok(Self {
            tray_icon: None,
            tray_menu,
            device_items: HashMap::new(),
            autostart_item,
            notifications_item,
            restart_item,
            about_item,
            quit_item,
        })
    }

    pub fn init(&mut self, icons: &IconSet) -> anyhow::Result<()> {
        if self.tray_icon.is_some() {
            return Ok(());
        }

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(self.tray_menu.clone()))
            .with_tooltip(format!("{}: Initializing...", APP_DISPLAY_NAME))
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
        self.sync_menu_items(devices);

        for (path, item) in &self.device_items {
            let is_active = path == active_device_path;
            if item.is_checked() != is_active {
                item.set_checked(is_active);
            }

            if let Some(device) = devices.iter().find(|d| d.path == *path) {
                item.set_text(format!("{} [{}]", device.device_type, device.status));
            }
        }

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

    pub fn set_autostart(&self, enabled: bool) {
        self.autostart_item.set_checked(enabled);
    }

    pub fn set_notifications(&self, enabled: bool) {
        self.notifications_item.set_checked(enabled);
    }

    /// Process a tray-icon MenuEvent and return the corresponding TrayEvent.
    pub fn process_menu_event(&self, event: &MenuEvent) -> Option<TrayEvent> {
        if event.id == self.quit_item.id() {
            return Some(TrayEvent::Quit);
        }
        if event.id == self.restart_item.id() {
            return Some(TrayEvent::Restart);
        }
        if event.id == self.about_item.id() {
            return Some(TrayEvent::ShowAbout);
        }
        if event.id == self.autostart_item.id() {
            return Some(TrayEvent::ToggleAutostart(self.autostart_item.is_checked()));
        }
        if event.id == self.notifications_item.id() {
            return Some(TrayEvent::ToggleNotifications(
                self.notifications_item.is_checked(),
            ));
        }

        for (path, item) in &self.device_items {
            if event.id == item.id() {
                return Some(TrayEvent::SelectDevice(path.clone()));
            }
        }

        None
    }

    fn sync_menu_items(&mut self, devices: &[DeviceState]) {
        // Remove items for devices that are gone
        self.device_items.retain(|path, item| {
            if !devices.iter().any(|d| d.path == *path) {
                let _ = self.tray_menu.remove(item);
                return false;
            }
            true
        });

        // Add new devices in sorted order (stable across calls)
        let mut sorted: Vec<_> = devices.iter().collect();
        sorted.sort_by(|a, b| a.path.cmp(&b.path));

        for device in sorted {
            if !self.device_items.contains_key(&device.path) {
                let item = CheckMenuItem::new(format!("{}", device.device_type), true, false, None);
                let _ = self.tray_menu.insert(&item, 0);
                self.device_items.insert(device.path.clone(), item);
            }
        }
    }
}
