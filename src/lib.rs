//! Library to query battery status of your Razer mouse.
//!
//! # Usage
//!
//! ```
//! use razer_battery::Razer;
//!
//! let context = Razer::new().expect("Failed to initialize context");
//! for device in context.get_connected_devices() {
//!     println!("Device: {}", device.device_type());
//!     println!("Connection: {:?}", device.connection_type());
//!     match device
//!         .open(&context)
//!         .and_then(|handle| handle.get_battery_level())
//!     {
//!         Ok(status) => println!("Battery: {}", status),
//!         Err(e) => println!("Failed to get battery status: {}", e),
//!     }
//! }
//! ```

use hidapi::HidApi;
use hidapi::HidDevice;
use hidapi::HidError;
use std::error::Error;
use std::fmt;

/// Razer context.
///
/// This can be used to list available devices.
pub struct Razer {
    api: HidApi,
}

impl fmt::Debug for Razer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Razer").finish()
    }
}

impl Razer {
    /// Initialize a new Razer context.
    pub fn new() -> DeviceResult<Self> {
        let api = HidApi::new()?;
        #[cfg(target_os = "macos")]
        api.set_open_exclusive(false);
        Ok(Razer { api })
    }

    /// Refreshes the list of connected devices.
    pub fn refresh_devices(&mut self) -> DeviceResult<()> {
        self.api.refresh_devices()?;
        Ok(())
    }

    /// Returns a list of supported devices currently connected.
    pub fn get_connected_devices(&self) -> Vec<Device> {
        let mut devices: Vec<_> = self
            .api
            .device_list()
            .filter_map(|info| Device::try_from(info).ok())
            .collect();

        // Sort devices so identical devices are adjacent to each other
        devices.sort_by_key(|d| (d.device_type, d.connection_type));

        // Helper function to determine if two devices are the same physical device
        fn should_deduplicate(first: &mut Device, second: &mut Device) -> bool {
            if first.device_type != second.device_type {
                return false;
            }
            // Prefer wired connection representation if available
            let has_wired = first.connection_type == ConnectionType::Wired
                || second.connection_type == ConnectionType::Wired;
            has_wired
                || (first.connection_type == ConnectionType::Wireless
                    && second.connection_type == ConnectionType::Wireless)
        }

        devices.dedup_by(should_deduplicate);
        devices
    }

    /// Prints udev rules for currently connected Razer devices to allow non-root access on Linux systems.
    /// These rules should be saved to `/etc/udev/rules.d/99-razer.rules`
    ///
    /// # Example
    /// ```no_run
    /// use razer_battery::Razer;
    /// let context = Razer::new().expect("Failed to initialize context");
    /// context.print_udev_rules();
    /// ```
    #[cfg(target_os = "linux")]
    pub fn print_udev_rules(&self) {
        use std::collections::HashSet;

        // Collect unique (vendor_id, product_id) pairs from connected devices
        let device_ids: HashSet<_> = self
            .get_connected_devices()
            .map(|device| (device.vendor_id, device.product_id))
            .collect();

        if device_ids.is_empty() {
            println!("No Razer devices currently connected.");
            return;
        }

        println!("# Razer Device Permissions");
        println!("# Save this to /etc/udev/rules.d/99-razer.rules");
        println!("# Then run: sudo udevadm control --reload-rules && sudo udevadm trigger");
        println!();

        // Print rules for each connected device
        for device in self.get_connected_devices() {
            println!(
                "# {} ({})",
                device.device_type(),
                match device.connection_type() {
                    ConnectionType::Wired => "Wired",
                    ConnectionType::Wireless => "Wireless",
                }
            );
            println!(
                "SUBSYSTEM==\"hidraw\", ATTRS{{idVendor}}==\"{:04x}\", ATTRS{{idProduct}}==\"{:04x}\", MODE=\"0666\"",
                device.vendor_id, device.product_id
            );
        }
    }
}

