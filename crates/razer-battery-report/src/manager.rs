use hidapi::HidApi;
use log::warn;
use std::collections::{HashMap, HashSet};

use crate::controller::DeviceController;
use crate::devices::{DeviceInfo, RAZER_DEVICE_LIST};

pub struct DeviceManager {
    api: Option<HidApi>,
    device_controllers: Vec<DeviceController>,
}

impl DeviceManager {
    pub fn new() -> Self {
        Self {
            api: None,
            device_controllers: Vec::new(),
        }
    }

    pub fn fetch_devices(&mut self) -> (Vec<u32>, Vec<u32>) {
        if self.api.is_none() {
            self.api = HidApi::new()
                .inspect_err(|e| warn!("Failed to initialize HID API: {e}"))
                .ok();
            if self.api.is_none() {
                return (vec![], vec![]);
            }
        }

        {
            let api = self.api.as_mut().unwrap();
            if let Err(e) = api.refresh_devices() {
                warn!("Failed to refresh device list: {:?}", e);
            }
        }

        let old_ids: HashSet<u32> = self
            .device_controllers
            .iter()
            .map(|c| c.pid as u32)
            .collect();

        let new_controllers = {
            let api = self.api.as_ref().unwrap();
            self.get_connected_devices(api)
        };
        let new_ids: HashSet<u32> = new_controllers.iter().map(|c| c.pid as u32).collect();

        let removed_devices: Vec<u32> = old_ids.difference(&new_ids).cloned().collect();
        let connected_devices: Vec<u32> = new_ids.difference(&old_ids).cloned().collect();

        self.device_controllers = new_controllers;

        (removed_devices, connected_devices)
    }

    pub fn device_pids(&self) -> HashSet<u32> {
        self.device_controllers
            .iter()
            .map(|c| c.pid as u32)
            .collect()
    }

    pub fn get_device_name(&self, id: u32) -> Option<String> {
        self.device_controllers
            .iter()
            .find(|c| c.pid as u32 == id)
            .map(|c| c.name.clone())
    }

    pub fn get_device_battery_level(&self, id: u32) -> Option<i32> {
        let controller = self
            .device_controllers
            .iter()
            .find(|c| c.pid as u32 == id)?;

        match controller.get_battery_level() {
            Ok(level) => Some(level),
            Err(err) => {
                warn!("Failed to get battery level: {:?}", err);
                None
            }
        }
    }

    pub fn is_device_charging(&self, id: u32) -> Option<bool> {
        let controller = self
            .device_controllers
            .iter()
            .find(|c| c.pid as u32 == id)?;

        if controller.swappable_battery {
            return Some(false);
        }

        match controller.get_charging_status() {
            Ok(status) => Some(status),
            Err(err) => {
                warn!("Failed to get charging status: {:?}", err);
                None
            }
        }
    }

    fn get_connected_devices(&self, api: &HidApi) -> Vec<DeviceController> {
        let razer_devices: HashMap<(u16, u16), &DeviceInfo> = RAZER_DEVICE_LIST
            .iter()
            .map(|d| ((d.vid, d.pid), d))
            .collect();

        api.device_list()
            .filter_map(|hid_device| {
                razer_devices
                    .get(&(hid_device.vendor_id(), hid_device.product_id()))
                    .and_then(|device| {
                        if hid_device.interface_number() != device.interface.into() {
                            return None;
                        }
                        if cfg!(target_os = "windows")
                            && (hid_device.usage_page() != device.usage_page
                                || hid_device.usage() != device.usage)
                        {
                            return None;
                        }
                        DeviceController::new(
                            device.name.to_owned(),
                            device.pid,
                            hid_device.path().to_string_lossy().into_owned(),
                        )
                        .map_err(|err| warn!("Failed to create device controller: {:?}", err))
                        .ok()
                    })
            })
            .collect()
    }
}
