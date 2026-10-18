//! When the updater looks for a new release. All times are unix seconds of the wall clock, so
//! a laptop that slept through the due time checks as soon as it wakes.

use std::time::Duration;

const CHECK_INTERVAL_SECONDS: u64 = 24 * 60 * 60;

pub const FIRST_TICK: Duration = Duration::from_secs(60);
pub const TICK: Duration = Duration::from_hours(1);

/// Whether the last check is old enough for another one. A last check in the future means the
/// clock was set back, which counts as due.
pub fn check_due(checked_at: Option<u64>, now: u64) -> bool {
    checked_at.is_none_or(|at| at > now || now - at >= CHECK_INTERVAL_SECONDS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_is_due_after_a_day_or_when_never_done() {
        let day = CHECK_INTERVAL_SECONDS;
        assert!(check_due(None, 100));
        assert!(!check_due(Some(1000), 1000 + day - 1));
        assert!(check_due(Some(1000), 1000 + day));
        assert!(check_due(Some(5000), 1000));
    }
}
