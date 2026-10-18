//! The tray app: the icon, its tooltip and menu, and the event loop that ties them to the poller.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context as _;
use razer_hid::{Hid, Percent};
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tray_icon::menu::{
    CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::cli;
use crate::config::{ALLOWED_LOW_THRESHOLDS, Config};
use crate::icon::{self, Shape};
use crate::logging;
use crate::monitor::{self, Level, Reading};
use crate::paths;
use crate::platform::{self, Theme};
use crate::poller;
#[cfg(feature = "self-update")]
use crate::update::{self, Updater};

/// The licence notices of this build, written to the data directory when the user asks for them.
const LICENSES: &str = include_str!(concat!(env!("OUT_DIR"), "/THIRD-PARTY.md"));
const REPOSITORY_URL: &str = env!("CARGO_PKG_REPOSITORY");
/// How often the taskbar theme is read again, because the OS sends no event for it.
const THEME_CHECK_INTERVAL: Duration = Duration::from_mins(1);
const SEARCHING_TEXT: &str = "Searching for Razer mice…";
const NO_MOUSE_TEXT: &str = "No Razer mouse found";
const NO_MOUSE_ITEM: &str = "No mouse found";
const DEVICE_ID_PREFIX: &str = "device:";
const THRESHOLD_ID_PREFIX: &str = "threshold:";
const BATTERY_ALERTS_ID: &str = "battery_alerts";
const SHOW_LEVEL_ID: &str = "show_level";
const AUTOSTART_ID: &str = "autostart";
const OPEN_LOG_ID: &str = "open_log";
const OPEN_REPOSITORY_ID: &str = "open_repository";
const LICENSES_ID: &str = "licenses";
const EXIT_ID: &str = "exit";

const DARK_NORMAL: [u8; 3] = [0xFF, 0xFF, 0xFF];
const DARK_LOW: [u8; 3] = [0xFA, 0xCC, 0x15];
const DARK_CRITICAL: [u8; 3] = [0xF8, 0x71, 0x71];
const LIGHT_NORMAL: [u8; 3] = [0x1C, 0x1C, 0x1C];
const LIGHT_LOW: [u8; 3] = [0xCA, 0x8A, 0x04];
const LIGHT_CRITICAL: [u8; 3] = [0xDC, 0x26, 0x26];

/// What wakes the event loop besides the OS.
#[derive(Debug)]
enum UserEvent {
    Readings(Vec<Reading>),
    Menu(MenuEvent),
    #[cfg(feature = "self-update")]
    Update(update::Event),
}

/// What the event loop does once an event is handled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Flow {
    Continue,
    Exit,
    /// Release the tray lock, start the new executable and exit.
    #[cfg(feature = "self-update")]
    Relaunch,
}

/// A menu entry, recovered from its id.
#[derive(Debug, PartialEq, Eq)]
enum Action {
    Select(String),
    BatteryAlerts,
    Threshold(u8),
    ShowLevel,
    Autostart,
    OpenLog,
    OpenRepository,
    Licenses,
    #[cfg(feature = "self-update")]
    Update(update::Command),
    Exit,
}

impl Action {
    fn id(&self) -> String {
        match self {
            Self::Select(serial) => format!("{DEVICE_ID_PREFIX}{serial}"),
            Self::BatteryAlerts => BATTERY_ALERTS_ID.to_owned(),
            Self::Threshold(percent) => format!("{THRESHOLD_ID_PREFIX}{percent}"),
            Self::ShowLevel => SHOW_LEVEL_ID.to_owned(),
            Self::Autostart => AUTOSTART_ID.to_owned(),
            Self::OpenLog => OPEN_LOG_ID.to_owned(),
            Self::OpenRepository => OPEN_REPOSITORY_ID.to_owned(),
            Self::Licenses => LICENSES_ID.to_owned(),
            #[cfg(feature = "self-update")]
            Self::Update(command) => command.id(),
            Self::Exit => EXIT_ID.to_owned(),
        }
    }

