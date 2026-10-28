# Parity inventory: ThinkVision Control 0.0.4 (C#) → Rust + Slint

The checklist for the Rust port. It covers every user-visible feature and behaviour of 0.0.4, with the source location for each. The version comes from `Directory.Build.props:4`.

**Path prefixes used below:**
- `C/` = `src/ThinkVisionControl.Core/`
- `W/` = `src/ThinkVisionControl/` (WPF, Windows)
- `A/` = `src/ThinkVisionControl.Avalonia/` (macOS, Linux, Windows preview)
- `DW/` = `src/ThinkVisionControl.Ddc.Windows/`
- `DL/` = `src/ThinkVisionControl.Ddc.Linux/`
- `DM/` = `src/ThinkVisionControl.Ddc.MacOS/`
- `T/` = `tests/ThinkVisionControl.Tests/`

Line numbers are for the 0.0.4 tree. Where WPF and Avalonia behave the same, the WPF location is cited and the Avalonia twin is `A/ViewModels/MainViewModel.cs` (the `PanelViewModel` port), `A/ViewModels/ControlItems.cs` or `A/App.axaml.cs`.

**Power (VCP 0xD6)** is deliberately dropped. It appears only in §11, and the other sections skip it.

---

## 1. Screens and controls

### 1.0 Global constants the UI relies on
- The monitor name `"ThinkVision P32p-20"` is hard-coded (`W/Ui/PanelViewModel.cs:53`).
- Display-page codes: `0x10, 0x12, 0x14, 0x16, 0x18, 0x1A, 0x60` (`:55`). These are refreshed on show and polled.
- Colour-mode display order: `0x0B sRGB, 0x01 DCI-P3, 0x0F BT.709, 0x06 Neutral, 0x0E Reddish, 0x05 Bluish, 0x08 Low Blue Light, 0x12 User / Custom`. The list is filtered by the capability list (`:56`, `:324-325`).
- Input options are always in the order HDMI `0x11`, DisplayPort `0x0F`, USB-C `0x31`, whatever order the capabilities use (`:329-331`).
- Card kind is chosen from the definition (`:307-319`):
  - Range → slider.
  - Action → button.
  - Enum → toggle when its labels are exactly `{0:"Off",1:"On"}` and every allowed value is ≤1 (`:334-336`). Otherwise a combo box.
- **Visible card** = `Section != Hidden` and the safety class is not NeverWrite or ReadOnlyUntilNamed (`:304-305`).
- Enum values come from the capability list for the code. Without one, they come from the profile labels (`:327`). An unlabelled value shows as `0xNN` (`:820`).
- Firmware is the value list of 0x04 decoded as ASCII, when it has ≥2 bytes and all are printable. On the P32p-20 it reads "LB1.2" (`:161-164`).

### 1.1 Main window shell
- **Window:**
  - Title "ThinkVision Control", 1008×720, centred on screen (`W/Ui/PanelWindow.xaml:5-8`).
  - Minimum size: WPF 500×500; Avalonia 500×400 (`A/MainWindow.axaml:10`).
- **Navigation:**
  - Display, Monitor settings, and App settings (in the footer) (`W/Ui/PanelWindow.xaml:24-28`).
  - Pages are built lazily and cached per window (`W/Ui/PanelWindow.xaml.cs:82-92`).
  - The window opens on Display (`:38`).
- **Pane:**
  - Expanded at ≥1008 px; icons only below that, with a menu button that opens the pane over the content (`:95-117`).
  - WPF also has a collapse toggle for wide windows.
  - Esc closes the overlay pane; F6 moves focus between the pane and the page (`:119-135`).
  - Avalonia uses `FANavigationView` with `PaneDisplayMode=Auto` and `ExpandedModeThresholdWidth=1008` (`A/MainWindow.axaml:82-85`).
- **ShowMainWindow:** runs `OnShowing()` (refresh the Display codes), restores the window if minimized, then Show and Activate (`W/Ui/PanelWindow.xaml.cs:59-65`).
- **Live sync:**
  - A 3 s `DispatcherTimer` runs only while the window is visible and not minimized (`:27`, `:49`).
  - `Activated` also polls (`:41-42`).
  - Avalonia: `A/MainWindow.axaml.cs:23,50-52,69-74`.
- **On close:**
  - The poll stops, the confirmer is detached and the pages are dropped (`W/Ui/PanelWindow.xaml.cs:67-73`).
  - Avalonia also closes the Finder (`A/MainWindow.axaml.cs:80`).
- **Theme:**
  - WPF: `ThemeMode="System"` with Mica (`W/App.xaml:4`).
  - Avalonia:
    - Mica on Windows 11; elsewhere a solid themed background (`A/Theming/Backdrop.cs:9-23`).
    - The Win32 title bar follows the theme (`A/MainWindow.axaml.cs:86-91`).
    - Accent: the platform accent, else the system accent (Windows and macOS), else ember `#F05A28` (`A/App.axaml.cs:34,286-292`).

### 1.2 State InfoBar: one bar on the Display and Monitor pages
Source: `W/Ui/PanelViewModel.cs:166-236`. The Avalonia version adds the link-failure reasons (`A/ViewModels/MainViewModel.cs:152-246`).

| State | Severity | Title | Message | Action |
|---|---|---|---|---|
| Connecting | Info | "Looking for your monitor…" | – | none; progress bar |
| NotFound | Info | "ThinkVision P32p-20 isn't connected" | "Connect it and we'll find it automatically." | "Check again" |
| NotResponding | Error | "Your monitor isn't responding" | "DDC/CI might be off. Hold the monitor's Exit key for 10 seconds, then try again." | "Try again" |
| *Avalonia only.* reason PermissionDenied (Linux) | Warning | "This app isn't allowed to use the monitor controls" | "Your system blocks access to the monitor's DDC/CI bus. See App settings → System setup." | "Try again" |
| *Avalonia only.* reason ModuleMissing (Linux) | Warning | "Monitor controls aren't turned on in your system" | "The i2c-dev kernel module isn't loaded, so the monitor can't be reached. See App settings → System setup." | "Try again" |
| *Avalonia only.* reason UnsupportedPort (macOS) | Warning | "This port can't control the monitor" | "Connect the monitor to a USB-C or Thunderbolt port. See App settings → System setup." | "Try again" |
| Connected | – | bar hidden | | |

- **Timing:**
  - Connecting shows only if it lasts more than 1 s.
  - Any new non-connected state is held 1 s before the bar changes, so it never flashes (`W/…PanelViewModel.cs:57,111,185-188`).
  - Connected hides the bar at once.
- **The action button:**
  - Calls `RequestReconnect()` and shows progress for up to 16 s, or until the status changes (`:222-236`).
  - Not closable.
  - The reason priority lives at `A/…MainViewModel.cs:199-211`. A reason change with the same status re-renders an open bar (`:159-163`).
- **Page states:** every monitor control panel is `IsEnabled=IsConnected` (`W/Ui/Pages/DisplayPage.xaml:60`, `MonitorPage.xaml:24`). App settings is always enabled.
- **Header status line** (`W/…PanelViewModel.cs:130-150`):

  | Status | Glyph | Text |
  |---|---|---|
  | Connected | ✓ | "Connected over {HDMI\|DisplayPort\|USB-C}", plus " · Firmware LB1.2" when known |
  | Connecting | sync | "Looking for your monitor…" |
  | NotFound | plug | "Not connected" |
  | NotResponding | error | "Not responding" |

### 1.3 Display page
Sources: `W/Ui/Pages/DisplayPage.xaml:38-92` and `A/Views/Pages/DisplayPage.axaml:11-97`.
- **Header:** a monitor glyph, "ThinkVision P32p-20", and the status glyph with its text. Then the InfoBar.
- **"Brightness and contrast"**
  - Brightness (0x10) and Contrast (0x12) sliders.
  - Range 0..Max. Max comes from the monitor reading; the default is 100.
  - Small step 1, large step 10, snap to whole values, click jumps to the point (`W/Ui/Pages/PageResources.xaml:32-43`).
  - The value text is "—" when unknown or disconnected (`W/…PanelViewModel.cs:702`).
  - Max is bound before Value, so a late Max never clamps a value and writes it back.
- **Slider write rules** (`RangeItem`, `W/…PanelViewModel.cs:684-788`):
  - **Pointer drag:** `BeginDrag`, then `DragTo` on each change (live, coalesced, no read-back). On release, `EndDragAsync` writes the final value and verifies it.
    - WPF uses thumb drag events (`W/Ui/Pages/LiveSlider.cs:22-36`).
    - Avalonia starts the drag on a pointer press anywhere on the slider and ends it on release or lost capture (`A/Views/LiveSlider.cs:37-56`).
  - **Keyboard or click:** a 300 ms debounce, then one write with verification (`:687,706-747`).
  - While dragging, settling or debouncing, readings from the monitor are ignored (`Busy`, `:704,776`).
  - **After a write** (`:749-767`):
    - OK → the note is cleared.
    - Blocked with "remains" → note "Your monitor kept {brightness\|contrast} at {actual}". Brightness adds " — the current colour mode may lock it". **This is the "kept old value" message.**
    - Other Blocked → the core message.
    - NotResponding → the note is unchanged.
  - The note replaces the card description until the next successful change (`:668`).
- **"Colour"**
  - The Colour mode (0x14) combo box. Description: "Changes apply straight away" (`:272`).
  - The **"Custom colour"** expander:
    - Contains the Red gain (0x16), Green gain (0x18) and Blue gain (0x1A) sliders.
    - They are enabled only while the cached 0x14 equals `0x12` (User / Custom) (`:250`; `DisplayPage.xaml:68`).
    - Description: "Red, green and blue gain" when enabled, else "Available in User / Custom colour mode" (`:251`).
