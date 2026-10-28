# What other open-source monitor apps can teach us

Date: 2026-10-28. Raw data: [2026-10-28-open-source-learnings.json](2026-10-28-open-source-learnings.json).

We read the source code and issue trackers of ten projects that control monitors over DDC/CI, looking for:
- features their users love that we lack;
- how they solved the problems we have (waking from standby, reconnecting, noticing what the other PC did, reliability);
- whether their code may be copied into ours.

A second pass added three questions:
- Do other projects that cover the P32p-20, or its siblings, agree with our settings map? (Group D)
- How do other projects support many monitor models, and is our architecture ready for that? (Group E)
- Your feature request: drop the monitor from this PC's desktop when you hand it to the other PC. (Group F — dropped 2026-10-28: the current behaviour is fine)
- Your request: remove the monitor power controls from the app. (Group G)

Every claim about our own code was checked against `src/`. The [Decide](#decide) table at the end lists every item for you to mark.

## The projects

| Project | Platform / stack | Licence | Can we copy code? |
|---|---|---|---|
| [Monitorian](https://github.com/emoacht/Monitorian) | Windows, C#/WPF (our stack) | MIT | Yes, with credit |
| [PowerToys Power Display](https://github.com/microsoft/PowerToys/tree/main/src/modules/powerdisplay) | Windows, C#/WinUI 3 | MIT | Yes, with credit |
| [Twinkle Tray](https://github.com/xanderfrangos/twinkle-tray) | Windows, Electron + C++ | MIT | Yes, with credit |
| [MonitorControl](https://github.com/MonitorControl/MonitorControl) | macOS, Swift | MIT | Yes, with credit |
| [Lunar](https://github.com/alin23/Lunar) | macOS, Swift | MIT (Pro modes encrypted, unreadable) | Visible files only |
| [display-switch](https://github.com/haimgel/display_switch) | Windows/macOS/Linux, Rust | MIT | Yes, with credit |
| [ddcutil](https://github.com/rockowitz/ddcutil) | Linux, C | GPL-2.0-or-later | Yes (same licence as ours) |
| KDE PowerDevil | Linux, C++ | LGPL-2.0 | Yes (converts to GPL) |
| GNOME ddcutil extension, Clight, vdu_controls | Linux | GPL-3.0 | Ideas only |
| BetterDisplay, ClickMonitorDDC, ControlMyMonitor | closed source | — | Ideas only |

Our licence is GPL-2.0-or-later. MIT code can be adapted if we keep its copyright notice; we'd list it under "Credits" in the README.

## Group A — fix what you reported (reliability)

These match the field test: the monitor coming back from standby, and the app not knowing what the other PC did.

### A1. Notice when the monitor turns off and on, not only when the PC sleeps · effort S–M
**What we do now:** `Platform/DisplayEvents.cs` listens only for display-setting changes and PC resume, then reconnects after 2 s. When the monitor alone goes to standby and wakes, we hear nothing. `RetryBurst` gives up after about 15 s.

**What the others do:**
- Monitorian and PowerToys subscribe to the "console display state" power notification (`GUID_CONSOLE_DISPLAY_STATE`: off / on / dimmed). They act only when it changes to *on*.
  - PowerToys does this with a callback that needs no window, which suits our lean tray process. See `DisplayChangeWatcher.cs` and `PowerSettingsNative.cs`.
- Monitorian also reconnects after unlock and after Remote Desktop. It retries on a long schedule: about 5, 10, 20, 30 and 60 s after resume. Some monitors take a minute to answer again.
- MonitorControl and Lunar send **nothing** to the monitor while it is asleep or the session is locked, then wait about 3 s after wake. Writing to a monitor mid-wake froze some Dell monitors (Twinkle Tray #567).

**For us:**
- Windows: add display-state and lock/unlock events. Send no monitor traffic while the display is off or the session is locked, including from hotkeys. Wait about 3 s after wake, then reconnect and re-read once.
- Stretch the reconnect schedule to about a minute.
- Linux: add the logind sleep/lock signals. Treat "permission denied" in the first seconds after resume as "retry", not "needs setup" (ddcutil does this).

### A2. Don't retry errors that can never succeed · effort S
**What we do now:** `DdcQueue` retries every monitor error 5 times. The Windows link doesn't look at the error code at all. A dead connection or an unsupported code costs 5 wasted attempts before we reconnect.

**What the others do:** PowerToys (`DdcErrorClassifier.cs`), Twinkle Tray and ddcutil sort errors into three groups:
- **Worth retrying:** a cable/bus error.
- **Final:** the monitor says "unsupported".
- **Connection is gone:** reconnect now.

**For us:** the same three groups, as distinct exception types. Faster recovery, and the hidden-settings finder gets quicker.

### A3. Make "Switch to this PC" sturdier · effort S
From display-switch (MIT), plus issues in SimpleKVM and ddcutil:
- **Read before writing.** Skip the write if the monitor is already on our input. This avoids a black flash.
- **Never retry an input write.** The link often drops mid-switch, so a retry can switch twice. Confirm with a delayed re-read instead.
- **Ignore the high byte of the input reply.** Some monitors put junk there.
- **Wake the monitor first.** A 1-pixel mouse nudge on Windows; `caffeinate -u` on macOS.

Test needed on your setup: does the P32p-20 accept the switch command from the PC it is *not* currently showing?

### A4. Know what the other PC did, without constant polling · effort S
We now poll only while the window is open. Cheap additions:
- Re-read once when the monitor wakes (from A1).
- Re-read before a hotkey step if our last reading is older than about 30 s. Otherwise "+10%" starts from a stale number (MonitorControl #1064).
- Possibly re-read when the mouse hovers over the tray icon, at most every 5 s (Twinkle Tray).
- **Worth a 5-minute test on the real monitor:** the DDC standard has a "something changed" flag (VCP `0x02`), and the P32p-20 advertises it. If the monitor sets it after an OSD or other-PC change, one 1-byte read can tell us whether anything changed. No other project uses it this way, so it's unproven.

### A5. Linux: warn about KDE fighting us · effort S
KDE Plasma 6 drives external-monitor brightness itself and can reset it at login. Detect it and point to the existing README fix (`POWERDEVIL_NO_DDCUTIL=1`).

## Group B — features users ask for most

| # | Feature | Who has it / asks for it | How they do it | Effort |
|---|---|---|---|---|
| B1 | **Day/Night automatically**, at set times or sunrise/sunset, catching up after sleep | Most-asked in Twinkle Tray (#252, #73), Monitorian (#107), PowerToys (#51137); Lunar, vdu_controls | Every 60 s, pick the latest event at or before now; re-check on wake. Sun maths: Lunar `Solar.swift` (MIT) or SunCalc (BSD). Location typed by the user, so no permission prompt | M |
| B2 | **Keyboard brightness keys** control the external monitor | Twinkle Tray's top request (#291, 31 votes); MonitorControl on Mac | Windows raw keyboard input for the brightness keys, active only when enabled; Mac media-key tap | S–M |
| B3 | **Scroll on the tray icon** to change brightness | Monitorian #174, PowerToys #51376 | Low-level mouse hook switched on only while the pointer is over the icon (`Shell_NotifyIconGetRect`). No extra window, so the tray stays lean | M |
| B4 | **Switch input automatically when a USB device connects** (a USB switch, or keyboard moved to this PC) | display-switch, SimpleKVM (863★), BetterDisplay #3033 | Watch USB plug events; on connect, switch to this PC's input. Each PC runs it, "towards itself" | M |
| B5 | **Better command line**: `--get brightness`, `--json`, clear exit codes, a `thinkvision://` link for Stream Deck/Raycast | PowerToys CLI, ControlMyMonitor, BetterDisplay | PowerToys `CliExitCodes.cs`: 0 OK, 1 not found, 2 out of range, 4 unsupported, 5 hardware, 8 timeout | S |
| B6 | **Darker than 0%** with a screen overlay at night | Twinkle Tray #695, MonitorControl, Lunar | Below 0, keep the monitor at 0 and dim with a click-through overlay. Avoid gamma ramps on Windows; they fight Night Light | M |
| B7 | **Optional "restore my settings after wake"** for monitors that forget | Monitorian #115 (top non-bug request), Lunar, MonitorControl | Opt-in, delayed, never while the display is off | S |
| B8 | Per-app presets (e.g. a game or photo editor) | Twinkle Tray #348, Lunar | Foreground-window hook; restore when leaving the app | M |
| B9 | Dim when idle | Twinkle Tray, Clight | Idle check every 5 s; skip during fullscreen or video | M |
| B10 | Fine 1% steps with a modifier key | MonitorControl (Option+Shift) | Extra hotkey modifier | S |

## Group C — cheap safety nets

- **Crash marker around risky monitor operations** (PowerToys `CrashDetectionScope`, Twinkle Tray): write a file before reading capabilities or running the hidden-settings finder. If it is still there at the next start, the app crashed, so start in safe mode. S.
- **Safe-mode start** (hold Shift at launch, MonitorControl): default settings, no monitor writes. S.
- **Mention graphics-driver updates** in the "isn't answering" message. An AMD driver (Adrenalin 26.1.1) broke DDC for Monitorian and Twinkle Tray users. S.
- **Per-code health score** (Lunar): codes that keep failing or are slow get skipped, and feed the "not responding" banner. M.

## What not to copy

- **Gamma-ramp dimming on Windows:** clashes with Night Light and f.lux. Use an overlay instead.
- **Smooth brightness fades:** on slow monitors DDC calls block. Lunar's FAQ: "DDC commands pause the system".
- **Brightness calibration curves:** Twinkle Tray silently disabled theirs because it didn't work (#860).
- **Retrying the power-off (`0xD6`) or input write:** has left monitors stuck (Twinkle Tray #432, PowerToys #50449).
- **Code from GPL-3 projects** (GNOME extension, Clight, vdu_controls) and Lunar's unreadable Pro modes. Ideas only.
- **The private macOS OSD:** broke on macOS Tahoe for MonitorControl. Keep our own indicator.

## Group D — are we right about the P32p-20? (cross-check)

**Nobody else maps this monitor.** There is no ddccontrol-db entry for `LEN62A2`, no ddcutil quirk, no public capability string except ours, and no scripts. The only public traces are:
- a brightness offset in [moco](https://github.com/magcode/mqtt-tools/blob/HEAD/moco/configuration-sample.json);
- one Sway config note;
- EDID dumps;
- Lenovo forum threads.

We compared our map against the same-generation siblings: P27h-20, P40w-20, E27q-20 and P24h-20.

**Where they agree with us:**
- **Input codes:** 0x11 HDMI, 0x0F DP and 0x31 USB-C match the P40w-20, the P24h-20 and BetterDisplay.
- **Menu-language codes and the power values** (01 on, 04 standby, 05 off) match.
- **0xCA (OSD lock):** we never write it, which is safer than the sibling XML that exposes it.
- **Over Drive (E0) and Dynamic Contrast (EA)** exist on every sibling.

**Where they disagree:**

| # | Setting | We say | They say | Who's likely right | 5-minute test on your monitor |
|---|---|---|---|---|---|
| D1 | **Colour mode (0x14)** | 01 DCI-P3, 05 Bluish, 06 Neutral, 08 Low Blue Light, 0B sRGB, 0E Reddish, 0F BT.709, 12 Custom | Standard and siblings (E27q-20, Lenovo's own LDCC names): 01 sRGB, 05 Reddish, 06 Neutral, 08 Bluish, 0B Custom | **Us — verified.** You checked every mode against the monitor's OSD on 2026-10-28 and the names match. The siblings simply use different values | Done |
| D2 | **Aspect ratio (0x86)** | 02 Full screen, 05 Original | Standard: 02 keeps proportions (= Original), 05 stretches (= Full screen) | Unclear, possibly swapped | Set a 4:3 or 16:10 resolution and toggle both |
| D3 | **Which modes lock brightness** | sRGB, DCI-P3, BT.709 lock brightness | P24h-20 owner: those modes lock brightness *and contrast*. ddcutil notes: Dynamic Contrast and Low Blue Light also lock brightness | Them (more evidence) | In each mode and with Dynamic Contrast on, change brightness and contrast and watch the app's "kept old value" message |

**What they know that we don't:**
- **The P32p-20 has a built-in KVM on firmware LB1.2 (yours), not on LB1.0.** It's set up in Lenovo's LDCC app: you assign which input belongs to PC1 and PC2, plug the keyboard and mouse into the monitor's USB ports, and a hotkey switches both ([Lenovo forum](https://forums.lenovo.com/t5/Displays-Options-and-Accessories/KVM-P32P-20/m-p/5101858)). The unknown codes **F7/F8** (two identical lists) may be this PC1/PC2 table, and **FA** looks like a list of controls a mode affects. Treat both as read-only.
- **USB Source Selection trade-off:** USB 3.0 halves the USB-C display lanes, so 4K runs at 30 Hz; USB 2.0 gives 4K@60.
- **Standby over USB-C** sometimes needs a power cycle to wake (P32p-20 owner). This connects to A1.
- **Nobody has shown input switching from the PC the monitor isn't showing** on any -20 model. Scripts always switch from the PC on screen. That makes the A3 hardware test more important.
- **0x62** (reads 50) is plausibly the level of the 3.5 mm audio output, since the monitor declares audio over HDMI/DP.

## Group E — supporting more monitor models

### How others do it

| Project | Approach | Model data | Licence |
|---|---|---|---|
| ddccontrol-db | Per-model XML files building on a VESA base | 460 models, mostly Dell, Samsung and old CRTs; only 10 Lenovo, **no P32p-20**. New entries comment out inputs, colour modes and vendor codes as "unverified" | GPL-2.0 only |
| ddcutil | Generic: a built-in standard name table plus optional per-model `.mccs` files written by users | None shipped | GPL-2.0-or-later |
| PowerToys Power Display | Generic: reads what the monitor advertises, names values from a standard table (`VcpNames.cs`), lets users rename values per monitor | None | MIT |
| Twinkle Tray | Generic, plus a 9-line quirks file (alternative brightness code, "don't re-apply on wake") | None | MIT |
| MonitorControl, Lunar | Generic, plus vendor quirks in code (e.g. LG input switching uses a different address and code) | None | MIT |
| vdu_controls | One config file per monitor (model + serial) with overrides | None | GPL-3.0 (ideas only) |
| Dell / LG / Lenovo / Samsung tools | Closed, brand-only, built-in list of series | — | Proprietary |

**Pattern:** every successful tool is **generic first**. It reads what the monitor advertises, names it from the standard, and adds small per-model or per-vendor corrections on top. No tool relies on a big model database.

### Should we copy a database? **No.**
- **ddccontrol-db** lacks our monitor, is thin on Lenovo, and leaves out exactly the parts that need verifying (inputs, colour modes). Bundling it would also make our program GPL-2.0-only. Use it only as a reference when writing a profile by hand.
- **We should copy a standard name table** for the generic layer. PowerToys `VcpNames.cs` is MIT and already C#; ddcutil's table is GPL-2.0-or-later and more complete but in C. Treat standard names as fallback labels only, because vendors break them: our 0x31 is USB-C, which PowerToys calls something else.

### Is our architecture ready?
**Already generic:**
- the control model with safety classes (same shape as ddcutil and ddccontrol);
- the user overlay, and "unknown codes are hidden and read-only";
- the capability parser and the safety policy;
- the hidden-settings finder;
- the Linux and macOS monitor matching, which already takes a model ID as a parameter.

**Blocks other models (hard-coded to the P32p-20):**
- **Profiles:** one embedded profile, one overlay file (`ProfileStore.cs`).
- **Input codes 0x11/0x0F/0x31** in `App.xaml.cs`, `App.axaml.cs`, `CommandLine.cs` and both view models. The shortcut list itself has `InputHdmi/InputDp/InputUsbC` (`AppSettings.cs`).
- **Colour-mode list:** it shows only the 8 Lenovo values (`ColourModeOrder` in `PanelViewModel.cs:56,325`, `MainViewModel.cs:26`). **Another monitor's colour modes would disappear. This is a bug to fix before adding any model.**
- **Model name strings** ("ThinkVision P32p-20") in the tray, the status text and the link defaults.
- **Descriptions and icons** for Over Drive, Dynamic Contrast and the other monitor-specific controls, which live in the view models.
- **One saved "this PC's input", global Day/Night presets** (the colour mode in a preset only makes sense for one model), and one capabilities cache, backup and overlay.
- **Timing** (400 ms after a write) is a constant measured on this monitor.
- **One monitor at a time** throughout.

**Path to more models, cheapest and most necessary first:**

| Step | Change | Effort |
|---|---|---|
| E1 | Profile registry: `profiles/<model-id>.json` plus a `generic.json` built from the standard table. Unknown models get brightness, contrast, RGB, input and colour mode with standard names; resets ask first; power (`0xD6`) and OSD lock (`0xCA`) are never written (see G); vendor codes stay hidden. Profile gains display name, value order, descriptions/icons and input kinds | S |
| E2 | Move every hard-coded P32p-20 value from the view models and CLI into the profile; share that logic between the WPF and Avalonia apps; fix the colour-mode filter | S–M |
| E3 | Per-monitor data folder: capabilities cache, backups, overlay, "this PC's input", presets | S |
| E4 | Data-driven inputs and shortcuts: one "switch to input X" shortcut type instead of three fixed ones; CLI `--input <name or 0xNN>`; migrate existing settings | M |
| E5 | Small quirks block in the profile: timing, write-only monitors, power-off value, capability override | S |
| E6 | Find any DDC monitor (not only `LEN62A2`); identify by model ID + serial; pick one monitor to control | M (3 platforms) |
| E7 | Several monitors at once: one queue per monitor, a monitor picker, per-monitor shortcuts, OSD shows which monitor | L |
| E8 | "Export profile" from the finder so owners of other models can contribute one by pull request (with ddccontrol-db's "conservative profile" rules) | S |

Recommendation: E1–E5 make the code model-neutral without changing what you see. E6 opens it to other monitors. E7 can wait, as long as E1–E6 key everything by monitor.

## Group F — your feature: "PC screen only" when you hand the monitor over

> **Dropped (2026-10-28).** You decided the current behaviour is fine. The design below is kept for reference only.

**Request:** on the HDMI laptop, switching the monitor to DisplayPort (to the other PC) should also make Windows drop the monitor from the laptop's desktop, so every window comes back to the laptop screen. Switching back should restore the extended desktop.

**It works on Windows.** Prior art: [Sunshine's libdisplaydevice](https://github.com/LizardByte/libdisplaydevice) (MIT) changes topology for a session and reverts it.

**How:**
- **Switch away:**
  1. Write the new input first, while the app can still talk to the monitor.
  2. Save the current layout.
  3. Turn off only the P32p-20's display path, as a *temporary* change (`QueryDisplayConfig` → clear that path → `SetDisplayConfig` without saving to the database). This works on laptops and on desktops with several monitors.
- **Switch to this PC:**
  1. Restore the saved layout (`SetDisplayConfig(SDC_USE_DATABASE_CURRENT)`, with the saved snapshot as a fallback).
  2. Wait for the monitor to reappear, usually 1–3 s, giving up after 10 s.
  3. Reconnect and write this PC's input.
- **Why that order:** while the monitor is off the desktop, Windows gives the app no way to talk to it. Switching back from that PC must re-add the monitor first.
- **Avoid:** `DisplaySwitch.exe`, whose arguments broke in Windows 11 22H2, and `SDC_TOPOLOGY_INTERNAL`, which drops *every* external screen and is undefined on desktops.

**Pitfalls:**
- **The monitor's own Input button:** if you switch back with it, the laptop is sending nothing, so the screen says "No signal". The app probably gets no event in that case (needs a test). Recovery is the "Switch to this PC" shortcut, the tray item, or Win+P → Extend.
  - Alternative: *mirror* instead of removing. The monitor stays on the desktop, so DDC keeps working and there's never "No signal", but both screens share one resolution and the laptop shows the same picture.
- **Windows may put the monitor back** on its own after sleep or another plug event. While "away", the app re-checks and removes it again.
- **Window positions:** Windows 11's "Remember window locations based on monitor connection" should move windows back on return. Untested after a programmatic change.
- **Lid closed or no other screen:** if the P32p-20 is the only active screen, the app only switches the input and leaves the layout alone.
- **App status:** the app needs a new state, "Handed to DisplayPort", instead of showing "Not connected". Today the shortcut handler refuses commands unless the monitor is connected (`App.xaml.cs:285`), so this needs a bypass.

**Other systems:**
- **Linux:** feasible, but needs one backend per desktop: GNOME `ApplyMonitorsConfig`, KDE `kscreen-doctor`, X11 `xrandr`, wlroots `wlr-randr`. DDC probably keeps working with the output off, so the re-add step may not be needed.
- **macOS:** no public way to remove a display. Mirroring only, or skip.

**Setting:** opt-in, off by default: "When I hand the monitor to another input, remove it from this PC's desktop". Windows first.

**Hardware tests needed:**
1. Can Windows re-add the monitor while it's showing DisplayPort, and how long does it take?
2. Does the switch-back command work right after re-adding?
3. Does pressing the monitor's Input button raise any Windows event?
4. Does sleep or resume put the monitor back?
5. Do windows return to where they were?

## Group G — remove monitor power control (your request)

**Request:** the app should have no power options. No Standby, no Off, nothing that turns the monitor on or off.

**Why the research supports it:** turning a monitor off over DDC/CI (VCP `0xD6`) is the most common way these tools strand users:
- **Monitors that won't wake:** Twinkle Tray users report monitors that only come back with a hard power cycle ([#432](https://github.com/xanderfrangos/twinkle-tray/issues/432), [#1213](https://github.com/xanderfrangos/twinkle-tray/issues/1213)).
- **A monitor left in a bad state:** "Off (Hard)" left a Samsung G7 stuck ([PowerToys #50449](https://github.com/microsoft/PowerToys/issues/50449)).
- **Our own monitor:** a P32p-20 owner reports standby over USB-C sometimes needs a power cycle to wake.

The monitor's power button and Windows' own screen timeout already cover this, so removing it costs little.

**What we have today:**
- **WPF app:** `Ui/Pages/MonitorPage.xaml:37-41` has a "Power" card with a Standby button and Off choices. `PanelViewModel.cs` has `PowerItem`, `StandbyCommand`, `CanStandby`, the `PowerStandby`/`PowerOff` constants and the confirm dialogs (lines 52, 98, 254–256, 275, 444–447).
- **Avalonia app:** the same in `Views/Pages/MonitorPage.axaml:35-40` and `MainViewModel.cs` (lines 22, 75, 264–266, 288, 464–467), plus the `Power` icon in `Glyphs.cs` and `IconGlyphs.cs`.
- **Profile:** `p32p20.profile.json` defines `0xD6` as an "Advanced" control that needs confirmation.
- **README:** lists "standby and power off" under Features and Safety. The monitor-settings screenshot shows the Power card.
- **Not affected:** the command line and keyboard shortcuts never offered power control.

**How to remove it:**
- **Profile:** don't delete `0xD6`. Change it to `NeverWrite`, the same rule as the OSD lock `0xCA`. If it were deleted, the monitor would still advertise it, so the app would treat it as an unknown code, and someone could name it in the hidden-settings finder and make it writable again. `NeverWrite` blocks every path: UI, finder and future command-line options.
- **Both apps:** delete the Power card, `PowerItem`, `StandbyCommand`, `CanStandby` and the power confirm dialogs.
- **Generic profile (E1):** for other models, `0xD6` is `NeverWrite` too.
- **README and screenshot:** remove power from Features and "Safe by design". Retake `monitor-settings.png`.
- **Tests:** add one: a write to `0xD6` is blocked.
- **Effort:** S.

**Reading stays:** the app may still *read* `0xD6` to tell "monitor in standby" apart from "monitor not answering". That's a read only, and useful for A1.

## Hardware tests that settle several items at once

One sitting at the monitor (about 20 minutes, with both PCs) answers D2, D3, A3 and A4 (the 0x02 change flag). D1 is already settled. The app's hidden-settings finder and log already cover most of the reads. A small test script would cover the rest.

## Recommendation

Everything goes into **0.0.5**. There is no 0.0.6. The work happens in three phases, in order:

1. **Phase 1 — move to the new stack (Rust + Slint).** Rebuild the app on the new stack with a solid architecture before adding anything new. The decision and the measurements are in [2026-10-28-lightweight-ui.md](2026-10-28-lightweight-ui.md) and [2026-10-28-ui-look-comparison.md](2026-10-28-ui-look-comparison.md). Phase 2 starts only once the new build does everything 0.0.4 does.
2. **Phase 2 — reliability and cleanup:** G, A1, A2, A3, A4, C1, C3. Before coding, one sitting at the monitor with both PCs to run the D2–D3 checks and the A3 test (see above). This fixes the field-test problems and removes power control. The colour-mode names are already verified (D1).
3. **Phase 3 — B1 (automatic Day/Night) and B5 (better command line).** Both are the most useful additions for a two-PC desk, and neither adds weight to the tray.

### To-do (reminder, not a plan)

- [ ] **Phase 1:** migrate to Rust + Slint with a solid architecture; reach parity with 0.0.4 in the new build.
  - [ ] Design the new architecture so E1–E5 (model-neutral profiles, no hard-coded P32p-20 values) fit later without rework.
  - [ ] Leave out power control (G) from the start instead of porting it and removing it.
  - [ ] Background mode as proven in [lightweight-ui-probes/background/NOTES.md](lightweight-ui-probes/background/NOTES.md):
    - [ ] Tray: Slint's own `SystemTrayIcon`. Hotkeys: the `global-hotkey` crate, handled through `invoke_from_event_loop` with no polling timer.
    - [ ] Free the main window when it closes ("drop": 3 MB in the tray, against 5.8 MB if hidden). This replaces WPF's restart-on-close.
    - [ ] Keep the OSD window alive and show/hide it by switching its opacity (4.7 ms per press, against 22 ms if it's recreated, and it never steals focus). The probe measured "drop" and "keep" separately; check this mix (drop the main window, keep the OSD) once.
    - [ ] Rules: only push values into a window while it's visible (set them on open); never hide a Slint window behind Slint's back. Breaking either makes the app spin the CPU while it sits in the tray.
  - [ ] Still untested from the probe: the tray icon after Explorer restarts, OSD placement on mixed-DPI screens, the OSD briefly not click-through for ~150 ms on its very first show.
  - [ ] Size: the probe exe is 11.0 MB, just over the 10 MB goal. Trim it.
- [ ] **Hardware session** (before Phase 2): D2 aspect ratio, D3 which modes lock brightness and contrast, A3 switching from the PC the monitor isn't showing, A4 the `0x02` change flag.
- [ ] **Phase 2:** G, A1, A2, A3, A4, C1, C3.
- [ ] **Phase 3:** B1, B5.
- [ ] Release 0.0.5.

**Later**, when there is a reason:
- E1–E5 and E6 when a second monitor model is on the table. E8 after E6.
- A5 when the Linux app is tested on real hardware.
- B4 if the monitor's built-in KVM and our shortcut aren't enough.
- B10 if 10% steps feel too coarse.

**Skip:**
- B2, B3: we already have global shortcuts. Brightness keys on the laptop belong to its own screen, and a tray scroll hook costs the lean tray.
- B6, B8, B9: each needs a background hook or window running all the time for little gain on one monitor. Windows already turns the screen off when idle.
- B7: the P32p-20 hasn't been seen forgetting settings, and writing during wake is what freezes monitors. Revisit if the hardware test shows it forgets.
- C2: C1 already starts in safe mode after a crash.
- C4: A2 covers most of it at a fraction of the work.
- E7: one monitor on your desk. E1–E6 keep the door open.

## Decide

Mark each with ✅ build, ❌ skip, or ⏸ later. "Recommended" is my suggestion; "Decision" is yours.

| ID | Item | Effort | Recommended | Decision |
|---|---|---|---|---|
| G | Remove monitor power control (Standby/Off); `0xD6` becomes never-write | S | ✅ 0.0.5 phase 2 | ✅ your request |
| A1 | React to monitor wake/sleep and lock; no traffic while asleep; ~1 min reconnect | S–M | ✅ 0.0.5 phase 2 | |
| A2 | Don't retry errors that can't succeed | S | ✅ 0.0.5 phase 2 | |
| A3 | Sturdier "Switch to this PC" | S | ✅ 0.0.5 phase 2 | |
| A4 | Fresher values without polling (re-read on wake, before stale hotkey steps) | S | ✅ 0.0.5 phase 2 | |
| C1 | Crash marker / safe start | S | ✅ 0.0.5 phase 2 | |
| C3 | Driver-update hint in "not answering" | S | ✅ 0.0.5 phase 2 | |
| D1 | Colour-mode names match the OSD | — | — | ✅ verified by you |
| D2–D3 | Hardware checks: aspect ratio, which modes lock brightness | S | ✅ before phase 2 | |
| F | "PC screen only" when handing the monitor over (Windows) | M | ✅ | ❌ dropped |
| B1 | Automatic Day/Night (times or sunset) | M | ✅ 0.0.5 phase 3 | |
| B5 | Better command line (`--get`, `--json`, exit codes, link scheme) | S | ✅ 0.0.5 phase 3 | |
| A5 | Linux: warn about KDE controlling brightness | S | ⏸ when Linux is tested | |
| E1–E5 | Model-neutral groundwork | M total | ⏸ when a second model comes | |
| E6 | Support any DDC monitor (one at a time) | M | ⏸ after E1–E5 | |
| E8 | Export profile for contributions | S | ⏸ after E6 | |
| B4 | Switch input when a USB device connects | M | ⏸ | |
| B10 | Fine 1% steps | S | ⏸ | |
| B2 | Keyboard brightness keys | S–M | ❌ | |
| B3 | Scroll on tray icon | M | ❌ | |
| B6 | Darker than 0% (overlay) | M | ❌ | |
| B7 | Restore settings after wake (opt-in) | S | ❌ unless the monitor forgets | |
| B8 | Per-app presets | M | ❌ | |
| B9 | Dim when idle | M | ❌ | |
| C2 | Safe-mode start (hold Shift) | S | ❌ (C1 covers it) | |
| C4 | Per-code health score | M | ❌ (A2 covers most) | |
| E7 | Several monitors at once | L | ❌ | |
