//! One USB interface of a mouse and the queries it answers.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use crate::catalog::{Entry, Link, Support};
use crate::error::{Error, Result};
use crate::report::{Command, REPORT_LEN, Report, Status};
use crate::transport::Transport;

/// How many times a command is sent before giving up on a busy or silent mouse; the same count
/// as openrazer's `razer_send_payload`.
const MAX_TRIES: u32 = 5;
/// Pause between two tries; the same 10 ms as openrazer's `razer_send_payload`.
const RETRY_DELAY: Duration = Duration::from_millis(10);
/// Raw battery byte that stands for 100 percent.
const RAW_BATTERY_FULL: u16 = 255;
/// Highest valid percentage.
const PERCENT_MAX: u8 = 100;
/// Index of the argument that carries the battery and charging value.
const VALUE_ARGUMENT: usize = 1;

/// A percentage between 0 and 100.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Percent(u8);

impl Percent {
    /// The value, 0..=100.
    pub fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for Percent {
    type Error = PercentOutOfRange;

    fn try_from(value: u8) -> std::result::Result<Self, Self::Error> {
        match value <= PERCENT_MAX {
            true => Ok(Self(value)),
            false => Err(PercentOutOfRange(value)),
        }
    }
}

/// The error of converting a number above 100 into a [`Percent`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0} is not a percentage between 0 and 100")]
pub struct PercentOutOfRange(pub u8);

/// A battery reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Battery {
    /// Charge level.
    pub level: Percent,
    /// Whether the mouse is charging; `None` when the model cannot report it.
    pub charging: Option<bool>,
}

/// One USB interface of a mouse, known from the catalog and reachable through one or more
/// HID collections.
#[derive(Debug)]
pub struct Endpoint {
    entry: &'static Entry,
    path: String,
    transports: Vec<Box<dyn Transport>>,
    /// Index of the transport that answered last; tried first next time.
    active: AtomicUsize,
    retry_delay: Duration,
}

impl Endpoint {
    /// Creates an endpoint over the candidate transports, most likely one first. The transport
    /// that answers is remembered, so a wrong HID collection is skipped only once.
    pub(crate) fn new(
        entry: &'static Entry,
        path: String,
        transports: Vec<Box<dyn Transport>>,
    ) -> Self {
        Self {
            entry,
            path,
            transports,
            active: AtomicUsize::new(0),
            retry_delay: RETRY_DELAY,
        }
    }

    /// USB product id of this endpoint.
    pub fn pid(&self) -> u16 {
        self.entry.pid
    }

    /// Kind of connection of this endpoint.
    pub fn link(&self) -> Link {
        self.entry.link
    }

