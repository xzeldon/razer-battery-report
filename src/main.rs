#![windows_subsystem = "windows"]

use std::{ffi::OsStr, os::windows::ffi::OsStrExt};

use tray::TrayApp;

mod controller;
mod devices;
mod manager;
mod tray;

fn main() {
    unsafe {
        // Allocate new console for the process
        winapi::um::consoleapi::AllocConsole();

        let title: Vec<u16> = OsStr::new("Razer Battery Report Debug Console")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        winapi::um::wincon::SetConsoleTitleW(title.as_ptr());

        let hwnd = winapi::um::wincon::GetConsoleWindow();

        // Disable close command in the sys.menu of the new console, otherwise the whole process will quit: https://stackoverflow.com/a/12015131/126995
        if !hwnd.is_null() {
            let hmenu = winapi::um::winuser::GetSystemMenu(hwnd, 0);
            if !hmenu.is_null() {
                winapi::um::winuser::DeleteMenu(
                    hmenu,
                    winapi::um::winuser::SC_CLOSE as u32,
                    winapi::um::winuser::MF_BYCOMMAND,
                );
            }
        }

        // Hide the console window
        if !hwnd.is_null() {
            winapi::um::winuser::ShowWindow(hwnd, winapi::um::winuser::SW_HIDE);
        }
    }

    std::env::set_var("RUST_LOG", "trace");
    pretty_env_logger::init();
    let checker = TrayApp::new();
    checker.run();
}
