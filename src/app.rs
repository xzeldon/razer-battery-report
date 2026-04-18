use std::env;
use std::sync::mpsc::Sender;

use auto_launch::AutoLaunchBuilder;
use log::{debug, error, info};
use razer_battery_report as librazer;

use crate::config::{APP_NAME, AppConfig};
use crate::icon::IconSet;
use crate::state::{DeviceState, DeviceStateManager};
use crate::tray::{AppTray, TrayEvent};
use crate::worker::WorkerCommand;

/// The main application controller.
pub struct RazerApp {
    config: AppConfig,
    icons: IconSet,
    tray: AppTray,
    state_manager: DeviceStateManager,
    active_device_path: Option<String>,
    last_known_status: Vec<DeviceState>,
    worker_tx: Sender<WorkerCommand>,
}

/// Configures OS-level autostart for the current executable.
fn set_autostart(enabled: bool) -> anyhow::Result<()> {
    let app_path = env::current_exe()?.to_string_lossy().into_owned();
    let auto = AutoLaunchBuilder::new()
        .set_app_name(APP_NAME)
        .set_app_path(&app_path)
        .set_use_launch_agent(true)
        .build()?;

    if enabled {
        auto.enable()?;
    } else {
        auto.disable()?;
    }
    Ok(())
}

impl RazerApp {
    pub fn new(
        config: AppConfig,
        icons: IconSet,
        worker_tx: Sender<WorkerCommand>,
    ) -> anyhow::Result<Self> {
        // Init only tray menus, but not the icon yet (requires EventLoop).
        let tray = AppTray::new(config.autostart_enabled, config.notifications_enabled)?;

        Ok(Self {
            config,
            icons,
            tray,
            state_manager: DeviceStateManager::new(),
            active_device_path: None,
            last_known_status: Vec::new(),
            worker_tx,
        })
    }

    /// Called when the Event Loop receives StartCause::Init.
    pub fn on_app_init(&mut self) {
        debug!("Initializing UI elements...");
        if let Err(e) = self.tray.init(&self.icons) {
            error!("Failed to initialize tray icon: {}", e);
        } else {
            info!("Tray initialized successfully.");
        }

        if self.config.autostart_enabled {
            if let Err(e) = set_autostart(true) {
                error!("Failed to configure OS autostart on startup: {}", e);
            }
        }
    }

    /// Handles battery updates coming from the worker thread.
    pub fn on_battery_update(
        &mut self,
        data: Vec<(String, librazer::DeviceType, librazer::BatteryStatus)>,
    ) {
        self.state_manager.process_update(&data, &self.config);
        self.last_known_status = data
            .into_iter()
            .map(|(path, device_type, status)| DeviceState {
                path,
                device_type,
                status,
            })
            .collect();
        self.ensure_active_device_validity();
        self.update_tray_view();
    }

    /// Handles clicks on the tray menu (Windows/macOS only).
    /// Returns `true` if the application should exit.
    #[cfg(not(target_os = "linux"))]
    pub fn on_menu_event(&mut self, event: tray_icon::menu::MenuEvent) -> bool {
        if let Some(tray_event) = self.tray.process_menu_event(&event) {
            return self.on_tray_event(tray_event);
        }
        false
    }

