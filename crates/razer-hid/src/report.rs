//! The 90-byte Razer report: encoding, decoding and checksum.
//!
//! Layout: `[0]` status, `[1]` transaction id, `[2..4]` remaining packets (big endian),
//! `[4]` protocol type, `[5]` data size, `[6]` command class, `[7]` command id,
//! `[8..88]` arguments, `[88]` checksum, `[89]` reserved.

use crate::error::{Error, Result};

/// Length of a report on the protocol level, without any hidapi report id.
pub const REPORT_LEN: usize = 90;
/// Number of argument bytes in a report.
pub const ARGUMENTS_LEN: usize = 80;

const ARGUMENTS_START: usize = 8;
pub(crate) const CRC_INDEX: usize = 88;
const CRC_START: usize = 2;

/// Result code carried in the first byte of a report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// A new command, set by the host.
    New = 0x00,
    /// The mouse is busy, try again later.
    Busy = 0x01,
    /// The command succeeded.
    Successful = 0x02,
    /// The command failed.
    Failure = 0x03,
    /// The mouse did not answer in time.
    NoResponse = 0x04,
    /// The mouse does not support the command.
    NotSupported = 0x05,
}

impl TryFrom<u8> for Status {
    type Error = Error;

    fn try_from(byte: u8) -> Result<Self> {
        match byte {
            0x00 => Ok(Self::New),
            0x01 => Ok(Self::Busy),
            0x02 => Ok(Self::Successful),
            0x03 => Ok(Self::Failure),
            0x04 => Ok(Self::NoResponse),
            0x05 => Ok(Self::NotSupported),
            _ => Err(Error::InvalidResponse("unknown status")),
        }
    }
}

/// A command: its class, id and the number of argument bytes it uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Command {
    /// Command class.
    pub class: u8,
    /// Command id within the class.
    pub id: u8,
    /// Number of meaningful argument bytes.
    pub size: u8,
}

impl Command {
    /// Reads the battery level; the level is in argument 1, 0..=255.
    pub const BATTERY: Self = Self {
        class: 0x07,
        id: 0x80,
        size: 0x02,
    };
    /// Reads the charging state; argument 1 is non-zero while charging.
    pub const CHARGING: Self = Self {
        class: 0x07,
        id: 0x84,
        size: 0x02,
    };
    /// Reads the serial number as ASCII in the arguments.
    pub const SERIAL: Self = Self {
        class: 0x00,
        id: 0x82,
        size: 0x16,
    };
}

/// A decoded report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    /// Result code.
    pub status: Status,
    /// Transaction id the report belongs to.
    pub transaction_id: u8,
    /// Number of packets that follow this one.
    pub remaining_packets: u16,
    /// Protocol type, always zero for the commands used here.
    pub protocol_type: u8,
    /// Number of meaningful argument bytes.
    pub data_size: u8,
    /// Command class.
    pub command_class: u8,
    /// Command id.
    pub command_id: u8,
    /// Argument bytes.
    pub arguments: [u8; ARGUMENTS_LEN],
}

impl Report {
    /// Builds a new request for `command` with the given transaction id.
    pub fn request(transaction_id: u8, command: Command) -> Self {
        Self {
            status: Status::New,
            transaction_id,
            remaining_packets: 0,
            protocol_type: 0,
            data_size: command.size,
            command_class: command.class,
            command_id: command.id,
            arguments: [0; ARGUMENTS_LEN],
        }
    }

    /// Encodes the report, filling in the checksum.
    pub fn encode(&self) -> [u8; REPORT_LEN] {
        let mut bytes = [0; REPORT_LEN];
        bytes[0] = self.status as u8;
        bytes[1] = self.transaction_id;
        bytes[2..4].copy_from_slice(&self.remaining_packets.to_be_bytes());
        bytes[4] = self.protocol_type;
        bytes[5] = self.data_size;
        bytes[6] = self.command_class;
        bytes[7] = self.command_id;
        bytes[ARGUMENTS_START..CRC_INDEX].copy_from_slice(&self.arguments);
        bytes[CRC_INDEX] = crc(&bytes);
        bytes
    }

