use log::{debug, error, info};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};
use tao::event_loop::EventLoopProxy;

use crate::AppEvent;
use razer_battery_report as librazer;

/// Commands sent from the Main Thread to the Worker Thread.
#[derive(Debug)]
pub enum WorkerCommand {
    /// Force an immediate refresh of devices and battery status.
    Refresh,
    /// Stop the worker loop.
    Quit,
}

/// The state and logic of the worker.
struct Worker {
    context: librazer::Razer,
    proxy: EventLoopProxy<AppEvent>,
    polling_interval: Duration,
    last_battery_query: Instant,
    known_devices_signature: HashSet<String>,
    last_known_levels: HashMap<String, librazer::BatteryStatus>,
}

impl Worker {
    /// Tries to initialize the Razer context and creates the worker.
    fn new(proxy: EventLoopProxy<AppEvent>, polling_interval: Duration) -> Option<Self> {
        let context = match librazer::Razer::new() {
            Ok(ctx) => ctx,
            Err(e) => {
                let msg = format!("Failed to initialize Razer driver: {}", e);
                error!("Critical: {}", msg);
                let _ = proxy.send_event(AppEvent::CriticalError(msg));
                return None;
            }
        };

        Some(Self {
            context,
            proxy,
            polling_interval,
            // Initialize timestamp in the past to trigger immediate update on start
            last_battery_query: Instant::now()
                .checked_sub(polling_interval)
                .unwrap_or_else(Instant::now),
            known_devices_signature: HashSet::new(),
            last_known_levels: HashMap::new(),
        })
    }

    /// Checks for physical device changes (Hotplug).
    fn check_hotplug(&mut self) -> bool {
        if let Err(e) = self.context.refresh_devices() {
            error!("Failed to refresh devices: {}", e);
            return false;
        }

        let devices = self.context.get_connected_devices();
        let current_signature: HashSet<String> = devices.iter().map(|d| d.path.clone()).collect();

        if current_signature != self.known_devices_signature {
            info!("Device list changed (Hotplug detected).");
            self.known_devices_signature = current_signature;
            true
        } else {
            false
        }
    }

    /// Performs the slow operation of querying battery status for all devices
    fn update_batteries(&mut self) {
        let devices = self.context.get_connected_devices();
        let mut updates: Vec<(String, librazer::DeviceType, librazer::BatteryStatus)> = Vec::new();
        let mut current_paths = HashSet::new();

        for device in devices {
            current_paths.insert(device.path.clone());
            let result = process_device(&self.context, &device);

            match result {
                Some((device_type, status)) => {
                    self.last_known_levels.insert(device.path.clone(), status);
                    updates.push((device.path.clone(), device_type, status));
                }
                None => {
                    // This keeps the device visible in the tray even if it sleeps.
                    if let Some(&cached_status) = self.last_known_levels.get(&device.path) {
                        debug!(
                            "Device {:?} ({}) is not responding. Using cached status.",
                            device.device_type(),
                            device.path
                        );
                        updates.push((device.path.clone(), device.device_type(), cached_status));
                    }
                }
            }
        }

        // Mock fake device for debug purposes
        #[cfg(debug_assertions)]
        {
            use razer_battery_report::{BatteryLevel, BatteryStatus, DeviceType};

            // Generate a fake battery level based on seconds to make it change
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let fake_level = (secs % 100) as u8;

            updates.push((
                "/dev/null".to_string(),
                DeviceType::DummyDevice,
                BatteryStatus::Level(BatteryLevel::new(fake_level)),
            ));
        }

        if !updates.is_empty() {
            let _ = self.proxy.send_event(AppEvent::BatteryUpdate(updates));
        } else {
            let _ = self.proxy.send_event(AppEvent::BatteryUpdate(vec![]));
        }

        self.last_battery_query = Instant::now();
    }

    fn is_time_for_routine_update(&self) -> bool {
        self.last_battery_query.elapsed() >= self.polling_interval
    }
}

/// Processes a single device
fn process_device(
    context: &librazer::Razer,
    device: &librazer::Device,
) -> Option<(librazer::DeviceType, librazer::BatteryStatus)> {
    let handle = device
        .open(context)
        .map_err(|e| {
            error!("Failed to open device {:?}: {}", device.device_type(), e);
        })
        .ok()?;

    match handle.get_battery_level() {
        Ok(status) => {
            debug!("Device {:?} status: {}", device.device_type(), status);
            Some((device.device_type(), status))
        }
        Err(e) => {
            if let librazer::DeviceError::CommunicationFailed {
                reason: librazer::CommunicationFailureReason::CommandTimedOut,
                ..
            } = e
            {
                debug!("Device {:?} timed out (Sleep/Off).", device.device_type());
            } else {
                error!(
                    "Failed to get battery for {:?}: {}",
                    device.device_type(),
                    e
                );
            }
            None
        }
    }
}

/// Starts the background worker thread.
pub fn start_worker(
    proxy: EventLoopProxy<AppEvent>,
    polling_interval: Duration,
) -> mpsc::Sender<WorkerCommand> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        info!("Worker thread started.");

        let mut worker = match Worker::new(proxy, polling_interval) {
            Some(m) => m,
            None => return,
        };

        let hotplug_check_interval = Duration::from_secs(3);

        info!("Performing initial device scan...");
        worker.check_hotplug();
        worker.update_batteries();

        loop {
            let mut force_update = false;

            // Check for incoming commands
            match rx.recv_timeout(hotplug_check_interval) {
                Ok(WorkerCommand::Quit) => {
                    info!("Worker received Quit command. Shutting down.");
                    break;
                }
                Ok(WorkerCommand::Refresh) => {
                    debug!("Worker forcing refresh.");
                    // Fall through to update logic immediately
                    force_update = true;
                }
                // No commands, proceed to normal polling
                Err(RecvTimeoutError::Timeout) => {}
                // The main thread died?
                Err(RecvTimeoutError::Disconnected) => {
                    error!("Worker channel disconnected. Main thread likely exited.");
                    break;
                }
            }

            // Check for Hardware Changes (Hotplug)
            // Runs frequently (every `hotplug_check_interval`) without triggering device wake-up.
            if worker.check_hotplug() {
                force_update = true;
            }

            // Update if Forced OR Hotplug changed OR Time elapsed
            if force_update || worker.is_time_for_routine_update() {
                worker.update_batteries();
            }
        }
    });

    tx
}
