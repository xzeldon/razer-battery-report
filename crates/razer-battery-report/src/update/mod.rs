//! Self-update: finds a newer release on GitHub, downloads it and either runs the installer or
//! replaces the executable. The tray owns an [`Updater`]; its network work runs on threads
//! that report back as [`Event`]s.

mod error;
mod fetch;
mod install;
mod menu;
mod reconcile;
mod release;
mod schedule;
mod state;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use semver::Version;

use crate::config::UpdateMode;
use crate::platform::{self, InstallKind};
use crate::{paths, tray};
use error::UpdateError;
use reconcile::Outcome;
use release::Available;
use state::State;

pub use schedule::{FIRST_TICK, TICK};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const TARGET: &str = env!("RBR_TARGET");
const REPOSITORY_URL: &str = env!("CARGO_PKG_REPOSITORY");
const COMMAND_ID_PREFIX: &str = "update:";

/// Something the user can ask for, from the menu or from a toast button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Install,
    /// The release page of the available release, or of the running version.
    WhatsNew,
    CheckNow,
    Retry,
    DownloadManually,
    OpenLog,
    OpenInstallerLog,
    SetMode(UpdateMode),
}

impl Command {
    pub fn id(self) -> String {
        let name = match self {
            Self::Install => "install",
            Self::WhatsNew => "whats_new",
            Self::CheckNow => "check_now",
            Self::Retry => "retry",
            Self::DownloadManually => "download_manually",
            Self::OpenLog => "open_log",
            Self::OpenInstallerLog => "open_installer_log",
            Self::SetMode(UpdateMode::Auto) => "mode_auto",
            Self::SetMode(UpdateMode::Notify) => "mode_notify",
            Self::SetMode(UpdateMode::Off) => "mode_off",
        };
        format!("{COMMAND_ID_PREFIX}{name}")
    }

    pub fn parse(id: &str) -> Option<Self> {
        let command = match id.strip_prefix(COMMAND_ID_PREFIX)? {
            "install" => Self::Install,
            "whats_new" => Self::WhatsNew,
            "check_now" => Self::CheckNow,
            "retry" => Self::Retry,
            "download_manually" => Self::DownloadManually,
            "open_log" => Self::OpenLog,
            "open_installer_log" => Self::OpenInstallerLog,
            "mode_auto" => Self::SetMode(UpdateMode::Auto),
            "mode_notify" => Self::SetMode(UpdateMode::Notify),
            "mode_off" => Self::SetMode(UpdateMode::Off),
            _ => return None,
        };
        Some(command)
    }

    /// What a click on a toast that starts with this command does, after "Click to".
    fn click_hint(self) -> &'static str {
        match self {
            Self::Install => "install it",
            Self::WhatsNew => "see what changed",
            Self::CheckNow => "check again",
            Self::Retry => "try again",
            Self::DownloadManually => "open the release page",
            Self::OpenLog => "open the log",
            Self::OpenInstallerLog => "open the installer log",
            Self::SetMode(_) => "change the update mode",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Install => "Install",
            Self::WhatsNew => "What's new",
            Self::CheckNow => "Check now",
            Self::Retry => "Try again",
            Self::DownloadManually => "Download manually",
            Self::OpenLog => "Open log",
            Self::OpenInstallerLog => "Open installer log",
            Self::SetMode(UpdateMode::Auto) => "Install automatically",
            Self::SetMode(UpdateMode::Notify) => "Notify only",
            Self::SetMode(UpdateMode::Off) => "Off",
        }
    }
}

/// What the updater's threads and toasts send to the tray.
#[derive(Debug)]
pub enum Event {
    Checked {
        manual: bool,
        result: Result<Option<Available>, UpdateError>,
    },
    Downloaded {
        manual: bool,
        result: Result<PathBuf, UpdateError>,
    },
    Toast(String),
}

/// What the tray has to do to finish an installation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handoff {
    /// Exit; the installer relaunches the app.
    Exit,
    /// Release the tray lock, start the new executable and exit.
    Relaunch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Busy {
    Checking,
    Downloading,
}

/// Checks for, downloads and installs new releases. Lives on the tray's thread.
pub struct Updater {
    state: State,
    busy: Option<Busy>,
    kind: InstallKind,
    emit: Arc<dyn Fn(Event) + Send + Sync>,
}

impl Updater {
    pub fn new(emit: impl Fn(Event) + Send + Sync + 'static) -> Self {
        Self {
            state: State::load(),
            busy: None,
            kind: platform::install_kind(),
            emit: Arc::new(emit),
        }
    }

