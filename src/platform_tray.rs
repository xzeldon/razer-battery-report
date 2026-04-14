//! Platform abstraction for system tray implementation.
//!
//! - Linux: Uses `ksni` crate (StatusNotifierItem spec)
//! - Windows/macOS: Uses `tray-icon` crate

use std::collections::HashMap;
use std::sync::{Arc, Mutex, mpsc};

use log::error;
use razer_battery_report as librazer;

use crate::icon::IconSet;
use crate::worker::WorkerCommand;

/// Platform-specific tray implementation.
pub struct PlatformTray {
    #[cfg(not(target_os = "linux"))]
    inner: tray_icon::TrayIcon,

    #[cfg(target_os = "linux")]
    inner: KsniInner,
}

/// Linux-specific inner state.
#[cfg(target_os = "linux")]
pub struct KsniInner {
    handle: ksni::Handle<KsniTray>,
    cmd_rx: Option<mpsc::Receiver<WorkerCommand>>,
}

impl PlatformTray {
    /// Creates a new platform tray (menu-only, no icon yet).
    /// Icon must be set via `init()` after event loop starts.
    pub fn new(autostart_enabled: bool, notifications_enabled: bool) -> anyhow::Result<Self> {
        #[cfg(not(target_os = "linux"))]
        {
            use tray_icon::{TrayIconBuilder, menu::Menu};

            let tray_menu = Menu::new();
            let tray_icon = TrayIconBuilder::new()
                .with_menu(Box::new(tray_menu))
                .with_tooltip("Razer Battery Report: Initializing...")
                .build()?;

            Ok(Self { inner: tray_icon })
        }

        #[cfg(target_os = "linux")]
        {
            let (cmd_tx, cmd_rx) = mpsc::channel();
            let mut state = TrayState::new();
            state.autostart_enabled = autostart_enabled;
            state.notifications_enabled = notifications_enabled;
            // Set default white icon immediately so ksni doesn't show a placeholder
            state.icons = Some(IconSet::load()?);
            let state = Arc::new(Mutex::new(state));
            let tray = KsniTray::new(state, cmd_tx);
            let handle = spawn_ksni(tray)?;
            Ok(Self {
                inner: KsniInner { handle, cmd_rx: Some(cmd_rx) },
            })
        }
    }

    /// Initializes the tray icon. Must be called after event loop starts.
    pub fn init(&mut self, _icons: &IconSet) -> anyhow::Result<()> {
        #[cfg(not(target_os = "linux"))]
        {
            self.inner.set_icon(Some(icons.white.clone()))?;
        }

        // On Linux, icons are already loaded in new() before spawn_ksni(),
        // so we don't need to call set_icons again. This avoids overwriting
        // the initial state and causing a brief pink square placeholder.
        // The icons will be updated properly when update() is called with device data.

        Ok(())
    }

    /// Updates the tray with current device status.
    pub fn update(
        &mut self,
        devices_status: &HashMap<String, (librazer::DeviceType, librazer::BatteryStatus)>,
        active_device_path: &str,
        icons: &IconSet,
        low_threshold: u8,
        critical_threshold: u8,
    ) {
        #[cfg(not(target_os = "linux"))]
        {
            if let Some((_, status)) = devices_status.get(active_device_path) {
                let new_icon = icons.get_icon(status, low_threshold, critical_threshold);
                let _ = self.inner.set_icon(Some(new_icon.clone()));

                let dtype = devices_status
                    .get(active_device_path)
                    .map(|(d, _)| format!("{}", d))
                    .unwrap_or_default();
                let tooltip = format!("{}: {}", dtype, status);
                let _ = self.inner.set_tooltip(Some(tooltip));
            }
        }

        #[cfg(target_os = "linux")]
        {
            let device_status = devices_status.clone();
            let active_path = active_device_path.to_string();
            let icons_clone = icons.clone();

            let result = async_io::block_on(self.inner.handle.update(move |tray| {
                tray.update_state(
                    device_status,
                    active_path,
                    icons_clone,
                    low_threshold,
                    critical_threshold,
                );
            }));
            if result.is_none() {
                error!("Failed to update ksni tray — service may have shut down");
            }
        }
    }

    /// Sets the tray to "no devices" state.
    pub fn set_no_devices_state(&mut self, _icons: &IconSet) {
        #[cfg(not(target_os = "linux"))]
        {
            let _ = self.inner.set_tooltip(Some("No Razer devices connected".to_string()));
            let _ = self.inner.set_icon(Some(_icons.white.clone()));
        }

        #[cfg(target_os = "linux")]
        {
            let _ = async_io::block_on(self.inner.handle.update(|tray| {
                tray.clear_devices();
            }));
        }
    }

