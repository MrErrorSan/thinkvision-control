# ThinkVision Control — Windows 11 redesign

Date: 2026-10-05
Status: draft for review
Builds on: [2026-10-03 design](2026-10-03-thinkvision-control-design.md) (monitor link, profile, safety rules, settings and hotkeys are unchanged unless stated here)
Evidence: [reports/Windows 11 design for WPF apps.md](../../../reports/Windows%2011%20design%20for%20WPF%20apps.md) · Mockups: [docs/mockups/thinkvision-control-ui.html](../../mockups/thinkvision-control-ui.html), variant **A**

## 1. Decisions (user, 2026-10-05)

| Topic | Decision |
|---|---|
| Layout | **Variant A — Settings app**: left navigation pane, settings cards, 1008×720 default window |
| Sliders | **Live while dragging**. The jitter and missed changes the user has seen must be root-caused on hardware first (§6) |
| Quick access | **Jump list** (works with the app closed) + **tray flyout** (background mode only) |
| Default run mode | **Exit on close**; Run in background stays opt-in |
| Theme technology | Built-in WPF Fluent theme (`ThemeMode="System"`); **no WPF-UI** (+7 MB, +160 ms per launch) |
| Quick Settings | Out of scope: Windows has no extension API |

## 2. Theme and visual foundation

- `Application.ThemeMode="System"` in `App.xaml` (XAML only, so no `WPF0001` suppression is needed). This gives Mica, a themed title bar, system accent and Fluent-styled stock controls, and follows light/dark live.
- `Ui/Theme.xaml` keeps only:
  - type-ramp text styles: Caption 12/16, Body 14/20, BodyStrong 14/20 Semibold, BodyLarge 18/24, Subtitle 20/28, Title 28/36, using Segoe UI Variable;
  - the hand-built controls (§3).

  Every hard-coded brush is replaced with Fluent resource keys through `DynamicResource`. Implicit styles that override Fluent are removed.
- `WindowsThemeManager` shrinks to one job: telling the tray icon whether the taskbar is light or dark. Fluent now owns the palette and the title bars, so the manual `DWMWA_USE_IMMERSIVE_DARK_MODE` call goes.
- Icons use `FontFamily="Segoe Fluent Icons, Segoe MDL2 Assets"`.
- The OSD stays an unthemed `AllowsTransparency` window with local styles: 8 px radius, 1 px stroke, BodyLarge value text, 167 ms fade.
- Values: 4 px grid, 4 px control radius, 4 px card radius (Toolkit SettingsCard), 8 px window/dialog/flyout radius, card gap 4 px, section header margin `1,30,0,6`, page padding 36 px, column max width 1000 px.

## 3. Hand-built controls (`Ui/Controls/`)

The Fluent theme doesn't include these, so each is a small custom control or style:

| Control | Shape |
|---|---|
| `SettingsCard` | Border with card fill and stroke, 4 px radius, 16 px padding, min height 68 px. Slots: Icon (20 px glyph), Header (Body), Description (Caption, secondary), Content (right-aligned). Disabled → disabled text tokens. Optional `IsClickEnabled` + chevron |
| `SettingsExpander` | `Expander` template built from a SettingsCard header; children indented 48 px |
| `ToggleSwitch` | `ToggleButton` style: 40×20 track, 12 px knob, "On"/"Off" label on the left |
| `InfoBar` | Severity glyph (Informational / Success / Warning / Error), Title (BodyStrong), Message, optional action button, optional close button |
| `ContentDialog` | In-window overlay: smoke scrim plus a centred 8 px card with a Title (Subtitle) and Primary / Secondary / Close buttons. The default button is configurable. Esc = Close. Focus trapped. It replaces `MessageBox` |
| `NavigationPane` | Styled `ListBox` with an accent selection pill, a menu button, items Display and Monitor settings, and App settings pinned at the bottom. Below 1008 px wide it collapses to icons; the menu button opens it as an overlay. F6 moves focus between pane and content |

## 4. Pages

The pages match mockup variant A. Every change applies immediately; there are no confirm buttons except where §5 requires a dialog.

