//! Finding the mice that are plugged in and grouping their endpoints by physical mouse.

use std::collections::HashMap;

use crate::catalog::{self, Entry};
use crate::device::{Battery, Endpoint};
use crate::error::{Error, Result};
use crate::transport::{Bus, Collection, Transport};

/// HID usage page and usage of the generic mouse collection, tried first on every interface.
const MOUSE_USAGE: (u16, u16) = (0x0001, 0x0002);
/// Windows marks a HID collection in the device path with this token followed by two digits.
const COLLECTION_TOKEN: &str = "&col";

/// One physical mouse, possibly reachable through several endpoints (cable, dongle, Bluetooth).
#[derive(Debug)]
pub struct Device {
    serial: Option<String>,
    /// Never empty; sorted by link preference, wired first.
    endpoints: Vec<Endpoint>,
}

impl Device {
    /// Model name without the connection suffix, for example `Razer Viper Ultimate`.
    pub fn model(&self) -> &'static str {
        self.endpoints[0].entry().model_name()
    }

    /// The serial number the mouse reports over the protocol, if it has a readable one.
    pub fn serial(&self) -> Option<&str> {
        self.serial.as_deref()
    }

    /// Every endpoint of this mouse, most preferred first. Meant for diagnostics.
    pub fn endpoints(&self) -> &[Endpoint] {
        &self.endpoints
    }

    /// Reads the battery through the most preferred endpoint that answers. When none does,
    /// the error of the most preferred endpoint is returned.
    ///
    /// # Errors
    ///
    /// [`Error::Timeout`] means the mouse is asleep; other errors are real failures.
    pub fn battery(&self) -> Result<Battery> {
        let mut first_error = None;
        for endpoint in &self.endpoints {
            match endpoint.battery() {
                Ok(battery) => return Ok(battery),
                Err(error) => {
                    log::debug!("hid: cannot read battery via {}: {error}", endpoint.name());
                    first_error.get_or_insert(error);
                }
            }
        }
        Err(first_error.unwrap_or(Error::Timeout))
    }
}

/// What makes two endpoints the same mouse.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Identity {
    /// The serial number read over the protocol.
    Serial(String),
    /// No usable serial: the endpoint stands alone, identified by its path.
    Path(String),
}

/// Lists the connected mice on `bus`.
pub(crate) fn discover(bus: &dyn Bus) -> Result<Vec<Device>> {
    let mut order: Vec<Identity> = Vec::new();
    let mut groups: HashMap<Identity, Vec<Endpoint>> = HashMap::new();
    for (entry, candidates) in group_collections(bus.collections()?) {
        let Some(endpoint) = open_endpoint(bus, entry, &candidates) else {
            continue;
        };
        let identity = match endpoint.serial() {
            Ok(serial) => Identity::Serial(serial),
            Err(error) => {
                log::debug!("hid: no serial from {}: {error}", endpoint.name());
                Identity::Path(endpoint.path().to_owned())
            }
        };
        if !groups.contains_key(&identity) {
            order.push(identity.clone());
        }
        groups.entry(identity).or_default().push(endpoint);
    }

    let mut devices = Vec::new();
    for identity in order {
        let Some(mut endpoints) = groups.remove(&identity) else {
            continue;
        };
        endpoints.sort_by_key(Endpoint::link);
        let serial = match identity {
            Identity::Serial(serial) => Some(serial),
            Identity::Path(_) => None,
        };
        devices.push(Device { serial, endpoints });
    }
    Ok(devices)
}