    /// Spawns a background thread that polls for ksni commands and forwards them to the event loop.
    /// Must be called after the event loop proxy is available.
    #[cfg(target_os = "linux")]
    pub fn spawn_command_receiver(
        &mut self,
        proxy: tao::event_loop::EventLoopProxy<crate::AppEvent>,
    ) {
        use crate::AppEvent;
        use std::thread;

        let cmd_rx = self.inner.cmd_rx.take().expect("command receiver already taken");

        thread::spawn(move || {
            loop {
                match cmd_rx.recv() {
                    Ok(cmd) => {
                        let _ = proxy.send_event(AppEvent::KsniCommand(cmd));
                    }
                    Err(mpsc::RecvError) => {
                        error!("Command channel closed, exiting receiver thread");
                        break;
                    }
                }
            }
        });
    }

    /// Updates the autostart setting in the tray state (Linux only).
    #[cfg(target_os = "linux")]
    pub fn set_autostart(&self, enabled: bool) {
        let _ = async_io::block_on(self.inner.handle.update(move |tray| {
            let mut s = tray.state.lock().unwrap();
            s.autostart_enabled = enabled;
        }));
    }

    /// Updates the notifications setting in the tray state (Linux only).
    #[cfg(target_os = "linux")]
    pub fn set_notifications(&self, enabled: bool) {
        let _ = async_io::block_on(self.inner.handle.update(move |tray| {
            let mut s = tray.state.lock().unwrap();
            s.notifications_enabled = enabled;
        }));
    }
}

/// Shared mutable state for the Linux tray.
/// Wrapped in Arc<Mutex<>> so both KsniTray and its callbacks can access it.
#[cfg(target_os = "linux")]
pub struct TrayState {
    pub device_status: HashMap<String, (librazer::DeviceType, librazer::BatteryStatus)>,
    pub active_device_path: Option<String>,
    pub icons: Option<IconSet>,
    pub low_threshold: u8,
    pub critical_threshold: u8,
    pub autostart_enabled: bool,
    pub notifications_enabled: bool,
}

#[cfg(target_os = "linux")]
impl TrayState {
    fn new() -> Self {
        Self {
            device_status: HashMap::new(),
            active_device_path: None,
            icons: None,
            low_threshold: 15,
            critical_threshold: 5,
            autostart_enabled: false,
            notifications_enabled: true,
        }
    }

    fn get_current_icon(&self) -> Option<ksni::Icon> {
        // If we have icons but no active device, return the default white icon
        if let Some(icons) = &self.icons {
            if self.active_device_path.is_none() || self.device_status.is_empty() {
                return png_to_ksni_icon(icons.white_png);
            }
        }

        let icons = self.icons.as_ref()?;
        let path = self.active_device_path.as_ref()?;
        let (_, status) = self.device_status.get(path)?;

        let png_bytes = icons.get_icon_png(status, self.low_threshold, self.critical_threshold);
        png_to_ksni_icon(png_bytes)
    }
}

/// Linux-specific ksni tray implementation.
/// Holds Arc<Mutex<TrayState>> for shared mutable access.
#[cfg(target_os = "linux")]
pub struct KsniTray {
    state: Arc<Mutex<TrayState>>,
    cmd_tx: mpsc::Sender<WorkerCommand>,
}

#[cfg(target_os = "linux")]
impl KsniTray {
    fn new(state: Arc<Mutex<TrayState>>, cmd_tx: mpsc::Sender<WorkerCommand>) -> Self {
        Self { state, cmd_tx }
    }

    fn update_state(
        &self,
        device_status: HashMap<String, (librazer::DeviceType, librazer::BatteryStatus)>,
        active_device_path: String,
        icons: IconSet,
        low_threshold: u8,
        critical_threshold: u8,
    ) {
        let mut s = self.state.lock().unwrap();
        s.device_status = device_status;
        s.active_device_path = Some(active_device_path);
        s.icons = Some(icons);
        s.low_threshold = low_threshold;
        s.critical_threshold = critical_threshold;
    }

    fn clear_devices(&self) {
        let mut s = self.state.lock().unwrap();
        s.device_status.clear();
        s.active_device_path = None;
    }
}

#[cfg(target_os = "linux")]
impl ksni::Tray for KsniTray {
    fn id(&self) -> String {
        "razer-battery-report".into()
    }