    fn parse(id: &str) -> Option<Self> {
        if let Some(serial) = id.strip_prefix(DEVICE_ID_PREFIX) {
            return Some(Self::Select(serial.to_owned()));
        }
        if let Some(percent) = id.strip_prefix(THRESHOLD_ID_PREFIX) {
            return percent.parse().ok().map(Self::Threshold);
        }
        #[cfg(feature = "self-update")]
        if let Some(command) = update::Command::parse(id) {
            return Some(Self::Update(command));
        }
        match id {
            BATTERY_ALERTS_ID => Some(Self::BatteryAlerts),
            SHOW_LEVEL_ID => Some(Self::ShowLevel),
            AUTOSTART_ID => Some(Self::Autostart),
            OPEN_LOG_ID => Some(Self::OpenLog),
            OPEN_REPOSITORY_ID => Some(Self::OpenRepository),
            LICENSES_ID => Some(Self::Licenses),
            EXIT_ID => Some(Self::Exit),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Look {
    shape: Shape,
    color: [u8; 3],
    badge_color: [u8; 3],
    fill: Option<Percent>,
}

struct View {
    icon: Icon,
    tooltip: String,
    menu: Menu,
}

struct App {
    config: Config,
    readings: Option<Vec<Reading>>,
    theme: Theme,
    icon_size: u32,
    tray: Option<TrayIcon>,
    #[cfg(feature = "self-update")]
    update: Updater,
}

impl App {
    fn shown(&self) -> Option<&Reading> {
        self.readings
            .as_deref()
            .and_then(|readings| shown(readings, self.config.selected.as_deref()))
    }

    fn low_threshold(&self) -> Percent {
        low_threshold(&self.config)
    }

    fn view(&self) -> anyhow::Result<View> {
        let look = look(
            self.shown(),
            self.readings.is_some(),
            self.low_threshold(),
            self.config.show_level,
            self.theme,
        );
        let (rgba, side) = icon::render(
            look.shape,
            look.color,
            look.badge_color,
            look.fill,
            self.icon_size,
        );
        Ok(View {
            icon: Icon::from_rgba(rgba, side, side).context("cannot build the icon")?,
            tooltip: tooltip(self.readings.as_deref(), self.shown()),
            menu: self.menu().context("cannot build the menu")?,
        })
    }

    fn menu(&self) -> tray_icon::menu::Result<Menu> {
        let menu = Menu::new();
        match self.readings.as_deref() {
            None => menu.append(&MenuItem::new(SEARCHING_TEXT, false, None))?,
            Some([]) => menu.append(&MenuItem::new(NO_MOUSE_ITEM, false, None))?,
            Some(readings) => {
                let shown_key = self.shown().map(|reading| &reading.key);
                for reading in readings {
                    let several = readings.len() > 1;
                    let text = format!("{} — {}", reading.model, status(reading));
                    let id = reading
                        .serial
                        .clone()
                        .map_or_else(String::new, |serial| Action::Select(serial).id());
                    let enabled = several && reading.serial.is_some();
                    let checked = shown_key == Some(&reading.key);
                    menu.append(&CheckMenuItem::with_id(id, text, enabled, checked, None))?;
                }
            }
        }
        menu.append(&PredefinedMenuItem::separator())?;
        let config = &self.config;
        menu.append(&switch(
            &Action::BatteryAlerts,
            "Battery alerts",
            config.battery_alerts,
        ))?;
        let thresholds = ALLOWED_LOW_THRESHOLDS.map(|percent| {
            let checked = percent == config.low_threshold;
            switch(&Action::Threshold(percent), &format!("{percent}%"), checked)
        });
        let items: Vec<&dyn IsMenuItem> = thresholds
            .iter()
            .map(|item| item as &dyn IsMenuItem)
            .collect();
        menu.append(&Submenu::with_items("Low battery", true, &items)?)?;
        menu.append(&switch(
            &Action::ShowLevel,
            "Show level on icon",
            config.show_level,
        ))?;
        menu.append(&switch(
            &Action::Autostart,
            "Start at login",
            config.autostart,
        ))?;
        #[cfg(feature = "self-update")]
        menu.append(&self.update.submenu(config.updates)?)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&MenuItem::with_id(
            Action::OpenLog.id(),
            "Open log",
            true,
            None,
        ))?;
        menu.append(&about()?)?;
        #[cfg(feature = "self-update")]
        if let Some(item) = self.update.install_item() {
            menu.append(&item)?;
        }
        menu.append(&MenuItem::with_id(Action::Exit.id(), "Exit", true, None))?;
        Ok(menu)
    }

    /// Call it once, when the event loop starts.
    fn create_tray(&mut self) -> anyhow::Result<()> {
        let view = self.view()?;
        let tray = TrayIconBuilder::new()
            .with_icon(view.icon)
            .with_tooltip(view.tooltip)
            .with_menu(Box::new(view.menu))
            .build()
            .context("cannot create the tray icon")?;
        self.tray = Some(tray);
        Ok(())
    }

    fn refresh(&mut self) {
        let view = match self.view() {
            Ok(view) => view,
            Err(error) => {
                log::warn!("tray: cannot redraw: {error:#}");
                return;
            }
        };
        let Some(tray) = &self.tray else {
            return;
        };
        if let Err(error) = tray.set_icon(Some(view.icon)) {
            log::warn!("tray: cannot set the icon: {error}");
        }
        if let Err(error) = tray.set_tooltip(Some(view.tooltip)) {
            log::warn!("tray: cannot set the tooltip: {error}");
        }
        tray.set_menu(Some(Box::new(view.menu)));
    }

    fn on_readings(&mut self, current: Vec<Reading>) {
        let events = monitor::events(self.readings.as_deref(), &current, self.low_threshold());
        for event in &events {
            log::info!("monitor: {event}");
            if self.config.battery_alerts {
                let shown = platform::notification().summary(&event.to_string()).show();
                if let Err(error) = shown {
                    log::warn!("tray: cannot show a notification: {error}");
                }
            }
            if matches!(event, monitor::Event::Connected { .. }) {
                log_diagnostics(&self.config);
            }
        }
        self.readings = Some(current);
        self.refresh();
    }

    /// Applies a click on a menu entry or a toast button.
    fn on_action(&mut self, action: Action) -> Flow {
        match action {
            Action::Select(serial) => self.change(|config| config.selected = Some(serial)),
            Action::BatteryAlerts => self.change(|config| config.battery_alerts ^= true),
            Action::Threshold(percent) => self.change(|config| config.low_threshold = percent),
            Action::ShowLevel => self.change(|config| config.show_level ^= true),
            Action::Autostart => {
                let enabled = !self.config.autostart;
                match platform::set_autostart(enabled) {
                    Ok(()) => self.change(|config| config.autostart = enabled),
                    Err(error) => {
                        log::warn!("tray: cannot change autostart: {error:#}");
                        // The menu has already toggled the checkbox; put it back.
                        self.refresh();
                    }
                }
            }
            Action::OpenLog => open_log(),
            Action::OpenRepository => {
                if let Err(error) = platform::open_url(REPOSITORY_URL) {
                    log::warn!("tray: {error:#}");
                }
            }
            Action::Licenses => {
                if let Err(error) = write_licenses().and_then(|path| platform::open(&path)) {
                    log::warn!("tray: {error:#}");
                }
            }
            #[cfg(feature = "self-update")]
            Action::Update(command) => {
                if let update::Command::SetMode(mode) = command {
                    self.change(|config| config.updates = mode);
                }
                self.update.command(command, self.config.updates);
                self.refresh();
            }
            Action::Exit => return Flow::Exit,
        }
        Flow::Continue
    }

    /// Applies what an updater thread or a toast reports.
    #[cfg(feature = "self-update")]
    fn on_update(&mut self, event: update::Event) -> Flow {
        if let update::Event::Toast(id) = &event {
            return Action::parse(id).map_or(Flow::Continue, |action| self.on_action(action));
        }
        let handoff = self.update.event(event, self.config.updates);
        self.refresh();
        match handoff {
            None => Flow::Continue,
            Some(update::Handoff::Exit) => Flow::Exit,
            Some(update::Handoff::Relaunch) => Flow::Relaunch,
        }
    }

    fn change(&mut self, apply: impl FnOnce(&mut Config)) {
        apply(&mut self.config);
        if let Err(error) = self.config.save() {
            log::warn!("tray: cannot save the config: {error:#}");
        }
        self.refresh();
    }
}

/// Runs the tray until the user exits. Returns early, with `Ok`, when another tray is running.
pub fn run(config: Config) -> anyhow::Result<()> {
    let Some(lock) = acquire_lock()? else {
        log::info!("tray: already running");
        return Ok(());
    };
    platform::announce_running();
    let hid = Hid::new().context("cannot initialise hid")?;
    if let Err(error) = platform::set_autostart(config.autostart) {
        log::warn!("tray: cannot apply autostart: {error:#}");
    }

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let menu_proxy = proxy.clone();
    #[cfg(feature = "self-update")]
    let update_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_proxy.send_event(UserEvent::Menu(event));
    }));
    poller::spawn(hid, move |readings| {
        proxy.send_event(UserEvent::Readings(readings)).is_ok()
    });

    let mut app = App {
        theme: platform::taskbar_theme(),
        icon_size: 0,
        readings: None,
        tray: None,
        #[cfg(feature = "self-update")]
        update: Updater::new(move |event| {
            let _ = update_proxy.send_event(UserEvent::Update(event));
        }),
        config,
    };
    let mut next_theme_check = Instant::now() + THEME_CHECK_INTERVAL;
    #[cfg(feature = "self-update")]
    let mut next_update_tick = Instant::now() + update::FIRST_TICK;
    let mut lock = Some(lock);
    event_loop.run(move |event, _, control_flow| {
        let _ = &mut lock;
        let mut flow = Flow::Continue;
        match event {
            Event::NewEvents(StartCause::Init) => {
                platform::register_notifications(icon::APP_ICON_PNG);
                #[cfg(feature = "self-update")]
                app.update.start();
                app.icon_size = platform::tray_icon_size();
                if let Err(error) = app.create_tray() {
                    log::error!("tray: {error:#}");
                    *control_flow = ControlFlow::Exit;
                    return;
                }
                log::info!("tray: started");
                log_diagnostics(&app.config);
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                let now = Instant::now();
                if now >= next_theme_check {
                    next_theme_check = now + THEME_CHECK_INTERVAL;
                    let theme = platform::taskbar_theme();
                    if theme != app.theme {
                        app.theme = theme;
                        app.refresh();
                    }
                }
                #[cfg(feature = "self-update")]
                if now >= next_update_tick {
                    next_update_tick = now + update::TICK;
                    app.update.tick(app.config.updates);
                    app.refresh();
                }
            }
            Event::UserEvent(UserEvent::Readings(readings)) => app.on_readings(readings),
            Event::UserEvent(UserEvent::Menu(event)) => {
                if let Some(action) = Action::parse(&event.id.0) {
                    flow = app.on_action(action);
                }
            }
            #[cfg(feature = "self-update")]
            Event::UserEvent(UserEvent::Update(event)) => flow = app.on_update(event),
            _ => {}
        }
        match flow {
            Flow::Continue => {}
            Flow::Exit => {
                shut_down(&mut app, control_flow);
                return;
            }
            #[cfg(feature = "self-update")]
            Flow::Relaunch => {
                if relaunch(&mut lock) {
                    shut_down(&mut app, control_flow);
                    return;
                }
            }
        }
        let wake = next_theme_check;
        #[cfg(feature = "self-update")]
        let wake = wake.min(next_update_tick);
        *control_flow = ControlFlow::WaitUntil(wake);
    })
}

