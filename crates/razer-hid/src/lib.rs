//! Razer HID protocol, device catalog, discovery and battery queries for wireless Razer mice.
//!
//! ```no_run
//! let hid = razer_hid::Hid::new()?;
//! for device in hid.devices()? {
//!     let battery = device.battery()?;
//!     println!("{}: {}%", device.model(), battery.level.get());
//! }
//! # Ok::<(), razer_hid::Error>(())
//! ```

#![warn(missing_docs, clippy::missing_errors_doc)]

pub mod catalog;
mod device;
mod discovery;
mod error;
mod report;
mod transport;

use std::fmt;
use std::sync::{Mutex, PoisonError};

use hidapi::HidApi;

pub use catalog::{Entry, Link, Support};
pub use device::{Battery, Endpoint, Percent, PercentOutOfRange};
pub use discovery::Device;
pub use error::{Error, Result};

/// A handle on the system's HID subsystem. Create one and keep it; ask it for devices whenever
/// the list of connected mice is needed.
pub struct Hid {
    state: Mutex<State>,
}

/// What `Hid` keeps between calls, under one lock so a whole scan sees a consistent state.
struct State {
    api: HidApi,
    serials: discovery::SerialCache,
}

impl fmt::Debug for Hid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Hid").finish_non_exhaustive()
    }
}

impl Hid {
    /// Initialises the HID subsystem.
    ///
    /// # Errors
    ///
    /// Fails when hidapi cannot be initialised.
    pub fn new() -> Result<Self> {
        Ok(Self {
            state: Mutex::new(State {
                api: HidApi::new()?,
                serials: discovery::SerialCache::default(),
            }),
        })
    }

    /// One [`Device`] per connected physical mouse. Rescans the bus on every call; serials are
    /// read once per endpoint, so calls are cheap. A sleeping mouse comes without a serial until
    /// it answers.
    ///
    /// # Errors
    ///
    /// Fails when the device list cannot be read.
    pub fn devices(&self) -> Result<Vec<Device>> {
        let mut guard = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let state = &mut *guard;
        state.api.refresh_devices()?;
        discovery::discover(&transport::HidBus::new(&state.api), &mut state.serials)
    }
}
