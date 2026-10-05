# ThinkVision Control — macOS and Linux

Date: 2026-10-05
Status: draft for review
Builds on: [2026-10-05 Windows 11 redesign](2026-10-05-windows11-redesign-design.md). The behaviour, wording, safety rules and states defined there apply on every OS unless this document says otherwise.
Evidence:
- [reports/Cross platform monitor control.md](../../../reports/Cross%20platform%20monitor%20control.md)
- Avalonia preview screenshots for Windows and Linux: [docs/previews/avalonia/](../../previews/avalonia/)

## 1. Decisions (user, 2026-10-05)

| Topic | Decision |
|---|---|
| Targets | Windows; macOS on Apple Silicon; Linux on GNOME and KDE (X11 and Wayland) |
| UI strategy | **Staged migration.** A new Avalonia app ships on macOS and Linux now. Windows keeps the WPF app until the Avalonia build passes the gates in §7, then WPF is retired |
| Distribution | **Build from source only.** Terminal commands in the README; Homebrew installs the toolchain on macOS. No packages, signing, notarization, DMG, deb/rpm, AppImage or Homebrew formula for now |
| Native look on GNOME/KDE | Not a goal. The app draws its own Windows 11-style controls on every OS. Window frames, the tray menu, fonts and light/dark mode come from the system |

## 2. Projects

| Project | Target framework | Contents |
|---|---|---|
| `ThinkVisionControl.Core` | `net10.0` (was `net10.0-windows`) | Control service, safety rules, profile, settings, command line, backups, finder, and the new `Mccs/DdcPacket` codec. **No OS calls.** |
| `ThinkVisionControl.Ddc.Windows` | `net10.0-windows` | `Dxva2MonitorLink` (moved out of Core unchanged) |
| `ThinkVisionControl.Ddc.Linux` | `net10.0` | `I2cMonitorLink`: libc `open`/`ioctl(I2C_SLAVE)`/`read`/`write` on `/dev/i2c-N`, plus EDID match on the bus |
| `ThinkVisionControl.Ddc.MacOS` | `net10.0` | `IoAvMonitorLink`: P/Invoke to the private `IOAVServiceCreateWithService`, `IOAVServiceReadI2C` and `IOAVServiceWriteI2C` in IOKit, with display matching through the IORegistry |
| `ThinkVisionControl` (WPF) | `net10.0-windows` | Today's app. Only its references change |
| `ThinkVisionControl.Avalonia` | `net10.0` | The new cross-platform app (§4) |
| `ThinkVisionControl.Tests` | `net10.0` | Existing tests plus codec tests |

- The Linux and macOS links share `DdcPacket`: Set/Get/capabilities request encoding, reply parsing, and both checksums (0x6E/0x51 for requests, 0x50 for replies).
- All three links implement the existing `IMonitorLink`, so `DdcQueue`, `ControlService`, the safety rules and the states are shared unchanged.
- `Dxva2MonitorLink` keeps using the OS's built-in DDC/CI framing, not the codec.

## 3. Monitor link per OS

### 3.1 Linux
- **Finding the monitor:** enumerate `/dev/i2c-*` and read the EDID at address 0x50. Match manufacturer `LEN` and product `0x62A2`. Prefer the bus named by `/sys/class/drm/*/ddc` when it exists.
- **Talking to it:** address 0x37. Allow 50 ms between commands, with retries (writes ×4, write+read ×10).
- **Locking:** one lock per bus around each write–sleep–read sequence, plus an advisory `flock` on the device file to coexist with other tools.
- **Permission problems:**
  - No access → status `NotResponding`, with reason `PermissionDenied`, which drives the Linux setup card (§5).
  - No `/dev/i2c-*` at all → reason `ModuleMissing` ("load `i2c-dev`").
- **KDE PowerDevil:** detect a running `org_kde_powerdevil` process that has DDC enabled (no `POWERDEVIL_NO_DDCUTIL` in its environment), then show a warning card.