    fn title(&self) -> String {
        let s = self.state.lock().unwrap();
        s.active_device_path
            .as_ref()
            .and_then(|path| s.device_status.get(path))
            .map(|(dtype, _)| format!("{}", dtype))
            .unwrap_or_else(|| "Razer Battery Report".into())
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        let s = self.state.lock().unwrap();
        if let Some(path) = &s.active_device_path {
            if let Some((dtype, status)) = s.device_status.get(path) {
                let dtype_str = format!("{}", dtype);
                let status_str = format!("{}", status);
                let desc = format!("Battery level: {}", status);
                drop(s);
                let icon_pixmap = png_to_ksni_icon_from_state(&self.state);
                return ksni::ToolTip {
                    icon_name: "".into(),
                    icon_pixmap,
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
        png_to_ksni_icon_from_state(&self.state)
    }

    fn menu(&self) -> Vec<ksni::menu::MenuItem<Self>> {
        let s = self.state.lock().unwrap();
        let device_status = s.device_status.clone();
        let active_device_path = s.active_device_path.clone();
        let autostart = s.autostart_enabled;
        let notifications = s.notifications_enabled;
        drop(s);

        let mut items = Vec::new();

        // Device selection — RadioGroup
        if !device_status.is_empty() {
            let mut device_paths: Vec<String> = device_status.keys().cloned().collect();
            device_paths.sort(); // Ensure stable order across menu rebuilds
            let active_index = active_device_path
                .as_ref()
                .and_then(|path| device_paths.iter().position(|p| p == path))
                .unwrap_or(0);

            let state_clone = self.state.clone();
            let cmd_tx = self.cmd_tx.clone();
            let device_paths_for_select = device_paths.clone();
            let device_status_for_labels = device_status.clone();

            items.push(
                ksni::menu::RadioGroup {
                    selected: active_index,
                    select: Box::new(move |_tray: &mut Self, idx: usize| {
                        if idx < device_paths_for_select.len() {
                            let path = device_paths_for_select[idx].clone();
                            // Update state immediately so menu rebuilds with correct selection
                            {
                                let mut s = state_clone.lock().unwrap();
                                s.active_device_path = Some(path.clone());
                            }
                            let _ = cmd_tx.send(WorkerCommand::SelectDevice(path));
                        }
                    }),
                    options: device_paths
                        .iter()
                        .map(|path| {
                            let label = device_status_for_labels
                                .get(path)
                                .map(|(dtype, status)| format!("{} [{}]", dtype, status))
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

        // Settings — CheckmarkItem
        let state_clone = self.state.clone();
        let cmd_tx_autostart = self.cmd_tx.clone();
        items.push(
            ksni::menu::CheckmarkItem {
                label: "Autostart".into(),
                checked: autostart,
                activate: Box::new(move |_tray: &mut Self| {
                    let mut s = state_clone.lock().unwrap();
                    s.autostart_enabled = !s.autostart_enabled;
                    let enabled = s.autostart_enabled;
                    drop(s);
                    let _ = cmd_tx_autostart.send(WorkerCommand::ToggleAutostart(enabled));
                }),
                ..Default::default()
            }
            .into(),
        );

        let state_clone = self.state.clone();
        let cmd_tx_notif = self.cmd_tx.clone();
        items.push(
            ksni::menu::CheckmarkItem {
                label: "Notifications".into(),
                checked: notifications,
                activate: Box::new(move |_tray: &mut Self| {
                    let mut s = state_clone.lock().unwrap();
                    s.notifications_enabled = !s.notifications_enabled;
                    let enabled = s.notifications_enabled;
                    drop(s);
                    let _ = cmd_tx_notif.send(WorkerCommand::ToggleNotifications(enabled));
                }),
                ..Default::default()
            }
            .into(),
        );

        items.push(ksni::menu::MenuItem::Separator);

        // Actions — StandardItem
        let cmd_tx_restart = self.cmd_tx.clone();
        items.push(
            ksni::menu::StandardItem {
                label: "Restart".into(),
                activate: Box::new(move |_tray: &mut Self| {
                    let _ = cmd_tx_restart.send(WorkerCommand::Restart);
                }),
                ..Default::default()
            }
            .into(),
        );

        let cmd_tx_about = self.cmd_tx.clone();
        items.push(
            ksni::menu::StandardItem {
                label: "About".into(),
                activate: Box::new(move |_tray: &mut Self| {
                    let _ = cmd_tx_about.send(WorkerCommand::ShowAbout);
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
                activate: Box::new(move |_tray: &mut Self| {
                    let _ = cmd_tx_quit.send(WorkerCommand::Quit);
                }),
                ..Default::default()
            }
            .into(),
        );

        items
    }
}

/// Helper: get icon from shared state without holding the lock across the call.
#[cfg(target_os = "linux")]
fn png_to_ksni_icon_from_state(state: &Arc<Mutex<TrayState>>) -> Vec<ksni::Icon> {
    let s = state.lock().unwrap();
    s.get_current_icon().into_iter().collect()
}

/// Converts PNG bytes to `ksni::Icon` (ARGB32 format).
#[cfg(target_os = "linux")]
fn png_to_ksni_icon(png_bytes: &[u8]) -> Option<ksni::Icon> {
    let img = image::load_from_memory(png_bytes).ok()?.into_rgba8();
    let (width, height) = img.dimensions();
    let mut data = img.into_vec();

    // Convert RGBA to ARGB32 (network byte order)
    for pixel in data.chunks_exact_mut(4) {
        pixel.rotate_right(1); // R,G,B,A -> A,R,G,B
    }

    Some(ksni::Icon {
        width: width as i32,
        height: height as i32,
        data,
    })
}

/// Spawns the ksni tray service on a background thread with its own async runtime.
#[cfg(target_os = "linux")]
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

        // Keepalive — handle is Arc-based, service runs until handle is dropped
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    });

    rx.recv()
        .map_err(|e| anyhow::anyhow!("Failed to receive ksni handle: {}", e))?
}
