//! The updater's part of the tray menu.

use std::time::Duration;

use tray_icon::menu::{CheckMenuItem, MenuItem, PredefinedMenuItem, Submenu};

use super::{Busy, Command, Updater};
use crate::config::UpdateMode;

const MODES: [UpdateMode; 3] = [UpdateMode::Auto, UpdateMode::Notify, UpdateMode::Off];
const MINUTE: u64 = 60;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;

impl Updater {
    /// The `Updates` submenu: the status, what to do about an error, the mode and a manual check.
    pub fn submenu(&self, mode: UpdateMode) -> tray_icon::menu::Result<Submenu> {
        let submenu = Submenu::new("Updates", true);
        submenu.append(&MenuItem::new(self.status_line(), false, None))?;
        if let (None, Some(error)) = (&self.busy, &self.state.last_error) {
            for command in error.actions() {
                submenu.append(&item(*command))?;
            }
        }
        submenu.append(&PredefinedMenuItem::separator())?;
        for candidate in MODES {
            let command = Command::SetMode(candidate);
            let checked = candidate == mode;
            submenu.append(&CheckMenuItem::with_id(
                command.id(),
                command.label(),
                true,
                checked,
                None,
            ))?;
        }
        submenu.append(&PredefinedMenuItem::separator())?;
        submenu.append(&item(Command::CheckNow))?;
        Ok(submenu)
    }

    /// The top-level entry that installs the available release, or tells that it downloads.
    pub fn install_item(&self) -> Option<MenuItem> {
        if let Some(Busy::Downloading) = self.busy {
            return Some(MenuItem::new("Downloading update…", false, None));
        }
        let available = self.state.available.as_ref()?;
        let text = format!("Update to v{}", available.version);
        Some(MenuItem::with_id(Command::Install.id(), text, true, None))
    }

    fn status_line(&self) -> String {
        let state = &self.state;
        match (
            &self.busy,
            &state.last_error,
            &state.available,
            state.checked_at,
        ) {
            (Some(Busy::Checking), ..) => "Checking for updates…".to_owned(),
            (Some(Busy::Downloading), ..) => "Downloading update…".to_owned(),
            (None, Some(error), ..) => error.status_line(),
            (None, None, Some(available), _) => {
                format!("Version {} is available", available.version)
            }
            (None, None, None, Some(at)) => {
                let age = Duration::from_secs(super::now().saturating_sub(at));
                format!("Up to date · checked {}", ago(age))
            }
            (None, None, None, None) => "Not checked yet".to_owned(),
        }
    }
}

fn item(command: Command) -> MenuItem {
    MenuItem::with_id(command.id(), command.label(), true, None)
}

fn ago(age: Duration) -> String {
    let seconds = age.as_secs();
    let (count, unit) = match seconds {
        0..MINUTE => return "just now".to_owned(),
        MINUTE..HOUR => (seconds / MINUTE, "minute"),
        HOUR..DAY => (seconds / HOUR, "hour"),
        _ => (seconds / DAY, "day"),
    };
    let plural = match count == 1 {
        true => "",
        false => "s",
    };
    format!("{count} {unit}{plural} ago")
}
