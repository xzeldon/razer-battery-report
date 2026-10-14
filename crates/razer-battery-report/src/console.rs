use parking_lot::Mutex;
use std::{ffi::OsStr, os::windows::ffi::OsStrExt, sync::Arc};
use winapi::um::{consoleapi, wincon, winuser};

pub struct DebugConsole {
    hwnd: *mut winapi::shared::windef::HWND__,
    visible: Arc<Mutex<bool>>,
}

impl DebugConsole {
    pub fn new(title: &str) -> Self {
        unsafe {
            consoleapi::AllocConsole();

            let title: Vec<u16> = OsStr::new(title)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            wincon::SetConsoleTitleW(title.as_ptr());

            let hwnd = wincon::GetConsoleWindow();

            if !hwnd.is_null() {
                let hmenu = winuser::GetSystemMenu(hwnd, 0);
                if !hmenu.is_null() {
                    winuser::DeleteMenu(hmenu, winuser::SC_CLOSE as u32, winuser::MF_BYCOMMAND);
                }
                winuser::ShowWindow(hwnd, winuser::SW_HIDE);
            }

            Self {
                hwnd,
                visible: Arc::new(Mutex::new(false)),
            }
        }
    }

    pub fn toggle_visibility(&self) {
        if !self.hwnd.is_null() {
            let mut visible = self.visible.lock();
            *visible = !*visible;
            unsafe {
                winuser::ShowWindow(
                    self.hwnd,
                    if *visible {
                        winuser::SW_SHOW
                    } else {
                        winuser::SW_HIDE
                    },
                );
            }
        }
    }

    pub fn is_visible(&self) -> bool {
        *self.visible.lock()
    }
}
