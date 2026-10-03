# ThinkVision Control — design

Date: 2026-10-03
Status: approved in conversation, awaiting written-spec review
Background research: [reports/ThinkVision P32p20 Windows control.md](../../../reports/ThinkVision%20P32p20%20Windows%20control.md)

## 1. Purpose

The Lenovo ThinkVision P32p-20 attached to the user's Windows 11 laptop has failing OSD buttons, and the buttons are hard to use in the dark. The goal is a small Windows tray app that does everything the OSD can do over DDC/CI, with global hotkeys and on-screen feedback, so the buttons are rarely needed.

**Success means:** brightness, contrast, input, colour mode, RGB gains, aspect ratio, OSD language and power can all be changed from the tray panel or a hotkey without touching the monitor. Lenovo's undocumented settings can be identified and then added to the panel.

**Constraints and context**
- The laptop is on HDMI now. A desktop PC will be connected soon on DisplayPort or USB-C, and the same exe must run there.
- DDC/CI is confirmed working over the laptop's HDMI link (read-only probe, 2026-10-03). No write has been tested yet.
- Toolchain: C# on .NET 10 (LTS), WPF, Windows 11. The .NET SDK must be installed with `winget install Microsoft.DotNet.SDK.10`.
- No admin rights are required at run time.

## 2. Ground truth: the monitor's capabilities

Captured from the user's monitor (EDID `LEN62A2`, MCCS 2.2):

```
(prot(monitor)type(LCD)model(ThinkVision P32p-20)cmds(01 02 03 07 0C E3 F3)
vcp(02 04(4C 42 31 2E 32) 05 08 10 12 14(01 05 06 08 0B 0E 0F 12) 16 18 1A 52
60(11 0F 31) 86(02 05) AC AE B2 B6 C6(02 01 C3 00 A0 CA 00 F8 00 EA 00 00 DC 00 46 00 00)
C8 C9 CA(01 02) CC(02 03 04 05 06 09 0A 0D) D6(01 04 05) DF E0(00 01 02)
EA(00 01) EB(00 01) EC(00 01) F6(07) F7(02 04 07 08 0D) F8(02 04 07 08 0D)
FA(10 12 EA 14))mswhql(1)asset_eep(40)mccs_ver(2.2))
```

| Control | VCP | Values | Kind | In v1 panel |
|---|---|---|---|---|
| Brightness | 0x10 | 0–100 | range | yes |
| Contrast | 0x12 | 0–100 | range | yes |
| Colour mode | 0x14 | 01 05 06 08 0B 0E 0F 12 (names mapped during the hardware check; 01 = sRGB, 0B = User/Custom by MCCS convention) | enum | yes |
| Red / green / blue gain | 0x16 / 0x18 / 0x1A | 0–100 | range | yes, shown when colour mode is Custom |
| Input source | 0x60 | 0x11 HDMI, 0x0F DP, 0x31 USB-C | enum | yes |
| Aspect ratio | 0x86 | 02, 05 (names mapped during the hardware check) | enum | Advanced |
| OSD language | 0xCC | 02 03 04 05 06 09 0A 0D (MCCS language table) | enum | Advanced |
| Power mode | 0xD6 | 01 on, 04 standby/off, 05 off | enum, guarded | Advanced |
| Factory reset / brightness+contrast reset / colour reset | 0x04 / 0x05 / 0x08 | write 1 | action, guarded | Advanced |
| OSD enable | 0xCA | — | **never written** | no |
| Vendor codes | E0 EA EB EC F6 F7 F8 FA | as listed | unknown, read-only until named | via finder |

## 3. Architecture

One process, `ThinkVisionControl.exe`, which lives in the tray and optionally starts with Windows. Six units, each with one job:

### 3.1 Monitor link (`Ddc/`)
- Wraps `dxva2.dll`/`user32.dll`: `EnumDisplayMonitors`, `GetPhysicalMonitorsFromHMONITOR`, `GetCapabilitiesStringLength`, `CapabilitiesRequestAndCapabilitiesReply`, `GetVCPFeatureAndVCPFeatureReply`, `SetVCPFeature`, `DestroyPhysicalMonitors`.
- Finds the target monitor by EDID ID `LEN62A2`, matched through the display device ID (`EnumDisplayDevices` with `EDD_GET_DEVICE_INTERFACE_NAME`), not by DISPLAYn number.
- Exposed as an interface `IMonitorLink` (`Connect`, `GetCapabilities`, `Get(code)`, `Set(code, value)`, `IsConnected`) so that everything above it can be tested against a fake.
- **All DDC traffic goes through one serial worker queue**, with at least 50 ms between commands and up to 3 retries per command.
- Re-enumerates on `WM_DISPLAYCHANGE`, on resume from sleep (`SystemEvents.PowerModeChanged`), and when a command fails.

### 3.2 Monitor profile (`Profile/`)
- `CapabilitiesParser` turns the capability string into a code → allowed-values map.
- `p32p20.profile.json` (embedded in the exe as a resource; finder results and other user edits are saved as an overlay in `%AppData%\ThinkVisionControl\profile.user.json`) holds the display name, kind, value labels and safety class for each code.
- When the profile and the capabilities differ, the capabilities decide which codes and values may be written; the profile only supplies names.

### 3.3 Tray panel (`Ui/`)
- WPF dark flyout anchored near the tray, closing when it loses focus.
- Opens immediately with cached values, then refreshes in the background.
- Sections, top to bottom:
  1. Brightness and contrast sliders.
  2. Input buttons (active one highlighted).
  3. Colour mode list, plus the RGB sliders when the mode is Custom.
  4. Day and Night preset buttons, each with "save current values".
  5. Advanced, collapsed: aspect ratio, OSD language, power, the three resets, restore backup, start with Windows, hotkey editor.
  6. Hidden-settings finder.
