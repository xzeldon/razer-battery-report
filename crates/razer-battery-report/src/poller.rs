//! The background thread that keeps the list of readings up to date.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use razer_hid::{Battery, Device, Error, Hid};

use crate::monitor::{DeviceKey, Reading};

/// How often the bus is scanned for plugged and unplugged mice.
const SCAN_INTERVAL: Duration = Duration::from_secs(5);
/// How often the battery of every mouse is read again.
const BATTERY_INTERVAL: Duration = Duration::from_mins(5);

/// One mouse found by a scan, with the outcome of reading its battery.
struct Scanned {
    key: DeviceKey,
    /// Paths of the mouse's endpoints. A mouse that was asleep without a serial was keyed by
    /// one of them.
    paths: Vec<String>,
    model: &'static str,
    serial: Option<String>,
    probe: Probe,
}

/// What happened when the battery of a mouse was read.
enum Probe {
    /// The battery was not read in this scan.
    Skipped,
    /// The mouse answered.
    Battery(Battery),
    /// The mouse did not answer; it is probably asleep.
    Asleep,
    /// Reading failed for another reason.
    Failed,
}

/// Starts the polling thread and returns the channel it reports on. The bus is scanned every
/// five seconds. The battery of a mouse is read when it appears, on every scan while it is asleep
/// or has no reading yet, and for all mice every five minutes. The first list is sent after the
/// first scan, even when empty; later lists are sent only when they differ from the previous
/// one. The thread ends when the receiver is dropped.
pub fn spawn(hid: Hid) -> Receiver<Vec<Reading>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || run(&hid, &sender));
    receiver
}

fn run(hid: &Hid, sender: &Sender<Vec<Reading>>) {
    let mut readings: Vec<Reading> = Vec::new();
    let mut last_sent: Option<Vec<Reading>> = None;
    let mut last_battery_read: Option<Instant> = None;
    loop {
        let read_all = last_battery_read.is_none_or(|at| at.elapsed() >= BATTERY_INTERVAL);
        match hid.devices() {
            Ok(devices) => {
                let scanned = devices
                    .iter()
                    .map(|device| scan(device, &readings, read_all))
                    .collect();
                readings = merge(&readings, scanned);
                if read_all {
                    last_battery_read = Some(Instant::now());
                }
            }
            Err(error) => log::warn!("poller: cannot list devices: {error}"),
        }
        if last_sent.as_ref() != Some(&readings) {
            if sender.send(readings.clone()).is_err() {
                return;
            }
            last_sent = Some(readings.clone());
        }
        thread::sleep(SCAN_INTERVAL);
    }
}

/// Describes `device`, reading its battery when [`needs_read`] says so.
fn scan(device: &Device, known: &[Reading], read_all: bool) -> Scanned {
    let key = DeviceKey::of(device);
    let paths: Vec<String> = device
        .endpoints()
        .iter()
        .map(|endpoint| endpoint.path().to_owned())
        .collect();
    let serial = device.serial();
    let probe = match needs_read(find_previous(known, &key, serial, &paths), read_all) {
        true => read_battery(device),
        false => Probe::Skipped,
    };
    Scanned {
        key,
        paths,
        model: device.model(),
        serial: serial.map(str::to_owned),
        probe,
    }
}

/// Whether to read a battery: for a mouse not seen before, one without a reading or asleep, or
/// when everything is due.
fn needs_read(known: Option<&Reading>, read_all: bool) -> bool {
    read_all || known.is_none_or(|reading| reading.battery.is_none() || reading.asleep)
}

/// The previous reading of the mouse with `key`, `serial` and endpoint `paths`. A mouse that had
/// no serial was keyed by an endpoint path, so a reading keyed by one of `paths` is the same
/// mouse after it gained a serial; a reading with the same serial is the same mouse after that
/// endpoint went away.
fn find_previous<'a>(
    previous: &'a [Reading],
    key: &DeviceKey,
    serial: Option<&str>,
    paths: &[String],
) -> Option<&'a Reading> {
    previous
        .iter()
        .find(|reading| reading.key == *key)
        .or_else(|| {
            let serial = serial?;
            previous
                .iter()
                .find(|reading| reading.serial.as_deref() == Some(serial))
        })
        .or_else(|| {
            previous.iter().find(|reading| {
                paths
                    .iter()
                    .any(|path| reading.key == DeviceKey::from_path(path))
            })
        })
}

/// Reads the battery of `device` and logs failures.
fn read_battery(device: &Device) -> Probe {
    match device.battery() {
        Ok(battery) => Probe::Battery(battery),
        Err(Error::Timeout) => {
            log::debug!(
                "poller: {} did not respond, it may be asleep",
                device.model()
            );
            Probe::Asleep
        }
        Err(error) => {
            log::warn!("poller: cannot read battery of {}: {error}", device.model());
            Probe::Failed
        }
    }
}

