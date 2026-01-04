use log::warn;
use notify_rust::Notification;

/// Handles sending desktop notifications.
///
/// This module abstracts over the `notify-rust` crate, providing a unified interface
/// for sending notifications with the correct application name and error handling.
pub struct Notifier;

impl Notifier {
    /// Sends a standard notification with the application name.
    pub fn send(summary: &str, body: &str) {
        // On Windows, appname is used for grouping in the Action Center.
        let res = Notification::new()
            .appname("Razer Battery Report")
            .summary(summary)
            .body(body)
            .show();

        if let Err(e) = res {
            warn!("Failed to show notification: {}", e);
        }
    }
}
