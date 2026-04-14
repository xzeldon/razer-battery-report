use std::collections::HashMap;
use std::sync::mpsc::Sender;

use log::{debug, error, info};
use razer_battery_report as librazer;

#[cfg(not(target_os = "linux"))]
use tray_icon::menu::MenuEvent;

use crate::config::AppConfig;
use crate::icon::IconSet;
use crate::state::DeviceStateManager;
use crate::tray::AppTray;
use crate::worker::WorkerCommand;

/// The main application controller.
pub struct RazerApp {
    config: AppConfig,
    icons: IconSet,
    tray: AppTray,
    state_manager: DeviceStateManager,
    active_device_path: Option<String>,
    last_known_status: HashMap<String, (librazer::DeviceType, librazer::BatteryStatus)>,
    worker_tx: Sender<WorkerCommand>,
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
            last_known_status: HashMap::new(),
            worker_tx,
        })
    }

    /// Called when the Event Loop receives StartCause::Init.
    pub fn on_app_init(&mut self) {
        debug!("Initializing UI elements...");
        if let Err(e) = self.tray.init_tray_icon(&self.icons) {
            error!("Failed to initialize tray icon: {}", e);
        } else {
            info!("Tray initialized successfully.");
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
            .map(|(path, dtype, status)| (path, (dtype, status)))
            .collect();
        self.ensure_active_device_validity();
        self.update_tray_view();
    }

    /// Handles clicks on the tray menu (Windows/macOS only).
    /// Returns `true` if the application should exit.
    #[cfg(not(target_os = "linux"))]
    pub fn on_menu_event(&mut self, event: MenuEvent) -> bool {
        // Check Quit
        if self.tray.is_quit_event(event.id.as_ref()) {
            info!("Exit requested via tray menu.");
            let _ = self.worker_tx.send(WorkerCommand::Quit);
            return true;
        }

        if let Some(selected_path) = self.tray.handle_menu_click(event.id.as_ref()) {
            self.set_active_device(selected_path);
        }

        false // Do not exit
    }

    /// Handles ksni commands from the Linux tray (Linux only).
    /// Returns `true` if the application should exit.
    #[cfg(target_os = "linux")]
    pub fn on_ksni_command(&mut self, cmd: WorkerCommand) -> bool {
        match cmd {
            WorkerCommand::Quit => {
                info!("Exit requested via ksni tray menu.");
                let _ = self.worker_tx.send(WorkerCommand::Quit);
                return true;
            }
            WorkerCommand::SelectDevice(path) => {
                self.set_active_device(path);
            }
            WorkerCommand::ToggleAutostart(enabled) => {
                info!("Autostart toggled: {}", enabled);
                self.config.autostart_enabled = enabled;
                if let Err(e) = self.config.save() {
                    error!("Failed to save config: {}", e);
                }
                self.tray.set_autostart(enabled);
            }
            WorkerCommand::ToggleNotifications(enabled) => {
                info!("Notifications toggled: {}", enabled);
                self.config.notifications_enabled = enabled;
                if let Err(e) = self.config.save() {
                    error!("Failed to save config: {}", e);
                }
                self.tray.set_notifications(enabled);
            }
            WorkerCommand::Restart => {
                info!("Restart requested.");
                // TODO: Implement graceful restart
            }
            WorkerCommand::ShowAbout => {
                info!("About dialog requested.");
                // TODO: Show about dialog
            }
            WorkerCommand::Refresh => {
                let _ = self.worker_tx.send(WorkerCommand::Refresh);
            }
        }

        false // Do not exit
    }

    /// Called when the OS requests the app to close.
    pub fn on_shutdown(&self) {
        let _ = self.worker_tx.send(WorkerCommand::Quit);
    }

    /// Spawns the ksni command receiver thread (Linux only).
    #[cfg(target_os = "linux")]
    pub fn spawn_ksni_command_receiver(
        &mut self,
        proxy: tao::event_loop::EventLoopProxy<crate::AppEvent>,
    ) {
        self.tray.spawn_command_receiver(proxy);
    }

    fn set_active_device(&mut self, path: String) {
        if let Some((dtype, _)) = self.last_known_status.get(&path) {
            info!("User selected device: {} ({})", dtype, path);
            self.active_device_path = Some(path.clone());

            // Persist config
            self.config.preferred_device = Some(*dtype);
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
        // This handles the case where we fallback to a secondary device,
        // but the preffered device just reconnected.
        if let Some(preferred_device) = self.config.preferred_device {
            if let Some((path, _)) = self
                .last_known_status
                .iter()
                .find(|(_, (device_type, _))| *device_type == preferred_device)
            {
                if self.active_device_path.as_ref() != Some(path) {
                    self.active_device_path = Some(path.clone());
                    info!(
                        "Auto-switching to preferred device: {} ({})",
                        preferred_device, path
                    );
                }
                // We found preferred device, no need to run fallback.
                return;
            }
        }

        let is_valid = self
            .active_device_path
            .as_ref()
            .is_some_and(|id| self.last_known_status.contains_key(id));

        if !is_valid {
            // Try to restore from config
            if let Some(pref_type) = self.config.preferred_device {
                if let Some((path, _)) = self
                    .last_known_status
                    .iter()
                    .find(|(_, (dtype, _))| *dtype == pref_type)
                {
                    self.active_device_path = Some(path.clone());
                    info!("Auto-selected preferred device: {} ({})", pref_type, path);
                    return;
                }
            }

            // Fallback: pick the first available
            if let Some((path, (dtype, _))) = self.last_known_status.iter().next() {
                self.active_device_path = Some(path.clone());
                info!("Auto-selected active device: {} ({})", dtype, path);
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
