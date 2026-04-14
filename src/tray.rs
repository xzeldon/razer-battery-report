use std::collections::HashMap;

use crate::icon::IconSet;
use razer_battery_report as librazer;

#[cfg(not(target_os = "linux"))]
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder,
};

#[cfg(target_os = "linux")]
use crate::platform_tray::PlatformTray;

/// Handles creation of the tray icon, the menu, and updates to the icon/tooltip.
pub struct AppTray {
    #[cfg(not(target_os = "linux"))]
    tray_icon: Option<TrayIcon>,
    #[cfg(not(target_os = "linux"))]
    tray_menu: Menu,
    #[cfg(not(target_os = "linux"))]
    device_items: HashMap<String, CheckMenuItem>,
    #[cfg(not(target_os = "linux"))]
    quit_item: MenuItem,

    #[cfg(target_os = "linux")]
    platform_tray: PlatformTray,
}

impl AppTray {
    pub fn new(autostart_enabled: bool, notifications_enabled: bool) -> anyhow::Result<Self> {
        #[cfg(not(target_os = "linux"))]
        {
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

        #[cfg(target_os = "linux")]
        {
            let platform_tray = PlatformTray::new(autostart_enabled, notifications_enabled)?;
            Ok(Self { platform_tray })
        }
    }

    /// Initializes the actual system tray icon.
    /// Must be called after the Event Loop has started.
    pub fn init_tray_icon(&mut self, icons: &IconSet) -> anyhow::Result<()> {
        #[cfg(not(target_os = "linux"))]
        {
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

        #[cfg(target_os = "linux")]
        {
            self.platform_tray.init(icons)
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub fn is_quit_event(&self, event_id: &str) -> bool {
        self.quit_item.id() == event_id
    }

    pub fn set_no_devices_state(&mut self, icons: &IconSet) {
        #[cfg(not(target_os = "linux"))]
        {
            if let Some(tray) = self.tray_icon.as_mut() {
                let _ = tray.set_tooltip(Some("No Razer devices connected".to_string()));
                let _ = tray.set_icon(Some(icons.white.clone()));
            }
        }

        #[cfg(target_os = "linux")]
        {
            self.platform_tray.set_no_devices_state(icons);
        }
    }

    /// Updates the tray icon, tooltip, and menu based on the device list.
    pub fn update(
        &mut self,
        devices_status: &HashMap<String, (librazer::DeviceType, librazer::BatteryStatus)>,
        active_device_path: &str,
        icons: &IconSet,
        low_threshold: u8,
        critical_threshold: u8,
    ) {
        #[cfg(not(target_os = "linux"))]
        {
            self.sync_menu_items(devices_status);

            // Update radio buttons
            for (path, item) in &self.device_items {
                let is_active = path == active_device_path;
                if item.is_checked() != is_active {
                    item.set_checked(is_active);
                }

                if let Some((dtype, status)) = devices_status.get(path) {
                    item.set_text(format!("{}  [{}]", dtype, status));
                }
            }

            // Update Icon and Tooltip
            if let Some(tray) = self.tray_icon.as_mut() {
                if let Some((dtype, status)) = devices_status.get(active_device_path) {
                    let new_icon = icons.get_icon(status, low_threshold, critical_threshold);

                    if let Err(e) = tray.set_icon(Some(new_icon.clone())) {
                        warn!("Failed to update tray icon: {}", e);
                    }

                    let tooltip = format!("{}: {}", dtype, status);
                    if let Err(e) = tray.set_tooltip(Some(tooltip)) {
                        warn!("Failed to update tooltip: {}", e);
                    }
                }
            }
        }

        #[cfg(target_os = "linux")]
        {
            self.platform_tray.update(
                devices_status,
                active_device_path,
                icons,
                low_threshold,
                critical_threshold,
            );
        }
    }

    /// Helper to add/remove menu items dynamically (Windows/macOS only).
    #[cfg(not(target_os = "linux"))]
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

    /// Checks if a menu event corresponds to one of the device selection items (Windows/macOS only).
    #[cfg(not(target_os = "linux"))]
    pub fn handle_menu_click(&self, event_id: &str) -> Option<String> {
        for (path, item) in &self.device_items {
            if item.id() == event_id {
                return Some(path.clone());
            }
        }
        None
    }

    /// Spawns the ksni command receiver thread (Linux only).
    #[cfg(target_os = "linux")]
    pub fn spawn_command_receiver(
        &mut self,
        proxy: tao::event_loop::EventLoopProxy<crate::AppEvent>,
    ) {
        self.platform_tray.spawn_command_receiver(proxy);
    }

    /// Updates the autostart setting in the tray state (Linux only).
    #[cfg(target_os = "linux")]
    pub fn set_autostart(&self, enabled: bool) {
        self.platform_tray.set_autostart(enabled);
    }

    /// Updates the notifications setting in the tray state (Linux only).
    #[cfg(target_os = "linux")]
    pub fn set_notifications(&self, enabled: bool) {
        self.platform_tray.set_notifications(enabled);
    }
}
