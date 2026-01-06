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
- Razer DeathAdder V3 Pro
- Razer DeathAdder V3 HyperSpeed
- Razer DeathAdder V2 Pro

> **Note:** If your device is not listed, you can add support for it yourself! Please see the [Adding new devices yourself](#adding-new-devices-yourself) section below. Contributions and Pull Requests are welcome!

## Usage

### Installation

> **Note:** Since this project is currently in active development, there is no installer (`.msi` or setup `.exe`) yet. To set up the application permanently (Start Menu icon, Autostart), please follow the manual steps below.

1. **Download** the latest `razer-battery-report.exe` from [Releases](https://github.com/xzeldon/razer-battery-report/releases/latest).

2. **Move the executable** to a safe permanent location.
   * *Recommended:* Create a folder at `C:\Program Files\Razer Battery Report\` and move the `.exe` there.
   * *Note:* This prevents accidental deletion from your Downloads folder.

3. **Add to Startup** (Recommended):
   * Right-click `razer-battery-report.exe` -> **Create shortcut**.
   * Press `Win + R`, type `shell:startup` and press Enter.
   * Move the created shortcut into the opened folder.
   * *Now the app will start automatically when you log in.*

4. **Add to Start Menu** (Optional):
   * Create another shortcut.
   * Press `Win + R`, type `shell:programs` and press Enter.
   * Move the shortcut into the opened folder.
   * *Now you can search for "Razer Battery Report" in Windows Search.*

> **Tips:**
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

> You can grab `pid` and other data from the [openrazer](https://github.com/openrazer/openrazer/blob/352d13c416f42e572016c02fd10a52fc9848644a/driver/razermouse_driver.h#L9)

## How it works

**Architecture**
The application runs on two threads:
1.  **Main Thread:** System tray, menu interactions, and OS event loop.
2.  **Worker Thread:** HID communication and device state management.

**Polling Strategy**
*   **Hotplug Check (default 3s):** Lightweight USB bus scan. Detects connections/disconnections without waking the device. Changes trigger an immediate update.
*   **Battery Query (default 60s):** Sends HID reports to fetch the actual battery level.
*   **State Caching:** If a device sleeps (command timeout), the app retains the *last known value*. This prevents "Device Connected" notification when mouse wakes up from sleep.

**Logic**
*   **Notifications:** Triggered only on specific state transitions (e.g., Discharging → Charging, Level < Threshold).
*   **Persistence:** The active device selection is saved to `config.toml`. If the preferred device is unavailable, the app falls back to the first detected device.

**Configuration**
*   Stored in `config.toml` following standard OS paths (XDG on Linux, AppData on Windows).

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
