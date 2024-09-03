use hidapi::HidApi;
use log::warn;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::vec::Vec;

use crate::controller::DeviceController;
use crate::devices::RAZER_DEVICE_LIST;

#[derive(Debug)]
pub struct DeviceManager {
    pub device_controllers: Arc<Mutex<Vec<DeviceController>>>,
}

impl DeviceManager {
    pub fn new() -> Self {
        Self {
            device_controllers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn fetch_devices(&mut self) -> (Vec<u32>, Vec<u32>) {
        let old_ids: HashSet<u32> = {
            let controllers = self.device_controllers.lock().unwrap();
            controllers
                .iter()
                .map(|controller| controller.pid as u32)
                .collect()
        };

        let new_controllers = self.get_connected_devices();
        let new_ids: HashSet<u32> = new_controllers
            .iter()
            .map(|controller| controller.pid as u32)
            .collect();

        let removed_devices: Vec<u32> = old_ids.difference(&new_ids).cloned().collect();
        let connected_devices: Vec<u32> = new_ids.difference(&old_ids).cloned().collect();

        *self.device_controllers.lock().unwrap() = new_controllers;

        (removed_devices, connected_devices)
    }

    pub fn get_device_name(&self, id: u32) -> Option<String> {
        let controllers = self.device_controllers.lock().unwrap();
        controllers
            .iter()
            .find(|controller| controller.pid as u32 == id)
            .map(|controller| controller.name.clone())
    }

    pub fn get_device_battery_level(&self, id: u32) -> Option<i32> {
        let controllers = self.device_controllers.lock().unwrap();
        let controller = controllers
            .iter()
            .find(|controller| controller.pid as u32 == id)?;

        match controller.get_battery_level() {
            Ok(level) => Some(level),
            Err(err) => {
                warn!("Failed to get battery level: {:?}", err);
                None
            }
        }
    }

    pub fn is_device_charging(&self, id: u32) -> Option<bool> {
        let controllers = self.device_controllers.lock().unwrap();
        let controller = controllers
            .iter()
            .find(|controller| controller.pid as u32 == id)?;

        match controller.get_charging_status() {
            Ok(status) => Some(status),
            Err(err) => {
                warn!("Failed to get charging status: {:?}", err);
                None
            }
        }
    }

    fn get_connected_devices(&self) -> Vec<DeviceController> {
        let mut connected_devices = Vec::new();
        let mut added_devices = HashSet::new();

        for device in RAZER_DEVICE_LIST.iter() {
            // Create a new HidApi instance
            let api = match HidApi::new() {
                Ok(api) => api,
                Err(err) => {
                    warn!("Failed to initialize HidApi: {:?}", err);
                    continue;
                }
            };

            // Iterate over the device list to find matching devices
            for hid_device in api.device_list() {
                if hid_device.vendor_id() == device.vid
                    && hid_device.product_id() == device.pid
                    && hid_device.interface_number() == device.interface.into()
                {
                    // Check platform-specific usage if on Windows
                    if cfg!(target_os = "windows")
                        && (hid_device.usage_page() != device.usage_page
                            || hid_device.usage() != device.usage)
                    {
                        continue;
                    }

                    // Only add the device if it hasn't been added yet
                    if !added_devices.contains(&device.pid) {
                        // Create a new DeviceController
                        match DeviceController::new(
                            device.name.to_owned(),
                            device.pid,
                            hid_device.path().to_string_lossy().into_owned(),
                        ) {
                            Ok(controller) => {
                                connected_devices.push(controller);
                                added_devices.insert(device.pid);
                            }
                            Err(err) => warn!("Failed to create device controller: {:?}", err),
                        }
                    }
                }
            }
        }

        connected_devices
    }
}