    /// Settles what an earlier update left behind and tells the user about its result. Call it
    /// once, after the notifications are registered.
    pub fn start(&mut self) {
        let current = running_version();
        let outcome = reconcile::decide(
            self.state.pending.as_deref(),
            self.state.last_version.as_deref(),
            &current,
        );
        self.state.pending = None;
        self.state.last_version = Some(current.to_string());
        if let Some(dir) = paths::update_dir() {
            install::remove_downloads(&dir);
        }
        let newer = |available: &Available| {
            Version::parse(&available.version).is_ok_and(|version| version > current)
        };
        if !self.state.available.as_ref().is_some_and(newer) {
            self.state.available = None;
        }
        match outcome {
            Outcome::Nothing => {}
            Outcome::Updated(version) => {
                self.state.last_error = None;
                self.toast(&format!("Updated to {version}"), "", &[Command::WhatsNew]);
            }
            Outcome::Failed => {
                let error = match self.kind {
                    InstallKind::Installer => {
                        let log = paths::update_dir().and_then(|dir| {
                            std::fs::read_to_string(dir.join(install::INSTALL_LOG)).ok()
                        });
                        UpdateError::installer_failed(&reconcile::failure_text(log.as_deref()))
                    }
                    InstallKind::Portable => {
                        UpdateError::not_replaced(reconcile::NOT_STARTED_REASON)
                    }
                };
                log::warn!("updater: the update did not happen: {error}");
                self.toast_error(&error);
                self.state.last_error = Some(error);
            }
        }
        self.save();
    }

    /// Starts a check when `mode` allows one and the last one is old enough. Call it on every
    /// schedule tick and when the mode changes.
    pub fn tick(&mut self, mode: UpdateMode) {
        if mode == UpdateMode::Off || self.busy.is_some() {
            return;
        }
        if schedule::check_due(self.state.checked_at, now()) {
            self.check(false);
        }
    }

    /// Carries out a command. `mode` is the current one, changed already by `SetMode`.
    pub fn command(&mut self, command: Command, mode: UpdateMode) {
        match command {
            Command::Install => self.install(),
            Command::CheckNow => self.check_now(),
            Command::Retry => {
                let install = self
                    .state
                    .last_error
                    .as_ref()
                    .is_some_and(UpdateError::is_install);
                match install {
                    true => self.install(),
                    false => self.check_now(),
                }
            }
            Command::WhatsNew => open_url(&self.release_page()),
            Command::DownloadManually => open_url(&self.state.available.as_ref().map_or_else(
                || format!("{REPOSITORY_URL}/releases/latest"),
                |available| available.html_url.clone(),
            )),
            Command::OpenLog => tray::open_log(),
            Command::OpenInstallerLog => {
                if let Some(path) = paths::update_dir().map(|dir| dir.join(install::INSTALL_LOG))
                    && let Err(error) = platform::open(&path)
                {
                    log::warn!("updater: {error:#}");
                }
            }
            Command::SetMode(_) => self.tick(mode),
        }
    }

    /// Takes in the result of a thread. Returns what the tray has to do when an installation
    /// started.
    pub fn event(&mut self, event: Event, mode: UpdateMode) -> Option<Handoff> {
        match event {
            Event::Checked { manual, result } => self.checked(manual, result, mode),
            Event::Downloaded { manual, result } => return self.downloaded(manual, result),
            Event::Toast(_) => {}
        }
        None
    }

    fn check_now(&mut self) {
        if self.busy.is_none() {
            self.check(true);
        }
    }

    fn check(&mut self, manual: bool) {
        self.busy = Some(Busy::Checking);
        let emit = Arc::clone(&self.emit);
        let suffix = platform::asset_suffix(self.kind);
        thread::spawn(move || {
            let result = fetch::latest_release()
                .and_then(|json| release::evaluate(&json, &running_version(), TARGET, suffix));
            emit(Event::Checked { manual, result });
        });
    }

