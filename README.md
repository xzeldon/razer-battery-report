# razer-battery-report

Shows the battery level of a wireless Razer mouse in the Windows system tray.

The tray icon is a mouse that fills up with the charge. It turns yellow when the battery is low
and red at 5% or less. A lightning bolt on the icon means the mouse is charging, a moon means it
is asleep, and a crossed-out mouse means no supported mouse is connected. Hover over the icon to
see the exact percentage.

Only Windows is supported for now. Linux and macOS are planned.

## Install

1. Download the file ending in `-setup.exe` from the
   [latest release](https://github.com/xzeldon/razer-battery-report/releases/latest).
2. Run it. It installs for your Windows user only and does not ask for admin rights.
3. If Windows hides the icon, drag it out of the overflow area of the taskbar.
4. Turn on **Start at login** in the tray menu to have it start with Windows.

The release also has the plain exe without an installer. Put it in any folder and run it. It
works the same way.

Windows 11 24H2 or newer is recommended. Older Windows, including Windows 10, works too, but a
console window flashes for a moment each time the app starts.

## Uninstall

Remove the app in **Settings > Apps**. The uninstaller asks whether to delete your settings and
logs. The autostart entry and the rest of the app's files go either way.

For the plain exe, exit the app, run `razer-battery-report cleanup --all` in a terminal and
delete the exe. Leave out `--all` to keep the settings and logs.

## Tray menu

Right-click the icon to open the menu.

- The connected mice. With more than one connected, click the one the icon should show.
- **Battery alerts** turns the popups on or off. A popup appears when a mouse connects or
  disconnects, when its battery gets low or nearly empty, and when it finishes charging.
- **Low battery** sets the level at which the icon turns yellow and the low battery popup
  appears: 10, 15, 20 or 25%.
- **Show level on icon** turns the fill off if you prefer a plain outline.
- **Start at login** starts the app when you log in to Windows.
- **Updates** decides what happens when a new version comes out. The app checks GitHub once a
  day. **Notify only**, the default, shows a popup and adds **Update to vX.Y.Z** to the menu.
  **Install automatically** installs it without asking. **Off** turns the daily check off.
  **Check now** checks right away in any mode.
- **Open log** opens the log file. Attach it when you report a bug.
- **About** shows the version and links to this page and to the licenses of the libraries the
  app is built from.

## Command line

The same exe prints the battery level in a terminal:

```console
> razer-battery-report status
Razer DeathAdder V4 Pro  42%
```

`status --json` prints the same data as JSON, with the serial number and the connection of every
mouse. `diagnose` prints a report with the app version, the paths below and every mouse it finds;
paste it into a bug report. `cleanup` removes everything the app stored on the computer, as
described under [Uninstall](#uninstall).

## Files

The menu settings are saved to `%APPDATA%\razer-battery-report\config.toml`. Logs are in
`%LOCALAPPDATA%\razer-battery-report\logs\`.

## Supported mice

Each connection of a mouse has its own USB product id, listed below with Razer's vendor id 1532.
A mouse connected by cable and receiver at the same time shows up once.

| Mouse | Cable | Receiver | Bluetooth |
| --- | --- | --- | --- |
| Razer Atheris ¹ |  | 0062 |  |
| Razer Basilisk Mobile ¹ | 00D3 | 00D4 |  |
| Razer Basilisk Ultimate | 0086 | 0088 |  |
| Razer Basilisk V3 Pro | 00AA | 00AB |  |
| Razer Basilisk V3 Pro 35K | 00CC | 00CD |  |
| Razer Basilisk V3 Pro 35K Phantom Green Edition | 00D6 | 00D7 |  |
| Razer Basilisk V3 X HyperSpeed ¹ |  | 00B9 |  |
| Razer Basilisk X HyperSpeed ¹ |  | 0083 |  |
| Razer Cobra HyperSpeed | 00DA | 00DB |  |
| Razer Cobra Pro | 00AF | 00B0 |  |
| Razer DeathAdder V2 Pro | 007C | 007D |  |
| Razer DeathAdder V2 X HyperSpeed ¹ |  | 009C |  |
| Razer DeathAdder V3 HyperSpeed | 00C4 | 00C5 |  |
| Razer DeathAdder V3 Pro | 00B6, 00C2 | 00B7, 00C3 |  |
| Razer DeathAdder V4 Pro | 00BE | 00BF |  |
| Razer HyperPolling Wireless Dongle |  | 00B3 |  |
| Razer Lancehead | 0059 | 005A |  |
| Razer Lancehead Wireless | 0070 | 006F |  |
| Razer Mamba 2012 | 0024 | 0025 |  |
| Razer Mamba Chroma | 0044 | 0045 |  |
| Razer Mamba Wireless | 0073 | 0072 |  |
| Razer Naga Epic | 001F |  |  |
| Razer Naga Epic Chroma | 003E | 003F |  |
| Razer Naga Pro | 008F | 0090 |  |
| Razer Naga V2 HyperSpeed ¹ |  | 00B4 |  |
| Razer Naga V2 Pro | 00A7 | 00A8 |  |
| Razer Orochi V2 ¹ |  | 0094 | 0095 |
| Razer Ouroboros | 0032 |  |  |
| Razer Pro Click | 0080 | 0077 |  |
| Razer Pro Click Mini |  | 009A |  |
| Razer Pro Click V2 | 00D0 | 00D1 |  |
| Razer Pro Click V2 Vertical Edition | 00C7 | 00C8 |  |
| Razer Viper Mini SE | 009E | 009F |  |
| Razer Viper Ultimate | 007A | 007B |  |
| Razer Viper V2 Pro | 00A5 | 00A6 |  |
| Razer Viper V3 HyperSpeed ¹ |  | 00B8 |  |
| Razer Viper V3 Pro | 00C0 | 00C1 |  |
| Razer Viper V3 Pro SE | 00DE | 00DF |  |

¹ The mouse cannot report whether it is charging, so the app never shows the lightning bolt or the
"fully charged" popup for it.

The list comes from [openrazer](https://github.com/openrazer/openrazer). If your mouse is
missing, see [adding a mouse](CONTRIBUTING.md#adding-a-mouse).

## Questions

**My mouse runs on rechargeable AA batteries and the percentage looks wrong.** The mouse computes
the percentage itself and expects alkaline batteries. NiMH batteries hold a lower, flatter voltage,
so the reading drops early and then stays put. The app shows what the mouse reports.

**The mouse is connected but the icon shows a moon.** The mouse is asleep and does not answer.
Move it and the level comes back within a few seconds.

## Building from source

See [CONTRIBUTING.md](CONTRIBUTING.md). Packagers can build with `--no-default-features` to leave
out the update check, so the package manager handles updates instead.

## Credits

- [openrazer](https://github.com/openrazer/openrazer) for the protocol and the device data.
- [Lucide](https://lucide.dev) for the icons, used under the ISC license
  ([`assets/icons/lucide/LICENSE`](assets/icons/lucide/LICENSE)).
- [razer-battery-checker](https://github.com/spozer/razer-battery-checker), a Python script that
  reads the same battery data.
- [elem](https://github.com/Fuwn/elem), a battery tray icon for Logitech mice.

## License

MIT, see [LICENSE](LICENSE).
