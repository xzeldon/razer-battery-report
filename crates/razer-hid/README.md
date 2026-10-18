# razer-hid

Reads the battery level of wireless Razer mice over HID. This is the library behind
[razer-battery-report](../../README.md); the tray app and its CLI use nothing else to talk to the
mouse.

```rust
fn main() -> Result<(), razer_hid::Error> {
    let hid = razer_hid::Hid::new()?;
    for device in hid.devices()? {
        let battery = device.battery()?;
        println!("{}: {}%", device.model(), battery.level.get());
    }
    Ok(())
}
```

## What it does

- Finds every supported mouse among the connected HID devices. A mouse plugged in by cable and
  through its receiver at the same time is one `Device`, grouped by the serial number the mouse
  reports.
- Reads the battery level and, where the mouse supports it, whether it is charging.
- Returns `Error::Timeout` when the mouse is asleep or switched off, so a caller can keep the last
  known level instead of treating it as a failure.

The supported mice are listed in [`src/catalog.rs`](src/catalog.rs). Product ids, transaction ids
and command bytes come from [openrazer](https://github.com/openrazer/openrazer).

## Using it

The crate is not on crates.io. Depend on it through git:

```toml
[dependencies]
razer-hid = { git = "https://github.com/xzeldon/razer-battery-report", tag = "v1.0.0" }
```

The library shares its version and tags with the app, so pin the tag of an app release. It is
tested on Windows; hidapi also builds on Linux and macOS, but nobody has checked the mice there
yet.

## License

MIT, see [LICENSE](../../LICENSE).
