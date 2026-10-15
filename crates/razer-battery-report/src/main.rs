//! Razer battery report: a tray app and CLI for the battery of wireless Razer mice.

mod cli;
mod config;
mod logging;
mod monitor;
mod paths;
mod poller;

use anyhow::Context as _;
use clap::Parser as _;
use razer_hid::{Hid, Percent};

use cli::Cli;
use config::Config;
use monitor::Reading;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    // Keeps the log files flushed until the app exits.
    let _logger = logging::init();
    let config = Config::load();
    match cli.command {
        Some(command) => cli::run(&command, &config),
        None => watch(&config),
    }
}

/// Prints readings and events to stdout. A stand-in for the tray until it exists.
fn watch(config: &Config) -> anyhow::Result<()> {
    let low_threshold = Percent::try_from(config.low_threshold).context("invalid low threshold")?;
    let hid = Hid::new().context("cannot initialise hid")?;
    let receiver = poller::spawn(hid);
    let mut previous: Option<Vec<Reading>> = None;
    for readings in &receiver {
        for reading in &readings {
            println!("{}  {}", reading.model, reading_text(reading));
        }
        for event in monitor::events(previous.as_deref(), &readings, low_threshold) {
            log::info!("monitor: {event}");
            println!("{event}");
        }
        previous = Some(readings);
    }
    Ok(())
}

fn reading_text(reading: &Reading) -> String {
    match (reading.battery, reading.asleep) {
        (Some(battery), false) => cli::battery_text(battery),
        (Some(battery), true) => format!("{} (asleep)", cli::battery_text(battery)),
        (None, true) => "asleep".to_owned(),
        (None, false) => "unknown".to_owned(),
    }
}
