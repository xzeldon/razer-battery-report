//! What can go wrong while checking for and installing an update, and how each case is told to
//! the user.

use std::fmt;
use std::io::{self, ErrorKind};
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::Command;

const STATUS_LINE_MAX_CHARS: usize = 80;
const REASON_MAX_CHARS: usize = 120;
const BYTES_PER_MB: u64 = 1024 * 1024;
const SECONDS_PER_MINUTE: u64 = 60;
const RETRY_FROM_MENU: &str = " Retry from the tray menu.";

const RETRY: &[Command] = &[Command::Retry];
const OPEN_LOG: &[Command] = &[Command::OpenLog];
const DOWNLOAD_MANUALLY: &[Command] = &[Command::DownloadManually];
const RETRY_OR_DOWNLOAD: &[Command] = &[Command::Retry, Command::DownloadManually];
const DOWNLOAD_OR_LOG: &[Command] = &[Command::DownloadManually, Command::OpenLog];
const INSTALLER_LOG_OR_DOWNLOAD: &[Command] =
    &[Command::OpenInstallerLog, Command::DownloadManually];

/// A reason an update did not happen. It is stored in the state file, so the menu can show it
/// again after a restart.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateError {
    Offline,
    Tls,
    RateLimited {
        minutes: u64,
    },
    GitHubDown,
    /// The release has no usable download for this computer.
    NoAsset,
    Interrupted,
    NoSpace {
        needed_mb: u64,
    },
    CannotSave {
        path: String,
    },
    /// The download does not match the digest GitHub published.
    Damaged,
    /// The download vanished or was locked right after it was written.
    Blocked,
    NoWriteAccess {
        dir: String,
    },
    InstallerBlocked {
        os_error: String,
    },
    /// The installer ran and the old version is still there.
    InstallerFailed {
        reason: String,
    },
    /// The portable exe was not replaced, or the replaced one did not start.
    NotReplaced {
        reason: String,
    },
}

impl UpdateError {
    /// An installer failure with a reason cut down to fit a toast.
    pub fn installer_failed(reason: &str) -> Self {
        let reason = reason.trim().trim_end_matches('.');
        Self::InstallerFailed {
            reason: truncate(reason, REASON_MAX_CHARS),
        }
    }

    /// A failure to replace the portable exe, with a reason cut down to fit a toast.
    pub fn not_replaced(reason: &str) -> Self {
        let reason = reason.trim().trim_end_matches('.');
        Self::NotReplaced {
            reason: truncate(reason, REASON_MAX_CHARS),
        }
    }

    pub fn from_request(error: &ureq::Error) -> Self {
        match error {
            ureq::Error::HostNotFound
            | ureq::Error::Timeout(_)
            | ureq::Error::ConnectionFailed
            | ureq::Error::Io(_) => Self::Offline,
            ureq::Error::Tls(_) | ureq::Error::NativeTls(_) => Self::Tls,
            _ => Self::GitHubDown,
        }
    }

    /// Classifies an HTTP status. `None` for a success. `remaining` and `reset` are GitHub's
    /// `X-RateLimit-*` headers, `now` is in unix seconds.
    pub fn from_status(
        status: u16,
        remaining: Option<&str>,
        reset: Option<&str>,
        now: u64,
    ) -> Option<Self> {
        match status {
            200..=299 => None,
            403 | 429 if remaining == Some("0") => {
                let minutes =
                    reset
                        .and_then(|reset| reset.parse::<u64>().ok())
                        .map_or(1, |reset| {
                            reset
                                .saturating_sub(now)
                                .div_ceil(SECONDS_PER_MINUTE)
                                .max(1)
                        });
                Some(Self::RateLimited { minutes })
            }
            404 => Some(Self::NoAsset),
            _ => Some(Self::GitHubDown),
        }
    }

    /// Classifies a failure to write the download of `needed` bytes into `dir`.
    pub fn from_save(error: &io::Error, dir: &Path, needed: u64) -> Self {
        match error.kind() {
            ErrorKind::StorageFull => Self::NoSpace {
                needed_mb: needed.div_ceil(BYTES_PER_MB),
            },
            _ => Self::CannotSave {
                path: dir.display().to_string(),
            },
        }
    }

    /// Whether the error came after a release was found, while downloading or installing it.
    pub fn is_install(&self) -> bool {
        !matches!(
            self,
            Self::Offline | Self::Tls | Self::RateLimited { .. } | Self::GitHubDown | Self::NoAsset
        )
    }