    /// Catalog name of this product id, including the connection suffix.
    pub fn name(&self) -> &'static str {
        self.entry.name
    }

    /// The catalog entry of this endpoint.
    pub fn entry(&self) -> &'static Entry {
        self.entry
    }

    /// OS path of the endpoint, unique per endpoint and stable while it stays plugged in.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Reads the battery level and, where the model supports it, the charging state.
    ///
    /// # Errors
    ///
    /// A sleeping mouse gives [`Error::Timeout`]; see [`Error`] for the rest.
    pub fn battery(&self) -> Result<Battery> {
        let response = self.query(Command::BATTERY, self.entry.tid)?;
        let level = percent_from_raw(response.arguments[VALUE_ARGUMENT]);
        let charging = match self.entry.charging {
            Support::Query => Some(self.charging()?),
            Support::Unsupported => None,
        };
        Ok(Battery { level, charging })
    }

    /// Reads whether the mouse is charging.
    ///
    /// # Errors
    ///
    /// Same as [`Endpoint::battery`].
    pub fn charging(&self) -> Result<bool> {
        let response = self.query(Command::CHARGING, self.entry.tid)?;
        Ok(response.arguments[VALUE_ARGUMENT] != 0)
    }

    /// Reads the serial number the mouse reports over the protocol.
    ///
    /// # Errors
    ///
    /// [`Error::NotSupported`] when the catalog says the model has none,
    /// [`Error::InvalidResponse`] when the answer is not a plain `[0-9A-Z]+` string, and the
    /// errors of [`Endpoint::battery`] otherwise.
    pub fn serial(&self) -> Result<String> {
        if self.entry.serial == Support::Unsupported {
            return Err(Error::NotSupported);
        }
        let tid = self.entry.serial_tid.unwrap_or(self.entry.tid);
        let response = self.query(Command::SERIAL, tid)?;
        parse_serial(&response.arguments[..usize::from(Command::SERIAL.size)])
            .ok_or(Error::InvalidResponse("malformed serial number"))
    }

    /// Sends `command` until the mouse answers with success, retrying like openrazer while it is
    /// busy, silent or garbled (a pairing dongle sends garbage).
    fn query(&self, command: Command, tid: u8) -> Result<Report> {
        let request = Report::request(tid, command);
        let mut garbled = None;
        for attempt in 1..=MAX_TRIES {
            if attempt > 1 {
                thread::sleep(self.retry_delay);
            }
            let response = match self.exchange(&request) {
                Ok(response) => response,
                Err(error @ Error::InvalidResponse(_)) => {
                    log::debug!("hid: {} answered garbage: {error}", self.entry.name);
                    garbled = Some(error);
                    continue;
                }
                Err(error) => return Err(error),
            };
            if response.remaining_packets != request.remaining_packets
                || response.command_class != request.command_class
                || response.command_id != request.command_id
            {
                log::debug!("hid: {} answered another request", self.entry.name);
                garbled = Some(Error::InvalidResponse("response does not match request"));
                continue;
            }
            garbled = None;
            match response.status {
                Status::Successful => return Ok(response),
                Status::NotSupported => return Err(Error::NotSupported),
                Status::Failure => return Err(Error::Failure),
                Status::New => return Err(Error::InvalidResponse("response has a new status")),
                // openrazer treats Busy as success; we retry, because a busy reply to a read
                // carries no guaranteed payload and could read as 0% and fire a false alert.
                status @ (Status::Busy | Status::NoResponse) => {
                    log::debug!(
                        "hid: {} answered {status:?} to {:#04x}/{:#04x} (try {attempt} of {MAX_TRIES})",
                        self.entry.name,
                        request.command_class,
                        request.command_id
                    );
                }
            }
        }
        Err(garbled.unwrap_or(Error::Timeout))
    }

    /// One request and one response. On a transport failure the other HID collections of the
    /// interface are tried; the one that works is remembered.
    fn exchange(&self, request: &Report) -> Result<Report> {
        let bytes = request.encode();
        let count = self.transports.len();
        let first = self.active.load(Ordering::Relaxed);
        let mut last_error = None;
        for offset in 0..count {
            let index = (first + offset) % count;
            match self.round_trip(&*self.transports[index], &bytes) {
                Ok(response) => {
                    self.active.store(index, Ordering::Relaxed);
                    return Ok(response);
                }
                Err(error @ Error::Transport(_)) => {
                    log::debug!(
                        "hid: {} collection {index} of {} failed: {error}",
                        self.entry.name,
                        self.path
                    );
                    last_error = Some(error);
                }
                Err(error) => return Err(error),
            }
        }
        Err(last_error.unwrap_or(Error::InvalidResponse("endpoint has no transport")))
    }

    /// Sends a request, waits the catalog's response time and decodes the answer.
    fn round_trip(&self, transport: &dyn Transport, bytes: &[u8; REPORT_LEN]) -> Result<Report> {
        transport.send(bytes)?;
        thread::sleep(Duration::from_micros(u64::from(self.entry.wait_us)));
        Report::decode(&transport.receive()?)
    }
}

/// Converts the raw 0..=255 battery byte into a rounded percentage.
fn percent_from_raw(raw: u8) -> Percent {
    // Integer form of round(raw / 255 * 100); exact halves cannot occur.
    let rounded =
        (u16::from(raw) * u16::from(PERCENT_MAX) + RAW_BATTERY_FULL / 2) / RAW_BATTERY_FULL;
    Percent(u8::try_from(rounded).unwrap_or(PERCENT_MAX))
}