/// The model of the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DeviceType {
    /// Razer [DeathAdder V3 Pro][dav3pro] mouse
    ///
    /// [dav3pro]: https://www.razer.com/gaming-mice/razer-deathadder-v3-pro
    DeathAdderV3Pro,
    /// Razer [DeathAdder V3 HyperSpeed][dav3hs] mouse
    ///
    /// [dav3hs]: https://www.razer.com/gaming-mice/razer-deathadder-v3-hyperspeed
    DeathAdderV3HyperSpeed,
    /// Razer [DeathAdder V2 Pro][dav2pro] mouse
    ///
    /// [dav2pro]: https://www.razer.com/mena-en/gaming-mice/razer-deathadder-v2-pro
    DeathAdderV2Pro,
}

impl fmt::Display for DeviceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceType::DeathAdderV3Pro => {
                write!(f, "Razer DeathAdder V3 Pro")
            }
            DeviceType::DeathAdderV3HyperSpeed => {
                write!(f, "Razer DeathAdder V3 HyperSpeed")
            }
            DeviceType::DeathAdderV2Pro => {
                write!(f, "Razer DeathAdder V2 Pro")
            }
        }
    }
}

/// A device related error.
#[derive(Debug)]
pub enum DeviceError {
    /// Tried to use a device that is not supported.
    Unsupported {
        vendor_id: u16,
        product_id: u16,
    },
    /// A [`hidapi`] operation failed.
    HidError(HidError),
    /// The device path contains invalid characters (e.g. null bytes).
    InvalidPath(std::ffi::NulError),
    /// Failed to send command to device
    CommunicationFailed {
        device_type: DeviceType,
        reason: CommunicationFailureReason,
    },
    DeviceDisconnected,
}

/// Specific reasons why device communication might fail
#[derive(Debug)]
pub enum CommunicationFailureReason {
    /// Failed to send feature report to device
    SendFeatureReport(HidError),
    /// Failed to get feature report from device
    GetFeatureReport(HidError),
    /// Device returned unexpected response status
    UnexpectedStatus(u8, u8),
    /// Maximum retry attempts reached
    MaxRetriesExceeded,
}

impl fmt::Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceError::Unsupported {
                vendor_id,
                product_id,
            } => {
                write!(
                    f,
                    "Device with vendor_id: {vendor_id}, product_id: {product_id} is not supported"
                )
            }
            DeviceError::HidError(error) => {
                write!(f, "HID error occurred: {}", error)
            }
            DeviceError::InvalidPath(error) => {
                write!(f, "Invalid device path: {}", error)
            }
            DeviceError::CommunicationFailed {
                device_type,
                reason,
            } => {
                write!(f, "Communication failed: {device_type}, reason: {reason}")
            }
            DeviceError::DeviceDisconnected => {
                write!(f, "Device is disconnected")
            }
        }
    }
}

impl fmt::Display for CommunicationFailureReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SendFeatureReport(err) => {
                write!(f, "failed to send feature report: {}", err)
            }
            Self::GetFeatureReport(err) => {
                write!(f, "failed to get feature report: {}", err)
            }
            Self::UnexpectedStatus(status, attempt) => {
                write!(
                    f,
                    "device returned unexpected status: {} on attempt: {}",
                    status, attempt
                )
            }
            Self::MaxRetriesExceeded => {
                write!(f, "maximum retry attempts reached")
            }
        }
    }
}

impl Error for DeviceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            DeviceError::HidError(error) => Some(error),
            DeviceError::InvalidPath(error) => Some(error),
            _ => None,
        }
    }
}

impl From<HidError> for DeviceError {
    fn from(error: HidError) -> Self {
        DeviceError::HidError(error)
    }
}

/// The [`Result`] of a Razer device operation.
pub type DeviceResult<T> = Result<T, DeviceError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConnectionType {
    Wired,
    Wireless,
}

/// A device that can be used.
#[derive(Debug, Clone)]
pub struct Device {
    pub name: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub path: String,
    pub device_type: DeviceType,
    pub connection_type: ConnectionType,
}

const VENDOR_ID: u16 = 0x1532;
const USAGE_PAGE: u16 = 1;
const USAGE: u16 = 2;

impl TryFrom<&hidapi::DeviceInfo> for Device {
    type Error = DeviceError;