    /// Handles tray events from all platforms.
    /// Returns `true` if the application should exit.
    pub fn on_tray_event(&mut self, event: TrayEvent) -> bool {
        match event {
            TrayEvent::SelectDevice(path) => {
                self.set_active_device(path);
            }
            TrayEvent::ToggleAutostart(enabled) => {
                info!("Autostart toggled: {}", enabled);
                self.config.autostart_enabled = enabled;
                if let Err(e) = self.config.save() {
                    error!("Failed to save config: {}", e);
                }
                self.tray.set_autostart(enabled);
                if let Err(e) = set_autostart(enabled) {
                    error!("Failed to configure OS autostart: {}", e);
                }
            }
            TrayEvent::ToggleNotifications(enabled) => {
                info!("Notifications toggled: {}", enabled);
                self.config.notifications_enabled = enabled;
                if let Err(e) = self.config.save() {
                    error!("Failed to save config: {}", e);
                }
                self.tray.set_notifications(enabled);
            }
            TrayEvent::Restart => {
                info!("Restarting application...");
                if let Ok(exe) = env::current_exe() {
                    let _ = std::process::Command::new(exe).spawn();
                }
                let _ = self.worker_tx.send(WorkerCommand::Quit);
                return true;
            }
            TrayEvent::ShowAbout => {
                let url = "https://github.com/xzeldon/razer-battery-report";
                if let Err(e) = webbrowser::open(url) {
                    error!("Failed to open browser: {}", e);
                }
            }
            TrayEvent::Quit => {
                info!("Exit requested via tray menu.");
                let _ = self.worker_tx.send(WorkerCommand::Quit);
                return true;
            }
        }

        false
    }

    /// Called when the OS requests the app to close.
    pub fn on_shutdown(&self) {
        let _ = self.worker_tx.send(WorkerCommand::Quit);
    }

    /// Spawns the tray command receiver thread.
    /// On Linux, this receives TrayEvents from ksni callbacks.
    /// On Windows/macOS, this is a no-op (events use global MenuEvent channel).
    pub fn spawn_command_receiver(
        &mut self,
        proxy: tao::event_loop::EventLoopProxy<crate::AppEvent>,
    ) {
        self.tray.spawn_command_receiver(proxy);
    }

    fn set_active_device(&mut self, path: String) {
        if let Some(device) = self.last_known_status.iter().find(|d| d.path == path) {
            info!("User selected device: {} ({})", device.device_type, device.path);
            self.active_device_path = Some(device.path.clone());

            // Persist config
            self.config.preferred_device = Some(device.device_type);
            if let Err(e) = self.config.save() {
                error!("Failed to save config: {}", e);
            }

            // Force UI update
            self.update_tray_view();

            // Request fresh data from worker
            let _ = self.worker_tx.send(WorkerCommand::Refresh);
        }
    }

    /// Logic to determine which device should be shown in the tray icon.
    fn ensure_active_device_validity(&mut self) {
        // Always try to switch to Preferred Device if it becomes available.
        if let Some(preferred_device) = self.config.preferred_device {
            if let Some(device) = self
                .last_known_status
                .iter()
                .find(|d| d.device_type == preferred_device)
            {
                if self.active_device_path.as_ref() != Some(&device.path) {
                    self.active_device_path = Some(device.path.clone());
                    info!(
                        "Auto-switching to preferred device: {} ({})",
                        preferred_device, device.path
                    );
                }
                return;
            }
        }

        let is_valid = self
            .active_device_path
            .as_ref()
            .is_some_and(|id| self.last_known_status.iter().any(|d| d.path == *id));

        if !is_valid {
            // Try to restore from config
            if let Some(pref_type) = self.config.preferred_device {
                if let Some(device) = self
                    .last_known_status
                    .iter()
                    .find(|d| d.device_type == pref_type)
                {
                    self.active_device_path = Some(device.path.clone());
                    info!("Auto-selected preferred device: {} ({})", pref_type, device.path);
                    return;
                }
            }

            // Fallback: pick the first available
            if let Some(device) = self.last_known_status.first() {
                self.active_device_path = Some(device.path.clone());
                info!("Auto-selected active device: {} ({})", device.device_type, device.path);
            } else {
                self.active_device_path = None;
            }
        }
    }

    fn update_tray_view(&mut self) {
        if let Some(active_id) = &self.active_device_path {
            self.tray.update(
                &self.last_known_status,
                active_id,
                &self.icons,
                self.config.low_battery_threshold,
                self.config.critical_battery_threshold,
            );
        } else {
            self.tray.set_no_devices_state(&self.icons);
        }
    }
}
