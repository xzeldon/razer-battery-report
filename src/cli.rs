use clap::Parser;

use crate::{config::LogLevel, logger::LogMode};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Set a log level (overrides config).
    #[arg(long, value_name = "LEVEL")]
    pub log_level: Option<LogLevel>,

    /// Run a quick diagnostic check and exit.
    /// This will try to initialize the driver, list connected devices, and print their status to the console.
    #[arg(long)]
    pub check: bool,

    /// Print udev rules for Linux and exit.
    #[arg(long)]
    #[cfg(target_os = "linux")]
    pub print_udev_rules: bool,
}

impl Args {
    pub fn parse() -> Self {
        <Self as Parser>::parse()
    }

    pub fn log_mode(&self) -> LogMode {
        #[cfg(target_os = "linux")]
        if self.print_udev_rules {
            return LogMode::ConsoleOnly;
        }

        if self.check {
            return LogMode::ConsoleOnly;
        }

        LogMode::FileAndConsole
    }
}
