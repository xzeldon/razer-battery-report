#![windows_subsystem = "windows"]

use tray::TrayApp;

mod controller;
mod devices;
mod manager;
mod tray;

fn main() {
    std::env::set_var("RUST_LOG", "trace");
    pretty_env_logger::init();
    let checker = TrayApp::new();
    checker.run();
}
