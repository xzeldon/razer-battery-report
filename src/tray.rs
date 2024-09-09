use std::{
    collections::HashMap,
    rc::Rc,
    sync::{mpsc, Arc},
    thread,
    time::{Duration, Instant},
};

use crate::{console::DebugConsole, manager::DeviceManager};
use log::{error, info, trace};
use parking_lot::Mutex;
use tao::event_loop::EventLoopBuilder;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    TrayIcon, TrayIconBuilder,
};

const BATTERY_UPDATE_INTERVAL: Duration = Duration::from_secs(60);
const DEVICE_FETCH_INTERVAL: Duration = Duration::from_secs(5);

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

pub struct TrayInner {
    tray_icon: Rc<Mutex<Option<TrayIcon>>>,
    menu_items: Arc<Mutex<Vec<MenuItem>>>,
    debug_console: Arc<DebugConsole>,
}

impl TrayInner {
    fn new(debug_console: Arc<DebugConsole>) -> Self {
        Self {
            tray_icon: Rc::new(Mutex::new(None)),
            menu_items: Arc::new(Mutex::new(Vec::new())),
            debug_console,
        }
    }

    fn create_menu(&self) -> Menu {
        let tray_menu = Menu::new();

        let show_console_item = MenuItem::new("Show Log Window", true, None);
        let quit_item = MenuItem::new("Exit", true, None);

        let mut menu_items = self.menu_items.lock();
        menu_items.push(show_console_item);
        menu_items.push(quit_item);

        tray_menu
            .append_items(&[&menu_items[0], &menu_items[1]])
            .unwrap();
        tray_menu
    }

    fn build_tray(
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
            Ok(tray) => *tray_icon.lock() = Some(tray),
            Err(err) => error!("Failed to create tray icon: {}", err),
        }
    }
}

pub struct TrayApp {
    device_manager: Arc<Mutex<DeviceManager>>,
    devices: Arc<Mutex<HashMap<u32, MemoryDevice>>>,
    tray_inner: TrayInner,
}

impl TrayApp {
    pub fn new(debug_console: DebugConsole) -> Self {
        Self {
            device_manager: Arc::new(Mutex::new(DeviceManager::new())),
            devices: Arc::new(Mutex::new(HashMap::new())),
            tray_inner: TrayInner::new(Arc::new(debug_console)),
        }
    }

    pub fn run(&self) {
        let icon = Self::create_icon();
        let event_loop = EventLoopBuilder::new().build();
        let tray_menu = self.tray_inner.create_menu();

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

    fn spawn_device_fetch_thread(&self, tx: mpsc::Sender<Vec<u32>>) {
        let devices = Arc::clone(&self.devices);
        let device_manager = Arc::clone(&self.device_manager);

        thread::spawn(move || loop {
            let (removed_devices, connected_devices) = {
                let mut manager = device_manager.lock();
                manager.fetch_devices()
            };

            {
                let mut devices = devices.lock();
                for id in removed_devices {
                    if let Some(device) = devices.remove(&id) {
                        info!("Device removed: {}", device.name);
                    }
                }

                for &id in &connected_devices {
                    if let std::collections::hash_map::Entry::Vacant(e) = devices.entry(id) {
                        if let Some(name) = device_manager.lock().get_device_name(id) {
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
            let device_ids: Vec<u32> = devices.lock().keys().cloned().collect();
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
        let devices = Arc::clone(&self.devices);
        let device_manager = Arc::clone(&self.device_manager);
        let tray_icon = Rc::clone(&self.tray_inner.tray_icon);
        let debug_console = Arc::clone(&self.tray_inner.debug_console);
        let menu_items = Arc::clone(&self.tray_inner.menu_items);

        let menu_channel = MenuEvent::receiver();

        event_loop.run(move |event, _, control_flow| {
            *control_flow = tao::event_loop::ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(100),
            );

            if let Ok(device_ids) = rx.try_recv() {
                Self::update(&devices, &device_manager, &device_ids, &tray_icon);
            }

            if let tao::event::Event::NewEvents(tao::event::StartCause::Init) = event {
                // We create the icon once the event loop is actually running
                // to prevent issues like https://github.com/tauri-apps/tray-icon/issues/90
                TrayInner::build_tray(&tray_icon, &tray_menu, icon.clone());
            }

            if let Ok(event) = menu_channel.try_recv() {
                let menu_items = menu_items.lock();

                let show_console_item = &menu_items[0];
                let quit_item = &menu_items[1];

                if event.id == show_console_item.id() {
                    debug_console.toggle_visibility();
                    let visible = debug_console.is_visible();
                    show_console_item.set_text(if visible {
                        "Hide Log Window"
                    } else {
                        "Show Log Window"
                    });
                    trace!("{} log window", if visible { "showing" } else { "hiding" });
                }

                if event.id == quit_item.id() {
                    *control_flow = tao::event_loop::ControlFlow::Exit;
                }
            }
        });
    }

    fn update(
        devices: &Arc<Mutex<HashMap<u32, MemoryDevice>>>,
        manager: &Arc<Mutex<DeviceManager>>,
        device_ids: &[u32],
        tray_icon: &Rc<Mutex<Option<TrayIcon>>>,
    ) {
        let mut devices = devices.lock();
        let manager = manager.lock();

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

                    if let Some(tray_icon) = tray_icon.lock().as_mut() {
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
