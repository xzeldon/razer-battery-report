//! Turns readings into events: the notification rules.

use std::fmt;

use razer_hid::{Battery, Device, Endpoint, Percent};

/// At or below this level, a mouse that is not charging is critical.
const CRITICAL_LEVEL: u8 = 5;
/// The level at which a charging mouse is full.
const FULL_LEVEL: u8 = 100;

/// Identity of one physical mouse between two scans: its protocol serial when it has one,
/// otherwise the path of its preferred endpoint.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DeviceKey(String);

impl DeviceKey {
    pub fn of(device: &Device) -> Self {
        let key = match device.serial() {
            Some(serial) => serial.to_owned(),
            None => device
                .endpoints()
                .first()
                .map(Endpoint::path)
                .unwrap_or_default()
                .to_owned(),
        };
        Self(key)
    }

    /// The key of a mouse without a serial, which is the path of its preferred endpoint.
    pub fn from_path(path: &str) -> Self {
        Self(path.to_owned())
    }

    /// A key with a made-up value, for tests in other modules.
    #[cfg(test)]
    pub fn for_test(key: &str) -> Self {
        Self(key.to_owned())
    }
}

/// What is known about one mouse right now.
#[derive(Clone, Debug, PartialEq)]
pub struct Reading {
    pub key: DeviceKey,
    /// Model name without the connection suffix.
    pub model: &'static str,
    /// The serial the mouse reports, if any.
    pub serial: Option<String>,
    /// The last battery reading; stays in place while the mouse sleeps.
    pub battery: Option<Battery>,
    /// Whether the last attempt to read the battery timed out.
    pub asleep: bool,
}