    fn try_from(info: &hidapi::DeviceInfo) -> Result<Self, DeviceError> {
        if info.vendor_id() != VENDOR_ID
            || info.usage_page() != USAGE_PAGE
            || info.interface_number() != 0
        {
            return Err(DeviceError::Unsupported {
                vendor_id: info.vendor_id(),
                product_id: info.product_id(),
            });
        }

        // Windows subdivides interfaces into different usages
        if cfg!(target_os = "windows") && (info.usage_page() != USAGE_PAGE || info.usage() != USAGE)
        {
            return Err(DeviceError::Unsupported {
                vendor_id: info.vendor_id(),
                product_id: info.product_id(),
            });
        }

        let (device_type, connection_type) =
            device_type_from_product_id(info.product_id()).ok_or(DeviceError::Unsupported {
                vendor_id: info.vendor_id(),
                product_id: info.product_id(),
            })?;

        Ok(Device {
            name: info
                .product_string()
                .unwrap_or("Unknown Razer Device")
                .to_string(),
            vendor_id: info.vendor_id(),
            product_id: info.product_id(),
            path: info.path().to_string_lossy().to_string(),
            device_type,
            connection_type,
        })
    }
}

#[derive(Debug, Default)]
pub enum BatteryStatus {
    #[default]
    Unknown,
    Charging(u8),
    Level(u8),
}

impl fmt::Display for BatteryStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BatteryStatus::Unknown => write!(f, "Unknown"),
            BatteryStatus::Charging(percentage) => write!(f, "Charging ({percentage}%)"),
            BatteryStatus::Level(percentage) => write!(f, "{percentage}%"),
        }
    }
}

/// The handle of an opened device that can be used for getting and setting the device status.
#[derive(Debug)]
pub struct DeviceHandle {
    hid_device: HidDevice,
    device_type: DeviceType,
}

impl DeviceHandle {
    /// The model of the device.
    #[must_use]
    pub fn device_type(&self) -> DeviceType {
        self.device_type
    }

    /// The [`HidDevice`] for the device.
    #[must_use]
    pub fn hid_device(&self) -> &HidDevice {
        &self.hid_device
    }

    /// Gets the current battery level of the device.
    pub fn get_battery_level(&self) -> DeviceResult<BatteryStatus> {
        let battery_report = send_command(self, 0x07, 0x80)?;
        // Byte 9 contains the battery level mapped to 0-255
        let battery_level = (battery_report[9] as f32 / 255.0 * 100.0) as u8;

        Ok(if self.is_charging()? {
            BatteryStatus::Charging(battery_level)
        } else {
            BatteryStatus::Level(battery_level)
        })
    }

    /// Checks if the device is currently charging.
    pub fn is_charging(&self) -> DeviceResult<bool> {
        let charging_report = send_command(self, 0x07, 0x84)?;
        // Byte 9 is non-zero if charging
        let is_charging = charging_report[9] != 0;
        Ok(is_charging)
    }
}

impl Device {
    /// The model of the device.
    #[must_use]
    pub fn device_type(&self) -> DeviceType {
        self.device_type
    }

    /// The connection type of the device.
    #[must_use]
    pub fn connection_type(&self) -> ConnectionType {
        self.connection_type
    }

    /// Opens the device.
    /// Note: We need the context again to open by path efficiently/safely
    pub fn open(&self, context: &Razer) -> DeviceResult<DeviceHandle> {
        // hidapi::HidApi::open_path takes a CStr/String.
        // We use the stored path.
        let c_path = std::ffi::CString::new(self.path.clone()).map_err(DeviceError::InvalidPath)?;

        let hid_device = context.api.open_path(&c_path)?;

        Ok(DeviceHandle {
            hid_device,
            device_type: self.device_type,
        })
    }
}

fn device_type_from_product_id(product_id: u16) -> Option<(DeviceType, ConnectionType)> {
    match product_id {
        // DeathAdder V3 Pro
        0x00b6 => Some((DeviceType::DeathAdderV3Pro, ConnectionType::Wired)),
        0x00b7 => Some((DeviceType::DeathAdderV3Pro, ConnectionType::Wireless)),
        // DeathAdder V3 HyperSpeed
        0x00c4 => Some((DeviceType::DeathAdderV3HyperSpeed, ConnectionType::Wired)),
        0x00c5 => Some((DeviceType::DeathAdderV3HyperSpeed, ConnectionType::Wireless)),
        // DeathAdder V2 Pro
        0x007C => Some((DeviceType::DeathAdderV2Pro, ConnectionType::Wired)),
        0x007D => Some((DeviceType::DeathAdderV2Pro, ConnectionType::Wireless)),
        _ => None,
    }
}

