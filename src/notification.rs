use log::warn;
use std::thread::{self, JoinHandle};

use crate::config::APP_DISPLAY_NAME;

#[cfg(not(target_os = "macos"))]
use notify_rust::Notification;

/// Handles sending desktop notifications.
///
/// This module abstracts over the `notify-rust` crate, providing a unified interface
/// for sending notifications with the correct application name and error handling.
pub struct Notifier;

impl Notifier {
    /// Sends a standard notification with the application name.
    pub fn send(summary: &str, body: &str) {
        let _ = Self::spawn(summary, body);
    }

    pub fn send_blocking(summary: &str, body: &str) {
        if let Some(handle) = Self::spawn(summary, body) {
            let _ = handle.join();
        }
    }

    fn spawn(summary: &str, body: &str) -> Option<JoinHandle<()>> {
        let summary = summary.to_owned();
        let body = body.to_owned();
        Some(dispatch(summary, body, |summary, body| {
            let res = show(summary, body);

            if let Err(e) = res {
                warn!("Failed to show notification: {}", e);
            }
        }))
    }
}

fn show(summary: String, body: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let script = build_macos_notification_script(&summary, &body);
        let status = std::process::Command::new("/usr/bin/osascript")
            .arg("-e")
            .arg(script)
            .status()
            .map_err(|e| e.to_string())?;

        if status.success() {
            Ok(())
        } else {
            Err(format!("osascript exited with {}", status))
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        Notification::new()
            .appname(APP_DISPLAY_NAME)
            .summary(&summary)
            .body(&body)
            .show()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

#[cfg(target_os = "macos")]
fn build_macos_notification_script(summary: &str, body: &str) -> String {
    format!(
        "display notification \"{}\" with title \"{}\"",
        escape_applescript(body),
        escape_applescript(summary)
    )
}

#[cfg(target_os = "macos")]
fn escape_applescript(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}

fn dispatch(
    summary: String,
    body: String,
    deliver: impl FnOnce(String, String) + Send + 'static,
) -> JoinHandle<()> {
    thread::spawn(move || deliver(summary, body))
}

#[cfg(test)]
mod tests {
    use super::dispatch;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn dispatch_returns_before_delivery_finishes() {
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();

        let handle = dispatch("summary".to_owned(), "body".to_owned(), move |summary, body| {
            assert_eq!(summary, "summary");
            assert_eq!(body, "body");
            started_tx.send(()).expect("notify test start");
            release_rx.recv().expect("notify test release");
        });

        started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("background delivery did not start");
        release_tx.send(()).expect("notify test release signal");
        handle.join().expect("notify thread join");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_script_escapes_quotes_and_newlines() {
        let script = super::build_macos_notification_script("razer \"battery\"", "line 1\nline 2");
        assert_eq!(
            script,
            r#"display notification "line 1\nline 2" with title "razer \"battery\"""#
        );
    }
}
