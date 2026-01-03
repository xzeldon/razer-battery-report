// Hide console window on Windows release builds
// #![windows_subsystem = "windows"]

mod config;

use log::{debug, error, info};
use razer_battery_report as librazer;

use config::AppConfig;

fn main() -> anyhow::Result<()> {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    pretty_env_logger::init();

    info!("Starting Razer Battery Report...");

    let config = match AppConfig::load() {
        Ok(cfg) => {
            debug!("Configuration loaded successfully.");
            cfg
        }
        Err(e) => {
            error!("Failed to load config: {}. Using defaults.", e);
            let cfg = AppConfig::default();
            if let Err(save_err) = cfg.save() {
                error!("Failed to save default config: {}", save_err);
            }
            cfg
        }
    };

    info!("Current settings: {:?}", config);

    info!("Initializing context...");
    match librazer::Razer::new() {
        Ok(ctx) => {
            let devices = ctx.get_connected_devices();
            info!("Found {} connected device(s).", devices.len());
            for device in devices {
                info!(" - {}", device.device_type());
            }
        }
        Err(e) => {
            error!("Failed to initialize Razer context: {}", e);
            // We don't panic here because the user might want to access the tray menu to exit/check logs even if the driver fails.
        }
    }

    // TODO: Initialize Tray, Event Loop and Worker Thread

    Ok(())
}