/// Starts the updated executable, which needs the tray lock, so the lock is released first.
/// When that fails the lock is taken back and this tray keeps running. Returns whether the
/// new executable started.
#[cfg(feature = "self-update")]
fn relaunch(lock: &mut Option<File>) -> bool {
    drop(lock.take());
    match platform::relaunch() {
        Ok(()) => true,
        Err(error) => {
            log::warn!("tray: cannot start the updated app: {error}");
            *lock = acquire_lock().ok().flatten();
            false
        }
    }
}

fn shut_down(app: &mut App, control_flow: &mut ControlFlow) {
    app.tray = None;
    log::info!("tray: exiting");
    log::logger().flush();
    *control_flow = ControlFlow::Exit;
}

/// Whether another process holds the tray lock. Creates neither the lock file nor its
/// directory, so it is safe to call before removing them.
pub fn running() -> anyhow::Result<bool> {
    let path = lock_path()?;
    let file = match OpenOptions::new().write(true).open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error).with_context(|| format!("cannot open {}", path.display())),
    };
    Ok(!try_lock(&file, &path)?)
}

/// `None` when another tray holds the lock; the lock lasts as long as the file is open.
fn acquire_lock() -> anyhow::Result<Option<File>> {
    let path = lock_path()?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).context("cannot create data directory")?;
    }
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .with_context(|| format!("cannot open {}", path.display()))?;
    Ok(try_lock(&file, &path)?.then_some(file))
}

