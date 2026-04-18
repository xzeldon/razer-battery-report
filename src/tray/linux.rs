use std::sync::{Arc, RwLock, mpsc};

use log::error;

use crate::config::{APP_DISPLAY_NAME, APP_NAME};
use crate::icon::IconSet;
use crate::state::DeviceState;
use crate::tray::TrayEvent;

pub struct AppTray {
    handle: Option<ksni::Handle<KsniTray>>,
    cmd_rx: Option<mpsc::Receiver<TrayEvent>>,
    state: Arc<RwLock<TrayState>>,
}

impl AppTray {
    pub fn new(autostart: bool, notifications: bool) -> anyhow::Result<Self> {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let state = Arc::new(RwLock::new(TrayState {
            devices: Vec::new(),
            active_device_path: None,
            icons: None,
            low_threshold: 15,
            critical_threshold: 5,
            autostart_enabled: autostart,
            notifications_enabled: notifications,
        }));

        let tray = KsniTray::new(state.clone(), cmd_tx);
        let handle = spawn_ksni(tray)?;

        Ok(Self {
            handle: Some(handle),
            cmd_rx: Some(cmd_rx),
            state,
        })
    }

    pub fn init(&mut self, icons: &IconSet) -> anyhow::Result<()> {
        let handle = match self.handle.as_ref() {
            Some(h) => h,
            None => return Ok(()),
        };

        let icons = icons.clone();
        let _ = async_io::block_on(handle.update(|tray| {
            let mut s = tray.state.write().unwrap();
            s.icons = Some(icons);
        }));

        Ok(())
    }

    pub fn update(
        &mut self,
        devices: &[DeviceState],
        active_device_path: &str,
        icons: &IconSet,
        low_threshold: u8,
        critical_threshold: u8,
    ) {
        let devices = devices.to_vec();
        let active_path = active_device_path.to_string();
        let icons = icons.clone();

        let handle = match self.handle.as_ref() {
            Some(h) => h,
            None => return,
        };

        let result = async_io::block_on(handle.update(move |tray| {
            let mut s = tray.state.write().unwrap();
            s.devices = devices;
            s.active_device_path = Some(active_path);
            s.icons = Some(icons);
            s.low_threshold = low_threshold;
            s.critical_threshold = critical_threshold;
        }));
        if result.is_none() {
            error!("Failed to update ksni tray — service may have shut down");
        }
    }

    pub fn set_no_devices_state(&mut self, icons: &IconSet) {
        let handle = match self.handle.as_ref() {
            Some(h) => h,
            None => return,
        };

        let icons = icons.clone();
        let _ = async_io::block_on(handle.update(|tray| {
            let mut s = tray.state.write().unwrap();
            s.devices.clear();
            s.active_device_path = None;
            s.icons = Some(icons);
        }));
    }

    pub fn spawn_command_receiver(
        &mut self,
        proxy: tao::event_loop::EventLoopProxy<crate::AppEvent>,
    ) {
        use crate::AppEvent;
        use std::thread;

        let cmd_rx = self.cmd_rx.take().expect("command receiver already taken");

        thread::spawn(move || {
            loop {
                match cmd_rx.recv() {
                    Ok(event) => {
                        let _ = proxy.send_event(AppEvent::TrayEvent(event));
                    }
                    Err(mpsc::RecvError) => {
                        error!("Command channel closed, exiting receiver thread");
                        break;
                    }
                }
            }
        });
    }

    pub fn set_autostart(&self, enabled: bool) {
        if self.state.read().unwrap().autostart_enabled == enabled {
            return;
        }

        let handle = match self.handle.as_ref() {
            Some(h) => h,
            None => return,
        };

        let _ = async_io::block_on(handle.update(move |tray| {
            let mut s = tray.state.write().unwrap();
            s.autostart_enabled = enabled;
        }));
    }

    pub fn set_notifications(&self, enabled: bool) {
        if self.state.read().unwrap().notifications_enabled == enabled {
            return;
        }

        let handle = match self.handle.as_ref() {
            Some(h) => h,
            None => return,
        };

        let _ = async_io::block_on(handle.update(move |tray| {
            let mut s = tray.state.write().unwrap();
            s.notifications_enabled = enabled;
        }));
    }
}

pub struct TrayState {
    pub devices: Vec<DeviceState>,
    pub active_device_path: Option<String>,
    pub icons: Option<IconSet>,
    pub low_threshold: u8,
    pub critical_threshold: u8,
    pub autostart_enabled: bool,
    pub notifications_enabled: bool,
}

impl TrayState {
    fn get_current_icon(&self) -> Option<ksni::Icon> {
        let icons = self.icons.as_ref()?;
        if self.active_device_path.is_none() || self.devices.is_empty() {
            return Some(icons.white.clone());
        }

        let device = self.devices.iter().find(|d| {
            Some(&d.path) == self.active_device_path.as_ref()
        })?;

        Some(icons.get_icon(&device.status, self.low_threshold, self.critical_threshold).clone())
    }
}

pub struct KsniTray {
    state: Arc<RwLock<TrayState>>,
    cmd_tx: mpsc::Sender<TrayEvent>,
}

impl KsniTray {
    fn new(state: Arc<RwLock<TrayState>>, cmd_tx: mpsc::Sender<TrayEvent>) -> Self {
        Self { state, cmd_tx }
    }
}

impl ksni::Tray for KsniTray {
    fn id(&self) -> String {
        APP_NAME.into()
    }

