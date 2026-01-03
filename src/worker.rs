use log::{debug, error, info};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
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

/// Starts the background worker thread.
///
/// # Arguments
/// * `proxy` - The channel to send events TO the UI (Main Thread).
/// * `polling_interval` - Time to sleep between automatic updates.
///
/// # Returns
/// * `Sender<WorkerCommand>` - A channel to send commands TO the worker.
pub fn start_worker(
    proxy: EventLoopProxy<AppEvent>,
    polling_interval: Duration,
) -> mpsc::Sender<WorkerCommand> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        info!("Worker thread started.");

        // Initialize Razer context once
        let mut context = match librazer::Razer::new() {
            Ok(ctx) => ctx,
            Err(e) => {
                error!("Critical: Failed to initialize Razer context in worker: {}. Retrying in loop...", e);
                // We construct a dummy context or handle this inside the loop.
                // For simplicity, let's try to re-init inside the loop if needed.
                // But for now, let's assume if init fails, we can't do much.
                return;
            }
        };

        loop {
            // Check for incoming commands (Non-blocking)
            // We use try_recv to see if the UI wants us to do something specific.
            match rx.try_recv() {
                Ok(WorkerCommand::Quit) => {
                    info!("Worker received Quit command. Shutting down.");
                    break;
                }
                Ok(WorkerCommand::Refresh) => {
                    info!("Worker forcing refresh.");
                    // Fall through to update logic immediately
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // No commands, proceed to normal polling
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    // The main thread died?
                    break;
                }
            }

            // Refresh device list (detect hotplug)
            if let Err(e) = context.refresh_devices() {
                error!("Failed to refresh devices: {}", e);
            }

            // Collect data
            let devices = context.get_connected_devices();
            let mut updates = Vec::new();

            for device in devices {
                // We create a new handle for each check to be stateless and safe
                match device.open(&context) {
                    Ok(handle) => match handle.get_battery_level() {
                        Ok(status) => {
                            debug!("Device {:?} status: {}", device.device_type(), status);
                            updates.push((device.device_type(), status));
                        }
                        Err(e) => {
                            error!(
                                "Failed to get battery for {:?}: {}",
                                device.device_type(),
                                e
                            );
                        }
                    },
                    Err(e) => {
                        error!("Failed to open device {:?}: {}", device.device_type(), e);
                    }
                }
            }

            // Send data to UI
            // This wakes up the Event Loop!
            if !updates.is_empty() {
                let _ = proxy.send_event(AppEvent::BatteryUpdate(updates));
            }

            // Sleep
            // We use recv_timeout for the sleep period
            // This means we sleep for `polling_interval`, OR wake up immediately if a command comes.
            match rx.recv_timeout(polling_interval) {
                Ok(WorkerCommand::Quit) => break,
                Ok(WorkerCommand::Refresh) => continue, // Loop again immediately
                Err(mpsc::RecvTimeoutError::Timeout) => continue, // Just timeout, loop again
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    tx
}
