// Hide console window on Windows release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod cli;
mod config;
mod icon;
mod logger;
mod notification;
mod state;
mod tray;
mod worker;

use std::time::Duration;

use log::{debug, error, info};
use razer_battery_report::{self as librazer};

use config::AppConfig;
use tao::{
    event::{Event, StartCause},
    event_loop::{ControlFlow, EventLoopBuilder},
};

#[cfg(target_os = "macos")]
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};

use crate::{app::RazerApp, icon::IconSet};

/// Events that can be sent to the main event loop.
#[derive(Debug)]
pub enum AppEvent {
    BatteryUpdate(Vec<(String, librazer::DeviceType, librazer::BatteryStatus)>),
    #[cfg(not(target_os = "linux"))]
    MenuEvent(tray_icon::menu::MenuEvent),
    TrayEvent(tray::TrayEvent),
    CriticalError(String),
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

    // Earyly load config to get log_level.
    let mut config = AppConfig::load().unwrap_or_else(|e| {
        eprintln!("Config error: {}. Using defaults.", e);
        AppConfig::default()
    });

    logger::init(args.log_level, config.log_level, args.log_mode())?;

    // Check for invalid config values and fix them.
    let warnings = config.sanitize();

    for warning in &warnings {
        log::warn!("{}", warning);
    }

    // Save fixed config file to disk.
    if !warnings.is_empty() {
        info!("Updating configuration file with safe values...");
        if let Err(e) = config.save() {
            error!("Failed to save sanitized config: {}", e);
        }
    }

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
    let _ = config.save();

    // Load icons
    debug!("Load icons...");
    let icons = IconSet::load()?;

    // Setup Event Loop
    let event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();

    #[cfg(target_os = "macos")]
    {
        event_loop.set_activation_policy(ActivationPolicy::Accessory);
        event_loop.set_dock_visibility(false);
    }

    let proxy = event_loop.create_proxy();

    // Spawn Menu listener thread (Windows/macOS only)
    // This bridges tray-icon's global channel to tao Event loop
    #[cfg(not(target_os = "linux"))]
    {
        use std::thread;
        use tray_icon::menu::MenuEvent;
        let menu_proxy = proxy.clone();
        thread::spawn(move || {
            while let Ok(event) = MenuEvent::receiver().recv() {
                let _ = menu_proxy.send_event(AppEvent::MenuEvent(event));
            }
        });
    }

    // Start Worker Thread
    debug!("Starting worker thread...");
    let polling_interval = Duration::from_secs(config.polling_interval_secs);
    let worker_tx = worker::start_worker(proxy.clone(), polling_interval);

    // Initialize Application Controller
    let mut app = RazerApp::new(config, icons, worker_tx)?;

    // Spawn tray command receiver thread
    app.spawn_command_receiver(proxy.clone());

    debug!("Entering main event loop.");

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            // Init
            Event::NewEvents(StartCause::Init) => {
                app.on_app_init();
            }
            // Battery Update
            Event::UserEvent(AppEvent::BatteryUpdate(data)) => {
                app.on_battery_update(data);
            }
            // Menu Interaction (Windows/macOS only)
            #[cfg(not(target_os = "linux"))]
            Event::UserEvent(AppEvent::MenuEvent(menu_event)) => {
                if app.on_menu_event(menu_event) {
                    *control_flow = ControlFlow::Exit;
                }
            }
            // Tray Events (all platforms)
            Event::UserEvent(AppEvent::TrayEvent(event)) => {
                if app.on_tray_event(event) {
                    *control_flow = ControlFlow::Exit;
                }
            }
            // Critical Error
            Event::UserEvent(AppEvent::CriticalError(msg)) => {
                error!("Critical error: {}", msg);
                crate::notification::Notifier::send_blocking(
                    "Razer Battery Report: Critical Error",
                    &msg,
                );
                *control_flow = ControlFlow::Exit;
            }
            // System shutdown/close
            Event::WindowEvent {
                event: tao::event::WindowEvent::CloseRequested,
                ..
            } => {
                app.on_shutdown();
                *control_flow = ControlFlow::Exit;
            }

            _ => (),
        }
    });
}

/// Runs diagnostic checks on connected Razer devices.
fn run_diagnostics() -> anyhow::Result<()> {
    info!("Running diagnostics...");

    let mut context = librazer::Razer::new()?;
    context.refresh_devices()?;

    let devices = context.get_connected_devices();

    if devices.is_empty() {
        println!("No Razer devices found via HID API.");
        return Ok(());
    }

    println!("Found {} device(s):", devices.len());

    for device in devices {
        println!(
            "- Device: {} (PID: 0x{:04X})",
            device.device_type(),
            device.product_id
        );
        println!("  Path: {}", device.path);

        match device.open(&context) {
            Ok(handle) => match handle.get_battery_level() {
                Ok(status) => println!("  Status: {}", status),
                Err(e) => println!("  Status: Error ({})", e),
            },
            Err(e) => println!("  Status: Error opening device ({})", e),
        }
        println!();
    }

    Ok(())
}
