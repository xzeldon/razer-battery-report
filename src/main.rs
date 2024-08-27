use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use log::{error, info};
use manager::DeviceManager;

mod controller;
mod devices;
mod manager;

const BATTERY_UPDATE_INTERVAL: u64 = 60; // seconds
const DEVICE_FETCH_INTERVAL: u64 = 5; // seconds

struct MemoryDevice {
    name: String,
    #[allow(unused)]
    id: u32,
    battery_level: i32,
    old_battery_level: i32,
    is_charging: bool,
}

impl MemoryDevice {
    fn new(name: String, id: u32) -> Self {
        MemoryDevice {
            name,
            id,
            battery_level: -1,
            old_battery_level: 50,
            is_charging: false,
        }
    }
}

struct BatteryChecker {
    device_manager: Arc<Mutex<DeviceManager>>,
    devices: Arc<Mutex<HashMap<u32, MemoryDevice>>>,
}

impl BatteryChecker {
    fn new() -> Self {
        BatteryChecker {
            device_manager: Arc::new(Mutex::new(DeviceManager::new())),
            devices: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn run(&self) {
        let devices = Arc::clone(&self.devices);
        let device_manager = Arc::clone(&self.device_manager);

        // Device fetching thread
        thread::spawn(move || loop {
            let (removed_devices, connected_devices) = {
                let mut manager = device_manager.lock().unwrap();
                manager.fetch_devices()
            };

            {
                let mut devices = devices.lock().unwrap();
                for id in removed_devices {
                    if let Some(device) = devices.remove(&id) {
                        info!("Device removed: {}", device.name);
                    }
                }

                for id in &connected_devices {
                    if !devices.contains_key(id) {
                        if let Some(name) = device_manager.lock().unwrap().get_device_name(*id) {
                            devices.insert(*id, MemoryDevice::new(name.clone(), *id));
                            info!("New device: {}", name);
                        } else {
                            error!("Failed to get device name for id: {}", id);
                        }
                    }
                }
            }

            if !connected_devices.is_empty() {
                Self::update(&devices, &device_manager, &connected_devices);
            }

            thread::sleep(Duration::from_secs(DEVICE_FETCH_INTERVAL));
        });

        // Battery check thread
        loop {
            let device_ids: Vec<u32> = {
                let devices = self.devices.lock().unwrap();
                devices.keys().cloned().collect()
            };
            Self::update(&self.devices, &self.device_manager, &device_ids);
            thread::sleep(Duration::from_secs(BATTERY_UPDATE_INTERVAL));
        }
    }

    fn update(
        devices: &Arc<Mutex<HashMap<u32, MemoryDevice>>>,
        manager: &Arc<Mutex<DeviceManager>>,
        device_ids: &[u32],
    ) {
        let mut devices = devices.lock().unwrap();
        let manager = manager.lock().unwrap();

        for &id in device_ids {
            if let Some(device) = devices.get_mut(&id) {
                if let Some(battery_level) = manager.get_device_battery_level(id) {
                    if let Some(is_charging) = manager.is_device_charging(id) {
                        info!("{}  battery level: {}%", device.name, battery_level);
                        info!("{}  charging status: {}", device.name, is_charging);

                        device.old_battery_level = device.battery_level;
                        device.battery_level = battery_level;
                        device.is_charging = is_charging;

                        Self::check_notify(device);
                    }
                }
            }
        }
    }

    fn check_notify(device: &MemoryDevice) {
        if device.battery_level == -1 {
            return;
        }

        if !device.is_charging
            && (device.battery_level <= 5
                || (device.old_battery_level > 15 && device.battery_level <= 15))
        {
            info!("{}: Battery low ({}%)", device.name, device.battery_level);
        } else if device.old_battery_level <= 99
            && device.battery_level == 100
            && device.is_charging
        {
            info!(
                "{}: Battery fully charged ({}%)",
                device.name, device.battery_level
            );
        }
    }
}

fn main() {
    std::env::set_var("RUST_LOG", "trace");
    pretty_env_logger::init();
    let checker = BatteryChecker::new();
    checker.run();
}
