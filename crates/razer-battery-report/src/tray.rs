use std::{
    collections::{HashMap, HashSet, hash_map::Entry},
    rc::Rc,
    sync::{Arc, OnceLock},
    thread,
    time::Duration,
};

use crate::{console::DebugConsole, manager::DeviceManager, notify::Notify};
use log::{error, info, trace, warn};
use parking_lot::Mutex;
use tao::event_loop::{EventLoopBuilder, EventLoopProxy};
use tray_icon::{
    TrayIcon, TrayIconBuilder,
    menu::{IsMenuItem, Menu, MenuEvent, MenuItem},
};

const BATTERY_UPDATE_INTERVAL: u64 = 300; // 5 min
const DEVICE_FETCH_INTERVAL: Duration = Duration::from_secs(5);

const BATTERY_CRITICAL_LEVEL: i32 = 5;
const BATTERY_LOW_LEVEL: i32 = 15;

struct CachedIcon {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

fn load_icon(icon_data: &[u8]) -> CachedIcon {
    let image = image::load_from_memory(icon_data)
        .expect("Failed to decode embedded icon")
        .into_rgba8();
    let (width, height) = image.dimensions();
    CachedIcon {
        rgba: image.into_raw(),
        width,
        height,
    }
}

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
    menu_items: Rc<Mutex<Vec<MenuItem>>>,
    debug_console: Rc<DebugConsole>,
}

impl TrayInner {
    fn new(debug_console: Rc<DebugConsole>) -> Self {
        Self {
            tray_icon: Rc::new(Mutex::new(None)),
            menu_items: Rc::new(Mutex::new(Vec::new())),
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

        let item_refs: Vec<&dyn IsMenuItem> = menu_items
            .iter()
            .map(|item| item as &dyn IsMenuItem)
            .collect();

        if let Err(e) = tray_menu.append_items(&item_refs) {
            warn!("Failed to append menu items: {}", e);
        }
        tray_menu
    }

    fn build_tray(
        tray_icon: &Rc<Mutex<Option<TrayIcon>>>,
        tray_menu: &Menu,
        icon: tray_icon::Icon,
    ) {
        let tray_builder = TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu.clone()))
            .with_tooltip("Search for devices")
            .with_icon(icon)
            .build();

        match tray_builder {
            Ok(tray) => *tray_icon.lock() = Some(tray),
            Err(err) => error!("Failed to create tray icon: {}", err),
        }
    }
}

pub struct TrayApp {
    devices: Arc<Mutex<HashMap<u32, MemoryDevice>>>,
    tray_inner: TrayInner,
    notify: Arc<Notify>,
}

#[derive(Debug)]
enum TrayEvent {
    DeviceUpdate,
    MenuEvent(MenuEvent),
}

impl TrayApp {
    pub fn new(debug_console: DebugConsole) -> Self {
        Self {
            devices: Arc::new(Mutex::new(HashMap::new())),
            tray_inner: TrayInner::new(Rc::new(debug_console)),
            notify: Arc::new(Notify::new()),
        }
    }

    pub fn run(&self) {
        let icon = match Self::create_icon() {
            Ok(icon) => icon,
            Err(e) => {
                error!("{}", e);
                return;
            }
        };
        let event_loop = EventLoopBuilder::with_user_event().build();
        let tray_menu = self.tray_inner.create_menu();

        let proxy = event_loop.create_proxy();

        self.spawn_worker_thread(proxy.clone());

        self.run_event_loop(event_loop, icon, tray_menu, proxy);
    }

    fn create_icon() -> Result<tray_icon::Icon, String> {
        let icon = include_bytes!("../../../assets/mouse_white.png");
        let image = match image::load_from_memory(icon) {
            Ok(image) => image.into_rgba8(),
            Err(e) => return Err(format!("Failed to open icon: {}", e)),
        };
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();

        tray_icon::Icon::from_rgba(rgba, width, height)
            .map_err(|e| format!("Failed to create icon: {}", e))
    }

    fn spawn_worker_thread(&self, proxy: EventLoopProxy<TrayEvent>) {
        let devices = Arc::clone(&self.devices);
        let notify = Arc::clone(&self.notify);

        thread::spawn(move || {
            let mut manager = DeviceManager::new();
            let mut last_devices = HashSet::new();
            let mut battery_update_counter = 0u64;

            loop {
                let (removed_devices, connected_devices) = manager.fetch_devices();

                let mut guard = devices.lock();

                for id in &removed_devices {
                    if let Some(device) = guard.remove(id) {
                        info!("Device removed: {}", device.name);
                        let _ = notify.device_disconnected(&device.name);
                    }
                }

                for &id in &connected_devices {
                    if let Entry::Vacant(e) = guard.entry(id) {
                        if let Some(name) = manager.get_device_name(id) {
                            e.insert(MemoryDevice::new(name.clone(), id));
                            info!("New device: {}", name);
                            let _ = notify.device_connected(&name);
                        } else {
                            error!("Failed to get device name for id: {}", id);
                        }
                    }
                }

                if battery_update_counter == 0
                    || !connected_devices.is_empty()
                    || !removed_devices.is_empty()
                {
                    for (&id, device) in guard.iter_mut() {
                        if battery_update_counter == 0 || connected_devices.contains(&id) {
                            let old_level = device.battery_level;
                            let old_charging = device.is_charging;

                            if let Some(level) = manager.get_device_battery_level(id) {
                                info!("{}  battery level: {}%", device.name, level);
                                device.old_battery_level = device.battery_level;
                                device.battery_level = level;
                            }
                            if let Some(charging) = manager.is_device_charging(id) {
                                info!("{}  charging status: {}", device.name, charging);
                                device.is_charging = charging;
                            }

                            if device.battery_level != old_level
                                || device.is_charging != old_charging
                            {
                                check_notify(device, &notify);
                            }
                        }
                    }
                }

                let current_devices: HashSet<u32> = manager.device_pids();
                if current_devices != last_devices || battery_update_counter == 0 {
                    let _ = proxy.send_event(TrayEvent::DeviceUpdate);
                }
                last_devices = current_devices;

                battery_update_counter = (battery_update_counter + 1)
                    % (BATTERY_UPDATE_INTERVAL / DEVICE_FETCH_INTERVAL.as_secs());

                drop(guard);
                thread::sleep(DEVICE_FETCH_INTERVAL);
            }
        });
    }

