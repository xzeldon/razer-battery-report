#![windows_subsystem = "windows"]

use tray::TrayApp;
use winapi::um::{self, wincon, winuser};

mod controller;
mod devices;
mod manager;
mod tray;

fn main() {
    unsafe {
        // Allocate new console for the process
        um::consoleapi::AllocConsole();

        // Modify the console window's style to remove the system menu (close, minimize, etc.).
        winuser::SetWindowLongPtrW(
            wincon::GetConsoleWindow(),
            winuser::GWL_STYLE,
            #[allow(clippy::cast_possible_wrap)]
            {
                winuser::GetWindowLongPtrW(wincon::GetConsoleWindow(), winuser::GWL_STYLE)
                    & !winuser::WS_SYSMENU as isize
            },
        );

        // Hide the console window
        winuser::ShowWindow(wincon::GetConsoleWindow(), winuser::SW_HIDE);
    }

    std::env::set_var("RUST_LOG", "trace");
    pretty_env_logger::init();
    let checker = TrayApp::new();
    checker.run();
}