| Page / section | Setting | Control |
|---|---|---|
| Display / header | Monitor name and status (icon + text, not a coloured dot) | Display header |
| Display / Brightness and contrast | 0x10, 0x12 | SettingsCard + Slider, step 1, value text beside it. **No Apply button** |
| Display / Colour | Colour mode 0x14 | ComboBox |
| Display / Colour | Red / Green / Blue gain | SettingsExpander "Custom colour"; description "Available in User / Custom colour mode" |
| Display / Input | 0x60 | RadioButtons (HDMI, DisplayPort, USB-C). "This PC" marks this machine's input |
| Display / Presets | Day, Night | One card each: Apply button + "More" menu → "Save current settings as Day/Night" |
| Monitor settings / Picture | Over Drive 0xE0 · Dynamic Contrast 0xEA · Aspect ratio 0x86 | ComboBox · ToggleSwitch · ComboBox |
| Monitor settings / Ports | USB Charging 0xEC | ToggleSwitch |
| Monitor settings / On-screen menu | Menu language 0xCC | ComboBox |
| Monitor settings / Power | 0xD6 | Card with Standby and Turn off buttons (dialog) |
| Monitor settings / Reset and restore | 0x04, 0x05, 0x08, restore backup | SettingsExpander with one card per action (dialogs for the resets) |
| Monitor settings / Tools | Find hidden settings | Clickable card with chevron → FinderWindow (restyled) |
| App settings / Startup | Run in background, with child Start with Windows | SettingsExpander + ToggleSwitches |
| App settings / This PC | This PC's input | ComboBox |
| App settings / Keyboard shortcuts | 11 actions | Card per action: keycaps + edit button → shortcut dialog (live capture; Save disabled until valid; Reset, Clear). A failed registration shows "Another app is using this shortcut" in the card description |
| App settings / About | Version, firmware | SettingsExpander |

## 5. Dialogs

Each has a question as the title, a verb on the action button, and Cancel as the default.

| Trigger | Title / message | Buttons |
|---|---|---|
| Factory reset | "Reset monitor to factory settings?" / "This restores every on-screen-menu setting, and it can turn off DDC/CI on some Lenovo monitors. We save a backup first." | **Reset** · Cancel |
| Reset brightness and contrast, Reset colour | "Reset brightness and contrast?" / "Reset colour settings?" | **Reset** · Cancel |
| Power off / standby | "Turn off ThinkVision P32p-20?" / "You might need the monitor's power button to turn it back on." | **Turn off** (or **Standby**) · Cancel |
| Switching input away from This PC's input | "Switch to DisplayPort?" / "You won't be able to control the monitor from this PC until you switch back." | **Switch** · Cancel |

## 6. Live sliders and the jitter (root cause first)

**Observed by the user:** live dragging sometimes doesn't apply and jitters.

**Hypothesis to confirm on hardware.** Every range write currently triggers a read-back. During a drag, those read-backs:
- arrive late, because each write is followed by a 400 ms settle period and then the read;
- raise `ValueChanged`, which moves the slider while the user is still dragging it;
- report a "mismatch" whenever they read a value the monitor hasn't finished applying.

**Investigation step:**
- Log every write and read with timestamps during a drag on the real monitor.
- Confirm or reject the hypothesis before changing behaviour.
- If the evidence points elsewhere, change this section before implementing it.

**Target behaviour, assuming the hypothesis holds:**
- **While dragging:** writes are latest-value-wins and coalesced. At most one write is in flight per control, and there is **no read-back** during the drag. Incoming value updates for a control being dragged are ignored.
- **On release (and on keyboard steps after a short pause):** one final write of the released value, then one read-back once the 400 ms settle period has passed.
- **If the monitor kept a different value:** the slider snaps to it, and the card description says why (for example "Your monitor keeps brightness fixed in sRGB mode").
- **Hotkey steps:** unchanged (single write + verify).

## 7. States: the full UI is always visible

`ControlService` exposes `Controls` built from the embedded profile **plus the last known capabilities**, cached as `%AppData%\ThinkVisionControl\capabilities.txt`. With no cache yet, the profile's value labels give the allowed values. This lets every card render before the monitor answers.