### 3.2 macOS (Apple Silicon)
- **Finding the service:** look up the `DCPAVServiceProxy` entry with `Location == "External"` that belongs to the framebuffer of the display whose `CGDisplayVendorNumber == 0x30AE` and `CGDisplayModelNumber == 0x62A2`. Use m1ddc's approach of registry-ID matching through `IODisplayLocation`.
- **Writing:** `IOAVServiceWriteI2C(svc, 0x37, 0x51, buf)`, sent twice with a 10 ms sleep before each.
- **Reading:** wait 50 ms, then `IOAVServiceReadI2C(svc, 0x37, 0, buf, 11)`. Retry up to 5 times, 20 ms apart.
- **Checksum:** try the specification form first. If the reply is invalid, fall back to the form that leaves 0x51 out of the checksum, as m1ddc and MonitorControl do. Remember which form worked for the session.
- **Re-lookup:** look the service up again after wake and after any display reconfiguration.
- **M1 / base M2 built-in HDMI:** use the m1ddc variant (chip address 0xB7). If it fails, show the reason "This Mac's HDMI port can't control the monitor; use USB-C".
- **Licensing:** all reference code is MIT (m1ddc, MonitorControl). Nothing GPL is copied (ddcctl).

## 4. Avalonia app

### 4.1 UI
- **Look:** layout A from the Windows 11 redesign, built with Avalonia 12.1.x and FluentAvalonia 3.1.x. `FANavigationView` provides the left pane with App settings in the footer. `FASettingsExpander` provides the cards. `FAInfoBar`, `FAContentDialog`, sliders, toggles, ComboBoxes and radio buttons cover the rest.
- **Shared view-model logic:** ported from the WPF `PanelViewModel`, in the same structure. It is a copy, not shared code, because WPF retires (§7).
- **Icons:** the bundled **Fluent UI System Icons** font (MIT), so icons render on every OS. No Segoe icon fonts.
- **Text font:** the system UI font.
- **Accent colour:** the system accent on Windows and macOS. On Linux, the desktop accent read from the portal (`org.freedesktop.appearance accent-color`), falling back to the brand ember `#F05A28`.
- **Background:**
  - Windows 11: Mica.
  - Every other OS: `TransparencyLevelHint = None` with a solid `SolidBackgroundFillColorBaseBrush` background. This is needed because Avalonia otherwise falls back to a see-through window, as observed in the Linux preview.
- **Linux backend:** X11 (XWayland on Wayland sessions). Avalonia's native Wayland backend can't position windows or keep them on top.

### 4.2 OS layer (`Platform/` in the Avalonia app)

Each feature is an interface with one implementation per OS, selected at startup.

| Interface | Windows | macOS | Linux |
|---|---|---|---|
| `IMonitorEvents` (plug/unplug) | `RegisterDeviceNotification` (existing) | `CGDisplayRegisterReconfigurationCallback` + wake | netlink `drm` uevents (fallback: re-check on `/sys/class/drm/*/status` change after display-change events) |
| `IHotkeys` | `RegisterHotKey` | Carbon `RegisterEventHotKey` (no permission prompt) | X11 `XGrabKey`; Wayland: `GlobalShortcuts` portal on KDE ≥ 5.27 and GNOME ≥ 48; otherwise unavailable, plus a shortcut-setup card |
| `ITray` | Avalonia `TrayIcon` + flyout | Menu-bar item (`ShowInDock = false`); flyout under the item | `TrayIcon` (StatusNotifierItem); quick controls in the **native menu**, no flyout |
| `IOsd` | Overlay window | Overlay window (`ignoresMouseEvents`, floating level) | X11: overlay window; Wayland: `org.freedesktop.Notifications` with `replaces_id` |
| `IAutostart` | Run key | `SMAppService.mainApp` | `~/.config/autostart/thinkvision-control.desktop` |
| `IQuickActions` | Jump list | Menu-bar menu items | `.desktop` `Actions=` (when the user installs the desktop file from the README) |
| `ISingleInstance` | Named pipe (existing) | Lock file + Unix socket (≤ 103-char path) | Lock file + Unix socket |
| `IThemeInfo` | Registry | Avalonia / `NSAppearance` | Portal `color-scheme` |

**Command-line verbs** (`--preset`, `--input`, `--step`, `--startup`) are identical on every OS. On Linux without hotkeys they are the documented way to bind desktop shortcuts.

## 5. Setup help in the app

App settings shows a "System setup" card only when it applies. Each card explains the issue in one sentence and gives the exact fix with a copy button.