    /// Decodes a report, checking its length, checksum and status.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let Ok(bytes) = <&[u8; REPORT_LEN]>::try_from(bytes) else {
            return Err(Error::InvalidResponse("wrong report length"));
        };
        if crc(bytes) != bytes[CRC_INDEX] {
            return Err(Error::InvalidResponse("bad checksum"));
        }
        let mut arguments = [0; ARGUMENTS_LEN];
        arguments.copy_from_slice(&bytes[ARGUMENTS_START..CRC_INDEX]);
        Ok(Self {
            status: Status::try_from(bytes[0])?,
            transaction_id: bytes[1],
            remaining_packets: u16::from_be_bytes([bytes[2], bytes[3]]),
            protocol_type: bytes[4],
            data_size: bytes[5],
            command_class: bytes[6],
            command_id: bytes[7],
            arguments,
        })
    }
}

/// Checksum of a report: XOR of bytes 2 up to the checksum byte.
fn crc(bytes: &[u8; REPORT_LEN]) -> u8 {
    bytes[CRC_START..CRC_INDEX]
        .iter()
        .fold(0, |crc, byte| crc ^ byte)
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::{Command, Report, Status};

    /// Serial arguments captured from a `DeathAdder` V4 Pro (transaction id 0x1f).
    pub const SERIAL_ARGUMENTS: [u8; 15] = [
        0x50, 0x4d, 0x32, 0x35, 0x34, 0x32, 0x48, 0x33, 0x35, 0x31, 0x30, 0x34, 0x38, 0x39, 0x38,
    ];

    /// A successful response to `command` carrying `arguments` at the start.
    pub fn response(transaction_id: u8, command: Command, arguments: &[u8]) -> Report {
        let mut report = Report::request(transaction_id, command);
        report.status = Status::Successful;
        report.arguments[..arguments.len()].copy_from_slice(arguments);
        report
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{SERIAL_ARGUMENTS, response};
    use super::*;

    #[test]
    fn encodes_battery_request() {
        let bytes = Report::request(0x1f, Command::BATTERY).encode();
        assert_eq!(bytes.len(), REPORT_LEN);
        assert_eq!(bytes[..8], [0x00, 0x1f, 0x00, 0x00, 0x00, 0x02, 0x07, 0x80]);
        assert!(bytes[8..88].iter().all(|&byte| byte == 0));
        // Bytes 2..88 are all zero except size 0x02, class 0x07 and id 0x80.
        assert_eq!(bytes[88], 0x02 ^ 0x07 ^ 0x80);
        assert_eq!(bytes[89], 0);
    }

    #[test]
    fn crc_covers_bytes_two_to_eighty_seven() {
        let mut report = Report::request(0x3f, Command::BATTERY);
        report.arguments[0] = 0x55;
        let bytes = report.encode();
        // Status and transaction id (bytes 0 and 1) are not part of the checksum.
        assert_eq!(bytes[88], 0x02 ^ 0x07 ^ 0x80 ^ 0x55);
    }

    #[test]
    fn decodes_real_serial_capture() {
        let report = response(0x1f, Command::SERIAL, &SERIAL_ARGUMENTS);
        let decoded = Report::decode(&report.encode()).unwrap();
        assert_eq!(decoded, report);
        assert_eq!(decoded.status, Status::Successful);
        assert_eq!(decoded.transaction_id, 0x1f);
        assert_eq!(decoded.command_class, 0x00);
        assert_eq!(decoded.command_id, 0x82);
        assert_eq!(decoded.data_size, 0x16);
        assert_eq!(&decoded.arguments[..15], b"PM2542H35104898");
    }

    #[test]
    fn rejects_bad_checksum() {
        let mut bytes = response(0x1f, Command::SERIAL, &SERIAL_ARGUMENTS).encode();
        bytes[10] ^= 0x01;
        assert!(matches!(
            Report::decode(&bytes),
            Err(Error::InvalidResponse(_))
        ));
    }

    #[test]
    fn remaining_packets_is_big_endian() {
        let mut report = Report::request(0x1f, Command::BATTERY);
        report.remaining_packets = 0x0102;
        let bytes = report.encode();
        assert_eq!(bytes[2..4], [0x01, 0x02]);
        assert_eq!(Report::decode(&bytes).unwrap().remaining_packets, 0x0102);
    }
}