| State | InfoBar (one, under the page header, not closable) | Monitor controls | App settings |
|---|---|---|---|
| Connecting | After about 1 s: Informational "Looking for your monitor…" with an indeterminate progress bar | Disabled; sliders show "—" | Enabled |
| Not found | Informational "ThinkVision P32p-20 isn't connected" / "Connect it and we'll find it automatically" / **Check again** | Disabled | Enabled |
| Not responding | Error "Your monitor isn't responding" / "DDC/CI might be off. Hold the monitor's Exit key for 10 seconds, then try again" / **Try again** | Disabled | Enabled |
| Connected | None | Enabled | Enabled |

A new state is held for about 1 s before the InfoBar changes, so it doesn't flash. The tray icon gets a disconnected or error overlay and a tooltip with the state. Tray-menu items that can't run (presets while disconnected) are removed, not greyed out.

## 8. Quick access

### Jump list and command line
- WPF `JumpList` tasks: **Day preset**, **Night preset**, **Switch to this PC**. Each runs `ThinkVisionControl.exe --preset day|night` or `--input this`.
- Command-line verbs: `--preset day|night`, `--input hdmi|dp|usbc|this`, `--step brightness=+5` (also usable from Task Scheduler or Stream Deck).
- **If an instance is running:** the arguments go to it over a named pipe (`ThinkVisionControl.Commands`). This replaces the show-only event; a launch with no arguments sends "show".
- **If none is running:** the app runs headless. It connects, applies the command, shows the OSD for about 1 s and exits **without creating the main window**.

### Tray flyout (background mode only)
- Left-clicking the tray icon opens a Quick Settings-style flyout:
  - about 360 px wide, 8 px radius, acrylic (`DWMSBT_TRANSIENTWINDOW`), anchored with `Shell_NotifyIconGetRect` / `CalculatePopupWindowPosition`;
  - contents: brightness and contrast sliders, input tiles, colour mode, Day/Night, a status line, and "All settings";
  - closes on deactivate or Esc.
- The flyout uses the same `InfoBar` and disabled states as the window. Switching away from This PC's input asks inline in the flyout, not in a dialog.
- Right-clicking the tray icon opens a menu: **Open ThinkVision Control** (bold), separator, Day preset, Night preset, separator, Exit.
- The tray icon is monochrome and adapts to the taskbar theme. It is re-added on `TaskbarCreated`.

## 9. Lightness

1. **Default `BackgroundMode = false`.** Today's default is `true`; existing settings files keep their value.
2. **A login launch (`--startup`) in background mode never creates `PanelWindow`.** The window is created on first show.
3. **In background mode, closing the window closes it.** The instance is released and recreated on the next show.
4. **The 5 s reconnect timer is replaced with event-driven detection:**
   - `RegisterDeviceNotification` for the monitor interface GUID `{E6F07B5F-EE97-4a90-B076-33F57BF4EAA7}`, plus display-change and resume events;
   - each event triggers a bounded retry burst (0.5, 1, 2, 4, 8 s, then stop);
   - **Check again** / **Try again** trigger a burst manually.
5. **EcoQoS** (`SetProcessInformation` ProcessPowerThrottling, EXECUTION_SPEED) while no window or flyout is open; cleared while one is.
6. **Publish settings:**
   - single-file, self-contained, **no** `EnableCompressionInSingleFile`;
   - `PublishReadyToRun`;
   - `SatelliteResourceLanguages=en`.
7. **Software rendering** (`RenderMode.SoftwareOnly`) is evaluated on this machine. Adopt it only if Mica and text still render correctly.

## 10. Testing

- **Core unit tests:**
  - `ControlService` states with no capabilities cache;
  - the capabilities cache round-trip;
  - the drag write coalescer: latest wins, no read during drag, one verify on release;
  - command-line parsing;
  - `BackgroundMode` default `false` for new installs;
  - retry-burst scheduling, with a fake clock.
- **UI (manual, against the mockup):**
  - every page in light and dark;
  - narrow width;
  - the four states with the monitor unplugged, unplugged then plugged in, and with DDC/CI off;
  - dialogs;
  - flyout anchoring;
  - jump list with the app closed and with it running.
- **Resources:** measure private memory for a login launch in background mode (target about 12 MB), with the window open, and after the window is closed.

## 11. Out of scope

- WPF-UI or any other UI library.
- Quick Settings, Widgets, toast notifications.
- An in-app theme picker (the app follows the system).
- Replacing the WinForms `NotifyIcon` (measured cost below 1 MB private).