/// Extracts a serial from the argument bytes: ASCII up to the first NUL, `[0-9A-Z]+` only.
fn parse_serial(arguments: &[u8]) -> Option<String> {
    let end = arguments
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(arguments.len());
    let serial = &arguments[..end];
    let valid = !serial.is_empty()
        && serial
            .iter()
            .all(|byte| byte.is_ascii_digit() || byte.is_ascii_uppercase());
    match valid {
        true => String::from_utf8(serial.to_vec()).ok(),
        false => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;
    use crate::report::fixtures::SERIAL_ARGUMENTS;
    use crate::transport::mock::{Behavior, MockTransport};

    /// `DeathAdder` V4 Pro, wired: tid 0x1f, charging and serial supported.
    const WIRED_PID: u16 = 0x00BE;

    fn endpoint(pid: u16, transports: Vec<Box<dyn Transport>>) -> Endpoint {
        let entry = catalog::find(pid).expect("pid is in the catalog");
        let mut endpoint = Endpoint::new(entry, "mock".to_owned(), transports);
        endpoint.retry_delay = Duration::ZERO;
        endpoint
    }

    fn mock(behavior: Behavior) -> Box<dyn Transport> {
        Box::new(MockTransport::new(behavior))
    }

    #[test]
    fn battery_rounding() {
        assert_eq!(percent_from_raw(0).get(), 0);
        assert_eq!(percent_from_raw(255).get(), 100);
        assert_eq!(percent_from_raw(128).get(), 50);
        for raw in 0..=255u8 {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // The float formula of the old app; the integer one must agree everywhere.
            let expected = (f32::from(raw) / 255.0 * 100.0).round() as u8;
            assert_eq!(percent_from_raw(raw).get(), expected, "raw {raw}");
        }
    }

    #[test]
    fn parses_real_serial() {
        assert_eq!(
            parse_serial(&[SERIAL_ARGUMENTS.as_slice(), &[0; 7]].concat()).as_deref(),
            Some("PM2542H35104898")
        );
    }

    #[test]
    fn rejects_malformed_serials() {
        assert_eq!(parse_serial(&[0; 22]), None);
        assert_eq!(parse_serial(b"abc123"), None);
        assert_eq!(parse_serial(b"AB-12"), None);
        assert_eq!(parse_serial(&[0xC3, 0xA9]), None);
    }

    #[test]
    fn reads_battery_and_charging() {
        let behavior = Behavior {
            battery: 128,
            charging: 1,
            ..Behavior::default()
        };
        let battery = endpoint(WIRED_PID, vec![mock(behavior)]).battery().unwrap();
        assert_eq!(battery.level.get(), 50);
        assert_eq!(battery.charging, Some(true));
    }

    #[test]
    fn charging_is_none_when_unsupported() {
        let entry = catalog::CATALOG
            .iter()
            .find(|entry| entry.charging == Support::Unsupported)
            .expect("the catalog has a model without charging");
        let endpoint = endpoint(entry.pid, vec![mock(Behavior::default())]);
        assert_eq!(endpoint.battery().unwrap().charging, None);
    }

    #[test]
    fn retries_while_busy() {
        let behavior = Behavior {
            statuses: vec![Status::Busy, Status::NoResponse],
            ..Behavior::default()
        };
        let endpoint = endpoint(WIRED_PID, vec![mock(behavior)]);
        assert_eq!(endpoint.battery().unwrap().level.get(), 100);
    }

    #[test]
    fn times_out_after_all_tries() {
        let behavior = Behavior {
            statuses: vec![Status::NoResponse; 20],
            ..Behavior::default()
        };
        let endpoint = endpoint(WIRED_PID, vec![mock(behavior)]);
        assert!(matches!(endpoint.battery(), Err(Error::Timeout)));
    }

    #[test]
    fn retries_garbled_answers() {
        let behavior = Behavior {
            bad_checksums: 1,
            mismatches: 1,
            ..Behavior::default()
        };
        let endpoint = endpoint(WIRED_PID, vec![mock(behavior)]);
        assert_eq!(endpoint.battery().unwrap().level.get(), 100);
    }

    #[test]
    fn garbled_after_all_tries_is_an_invalid_response() {
        let behavior = Behavior {
            mismatches: 20,
            ..Behavior::default()
        };
        let endpoint = endpoint(WIRED_PID, vec![mock(behavior)]);
        assert!(matches!(endpoint.battery(), Err(Error::InvalidResponse(_))));
    }

    #[test]
    fn falls_back_to_another_collection_and_remembers_it() {
        let broken = Behavior {
            broken: true,
            ..Behavior::default()
        };
        let endpoint = endpoint(WIRED_PID, vec![mock(broken), mock(Behavior::default())]);
        assert!(endpoint.battery().is_ok());
        assert_eq!(endpoint.active.load(Ordering::Relaxed), 1);
    }
}