/// Something worth telling the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A mouse appeared while the app was running.
    Connected { model: &'static str },
    /// A mouse went away.
    Disconnected { model: &'static str },
    /// The battery fell to the low threshold.
    Low { model: &'static str, level: Percent },
    /// The battery is nearly empty.
    Critical { model: &'static str, level: Percent },
    /// The battery reached 100% while charging.
    Full { model: &'static str },
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connected { model } => write!(f, "{model} connected"),
            Self::Disconnected { model } => write!(f, "{model} disconnected"),
            Self::Low { model, level } => write!(f, "{model} battery is low ({}%)", level.get()),
            Self::Critical { model, level } => {
                write!(f, "{model} battery is critical ({}%)", level.get())
            }
            Self::Full { model } => write!(f, "{model} is fully charged"),
        }
    }
}

/// Events caused by the change from `previous` to `current`. `previous` is `None` at app start:
/// nothing counts as connected then, and every battery counts as newly seen.
pub fn events(
    previous: Option<&[Reading]>,
    current: &[Reading],
    low_threshold: Percent,
) -> Vec<Event> {
    let before = previous.unwrap_or_default();
    let mut events = Vec::new();
    if previous.is_some() {
        for reading in current {
            if !before.iter().any(|old| old.key == reading.key) {
                events.push(Event::Connected {
                    model: reading.model,
                });
            }
        }
        for old in before {
            if !current.iter().any(|reading| reading.key == old.key) {
                events.push(Event::Disconnected { model: old.model });
            }
        }
    }
    for reading in current {
        let Some(now) = reading.battery else {
            continue;
        };
        let was = before
            .iter()
            .find(|old| old.key == reading.key)
            .and_then(|old| old.battery);
        if was == Some(now) {
            continue;
        }
        events.extend(battery_event(reading.model, was, now, low_threshold));
    }
    events
}

/// The event for a battery that changed from `was` (`None` when unknown) to `now`, if any.
fn battery_event(
    model: &'static str,
    was: Option<Battery>,
    now: Battery,
    low_threshold: Percent,
) -> Option<Event> {
    let level = now.level;
    let charging = now.charging == Some(true);
    if !charging && level.get() <= CRITICAL_LEVEL {
        return Some(Event::Critical { model, level });
    }
    if !charging && level <= low_threshold && was.is_none_or(|was| was.level > low_threshold) {
        return Some(Event::Low { model, level });
    }
    let was_full =
        was.is_some_and(|was| was.level.get() == FULL_LEVEL && was.charging == Some(true));
    if charging && level.get() == FULL_LEVEL && !was_full {
        return Some(Event::Full { model });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn percent(value: u8) -> Percent {
        Percent::try_from(value).unwrap()
    }

    fn battery(level: u8, charging: Option<bool>) -> Battery {
        Battery {
            level: percent(level),
            charging,
        }
    }

    fn reading(key: &str, battery: Option<Battery>) -> Reading {
        Reading {
            key: DeviceKey(key.to_owned()),
            model: "Mouse",
            serial: Some(key.to_owned()),
            battery,
            asleep: false,
        }
    }

    fn one(level: u8, charging: Option<bool>) -> Vec<Reading> {
        vec![reading("a", Some(battery(level, charging)))]
    }

    fn run(previous: Option<&[Reading]>, current: &[Reading]) -> Vec<Event> {
        events(previous, current, percent(15))
    }

    fn low(level: u8) -> Event {
        Event::Low {
            model: "Mouse",
            level: percent(level),
        }
    }

    fn critical(level: u8) -> Event {
        Event::Critical {
            model: "Mouse",
            level: percent(level),
        }
    }

    const NONE: [Event; 0] = [];
    const FULL: Event = Event::Full { model: "Mouse" };

    #[test]
    fn connect_and_disconnect() {
        let with = one(50, Some(false));
        assert_eq!(run(Some(&[]), &with), [Event::Connected { model: "Mouse" }]);
        assert_eq!(
            run(Some(&with), &[]),
            [Event::Disconnected { model: "Mouse" }]
        );
    }

    #[test]
    fn startup_has_no_connected_event() {
        assert_eq!(run(None, &one(50, Some(false))), NONE);
        assert_eq!(run(None, &[]), NONE);
    }

    #[test]
    fn startup_with_a_low_mouse_fires_low() {
        assert_eq!(run(None, &one(10, Some(false))), [low(10)]);
    }

    #[test]
    fn asleep_alone_is_silent() {
        let before = one(10, Some(false));
        let mut after = before.clone();
        after[0].asleep = true;
        assert_eq!(run(Some(&before), &after), NONE);
    }

    #[test]
    fn low_fires_once_when_crossing_the_threshold_downwards() {
        let a = one(16, Some(false));
        let b = one(15, Some(false));
        let c = one(14, Some(false));
        assert_eq!(run(Some(&a), &b), [low(15)]);
        assert_eq!(run(Some(&b), &c), NONE);
    }

    #[test]
    fn low_does_not_fire_when_unplugging_while_already_low() {
        let charging = one(10, Some(true));
        let unplugged = one(10, Some(false));
        assert_eq!(run(Some(&charging), &unplugged), NONE);
    }

    #[test]
    fn unknown_charging_counts_as_not_charging() {
        assert_eq!(run(Some(&one(20, None)), &one(14, None)), [low(14)]);
    }

    #[test]
    fn critical_fires_on_every_change() {
        let a = one(5, Some(false));
        let b = one(4, Some(false));
        let c = one(4, None);
        assert_eq!(run(Some(&one(6, Some(false))), &a), [critical(5)]);
        assert_eq!(run(Some(&a), &b), [critical(4)]);
        assert_eq!(run(Some(&b), &c), [critical(4)]);
        assert_eq!(run(Some(&b), &b), NONE);
    }

    #[test]
    fn critical_replaces_low() {
        assert_eq!(
            run(Some(&one(40, Some(false))), &one(4, Some(false))),
            [critical(4)]
        );
    }

    #[test]
    fn full_fires_when_reaching_100_while_charging() {
        assert_eq!(
            run(Some(&one(99, Some(true))), &one(100, Some(true))),
            [FULL]
        );
    }

    #[test]
    fn full_fires_only_once() {
        let full = one(100, Some(true));
        assert_eq!(run(Some(&full), &full), NONE);
    }

    #[test]
    fn full_never_fires_when_charging_is_unknown() {
        assert_eq!(run(Some(&one(99, None)), &one(100, None)), NONE);
        assert_eq!(run(None, &one(100, None)), NONE);
    }
}
