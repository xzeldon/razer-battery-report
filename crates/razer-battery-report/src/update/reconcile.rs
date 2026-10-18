//! What a freshly started app learns about the update that ran before it: whether it worked
//! and, for the installer, why not.

use semver::Version;

const REASON_FALLBACK: &str = "Its log gives no reason";
pub const NOT_STARTED_REASON: &str = "The new version did not start";
/// Precedes the text of a message box that a silent run suppressed; in a silent run every
/// failure the user would have seen ends up there.
const SUPPRESSED_BOX_MARKER: &str = "for suppressed message box";

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Nothing,
    Updated(String),
    /// An update was started and the old version still runs.
    Failed,
}

/// Decides from the recorded `pending` version (an update was started) and `last_version`
/// (the version that ran before) what happened. Unreadable versions are ignored.
pub fn decide(pending: Option<&str>, last_version: Option<&str>, current: &Version) -> Outcome {
    let parse = |text: Option<&str>| text.and_then(|text| Version::parse(text).ok());
    match (parse(pending), parse(last_version)) {
        (Some(pending), _) if *current >= pending => Outcome::Updated(current.to_string()),
        (Some(_), _) => Outcome::Failed,
        (None, Some(last)) if last < *current => Outcome::Updated(current.to_string()),
        (None, _) => Outcome::Nothing,
    }
}

/// The reason the installer gave in its log: the first sentence of the first message box it
/// suppressed, without the file path that file errors start with. `None` when the log has no
/// such box.
pub fn failure_reason(log: &str) -> Option<String> {
    let mut lines = log.lines();
    lines.find(|line| line.contains(SUPPRESSED_BOX_MARKER))?;
    // The box text continues on indented or blank lines; the next log entry starts with a
    // timestamp.
    let text = lines
        .take_while(|line| line.is_empty() || line.starts_with(char::is_whitespace))
        .map(str::trim)
        .filter(|line| !line.is_empty() && !is_path(line))
        .collect::<Vec<_>>()
        .join(" ");
    let sentence = text
        .split_once(". ")
        .map_or(text.as_str(), |(sentence, _)| sentence);
    let sentence = sentence.trim_end_matches('.');
    (!sentence.is_empty()).then(|| sentence.to_owned())
}

pub fn failure_text(log: Option<&str>) -> String {
    log.and_then(failure_reason)
        .unwrap_or_else(|| REASON_FALLBACK.to_owned())
}

/// Whether a line of a message box is a file path, which Inno puts above file errors.
fn is_path(line: &str) -> bool {
    line.contains(":\\") || line.starts_with(r"\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_outcome() {
        let current = Version::new(1, 2, 0);
        let outcome = |pending, last| decide(pending, last, &current);
        let updated = Outcome::Updated("1.2.0".to_owned());
        assert_eq!(outcome(Some("1.2.0"), Some("1.1.0")), updated);
        assert_eq!(outcome(Some("1.1.0"), None), updated);
        assert_eq!(outcome(Some("1.3.0"), Some("1.2.0")), Outcome::Failed);
        assert_eq!(outcome(None, Some("1.1.0")), updated);
        assert_eq!(outcome(None, Some("1.2.0")), Outcome::Nothing);
        assert_eq!(outcome(None, None), Outcome::Nothing);
        assert_eq!(outcome(Some("junk"), Some("junk")), Outcome::Nothing);
    }

    #[test]
    fn reason_comes_from_the_suppressed_message_box() {
        // From a real Inno Setup 6.7.3 log of `setup /VERYSILENT /SUPPRESSMSGBOXES`.
        let log = "2026-10-19 05:09:39.020   Defaulting to Abort for suppressed message box (Abort/Retry/Ignore):
                          Setup was unable to automatically close all applications. It is recommended that you close all applications.
2026-10-19 05:09:39.020   User canceled the installation process.";
        assert_eq!(
            failure_reason(log).as_deref(),
            Some("Setup was unable to automatically close all applications")
        );
        assert_eq!(failure_reason("Installation process succeeded."), None);
    }

    #[test]
    fn reason_skips_the_path_of_a_file_error() {
        // From a real Inno Setup 6.7.3 log, with the install folder denied to the user.
        let log = r"2026-10-19 07:03:42.672   Defaulting to Abort for suppressed message box (Abort/Retry/Ignore):
                          C:\Users\user\AppData\Local\Programs\Razer Battery Report\unins000.exe

                          An error occurred while trying to create a file in the destination directory:
                          Access is denied.
2026-10-19 07:03:42.672   User canceled the installation process.";
        assert_eq!(
            failure_reason(log).as_deref(),
            Some(
                "An error occurred while trying to create a file in the destination directory: \
                 Access is denied"
            )
        );
    }
}
