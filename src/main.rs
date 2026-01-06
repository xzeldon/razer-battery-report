// Hide console window on Windows release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cli;
mod config;
mod icon;
mod logger;
mod notification;
mod state;
mod tray;
mod worker;

use std::{collections::HashMap, thread, time::Duration};

use log::{debug, error, info};
use razer_battery_report::{self as librazer};

use config::AppConfig;
use tao::event_loop::{ControlFlow, EventLoop, EventLoopBuilder};
use tray_icon::menu::MenuEvent;

use crate::{icon::IconSet, state::DeviceStateManager, tray::AppTray};

/// Events that can be sent to the main event loop.
#[derive(Debug)]
pub enum AppEvent {
    BatteryUpdate(Vec<(librazer::DeviceType, librazer::BatteryStatus)>),
    #[allow(dead_code)]
    MenuEvent(tray_icon::menu::MenuEvent),
}

fn main() -> anyhow::Result<()> {
    // This allows the GUI app to print to stdout/stderr if launched from a terminal.
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
        // We ignore the error.
        // If it fails (e.g. launched via double-click in Explorer), we just continue as a GUI app.
        // If it succeeds, we become a console app.
        unsafe {
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }

    let args = cli::Args::parse();

    // Earyly load config to get log_level
    let mut config = match AppConfig::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Config error: {}. Using defaults.", e);
            AppConfig::default()
        }
    };

    logger::init(args.log_level, config.log_level, args.log_mode())?;

    info!("Starting Razer Battery Report...");

    // Handle CLI
    #[cfg(target_os = "linux")]
    if args.print_udev_rules {
        let mut context = librazer::Razer::new()?;
        // Refresh isn't strictly necessary for printing static rules,
        // but we want to ensure context is valid.
        context.refresh_devices()?;
        context.print_udev_rules();
        return Ok(());
    }

    if args.check {
        run_diagnostics()?;
        return Ok(());
    }

    // Save config to disk
    if let Err(e) = config.save() {
        error!("Failed to update config file on disk: {}", e);
    }

    // Load icons
    info!("Load icons...");
    let icons = IconSet::load()?;

    // Logic and UI
    let mut state_manager = DeviceStateManager::new();
    let mut app_tray = AppTray::new(&icons)?;

    // UI State
    let mut active_device: Option<librazer::DeviceType> = config.preferred_device;
    let mut last_known_status: HashMap<librazer::DeviceType, librazer::BatteryStatus> =
        HashMap::new();

    // Setup Event Loop
    let event_loop: EventLoop<AppEvent> = EventLoopBuilder::<AppEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    // Spawn Menu listener thread
    // This bridges tray-icon's global channel to tao Event loop
    let menu_proxy = proxy.clone();
    thread::spawn(move || {
        while let Ok(event) = MenuEvent::receiver().recv() {
            let _ = menu_proxy.send_event(AppEvent::MenuEvent(event));
        }
    });

    // Start Worker Thread
    info!("Starting worker thread...");
    let polling_interval = Duration::from_secs(config.polling_interval_secs);
    let _worker_tx = worker::start_worker(proxy, polling_interval);

    // Run Event Loop
    info!("Entering main event loop.");
    event_loop.run(move |event, _, control_flow| {
        // This effectively sleeps the main thread,  using 0% CPU until
        // the worker sends an event or user interacts with tray.
        *control_flow = ControlFlow::Wait;

        match event {
            // Battery updates
            tao::event::Event::UserEvent(AppEvent::BatteryUpdate(data)) => {
                debug!("Received update: {:?}", data);

                // Update state first
                state_manager.process_update(&data, &config);
                last_known_status = data.into_iter().collect();

                if let Some(pref) = config.preferred_device {
                    if last_known_status.contains_key(&pref) {
                        active_device = Some(pref);
                    }
                }

                let is_active_valid = active_device
                    .as_ref()
                    .is_none_or(|d| last_known_status.contains_key(d));

                if !is_active_valid || active_device.is_none() {
                    if let Some(first_key) = last_known_status.keys().next() {
                        active_device = Some(*first_key);
                        info!("Auto-selected active device: {}", first_key);
                    } else {
                        active_device = None;
                    }
                }

                // Update Tray
                if let Some(current_active) = &active_device {
                    app_tray.update(
                        &last_known_status,
                        current_active,
                        &icons,
                        config.low_battery_threshold,
                        config.critical_battery_threshold,
                    );
                }
            }

            // Menu clicks
            tao::event::Event::UserEvent(AppEvent::MenuEvent(menu_event)) => {
                // Exit
                if menu_event.id == app_tray.quit_item.id() {
                    info!("Exit requested via tray menu.");

                    if let Err(e) = _worker_tx.send(worker::WorkerCommand::Quit) {
                        error!("Failed to send Quit command to worker: {}", e);
                    }

                    *control_flow = ControlFlow::Exit;
                }
                // Device selection
                else if let Some(selected_device) =
                    app_tray.handle_menu_click(menu_event.id.as_ref())
                {
                    info!("User selected device: {}", selected_device);
                    active_device = Some(selected_device);

                    // This redraws the menu with the correct checkmark immediately.
                    app_tray.update(
                        &last_known_status,
                        &selected_device,
                        &icons,
                        config.low_battery_threshold,
                        config.critical_battery_threshold,
                    );

                    config.preferred_device = Some(selected_device);
                    if let Err(e) = config.save() {
                        error!("Failed to save config: {}", e);
                    }

                    // Trigger worker refresh to get fresh data
                    let _ = _worker_tx.send(worker::WorkerCommand::Refresh);
                }
            }

            // System exit request (e.g. OS shutdown)
            tao::event::Event::WindowEvent {
                event: tao::event::WindowEvent::CloseRequested,
                ..
            } => {
                let _ = _worker_tx.send(worker::WorkerCommand::Quit);
                *control_flow = ControlFlow::Exit;
            }

            _ => (),
        }
    });
}

fn run_diagnostics() -> anyhow::Result<()> {
    info!("Running diagnostics...");
    let mut context = librazer::Razer::new()?;
    context.refresh_devices()?;

    let devices = context.get_connected_devices();

    if devices.is_empty() {
        info!("No Razer devices found via HID API.");
        return Ok(());
    }

    info!("Found {} device(s):", devices.len());
    for device in devices {
        info!(
            "- Device: {} (PID: 0x{:04X})",
            device.device_type(),
            device.product_id
        );
        info!("  Path: {}", device.path);

        match device.open(&context) {
            Ok(handle) => match handle.get_battery_level() {
                Ok(status) => info!("  Status: {}", status),
                Err(e) => error!("  Failed to get status: {}", e),
            },
            Err(e) => error!("  Failed to open device: {}", e),
        }
    }

    Ok(())
}
