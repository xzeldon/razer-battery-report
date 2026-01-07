use crate::config::AppConfig;
use crate::notification::Notifier;
use razer_battery_report as librazer;
use std::collections::HashMap;

/// Manages the state of connected devices to detect changes.
///
/// Stores the last known battery status for each device and compares it with
/// new updates to trigger notifications.
pub struct DeviceStateManager {
    last_device_states: HashMap<String, (librazer::DeviceType, librazer::BatteryStatus)>,
}

impl DeviceStateManager {
    pub fn new() -> Self {
        Self {
            last_device_states: HashMap::new(),
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
        // Convert input Vec to HashMap for efficient lookup
        let current_devices: HashMap<String, (librazer::DeviceType, librazer::BatteryStatus)> =
            current_data
                .iter()
                .map(|(path, device_type, status)| (path.clone(), (*device_type, *status)))
                .collect();

        // Check for Disconnected devices
        self.last_device_states.retain(|device_type, _| {
            if !current_devices.contains_key(device_type) {
                if config.notifications_enabled {
                    Notifier::send("Device Disconnected", &device_type.to_string());
                }
                return false; // Remove from map
            }
            true // Keep in map
        });

        // Check for Connected or Updated devices
        for (path, (device_type, new_status)) in &current_devices {
            if let Some((_, old_status)) = self.last_device_states.get(path) {
                // Device exists, check logic for changes
                if config.notifications_enabled {
                    self.check_state_changes(device_type, old_status, new_status, config);
                }
            } else {
                // New device detected
                if config.notifications_enabled {
                    Notifier::send("Device Connected", &format!("{}", device_type));
                }
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
            // Event: Started Charging
            // Level(x) -> Charging(x)
            (BatteryStatus::Level(_), BatteryStatus::Charging(_)) => {
                Notifier::send(
                    "Charging Started",
                    &format!("{} is now charging", device_name),
                );
            }

            // Event: Stopped Charging
            // Charging(x) -> Level(x)
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

            // Event: Battery Level Dropped
            // Level(old) -> Level(new)
            (BatteryStatus::Level(old_lvl), BatteryStatus::Level(new_lvl)) => {
                // Low Battery Threshold
                if old_lvl.value() > config.low_battery_threshold
                    && new_lvl.value() <= config.low_battery_threshold
                {
                    Notifier::send(
                        "Low Battery",
                        &format!("{} is at {}", device_name, new_lvl), // new_lvl Display implies %
                    );
                }

                // Critical Battery Threshold
                if old_lvl.value() > config.critical_battery_threshold
                    && new_lvl.value() <= config.critical_battery_threshold
                {
                    Notifier::send(
                        "Critical Battery",
                        &format!("{} is at {}! Please charge.", device_name, new_lvl),
                    );
                }
            }

            // Other transitions (e.g. Unknown -> Level) are ignored to prevent startup spam
            _ => {}
        }
    }
}
