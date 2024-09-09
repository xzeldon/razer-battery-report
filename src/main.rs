#![windows_subsystem = "windows"]

use console::DebugConsole;
use tray::TrayApp;

mod console;
mod controller;
mod devices;
mod manager;
mod tray;

fn main() {
    let console = DebugConsole::new("Razer Battery Report Debug Console");

    std::env::set_var("RUST_LOG", "trace");
    pretty_env_logger::init();

    let checker = TrayApp::new(console);
    checker.run();
}
