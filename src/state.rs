use crate::config::AppConfig;
use crate::notification::Notifier;
use razer_battery_report as librazer;

/// The state of a single connected device.
#[derive(Clone)]
pub struct DeviceState {
    pub path: String,
    pub device_type: librazer::DeviceType,
    pub status: librazer::BatteryStatus,
}

/// Manages the state of connected devices to detect changes.
///
/// Stores the last known battery status for each device and compares it with
/// new updates to trigger notifications.
pub struct DeviceStateManager {
    last_device_states: Vec<DeviceState>,
}

impl DeviceStateManager {
    pub fn new() -> Self {
        Self {
            last_device_states: Vec::new(),
        }
    }

    /// Processes a new update from the worker.
    ///
    /// Detects connections, disconnections, and status changes, triggering
    /// notifications via `Notifier`.
    pub fn process_update(
        &mut self,
        current_data: &[(String, librazer::DeviceType, librazer::BatteryStatus)],
        config: &AppConfig,
    ) {
        let current_devices: Vec<DeviceState> = current_data
            .iter()
            .map(|(path, device_type, status)| DeviceState {
                path: path.clone(),
                device_type: *device_type,
                status: *status,
            })
            .collect();

        // Check for Disconnected devices
        self.last_device_states.retain(|prev| {
            if !current_devices.iter().any(|c| c.path == prev.path) {
                if config.notifications_enabled {
                    Notifier::send("Device Disconnected", &prev.device_type.to_string());
                }
                return false;
            }
            true
        });

        // Check for Connected or Updated devices
        for current in &current_devices {
            if let Some(prev) = self.last_device_states.iter().find(|p| p.path == current.path) {
                if config.notifications_enabled {
                    self.check_state_changes(
                        &current.device_type,
                        &prev.status,
                        &current.status,
                        config,
                    );
                }
            } else if config.notifications_enabled {
                    Notifier::send("Device Connected", &format!("{}", current.device_type));
                }
        }

        // Update internal state
        self.last_device_states = current_devices;
    }

    /// Compare states and notify on specific transitions.
    fn check_state_changes(
        &self,
        device_name: &librazer::DeviceType,
        old: &librazer::BatteryStatus,
        new: &librazer::BatteryStatus,
        config: &AppConfig,
    ) {
        use librazer::BatteryStatus;

        match (old, new) {
            (BatteryStatus::Level(_), BatteryStatus::Charging(_)) => {
                Notifier::send(
                    "Charging Started",
                    &format!("{} is now charging", device_name),
                );
            }

            (BatteryStatus::Charging(_), BatteryStatus::Level(lvl)) => {
                if lvl.value() == 100 {
                    Notifier::send(
                        "Fully Charged",
                        &format!("{} is fully charged", device_name),
                    );
                } else {
                    Notifier::send(
                        "Discharging",
                        &format!("{} is on battery ({}%)", device_name, lvl.value()),
                    );
                }
            }

            (BatteryStatus::Level(old_lvl), BatteryStatus::Level(new_lvl)) => {
                if old_lvl.value() > config.low_battery_threshold
                    && new_lvl.value() <= config.low_battery_threshold
                {
                    Notifier::send(
                        "Low Battery",
                        &format!("{} is at {}", device_name, new_lvl),
                    );
                }

                if old_lvl.value() > config.critical_battery_threshold
                    && new_lvl.value() <= config.critical_battery_threshold
                {
                    Notifier::send(
                        "Critical Battery",
                        &format!("{} is at {}! Please charge.", device_name, new_lvl),
                    );
                }
            }

            _ => {}
        }
    }
}
