//! What the updater remembers between runs, stored next to the other per-user state.

use std::fs;
use std::io::ErrorKind;

use anyhow::Context as _;
use serde::{Deserialize, Serialize};

use super::error::UpdateError;
use super::release::Available;
use crate::paths;

/// The updater's bookkeeping. Missing fields take their defaults.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub checked_at: Option<u64>,
    /// Why the last check or install failed; shown in the menu until the next success.
    pub last_error: Option<UpdateError>,
    pub available: Option<Available>,
    /// Version being installed; set just before the app exits for the installer.
    pub pending: Option<String>,
    /// Lets the next start tell about an update done while the app was closed.
    pub last_version: Option<String>,
}

impl State {
    /// Reads the state file; a missing or corrupt one gives the defaults.
    pub fn load() -> Self {
        let Some(path) = paths::state_file() else {
            return Self::default();
        };
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => return Self::default(),
            Err(error) => {
                log::warn!("updater: cannot read {}: {error}", path.display());
                return Self::default();
            }
        };
        serde_json::from_str(&text).unwrap_or_else(|error| {
            log::warn!("updater: {}: {error}", path.display());
            Self::default()
        })
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = paths::state_file().context("no per-user data directory")?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).context("cannot create data directory")?;
        }
        let text = serde_json::to_string_pretty(self).context("cannot serialise state")?;
        fs::write(&path, text).with_context(|| format!("cannot write {}", path.display()))
    }
}