    /// The first line of the toast.
    pub fn title(&self) -> &'static str {
        match self {
            Self::Offline => "Cannot reach GitHub",
            Self::Tls => "Secure connection to GitHub failed",
            Self::RateLimited { .. } => "GitHub limits update checks",
            Self::GitHubDown => "GitHub is not responding",
            Self::NoAsset => "No download for this computer",
            Self::Interrupted => "Update failed: the download stopped",
            Self::NoSpace { .. } => "Update failed: not enough disk space",
            Self::CannotSave { .. } => "Update failed: cannot save the download",
            Self::Damaged => "Update failed: the download was damaged",
            Self::Blocked => "Update failed: your antivirus blocked it",
            Self::NoWriteAccess { .. } => "Update failed: no write access to the app folder",
            Self::InstallerBlocked { .. } => "Update failed: Windows blocked the installer",
            Self::InstallerFailed { .. } => "Update failed: the installer stopped",
            Self::NotReplaced { .. } => "Update failed: the app was not replaced",
        }
    }

    /// The toast text under the title. `current` is the running version.
    pub fn body(&self, current: &str, buttons: bool) -> String {
        let unchanged = format!("Nothing changed, you still have {current}.");
        let mut body = match self {
            Self::Offline => "Check your internet connection.".to_owned(),
            Self::Tls => "An antivirus or proxy may be blocking it.".to_owned(),
            Self::RateLimited { minutes: 1 } => "Try again in 1 minute.".to_owned(),
            Self::RateLimited { minutes } => format!("Try again in {minutes} minutes."),
            Self::GitHubDown => "The app will check again later.".to_owned(),
            Self::NoAsset => format!("You still have {current}."),
            Self::Interrupted => format!("The connection was lost. {unchanged}"),
            Self::NoSpace { needed_mb } => format!("It needs {needed_mb} MB. {unchanged}"),
            Self::CannotSave { .. } => format!("Access denied. {unchanged}"),
            Self::Damaged => format!("It was deleted. {unchanged}"),
            Self::Blocked | Self::InstallerBlocked { .. } => unchanged,
            Self::NoWriteAccess { .. } => format!(
                "Move the app to a folder you own or use the installer. You still have {current}."
            ),
            Self::InstallerFailed { reason } | Self::NotReplaced { reason } => {
                format!("{reason}. You still have {current}.")
            }
        };
        if !buttons && self.actions().contains(&Command::Retry) {
            body.push_str(RETRY_FROM_MENU);
        }
        body
    }

    /// The one-line description in the menu; it may name a path or an OS message.
    pub fn status_line(&self) -> String {
        let line = match self {
            Self::CannotSave { path } => format!("Update failed: cannot save to {path}"),
            Self::NoWriteAccess { dir } => format!("Update failed: no write access to {dir}"),
            Self::InstallerBlocked { os_error } => {
                format!("Update failed: Windows blocked the installer ({os_error})")
            }
            Self::InstallerFailed { reason } => format!("The installer stopped: {reason}"),
            Self::NotReplaced { reason } => format!("The app was not replaced: {reason}"),
            _ => self.title().to_owned(),
        };
        truncate(&line, STATUS_LINE_MAX_CHARS)
    }

    /// What the user can do about it.
    pub fn actions(&self) -> &'static [Command] {
        match self {
            Self::Offline | Self::GitHubDown | Self::Interrupted | Self::NoSpace { .. } => RETRY,
            Self::Tls | Self::CannotSave { .. } => OPEN_LOG,
            Self::RateLimited { .. } => &[],
            Self::NoAsset | Self::Blocked | Self::NoWriteAccess { .. } => DOWNLOAD_MANUALLY,
            Self::Damaged => RETRY_OR_DOWNLOAD,
            Self::InstallerBlocked { .. } | Self::NotReplaced { .. } => DOWNLOAD_OR_LOG,
            Self::InstallerFailed { .. } => INSTALLER_LOG_OR_DOWNLOAD,
        }
    }
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.status_line())
    }
}

/// `text` cut to `max` characters, with an ellipsis when something was cut.
fn truncate(text: &str, max: usize) -> String {
    match text.chars().count() > max {
        true => text.chars().take(max - 1).chain(Some('…')).collect(),
        false => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_waits_whole_minutes() {
        let status = |reset| UpdateError::from_status(403, Some("0"), reset, 1000);
        assert_eq!(
            status(Some("1090")),
            Some(UpdateError::RateLimited { minutes: 2 })
        );
        assert_eq!(
            status(Some("900")),
            Some(UpdateError::RateLimited { minutes: 1 })
        );
        assert_eq!(status(None), Some(UpdateError::RateLimited { minutes: 1 }));
    }
}
