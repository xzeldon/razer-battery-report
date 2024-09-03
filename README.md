<h1 align="center">razer-battery-report</h1>

<p align="center">
  <b>Razer Battery Level Tray Indicator</b>
</p>

<p align="center">
  <img src="img/demo.png">
</p>

Show your wireless Razer devices battery levels in your system tray.

> This is a work in progress and currently support only **Razer DeathAdder V3 Pro**.

> This works pretty well on **Windows**, should work on **Linux** if you *add udev rule to get access to usb devices* (see [here](https://github.com/libusb/hidapi/blob/master/udev/69-hid.rules)). But I haven't tested yet.

## Usage

### Downloading a Prebuilt Binary
> *Todo*

### Building from Source

To build, you must have [Rust](https://www.rust-lang.org/) and
[Git](https://git-scm.com/) installed on your system.

1. Clone this repository: `git clone https://github.com/xzeldon/razer-battery-report.git`
2. Navigate into your local repository: `cd razer-battery-report`
3. Build: `cargo build razer-battery-report --release`
4. Executable will be located at `target/release/razer-battery-report`

## Adding new devices yourself
* add device with `name`, `pid`, `interface`, `usage_page`, `usage` to [devices.rs](/src/devices.rs)
* add `transaction_id` to switch statement in `DeviceInfo` in [devices.rs](/src/devices.rs)

> You can grab `pid` and other data from the [openrazer](https://github.com/openrazer/openrazer/blob/352d13c416f42e572016c02fd10a52fc9848644a/driver/razermouse_driver.h#L9)

## Todo
- [x] Tray Applet
  - [ ] Force update devices button in tray menu
  - [ ] Colored tray icons for different battery levels
  - [ ] Show log window button in tray menu
- [ ] Prebuilt Binary
- [ ] Command Line Arguments for update frequency
- [ ] Support for other Razer Devices (I only have DeathAdder V3 Pro, so I won't be able to test it with other devices)

## Acknowledgments
* Linux Drivers for Razer devices: https://github.com/openrazer/openrazer
* This python script: https://github.com/spozer/razer-battery-checker
* 🖱️ Logitech Battery Level Tray Indicator (Elem): https://github.com/Fuwn/elem