fn lock_path() -> anyhow::Result<PathBuf> {
    paths::lock_file().context("no per-user data directory")
}

/// Takes the lock on `file`, which was opened from `path`; `false` when someone else holds it.
fn try_lock(file: &File, path: &Path) -> anyhow::Result<bool> {
    match file.try_lock() {
        Ok(()) => Ok(true),
        Err(TryLockError::WouldBlock) => Ok(false),
        Err(TryLockError::Error(error)) => {
            Err(error).with_context(|| format!("cannot lock {}", path.display()))
        }
    }
}

/// Runs on its own thread because the report reads the hardware.
fn log_diagnostics(config: &Config) {
    let config = config.clone();
    thread::spawn(move || log::info!("tray: diagnostics:\n{}", cli::diagnose(&config)));
}

pub fn open_log() {
    let Some(path) = logging::current_file() else {
        log::warn!("tray: no per-user log directory");
        return;
    };
    if let Err(error) = platform::open(&path) {
        log::warn!("tray: {error:#}");
    }
}

/// Writes the embedded notices over any older copy, so they always match the running build.
fn write_licenses() -> anyhow::Result<PathBuf> {
    let path = paths::licenses_file().context("no per-user data directory")?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).context("cannot create data directory")?;
    }
    fs::write(&path, LICENSES).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(path)
}