| Condition | Card |
|---|---|
| Linux, `PermissionDenied` | "Allow access to monitor controls". The fix is the udev rule from §6, or installing the distro's `ddcutil` package, then logging out and back in |
| Linux, `ModuleMissing` | "Load the i2c-dev kernel module" (`modules-load.d` line) |
| KDE PowerDevil DDC active | "KDE's power manager also controls monitor brightness". Shows the `POWERDEVIL_NO_DDCUTIL=1` override command |
| GNOME without StatusNotifierItem host | "Install the AppIndicator extension to see the tray icon" |
| Hotkeys unavailable (Wayland without portal) | "Set desktop shortcuts". Lists each action's command |
| macOS, DDC fails on a built-in HDMI port | "Use a USB-C or Thunderbolt port for this monitor" |

## 6. README: build from source

The README gets one section per OS:
- prerequisites (Windows: `winget install Microsoft.DotNet.SDK.10`; macOS: `brew install --cask dotnet-sdk`; Ubuntu/Fedora: apt/dnf `dotnet-sdk-10.0`);
- `git clone`;
- `dotnet run` / `dotnet publish` commands for the right project and RID (`win-x64`, `osx-arm64`, `linux-x64`);
- where the result goes and how to start it.

Platform extras:
- **Linux:**
  - `sudo modprobe i2c-dev` plus persistence;
  - the udev rule `/etc/udev/rules.d/60-thinkvision-i2c.rules` with `KERNEL=="i2c-[0-9]*", TAG+="uaccess"` (or `sudo apt install ddcutil`);
  - an optional `.desktop` file install so the app appears in launchers with Actions;
  - the PowerDevil note;
  - the NVIDIA `RMUseSwI2c` note.
- **macOS:**
  - building a menu-bar `.app` bundle (`Info.plist` with `LSUIElement`) from the publish output with a small script (`scripts/make-macos-app.sh`);
  - a note that locally built apps are not quarantined, so no signing is needed;
  - the HDMI-port note.
- **Windows:** build the WPF app until §7's gates pass. The Avalonia app is listed as "preview on Windows".

## 7. Windows switch gates

Measured on the developer's laptop, Release build, against the WPF build at the same commit. All four must pass before the README switches Windows to Avalonia and the WPF project is removed:
1. Mica renders on the main window.
2. Time to first window ≤ 0.6 s (WPF: about 0.5 s).
3. Private memory for a tray-only login launch ≤ 24 MB (WPF today).
4. A side-by-side screenshot review against the WPF app, signed off by the user.

Levers to reach the gates, each to be measured: NativeAOT (FluentAvalonia AOT compatibility unverified), and software rendering only on surfaces where Mica isn't needed.

## 8. Lightness

The rules are the same as the Windows app:
- exit on close by default;
- a login launch never creates a window;
- event-driven reconnects with a bounded retry burst, and no polling;
- efficiency hints while hidden where the OS has them: EcoQoS on Windows; on macOS, App Nap applies automatically because the app never opts out of it;
- no compression of single-file output.

Memory is measured per OS in §9.

## 9. Testing

**Automated (CI-free, run locally):**
- the existing 76 tests, with Core now `net10.0`;
- new `DdcPacket` tests built on recorded byte sequences: Set, Get, reply parsing, bad-checksum rejection, the capabilities fragment loop, and the macOS checksum fallback;
- `CommandLine` unchanged;
- per-OS link classes are kept thin, and their logic sits behind small testable helpers: EDID matching from a byte array, and Linux bus selection from a sysfs listing.

**Manual, on hardware (scripted in the README):**

| Where | Checks |
|---|---|
| Ubuntu under WSL on the laptop (already installed) | Avalonia UI pages, both themes, all states (the monitor is unreachable from WSL, so Not found / permission states) |
| Live-USB Ubuntu or Fedora on the laptop with the P32p-20 on HDMI | I²C detection, Get/Set, the permission card before and after the udev rule, hotplug, GNOME tray with and without the extension, hotkeys or the setup card |
| KDE (live USB or a KDE machine) | Tray, portal hotkeys, PowerDevil card |
| Apple Silicon Mac on USB-C/Thunderbolt | Detection by vendor/model, Get/Set, checksum mode, menu-bar item and flyout, Carbon hotkeys, OSD, login item, sleep/wake re-lookup |
| Windows | The §7 gate measurements |

## 10. Out of scope

- Installers, packages, signing, notarization, app-store distribution, a Homebrew formula and Flatpak/Snap.
- Intel Macs.
- A GTK, Qt or libadwaita native look.
- KDE blur and macOS vibrancy (solid backgrounds instead).
- Running Avalonia's native Wayland backend (X11/XWayland only).
