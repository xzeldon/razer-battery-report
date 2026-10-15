//! Razer battery report: a tray app and CLI for the battery of wireless Razer mice.

mod cli;
mod config;
mod icon;
mod logging;
mod monitor;
mod paths;
mod platform;
mod poller;
mod tray;

use clap::Parser as _;

use cli::Cli;
use config::Config;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    // Keeps the log files flushed until the app exits.
    let _logger = logging::init();
    let config = Config::load();
    if let Some(command) = cli.command {
        return cli::run(&command, &config);
    }
    platform::detach_console();
    tray::run(config)
}
