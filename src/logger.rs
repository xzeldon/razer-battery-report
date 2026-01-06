use anyhow::{Context, Result};
use directories::{BaseDirs, ProjectDirs, UserDirs};
use flexi_logger::{Cleanup, Criterion, Duplicate, FileSpec, Logger, Naming, WriteMode};
use log::Record;
use std::path::PathBuf;

use crate::config::LogLevel;

/// Defines how the logger should output data.
pub enum LogMode {
    /// Log to stderr only, no files.
    ConsoleOnly,
    /// Log to file with rotation + stderr.
    FileAndConsole,
}

/// Initializes the logging system.
///
/// Logs are written to platform-native locations:
/// - Windows: `%LOCALAPPDATA%\razer-battery-report\logs`
/// - Linux: `$XDG_STATE_HOME/razer-battery-report/logs`
/// - macOS: `~/Library/Logs/razer-battery-report`
pub fn init(cli_level: Option<LogLevel>, config_level: LogLevel, mode: LogMode) -> Result<()> {
    let level = cli_level.unwrap_or(config_level);

    let log_format =
        |w: &mut dyn std::io::Write, now: &mut flexi_logger::DeferredNow, record: &Record| {
            write!(
                w,
                "[{}] [{}] [{}] {}",
                now.format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                record.module_path().unwrap_or("<unnamed>"),
                record.args()
            )
        };

    let logger = Logger::try_with_str(level.to_string())
        .context("Invalid log level")?
        .format(log_format);

    match mode {
        LogMode::ConsoleOnly => {
            logger
                .log_to_stderr()
                .start()
                .context("Failed to start CLI logger")?;
        }
        LogMode::FileAndConsole => {
            let project_dirs = ProjectDirs::from("", "", "razer-battery-report")
                .context("Could not determine home directory")?;

            let log_dir: PathBuf = if cfg!(target_os = "linux") {
                // Linux: $XDG_STATE_HOME (~/.local/state/...)
                project_dirs
                    .state_dir()
                    .unwrap_or_else(|| project_dirs.data_local_dir())
                    .join("logs")
            } else if cfg!(target_os = "macos") {
                // macOS: ~/Library/Logs/<App>
                if let Some(user_dirs) = UserDirs::new() {
                    user_dirs
                        .home_dir()
                        .join("Library/Logs/razer-battery-report")
                } else {
                    // Fallback if we somehow can't find the home dir
                    project_dirs.data_local_dir().join("logs")
                }
            } else {
                // Windows: %LOCALAPPDATA%\razer-battery-report\logs
                BaseDirs::new()
                    .context("Could not get BaseDirs")?
                    .data_local_dir()
                    .join("razer-battery-report")
                    .join("logs")
            };

            if !log_dir.exists() {
                std::fs::create_dir_all(&log_dir).context("Failed to create log directory")?;
            }

            Logger::try_with_str(level.to_string())
                .context(format!("Invalid log level: {}", level))?
                .log_to_file(
                    FileSpec::default()
                        .directory(log_dir)
                        .basename("razer-battery-report"),
                )
                // Keep last 5 files, rotate if > 1MB
                .rotate(
                    Criterion::Size(1_000_000),
                    Naming::Timestamps,
                    Cleanup::KeepLogFiles(5),
                )
                .duplicate_to_stderr(Duplicate::All)
                .write_mode(WriteMode::BufferAndFlush)
                .format(log_format)
                .start()
                .context("Failed to start logger")?;
        }
    }

    Ok(())
}
