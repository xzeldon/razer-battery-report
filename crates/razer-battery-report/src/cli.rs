//! The command line: `status`, `diagnose` and `cleanup`.

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use razer_hid::{Battery, Device, Error, Hid, Link};
use serde::Serialize;

use crate::cleanup;
use crate::config::Config;
use crate::paths;

const NO_MOUSE: &str = "no supported Razer mouse found";

/// Command line of the app. Without a subcommand the app runs in the tray.
#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Cli {
    /// What to do instead of running in the tray.
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// The things the binary can do on the command line.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print the battery level of every connected mouse.
    Status {
        /// Print machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Print a report to attach to a bug report.
    Diagnose,
    /// Remove everything the app stored on this computer, before uninstalling it.
    Cleanup {
        /// Also remove the settings and the logs.
        #[arg(long)]
        all: bool,
    },
}

/// The outcome of reading a mouse.
enum State {
    Ok(Battery),
    Asleep,
    Error(String),
}

/// One mouse in the `status --json` output.
#[derive(Serialize)]
struct DeviceJson<'a> {
    model: &'static str,
    serial: Option<&'a str>,
    state: &'static str,
    level: Option<u8>,
    charging: Option<bool>,
    error: Option<&'a str>,
    endpoints: Vec<EndpointJson>,
}

/// One endpoint in the `status --json` output.
#[derive(Serialize)]
struct EndpointJson {
    name: &'static str,
    pid: String,
    link: &'static str,
}

pub fn run(command: &Command, config: &Config) -> anyhow::Result<()> {
    match command {
        Command::Status { json } => status(*json),
        Command::Cleanup { all } => cleanup::run(*all),
        Command::Diagnose => {
            println!("{}", diagnose(config));
            Ok(())
        }
    }
}

/// The battery as text, for example `45%  charging`; nothing is said about charging when the
/// model cannot report it.
pub fn battery_text(battery: Battery) -> String {
    let level = battery.level.get();
    match battery.charging {
        Some(true) => format!("{level}%  charging"),
        Some(false) | None => format!("{level}%"),
    }
}

/// The text of `diagnose`: app, system, paths and every mouse with its endpoints. Problems with
/// the HID subsystem are part of the report, not an error.
pub fn diagnose(config: &Config) -> String {
    let mut lines = vec![
        format!("razer-battery-report {}", env!("CARGO_PKG_VERSION")),
        format!("os: {} {}", std::env::consts::OS, std::env::consts::ARCH),
        format!("config file: {}", display_path(paths::config_file())),
        format!("log directory: {}", display_path(paths::log_dir())),
        format!(
            "config: notifications={} low_threshold={} autostart={} selected={} show_level={}",
            config.notifications,
            config.low_threshold,
            config.autostart,
            config.selected.as_deref().unwrap_or("none"),
            config.show_level,
        ),
    ];
    match list_devices() {
        Err(error) => lines.push(format!("devices: {error:#}")),
        Ok(devices) if devices.is_empty() => lines.push(NO_MOUSE.to_owned()),
        Ok(devices) => {
            for device in &devices {
                lines.push(String::new());
                lines.push(device.model().to_owned());
                lines.push(format!("  serial: {}", device.serial().unwrap_or("none")));
                lines.push(format!("  battery: {}", state_text(&probe(device))));
                for endpoint in device.endpoints() {
                    lines.push(format!(
                        "  endpoint: {} pid={} link={} path={}",
                        endpoint.name(),
                        pid_text(endpoint.pid()),
                        link_text(endpoint.link()),
                        endpoint.path(),
                    ));
                }
            }
        }
    }
    lines.join("\n")
}

fn status(json: bool) -> anyhow::Result<()> {
    let devices = list_devices()?;
    let states: Vec<State> = devices.iter().map(probe).collect();
    match json {
        true => {
            let output: Vec<DeviceJson> = devices.iter().zip(&states).map(device_json).collect();
            let text = serde_json::to_string_pretty(&output).context("cannot encode json")?;
            println!("{text}");
        }
        false if devices.is_empty() => println!("{NO_MOUSE}"),
        false => {
            for (device, state) in devices.iter().zip(&states) {
                println!("{}  {}", device.model(), state_text(state));
            }
        }
    }
    Ok(())
}

fn list_devices() -> anyhow::Result<Vec<Device>> {
    let hid = Hid::new().context("cannot initialise hid")?;
    hid.devices().context("cannot list devices")
}

/// Reads the battery of `device` and sorts the outcome into a [`State`].
fn probe(device: &Device) -> State {
    match device.battery() {
        Ok(battery) => State::Ok(battery),
        Err(Error::Timeout) => State::Asleep,
        Err(error) => State::Error(error.to_string()),
    }
}

fn state_text(state: &State) -> String {
    match state {
        State::Ok(battery) => battery_text(*battery),
        State::Asleep => "asleep".to_owned(),
        State::Error(error) => format!("error: {error}"),
    }
}

fn device_json<'a>((device, state): (&'a Device, &'a State)) -> DeviceJson<'a> {
    let (name, battery, error) = match state {
        State::Ok(battery) => ("ok", Some(battery), None),
        State::Asleep => ("asleep", None, None),
        State::Error(error) => ("error", None, Some(error.as_str())),
    };
    DeviceJson {
        model: device.model(),
        serial: device.serial(),
        state: name,
        level: battery.map(|battery| battery.level.get()),
        charging: battery.and_then(|battery| battery.charging),
        error,
        endpoints: device
            .endpoints()
            .iter()
            .map(|endpoint| EndpointJson {
                name: endpoint.name(),
                pid: pid_text(endpoint.pid()),
                link: link_text(endpoint.link()),
            })
            .collect(),
    }
}

fn pid_text(pid: u16) -> String {
    format!("{pid:#06X}")
}

fn link_text(link: Link) -> &'static str {
    match link {
        Link::Wired => "wired",
        Link::Dongle => "dongle",
        Link::Bluetooth => "bluetooth",
    }
}

fn display_path(path: Option<std::path::PathBuf>) -> String {
    path.map_or_else(|| "unknown".to_owned(), |path| path.display().to_string())
}
