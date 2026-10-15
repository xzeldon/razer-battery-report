//! Logging to a rotating file in the per-user log directory.

use std::io::Write;
use std::path::{Path, PathBuf};

use flexi_logger::{
    Cleanup, Criterion, DeferredNow, Duplicate, FileSpec, FlexiLoggerError, Logger, LoggerHandle,
    Naming, Record,
};

use crate::paths;

/// Level used when `RUST_LOG` is not set.
const DEFAULT_LEVEL: &str = "info";
/// File name of the log without the extension.
const LOG_BASENAME: &str = "razer-battery-report";
/// The name `flexi_logger` gives the log being written when timestamps are suppressed.
const CURRENT_LOG_FILE: &str = "razer-battery-report_rCURRENT.log";
/// Size at which the log file is rotated.
const ROTATE_AT_BYTES: u64 = 1024 * 1024;
/// Number of rotated files kept next to the current one.
const KEPT_LOG_FILES: usize = 3;
/// Timestamp format of a log line.
const TIMESTAMP: &str = "%Y-%m-%d %H:%M:%S%.3f";

/// Starts logging to the log directory, with warnings and errors also on stderr. When the
/// directory cannot be used, logs go to stderr only and one line says so. The returned handle
/// must stay alive for as long as the app logs; `None` means logging could not start at all.
pub fn init() -> Option<LoggerHandle> {
    start_in_log_dir().or_else(|| {
        start_stderr()
            .inspect_err(|error| eprintln!("logging: cannot start: {error}"))
            .ok()
    })
}

/// The log file being written, or `None` when the OS reports no home directory.
pub fn current_file() -> Option<PathBuf> {
    paths::log_dir().map(|dir| dir.join(CURRENT_LOG_FILE))
}

/// Starts file logging, or says on stderr why it cannot.
fn start_in_log_dir() -> Option<LoggerHandle> {
    let Some(dir) = paths::log_dir() else {
        eprintln!("logging: no per-user log directory; logging to stderr only");
        return None;
    };
    start_file(&dir)
        .inspect_err(|error| {
            eprintln!(
                "logging: cannot write to {}: {error}; logging to stderr only",
                dir.display()
            );
        })
        .ok()
}

/// A logger with the level from `RUST_LOG`, or the default when it is unset or malformed.
fn logger() -> Result<Logger, FlexiLoggerError> {
    Logger::try_with_env_or_str(DEFAULT_LEVEL).or_else(|error| {
        eprintln!("logging: ignoring RUST_LOG: {error}");
        Logger::try_with_str(DEFAULT_LEVEL)
    })
}

fn start_file(dir: &Path) -> Result<LoggerHandle, FlexiLoggerError> {
    logger()?
        .log_to_file(
            FileSpec::default()
                .directory(dir)
                .basename(LOG_BASENAME)
                .suppress_timestamp(),
        )
        .append()
        .rotate(
            Criterion::Size(ROTATE_AT_BYTES),
            Naming::Numbers,
            Cleanup::KeepLogFiles(KEPT_LOG_FILES),
        )
        .duplicate_to_stderr(Duplicate::Warn)
        .format(line_format)
        .start()
}

fn start_stderr() -> Result<LoggerHandle, FlexiLoggerError> {
    logger()?.log_to_stderr().format(line_format).start()
}

fn line_format(
    out: &mut dyn Write,
    now: &mut DeferredNow,
    record: &Record,
) -> Result<(), std::io::Error> {
    write!(
        out,
        "{} {:<5} {}: {}",
        now.format(TIMESTAMP),
        record.level(),
        record.target(),
        record.args()
    )
}
