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

/// Processes a single device: opens it and retrieves the battery level.
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
            // Check if the device being asleep/off
            if let librazer::DeviceError::CommunicationFailed {
                reason: librazer::CommunicationFailureReason::CommandTimedOut,
                ..
            } = e
            {
                debug!(
                    "Device {:?} timed out (Sleep/Off). Keeping last known state.",
                    device.device_type()
                );
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

        // Initialize Razer context once
        let mut context = match librazer::Razer::new() {
            Ok(ctx) => ctx,
            Err(e) => {
                error!("Critical: Failed to initialize Razer context. Worker thread stopping. Error: {}", e);
                // Maybe try creating the context again?
                // For now, we return, which kills the worker (but not the main UI).
                return;
            }
        };

        loop {
            // Check for incoming commands (Non-blocking)
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
            let updates: Vec<_> = context
                .get_connected_devices()
                .into_iter()
                .filter_map(|device| process_device(&context, &device))
                .collect();

            // Send data to UI
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