/// The About submenu: name, version and author as plain text, then the links.
fn about() -> tray_icon::menu::Result<Submenu> {
    let version = format!(
        "{} {}",
        platform::APP_DISPLAY_NAME,
        env!("CARGO_PKG_VERSION")
    );
    let author = env!("CARGO_PKG_AUTHORS");
    // Cargo authors read "name <email>"; the menu shows the name only.
    let author = author.split_once(" <").map_or(author, |(name, _)| name);
    Submenu::with_items(
        "About",
        true,
        &[
            &MenuItem::new(version, false, None),
            &MenuItem::new(format!("by {author}"), false, None),
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id(Action::OpenRepository.id(), "Open on GitHub", true, None),
            &MenuItem::with_id(Action::Licenses.id(), "Licenses", true, None),
        ],
    )
}

fn switch(action: &Action, text: &str, checked: bool) -> CheckMenuItem {
    CheckMenuItem::with_id(action.id(), text, true, checked, None)
}

/// Cannot fail: the config only ever holds supported thresholds.
fn low_threshold(config: &Config) -> Percent {
    Percent::try_from(config.low_threshold).expect("the config holds a valid threshold")
}

/// The mouse to show: the one with the `selected` serial, else the first.
fn shown<'a>(readings: &'a [Reading], selected: Option<&str>) -> Option<&'a Reading> {
    selected
        .and_then(|serial| {
            readings
                .iter()
                .find(|reading| reading.serial.as_deref() == Some(serial))
        })
        .or(readings.first())
}

fn status(reading: &Reading) -> String {
    match (reading.battery, reading.asleep) {
        (Some(battery), false) => cli::battery_text(battery),
        (Some(battery), true) => format!("{} (asleep)", cli::battery_text(battery)),
        (None, true) => "asleep".to_owned(),
        (None, false) => "battery unknown".to_owned(),
    }
}

/// The tooltip for the shown mouse; `readings` is `None` before the first scan.
fn tooltip(readings: Option<&[Reading]>, shown: Option<&Reading>) -> String {
    match (readings, shown) {
        (None, _) => SEARCHING_TEXT.to_owned(),
        (Some(_), None) => NO_MOUSE_TEXT.to_owned(),
        (Some(_), Some(reading)) => format!("{}: {}", reading.model, status(reading)),
    }
}

/// The icon colour for a taskbar theme and a battery severity.
fn palette(theme: Theme, level: Level) -> [u8; 3] {
    match (theme, level) {
        (Theme::Dark, Level::Normal) => DARK_NORMAL,
        (Theme::Dark, Level::Low) => DARK_LOW,
        (Theme::Dark, Level::Critical) => DARK_CRITICAL,
        (Theme::Light, Level::Normal) => LIGHT_NORMAL,
        (Theme::Light, Level::Low) => LIGHT_LOW,
        (Theme::Light, Level::Critical) => LIGHT_CRITICAL,
    }
}

