use log::{debug, error, info};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use tao::event_loop::EventLoopProxy;

use crate::AppEvent;
use razer_battery_report as librazer;

/// Commands sent from the Main Thread to the Worker Thread.
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
    known_devices_signature: HashSet<(u16, u16)>,
    last_known_levels: HashMap<librazer::DeviceType, librazer::BatteryStatus>,
}

impl Worker {
    /// Tries to initialize the Razer context and creates the worker.
    fn new(proxy: EventLoopProxy<AppEvent>, polling_interval: Duration) -> Option<Self> {
        let context = match librazer::Razer::new() {
            Ok(ctx) => ctx,
            Err(e) => {
                error!("Critical: Failed to initialize Razer context. Worker thread stopping. Error: {}", e);
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
        let current_signature: HashSet<(u16, u16)> = devices
            .iter()
            .map(|d| (d.vendor_id, d.product_id))
            .collect();

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
        let mut updates = Vec::new();

        for device in devices {
            let result = process_device(&self.context, &device);

            match result {
                Some((dev_type, status)) => {
                    self.last_known_levels.insert(dev_type, status);
                    updates.push((dev_type, status));
                }
                None => {
                    // This keeps the device visible in the tray even if it sleeps.
                    if let Some(&cached_status) = self.last_known_levels.get(&device.device_type())
                    {
                        info!(
                            "Device {:?} is not responding (Sleep/Off). Using cached status: {}",
                            device.device_type(),
                            cached_status
                        );
                        updates.push((device.device_type(), cached_status));
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
                DeviceType::DummyDevice,
                BatteryStatus::Level(BatteryLevel::new(fake_level)),
            ));
        }

        if !updates.is_empty() {
            let _ = self.proxy.send_event(AppEvent::BatteryUpdate(updates));
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
            // TODO: Maybe try to implement something like AppEvent::CriticalError and send it to the UI?
            // For now, we return, which kills the worker (but not the main UI).
            None => return,
        };

        let hotplug_check_interval = Duration::from_secs(3);

        loop {
            let mut force_update = false;

            // Check for incoming commands (Non-blocking)
            match rx.try_recv() {
                Ok(WorkerCommand::Quit) => {
                    info!("Worker received Quit command. Shutting down.");
                    break;
                }
                Ok(WorkerCommand::Refresh) => {
                    info!("Worker forcing refresh.");
                    // Fall through to update logic immediately
                    force_update = true;
                }
                // No commands, proceed to normal polling
                Err(mpsc::TryRecvError::Disconnected) => break,
                // The main thread died?
                Err(mpsc::TryRecvError::Empty) => {}
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

            thread::sleep(hotplug_check_interval);
        }
    });

    tx
}
