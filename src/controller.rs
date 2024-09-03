use hidapi::{HidApi, HidDevice};
use log::{info, warn};
use std::ffi::CString;
use std::thread;
use std::time::Duration;

use crate::devices::RAZER_DEVICE_LIST;

const MAX_TRIES_SEND: u8 = 10;
const TIME_BETWEEN_SEND: Duration = Duration::from_millis(500);

pub struct RazerReport {
    pub status: u8,
    pub transaction_id: u8,
    pub remaining_packets: u16,
    pub protocol_type: u8,
    pub data_size: u8,
    pub command_class: u8,
    pub command_id: u8,
    pub arguments: [u8; 80],
    pub crc: u8,
    pub reserved: u8,
}

impl RazerReport {
    pub const STATUS_NEW_COMMAND: u8 = 0x00;
    pub const STATUS_BUSY: u8 = 0x01;
    pub const STATUS_SUCCESSFUL: u8 = 0x02;
    pub const STATUS_FAILURE: u8 = 0x03;
    pub const STATUS_NO_RESPONSE: u8 = 0x04;
    pub const STATUS_NOT_SUPPORTED: u8 = 0x05;

    pub fn new() -> Self {
        RazerReport {
            status: 0,
            transaction_id: 0,
            remaining_packets: 0,
            protocol_type: 0,
            data_size: 0,
            command_class: 0,
            command_id: 0,
            arguments: [0; 80],
            crc: 0,
            reserved: 0,
        }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() != 90 {
            return Err("Expected 90 bytes of data as razer report");
        }

        let mut report = RazerReport::new();
        report.status = data[0];
        report.transaction_id = data[1];
        report.remaining_packets = u16::from_be_bytes([data[2], data[3]]);
        report.protocol_type = data[4];
        report.data_size = data[5];
        report.command_class = data[6];
        report.command_id = data[7];
        report.arguments.copy_from_slice(&data[8..88]);
        report.crc = data[88];
        report.reserved = data[89];

        Ok(report)
    }

    pub fn pack(&self) -> Vec<u8> {
        let mut data = vec![
            self.status,
            self.transaction_id,
            (self.remaining_packets >> 8) as u8,
            (self.remaining_packets & 0xFF) as u8,
            self.protocol_type,
            self.data_size,
            self.command_class,
            self.command_id,
        ];
        data.extend_from_slice(&self.arguments);
        data.push(self.crc);
        data.push(self.reserved);
        data
    }

    pub fn calculate_crc(&self) -> u8 {
        let data = self.pack();
        data[2..88].iter().fold(0, |crc, &byte| crc ^ byte)
    }

    pub fn is_valid(&self) -> bool {
        self.calculate_crc() == self.crc
    }
}

#[derive(Debug)]
pub struct DeviceController {
    pub handle: HidDevice,
    pub name: String,
    pub pid: u16,
    pub report_id: u8,
    pub transaction_id: u8,
}

impl DeviceController {
    pub fn new(name: String, pid: u16, path: String) -> Result<Self, Box<dyn std::error::Error>> {
        let api = HidApi::new()?;

        let c_path = CString::new(path)?;
        let handle = api.open_path(c_path.as_ref())?;

        let transaction_id = RAZER_DEVICE_LIST
            .iter()
            .find(|device| device.pid == pid)
            .map_or(0x3F, |device| device.transaction_id());

        Ok(DeviceController {
            handle,
            name,
            pid,
            report_id: 0x00,
            transaction_id,
        })
    }

    pub fn get_battery_level(&self) -> Result<i32, Box<dyn std::error::Error>> {
        let request = self.create_command(0x07, 0x80, 0x02);
        let response = self.send_payload(request)?;
        let battery_level = (response.arguments[1] as f32 / 255.0) * 100.0;
        Ok(battery_level.round() as i32)
    }

    pub fn get_charging_status(&self) -> Result<bool, Box<dyn std::error::Error>> {
        let request = self.create_command(0x07, 0x84, 0x02);
        let response = self.send_payload(request)?;
        let charging_status = response.arguments[1] != 0;
        Ok(charging_status)
    }

    pub fn send_payload(
        &self,
        mut request: RazerReport,
    ) -> Result<RazerReport, Box<dyn std::error::Error>> {
        request.crc = request.calculate_crc();

        for _ in 0..MAX_TRIES_SEND {
            self.usb_send(&request)?;
            let response = self.usb_receive()?;

            if response.remaining_packets != request.remaining_packets
                || response.command_class != request.command_class
                || response.command_id != request.command_id
            {
                return Err("Response doesn't match request".into());
            }

            match response.status {
                RazerReport::STATUS_SUCCESSFUL => return Ok(response),
                RazerReport::STATUS_BUSY => info!("Device is busy"),
                RazerReport::STATUS_NO_RESPONSE => info!("Command timed out"),
                RazerReport::STATUS_NOT_SUPPORTED => return Err("Command not supported".into()),
                RazerReport::STATUS_FAILURE => return Err("Command failed".into()),
                _ => return Err("Error unknown report status".into()),
            }

            thread::sleep(TIME_BETWEEN_SEND);
            warn!("Trying to resend command");
        }

        Err(format!("Abort command (tries: {})", MAX_TRIES_SEND).into())
    }

    pub fn create_command(&self, command_class: u8, command_id: u8, data_size: u8) -> RazerReport {
        let mut report = RazerReport::new();
        report.status = RazerReport::STATUS_NEW_COMMAND;
        report.transaction_id = self.transaction_id;
        report.command_class = command_class;
        report.command_id = command_id;
        report.data_size = data_size;
        report
    }

    pub fn usb_send(&self, report: &RazerReport) -> Result<(), Box<dyn std::error::Error>> {
        let mut data = vec![self.report_id];
        data.extend_from_slice(&report.pack());
        self.handle.send_feature_report(&data)?;
        thread::sleep(Duration::from_millis(60));
        Ok(())
    }

    pub fn usb_receive(&self) -> Result<RazerReport, Box<dyn std::error::Error>> {
        let expected_length = 91;
        let mut buf = vec![0u8; expected_length];
        let bytes_read = self.handle.get_feature_report(&mut buf)?;

        if bytes_read != expected_length {
            return Err("Error while getting feature report".into());
        }

        let report = RazerReport::from_bytes(&buf[1..])?;
        if !report.is_valid() {
            return Err("Get report has no valid crc".into());
        }

        Ok(report)
    }
}