fn transaction_id_from_device_type(device_type: &DeviceType) -> u8 {
    match device_type {
        DeviceType::DeathAdderV3Pro => 0x1F,
        DeviceType::DeathAdderV3HyperSpeed => 0x1F,
        DeviceType::DeathAdderV2Pro => 0x1F,
    }
}

// Constants for device communication
const MAX_RETRIES: u8 = 4;
const RETRY_DELAY_MS: u64 = 60;
const REPORT_SIZE: usize = 91;
const REPORT_DATA_SIZE: usize = 90;
const VALID_STATUS_RANGE: std::ops::Range<u8> = 0..4;

// Constants for report structure
const REPORT_ARGS_SIZE: usize = 80;
const REPORT_CRC_OFFSET: usize = 2;
const REPORT_CRC_LENGTH: usize = 86; // 88 - 2

/// Calculates CRC for the report using a XOR-based algorithm.
///
/// The CRC is calculated from byte 2 to byte 88 (inclusive) of the report.
fn calculate_report_crc(data: &[u8]) -> u8 {
    // Razer mice typically use a simple XOR checksum for this report format.
    // We skip the first 2 bytes (Report ID, Status) and stop before the CRC byte itself.
    data[REPORT_CRC_OFFSET..REPORT_CRC_OFFSET + REPORT_CRC_LENGTH]
        .iter()
        .fold(0u8, |crc, &byte| crc ^ byte)
}

fn create_report(
    device_type: &DeviceType,
    command_class: u8,
    command_id: u8,
    data_size: u8,
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(REPORT_DATA_SIZE);

    // Header
    buf.extend(vec![
        0x00,                                         // status
        transaction_id_from_device_type(device_type), // transaction_id
        0x00,                                         // remaining_packets (high byte)
        0x00,                                         // remaining_packets (low byte)
        0x00,                                         // protocol_type
        data_size,                                    // data_size
        command_class,                                // command_class
        command_id,                                   // command_id
    ]);

    // Arguments (zero filled)
    buf.extend(vec![0x00; REPORT_ARGS_SIZE]);

    // Calculate CRC
    let crc = calculate_report_crc(&buf);

    buf.push(crc); // crc
    buf.push(0x00); // reserved

    #[cfg(debug_assertions)]
    println!("Created report buffer: {:02X?}", &buf);

    buf
}

fn send_command(
    device_handle: &DeviceHandle,
    command: u8,
    command_id: u8,
) -> Result<Vec<u8>, DeviceError> {
    let report = create_report(&device_handle.device_type, command, command_id, 0x02);
    let mut send_buf = Vec::with_capacity(REPORT_SIZE);
    let mut response = vec![0u8; REPORT_SIZE];

    send_buf.push(0x0); // report_id
    send_buf.extend(&report);

    #[cfg(debug_assertions)]
    println!("Sending command buffer: {:02X?}", &send_buf);

    // Try to send the command
    for attempt in 0..MAX_RETRIES {
        // Send command
        device_handle
            .hid_device
            .send_feature_report(&send_buf)
            .map_err(|e| DeviceError::CommunicationFailed {
                device_type: device_handle.device_type,
                reason: CommunicationFailureReason::SendFeatureReport(e),
            })?;

        std::thread::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS));

        // Get response
        device_handle
            .hid_device
            .get_feature_report(&mut response)
            .map_err(|e| DeviceError::CommunicationFailed {
                device_type: device_handle.device_type,
                reason: CommunicationFailureReason::GetFeatureReport(e),
            })?;

        // Check response status
        if VALID_STATUS_RANGE.contains(&response[1]) {
            let mut result = vec![0u8; REPORT_DATA_SIZE];
            result.copy_from_slice(&response[1..]);
            return Ok(result);
        } else if attempt == MAX_RETRIES - 1 {
            return Err(DeviceError::CommunicationFailed {
                device_type: device_handle.device_type,
                reason: CommunicationFailureReason::UnexpectedStatus(response[1], attempt),
            });
        }
    }

    Err(DeviceError::CommunicationFailed {
        device_type: device_handle.device_type,
        reason: CommunicationFailureReason::MaxRetriesExceeded,
    })
}
