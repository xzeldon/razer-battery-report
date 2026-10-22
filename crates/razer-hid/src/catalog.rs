//! One [`Entry`] per USB product id: every battery mouse of openrazer (commit `477f4d6`) as
//! printed by `scripts/list-openrazer-mice.py`. Only `link` is written by hand.

/// Razer's USB vendor id.
pub const VENDOR_ID: u16 = 0x1532;

/// Name suffixes that describe the connection rather than the model.
const LINK_SUFFIXES: [&str; 5] = [
    " (Wired)",
    " (Wireless)",
    " (Receiver)",
    " (Bluetooth)",
    " Receiver",
];

/// Whether the mouse answers a command on a given endpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Support {
    /// The command can be sent.
    Query,
    /// The command is not available on this product id.
    Unsupported,
}

/// How a product id reaches the mouse. The order is the preference when one mouse is visible
/// through several endpoints: wired first, then the dongle, then Bluetooth.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Link {
    /// The mouse on its USB cable.
    Wired,
    /// The wireless dongle or receiver.
    Dongle,
    /// Bluetooth.
    Bluetooth,
}

/// One USB product id with the protocol values needed to talk to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    /// USB product id.
    pub pid: u16,
    /// Product name exactly as openrazer spells it, including the connection suffix.
    pub name: &'static str,
    /// Transaction id used for commands.
    pub tid: u8,
    /// USB interface number that answers commands.
    pub interface: u8,
    /// Microseconds to wait between sending a request and reading the response.
    pub wait_us: u32,
    /// Whether the charging state can be read.
    pub charging: Support,
    /// Whether the serial number can be read.
    pub serial: Support,
    /// Transaction id for the serial command only, when it differs from `tid`.
    pub serial_tid: Option<u8>,
    /// Which kind of connection this product id represents.
    pub link: Link,
}

impl Entry {
    /// The model name without the connection suffix, for example `Razer Viper Ultimate`.
    pub fn model_name(&self) -> &'static str {
        LINK_SUFFIXES
            .iter()
            .find_map(|suffix| self.name.strip_suffix(suffix))
            .unwrap_or(self.name)
    }
}

/// Finds the entry of a USB product id.
pub fn find(pid: u16) -> Option<&'static Entry> {
    CATALOG.iter().find(|entry| entry.pid == pid)
}

