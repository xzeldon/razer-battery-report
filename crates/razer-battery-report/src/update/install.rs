//! Handing the downloaded file over: to the installer, or over the running executable.

use std::fs;
use std::io::{self, ErrorKind};
use std::path::Path;

use super::Handoff;
use super::error::UpdateError;
use crate::platform;

pub const INSTALL_LOG: &str = "install.log";

/// Starts the installer; the caller has to exit so it can replace the app.
pub fn launch_installer(setup: &Path, dir: &Path) -> Result<Handoff, UpdateError> {
    platform::launch_installer(setup, &dir.join(INSTALL_LOG)).map_err(|error| {
        log::warn!("updater: cannot start the installer: {error}");
        UpdateError::InstallerBlocked {
            os_error: error.to_string(),
        }
    })?;
    Ok(Handoff::Exit)
}

/// Replaces the running executable with the downloaded one; the caller has to start the new
/// one after releasing the tray lock.
pub fn replace_executable(file: &Path) -> Result<Handoff, UpdateError> {
    self_replace::self_replace(file).map_err(|error| {
        log::warn!("updater: cannot replace the executable: {error}");
        match error.kind() {
            ErrorKind::PermissionDenied => UpdateError::NoWriteAccess {
                dir: executable_dir(),
            },
            _ => UpdateError::not_replaced(&error.to_string()),
        }
    })?;
    Ok(Handoff::Relaunch)
}

/// Deletes the downloads left in `dir`, keeping the installer log. Files still in use stay for
/// the next start.
pub fn remove_downloads(dir: &Path) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return,
        Err(error) => {
            log::warn!("updater: cannot list {}: {error}", dir.display());
            return;
        }
    };
    for entry in entries.flatten() {
        if entry.file_name() == INSTALL_LOG {
            continue;
        }
        if let Err(error) = remove(&entry.path()) {
            log::debug!("updater: cannot remove {}: {error}", entry.path().display());
        }
    }
}

fn remove(path: &Path) -> io::Result<()> {
    match path.is_dir() {
        true => fs::remove_dir_all(path),
        false => fs::remove_file(path),
    }
}

fn executable_dir() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.display().to_string()))
        .unwrap_or_default()
}