- Slider drags are throttled: only the latest value is sent, at about 10 per second.
- Tray icon states: normal; grey when the monitor is not responding (tooltip: "Monitor not responding — DDC/CI may be off (hold Exit for 10 s)").

### 3.4 Hotkeys and on-screen feedback (`Hotkeys/`)
- Global hotkeys via `RegisterHotKey`. When a registration fails (the key is taken by another app), the app shows that in the hotkey editor instead of failing silently.
- Default bindings:

| Action | Keys |
|---|---|
| Brightness +10 / −10 | Ctrl+Alt+PageUp / PageDown |
| Contrast +5 / −5 | Ctrl+Alt+Shift+PageUp / PageDown |
| Day / Night preset | Ctrl+Alt+Home / End |
| Input HDMI / DP / USB-C | Ctrl+Alt+F1 / F2 / F3 |
| Switch monitor to this machine | Ctrl+Alt+M |
| Open panel | Ctrl+Alt+O |

- Every hotkey action shows a small dark, click-through box near the bottom-centre of the screen for about 1 second, e.g. "Brightness 40" or "Input → DisplayPort".

### 3.5 Settings (`Settings/`)
- `%AppData%\ThinkVisionControl\settings.json` holds:
  - hotkeys;
  - the presets (Day: brightness 60, contrast 70, current colour mode; Night: brightness 0, contrast 70, Low Blue Light once identified);
  - **this machine's input** (HDMI on the laptop; DP or USB-C on the PC), which drives "switch to me";
  - the start-with-Windows flag (an `HKCU\...\Run` value).
- Missing or corrupt file → defaults are used, and the bad file is renamed `.bak`.

### 3.6 Hidden-settings finder (`Finder/`)
- Read-only flow:
  1. Snapshot every vendor code (E0 EA EB EC F6 F7 F8 FA).
  2. Prompt the user to change one OSD setting with the buttons.
  3. Snapshot again and show which codes changed.
  4. The user names the setting and its values; the result is saved to the user profile.
- A code becomes writable from the panel only after it is named this way. It then appears in Advanced as an enum.

## 4. Data flow

```
Hotkey / panel action
  → ControlService (validates against capabilities + safety rules)
    → DDC worker queue (serial, spaced, retried)
      → IMonitorLink (dxva2)
  ← result → value cache → panel + on-screen box
```

`ControlService` is the only path to writes. It owns the value cache and the safety rules.

## 5. Safety rules (enforced in ControlService, unit-tested)

1. Never write `0xCA`.
2. Never write a code that is not in the capability string, or a value outside its allowed list or 0–max range. Range values are clamped.
3. Never write a vendor code until the finder has named it.
4. Factory reset (`0x04`) needs confirmation, with a warning that it may switch DDC/CI off.
5. Before any reset, all readable controls are backed up automatically to `%AppData%\ThinkVisionControl\backup-<timestamp>.json`. "Restore backup" re-applies the newest backup.
6. Power off/standby (`0xD6` = 04/05) needs confirmation, with a warning that the power button may be needed to wake the monitor.

## 6. Input switching across two machines

- Each machine's settings say which input it is on. "Switch to me" sets `0x60` to that input.
- Switching from the machine that is currently on screen always works.
- Switching from the machine that is *not* on screen depends on whether the P32p-20 still accepts DDC/CI on an inactive input. This is untested and gets checked once the PC is connected.
- If the inactive machine cannot reach the monitor, its "switch to me" fails with the message: "Monitor not reachable from this input — switch from the other machine or press Exit."
- The panel's input buttons work from the active machine either way.

## 7. Error handling

- DDC command failure → retry (3×) → mark the monitor not responding → grey tray icon → background reconnect every 5 s.
- Display change or resume → reconnect, then reload the cache.
- Errors and DDC failures are logged to `%AppData%\ThinkVisionControl\log.txt` (rolling, 1 MB cap).
- No modal error dialogs from hotkeys; the on-screen box shows "Monitor not responding" instead.

## 8. Testing

**Automated (xUnit, against `FakeMonitorLink`):**
- capabilities parser, using the real string from section 2;
- profile and capabilities merge;
- every safety rule in section 5;
- slider throttling;
- settings load, save and corrupt-file recovery;
- hotkey string parsing;
- preset apply;
- backup and restore;
- finder diff logic.

**Hardware checks (manual, with the user watching):**
1. Write test: brightness 0 → 20 → 0.
2. Read and write each confirmed control.
3. Map the 0x14 colour-mode values and the 0x86 aspect values to OSD names, and record them in the profile.
4. Sleep/resume and re-plug reconnection.
5. Later, when the PC is connected: the inactive-input reachability test from section 6.

## 9. Packaging and repository

- `dotnet publish` as a single-file, self-contained `win-x64` exe (about 70 MB). Copy it to the PC; nothing to install.
- Layout:
  - `src/ThinkVisionControl/` — the app.
  - `tests/ThinkVisionControl.Tests/` — the automated tests.
  - `docs/` — specs and plans.
  - Existing `reports/` and `research_notes/` stay as they are.
- Git repository, initialised now. Commits carry no Claude/Anthropic attribution trailers.

## 10. Out of scope for v1

- A command-line interface.
- Support for monitors other than the P32p-20.
- Scheduled or time-of-day brightness changes.
- Stream Deck or phone control.
- Firmware updates.
- The USB upstream (KVM) switch, unless the finder shows it is exposed through a vendor code.
