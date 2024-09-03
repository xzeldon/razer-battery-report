use std::{
    collections::HashMap,
    rc::Rc,
    sync::{mpsc, Arc, Mutex},
    thread,
};

use crate::manager::DeviceManager;
use log::{error, info};
use tao::event_loop::EventLoopBuilder;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    TrayIcon, TrayIconBuilder,
};

const BATTERY_UPDATE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);
const DEVICE_FETCH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Debug)]
pub struct MemoryDevice {
    pub name: String,
    #[allow(unused)]
    pub pid: u32,
    pub battery_level: i32,
    pub old_battery_level: i32,
    pub is_charging: bool,
}

impl MemoryDevice {
    fn new(name: String, pid: u32) -> Self {
        Self {
            name,
            pid,
            battery_level: -1,
            old_battery_level: 50,
            is_charging: false,
        }
    }
}

pub struct TrayApp {
    device_manager: Arc<Mutex<DeviceManager>>,
    devices: Arc<Mutex<HashMap<u32, MemoryDevice>>>,
    tray_icon: Rc<Mutex<Option<TrayIcon>>>,
}

impl TrayApp {
    pub fn new() -> Self {
        Self {
            device_manager: Arc::new(Mutex::new(DeviceManager::new())),
            devices: Arc::new(Mutex::new(HashMap::new())),
            tray_icon: Rc::new(Mutex::new(None)),
        }
    }

    pub fn run(&self) {
        let icon = Self::create_icon();
        let event_loop = EventLoopBuilder::new().build();
        let tray_menu = Self::create_menu();

        let (sender, receiver) = mpsc::channel();

        self.spawn_device_fetch_thread(sender.clone());
        self.spawn_battery_check_thread(sender);

        self.run_event_loop(event_loop, icon, tray_menu, receiver);
    }

    fn create_icon() -> tray_icon::Icon {
        let icon = include_bytes!("../assets/mouse_white.png");
        let image = image::load_from_memory(icon)
            .expect("Failed to open icon")
            .into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();

        tray_icon::Icon::from_rgba(rgba, width, height).expect("Failed to create icon")
    }

    fn create_menu() -> Menu {
        let tray_menu = Menu::new();
        let quit_item = MenuItem::new("Exit", true, None);
        tray_menu.append_items(&[&quit_item]).unwrap();
        tray_menu
    }

    fn spawn_device_fetch_thread(&self, tx: mpsc::Sender<Vec<u32>>) {
        let devices = Arc::clone(&self.devices);
        let device_manager = Arc::clone(&self.device_manager);

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

                for &id in &connected_devices {
                    if let std::collections::hash_map::Entry::Vacant(e) = devices.entry(id) {
                        if let Some(name) = device_manager.lock().unwrap().get_device_name(id) {
                            e.insert(MemoryDevice::new(name.clone(), id));
                            info!("New device: {}", name);
                        } else {
                            error!("Failed to get device name for id: {}", id);
                        }
                    }
                }
            }

            if !connected_devices.is_empty() {
                tx.send(connected_devices).unwrap();
            }

            thread::sleep(DEVICE_FETCH_INTERVAL);
        });
    }

    fn spawn_battery_check_thread(&self, tx: mpsc::Sender<Vec<u32>>) {
        let devices = Arc::clone(&self.devices);

        thread::spawn(move || loop {
            let device_ids: Vec<u32> = devices.lock().unwrap().keys().cloned().collect();
            tx.send(device_ids).unwrap();
            thread::sleep(BATTERY_UPDATE_INTERVAL);
        });
    }

    fn run_event_loop(
        &self,
        event_loop: tao::event_loop::EventLoop<()>,
        icon: tray_icon::Icon,
        tray_menu: Menu,
        rx: mpsc::Receiver<Vec<u32>>,
    ) {
        let tray_icon = Rc::clone(&self.tray_icon);
        let devices = Arc::clone(&self.devices);
        let device_manager = Arc::clone(&self.device_manager);

        let menu_channel = MenuEvent::receiver();

        event_loop.run(move |event, _, control_flow| {
            *control_flow = tao::event_loop::ControlFlow::WaitUntil(
                std::time::Instant::now() + std::time::Duration::from_millis(100),
            );

            if let Ok(device_ids) = rx.try_recv() {
                Self::update(&devices, &device_manager, &device_ids, &tray_icon);
            }

            if let tao::event::Event::NewEvents(tao::event::StartCause::Init) = event {
                Self::create_tray_icon(&tray_icon, &tray_menu, icon.clone());
            }

            if let Ok(event) = menu_channel.try_recv() {
                if event.id == tray_menu.items()[0].id() {
                    *control_flow = tao::event_loop::ControlFlow::Exit;
                }
            }
        });
    }

    fn create_tray_icon(
        tray_icon: &Rc<Mutex<Option<TrayIcon>>>,
        tray_menu: &Menu,
        icon: tray_icon::Icon,
    ) {
        let tray_builder = TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu.clone()))
            .with_tooltip("Service is running")
            .with_icon(icon)
            .build();

        match tray_builder {
            Ok(tray) => *tray_icon.lock().unwrap() = Some(tray),
            Err(err) => error!("Failed to create tray icon: {}", err),
        }
    }

    fn update(
        devices: &Arc<Mutex<HashMap<u32, MemoryDevice>>>,
        manager: &Arc<Mutex<DeviceManager>>,
        device_ids: &[u32],
        tray_icon: &Rc<Mutex<Option<TrayIcon>>>,
    ) {
        let mut devices = devices.lock().unwrap();
        let manager = manager.lock().unwrap();

        for &id in device_ids {
            if let Some(device) = devices.get_mut(&id) {
                if let (Some(battery_level), Some(is_charging)) = (
                    manager.get_device_battery_level(id),
                    manager.is_device_charging(id),
                ) {
                    info!("{}  battery level: {}%", device.name, battery_level);
                    info!("{}  charging status: {}", device.name, is_charging);

                    device.old_battery_level = device.battery_level;
                    device.battery_level = battery_level;
                    device.is_charging = is_charging;

                    Self::check_notify(device);

                    if let Some(tray_icon) = tray_icon.lock().unwrap().as_mut() {
                        let _ = tray_icon
                            .set_tooltip(Some(format!("{}: {}%", device.name, battery_level)));
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
