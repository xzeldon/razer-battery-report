#![windows_subsystem = "windows"]

mod console;
mod controller;
mod devices;
mod manager;
mod notify;
mod tray;

use console::DebugConsole;
use tray::TrayApp;

fn main() {
    let console = DebugConsole::new("Razer Battery Report Debug Console");

    std::env::set_var("RUST_LOG", "trace");
    pretty_env_logger::init();

    TrayApp::new(console).run();
}