- **"Input"** (hidden if 0x60 isn't visible)
  - Card "Input", description "This PC uses {label}" (`:252`).
  - Radio buttons: HDMI, DisplayPort, USB-C.
  - Avalonia shows a "This PC" caption under this PC's input (`A/Views/Pages/DisplayPage.axaml:82`).
  - A click calls `SelectInputAsync`. It is a no-op when disconnected or already selected. Avalonia snaps the radio back in that case (`A/…MainViewModel.cs:418-428`).
  - **Confirm before switching away from this PC:** see §1.8.
- **"Presets":** see §4.

### 1.4 Monitor settings page
Sources: `W/Ui/Pages/MonitorPage.xaml:18-72` and `A/Views/Pages/MonitorPage.axaml:12-74`.

**Groups, in this order** (`W/…PanelViewModel.cs:60-68,277-289`). Empty groups are hidden.

| Group | Code | Name | Control | Values | Description |
|---|---|---|---|---|---|
| Picture | 0xE0 | Over Drive | combo | 00 Off, 01 Normal, 02 Extreme | "Reduces motion blur. Extreme can leave trails behind moving objects" |
| Picture | 0xEA | Dynamic Contrast | toggle | 00 Off, 01 On | "Adjusts contrast to match what's on screen" |
| Picture | 0x86 | Aspect ratio | combo | 02 Full screen, 05 Original aspect ratio | "The monitor greys this out at its native resolution and may ignore changes" |
| Ports | 0xEC | USB Charging | toggle | 00 Off, 01 On | "Charge devices from the monitor's USB ports while it's in standby" |
| On-screen menu | 0xCC | Menu language | combo | 02 English, 03 French, 04 German, 05 Italian, 06 Japanese, 09 Russian, 0A Spanish, 0D Chinese (Simplified) | "The language of the monitor's own menus" |
| More settings | any finder-named code | user name | combo or toggle | user labels | – |

- A code is placed when it is visible, not an Action, not a Display code and not 0xD6. Known codes keep the order above; the rest are ordered by code.
- Avalonia toggles show "On"/"Off" text beside the switch (`A/ViewModels/ControlItems.cs:232`).
- **Enum and toggle writes:**
  - The value is marked at once, then `WriteAsync` runs.
  - On Blocked or NeedsConfirmation, the core message becomes the card note.
  - The card then resyncs from the cache, so a failed change snaps back (`W/…PanelViewModel.cs:410-430,840-874`).
- **"Reset" → expander "Reset and restore"** ("Return settings to their factory values, or restore a backup"):
  - The Action rows, ordered by code, each with a **Reset** button (`:290-294`):
    - 0x04 "Factory reset": "Restores every on-screen-menu setting. We save a backup first"
    - 0x05 "Reset brightness and contrast"
    - 0x08 "Reset colour"
  - A successful reset re-reads every non-action card (`:428-429`).
  - **"Restore backup"**:
    - Button **Restore**. Description "Put back the settings saved before the last reset"; after a run it reads "Backup restored" or the error, for example "No backup found." (`:259,495-506`).
    - **No confirmation.**
- **"Tools":** the clickable card "Find hidden settings" ("Look for settings your monitor supports but doesn't list") opens the Finder (§1.7).

### 1.5 App settings page
Sources: `W/Ui/Pages/AppSettingsPage.xaml:17-89` and `A/Views/Pages/AppSettingsPage.axaml:14-130`.
- **Error InfoBar:** "Couldn't save settings: {ex.Message}", shown while the last save failed (`W/…PanelViewModel.cs:575-581,638-653`).
- **"System setup"** (Avalonia only; hidden when empty) (`A/…AppSettingsPage.axaml:18-49`):
  - One card per `SetupHint` (title, message).
  - The command sits in a selectable text row with a **Copy** button. Copying puts it on the clipboard and shows a "Copied" tooltip (`A/Views/Pages/AppSettingsPage.axaml.cs:20-34`).
  - The hints are re-detected on show and on every status change (`A/…MainViewModel.cs:554-565,356-360`).
  - Hint content is in §9.
- **"Startup"**
  - Expander **"Run in background"**, toggle bound to `BackgroundMode`.
    - Description: "Keep the tray icon and keyboard shortcuts running after you close the window"; macOS says "menu bar icon" (`A/…MainViewModel.cs:608-610`).
  - Nested **"Start with Windows"** (Avalonia on Linux and macOS: "Start when you log in") (`:622`), toggle bound to `StartWithWindows`.
    - Description: "Starts in the background" or "Opens the window", depending on BackgroundMode, followed by " when you sign in" (Windows) or " when you log in" (`:624-625`).
- **"This PC":** card "This PC's input", combo HDMI / DisplayPort / USB-C, description "We ask before you switch the monitor away from it" (`W/…PanelViewModel.cs:520-537`).
- **"Keyboard shortcuts"**
  - **Caption** (`W/…:566-568`; `A/…:629-632`):
    - "Shortcuts work whenever ThinkVision Control is running." when BackgroundMode is on.
    - "Shortcuts work while Run in background is on. Turn it on to use them." when it is off.
    - Avalonia, when hotkeys are unavailable: "This desktop doesn't let apps register global shortcuts. See System setup above."
  - **11 cards** (labels at `:622-636`). Each card:
    - Shows the keycaps, or "Not set".
    - Has an edit glyph button, which opens the editor (§3.4).
    - Has the accessible name "Edit shortcut for {Label}, currently {keys joined by " + " | none}" (`:957`).
  - **Keycap rendering** (`:960-963`):
    - Up, Down, Left and Right show as ↑ ↓ ← →.
    - Avalonia renames Win to Cmd (macOS) or Super (Linux), and Alt to Option on macOS (`A/ViewModels/ControlItems.cs:354-368`).
  - **The card description** is "Another app is using this shortcut" when its registration failed. This only applies while BackgroundMode is on and, on Avalonia, while hotkeys are available (`W/…:593-597,615-620`; `A/…:666-671`).
- **"About"**
  - Expander "ThinkVision Control", "Version {major}.{minor}.{build}" (`:570`).
  - Row "Monitor profile": "ThinkVision P32p-20 · Firmware LB1.2" (`:572`).

### 1.6 OSD: shortcut and command feedback
Sources: `W/Ui/OsdWindow.xaml(.cs)` and `A/Views/OverlayOsd.cs`.
- **Window:**
  - Click-through, never activated, topmost, no taskbar entry.
  - WPF ex-styles: `TRANSPARENT|NOACTIVATE|TOOLWINDOW|LAYERED` (`W/Ui/OsdWindow.xaml.cs:105-111`).
  - Avalonia on Windows adds `LAYERED|TRANSPARENT` only when composited (`A/Views/OverlayOsd.cs:195-207`).
  - macOS: `ignoresMouseEvents`, level 25, collection behaviour 337 (`A/Platform/MacOS/MacWindowTweaks.cs:19-43`).
- **Placement:** bottom-centre of the primary work area, with the card's bottom edge 96 px above it. 24 px of shadow margin (`W/…OsdWindow.xaml.cs:17,98-103`; `A/Views/OverlayOsd.cs:19,99-113`).
- **Card:**
  - Width 240–420, padding 16×12, corner radius 8, 1 px stroke, base fill at 95 %, drop shadow (`W/Ui/OsdWindow.xaml:10-45`).
  - Avalonia drops the margin and shadow when there's no compositor (`A/…OverlayOsd.cs:176-186`).
- **Two modes** (`W/…OsdWindow.xaml.cs:35-56`):
  - `ShowMessage(text)`: centred wrapped text.
  - `ShowLevel(label, value, max)`: a sun or contrast glyph, the label, the value in semibold, and a 4 px bar filled to `min(value,max)/max`.
- **Timing** (`:58-95`):
  - Fade in over 167 ms. WPF adds a 12 px slide-up (exponential ease, exponent 7) when client animations are on.
  - It stays 1.2 s after the **last** update. A re-show cancels a fade-out.
  - Fade out over 167 ms, hide, then raise `Hidden`. Headless mode exits on `Hidden`.
- **Wayland:** the OSD is a desktop notification instead (§9).
- **Texts** (`W/App.xaml.cs:282-306`):
  - When not connected: "Your monitor isn't responding" (NotResponding) or "ThinkVision P32p-20 isn't connected" (otherwise).
  - A step that succeeds → `ShowLevel("Brightness"|"Contrast", current, max)`.
  - Anything else → the core `WriteResult.Message`: "Day preset", "Input → HDMI", "Brightness remains 50; the monitor did not apply 55. The current color mode may lock brightness.", "Monitor not responding", "0x13 is not a valid value for Input.", and so on.

### 1.7 Finder ("Find hidden settings") window
Sources: `W/Ui/FinderWindow.xaml(.cs)` and `A/Views/FinderWindow.axaml(.cs)`.
- **Window:**
  - 600×720, minimum 460×480, centred on the owner.
  - Only one at a time; reopening activates it (`W/Ui/PanelWindow.xaml.cs:169-182`).
- **Intro text:** "This only reads from the monitor; nothing is written here. It compares Lenovo's undocumented codes before and after you change one setting with the monitor's own buttons, so you can name what each code controls. A named code then appears in Monitor settings."
- **Status InfoBar:**
  - Not closable; shows progress while busy.
  - Messages: "Reading the monitor…" while working; "Couldn't read the monitor: {msg}" (Error).
- **Step 1, "Take a snapshot"** ("Reads the current value of every code the monitor reports"):
  - The **Take snapshot** button calls `VendorCodeFinder.SnapshotAsync`. It reads every code that is Hidden and not NeverWrite, so 0xCA is excluded (`C/Finder/VendorCodeFinder.cs:12-25`).
  - The card then reads "Snapshot taken ({n} codes)".
  - Reads only happen while Connected; otherwise the snapshot is empty.
- **Step 2:** an instruction card, "Change one setting with the monitor's buttons" ("For example Over Drive. Change only one, then compare.").
- **Step 3, "Compare"** ("Lists the codes whose value changed"):
  - The button is disabled until there is a snapshot.
  - It lists the diff as rows of the form `0xC6 (Name): 1 → 2` (`C/Finder/VendorCodeFinder.cs:27-31`; `W/…FinderWindow.xaml.cs:142-146`).
  - Status after comparing: "Nothing changed. Make sure the setting really changed on screen, then compare again." or "Select a change to name it.".
  - A single change is selected automatically.
- **Naming panel** (`W/…FinderWindow.xaml.cs:75-122`):
  - A "Setting name" text box, prefilled if the code is already named.
  - One text box per value. The values come from the capability list for the code, or else the before/after values. Captions read `0x01 (before)` / `0x02 (now)`.
  - **Save:**
    - Without a name: "Enter a name for the setting first." (Warning).
    - Otherwise `ProfileStore.SaveOverlay(code, name, Enum, Advanced, Normal, labels)`. Blank labels are dropped.
    - Then `ReloadProfile()` and a card rebuild.
    - Success: "Saved \"{name}\" (0xNN). It now appears in Monitor settings."; failure: "Couldn't save: {msg}".

### 1.8 Confirm dialogs
Source: `W/…PanelViewModel.cs:432-451`.

**Dialog behaviour:**
- The default is always **Close/Cancel**, which has the accent style, initial focus and Enter (`DialogSpec` default `Close`, `W/Ui/Controls/ContentDialogHost.cs:23-24`).
- Esc means Cancel.
- If the window isn't visible, the confirm returns false, so the write is cancelled and the card resyncs (`W/Ui/PanelWindow.xaml.cs:75-76`).
- WPF queues dialogs first in, first out (`ContentDialogHost.cs:75-88`). Avalonia refuses, and so cancels, a confirm while another dialog is open (`A/MainWindow.axaml.cs:127-128`).

| Trigger | Title | Body | Primary |
|---|---|---|---|
| Input (0x60) value ≠ ThisMachineInput | "Switch to {label}?" | "You won't be able to control the monitor from this PC until you switch back." | Switch |
| 0x04 Action | "Reset monitor to factory settings?" | "This restores every on-screen-menu setting, and it can turn off DDC/CI on some Lenovo monitors. We save a backup first." | Reset |
| 0x05 Action | "Reset brightness and contrast?" | – | Reset |
| 0x08 Action | "Reset colour settings?" | – | Reset |
| any other ConfirmRequired code | `ConfirmMessage` or "Change {Name}?" | – | Continue |

- These are UI-only. **Hotkeys, the command line, the tray menu and Restore backup never confirm an input switch.** Input is safety-class Normal.
- A confirmed write passes `confirmed:true` to the core, which satisfies `SafetyPolicy` for ConfirmRequired codes.
- **Shortcut-editor dialog:** title "Edit shortcut". Buttons: Save (default; enabled only while valid), Reset (WPF) or Reset to default (Avalonia), Cancel (`W/Ui/PanelWindow.xaml.cs:139-167`; `A/MainWindow.axaml.cs:156-191`).
- **Default-conflict dialog:**
  - Title: "{Label} can't use its default shortcut".
  - Body: "{Other} already uses {default}. Change that shortcut first." Avalonia renders the default as keycaps joined with "+".
  - Button: OK.

### 1.9 Tray flyout
- **There is none in 0.0.4.** It was removed (`docs/superpowers/plans/2026-10-28-tray-sync-0.0.4.md:29`). A tray click opens or focuses the main window.

---

## 2. Tray
- **Created** only when BackgroundMode is on (`W/App.xaml.cs:238-251`; `A/App.axaml.cs:177-198`).
- **Turning BackgroundMode off:**
  - WPF disposes the tray and the hotkeys.
  - Avalonia keeps the tray until exit and only suspends the hotkeys (`A/App.axaml.cs:193-198`).
- **Icon, WPF** (`W/Tray/TrayIcon.cs:108-164`):
  - `Tray.OnLight.ico` or `Tray.OnDark.ico` (copies in `assets/brand/ico/`), chosen by `HKCU\…\Themes\Personalize\SystemUsesLightTheme`, which is the taskbar theme, not the app theme.
  - Reloaded on `UserPreferenceChanged` (General or Color).
  - Size: `SmallIconSize`.
- **Badges:**
  - **WPF:**
    - NotFound: × in a foreground-coloured circle (dark taskbar: white; light: #1B1B1B).
    - NotResponding: ! in a caution circle (dark: #FCE100; light: #9D5D00).
    - The mark is black on dark taskbars and white on light ones.
    - Badge diameter `max(7, round(0.45·size))`, cut out with a 1 px transparent ring.
    - No badge for Connecting or Connected.
  - **Avalonia** (`A/Platform/Shared/AvaloniaTray.cs:214-251`):
    - Drawn on the app icon (`ThinkVisionControl.ico`, largest PNG entry) at 32 px.
    - NotFound: grey #767676 with a white ×. NotResponding: amber #FFB900 with a black !.
    - Diameter 0.5·size, gap size/16.
- **Tooltip:** "ThinkVision Control – " (en dash) followed by "P32p-20", "Connecting", "Not connected" or "Not responding" (`W/Tray/TrayIcon.cs:58-64`; `A/…AvaloniaTray.cs:129-135`).
- **Left click:**
  - Opens or focuses the window (`W/Tray/TrayIcon.cs:45`; `A/…AvaloniaTray.cs:52`).
  - WPF also re-adds the icon after Explorer restarts, through NotifyIcon's built-in TaskbarCreated handling (`:14-15`).
- **Right-click menu**, rebuilt on every open:
  - **WPF** (`W/Tray/TrayIcon.cs:80-93`; `W/App.xaml.cs:256-258`):
    1. **Open ThinkVision Control** (bold)
    2. separator
    3. *only while Connected:* Day preset, Night preset, separator
    4. Exit
  - WPF has **no "Switch to this PC"** item, despite the plan saying "unchanged".
  - WPF styling: Windows 11 look with rounded corners (DWM attribute 33 = 2), Segoe UI 9.
    - Light: background #F9F9F9, hover #EAEAEA, text #1A1A1A, separator #E0E0E0.
    - Dark: background #2C2C2C, hover #3D3D3D, text white, separator #404040.
  - **Avalonia** (`A/App.axaml.cs:207-224`):
    1. Open ThinkVision Control (default, placed first)
    2. separator
    3. *only while Connected:* Brightness up, Brightness down (Linux only), then Day preset, Night preset, Switch to this PC, separator
    4. Exit
  - Avalonia removes leading, trailing and doubled separators (`AvaloniaTray.cs:108-123`).
  - Menu actions run the same path as the hotkeys (OSD feedback included).
- **Exit:**
  - WPF: `ExitApp`, with no relaunch.
  - Avalonia: `desktop.Shutdown()`.
- **Linux tray host:** `org.kde.StatusNotifierWatcher` NameHasOwner, with a 1 s timeout (`A/Platform/Linux/SniHost.cs:12-19`). If it is missing, the tray is still "shown" but invisible, and GNOME gets a setup hint.

---

## 3. Keyboard shortcuts

### 3.1 Actions, defaults and effects
Sources: `C/Settings/AppSettings.cs:3-7`, `C/Settings/SettingsStore.cs:17-31`, and `W/App.xaml.cs:260-279`.

| Action (enum = settings key) | Label | Default | Effect |
|---|---|---|---|
| BrightnessUp | Brightness up | Ctrl+Alt+Up | Step 0x10 by +5 |
| BrightnessDown | Brightness down | Ctrl+Alt+Down | Step 0x10 by −5 |
| ContrastUp | Contrast up | Ctrl+Alt+Left | Step 0x12 by +5 |
| ContrastDown | Contrast down | Ctrl+Alt+Right | Step 0x12 by −5 |
| DayPreset | Day preset | Ctrl+Alt+Home | Apply the Day preset |
| NightPreset | Night preset | Ctrl+Alt+End | Apply the Night preset |
| InputHdmi | Input HDMI | Ctrl+Alt+F1 | Set 0x60 = 0x11 |
| InputDp | Input DisplayPort | Ctrl+Alt+F2 | Set 0x60 = 0x0F |
| InputUsbC | Input USB-C | Ctrl+Alt+F3 | Set 0x60 = 0x31 |
| SwitchToMe | Switch to this PC | Ctrl+Alt+M | Set 0x60 = ThisMachineInput |
| OpenPanel | Open ThinkVision Control | Ctrl+Alt+O | ShowWindow |

- **Steps** clamp to 0..Max and go through the verified `SetAsync` (`C/Control/ControlService.cs:370-380`).
- Without a cached value, a step refreshes first. If that fails: "Could not read 0xNN." or NotResponding.
- Feedback is the OSD (§1.6).
- **Order** (the enum order) is also the order of the App settings cards and the Win32 hotkey ids (`id = action+1`).

### 3.2 Gesture format
Source: `C/Settings/HotkeyGesture.cs`.
- **Text form:** "Ctrl+Alt+PageUp".
  - Modifier names Ctrl, Control, Alt, Shift, Win, case-insensitive.
  - Exactly one key, and ≥1 modifier (`:27-42`).
- **Keys:**
  - A–Z, 0–9, F1–F24 (VK `0x6F+n`).
  - Named keys: PageUp/PgUp, PageDown/PgDn, End, Home, Left, Up, Right, Down, Insert, Delete (`:15-25,61-76`).
- **Stored as** Win32 MOD bits (Alt 1, Ctrl 2, Shift 4, Win 8) plus a VK.
- **Canonical output order:** Ctrl, Alt, Shift, Win, then the key (`:78-92`).
- **`TryCreate`** rejects zero modifiers, unknown bits and unsupported keys (`:44-59`).
- **Empty string = not set.** It is stored and kept, and never registered.

### 3.3 Registration per OS
- **Windows** (both apps):
  - `RegisterHotKey` on a message-only window, with `MOD_NOREPEAT (0x4000)`, `id = (int)action+1`, handling `WM_HOTKEY 0x0312` (`W/Hotkeys/HotkeyManager.cs:28-41,55-60`; `A/Platform/Windows/WindowsHotkeys.cs:23-37`).
  - A failure means "another app owns it" → the card warning.
- **Linux X11** (`A/Platform/Linux/X11Hotkeys.cs`):
  - Uses its own `XOpenDisplay` connection on a dedicated thread.
  - Grabs the key on the root window with XCB checked `grab_key`, four times: with and without CapsLock and NumLock (`:18,152-171`).
  - BadAccess, an unknown keycode or a duplicate → failed.
  - A press matches on (keycode, state & (Shift|Ctrl|Mod1|Mod4)).
  - Mapping: Ctrl→ControlMask, Alt→Mod1, Shift→Shift, Win→Mod4, plus the VK→keysym table (`A/Platform/Linux/LinuxKeys.cs:12-58`).
  - `Suspend` ungrabs.
- **Linux Wayland** (`A/Platform/Linux/PortalHotkeys.cs`):
  - xdg-desktop-portal `GlobalShortcuts`. It first calls `org.freedesktop.host.portal.Registry.Register("thinkvision-control")`, if that exists.
  - Then `CreateSession` and `BindShortcuts`:
    - The ids are the enum names.
    - The description comes from `LinuxKeys.Describe`, for example "Monitor brightness up".
    - `preferred_trigger` is, for example, "CTRL+ALT+Up" (`LinuxKeys.cs:61-88`).
  - It re-binds, opening a new session, only when the map changes (`:55-78`). The desktop may prompt the user and has the final say.
  - **Suspend only ignores activations**, so editing never re-prompts (`:80`).
  - Only unparseable gestures are reported as failed.
  - It is available when the `version` property is readable (3 s probe) (`:41-46,90-133`).
- **Selection:**
  - A Wayland session (`XDG_SESSION_TYPE=wayland`, or `WAYLAND_DISPLAY` set) → the portal.
  - Else `DISPLAY` set → X11.
  - Else Null: unavailable, and everything fails (`A/Platform/Linux/LinuxPlatform.cs:16-31`).
- **macOS** (`A/Platform/MacOS/MacHotkeys.cs`, `MacRules.cs`):
  - Carbon `RegisterEventHotKey` on `GetApplicationEventTarget()`, with signature `'TVCh'` (0x54564368) and id = action+1 (`:20,34-60`).
  - The VK→kVK table is for ANSI positions; Insert maps to Help and Delete to Forward Delete (`MacRules.cs:25-50`).
  - Ctrl→control, Alt→option, Shift→shift, Win→**cmd**.
  - **Command or Control is required**, by the Sequoia rule; Shift- or Option-only gestures fail. F21–F24 fail (`MacRules.cs:70-79`).
  - No permission prompt.

### 3.4 Shortcut editor
Sources: `W/Ui/HotkeyEditor.xaml.cs` and `A/Views/ShortcutEditor.axaml.cs`.
- **Global hotkeys are suspended while the dialog is open.** They are re-registered afterwards:
  - WPF: always, then the warnings refresh (`W/Ui/PanelWindow.xaml.cs:144,162-166`).
  - Avalonia: through `settingsChanged`, but only if nothing was saved (`A/MainWindow.axaml.cs:186-190`).
- **Text:** "Press a combination of keys to change the shortcut for **{Label}**."
- **Capture area:**
  - Placeholder "Press keys"; takes focus on open.
  - Keys show as large accent keycaps. Modifiers alone show "live" at 75 % opacity and revert on release.
- **Not recorded:** Esc (closes), Enter with no modifiers (saves), Tab and Shift+Tab (move focus) (`W/…:44-46`).
- **Errors:**
  - Without Ctrl, Alt or Win: "This shortcut isn't valid" / "Start with Ctrl, Alt or the Windows key." Avalonia words the keys per OS: "Control, Option or Command" on macOS, "Ctrl, Alt or Super" on Linux (`A/…:47-50`).
  - An unsupported key: "This key can't be used" / "Use a letter, number, arrow, Home, End, Page Up, Page Down, Insert, Delete or function key."
  - A **conflict** (another action has the same text, case-insensitive) → Warning "{Other} already uses this shortcut" / "Choose a different combination."
- **Save is enabled** only when the gesture is valid, has no conflict and differs from the original (case-insensitive) (`W/…:77-84`).
- **Clear** sets the result to "" and is valid when the original was set. The button is disabled while the result is empty (`:94-101,123`).
- **Hint:** "Shortcuts start with {Ctrl, Alt or the Windows key}, and can include Shift."
- **Reset button** restores the default from `SettingsStore.Defaults`. If another action uses that default, the conflict dialog (§1.8) appears instead.

---

## 4. Presets (Day and Night)
- **Model:** `Preset(Name, Brightness:uint, Contrast:uint, ColourMode:byte?)` (`C/Settings/AppSettings.cs:9`).
- **Defaults:** Day 60/70/null; Night 0/70/null (`C/Settings/SettingsStore.cs:32-33`).
- **Apply** (`C/Control/ControlService.cs:382-397`):
  - Order: colour mode (if not null), then brightness, then contrast.
  - Stops at the first non-OK result. Example: sRGB locks brightness → Blocked "Brightness remains…", and contrast is skipped.
  - Success message: "{Name} preset".
- **Applied from:** the card's Apply button, hotkeys, the tray, the jump list, and `--preset`.
- **Card** (`W/…PanelViewModel.cs:889-923`; `W/Ui/Pages/DisplayPage.xaml:13-27`):
  - Header "Day" or "Night", with a sun or moon icon.
  - Description "Brightness {b}, contrast {c}", plus ", {colour mode label}" when the preset has one (`:487-493`).
  - An error note replaces the description: Blocked or NeedsConfirmation messages from an apply, or the settings error from a save (`:453-458`).
  - Buttons: **Apply**, and **More (…)**, which opens a menu with one item, "Save current settings as {Name}".
- **Save** (`W/…PanelViewModel.cs:462-479`):
  - Copies the *cached* brightness, contrast and colour mode. Each falls back to the old preset value when not cached.
  - **No confirmation.**
  - The result shows as a tooltip under the More button for 2.5 s, with a check or error glyph: "Saved as Day" or "Couldn't save Day" (`W/Ui/Pages/DisplayPage.xaml.cs:12,32-64`; `A/MainWindow.axaml.cs:105-123`).
  - WPF also raises a UIA notification.

---

## 5. Persisted files
- **Data directory:** `Environment.SpecialFolder.ApplicationData` + `ThinkVisionControl` (`W/App.xaml.cs:60`; `A/App.axaml.cs:56`):
  - Windows: `%APPDATA%\ThinkVisionControl`.
  - Linux and macOS: `~/.config/ThinkVisionControl` (.NET maps ApplicationData to `$XDG_CONFIG_HOME` or `~/.config` on both; README:254).
- **Both apps share the same directory and files.**

### 5.1 `settings.json` (`C/Settings/SettingsStore.cs`)
```json
{
  "Hotkeys": { "BrightnessUp": "Ctrl+Alt+Up", "...": "...", "OpenPanel": "Ctrl+Alt+O" },
  "Day":   { "Name": "Day",   "Brightness": 60, "Contrast": 70, "ColourMode": null },
  "Night": { "Name": "Night", "Brightness": 0,  "Contrast": 70, "ColourMode": "0x12" },
  "ThisMachineInput": "0x11",
  "StartWithWindows": false,
  "BackgroundMode": false
}
```
- **Format:**
  - PascalCase property names; enum keys and values are strings; indented (`:10-15`).
  - `byte` values are written as `"0xNN"`. On read, a number, a `"0x"`-hex string or a decimal string is accepted (`:80-94`).
- **Missing file** → Defaults: BackgroundMode **false**, StartWithWindows false, ThisMachineInput 0x11 (`:17-36,43`).
- **Corrupt file** (a JsonException, or a null Day or Night) → renamed to `settings.json.bak` (overwriting it) and Defaults are returned (`:45-53`).
- **Migrations on load:**
  - A file without `BackgroundMode` → **true** (the record's default parameter) (`C/Settings/AppSettings.cs:17`; test `T/SettingsTests.cs:63`).
  - A missing hotkey action → its default.
  - The legacy defaults are replaced: BrightnessUp "Ctrl+Alt+PageUp", BrightnessDown "Ctrl+Alt+PageDown", ContrastUp "Ctrl+Alt+Shift+PageUp" and ContrastDown "Ctrl+Alt+Shift+PageDown" become the current defaults. Custom values are kept (`:55-68`).
- **Save** is atomic: write `settings.json.tmp`, then move over (`:71-78`).
- **Every UI change saves immediately and then re-applies the settings** (`W/…PanelViewModel.cs:638-653` → `App.ApplySettings`). That re-registers the hotkeys and rewrites the autostart entry.

### 5.2 Other files in the same directory
| File | Content | Source |
|---|---|---|
| `profile.user.json` | User overlay: `{"edidId":"LEN62A2","controls":[{"code":"0xNN","name":…,"kind":"Enum","section":"Advanced","safety":"Normal","values":{"0x00":"Off"},"confirm":null}]}`. camelCase (web defaults), enums as strings. **The overlay wins per code.** A corrupt file is ignored. | `C/Profile/ProfileStore.cs:12-18,22,62-67,76-88` |
| `capabilities.txt` | The raw capability string from the last live read. It renders the controls offline at the next start. Written only when it changed. | `C/Control/CapabilitiesCache.cs`; `C/Control/ControlService.cs:41-42,85-89` |
| `backup-yyyyMMdd-HHmmss[-n].json` | `{"0x10": 60, …}`: every **cached** Range or Enum value with safety Normal (Input 0x60 included). Written before **every** Action write (all three resets). The latest is chosen by stamp, then suffix. | `C/Control/BackupStore.cs`; `ControlService.cs:225-236,440-450` |
| `log.txt` / `log.1.txt` | `yyyy-MM-dd HH:mm:ss.fff message` lines. Rolled when the log exceeds 1 MB. Never throws. | `C/Logging/FileLog.cs` |

---

## 6. Command line and single instance

### 6.1 Flags (`C/Commands/CommandLine.cs`)
| Flag | Values |
|---|---|
| `--preset` | `day` / `night` |
| `--input` | `hdmi`(0x11), `dp` / `displayport`(0x0F), `usbc` / `usb-c`(0x31), `this` (ThisMachineInput) |
| `--step` | `brightness=±N` / `contrast=±N`, with \|N\| ≤ 100; the sign is optional |
| `--startup` | Start in the tray with no window. Combines with the verbs |

- **Parsing:**
  - `--x value` and `--x=value` are both accepted; names and values are case-insensitive. Arguments without `--` are ignored, and so are unknown `--flags` (`:28-48`).
  - **The first verb wins.**
  - **A malformed first verb → Show** (open the window). Any later valid verb is ignored (`:45`; tests `T/RedesignCoreTests.cs:171-190`).
- **Canonical output** (for the jump list): `--preset day`, `--input hdmi|dp|usbc|this`, `--step brightness=+5` (`:50-58`).

### 6.2 Launch behaviour (`W/App.xaml.cs:48-88,187-193`; `A/App.axaml.cs:49-141`)
- **A verb, with no running instance → headless** (`W/App.xaml.cs:173-185`):
  1. Connect once (no retry burst), run the command and show the OSD.
  2. Exit when the OSD hides, with a 5 s safety net.
  3. No window, tray or settings side-effects.
- **No verb:**
  - `StartResident(showWindow: !BackgroundMode || !startup)`.
  - So `--startup` with BackgroundMode → tray only; without BackgroundMode, the window opens.
- **A second launch forwards its args** to the primary. In the primary:
  - Show → `StartResident(showWindow: !startup || !BackgroundMode)`. A forwarded `--startup` (from a restart that lost a race) keeps the tray only.
  - Verbs → `RunCommandAsync`.
  - A forward that arrives during a headless run turns that process resident (`_headless=false`).
- **WPF single instance:**
  - Mutex `ThinkVisionControl.SingleInstance` (`W/App.xaml.cs:53`), which is also the installer's AppMutex.
  - Named pipe `ThinkVisionControl.Commands.{UserName}.{SessionId}`, current user only, args joined with `\n` (`W/Platform/CommandPipe.cs:16-41`).
  - The sender calls `AllowSetForegroundWindow(serverPid)`, so the existing window can come to the front (`:30-32`).
  - **Hand-off or take-over** (`W/App.xaml.cs:132-142`):
    - For up to 5 s it alternates a pipe send (250 ms) with waiting on the mutex (250 ms).
    - If it gets the mutex, or the mutex was abandoned, this process carries on as primary.
    - **It never quits silently** while an old copy is still shutting down.
  - If the listener errors, it retries after 500 ms (`CommandPipe.cs:51-74`).
- **Avalonia, Windows** (`A/Platform/Windows/WindowsSingleInstance.cs`):
  - Mutex `thinkvision-control.SingleInstance`, pipe `thinkvision-control.Commands.{user}.{session}`. The names differ from WPF's on purpose, so both can run side by side.
  - Send: 3 attempts × (1 s connect + 200 ms). There is no take-over, and the process quits even if the send fails (`A/App.axaml.cs:68-73`).
  - No AllowSetForegroundWindow.
- **Avalonia, Linux** (`A/Platform/Linux/UnixSingleInstance.cs:29-57`):
  - A `flock` (FileShare.None) on `$XDG_RUNTIME_DIR/thinkvision-control.lock`; the socket is `$XDG_RUNTIME_DIR/tvc-cmd`.
  - Fallback: `/tmp/thinkvision-control-{user}.lock` and `/tmp/CoreFxPipe_tvc-cmd`.
- **Avalonia, macOS** (`A/Platform/MacOS/UnixSingleInstance.cs:18-51`):
  - Lock `$TMPDIR/thinkvision-control.lock`; socket `$TMPDIR/CoreFxPipe_tvc-cmd`. The short name keeps it within the 104-byte limit for socket paths.
  - The stale socket is deleted once the lock is held.
  - If the lock file belongs to someone else (UnauthorizedAccess), it runs as primary without forwarding.

---

## 7. Background and lifecycle
- **Run in background (BackgroundMode)** gates the tray and the hotkeys. Default false on a new install; true for upgraded settings (§5.1).
- **Closing the window:**
  - **WPF** (`W/App.xaml.cs:216-230`):
    - BackgroundMode off → the process exits.
    - BackgroundMode on → **restart-on-close**:
      1. Wait 400 ms.
      2. `WhenQuietAsync(quiet 1 s, max 10 s)`, so writes still in flight finish.
      3. Abort if the window was reopened or the app is exiting.
      4. `Shutdown()` with `_relaunch`.
      5. At the very end of `OnExit`, after the pipe, hotkeys, tray, link and mutex are disposed, start `"<exe>" --startup` (`:90-113`).
    - The reason: a tray-only process uses about 15 MB, against about 45–65 MB once WPF has rendered (`:214-215`).
    - Accepted cost: the tray icon blinks and the hotkeys are dead for about 1 s.
    - Never on tray Exit or session end (`OnSessionEnding` sets `_exiting`, `:115-119`).
  - **Avalonia** (`A/App.axaml.cs:160-166`):
    - BackgroundMode off → Shutdown.
    - BackgroundMode on → the window is released and the process stays. The view model lives on. **No restart trick.**
- **Lazy UI:**
  - A `--startup` launch builds no window, view model or OSD until first use. The view model is created on the first show (`W/App.xaml.cs:195-200`).
  - WPF uses raw message-only windows (not HwndSource) to avoid loading D3D (`W/Platform/MessageWindow.cs:5-9`).
  - The jump list is set at ApplicationIdle (`W/App.xaml.cs:153`).
- **Efficiency mode, Windows only:**
  - EcoQoS (`ProcessPowerThrottling` EXECUTION_SPEED) plus low memory priority (2) while no window is visible.
  - HighQoS and memory priority 5 while it is visible.
  - Sources: `W/Platform/EfficiencyMode.cs:22-51`, `W/App.xaml.cs:233`, `A/App.axaml.cs:168-172`.
- **Start with the OS:**
  - The setting is re-applied on **every** `ApplySettings`: at start and after every settings save.
  - **Windows** (`W/Platform/StartupRegistration.cs:9-21`; `A/Platform/Windows/WindowsAutostart.cs:14-40`):
    - Value `ThinkVisionControl` = `"<exe>" --startup` under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
    - Deleted when off.
    - The uninstaller removes it.
  - **Linux** (`A/Platform/Linux/LinuxAutostart.cs:33-65`):
    - The file `$XDG_CONFIG_HOME|~/.config/autostart/thinkvision-control.desktop`, with `Exec="<exe>" --startup`, `TryExec`, `Icon=thinkvision-control` and `X-GNOME-Autostart-enabled=true`.
    - Exec values are escaped per the Desktop Entry spec (`:71-86`).
    - Deleted when off.
  - **macOS** (`A/Platform/MacOS/MacAutostart.cs:27-58`):
    - `SMAppService.mainAppService` register and unregister (macOS 13+), and only from a `.app` bundle.
    - When the item needs approval, it opens System Settings → Login Items. Because the setting is re-applied on every save, **Login Items reopens on every settings change while approval is pending.**
- **Connect on start:** `StartAsync()`, then `RequestReconnect()` if not connected (`W/App.xaml.cs:159-163`).
- **Recheck after display, monitor or power events** (`W/App.xaml.cs:166-171`; `A/App.axaml.cs:116-120`):
  - `ReconnectAsync()` runs even while connected: it re-runs `TryConnect`, which releases and re-acquires the handle.
  - Then `RequestReconnect()` if the monitor is still not connected.
- **Event sources:**

  | App / OS | Source | Debounce | Location |
  |---|---|---|---|
  | WPF | `SystemEvents.DisplaySettingsChanged` + `PowerModeChanged(Resume)` | 2 s | `W/Platform/DisplayEvents.cs:8-35` |
  | WPF | `MonitorArrivalWatcher`: `RegisterDeviceNotification` for monitor interface GUID `E6F07B5F-EE97-4a90-B076-33F57BF4EAA7`, arrival or remove-complete, on a message-only window | 500 ms | `W/Platform/MonitorArrivalWatcher.cs:13-76` |
  | Avalonia, Windows | One hidden top-level window: `WM_DISPLAYCHANGE`, `WM_POWERBROADCAST` (PBT 0x7 / 0x12) and `WM_DEVICECHANGE` (same GUID) | 500 ms | `A/Platform/Windows/WindowsMonitorEvents.cs:20-99` |
  | Linux | NETLINK_KOBJECT_UEVENT group 1, `SUBSYSTEM=drm` with `HOTPLUG=1` or ACTION change/add/remove. Fallback: poll the `/sys/class/drm/*/status` fingerprint every 3 s. No explicit resume hook. | 1.5 s | `A/Platform/Linux/LinuxMonitorEvents.cs:18-152` |
  | macOS | `CGDisplayRegisterReconfigurationCallback` ("after" calls only) + `IORegisterForSystemPower` (`HasPoweredOn`). It always answers sleep queries with `IOAllowPowerChange`. | 500 ms | `A/Platform/MacOS/MacMonitorEvents.cs:26-148` |

- **Retry burst:** delays of 0.5, 1, 2, 4 and 8 s, each followed by `ReconnectAsync`. It stops once connected and is a no-op while connected. A new request cancels the old burst (`C/Control/RetryBurst.cs:6-7`; `ControlService.cs:108-139`).
- **Live sync while the window is open:**
  - **On show:** a full `RefreshAsync` of the Display codes. This can mark the monitor lost.
  - **On activation and every 3 s:** `PollAsync`, which:
    - Reads nothing during a drag or within 1 s of a write.
    - Stops quietly on the first failed read and never changes the status.
    - Skips a tick while the previous poll is still running.
  - Sources: `W/…PanelViewModel.cs:341-357`; `C/Control/ControlService.cs:150-182`.
  - An external change updates the card quietly, with no OSD.
- **After a status change to Connected:**
  - If the control set changed, the cards are rebuilt; otherwise they resync.
  - Codes that aren't cached are then read (`W/…PanelViewModel.cs:175-182`).
- **Unhandled UI exceptions** are logged and swallowed (`W/App.xaml.cs:62-66`; `A/App.axaml.cs:58-62`).

---

## 8. Core logic: the API to re-express in Rust

### 8.1 Types
- `MonitorStatus { Connecting, NotFound, NotResponding, Connected }`
- `WriteOutcome { Ok, NeedsConfirmation, Blocked, NotResponding }`
- `WriteResult(outcome, message, value?)`
- These three live at `C/Control/ControlService.cs:8-13`.
- `LinkFailure { None, NotFound, PermissionDenied, ModuleMissing, UnsupportedPort }` (`C/Ddc/LinkFailure.cs`).
- `VcpReading(current:u32, max:u32)` and `DdcException(msg)` (`C/Ddc/IMonitorLink.cs:24-26`).
- `ControlKind { Range, Enum, Action }`
- `PanelSection { Main, Advanced, Hidden }`
- `SafetyClass { Normal, ConfirmRequired, NeverWrite, ReadOnlyUntilNamed }`
- `ControlDefinition(code, name, kind, section, safety, value_labels: map<u8,String>, confirm_message?)` (`C/Profile/ControlDefinition.cs`).
- `MonitorCapabilities(model, vcp: map<u8, Option<Vec<u8>>>)`, with:
  - `supports(code)`.
  - `allows_value(code, v)`: a `None` list means continuous.
  - Source: `C/Profile/MonitorCapabilities.cs`.

### 8.2 `IMonitorLink` (per-OS; not thread-safe; used only through `DdcQueue`)
`C/Ddc/IMonitorLink.cs:4-22`.
- `try_connect() -> bool`
- `is_connected`
- `last_failure: LinkFailure`
- `read_capabilities() -> String`
- `read(code) -> VcpReading`
- `write(code, value: u32)`

All of them return `Err(Ddc)` on failure.

### 8.3 `DdcQueue` (`C/Ddc/DdcQueue.cs`)
- **One worker** and one command at a time: `run(op: FnOnce(&mut Link)->T) -> Future<T>` (`:32-38`).
- **≥50 ms gap** from the end of one operation to the start of the next (`:14,92-94`).
- **Reads and capability reads** sleep until **400 ms after the last write**. Writes are not held back, so drags stream 50 ms apart (`:15,128-133`).
- **Retries:** up to **5 attempts** on `DdcException`, 150 ms apart (`:13,16,67-79`).
- `wrote_within(window)` (`:41`).
- `dispose`: stop accepting work and wait ≤1 s for the operation in flight (`:44-50`; test `T/DdcQueueTests.cs:11`).

### 8.4 `ControlService` (`C/Control/ControlService.cs`): the only write path
**Construction** (`:35-44`):
- Load the capability cache; parse failures fall back to `profiles.synthesize_capabilities()`.
- `controls = profiles.resolve(caps)`.
- `status = Connecting`.

**Events:** `value_changed(code, value)`, `responding_changed(bool)`, `status_changed(status)`, plus `status_reason` (`:46-53`).

**Methods:**
- `start()` = `reconnect()`.
- `reconnect()` (`:68-105`):
  1. `try_connect`; on false → NotFound.
  2. **Capabilities are read once per process.** The caps are parsed, the controls re-resolved and the cache saved if it changed.
  3. Read every Range or Enum code in Main or Advanced, plus every code already cached.
  4. If every read fails → NotResponding; otherwise Connected.
  - A DdcException or FormatException → NotResponding.
- `request_reconnect()`: the retry burst (§7).
- `refresh(codes)`:
  - A no-op unless Connected.
  - If all reads fail → `lost(NotResponding)`, which sets the status and starts the burst (`:142-143,185-210`).
- `poll(codes)` (§7) and `when_quiet(quiet, limit)` (100 ms poll) (`:150-172`).
- `set(code, value, confirmed=false) -> WriteResult` (`:212-277`):
  1. Not Connected → NotResponding "Monitor not responding".
  2. `SafetyPolicy`. Range max = the cached max.
  3. **An Action saves a backup first.** If the backup fails with an IO error → Blocked "Backup failed — {Name} cancelled."
  4. Write.
  5. **Range: read back and verify.** A mismatch → Blocked "{Name} remains {actual}; the monitor did not apply {v}." Brightness adds " The current color mode may lock brightness."
  6. Enum: the cache takes the value with no read-back. Action: no cache.
  7. A write or verify failure → `lost(NotResponding)`, returning NotResponding.
  - **OK message:** Range "{Name} {v}"; Enum "{Name} → {label | 0xNN}"; Action "{Name}" (`:486-491`).
- `begin_drag(code)`, `drag_to(code, v)`, `end_drag(code, v) -> WriteResult` (`:280-367`):
  - The drag is Range-only and Allow-only; the latest value wins, with ≤1 write in flight per code.
  - The cache takes the assumed value (max defaults to 100).
  - `value_changed` is suppressed for the dragged code.
  - End: wait until idle, then `set` (verified), clear the drag, and emit the actual value.
- `step(code, delta)`: clamp to 0..max, then `set` (`:370-380`).
- `apply_preset(preset)` (§4).
- `restore_latest_backup()`:
  - Calls `set` for each code in the file, with no confirm, and stops on the first failure.
  - "No backup found." / "Backup restored" (`:399-410`).
- `reload_profile()` (after the Finder saves) and `try_get_cached(code)`.

**Cache rule:** `value_changed` fires only when the value changed and the code isn't being dragged (`:452-461`).

### 8.5 `SafetyPolicy::evaluate(def?, caps, code, value, max?, confirmed) -> (Verdict, value, message)`
`C/Control/SafetyPolicy.cs:12-38`. The checks run in this order:

1. No definition, or the code isn't in the caps → Block "This monitor does not support 0xNN."
2. NeverWrite → Block "{Name} is never written." Only 0xCA (OSD enable) has this class.
3. ReadOnlyUntilNamed → Block "Unknown setting — identify it with the finder first."
4. By kind:
   - Action → value forced to 1. The firmware bytes in the 0x04 caps list are ignored.
   - Range → `min(value, max ?? 100)`.
   - Enum not in the caps list → Block "0xVV is not a valid value for {Name}."
5. ConfirmRequired and not confirmed → NeedsConfirmation(`confirm ?? "Change {Name}?"`).
6. Otherwise Allow.

Tests: `T/SafetyPolicyTests.cs`.

### 8.6 Profile (`C/Profile/p32p20.profile.json`, `ProfileStore.cs`)
- **Embedded profile:** edidId `LEN62A2`, plus these entries (`json:4-45`):

  | Code | Name | Kind | Section | Safety |
  |---|---|---|---|---|
  | 0x10 | Brightness | Range | Main | Normal |
  | 0x12 | Contrast | Range | Main | Normal |
  | 0x14 | Colour mode | Enum | Main | Normal |
  | 0x16 | Red gain | Range | Main | Normal |
  | 0x18 | Green gain | Range | Main | Normal |
  | 0x1A | Blue gain | Range | Main | Normal |
  | 0x60 | Input | Enum | Main | Normal |
  | 0xE0 | Over Drive | Enum | Advanced | Normal |
  | 0xEA | Dynamic Contrast | Enum | Advanced | Normal |
  | 0xEC | USB Charging | Enum | Advanced | Normal |
  | 0x86 | Aspect ratio | Enum | Advanced | Normal |
  | 0xCC | Menu language | Enum | Advanced | Normal |
  | 0x04 | Factory reset | Action | Advanced | ConfirmRequired |
  | 0x05 | Reset brightness and contrast | Action | Advanced | ConfirmRequired |
  | 0x08 | Reset colour | Action | Advanced | ConfirmRequired |
  | 0xCA | OSD enable | Enum | Hidden | NeverWrite |
  | 0xD6 | (see §11) | | | |

  - The value labels are as in §1.3 and §1.4.
  - The profile's confirm messages are fallbacks; the UI's dialog texts (§1.8) take precedence.
- `resolve(caps)` (`:30-40`):
  - Returns a definition for **every code in the caps**.
  - The overlay wins over the embedded entry.
  - Unknown codes → `"Unknown 0xNN"`, Enum, Hidden, ReadOnlyUntilNamed.
- `synthesize_capabilities()` (`:46-59`): Range → continuous; Enum → its label keys; Action → `[1]`.
- `save_overlay(def)`: replaces the entry, sorted by code (`:62-67`).
- **Real capability string** (test fixture `T/FakeMonitorLink.cs:8-13`): `…vcp(02 04(4C 42 31 2E 32) 05 08 10 12 14(01 05 06 08 0B 0E 0F 12) 16 18 1A 52 60(11 0F 31) 86(02 05) AC AE B2 B6 C6(…17 bytes) C8 C9 CA(01 02) CC(02 03 04 05 06 09 0A 0D) D6(01 04 05) DF E0(00 01 02) EA(00 01) EB(00 01) EC(00 01) F6(07) F7(…) F8(…) FA(10 12 EA 14))`.
  - The codes not in the profile (02, 52, AC, AE, B2, B6, C6, C8, C9, DF, EB, F6, F7, F8, FA) are what the Finder snapshots.

### 8.7 `CapabilitiesParser::parse(s) -> Result<MonitorCapabilities, Format>` (`C/Profile/CapabilitiesParser.cs`)
- Finds the first `vcp(` and `model(` at depth ≤1, with a word boundary before the name (`:14-29`).
- The vcp section is a sequence of two-digit hex codes, each optionally followed by `( hex… )` (`:42-75`). Spaces are optional (test `Parses_tokens_without_spaces`).
- A missing vcp section or unbalanced parentheses → error.

### 8.8 `VendorCodeFinder` (`C/Finder/VendorCodeFinder.cs`)
- `snapshot(control) -> map<u8,u32>`: a fresh `refresh` of the Hidden codes that are not NeverWrite, then read from the cache.
- `diff(before, after) -> Vec<(code, before, after)>`: only keys present in both maps, sorted by code.

### 8.9 `DdcPacket`, shared by the Linux and macOS links (`C/Mccs/DdcPacket.cs`)
- **Constants:** address `0x37`, host source `0x51`, length flag `0x80`.
- **Opcodes:** Get 0x01 / reply 0x02; Set 0x03; Capabilities 0xF3 / reply 0xE3.
- **Frames:**
  - `get_vcp(code, include_src=true)` = `[0x82, 0x01, code, chk]`.
  - `set_vcp(code, u16)` = `[0x84, 0x03, code, hi, lo, chk]`.
  - `caps_request(offset)` = `[0x83, 0xF3, hi, lo, chk]`.
  - The 0x51 byte is prepended by the transport.
- **Request checksum** = `0x6E ^ 0x51 ^ bytes`; the legacy form omits 0x51.
- **Reply checksum** = `0x50 ^ bytes` (`:53-69`).
- **VCP reply:**
  - Byte layout: `[6E, 88, 02, result, code, type, maxHi, maxLo, curHi, curLo, chk]`.
  - It is rejected when result ≠0, the code differs, or the checksum is bad.
- **Capabilities fragment:**
  - `[6E, 80|len, E3, offHi, offLo, data…, chk]`; the offset must match.
  - Empty data means the end of the string.
- Pinned bytes: `T/CrossPlatformTests.cs:14-46`. For example, `GetVcp(0x10)` = `82 01 10 AC`, legacy `…FD`; `SetVcp(0x10,50)` = `84 03 10 00 32 9A`.

### 8.10 Settings, log and caches
See §5.

---

## 9. Per-OS DDC backends and setup

### 9.1 Windows: dxva2 (`DW/Dxva2MonitorLink.cs`)
- **Connect** (`:18-54`):
  1. `EnumDisplayMonitors`, then `GetPhysicalMonitorsFromHMONITOR` for each.
  2. Match `EnumDisplayDevices(EDD_GET_DEVICE_INTERFACE_NAME).DeviceID` containing `LEN62A2`.
  3. Fall back to a physical description containing `P32p-20`.
  4. **Keep one handle; destroy the rest.**
- **Calls:**
  - Capabilities: `GetCapabilitiesStringLength`, then `CapabilitiesRequestAndCapabilitiesReply` (ASCII, trailing NULs trimmed).
  - Read: `GetVCPFeatureAndVCPFeatureReply`.
  - Write: `SetVCPFeature`.
  - Errors carry `GetLastWin32Error` (`:56-87`).
- `LastFailure` is NotFound whenever disconnected; there are no other reasons (`:16`).

### 9.2 Linux: i2c-dev (`DL/`)
- **Bus discovery** (`DL/I2cBusLocator.cs:22-45`):
  - Candidates are the `/dev/i2c-*` nodes. Buses linked from `/sys/class/drm/*/ddc` come first, then the rest by bus number.
  - Adapters whose names start with `SMBus`, `i2c-designware`, `Synopsys DesignWare`, `AMDGPU SMU`, `smu`, `soc:i2cdsi`, `mac-io` or `u4` are skipped, unless a drm link names them (`:13-14`).
- **EDID** (`DL/I2cBusLocator.cs:89-122`; `DL/Edid.cs:13-33`):
  - Open, `ioctl(I2C_SLAVE 0x0703, 0x50)`, write `00`, read 128 bytes.
  - Match the header plus a PNP id of `LEN` (bytes 8–9, big-endian 5-bit letters) and product `0x62A2` (bytes 10–11, little-endian).
- **Connect** (`DL/I2cMonitorLink.cs:39-98`):
  - No `/dev/i2c-*` at all → **ModuleMissing**.
  - Several matching buses (MST): keep the first that answers a brightness probe (3 tries), else the first that opened.
  - Nothing found, with EACCES or EPERM seen → **PermissionDenied**; otherwise NotFound.
- **Slave select** uses `I2C_SLAVE`, falling back to `I2C_SLAVE_FORCE (0x0706)` on EBUSY (`DL/Libc.cs:15-16`, `SetSlave` about `:58-68`).
- **Transactions** (`DL/I2cMonitorLink.cs:18-28,100-221`):
  - Write `[0x51, payload…]` to 0x37, sleep **50 ms**, then read a fixed length: 11 bytes for VCP, 38 for a capabilities fragment.
  - Attempts: reads 10, writes 4, capability fragments 8, probe 3; 50 ms apart.
  - Capabilities: at most 1024 bytes, with a 50 ms gap between fragments.
  - A valid "unsupported" reply (result 1) throws at once (`:272-279`).
- **Advisory `flock(LOCK_EX|LOCK_NB)` on the device fd** for each transaction:
  - It interoperates with ddcutil.
  - It polls for up to 2 s, then fails "{bus} is busy" (`:248-261`).
  - If flock isn't supported, it carries on without the lock.
- **ENODEV/ENOENT** stop the retries. If the last error was a bus error (ENXIO, EIO, EREMOTEIO, ENODEV, ENOENT), the fd is released and LastFailure becomes NotFound, so the next connect re-scans; bus numbers change after a dock replug (`:282-306`).
- **udev rule** (`packaging/linux/60-thinkvision-i2c.rules:13`): `KERNEL=="i2c-[0-9]*", SUBSYSTEM=="i2c-dev", ATTRS{class}=="0x03*", TAG+="uaccess"`.
  - The broader fallback without the class match is commented out (`:17`).
- **Setup hints** (Avalonia "System setup") (`A/Platform/Linux/LinuxSystemSetup.cs:22-91`). Each hint has a stable id:
  - `linux-i2c-permission` (on PermissionDenied):
    - Title "Allow access to monitor controls".
    - Copy text: `echo '<the rule>' | sudo tee /etc/udev/rules.d/60-thinkvision-i2c.rules`, then `sudo udevadm control --reload && sudo udevadm trigger`.
  - `linux-i2c-module` (on ModuleMissing):
    - Title "Load the i2c-dev kernel module".
    - Copy text: `sudo modprobe i2c-dev`, then `echo i2c-dev | sudo tee /etc/modules-load.d/i2c-dev.conf`.
  - `linux-powerdevil`:
    - Shown when a process named `org_kde_powerdevil` runs without `POWERDEVIL_NO_DDCUTIL` set to a non-zero value. Detection scans `/proc/*/comm`, `cmdline` and `environ` (`:104-149`).
    - Copy text: a systemd user drop-in that sets `POWERDEVIL_NO_DDCUTIL=1`, then a daemon-reload and a restart.
  - `linux-gnome-appindicator`:
    - Shown on GNOME (`XDG_CURRENT_DESKTOP` contains GNOME) when there is no SNI host.
    - Ubuntu (`ID` / `ID_LIKE` in `/etc/os-release`): `gnome-extensions enable ubuntu-appindicators@ubuntu.com`.
    - Elsewhere: a dnf install plus enable `appindicatorsupport@rgcjonas.gmail.com`.
  - `linux-hotkeys` (when hotkeys are unavailable):
    - Lists the CLI commands to bind as desktop shortcuts: `{cmd} --step brightness=+5    # Brightness up`, and so on, 11 lines (`:78-91`).
    - `cmd` is the full exe path (single-quoted if it contains a space), or `thinkvision-control` under `dotnet` (`:97-101`).
- **Linux OSD on Wayland** (`A/Platform/Linux/NotificationOsd.cs`):
  - Sends `org.freedesktop.Notifications.Notify`. App name "ThinkVision Control", icon `video-display`, summary = the app name.
  - Body: the message, or "{Label} {value}".
  - Hints: `transient`, `urgency 0`, `desktop-entry thinkvision-control`, and `value` (as a percentage) for levels.
  - Timeout 1500 ms. It **replaces the previous notification** (replaces_id).
  - `Hidden` fires 1.5 s after the last show.
- **Linux theme** (`A/Platform/Linux/LinuxThemeInfo.cs`): read from the portal's `org.freedesktop.appearance`:
  - `color-scheme`: 1 = dark, 2 = light.
  - `accent-color`: RGB doubles in 0–1; out of range means none.
  - It follows `SettingChanged`.
- **The app always runs on X11 or XWayland** (`A/Program.cs:15-17`).

### 9.3 macOS: IOAVService, Apple Silicon (`DM/`)
- **Target display:**
  - `CGGetOnlineDisplayList` (16 at most); not built-in, with vendor `0x30AE` ("LEN" packed) and model `0x62A2` (`DM/MacDdcRules.cs:55-56`; `A/Platform/LinkFactory.cs:24`).
- **Service lookup** (`DM/AvServiceLocator.cs:49-147`):
  1. Get the display's `IODisplayLocation` from CoreDisplay (`CoreDisplay_DisplayCreateInfoDictionary`; Frameworks first, then PrivateFrameworks; `DM/Native/CoreDisplay.cs:15-22`).
  2. Recursively walk the IOService plane. Remember the last framebuffer seen (`AppleCLCD2`, `IOMobileFramebufferShim`, or anything that conforms to `IOMobileFramebuffer`) and collect the `DCPAVServiceProxy` entries with `Location=External`.
  3. Choose a service (`DM/MacDdcRules.cs:98-118`):
     1. The service after the framebuffer whose registry id equals the display's location.
     2. Else a unique framebuffer whose `DisplayAttributes.ProductAttributes` match. Two identical monitors are ambiguous, so this step gives up.
     3. Else the single external proxy.
  4. `IOAVServiceCreateWithService`.
- **Port:**
  - The proxy's parent has `EPICProviderClass=AppleDCPMCDP29XX`, or `Transport.Downstream=HDMI` → HDMI (`:83-91`).
- **I/O** (`DM/IoAvMonitorLink.cs:150-219`):
  - **Every request is written twice**, each write preceded by 10 ms, through `IOAVServiceWriteI2C(chip, 0x51, payload)`.
  - Then a wait, then `IOAVServiceReadI2C(chip, offset, len)`.
  - 5 attempts, 20 ms apart.
- **Transports** (`DM/MacDdcRules.cs:30-37`):

  | Path | Chip | Read offset | VCP reply | Wait |
  |---|---|---|---|---|
  | Standard | 0x37 | 0 | 11 bytes | 50 ms |
  | MCDP29xx (built-in HDMI on M1 and base M2) | 0xB7 | 0x51 | 12 bytes | 100 ms |

  - Capability fragments are 38 bytes, up to 1024 bytes in all; the string is cut at the first NUL.
- **Connect** (`DM/IoAvMonitorLink.cs:39-92`):
  - Verify with a read of 0x10, trying the transports in order. The MCDP path goes first when the hardware hints at it.
  - **The checksum form is negotiated:**
    - The spec form (0x51 included) is tried first.
    - Only when no form is known yet and the reply was invalid (not an I/O error) does it fall back to the legacy form.
    - The working form is remembered (`:150-171`; `MacDdcRules.cs:76-77`).
    - A write uses the remembered form; the spec form when none is known.
  - No reply and HDMI → **UnsupportedPort**: the link is released.
  - No reply on another port → **stay connected**, so commands report NotResponding.
- **The whole macOS path is unverified on hardware** (`DM/README.md:16-29`).
- **Setup hint** (`A/Platform/MacOS/MacSystemSetup.cs:14-23`), on UnsupportedPort:
  - `macos-unsupported-port`, "Use a USB-C or Thunderbolt port for this monitor", with no copy text.

---

## 10. Packaging and scripts
- **Windows installer** (`packaging/windows/ThinkVisionControl.iss`):
  - **Identity:**
    - AppId `{C040F988-B1EE-407E-AAC8-81D5912B5E47}` (`:17`).
    - Default directory `{autopf}\ThinkVision Control`.
    - Admin rights by default; `/CURRENTUSER` installs per user (`:25-29`).
    - x64 only, Windows 10 1809 or later (`:30-32`).
  - **Close-app prompt:** `AppMutex=ThinkVisionControl.SingleInstance` with `CloseApplications=yes` (`:38-39`). The port must keep this mutex name or update the installer.
  - **Tasks** (`:45-48`):
    - Download and install the .NET 10 Desktop Runtime. It is offered only when missing (a check for `{commonpf64}\dotnet\shared\Microsoft.WindowsDesktop.App\10.*`) and is ticked by default.
    - Start menu shortcut.
    - Desktop shortcut.
    - **The Rust port needs no runtime**, so this task and its code (`:67-140`) disappear.
  - **InstallDelete** removes the portable 0.0.1 and 0.0.2 leftovers: `{userprograms}\ThinkVision Control.lnk` and `%TEMP%\.net\ThinkVisionControl` (`:53-57`).
  - **Post-install launch** runs `runasoriginaluser` (`:63-65`).
  - **Uninstall** deletes the HKCU Run value `ThinkVisionControl` (`:152-157`). User data is kept.
- `scripts/make-windows-setup.ps1`:
  1. Read the version from `Directory.Build.props`.
  2. `dotnet publish` the WPF app to `build\win-x64` (framework-dependent, R2R, win-x64; `W/ThinkVisionControl.csproj:16-21`).
  3. Run ISCC, which writes `build\ThinkVisionControl-Setup-<ver>.exe`.
- **Linux:**
  - `packaging/linux/thinkvision-control.desktop`:
    - `Exec=thinkvision-control`, `Icon=thinkvision-control`, categories Settings and HardwareSettings.
    - **Actions:** Day (`--preset day`), Night (`--preset night`), SwitchToMe ("Switch monitor to this computer", `--input this`).
  - The udev rule is in §9.2.
  - Installation is manual, per the README (`README.md:179-188`): publish to `~/.local/share/thinkvision-control`, a symlink in `~/.local/bin`, the .desktop file, and the SVG icon in hicolor.
  - There is no tarball script in the repo, although the plan mentions one.
- **macOS** (`scripts/make-macos-app.sh`):
  1. Self-contained `osx-arm64` publish.
  2. Assemble `build/ThinkVision Control.app`. The executable is `thinkvision-control` (`A/ThinkVisionControl.Avalonia.csproj:7`).
  3. Info.plist:
     - `CFBundleIdentifier com.thinkvisioncontrol.app`.
     - `LSUIElement=true` (menu-bar app, no Dock icon; also `ShowInDock=false` at `A/Program.cs:19`).
     - `LSMinimumSystemVersion 13.0`, `NSHighResolutionCapable`.
     - Icon `ThinkVisionControl.icns`.
  4. `codesign --force --deep -s -` (ad hoc).
  - **`VERSION="0.0.2"` is hard-coded and stale** (`:13`).
- **Assets:** the `assets/brand/` icons (ico, icns, png, svg) and the tray icons `Tray.OnLight.ico` / `Tray.OnDark.ico`. The Avalonia app bundles the FluentSystemIcons font (MIT; the licence must be kept if the font is reused).

---

## 11. Not ported: power control (VCP 0xD6)
Every place it appears, so the port can drop it cleanly:
- **Profile entry:** `C/Profile/p32p20.profile.json:36-38`.
  - "Power", Enum, Advanced, **ConfirmRequired**.
  - Values 01 On, 04 Standby, 05 Off.
  - Confirm text "Turn the monitor off? You may need the monitor's power button to turn it back on."
- **Capability string:** `D6(01 04 05)` (`T/FakeMonitorLink.cs:11`).
- **⚠ Port hazard:** if the 0xD6 entry is simply deleted, `ProfileStore.Resolve` turns it into "Unknown 0xD6", Hidden, ReadOnlyUntilNamed. It would then appear in Finder snapshots, and a user could name it and so make it writable. **Mark 0xD6 NeverWrite, or exclude it from Resolve and the Finder.**
- **Today 0xD6 is read on every connect,** because it is an Enum in Advanced (`C/Control/ControlService.cs:94-97`).
  - It is excluded from backups, being ConfirmRequired (`:446-447`).
- **WPF view model** (`W/Ui/PanelViewModel.cs`):
  - The `Power` const and `PowerStandby` / `PowerOff` (`:51-52`).
  - `PowerItem` (`:246,275`); `HasPower`, `CanStandby`, `CanTurnOff` (`:253-255`).
  - `StandbyCommand` and `TurnOffCommand` (`:98-99,256-257`).
  - Excluded from MonitorGroups (`:278`).
  - Confirm dialogs (`:444-447`):
    - "Turn off ThinkVision P32p-20?" / "You might need the monitor's power button to turn it back on." / Turn off.
    - "Put ThinkVision P32p-20 in standby?" / "You might need the monitor's power button to wake it." / Standby.
- **WPF view:** the "Power" section, card description "Put the monitor in standby or turn it off", Standby and Turn off buttons (`W/Ui/Pages/MonitorPage.xaml:36-46`).
- **Avalonia:** the same members (`A/ViewModels/MainViewModel.cs:21-22,75-76,256,264-268,288,291,464-467`), `Glyphs.Power` (`A/ViewModels/Glyphs.cs:34`), and the Power section (`A/Views/Pages/MonitorPage.axaml:34-45`).
- **Tests:** `T/SafetyPolicyTests.cs:28` (`Power_off_needs_confirmation_then_is_allowed`).
- **README:**
  - Features: "standby and power off" (`README.md:32`) and "power off all ask first" (`:40`).
  - The screenshot caption "power and resets" (`:53`).
  - The `docs/screenshots/monitor-settings.png` image shows it.

---

## 12. WPF vs Avalonia differences that matter

| Area | WPF (Windows, stable) | Avalonia (macOS, Linux, Windows preview) |
|---|---|---|
| Close in background mode | **Restart-on-close** into a fresh tray-only process (§7) | The process stays; no restart |
| BackgroundMode turned off | Tray **and** hotkeys disposed | Hotkeys suspended; **the tray stays until exit** |
| Tray menu | Open, Day, Night, Exit. **No "Switch to this PC"** | Open, (Linux: Brightness up and down), Day, Night, **Switch to this PC**, Exit |
| Tray icon | Dual-tone glyph per **taskbar** theme; badge colours follow the theme | App icon; fixed grey and amber badges |
| Single instance | Mutex + pipe; **5 s hand-off or take-over**; AllowSetForegroundWindow | Lock or mutex + pipe; 3 send tries, then quit even on failure; per-OS names (§6) |
| Link-failure info bars and the "System setup" section | – (dxva2 has only NotFound) | PermissionDenied, ModuleMissing and UnsupportedPort bars; setup cards with Copy |
| Input radio | Label only | Adds a "This PC" caption; snaps back on a no-op click |
| Toggles | Switch only | "On"/"Off" text beside the switch |
| Keycap names | Win, Alt | Cmd/Option on macOS, Super on Linux |
| Shortcut editor | Secondary button "Reset"; hotkeys always re-registered after | "Reset to default"; re-registered only if nothing was saved; per-OS hint text |
| Dialogs | Queued first in, first out | A second confirm is refused (counts as Cancel) |
| Navigation | Custom pane: collapse toggle, F6 focus switch, Esc closes the overlay | `FANavigationView` (Auto) |
| Minimum window height | 500 | 400 |
| OSD | Fade plus 12 px slide | Fade only; no shadow without a compositor; Wayland uses notifications; macOS overlay level 25 on all Spaces |
| Accent | System | Platform accent, else system, else ember `#F05A28` |
| Jump list | `System.Windows.Shell.JumpList` at idle | Raw `ICustomDestinationList` COM (`A/Platform/Windows/WindowsQuickActions.cs:35-76`); Linux has .desktop Actions; macOS has none (the menu bar serves instead) |
| Monitor events | SystemEvents (2 s debounce) + device notifications (500 ms) | One hidden window (500 ms); Linux netlink (1.5 s); macOS CG and IOKit (500 ms) |
| `OnShowing` | Refresh only | Also re-detects the setup hints |
| Finder on main-window close | Closed as an owned window | Closed explicitly |
| Hotkeys unavailable | n/a | Caption text plus a setup card (Linux) |
| Settings "Start with…" label | "Start with Windows" | "Start when you log in" on macOS and Linux |

---

## 13. Behaviour pinned by tests (`tests/ThinkVisionControl.Tests`)
- **`ControlServiceTests.cs`:**
  - Start reads the capabilities and values.
  - Step clamps; a brightness step that isn't applied is reported.
  - Range writes are verified; the Enum label is in the message.
  - Dead monitor → NotResponding and an event.
  - 0xCA and unnamed codes are never written.
  - Factory reset needs confirmation and backs up first; restore re-applies the latest backup.
  - Preset order.
  - Finder snapshot and diff.
  - Connecting before start; NotFound vs NotResponding; plugging in later and reconnecting; unplugging after connect.
  - Poll: sees an external change; reads nothing during a drag or right after a write; a failed poll keeps the monitor connected.
  - `WhenQuiet` timing.
- **`RedesignCoreTests.cs`:**
  - Without a cache, the controls come from the profile and the status is Connecting.
  - Capabilities are cached.
  - Writes are refused, and refresh does no DDC traffic, while not connected.
  - The burst makes 5 attempts, then stops; a plug-in during a burst ends it.
  - A drag coalesces writes, reads nothing, and verifies once on release.
  - A release reports a value the monitor kept.
  - CLI: format/parse round trip, both value forms, malformed input → Show, `--startup` combined with a verb.
- **`SafetyPolicyTests.cs`:** all the rules in §8.5.
- **`SettingsTests.cs`:**
  - Defaults; a corrupt file becomes `.bak`; round trip.
  - Missing BackgroundMode → true; legacy shortcut migration.
  - Gesture parse, reject and create; all defaults round-trip.
  - Latest backup with the same-second suffix; no backup → null.
- **`ProfileTests.cs`:** parses the real P32p-20 string and the no-space form; a missing vcp throws; known and unknown resolution; overlay naming; a corrupt overlay is ignored.
- **`CrossPlatformTests.cs`:**
  - DdcPacket wire bytes and reply parsing; capability fragment reassembly.
  - EDID match; bus ordering; SMBus skipping.
  - macOS vendor/model; built-in HDMI is the only unsupported port.
  - The link-failure reason is reported with the status.
- **`DdcQueueTests.cs`:** dispose doesn't wait for a stuck command.
