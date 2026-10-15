//! Moving raw reports to and from a device, behind a small trait so tests can use a mock.

use std::ffi::CString;
use std::fmt::{self, Debug};

use hidapi::{HidApi, HidDevice};

use crate::catalog::VENDOR_ID;
use crate::error::{Error, Result};
use crate::report::REPORT_LEN;

/// hidapi prefixes every feature report with a report id; Razer devices use id zero.
const REPORT_ID: u8 = 0x00;
/// Length of a feature report on the wire: report id plus the 90-byte report.
const WIRE_LEN: usize = REPORT_LEN + 1;

/// One HID collection as seen by the enumeration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Collection {
    /// OS path used to open the collection.
    pub path: String,
    /// USB product id.
    pub pid: u16,
    /// USB interface number, or -1 when the platform does not report it.
    pub interface: i32,
    /// HID usage page of the collection.
    pub usage_page: u16,
    /// HID usage of the collection.
    pub usage: u16,
}

/// A channel that carries 90-byte reports to one HID collection and back.
pub trait Transport: Debug + Send {
    /// Sends a request as a feature report.
    fn send(&self, report: &[u8; REPORT_LEN]) -> Result<()>;
    /// Reads the current feature report.
    fn receive(&self) -> Result<[u8; REPORT_LEN]>;
}

/// The source of collections and transports; hidapi in production, a mock in tests.
pub trait Bus {
    /// Lists every HID collection of Razer devices.
    fn collections(&self) -> Result<Vec<Collection>>;
    /// Opens a collection returned by [`Bus::collections`].
    fn open(&self, collection: &Collection) -> Result<Box<dyn Transport>>;
}

/// The hidapi implementation of [`Bus`] over an already refreshed device list.
pub struct HidBus<'a> {
    api: &'a HidApi,
}

impl fmt::Debug for HidBus<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HidBus").finish_non_exhaustive()
    }
}

impl<'a> HidBus<'a> {
    /// Wraps a `HidApi` whose device list is up to date.
    pub fn new(api: &'a HidApi) -> Self {
        Self { api }
    }
}

impl Bus for HidBus<'_> {
    fn collections(&self) -> Result<Vec<Collection>> {
        Ok(self
            .api
            .device_list()
            .filter(|info| info.vendor_id() == VENDOR_ID)
            .map(|info| Collection {
                path: info.path().to_string_lossy().into_owned(),
                pid: info.product_id(),
                interface: info.interface_number(),
                usage_page: info.usage_page(),
                usage: info.usage(),
            })
            .collect())
    }

    fn open(&self, collection: &Collection) -> Result<Box<dyn Transport>> {
        let Ok(path) = CString::new(collection.path.as_str()) else {
            return Err(Error::InvalidResponse("device path contains a NUL byte"));
        };
        let device = self.api.open_path(&path)?;
        Ok(Box::new(HidTransport { device }))
    }
}

/// A [`Transport`] over an open hidapi device.
#[derive(Debug)]
struct HidTransport {
    device: HidDevice,
}

impl Transport for HidTransport {
    fn send(&self, report: &[u8; REPORT_LEN]) -> Result<()> {
        let mut wire = [0; WIRE_LEN];
        wire[0] = REPORT_ID;
        wire[1..].copy_from_slice(report);
        self.device.send_feature_report(&wire)?;
        Ok(())
    }

    fn receive(&self) -> Result<[u8; REPORT_LEN]> {
        let mut wire = [0; WIRE_LEN];
        wire[0] = REPORT_ID;
        let read = self.device.get_feature_report(&mut wire)?;
        if read != WIRE_LEN {
            return Err(Error::InvalidResponse("wrong report length"));
        }
        let mut report = [0; REPORT_LEN];
        report.copy_from_slice(&wire[1..]);
        Ok(report)
    }
}

