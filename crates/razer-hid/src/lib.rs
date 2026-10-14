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
    api: Mutex<HidApi>,
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
            api: Mutex::new(HidApi::new()?),
        })
    }

    /// Enumerates the connected Razer mice from the catalog, one [`Device`] per physical mouse.
    /// Every call rescans the bus, so plugged and unplugged mice show up.
    ///
    /// # Errors
    ///
    /// Fails when the device list cannot be read.
    pub fn devices(&self) -> Result<Vec<Device>> {
        let mut api = self.api.lock().unwrap_or_else(PoisonError::into_inner);
        api.refresh_devices()?;
        discovery::discover(&transport::HidBus::new(&api))
    }
}
