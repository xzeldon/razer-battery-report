// Hide console window on Windows release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod icon;
mod notification;
mod state;
mod tray;
mod worker;

use std::{thread, time::Duration};

use log::{error, info};
use razer_battery_report as librazer;

use config::AppConfig;
use tao::event_loop::{ControlFlow, EventLoop, EventLoopBuilder};
use tray_icon::menu::MenuEvent;

use crate::{icon::IconSet, state::DeviceStateManager, tray::AppTray};

/// Events that can be sent to the main event loop.
#[derive(Debug)]
pub enum AppEvent {
    /// Update received from the worker thread.
    BatteryUpdate(Vec<(librazer::DeviceType, librazer::BatteryStatus)>),
    /// User clicked an item in the tray menu.
    #[allow(dead_code)]
    MenuEvent(tray_icon::menu::MenuEvent),
}

fn main() -> anyhow::Result<()> {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    pretty_env_logger::init();

    info!("Starting Razer Battery Report...");

    let config = match AppConfig::load() {
        Ok(cfg) => {
            // Force save to ensure new fields are written to the file on disk
            if let Err(e) = cfg.save() {
                error!("Failed to update config file on disk: {}", e);
            }
            cfg
        }
        Err(e) => {
            error!("Config error: {}. Using defaults.", e);
            let cfg = AppConfig::default();
            if let Err(save_err) = cfg.save() {
                error!("Failed to save default config: {}", save_err);
            }
            cfg
        }
    };

    // Load icons
    info!("Load icons...");
    let icons = IconSet::load()?;

    // Logic and UI
    let mut state_manager = DeviceStateManager::new();
    let mut app_tray = AppTray::new(&icons)?;

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
        // By default, just wait for events. This effectively sleeps the main thread,
        // using 0% CPU until the worker sends an event or user interacts with tray.
        *control_flow = ControlFlow::Wait;

        match event {
            // Battery updates
            tao::event::Event::UserEvent(AppEvent::BatteryUpdate(data)) => {
                info!("Received update: {:?}", data);

                // Update state first
                state_manager.process_update(&data, &config);

                // Update Tray
                // For now, just pick first device
                if let Some((device_type, status)) = data.first() {
                    app_tray.update(
                        device_type,
                        status,
                        &icons,
                        config.low_battery_threshold,
                        config.critical_battery_threshold,
                    );
                }
            }

            // Menu clicks
            tao::event::Event::UserEvent(AppEvent::MenuEvent(menu_event)) => {
                if menu_event.id == app_tray.quit_item.id() {
                    info!("Exit requested via tray menu.");
                    *control_flow = ControlFlow::Exit;
                }
            }

            // Exit request
            tao::event::Event::WindowEvent {
                event: tao::event::WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }

            _ => (),
        }
    });
}