#[cfg(test)]
pub(crate) mod mock {
    //! A fake mouse and a fake bus for tests of the device and discovery code.

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex, PoisonError};

    use super::{Bus, Collection, Transport};
    use crate::error::{Error, Result};
    use crate::report::{CRC_INDEX, Command, REPORT_LEN, Report, Status};

    /// What a mock transport answers to the requests it receives.
    #[derive(Clone, Debug)]
    pub struct Behavior {
        /// Serial bytes returned for the serial command; `None` answers "not supported".
        pub serial: Option<&'static str>,
        /// Raw battery byte returned for the battery command.
        pub battery: u8,
        /// Raw charging byte returned for the charging command.
        pub charging: u8,
        /// Fails every send with a transport error, like a wrong HID collection.
        pub broken: bool,
        /// Statuses returned, one per request, before answering normally.
        pub statuses: Vec<Status>,
        /// Number of answers with a broken checksum before answering normally.
        pub bad_checksums: usize,
        /// Number of answers to another command before answering normally.
        pub mismatches: usize,
    }

    impl Default for Behavior {
        fn default() -> Self {
            Self {
                serial: None,
                battery: 255,
                charging: 0,
                broken: false,
                statuses: Vec::new(),
                bad_checksums: 0,
                mismatches: 0,
            }
        }
    }

    /// A fake mouse: answers every request according to its [`Behavior`].
    #[derive(Debug)]
    pub struct MockTransport {
        behavior: Mutex<Behavior>,
        pending: Mutex<Option<[u8; REPORT_LEN]>>,
        serial_requests: Arc<AtomicUsize>,
    }

    impl MockTransport {
        pub fn new(behavior: Behavior) -> Self {
            Self::counting(behavior, Arc::default())
        }

        /// Creates a mouse that adds one to `serial_requests` for every serial command it gets.
        pub fn counting(behavior: Behavior, serial_requests: Arc<AtomicUsize>) -> Self {
            Self {
                behavior: Mutex::new(behavior),
                pending: Mutex::new(None),
                serial_requests,
            }
        }
    }

    impl Transport for MockTransport {
        fn send(&self, report: &[u8; REPORT_LEN]) -> Result<()> {
            let mut behavior = self.behavior.lock().unwrap_or_else(PoisonError::into_inner);
            if behavior.broken {
                return Err(hidapi::HidError::HidApiError {
                    message: "mock: wrong collection".to_owned(),
                }
                .into());
            }
            let request = Report::decode(report)?;
            let mut response = request.clone();
            response.status = Status::Successful;
            let command = (request.command_class, request.command_id);
            if command == (Command::BATTERY.class, Command::BATTERY.id) {
                response.arguments[1] = behavior.battery;
            } else if command == (Command::CHARGING.class, Command::CHARGING.id) {
                response.arguments[1] = behavior.charging;
            } else if command == (Command::SERIAL.class, Command::SERIAL.id) {
                self.serial_requests.fetch_add(1, Ordering::Relaxed);
                match behavior.serial {
                    Some(serial) => {
                        response.arguments[..serial.len()].copy_from_slice(serial.as_bytes());
                    }
                    None => response.status = Status::NotSupported,
                }
            }
            if !behavior.statuses.is_empty() {
                response.status = behavior.statuses.remove(0);
            }
            if behavior.mismatches > 0 {
                behavior.mismatches -= 1;
                response.command_id ^= 1;
            }
            let mut bytes = response.encode();
            if behavior.bad_checksums > 0 {
                behavior.bad_checksums -= 1;
                bytes[CRC_INDEX] ^= 0xFF;
            }
            *self.pending.lock().unwrap_or_else(PoisonError::into_inner) = Some(bytes);
            Ok(())
        }

        fn receive(&self) -> Result<[u8; REPORT_LEN]> {
            self.pending
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take()
                .ok_or(Error::Timeout)
        }
    }

    /// A fake bus: a list of collections, each with the behavior of the mouse behind it.
    #[derive(Debug, Default)]
    pub struct MockBus {
        entries: Vec<(Collection, Behavior)>,
        serial_requests: Arc<AtomicUsize>,
    }

    impl MockBus {
        /// Adds a collection of the given product id and usage pair.
        pub fn add(&mut self, path: &str, pid: u16, usage: (u16, u16), behavior: Behavior) {
            let collection = Collection {
                path: path.to_owned(),
                pid,
                interface: 0,
                usage_page: usage.0,
                usage: usage.1,
            };
            self.entries.push((collection, behavior));
        }

        /// Unplugs the collection with the given path.
        pub fn remove(&mut self, path: &str) {
            self.entries
                .retain(|(collection, _)| collection.path != path);
        }

        /// Changes how the collection with the given path answers from the next open on.
        pub fn set_behavior(&mut self, path: &str, behavior: Behavior) {
            if let Some((_, current)) = self
                .entries
                .iter_mut()
                .find(|(collection, _)| collection.path == path)
            {
                *current = behavior;
            }
        }

        /// How many serial commands the transports of this bus have received so far.
        pub fn serial_requests(&self) -> usize {
            self.serial_requests.load(Ordering::Relaxed)
        }
    }

    impl Bus for MockBus {
        fn collections(&self) -> Result<Vec<Collection>> {
            Ok(self
                .entries
                .iter()
                .map(|(collection, _)| collection.clone())
                .collect())
        }

        fn open(&self, collection: &Collection) -> Result<Box<dyn Transport>> {
            let (_, behavior) = self
                .entries
                .iter()
                .find(|(candidate, _)| candidate == collection)
                .ok_or(Error::Timeout)?;
            Ok(Box::new(MockTransport::counting(
                behavior.clone(),
                Arc::clone(&self.serial_requests),
            )))
        }
    }
}
