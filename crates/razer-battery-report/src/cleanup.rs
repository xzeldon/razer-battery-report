//! `cleanup`: removes everything the app stored outside its own executable.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::Context as _;

use crate::{paths, platform, tray};

/// Removes the autostart entry, the notification registration and the app's files. With `all`
/// the settings and the logs go too. Every step runs even when an earlier one failed; the
/// failures are logged and make the result an error. Does nothing while the tray runs.
pub fn run(all: bool) -> anyhow::Result<()> {
    if tray::running()? {
        anyhow::bail!("the tray is running; exit it first");
    }
    let mut failed = 0_usize;
    let mut check = |step: anyhow::Result<()>| {
        if let Err(error) = step {
            log::error!("cleanup: {error:#}");
            failed += 1;
        }
    };
    check(platform::set_autostart(false));
    check(platform::unregister_notifications());
    for file in [
        paths::lock_file(),
        paths::notification_icon(),
        paths::licenses_file(),
    ] {
        check(remove_file(file.as_deref()));
    }
    if all {
        check(remove_dir_all(paths::log_dir().as_deref()));
        check(remove_file(paths::config_file().as_deref()));
    }
    let config_dir = paths::config_file().and_then(|file| file.parent().map(PathBuf::from));
    for dir in [paths::data_dir(), config_dir] {
        check(remove_empty_dir(dir.as_deref()));
    }
    match failed {
        0 => Ok(()),
        _ => anyhow::bail!("{failed} cleanup steps failed"),
    }
}

/// Removes a file; a missing file or an unknown location is fine.
fn remove_file(path: Option<&Path>) -> anyhow::Result<()> {
    let Some(path) = path else {
        return Ok(());
    };
    ignore_missing(fs::remove_file(path))
        .with_context(|| format!("cannot remove {}", path.display()))
}

/// Removes a directory with everything in it; a missing one is fine.
fn remove_dir_all(path: Option<&Path>) -> anyhow::Result<()> {
    let Some(path) = path else {
        return Ok(());
    };
    ignore_missing(fs::remove_dir_all(path))
        .with_context(|| format!("cannot remove {}", path.display()))
}

/// Removes a directory only when it is empty; a missing or non-empty one stays as it is.
fn remove_empty_dir(path: Option<&Path>) -> anyhow::Result<()> {
    let Some(path) = path else {
        return Ok(());
    };
    match fs::remove_dir(path) {
        Err(error) if error.kind() == ErrorKind::DirectoryNotEmpty => Ok(()),
        result => {
            ignore_missing(result).with_context(|| format!("cannot remove {}", path.display()))
        }
    }
}

fn ignore_missing(result: std::io::Result<()>) -> std::io::Result<()> {
    match result {
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        result => result,
    }
}
