<h1 align="center">razer-battery-report</h1>

<p align="center">
  <b>Razer Battery Level Tray Indicator</b>
</p>

<p align="center">
  <img src="img/demo.png">
</p>

Show your wireless Razer devices battery levels in your system tray.

> Currently, this works only on **Windows**.

## Supported devices

| Device                                                     | USB VID:PID |
| ---------------------------------------------------------- | ----------- |
| Razer DeathAdder V3 Pro (Wired)                            | 1532:00B6   |
| Razer DeathAdder V3 Pro (Wireless)                         | 1532:00B7   |
| Razer DeathAdder V3 HyperSpeed (Wired)                     | 1532:00C4   |
| Razer DeathAdder V3 HyperSpeed (Wireless)                  | 1532:00C5   |
| Razer DeathAdder V2 Pro (Wired)                            | 1532:007C   |
| Razer DeathAdder V2 Pro (Wireless)                         | 1532:007D   |
| Razer DeathAdder V4 Pro (Wired)                            | 1532:00BE   |
| Razer DeathAdder V4 Pro (Wireless)                         | 1532:00BF   |
| Razer Basilisk V3 Pro (Wired)                              | 1532:00AA   |
| Razer Basilisk V3 Pro (Wireless)                           | 1532:00AB   |
| Razer Basilisk V3 Pro 35K (Wired)                          | 1532:00CC   |
| Razer Basilisk V3 Pro 35K (Wireless)                       | 1532:00CD   |
| Razer Basilisk V3 Pro 35K Phantom Green Edition (Wired)    | 1532:00D6   |
| Razer Basilisk V3 Pro 35K Phantom Green Edition (Wireless) | 1532:00D7   |
| Razer Viper Ultimate (Wired)                               | 1532:007A   |
| Razer Viper Ultimate (Wireless)                            | 1532:007B   |
| Razer Orochi V2 (Receiver)                                 | 1532:0094   |
| Razer Orochi V2 (Bluetooth)                                | 1532:0095   |
| Razer Viper V4 Pro (Wired)                                 | 1532:00E5   |
| Razer Viper V4 Pro (Wireless)                              | 1532:00E6   |

> **Note:** If your device is not listed, you can add support for it yourself! Please see the [Adding new devices yourself](#adding-new-devices-yourself) section below. Contributions and Pull Requests are welcome!

## Usage

### Installation

> **Note:** Since this project is currently in active development, there is no installer (`.msi` or setup `.exe`) yet. To set up the application permanently (Start Menu icon, Autostart), please follow the manual steps below.

1. **Download** the latest `razer-battery-report.exe` from [Releases](https://github.com/xzeldon/razer-battery-report/releases/latest).

2. **Move the executable** to a safe permanent location.
   - _Recommended:_ Create a folder at `C:\Program Files\Razer Battery Report\` and move the `.exe` there.
   - _Note:_ This prevents accidental deletion from your Downloads folder.

3. **Add to Startup** (Recommended):
   - Right-click `razer-battery-report.exe` -> **Create shortcut**.
   - Press `Win + R`, type `shell:startup` and press Enter.
   - Move the created shortcut into the opened folder.
   - _Now the app will start automatically when you log in._

4. **Add to Start Menu** (Optional):
   - Create another shortcut.
   - Press `Win + R`, type `shell:programs` and press Enter.
   - Move the shortcut into the opened folder.
   - _Now you can search for "Razer Battery Report" in Windows Search._

> **Tips:**
>
> - You can rename the shortcuts to simply **"Razer Battery Report"** (remove ".exe" and "Shortcut").
> - **Custom Icon:**
>   1. Download [`mouse.ico`](https://github.com/xzeldon/razer-battery-report/raw/master/assets/mouse.ico) (save it in the same folder as the `.exe`).
>   2. Right-click the shortcut -> **Properties** -> **Change Icon...** -> **Browse** -> Select the downloaded `.ico` file.

### Building from Source

To build, you must have [Rust](https://www.rust-lang.org/) and
[Git](https://git-scm.com/) installed on your system.

1. Clone this repository: `git clone https://github.com/xzeldon/razer-battery-report.git`
2. Navigate into your local repository: `cd razer-battery-report`
3. Build: `cargo build --release`
4. Executable will be located at `target/release/razer-battery-report.exe`

## Adding new devices yourself

- add device with `name`, `pid`, `interface`, `usage_page`, `usage` to [devices.rs](/src/devices.rs)
- add `transaction_id` to switch statement in `DeviceInfo` in [devices.rs](/src/devices.rs)

> You can grab `pid` and other data from the [openrazer](https://github.com/openrazer/openrazer/blob/685290bb0128dc0b063c524b55f76f4091da9b15/driver/razermouse_driver.h)

## Todo

- [x] Tray Applet
  - [ ] Force update devices button in tray menu
  - [x] Colored tray icons for different battery levels
  - [x] Show log window button in tray menu
  - [x] Further reduce CPU usage by using Event Loop Proxy events (more info [here](https://github.com/tauri-apps/tray-icon/issues/83#issuecomment-1697773065))
- [x] Prebuilt Binary
- [ ] Command Line Arguments for update frequency
- [ ] Support for other Razer Devices (I only have DeathAdder V3 Pro, so I won't be able to test it with other devices)

## Acknowledgments

- Linux Drivers for Razer devices: https://github.com/openrazer/openrazer
- This python script: https://github.com/spozer/razer-battery-checker
- 🖱️ Logitech Battery Level Tray Indicator (Elem): https://github.com/Fuwn/elem
