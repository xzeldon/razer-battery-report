//! Razer battery report: a tray app and CLI for the battery of wireless Razer mice.

mod cleanup;
mod cli;
mod config;
mod icon;
mod logging;
mod monitor;
mod paths;
mod platform;
mod poller;
mod profiling;
mod tray;
#[cfg(feature = "self-update")]
mod update;

use clap::Parser as _;

use cli::{Cli, Command};
use config::Config;

fn main() -> anyhow::Result<()> {
    profiling::start();
    let cli = Cli::parse();
    // Keeps the log files flushed until the app exits.
    let _logger = match cli.command {
        // The log file would stay open and block removing the log directory.
        Some(Command::Cleanup { .. }) => logging::init_stderr(),
        _ => logging::init(),
    };
    let config = Config::load();
    if let Some(command) = cli.command {
        return cli::run(&command, &config);
    }
    platform::detach_console();
    tray::run(config)
}