/// Every supported product id, sorted by `pid`.
#[rustfmt::skip]
pub const CATALOG: &[Entry] = &[
    // The PID is the mouse on its cable; its wireless dock has a separate PID.
    Entry { pid: 0x001F, name: "Razer Naga Epic", tid: 0xFF, interface: 0, wait_us: 600, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x0024, name: "Razer Mamba 2012 (Wired)", tid: 0xFF, interface: 0, wait_us: 600, charging: Support::Query, serial: Support::Unsupported, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x0025, name: "Razer Mamba 2012 (Wireless)", tid: 0xFF, interface: 0, wait_us: 600, charging: Support::Query, serial: Support::Unsupported, serial_tid: None, link: Link::Dongle },
    // The PID is the mouse on its cable; its wireless dock has a separate PID.
    Entry { pid: 0x0032, name: "Razer Ouroboros", tid: 0xFF, interface: 0, wait_us: 600, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x003E, name: "Razer Naga Epic Chroma (Wired)", tid: 0xFF, interface: 0, wait_us: 600, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x003F, name: "Razer Naga Epic Chroma (Wireless)", tid: 0xFF, interface: 0, wait_us: 600, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x0044, name: "Razer Mamba Chroma (Wired)", tid: 0xFF, interface: 0, wait_us: 600, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x0045, name: "Razer Mamba Chroma (Wireless)", tid: 0xFF, interface: 0, wait_us: 600, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x0059, name: "Razer Lancehead (Wired)", tid: 0x3F, interface: 0, wait_us: 600, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x005A, name: "Razer Lancehead (Wireless)", tid: 0x3F, interface: 0, wait_us: 600, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x0062, name: "Razer Atheris (Receiver)", tid: 0x1F, interface: 0, wait_us: 400_000, charging: Support::Unsupported, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x006F, name: "Razer Lancehead Wireless (Receiver)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: Some(0x3F), link: Link::Dongle },
    Entry { pid: 0x0070, name: "Razer Lancehead Wireless (Wired)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: Some(0x3F), link: Link::Wired },
    Entry { pid: 0x0072, name: "Razer Mamba Wireless (Receiver)", tid: 0x3F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x0073, name: "Razer Mamba Wireless (Wired)", tid: 0x3F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x0077, name: "Razer Pro Click (Receiver)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x007A, name: "Razer Viper Ultimate (Wired)", tid: 0xFF, interface: 0, wait_us: 59_900, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x007B, name: "Razer Viper Ultimate (Wireless)", tid: 0xFF, interface: 0, wait_us: 59_900, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x007C, name: "Razer DeathAdder V2 Pro (Wired)", tid: 0x3F, interface: 0, wait_us: 59_900, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x007D, name: "Razer DeathAdder V2 Pro (Wireless)", tid: 0x3F, interface: 0, wait_us: 59_900, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x0080, name: "Razer Pro Click (Wired)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x0083, name: "Razer Basilisk X HyperSpeed", tid: 0xFF, interface: 0, wait_us: 31_000, charging: Support::Unsupported, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x0086, name: "Razer Basilisk Ultimate", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x0088, name: "Razer Basilisk Ultimate Receiver", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x008F, name: "Razer Naga Pro (Wired)", tid: 0x1F, interface: 0, wait_us: 600, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x0090, name: "Razer Naga Pro (Wireless)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x0094, name: "Razer Orochi V2 (Receiver)", tid: 0x1F, interface: 0, wait_us: 400_000, charging: Support::Unsupported, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x0095, name: "Razer Orochi V2 (Bluetooth)", tid: 0x1F, interface: 0, wait_us: 400_000, charging: Support::Unsupported, serial: Support::Query, serial_tid: None, link: Link::Bluetooth },
    Entry { pid: 0x009A, name: "Razer Pro Click Mini (Receiver)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x009C, name: "Razer DeathAdder V2 X HyperSpeed", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Unsupported, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x009E, name: "Razer Viper Mini SE (Wired)", tid: 0x1F, interface: 0, wait_us: 59_900, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x009F, name: "Razer Viper Mini SE (Wireless)", tid: 0x1F, interface: 0, wait_us: 59_900, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x00A5, name: "Razer Viper V2 Pro (Wired)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x00A6, name: "Razer Viper V2 Pro (Wireless)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x00A7, name: "Razer Naga V2 Pro (Wired)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x00A8, name: "Razer Naga V2 Pro (Wireless)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x00AA, name: "Razer Basilisk V3 Pro (Wired)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x00AB, name: "Razer Basilisk V3 Pro (Wireless)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x00AF, name: "Razer Cobra Pro (Wired)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x00B0, name: "Razer Cobra Pro (Wireless)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x00B3, name: "Razer HyperPolling Wireless Dongle", tid: 0x1F, interface: 0, wait_us: 59_900, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x00B4, name: "Razer Naga V2 HyperSpeed (Receiver)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Unsupported, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x00B6, name: "Razer DeathAdder V3 Pro (Wired)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x00B7, name: "Razer DeathAdder V3 Pro (Wireless)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x00B8, name: "Razer Viper V3 HyperSpeed", tid: 0x1F, interface: 0, wait_us: 59_900, charging: Support::Unsupported, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x00B9, name: "Razer Basilisk V3 X HyperSpeed", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Unsupported, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x00BE, name: "Razer DeathAdder V4 Pro (Wired)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x00BF, name: "Razer DeathAdder V4 Pro (Wireless)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x00C0, name: "Razer Viper V3 Pro (Wired)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x00C1, name: "Razer Viper V3 Pro (Wireless)", tid: 0x1F, interface: 0, wait_us: 59_900, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x00C2, name: "Razer DeathAdder V3 Pro (Wired)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x00C3, name: "Razer DeathAdder V3 Pro (Wireless)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x00C4, name: "Razer DeathAdder V3 HyperSpeed (Wired)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x00C5, name: "Razer DeathAdder V3 HyperSpeed (Wireless)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x00C7, name: "Razer Pro Click V2 Vertical Edition (Wired)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x00C8, name: "Razer Pro Click V2 Vertical Edition (Wireless)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x00CC, name: "Razer Basilisk V3 Pro 35K (Wired)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x00CD, name: "Razer Basilisk V3 Pro 35K (Wireless)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x00D0, name: "Razer Pro Click V2 (Wired)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x00D1, name: "Razer Pro Click V2 (Wireless)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x00D3, name: "Razer Basilisk Mobile (Wired)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Unsupported, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x00D4, name: "Razer Basilisk Mobile (Receiver)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Unsupported, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x00D6, name: "Razer Basilisk V3 Pro 35K Phantom Green Edition (Wired)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x00D7, name: "Razer Basilisk V3 Pro 35K Phantom Green Edition (Wireless)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x00DA, name: "Razer Cobra HyperSpeed", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x00DB, name: "Razer Cobra HyperSpeed Receiver", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    Entry { pid: 0x00DE, name: "Razer Viper V3 Pro SE (Wired)", tid: 0x1F, interface: 0, wait_us: 31_000, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x00DF, name: "Razer Viper V3 Pro SE (Wireless)", tid: 0x1F, interface: 0, wait_us: 59_900, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    // Values from openrazer issue #2958, tested by @Dulkman on the cable and the dongle.
    Entry { pid: 0x00E2, name: "Razer Basilisk V4 Pro (Wired)", tid: 0x1F, interface: 3, wait_us: 59_900, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x00E3, name: "Razer Basilisk V4 Pro (Wireless)", tid: 0x1F, interface: 3, wait_us: 59_900, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
    // Values from openrazer PR #2751, tested by @morphly on the cable and the dongle.
    Entry { pid: 0x00E5, name: "Razer Viper V4 Pro (Wired)", tid: 0x1F, interface: 3, wait_us: 59_900, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Wired },
    Entry { pid: 0x00E6, name: "Razer Viper V4 Pro (Wireless)", tid: 0x1F, interface: 3, wait_us: 59_900, charging: Support::Query, serial: Support::Query, serial_tid: None, link: Link::Dongle },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn pids_are_unique() {
        let pids: HashSet<u16> = CATALOG.iter().map(|entry| entry.pid).collect();
        assert_eq!(pids.len(), CATALOG.len());
    }

    #[test]
    fn names_are_not_empty() {
        assert!(CATALOG.iter().all(|entry| !entry.name.is_empty()));
    }

    #[test]
    fn transaction_ids_are_known() {
        let known = [0x1f, 0x3f, 0x9f, 0xff];
        for entry in CATALOG {
            assert!(known.contains(&entry.tid), "{}", entry.name);
            if let Some(tid) = entry.serial_tid {
                assert!(known.contains(&tid), "{}", entry.name);
            }
        }
    }

    #[test]
    fn serial_tid_needs_serial_support() {
        for entry in CATALOG {
            if entry.serial_tid.is_some() {
                assert_eq!(entry.serial, Support::Query, "{}", entry.name);
            }
        }
    }

    #[test]
    fn model_name_drops_the_link_suffix() {
        let name = |pid| find(pid).unwrap().model_name();
        assert_eq!(name(0x0024), "Razer Mamba 2012");
        assert_eq!(name(0x00B3), "Razer HyperPolling Wireless Dongle");
        assert!(find(0xFFFF).is_none());
    }
}
