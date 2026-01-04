use log::{debug, error, info};
use std::collections::HashSet;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use tao::event_loop::EventLoopProxy;

use crate::AppEvent;
use razer_battery_report as librazer;

/// Commands sent from the Main Thread to the Worker Thread.
pub enum WorkerCommand {
    /// Force an immediate refresh of devices and battery status.
    #[allow(dead_code)]
    Refresh,
    /// Stop the worker loop (graceful shutdown).
    #[allow(dead_code)]
    Quit,
}

/// The state and logic of the worker.
struct Worker {
    context: librazer::Razer,
    proxy: EventLoopProxy<AppEvent>,
    polling_interval: Duration,
    last_battery_query: Instant,
    known_devices_signature: HashSet<(u16, u16)>,
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
        })
    }

    /// Checks for physical device changes (Hotplug).
    /// Returns `true` if the device list has changed since the last check.
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

        let updates: Vec<_> = devices
            .iter()
            .filter_map(|device| process_device(&self.context, device))
            .collect();

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