    fn title(&self) -> String {
        let s = self.state.read().unwrap_or_else(|e| e.into_inner());
        s.active_device_path
            .as_ref()
            .and_then(|path| s.devices.iter().find(|d| d.path == *path))
            .map(|d| format!("{}", d.device_type))
            .unwrap_or_else(|| APP_DISPLAY_NAME.into())
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        let s = self.state.read().unwrap_or_else(|e| e.into_inner());
        if let Some(path) = &s.active_device_path {
            if let Some(device) = s.devices.iter().find(|d| d.path == *path) {
                let dtype_str = format!("{}", device.device_type);
                let status_str = format!("{}", device.status);
                let desc = format!("Battery level: {}", device.status);
                drop(s);
                return ksni::ToolTip {
                    icon_name: "".into(),
                    icon_pixmap: self.get_current_icon(),
                    title: format!("{}: {}", dtype_str, status_str),
                    description: desc,
                };
            }
        }

        ksni::ToolTip {
            icon_name: "".into(),
            icon_pixmap: vec![],
            title: "Razer Battery Report".into(),
            description: "No devices connected".into(),
        }
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.get_current_icon()
    }

    fn menu(&self) -> Vec<ksni::menu::MenuItem<Self>> {
        let s = self.state.read().unwrap_or_else(|e| e.into_inner());
        let devices = s.devices.clone();
        let active_device_path = s.active_device_path.clone();
        let autostart = s.autostart_enabled;
        let notifications = s.notifications_enabled;
        drop(s);

        let mut items = Vec::new();

        // Device selection -- RadioGroup
        if !devices.is_empty() {
            let mut device_paths: Vec<String> = devices.iter().map(|d| d.path.clone()).collect();
            device_paths.sort();
            let active_index = active_device_path
                .as_ref()
                .and_then(|path| device_paths.iter().position(|p| p == path))
                .unwrap_or(0);

            let cmd_tx = self.cmd_tx.clone();
            let device_paths_for_select = device_paths.clone();
            let devices_for_labels = devices.clone();

            items.push(
                ksni::menu::RadioGroup {
                    selected: active_index,
                    select: Box::new(move |_tray: &mut Self, idx: usize| {
                        if idx < device_paths_for_select.len() {
                            let path = device_paths_for_select[idx].clone();
                            let _ = cmd_tx.send(TrayEvent::SelectDevice(path));
                        }
                    }),
                    options: device_paths
                        .iter()
                        .map(|path| {
                            let label = devices_for_labels
                                .iter()
                                .find(|d| d.path == *path)
                                .map(|d| format!("{} [{}]", d.device_type, d.status))
                                .unwrap_or_else(|| path.clone());

                            ksni::menu::RadioItem {
                                label,
                                ..Default::default()
                            }
                        })
                        .collect(),
                }
                .into(),
            );

            items.push(ksni::menu::MenuItem::Separator);
        }

        // Settings -- CheckmarkItem
        let cmd_tx_autostart = self.cmd_tx.clone();
        let state_clone = self.state.clone();
        items.push(
            ksni::menu::CheckmarkItem {
                label: "Autostart".into(),
                checked: autostart,
                activate: Box::new(move |_| {
                    let current = state_clone.read().unwrap().autostart_enabled;
                    let _ = cmd_tx_autostart.send(TrayEvent::ToggleAutostart(!current));
                }),
                ..Default::default()
            }
            .into(),
        );

        let cmd_tx_notif = self.cmd_tx.clone();
        let state_clone2 = self.state.clone();
        items.push(
            ksni::menu::CheckmarkItem {
                label: "Notifications".into(),
                checked: notifications,
                activate: Box::new(move |_| {
                    let current = state_clone2.read().unwrap().notifications_enabled;
                    let _ = cmd_tx_notif.send(TrayEvent::ToggleNotifications(!current));
                }),
                ..Default::default()
            }
            .into(),
        );

        items.push(ksni::menu::MenuItem::Separator);

        // Actions -- StandardItem
        let cmd_tx_restart = self.cmd_tx.clone();
        items.push(
            ksni::menu::StandardItem {
                label: "Restart".into(),
                activate: Box::new(move |_| {
                    let _ = cmd_tx_restart.send(TrayEvent::Restart);
                }),
                ..Default::default()
            }
            .into(),
        );

        let cmd_tx_about = self.cmd_tx.clone();
        items.push(
            ksni::menu::StandardItem {
                label: "About".into(),
                activate: Box::new(move |_| {
                    let _ = cmd_tx_about.send(TrayEvent::ShowAbout);
                }),
                ..Default::default()
            }
            .into(),
        );

        items.push(ksni::menu::MenuItem::Separator);

        let cmd_tx_quit = self.cmd_tx.clone();
        items.push(
            ksni::menu::StandardItem {
                label: "Exit".into(),
                activate: Box::new(move |_| {
                    let _ = cmd_tx_quit.send(TrayEvent::Quit);
                }),
                ..Default::default()
            }
            .into(),
        );

        items
    }
}

impl KsniTray {
    fn get_current_icon(&self) -> Vec<ksni::Icon> {
        let s = self.state.read().unwrap_or_else(|e| e.into_inner());
        s.get_current_icon().into_iter().collect()
    }
}

fn spawn_ksni(tray: KsniTray) -> anyhow::Result<ksni::Handle<KsniTray>> {
    use ksni::TrayMethods;
    use std::thread;

    let (tx, rx) = std::sync::mpsc::channel();

    thread::spawn(move || {
        let rt = match async_io::block_on(tray.spawn()) {
            Ok(handle) => handle,
            Err(e) => {
                error!("Failed to spawn ksni tray: {}", e);
                let _ = tx.send(Err(anyhow::anyhow!("ksni spawn failed: {}", e)));
                return;
            }
        };

        let _ = tx.send(Ok(rt));

        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    });

    rx.recv()
        .map_err(|e| anyhow::anyhow!("Failed to receive ksni handle: {}", e))?
}