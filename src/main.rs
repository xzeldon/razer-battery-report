// Hide console window on Windows release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod icon;
mod worker;

use std::{thread, time::Duration};

use log::{error, info, warn};
use razer_battery_report as librazer;

use config::AppConfig;
use tao::event_loop::{ControlFlow, EventLoop, EventLoopBuilder};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    TrayIconBuilder,
};

use crate::icon::IconSet;

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
        Ok(cfg) => cfg,
        Err(e) => {
            error!("Config error: {}. Using defaults.", e);
            AppConfig::default()
        }
    };

    // Load icons
    info!("Load icons...");
    let icons = IconSet::load()?;

    // Build tray
    let tray_menu = Menu::new();
    let quit_item = MenuItem::new("Exit", true, None);
    // TODO: Add settings and logs (?) later
    tray_menu.append(&quit_item)?;

    let mut _tray_icon = Some(
        TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu))
            .with_tooltip("Razer Battery Report: Initializing")
            .with_icon(icons.white.clone())
            .build()?,
    );

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

                // We take the first available device to update the icon
                if let Some((device_type, status)) = data.first() {
                    let new_icon = icons.get_icon(
                        status,
                        config.low_battery_threshold,
                        config.critical_battery_threshold,
                    );

                    if let Some(tray) = _tray_icon.as_mut() {
                        if let Err(e) = tray.set_icon(Some(new_icon.clone())) {
                            warn!("Failed to update tray icon: {}", e);
                        }

                        let tooltip = format!("{}: {}", device_type, status);
                        if let Err(e) = tray.set_tooltip(Some(tooltip)) {
                            warn!("Failed to update tooltip: {}", e);
                        }
                    }
                }
            }

            // Menu clicks
            tao::event::Event::UserEvent(AppEvent::MenuEvent(menu_event)) => {
                if menu_event.id == quit_item.id() {
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
