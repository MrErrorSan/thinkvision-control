# Windows tools, built-in features and scripting for DDC/CI control of an external monitor (Lenovo ThinkVision P32p-20)

Research date: 2026-10-03. Scope: Windows 11 only. Sources were fetched live. Where a WebFetch summary looked unreliable, that is flagged.

## Q1. Native Windows 11: does Settings or Quick Settings control external monitor brightness over DDC/CI?

### Takeaway
No. As of mid/late 2026, Windows 11 (24H2/25H2) has **no built-in DDC/CI brightness control for external monitors**. The Settings > Display and Quick Settings brightness slider only covers the panel the OEM marks as internal (the laptop's own screen). Microsoft's own answer is "use third-party tools / send Feedback Hub requests". Microsoft's closest first-party option is **PowerToys Power Display** (PowerToys 0.99, April 2026). It is a separate install, not part of the OS.

### Cited Findings
- On Microsoft Q&A (question posted 22 July 2026), a Microsoft moderator replied on 23 July 2026 that "Windows 11 does NOT include universal built-in brightness control for external DDC/CI-capable monitors". Users have to rely on third-party tools or the vendor's own software. DDC/CI itself works fine on Windows, as Monitorian shows; the gap is OS integration. The suggested route was a Feedback Hub request. No build that adds the feature was mentioned. — [Microsoft Q&A](https://learn.microsoft.com/en-us/answers/questions/5954664/can-we-get-ddc-ci-support-for-monitors-on-windows)
- "Windows 11 24H2 and 25H2 have shipped throughout 2025 and early 2026 without adding native external-monitor DDC support." Windows "by design only exposes the slider for the panel the OEM tagged as the internal display." — [DisplayBuddy guide](https://displaybuddy.app/monitor-brightness-control). This is a vendor source with a commercial interest, but it agrees with Microsoft Q&A.
- A June 2026 XDA article says Windows 11 itself still lacks native external-monitor brightness control, and that PowerToys fills the gap. — [XDA Developers](https://www.xda-developers.com/powertoys-monitor-brightness-windows-still-cant/)
- A Microsoft Tech Community thread is titled "Cannot Adjust External Monitor Brightness in Windows 11 Without Third-Party Software". — [Microsoft Tech Community](https://techcommunity.microsoft.com/discussions/windows11/cannot-adjust-external-monitor-brightness-in-windows-11-without-third-party-soft/4394779)
- Searches across Windows news sites found no Windows Insider build announcement that adds a native DDC/CI slider for external monitors in 2025–2026. — [search results incl. windowsforum](https://windowsforum.com/news/master-windows-brightness-quick-settings-settings-and-external-monitors.385396/)

### Inferences
- The premise of the question ("which build added it?") does not hold. No Windows build adds native DDC/CI control of external monitor brightness, contrast or input.
- Laptop brightness keys (Fn+F5/F6) go through the Windows brightness stack, which only drives the internal panel. Out of the box they will not change the P32p-20. Twinkle Tray 1.18 betas added "hotkey support for brightness keys", which suggests a third-party app can capture those keys and send them to external monitors (see Q2).

### Gaps
- There is no authoritative Microsoft statement listing every limitation (multiple monitors, contrast, input) because the feature does not exist.
- I did not verify whether specific monitors that use USB "Monitor Control" HID (not DDC/CI) get a Windows slider. This does not apply to the P32p-20.

## Q2. GUI apps: catalogue with cost, licence, features and 2026 maintenance status

### Takeaway
The current best free options are **PowerToys Power Display** (Microsoft, open source, April 2026, includes a CLI), **Twinkle Tray** (MIT, the most features of any free tray app, but releases have slowed) and **Monitorian** (MIT, actively released; hotkeys and CLI are paid Store add-ons). **ControlMyMonitor** (NirSoft freeware) is the best tool for scripting and probing VCP codes. **ClickMonitorDDC** is abandoned (v7.2, 2020) but still works. **DisplayBuddy** is a paid option ($18.99 on Windows). Lenovo's own **Display Control Center / ThinkColor** software is officially listed for the P32p-20.

### Cited Findings

**PowerToys Power Display (Microsoft, free, open source)**
- It was introduced in **PowerToys 0.99.0 (28 April 2026)**, with fixes in 0.99.1 (29 April 2026). — [GitHub release v0.99.0](https://github.com/microsoft/PowerToys/releases/tag/v0.99.0), [Neowin 0.99.1](https://www.neowin.net/news/powertoys-0991-arrives-with-improvements-for-the-new-utilities/)
- Per-monitor controls: brightness, contrast, volume, input source, rotation, color temperature and power state, "depending on what your monitor reports through DDC/CI". It also offers linked brightness across monitors, tray-icon mouse-wheel brightness, **profiles**, Light Switch integration (applies different profiles in dark mode and light mode), custom VCP name mappings (e.g. renaming `0x60` input values) and a "VCP capabilities" card. There is a customizable **activation shortcut** that opens the flyout. Docs dated 25 August 2026. — [Microsoft Learn: Power Display](https://learn.microsoft.com/en-us/windows/powertoys/power-display)
- Warnings from the docs:
  - On "a small number of monitors with malformed capability strings", the capability read "can trigger a Windows kernel bug that causes a system crash (BSOD)". After such a crash the module auto-disables and adds the monitor to an exclusion list.
  - Color temperature, power state and input source controls are off by default and ask for confirmation when enabled.
  - Input switch caveat: "After switching, software control may be unavailable until you change inputs again using the monitor's physical buttons."
  - "Max compatibility mode" probes every VCP code and can black the screen or change settings.

  — [Microsoft Learn: Power Display](https://learn.microsoft.com/en-us/windows/powertoys/power-display)
- Docks, USB-C hubs and DisplayLink adapters can block DDC commands. — [search summary of Power Display coverage](https://powertoy.net/utilities/power-display/)
- There is an open GitHub issue asking for global shortcuts that raise or lower brightness directly. Only the flyout activation shortcut exists natively. — [PowerToys issue #48784](https://github.com/microsoft/PowerToys/issues/48784)

**Twinkle Tray (free, MIT, open source)**
- Features: tray brightness sliders per monitor or linked, time-of-day and idle-based adjustments, hotkeys, contrast and other DDC/CI controls, "volume, and power state", and a command line (v1.13.0+). — [GitHub README](https://github.com/xanderfrangos/twinkle-tray), [twinkletray.com](https://twinkletray.com/)
- Install from twinkletray.com, the Microsoft Store, `winget install xanderfrangos.twinkletray`, or the community-maintained Chocolatey/Scoop packages. — [GitHub README](https://github.com/xanderfrangos/twinkle-tray)
- Release history:
  - v1.17.0 (23 August 2023): SDR brightness on HDR displays.
  - v1.17.1 (2 December 2023): **input switching from the flyout**.
  - v1.17.2 (25 December 2023): latest stable.
  - v1.18.0-beta1 (20 July 2024): ambient light sensor support.
  - v1.18.0-beta2 (13 August 2024): gamma-ramp software dimming and hotkeys for the brightness keys.

  — [GitHub releases](https://github.com/xanderfrangos/twinkle-tray/releases). The website also lists v1.17.2 as the latest version — [twinkletray.com](https://twinkletray.com/)

**Monitorian (free core, MIT, open source; paid add-on)**
- Adjusts brightness and contrast for multiple monitors, individually or all at once, and shows up to 4 monitors. It supports custom ranges and displays ambient light sensor readings. Requires Windows 7+ and .NET Framework 4.8. — [GitHub](https://github.com/emoacht/Monitorian)
- **Hotkeys, shortcut keys and command-line options are paid add-ons (subscription) in the Microsoft Store version.** — [GitHub](https://github.com/emoacht/Monitorian)
- Latest release **v4.16, 21 September 2026**, so it is actively maintained. It has no input switching. — [GitHub](https://github.com/emoacht/Monitorian)

**NirSoft ControlMyMonitor (freeware, closed source)**
- A GUI table of every VCP code the monitor reports, with read/write access and a full command line (see Q3). Latest listed version is 1.43 (adds a "Black Background" option). Earlier versions:
  - 1.42 added `/Wait`.
  - 1.41 added the **"Input Select – Use Only One Byte"** option.
  - 1.40 added "Loading Mode" (load all VCP codes or only specified ones).

  — [NirSoft](https://www.nirsoft.net/utils/control_my_monitor.html)
- Caveat: one fetch summary dated v1.43 "December 2026". That cannot be right on 2026-10-03, so the release date of v1.43 is unverified.

**ClickMonitorDDC (freeware, abandoned)**
- The original author abandoned it at **v7.2 (30 June 2020)**. It is preserved on GitHub as ClickMonitorDDC7.2. Reports say it still works on Windows 10/11. — [ddcutil.com Windows programs](https://www.ddcutil.com/windows_programs/), [search summary incl. FileHippo](https://filehippo.com/download_clickmonitorddc/), [GitHub mirror](https://github.com/chrismah/ClickMonitorDDC7.2)

**DisplayBuddy (paid, closed source)**
- $18.99 lifetime on Windows ($24.99 on Mac), with a free trial on Windows. Controls brightness, contrast, volume and **input switching**. It also has presets, multi-monitor sync with offsets, time / sunrise-sunset / battery schedules, and hotkeys. — [DisplayBuddy](https://displaybuddy.app/monitor-brightness-control)

**Other tools listed by ddcutil.com for Windows**
- EnTech softMCCS (proprietary GUI, an MCCS/VCP inspector), Monitorian, Twinkle Tray, monitorcontrol and winddcutil. — [ddcutil.com Windows programs](https://www.ddcutil.com/windows_programs/)
- Other small open-source projects exist, e.g. "Brightness-Controller-for-Desktop", which shows a Windows 11-style OSD. — [GitHub atakansariyar](https://github.com/atakansariyar/Brightness-Controller-for-Desktop)

**Lenovo first-party software**
- The **ThinkVision P32p-20 supports Lenovo Display Control Center – ThinkColor**. "Lenovo Accessories and Display Manager" (LADM) was formerly Lenovo Display Control Center. "Artery" is another LDCC-based variant for select consumer and Legion monitors. — [search summary of Lenovo support pages](https://support.lenovo.com/us/en/downloads/ds547236-lenovo-display-control-center-artery), [Artery guidebook PDF](https://smartdisplay.lenovo.com/wp-content/uploads/2021/02/Solution-Guidebook_Lenovo-Display-Control-Center-Artery.pdf)
- Lenovo Display Control Center is listed in the Microsoft Store (product 9N6PGT5DKQFQ). — [Microsoft Store](https://apps.microsoft.com/detail/9n6pgt5dkqfq?hl=en-US&gl=US). The fetch did not return the listing details, so its feature list (brightness, contrast, input, colour modes, shortcuts, partition) is **unverified**.
- The P32p-20 also has a Windows monitor driver (.inf) download for Windows 10/7. — [Lenovo Support](https://support.lenovo.com/us/en/downloads/ds544612-thinkvision-p32p-20-monitor-driver-for-microsoft-windows-10-7-32-bit-64-bit)

### Inferences
- For a P32p-20 user in October 2026, the shortlist is:
  - PowerToys Power Display: covers the most controls (including input and power) with a supported CLI.
  - Twinkle Tray: time-of-day schedules and brightness hotkeys out of the box.
  - ControlMyMonitor: scripting and finding the monitor's actual VCP values.
  - Lenovo LDCC/ThinkColor: vendor colour presets.
- Twinkle Tray is not dead: Store and winget installs still work. But no stable release has shipped since December 2023, at least according to the release page as fetched. Flag it as "slow / beta-only".
- ClickMonitorDDC should be marked abandoned.

### Gaps
- **DisplayFusion** monitor/DDC features: not researched; ran out of tool budget.
- **MagicMonitor**: not researched; no source found.
- Dell Display Manager and LG OnScreen Control only support their own vendors' monitors. This is not verified with a source here.
- Exact Twinkle Tray releases in 2025–2026: the GitHub releases fetch showed nothing after August 2024. If later releases exist, the fetch did not surface them.
- The P32p-20 spec sheet (PSREF) and user guide fetches failed (empty or 403). Exact ports, the DDC/CI OSD location and whether the monitor has speakers are **not confirmed** here.

## Q3. Scripting and command-line methods

### Takeaway
There are four practical scripting routes:
- **ControlMyMonitor** CLI: the most flexible, works on any VCP code, with no install needed.
- **PowerToys Power Display CLI**: needs PowerToys running.
- **Twinkle Tray CLI**: needs Twinkle Tray running.
- Direct **dxva2.dll** calls from PowerShell, AutoHotkey or Python (via `monitorcontrol` / `winddcutil`).

WMI brightness methods only work for internal panels.

### Cited Findings

**ControlMyMonitor command line** — [NirSoft](https://www.nirsoft.net/utils/control_my_monitor.html)

Commands:
- `ControlMyMonitor.exe /SetValue <Monitor String> <VCP Code> <Value>`. Examples:
  - `/SetValue Primary 10 70` (brightness)
  - `/SetValue "102ABC335" 12 65` (contrast by serial)
  - `/SetValue "\\.\DISPLAY3\Monitor0" 60 3` (input)
  - `/SetValue "\\.\DISPLAY2\Monitor0" D6 1` (power on); `D6 5` (power off)
- `/SetValueIfNeeded <Monitor> <VCP> <Value>` sets the value only when it differs from the current one.
- `/ChangeValue <Monitor> <VCP> <delta>` adjusts relative to the current value, e.g. `/ChangeValue Secondary 10 5`.
- `/GetValue <Monitor> <VCP>` returns the value as the process exit code (`%errorlevel%` in batch).
- `/SwitchValue <Monitor> <VCP> <v1> <v2> [...]` cycles through values, e.g. `D6 1 5` toggles power.
- `/TurnOn`, `/TurnOff`, `/SwitchOffOn <Monitor>`
- `/Wait <ms>` (v1.42+) pauses between commands.
- `/SaveConfig`, `/LoadConfig <file> <Monitor>` save and restore settings.
- Export: `/stext`, `/stab`, `/scomma`, `/shtml`, `/sxml`, `/sjson <file> <Monitor>`, and `/smonitors <file>` to list monitors.

Ways to name the monitor (Monitor String): `Primary`, `Secondary`, the device name `\\.\DISPLAY1\Monitor0`, the monitor name, the serial number, or the Short Monitor ID (e.g. `GSM59A4`).

Common VCP codes: 10 brightness, 12 contrast, 60 input select, 04 factory reset, D6 power.

**PowerToys Power Display CLI** (`PowerToys.PowerDisplay.Cli.exe`, needs Power Display running) — [Microsoft Learn](https://learn.microsoft.com/en-us/windows/powertoys/power-display)
- `list`
- `capabilities --monitor-number 1 [--setting input-source|color-temperature|power-state]`
- `get --monitor-number 1 [--setting brightness|contrast|volume|color-temperature|input-source|power-state|orientation]`
- `set --monitor-number 1,2 --brightness 70`: one value per command. Other options are `--contrast`, `--volume`, `--color-temperature 0xNN`, `--input-source 0xNN`, `--power-state 0xNN` (needs `--confirm-power-off` to blank or sleep the display) and `--orientation 0|90|180|270`.
- `up` / `down --brightness [--step N]`
- `profiles`
- `apply-profile <id>`
- Monitor selection: `-n` / `--monitor-number` (1-based) or `-i` / `--monitor-id`. Add `--quiet` to suppress warnings.

**Twinkle Tray CLI** (v1.13.0+; the app must be running) — [GitHub README](https://github.com/xanderfrangos/twinkle-tray)
- `--List`
- Monitor selection: `--MonitorNum=#`, `--MonitorID="..."` or `--All`
- Brightness: `--Set=<pct>` or `--Offset=<±n>`
- Any VCP code: `--VCP="code:value"`
- `--Overlay` and `--Panel` show UI feedback.

**Monitorian CLI** (paid Store add-on) — [GitHub](https://github.com/emoacht/Monitorian)
- `/get`, `/get all`, `/get [DeviceInstanceID]`
- `/set [n]`, `/set all [n]`, `/set [DeviceInstanceID] [n]`
- Add `contrast` after `/get` or `/set` to work on contrast instead.
- Values can be absolute (`20`) or relative (`+10` / `-10`).
- It has no input switching.

**Win32 Monitor Configuration API (dxva2.dll)** — [Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/monitor/using-the-high-level-monitor-configuration-functions)
- How to get a handle: `EnumDisplayMonitors` or `MonitorFromWindow` returns an HMONITOR, which "can be associated with more than one physical monitor". Call `GetPhysicalMonitorsFromHMONITOR` to get physical monitor handles.
- High-level functions: `GetMonitorCapabilities`, brightness and contrast "continuous" settings, `RestoreMonitorFactoryDefaults`.
- Low-level functions (by VCP code) are used for anything else.
- Caveats:
  - Getting each setting "takes about 40 milliseconds".
  - Low-level VCP writes can change or disable high-level settings in vendor-specific ways (`ERROR_DISABLED_MONITOR_SETTING`).
  - Units are arbitrary and vendor-specific.

**AutoHotkey**
- The common pattern is `getMonitorHandle()` via the Win32 API, then `DllCall("dxva2\SetVCPFeature", ...)` with VCP `0x60` for input switching, and `GetVCPFeatureAndVCPFeatureReply` to read it back. — [AHK forum: Change Monitor Input with AHK](https://www.autohotkey.com/boards/viewtopic.php?t=44677), [AHK forum: Switch Second Monitor Input Source](https://www.autohotkey.com/boards/viewtopic.php?t=56202), [example script](https://github.com/srnln/AutoHotKey/blob/main/MonitorChange.AHK)
- A forum thread reports that a Windows 10 1903 update broke an input-switch script. Behaviour can regress with OS or driver updates. — [AHK forum](https://www.autohotkey.com/boards/viewtopic.php?t=66292)

**Python `monitorcontrol`** — [docs](https://newam.github.io/monitorcontrol/), [API](https://newam.github.io/monitorcontrol/api.html)
- Install with `py -3 -m pip install monitorcontrol`. Docs say it is "tested with Windows 10"; the version shown is 4.2.0.
- API:
  - `get_monitors()`, used with a context manager (`with monitor:`)
  - `get_/set_luminance`, `get_/set_contrast`, `get_/set_volume`
  - `get_/set_input_source`, `get_/set_power_mode` (PowerMode on=1, standby=2, suspend=3, off_soft=4, off_hard=5)
  - `get_/set_color_preset`, `get_/set_audio_mute_mode`
  - `get_vcp_capabilities()`

**winddcutil**
- A Windows clone of Linux ddcutil with the commands `detect`, `capabilities`, `getvcp`, `setvcp`.
- v2.0.0 (September 2023) was rewritten in Python on top of `monitorcontrol` and ships a standalone `winddcutil.exe`.
- Maintenance is light: 24 commits, and the latest release is from 2023.

— [GitHub scottaxcell/winddcutil](https://github.com/scottaxcell/winddcutil)

### Inferences
- PowerShell has no built-in cmdlet for DDC/CI. It has to P/Invoke dxva2.dll (`Add-Type` with `GetPhysicalMonitorsFromHMONITOR`, `SetVCPFeature`, `GetVCPFeatureAndVCPFeatureReply`, `CapabilitiesRequestAndCapabilitiesReply`, `DestroyPhysicalMonitors`), following the Microsoft Learn handle model above. Calling `ControlMyMonitor.exe` from PowerShell is simpler.
- For a hotkey that "just works", ControlMyMonitor wins: it is a portable exe, needs no running service and accepts any VCP code. The PowerToys and Twinkle Tray CLIs depend on their tray apps running.

### Gaps
- **WMI `WmiMonitorBrightnessMethods`**: I fetched no source this session. Its documented behaviour, controlling only internal panels through the ACPI/display-driver brightness interface, is widely known but **unverified here**. It should not be used for the P32p-20.
- **`ddcset` / `ddc-hi` (Rust)**: not researched in this session; maintenance status unknown.
- The exact Microsoft Learn reference pages for `SetVCPFeature`, `GetVCPFeatureAndVCPFeatureReply` and `CapabilitiesRequestAndCapabilitiesReply` were not individually fetched. Function names follow the Win32 API.

## Q4. Input switching via VCP 0x60

### Takeaway
VCP **0x60** is Input Select. Common MCCS values are **0x0F (15) = DisplayPort-1, 0x10 (16) = DisplayPort-2, 0x11 (17) = HDMI-1, 0x12 (18) = HDMI-2**. USB-C values are vendor-specific (0x1B is commonly used but is **not** in the base MCCS table and is unverified for the P32p-20). Always read the monitor's own capability string to get the real values. The big pitfall: once the monitor switches away from the laptop's input, the laptop usually can't talk to it any more, so switching back must be done from the other source device or with the monitor's buttons.

### Cited Findings
- Commands for each tool:
  - ControlMyMonitor: `/SetValue <mon> 60 <value>`, plus the "Input Select – Use Only One Byte" option (v1.41) for monitors that use only the low byte. — [NirSoft](https://www.nirsoft.net/utils/control_my_monitor.html)
  - PowerToys: `--input-source 0xNN`, using the values reported by `capabilities --setting input-source`. — [Microsoft Learn](https://learn.microsoft.com/en-us/windows/powertoys/power-display)
  - Twinkle Tray: `--VCP="0x60:<value>"`, plus flyout input switching since v1.17.1. — [GitHub README](https://github.com/xanderfrangos/twinkle-tray), [releases](https://github.com/xanderfrangos/twinkle-tray/releases)
  - monitorcontrol: `set_input_source(...)`. — [API](https://newam.github.io/monitorcontrol/api.html)
- AutoHotkey users report HDMI1 = 17 (0x11), HDMI2 = 18 (0x12), DP = 15 (0x0F). Other monitors use DVI = 3 and HDMI = 4, so "values can vary by monitor model". — [AHK forum threads (search summary)](https://www.autohotkey.com/boards/viewtopic.php?t=44677)
- Pitfall, as stated by Microsoft: "If the selected input has no signal, the screen will go black… After switching, software control may be unavailable until you change inputs again using the monitor's physical buttons. Not every monitor reliably exposes all of its inputs over DDC/CI." — [Microsoft Learn: Power Display](https://learn.microsoft.com/en-us/windows/powertoys/power-display)

### Inferences
- For a laptop and a second source (e.g. a desktop on DP and the laptop on USB-C), put a "switch to my input" script on **each** source machine. A "toggle" script on one machine can switch away but cannot switch back.
- `ControlMyMonitor /SwitchValue <mon> 60 <a> <b>` only helps when the DDC channel stays reachable on both inputs. That is rare unless both inputs come from the same PC.
- Run `ControlMyMonitor` (GUI or `/scomma`) or `PowerDisplay.Cli capabilities --setting input-source` once on the P32p-20 to get its exact 0x60 value list, especially for its USB-C input.

### Gaps
- The exact 0x60 value table for the P32p-20 (HDMI / DP / USB-C) was not found. The Lenovo spec and user-guide fetches failed.
- MCCS 2.2 spec text for values above 0x12 and the origin of the 0x1B "USB-C" convention: no primary source fetched.

## Q5. Binding to hotkeys, and remote/phone control

### Takeaway
Hotkey options:
- Twinkle Tray hotkeys (brightness, including the 1.18-beta brightness-key capture).
- The PowerToys Power Display activation shortcut, which opens the flyout only; direct brightness hotkeys are still a feature request.
- Monitorian paid hotkeys.
- DisplayBuddy hotkeys.
- Arbitrary hotkeys via AutoHotkey calling dxva2 or ControlMyMonitor.

I found no well-sourced phone or remote-control apps for DDC/CI on Windows.

### Cited Findings
- Twinkle Tray: "Bind hotkeys to adjust the brightness of specific or all displays". — [twinkletray.com](https://twinkletray.com/). Hotkey support for brightness keys arrived in v1.18.0-beta2. — [releases](https://github.com/xanderfrangos/twinkle-tray/releases)
- PowerToys Power Display has a customizable activation shortcut for the flyout. — [Microsoft Learn](https://learn.microsoft.com/en-us/windows/powertoys/power-display). Global increase/decrease brightness shortcuts are requested in [issue #48784](https://github.com/microsoft/PowerToys/issues/48784).
- Monitorian hotkeys and shortcut keys are paid add-ons. — [GitHub](https://github.com/emoacht/Monitorian)
- DisplayBuddy has hotkeys on Windows and Wi-Fi control for Samsung/LG smart monitors only. — [DisplayBuddy](https://displaybuddy.app/monitor-brightness-control)
- AutoHotkey scripts can call `dxva2\SetVCPFeature` directly from a hotkey. — [AHK forum](https://www.autohotkey.com/boards/viewtopic.php?t=44677)

### Inferences
- Other low-friction hotkey paths:
  - A `.lnk` shortcut to `ControlMyMonitor.exe /SetValue ...` with a "Shortcut key" set in its Properties. This only works from the Start menu or desktop.
  - PowerToys Keyboard Manager's remap-to-run-program feature pointing at the same command line.
  - A Stream Deck "open/run" action.
  - The PowerToys Power Display CLI (`up` / `down --brightness`) bound the same way.
- Remote or phone control: no dedicated app was found. A workaround is to trigger the CLI commands remotely (e.g. via a home-automation agent, SSH or Remote Desktop into the laptop).

### Gaps
- No source fetched on whether the PowerToys Keyboard Manager "run program" feature can launch a CLI with arguments, or on the `.lnk` hotkey mechanism. Both are inferred from general knowledge.
- Stream Deck plugins for DDC/CI and remote/phone apps: not researched (tool budget).
- DisplayFusion monitor control and hotkey features: not researched.