/// Keeps the collections of catalog interfaces and groups them per interface. The mouse usage
/// pair comes first in each group, because that collection answers on Windows.
fn group_collections(collections: Vec<Collection>) -> Vec<(&'static Entry, Vec<Collection>)> {
    let mut index: HashMap<(u16, String), usize> = HashMap::new();
    let mut groups: Vec<(&'static Entry, Vec<Collection>)> = Vec::new();
    for collection in collections {
        let Some(entry) = catalog::find(collection.pid) else {
            continue;
        };
        if collection.interface != i32::from(entry.interface) {
            continue;
        }
        let key = (collection.pid, interface_key(&collection.path));
        let slot = *index.entry(key).or_insert_with(|| {
            groups.push((entry, Vec::new()));
            groups.len() - 1
        });
        groups[slot].1.push(collection);
    }
    for (_, candidates) in &mut groups {
        candidates.sort_by_key(|c| (c.usage_page, c.usage) != MOUSE_USAGE);
    }
    groups
}

/// Opens the candidates of one interface; collections that cannot be opened are skipped.
fn open_endpoint(
    bus: &dyn Bus,
    entry: &'static Entry,
    candidates: &[Collection],
) -> Option<Endpoint> {
    let transports: Vec<Box<dyn Transport>> = candidates
        .iter()
        .filter_map(|collection| match bus.open(collection) {
            Ok(transport) => Some(transport),
            Err(error) => {
                log::debug!("hid: cannot open {}: {error}", collection.path);
                None
            }
        })
        .collect();
    if transports.is_empty() {
        log::warn!("hid: cannot open any collection of {}", entry.name);
        return None;
    }
    let path = candidates[0].path.clone();
    Some(Endpoint::new(entry, path, transports))
}

/// Maps the path of a HID collection to a key shared by all collections of the same interface.
///
/// On Windows each collection of an interface has its own path, differing in a `&colNN` token,
/// in the last number of the instance id and sometimes in a `\KBD` tail after the class guid
/// (`hid#vid_1532&pid_00bf&mi_01&col02#a&29f59cb0&0&0001#{guid}`). The key keeps only the
/// device and the instance id without the collection parts. Paths without a collection token
/// (Linux, macOS) are returned unchanged.
fn interface_key(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    let parts: Vec<&str> = lower.split('#').collect();
    let [prefix, device, instance, ..] = parts.as_slice() else {
        return lower;
    };
    let Some(token) = device.find(COLLECTION_TOKEN) else {
        return lower;
    };
    let device = &device[..token];
    let instance = instance
        .rsplit_once('&')
        .map_or(*instance, |(head, _)| head);
    format!("{prefix}#{device}#{instance}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Link;
    use crate::transport::mock::{Behavior, MockBus};

    const WIRED: u16 = 0x00BE;
    const DONGLE: u16 = 0x00BF;
    const MOUSE: (u16, u16) = MOUSE_USAGE;
    const OTHER: (u16, u16) = (0xFF00, 0x0001);
    const COLLECTION_PATH: &str = "\\\\?\\hid#vid_1532&pid_00be&mi_00&col0";

    fn with_serial(serial: &'static str) -> Behavior {
        Behavior {
            serial: Some(serial),
            ..Behavior::default()
        }
    }

    fn broken() -> Behavior {
        Behavior {
            broken: true,
            ..Behavior::default()
        }
    }

    #[test]
    fn groups_cable_and_dongle_by_serial_and_prefers_wired() {
        let mut bus = MockBus::default();
        bus.add("dongle", DONGLE, MOUSE, with_serial("PM1"));
        bus.add("cable", WIRED, MOUSE, with_serial("PM1"));
        let devices = discover(&bus).unwrap();
        assert_eq!(devices.len(), 1);
        let device = &devices[0];
        assert_eq!(device.serial(), Some("PM1"));
        assert_eq!(device.model(), "Razer DeathAdder V4 Pro");
        let links: Vec<Link> = device.endpoints().iter().map(Endpoint::link).collect();
        assert_eq!(links, [Link::Wired, Link::Dongle]);
        assert!(device.battery().is_ok());
    }

    #[test]
    fn identical_mice_with_different_serials_are_two_devices() {
        let mut bus = MockBus::default();
        bus.add("one", WIRED, MOUSE, with_serial("PM1"));
        bus.add("two", WIRED, MOUSE, with_serial("PM2"));
        let devices = discover(&bus).unwrap();
        let serials: Vec<_> = devices.iter().map(Device::serial).collect();
        assert_eq!(serials, [Some("PM1"), Some("PM2")]);
    }

    #[test]
    fn endpoints_without_a_serial_stand_alone() {
        let mut bus = MockBus::default();
        // The mock answers "not supported" to the serial command.
        bus.add("a", WIRED, MOUSE, Behavior::default());
        bus.add("b", DONGLE, MOUSE, Behavior::default());
        let devices = discover(&bus).unwrap();
        assert_eq!(devices.len(), 2);
        assert!(devices.iter().all(|device| device.serial().is_none()));
    }

    #[test]
    fn serial_unsupported_in_catalog_stands_alone() {
        let mut bus = MockBus::default();
        // Razer Mamba 2012 (Wired) has no serial in the catalog.
        bus.add("old", 0x0024, MOUSE, with_serial("PM1"));
        let devices = discover(&bus).unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].serial(), None);
    }

    #[test]
    fn ignores_products_outside_the_catalog() {
        let mut bus = MockBus::default();
        bus.add("other", 0x0203, MOUSE, with_serial("PM1"));
        assert!(discover(&bus).unwrap().is_empty());
    }

    #[test]
    fn collections_of_one_interface_make_one_endpoint() {
        let mut bus = MockBus::default();
        let other = format!("{COLLECTION_PATH}2#8&aa&0&0001#{{g}}");
        let mouse = format!("{COLLECTION_PATH}1#8&aa&0&0000#{{g}}");
        bus.add(&other, WIRED, OTHER, broken());
        bus.add(&mouse, WIRED, MOUSE, with_serial("PM1"));
        let devices = discover(&bus).unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].endpoints().len(), 1);
        assert_eq!(devices[0].endpoints()[0].path(), mouse);
        assert_eq!(devices[0].serial(), Some("PM1"));
    }

    #[test]
    fn falls_back_to_another_collection_when_the_mouse_usage_is_silent() {
        let mut bus = MockBus::default();
        let mouse = format!("{COLLECTION_PATH}1#8&aa&0&0000#{{g}}");
        let other = format!("{COLLECTION_PATH}2#8&aa&0&0001#{{g}}");
        bus.add(&mouse, WIRED, MOUSE, broken());
        bus.add(&other, WIRED, OTHER, with_serial("PM1"));
        let devices = discover(&bus).unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].serial(), Some("PM1"));
        assert!(devices[0].battery().is_ok());
    }

    #[test]
    fn interface_key_strips_collection_markers() {
        // Real paths of a DeathAdder V4 Pro dongle on Windows 11.
        let guid = "{4d1e55b2-f16f-11cf-88cb-001111000030}";
        let a = interface_key(&format!(
            "\\\\?\\HID#VID_1532&PID_00BF&MI_01&Col01#a&29f59cb0&0&0000#{guid}\\KBD"
        ));
        let b = interface_key(&format!(
            "\\\\?\\HID#VID_1532&PID_00BF&MI_01&Col02#a&29f59cb0&0&0001#{guid}"
        ));
        let other = interface_key(&format!(
            "\\\\?\\HID#VID_1532&PID_00BF&MI_01&Col01#a&ffffffff&0&0000#{guid}"
        ));
        assert_eq!(a, b);
        assert_ne!(a, other);
        let single = format!("\\\\?\\HID#VID_1532&PID_00BF&MI_00#a&18aeee7&0&0000#{guid}");
        assert_eq!(interface_key(&single), single.to_ascii_lowercase());
        assert_eq!(interface_key("/dev/hidraw3"), "/dev/hidraw3");
    }
}