    fn run_event_loop(
        &self,
        event_loop: tao::event_loop::EventLoop<TrayEvent>,
        icon: tray_icon::Icon,
        tray_menu: Menu,
        proxy: EventLoopProxy<TrayEvent>,
    ) {
        let devices = Arc::clone(&self.devices);
        let tray_icon = Rc::clone(&self.tray_inner.tray_icon);
        let debug_console = Rc::clone(&self.tray_inner.debug_console);
        let menu_items = Rc::clone(&self.tray_inner.menu_items);

        let menu_channel = MenuEvent::receiver();

        event_loop.run(move |event, _, control_flow| {
            *control_flow = tao::event_loop::ControlFlow::Wait;

            match event {
                tao::event::Event::NewEvents(tao::event::StartCause::Init) => {
                    TrayInner::build_tray(&tray_icon, &tray_menu, icon.clone());
                }
                tao::event::Event::UserEvent(TrayEvent::DeviceUpdate) => {
                    Self::update_tray_ui(&devices, &tray_icon);
                }
                tao::event::Event::UserEvent(TrayEvent::MenuEvent(event)) => {
                    let menu_items = menu_items.lock();

                    if event.id == menu_items[0].id() {
                        debug_console.toggle_visibility();
                        let visible = debug_console.is_visible();
                        menu_items[0].set_text(if visible {
                            "Hide Log Window"
                        } else {
                            "Show Log Window"
                        });
                        trace!("{} log window", if visible { "showing" } else { "hiding" });
                    }

                    if event.id == menu_items[1].id() {
                        *control_flow = tao::event_loop::ControlFlow::Exit;
                    }
                }
                _ => (),
            }

            if let Ok(event) = menu_channel.try_recv() {
                let _ = proxy.send_event(TrayEvent::MenuEvent(event));
            }
        });
    }

    fn update_tray_ui(
        devices: &Arc<Mutex<HashMap<u32, MemoryDevice>>>,
        tray_icon: &Rc<Mutex<Option<TrayIcon>>>,
    ) {
        let guard = devices.lock();

        if guard.is_empty() {
            if let Some(tray) = tray_icon.lock().as_mut() {
                let _ = tray.set_tooltip(Some(String::from("No Razer devices detected")));
            }
            return;
        }

        let Some(device) = guard.values().next() else {
            return;
        };

        if device.battery_level == -1 {
            return;
        }

        if let Ok(new_icon) = Self::get_battery_icon(device.battery_level, device.is_charging) {
            if let Some(tray) = tray_icon.lock().as_mut() {
                let _ = tray.set_icon(Some(new_icon));
                let _ = tray.set_tooltip(Some(format!(
                    "{}: {}%{}",
                    device.name,
                    device.battery_level,
                    if device.is_charging {
                        " (charging)"
                    } else {
                        ""
                    }
                )));
            }
        }
    }

    fn get_battery_icon(battery_level: i32, is_charging: bool) -> Result<tray_icon::Icon, String> {
        static WHITE: OnceLock<CachedIcon> = OnceLock::new();
        static YELLOW: OnceLock<CachedIcon> = OnceLock::new();
        static RED: OnceLock<CachedIcon> = OnceLock::new();

        let cached = match (battery_level, is_charging) {
            (lvl, false) if lvl <= BATTERY_CRITICAL_LEVEL => {
                RED.get_or_init(|| load_icon(include_bytes!("../../../assets/mouse_red.png")))
            }
            (lvl, false) if lvl <= BATTERY_LOW_LEVEL => {
                YELLOW.get_or_init(|| load_icon(include_bytes!("../../../assets/mouse_yellow.png")))
            }
            _ => WHITE.get_or_init(|| load_icon(include_bytes!("../../../assets/mouse_white.png"))),
        };

        tray_icon::Icon::from_rgba(cached.rgba.clone(), cached.width, cached.height)
            .map_err(|e| format!("Failed to create icon: {}", e))
    }
}

fn check_notify(device: &MemoryDevice, notify: &Notify) {
    if device.battery_level == -1 {
        return;
    }

    if !device.is_charging
        && (device.battery_level <= BATTERY_CRITICAL_LEVEL
            || (device.old_battery_level > BATTERY_LOW_LEVEL
                && device.battery_level <= BATTERY_LOW_LEVEL))
    {
        info!("{}: Battery low ({}%)", device.name, device.battery_level);
        if let Err(e) = notify.battery_low(&device.name, device.battery_level) {
            warn!("Failed to send low battery notification: {}", e);
        }
    } else if device.old_battery_level <= 99 && device.battery_level == 100 && device.is_charging {
        info!(
            "{}: Battery fully charged ({}%)",
            device.name, device.battery_level
        );
        if let Err(e) = notify.battery_full(&device.name) {
            warn!("Failed to send full battery notification: {}", e);
        }
    }
}