    fn checked(
        &mut self,
        manual: bool,
        result: Result<Option<Available>, UpdateError>,
        mode: UpdateMode,
    ) {
        self.busy = None;
        let now = now();
        self.state.checked_at = Some(now);
        match result {
            Ok(found) => {
                self.state.last_error = None;
                self.state.available.clone_from(&found);
                match (found, manual, mode) {
                    (None, true, _) => {
                        self.toast("You are up to date", &format!("You have {VERSION}."), &[]);
                    }
                    (Some(_), manual, UpdateMode::Auto) => self.download(manual),
                    (Some(found), true, _) | (Some(found), false, UpdateMode::Notify) => {
                        self.toast_available(&found);
                    }
                    (None, false, _) | (Some(_), false, UpdateMode::Off) => {}
                }
            }
            Err(error) => {
                log::warn!("updater: check failed: {error}");
                if manual {
                    self.toast_error(&error);
                }
                self.state.last_error = Some(error);
            }
        }
        self.save();
    }

    /// Downloads the available release; the install follows when the download arrives.
    fn install(&mut self) {
        if self.busy.is_none() && self.state.available.is_some() {
            self.download(true);
        }
    }

    fn download(&mut self, manual: bool) {
        let (Some(available), Some(dir)) = (self.state.available.clone(), paths::update_dir())
        else {
            return;
        };
        self.busy = Some(Busy::Downloading);
        let emit = Arc::clone(&self.emit);
        thread::spawn(move || {
            let result = fetch::download(&available.asset, &dir);
            emit(Event::Downloaded { manual, result });
        });
    }

    fn downloaded(
        &mut self,
        manual: bool,
        result: Result<PathBuf, UpdateError>,
    ) -> Option<Handoff> {
        self.busy = None;
        let handoff = match result.and_then(|file| self.hand_over(&file)) {
            Ok(handoff) => {
                self.state.last_error = None;
                Some(handoff)
            }
            Err(error) => {
                log::warn!("updater: install failed: {error}");
                if manual {
                    self.toast_error(&error);
                }
                self.state.last_error = Some(error);
                None
            }
        };
        self.save();
        handoff
    }

    /// Starts the installation of the downloaded `file`. The version is recorded first, so the
    /// next start can tell whether it worked.
    fn hand_over(&mut self, file: &Path) -> Result<Handoff, UpdateError> {
        let version = self
            .state
            .available
            .as_ref()
            .map(|available| available.version.clone());
        let dir = file.parent().map(Path::to_path_buf).unwrap_or_default();
        let handoff = match self.kind {
            InstallKind::Installer => {
                self.state.pending = version;
                self.save();
                install::launch_installer(file, &dir)
            }
            InstallKind::Portable => install::replace_executable(file).inspect(|_| {
                self.state.pending = version;
                self.save();
            }),
        };
        if handoff.is_err() {
            self.state.pending = None;
        }
        handoff
    }

    fn release_page(&self) -> String {
        match &self.state.available {
            Some(available) => available.html_url.clone(),
            None => format!("{REPOSITORY_URL}/releases/tag/v{VERSION}"),
        }
    }

    fn toast_available(&self, available: &Available) {
        let hint = match platform::toast_actions_supported() {
            true => "",
            false => " Install it from the tray menu.",
        };
        self.toast(
            &format!("Version {} is available", available.version),
            &format!("You have {VERSION}.{hint}"),
            &[Command::Install, Command::WhatsNew],
        );
    }

    fn toast_error(&self, error: &UpdateError) {
        let buttons = platform::toast_actions_supported();
        self.toast(
            error.title(),
            &error.body(VERSION, buttons),
            error.actions(),
        );
    }

    /// Shows a toast whose click runs the first of `commands`; the body says what that click does,
    /// because the notification centre hides the buttons.
    fn toast(&self, title: &str, body: &str, commands: &[Command]) {
        let body = match commands.first() {
            Some(first) if platform::toast_actions_supported() => {
                format!("{body} Click to {}.", first.click_hint())
                    .trim_start()
                    .to_owned()
            }
            _ => body.to_owned(),
        };
        let ids: Vec<String> = commands.iter().map(|command| command.id()).collect();
        let actions: Vec<(&str, &str)> = ids
            .iter()
            .zip(commands)
            .map(|(id, command)| (id.as_str(), command.label()))
            .collect();
        let emit = Arc::clone(&self.emit);
        let shown = platform::show_toast(title, &body, &actions, move |id| emit(Event::Toast(id)));
        if let Err(error) = shown {
            log::warn!("updater: cannot show a toast: {error:#}");
        }
    }

    fn save(&self) {
        if let Err(error) = self.state.save() {
            log::warn!("updater: cannot save the state: {error:#}");
        }
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

fn running_version() -> Version {
    Version::parse(VERSION).expect("the package version is valid semver")
}

fn open_url(url: &str) {
    if let Err(error) = platform::open_url(url) {
        log::warn!("updater: {error:#}");
    }
}