/// The new list of readings from the previous one and a fresh scan. Mice that are gone drop out;
/// a reading that did not produce a battery keeps the last known one. A mouse that gained a serial
/// keeps the key it had, so the monitor sees the same mouse.
fn merge(previous: &[Reading], scanned: Vec<Scanned>) -> Vec<Reading> {
    scanned
        .into_iter()
        .map(|found| {
            let old = find_previous(previous, &found.key, found.serial.as_deref(), &found.paths);
            let last_battery = old.and_then(|reading| reading.battery);
            let (battery, asleep) = match found.probe {
                Probe::Skipped => (last_battery, old.is_some_and(|reading| reading.asleep)),
                Probe::Battery(battery) => (Some(battery), false),
                Probe::Asleep => (last_battery, true),
                Probe::Failed => (last_battery, false),
            };
            Reading {
                key: old.map_or(found.key, |reading| reading.key.clone()),
                model: found.model,
                serial: found.serial,
                battery,
                asleep,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use razer_hid::Percent;

    fn battery(level: u8) -> Battery {
        Battery {
            level: Percent::try_from(level).unwrap(),
            charging: Some(false),
        }
    }

    fn reading(key: &str, level: Option<u8>, asleep: bool) -> Reading {
        Reading {
            key: DeviceKey::for_test(key),
            model: "Mouse",
            serial: Some(key.to_owned()),
            battery: level.map(battery),
            asleep,
        }
    }

    fn scanned(key: &str, probe: Probe) -> Scanned {
        Scanned {
            key: DeviceKey::for_test(key),
            paths: Vec::new(),
            model: "Mouse",
            serial: Some(key.to_owned()),
            probe,
        }
    }

    #[test]
    fn skipped_probe_carries_the_previous_reading_over() {
        let previous = [reading("a", Some(40), true)];
        let merged = merge(&previous, vec![scanned("a", Probe::Skipped)]);
        assert_eq!(merged, previous);
    }

    #[test]
    fn removed_mouse_drops_out() {
        let previous = [reading("a", Some(40), false), reading("b", Some(60), false)];
        let merged = merge(&previous, vec![scanned("b", Probe::Skipped)]);
        assert_eq!(merged, [reading("b", Some(60), false)]);
    }

    #[test]
    fn asleep_keeps_the_last_battery() {
        let previous = [reading("a", Some(40), false)];
        let merged = merge(&previous, vec![scanned("a", Probe::Asleep)]);
        assert_eq!(merged, [reading("a", Some(40), true)]);
    }

    #[test]
    fn needs_read_for_new_empty_or_sleeping_mice_and_when_due() {
        assert!(needs_read(None, false));
        assert!(needs_read(Some(&reading("a", None, false)), false));
        assert!(needs_read(Some(&reading("a", Some(40), true)), false));
        assert!(needs_read(Some(&reading("a", Some(40), false)), true));
        assert!(!needs_read(Some(&reading("a", Some(40), false)), false));
    }

    #[test]
    fn mouse_that_gains_a_serial_keeps_its_key() {
        let mut asleep = reading("path-1", None, true);
        asleep.serial = None;
        let previous = [asleep];
        let mut woke = scanned("PM1", Probe::Battery(battery(70)));
        woke.paths = vec!["path-1".to_owned()];
        let merged = merge(&previous, vec![woke]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].key, DeviceKey::for_test("path-1"));
        assert_eq!(merged[0].serial.as_deref(), Some("PM1"));
        assert_eq!(merged[0].battery, Some(battery(70)));
        assert!(!merged[0].asleep);
        let low = Percent::try_from(15).unwrap();
        let events = crate::monitor::events(Some(&previous), &merged, low);
        assert_eq!(events.len(), 0, "{events:?}");
    }

    #[test]
    fn inherited_key_survives_losing_that_endpoint() {
        // Keyed by the dongle path, then the dongle goes away and the cable stays.
        let mut previous = reading("dongle-path", Some(70), false);
        previous.serial = Some("PM1".to_owned());
        let mut cable = scanned("PM1", Probe::Skipped);
        cable.paths = vec!["cable-path".to_owned()];
        let merged = merge(&[previous], vec![cable]);
        assert_eq!(merged[0].key, DeviceKey::for_test("dongle-path"));
        assert_eq!(merged[0].battery, Some(battery(70)));
    }

    #[test]
    fn unrelated_new_mouse_gets_its_own_key() {
        let previous = [reading("path-1", Some(70), false)];
        let mut other = scanned("PM2", Probe::Battery(battery(50)));
        other.paths = vec!["path-2".to_owned()];
        let merged = merge(&previous, vec![other]);
        assert_eq!(merged[0].key, DeviceKey::for_test("PM2"));
    }
}