/// How the icon looks for the shown mouse (`None` when there is none). `scanned` tells whether
/// the first scan has finished; before that the icon is a plain mouse.
fn look(
    shown: Option<&Reading>,
    scanned: bool,
    low_threshold: Percent,
    show_level: bool,
    theme: Theme,
) -> Look {
    let normal = palette(theme, Level::Normal);
    let plain = |shape| Look {
        shape,
        color: normal,
        badge_color: normal,
        fill: None,
    };
    if !scanned {
        return plain(Shape::Mouse);
    }
    let Some(reading) = shown else {
        return plain(Shape::Off);
    };
    let shape = match (reading.battery, reading.asleep) {
        (Some(battery), _) if battery.charging == Some(true) => Shape::Charging,
        (_, true) => Shape::Asleep,
        _ => Shape::Mouse,
    };
    let level = reading.battery.map_or(Level::Normal, |battery| {
        monitor::level(battery, low_threshold)
    });
    let color = palette(theme, level);
    Look {
        shape,
        color,
        // The charging bolt borrows the low battery yellow.
        badge_color: match shape {
            Shape::Charging => palette(theme, Level::Low),
            _ => color,
        },
        fill: reading
            .battery
            .filter(|_| show_level)
            .map(|battery| battery.level),
    }
}

#[cfg(test)]
mod tests {
    use razer_hid::Battery;

    use super::*;
    use crate::monitor::DeviceKey;

    fn percent(value: u8) -> Percent {
        Percent::try_from(value).unwrap()
    }

    fn reading(serial: Option<&str>, level: Option<(u8, Option<bool>)>, asleep: bool) -> Reading {
        Reading {
            key: DeviceKey::for_test(serial.unwrap_or("path")),
            model: "Mouse",
            serial: serial.map(str::to_owned),
            battery: level.map(|(level, charging)| Battery {
                level: percent(level),
                charging,
            }),
            asleep,
        }
    }

    #[test]
    fn icon_look() {
        let look = |shown: Option<&Reading>, scanned, show_level, theme| {
            look(shown, scanned, percent(15), show_level, theme)
        };
        let normal = reading(Some("a"), Some((80, Some(false))), false);
        let charging = reading(Some("a"), Some((50, Some(true))), false);
        let asleep = reading(Some("a"), Some((40, None)), true);
        let low = reading(Some("a"), Some((12, Some(false))), false);
        let (dark, light) = (Theme::Dark, Theme::Light);

        let cases = [
            (
                look(None, false, true, dark),
                Shape::Mouse,
                DARK_NORMAL,
                None,
            ),
            (look(None, true, true, dark), Shape::Off, DARK_NORMAL, None),
            (
                look(Some(&normal), true, true, dark),
                Shape::Mouse,
                DARK_NORMAL,
                Some(80),
            ),
            (
                look(Some(&charging), true, true, dark),
                Shape::Charging,
                DARK_NORMAL,
                Some(50),
            ),
            (
                look(Some(&asleep), true, true, dark),
                Shape::Asleep,
                DARK_NORMAL,
                Some(40),
            ),
            (
                look(Some(&low), true, true, light),
                Shape::Mouse,
                LIGHT_LOW,
                Some(12),
            ),
            (
                look(Some(&low), true, false, dark),
                Shape::Mouse,
                DARK_LOW,
                None,
            ),
        ];
        for (got, shape, color, fill) in cases {
            assert_eq!(
                (got.shape, got.color, got.fill),
                (shape, color, fill.map(percent))
            );
        }
        assert_eq!(
            look(Some(&charging), true, true, dark).badge_color,
            DARK_LOW
        );
        assert_eq!(
            look(Some(&asleep), true, true, light).badge_color,
            LIGHT_NORMAL
        );
    }

    #[test]
    fn shown_mouse_is_the_selected_one_else_the_first() {
        let readings = [
            reading(Some("a"), None, false),
            reading(Some("b"), None, false),
        ];
        let serial = |selected| shown(&readings, selected).and_then(|r| r.serial.as_deref());
        assert_eq!(serial(Some("b")), Some("b"));
        assert_eq!(serial(Some("gone")), Some("a"));
        assert_eq!(serial(None), Some("a"));
        assert!(shown(&[], Some("a")).is_none());
    }
}
