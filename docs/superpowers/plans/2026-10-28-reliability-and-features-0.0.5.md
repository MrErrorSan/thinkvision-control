---
name: Reliability and features (0.0.5 phases 2–3)
overview: A1–A4, C1, C3 (reliability) and B1, B5 (automatic Day/Night, better command line) on the Rust app, built in parallel against pinned contracts; hardware tests deferred until the monitor is attached.
todos:
  - id: contracts
    content: Pin ResultKind (crates/tvc-core/src/result.rs) and stub crash/schedule/solar modules
    status: completed
  - id: core-reliability
    content: tvc-core engine — A1 pause while asleep/locked + wake delay + 1-min burst, A2 retry by error class, A3 sturdier input switch, A4 stale re-read before steps, C1 crash marker + safe mode, ResultKind on every WriteResult, Origin::Schedule
    status: completed
    parallel: true
  - id: core-features
    content: tvc-core — solar.rs (sunrise/sunset), schedule.rs (B1 current phase), Settings.auto_day_night (+ settings.json field), cli.rs B5 (--get, --json, --help, thinkvision:// URLs, exit codes)
    status: completed
    parallel: true
  - id: ddc-errors
    content: tvc-ddc — classify backend errors into Transient / Unsupported / Gone (A2)
    status: completed
    parallel: true
  - id: platform
    content: tvc-app/src/platform — SystemEvent stream (display on/off, lock/unlock, suspend/resume, display change), wake nudge, console attach, URL-scheme self-registration (Windows)
    status: completed
    parallel: true
  - id: packaging
    content: thinkvision:// registration in the installer, .desktop and Info.plist; README command-line and Automatic Day/Night sections
    status: completed
    parallel: true
  - id: app-wiring
    content: tvc-app controller/view/ui/main — wire all of the above; Automatic Day/Night settings UI; asleep and safe-mode banners; C3 hint
    status: completed
    depends-on: core-reliability, core-features, ddc-errors, platform
  - id: verify
    content: Hardware session (D2, D3, A3 switch from the PC not shown, A4 0x02 flag) + the phase-1 checklist
    status: completed
    depends-on: app-wiring
---

# Reliability and features (0.0.5 phases 2–3)

Source: [2026-10-28-open-source-learnings.md](../../research/2026-10-28-open-source-learnings.md) (items by id). Builds on the Rust port: [2026-10-28-rust-slint-0.0.5.md](2026-10-28-rust-slint-0.0.5.md). Parity checklist: [INV](../specs/2026-10-28-parity-inventory.md).

**Owner (2026-10-28):** implement now, test everything when the monitor is attached. So agents compile and lint only; no `cargo test`, no runs against real hardware.

## Goal

- A1: no monitor traffic while the display is off, the session is locked or the PC sleeps; wait 3 s after wake, then reconnect and re-read once; reconnect schedule stretched to about a minute.
- A2: retry only transient errors; "unsupported" is final; "connection gone" reconnects at once.
- A3: read before switching input, write once, confirm by a delayed re-read, ignore the input reply's high byte, wake the monitor first.
- A4: re-read before a hotkey step when the cached value is older than 30 s; re-read on wake (from A1).
- C1: a crash during a capability read or the finder starts the next run in safe mode (no writes, no capability read, no finder) until the user leaves it.
- C3: "isn't responding" mentions graphics-driver updates.
- B1: automatic Day/Night at set times or at sunrise/sunset for a typed location; catches up after sleep.
- B5: `--get`, `--json`, `--help`, documented exit codes, `thinkvision://` links.

## Pinned contracts

### ResultKind (done) — [result.rs](../../../crates/tvc-core/src/result.rs)

`WriteResult` gains `pub kind: ResultKind` (Ok, Kept, Refused, OutOfRange, Unsupported, Asleep, SafeMode, NotFound, NotResponding). The engine sets it on every result; `outcome` stays for existing callers.

### Engine additions (core-reliability)

```rust
pub enum PowerState { DisplayOff, DisplayOn, Locked, Unlocked, Suspending, Resumed }
Command::Power(PowerState)
Command::SetSafeMode(bool)
Event::Asleep(bool)          // true while display off / locked / suspended
Event::SafeMode(bool)        // at spawn when started in safe mode, and on change
Origin::Schedule             // B1 applies presets with it
Timing { wake_delay: 3 s, input_confirm: 1.5 s, stale_after: 30 s, burst: 0.5,1,2,4,8,15,30 s, .. }
Engine::spawn(.., safe_mode: bool, ..)   // new parameter, after `timing`
pub mod crash { pub struct Marker; Marker::new(dir); fn left_over(&self) -> Option<String>; fn clear(&self) }
```

- While asleep: commands that would touch the link return `kind: Asleep` (outcome Blocked, message "The monitor is asleep or this PC is locked"); Refresh/Poll are dropped; the burst is paused. On the last of DisplayOn/Unlocked/Resumed: after `wake_delay`, `RecheckAfterEvent`.
- Linux: `PermissionDenied` within 10 s of `Resumed` is retried by the burst, not reported (ddcutil behaviour).
- A2 in `run_op`: only `Transient` is retried; `Unsupported` → result `kind: Unsupported`, never retried; `Gone` → no retry, monitor marked lost, burst starts now.
- A3 for the input code (0x60): read first (low byte only) and skip the write if already there; one write attempt; after `input_confirm` re-read once — same value → Ok; old value → Blocked "The monitor stayed on {label}"; read fails → Ok "Input → {label}" (the other PC now owns DDC). Every 0x60 reading is masked to its low byte.
- A4: `Step` refreshes the code first when its cached reading is older than `stale_after`.
- C1: the engine writes the marker (`risky-operation.txt` in the data dir, containing the operation name) before a capability read and before finder snapshot/compare, and removes it after. The app checks `left_over()` before spawning; if set, it spawns with `safe_mode: true` and clears the marker. Safe mode: capabilities come from the cache or the profile, writes return `kind: SafeMode`, finder commands are refused.

### Features (core-features)

```rust
pub mod solar { pub fn sun_times(year: i32, month: u32, day: u32, lat: f64, lon: f64) -> Option<(u32, u32)>; }  // minutes after 00:00 UTC; None = polar day/night
pub mod schedule {
    pub enum AutoMode { Off, Times { day: u16, night: u16 /* minutes after local midnight */ }, Sun { lat: f64, lon: f64 } }
    pub enum Phase { Day, Night }
    /// The latest scheduled event at or before `now` (looking back into yesterday), so a missed event after sleep still applies.
    pub fn current(mode: &AutoMode, now_utc_secs: i64, utc_offset_secs: i32) -> Option<Phase>;
}
Settings.auto_day_night: AutoMode   // settings.json "AutoDayNight": {"Mode":"Off|Times|Sun","Day":"07:00","Night":"19:00","Latitude":..,"Longitude":..}; missing → Off; 0.0.4 ignores it
pub enum GetTarget { Brightness, Contrast, Input, ColourMode, All }
CliCommand::Get(GetTarget) | CliCommand::Help
Parsed { command, startup, json: bool }
pub fn cli::parse_url(url: &str) -> Option<CliCommand>    // thinkvision://open | preset/day | input/hdmi|this | step/brightness/+5 | get/brightness
pub fn cli::exit_code(kind: Option<ResultKind>, status_found: bool) -> i32   // 0 OK, 1 not found, 2 out of range, 4 unsupported, 5 hardware, 8 timeout (PowerToys CliExitCodes)
pub fn cli::help() -> &'static str
```

`parse` accepts a lone `thinkvision://…` argument (how the OS launches a link). A malformed first verb still means Show (INV §6.1).

### Platform (platform)

```rust
pub enum SystemEvent { DisplayChanged, DisplayOff, DisplayOn, Locked, Unlocked, Suspending, Resumed }
pub fn monitor_events::watch(on_event: impl Fn(SystemEvent) + Send + 'static) -> Watcher;   // replaces the Fn() version; DisplayChanged keeps its debounce
pub mod wake { pub fn nudge(); }            // Windows: 1 px mouse move and back (SendInput); macOS: `caffeinate -u -t 1`; Linux: no-op
pub mod console { pub fn attach(); }        // Windows: AttachConsole(ATTACH_PARENT_PROCESS) so CLI output reaches the terminal
pub mod url_scheme { pub fn register(); }   // Windows: HKCU\Software\Classes\thinkvision → "<exe>" "%1" (idempotent); others no-op
```

Windows: `RegisterPowerSettingNotification(GUID_CONSOLE_DISPLAY_STATE)` (0 off, 1 on, 2 dimmed = no event), `WTSRegisterSessionNotification` (lock/unlock), `PBT_APMSUSPEND` / resume. Linux: logind `PrepareForSleep` and session `Lock`/`Unlock` (lean: no new heavy dependency; report the choice). macOS: system sleep/wake via the existing IOKit power callback; display sleep if cheap.

### `--get` and forwarding

`--get` always runs its own headless connection (read-only), even when a resident copy runs: a forwarded command can't return output. Other verbs forwarded to a resident copy exit 0 once handed over (the result shows as the OSD there). Recorded as a known limit.

## Key design decisions

- **One engine agent** owns every `engine.rs` change (A1–A4, C1): they all touch `run_op`, `set` and the loop; splitting them would collide.
- **ResultKind pinned first** so the CLI exit codes and the engine are built in parallel.
- **Safe mode = no writes, no capability read, no finder**, left by a button. Matches MonitorControl's "no writes" and PowerToys' "skip the probe that crashed".
- **A3's "read failed after switching" counts as success**: on a two-PC desk the monitor usually stops answering this PC once it shows the other.
- **B1 location is typed, not detected**: no permission prompt (learnings B1).
- **B1 re-applies only on a phase change** (Day→Night), not every minute, so manual changes in between stick.
- **Asleep is an event, not a `MonitorStatus`**: status keeps meaning "can we reach the monitor".

## Implementation steps

1. **core-reliability** — `engine.rs`, `crash.rs`; `WriteResult.kind` everywhere; update existing engine tests only where the new behaviour changes them (compile only; don't run).
2. **core-features** — `solar.rs` (NOAA algorithm, own code), `schedule.rs`, `store.rs` (new field + JSON), `cli.rs` (B5). Unit tests may be written but not run.
3. **ddc-errors** — Windows: `ERROR_GRAPHICS_DDCCI_VCP_NOT_SUPPORTED` → Unsupported; handle/monitor-gone codes → Gone (exists); else Transient. macOS: a valid "unsupported" reply → Unsupported once the checksum form is known. Linux: already done; check.
4. **platform** — as pinned.
5. **packaging** — installer `[Registry]` for HKA `Software\Classes\thinkvision` (URL Protocol, `"{app}\thinkvision-control.exe" "%1"`, removed on uninstall); `.desktop` `MimeType=x-scheme-handler/thinkvision;` and `Exec=thinkvision-control %u`; Info.plist `CFBundleURLTypes`; README sections.
6. **app-wiring** — controller: SystemEvent → `Command::Power` / `RecheckAfterEvent`; `wake::nudge()` before every input switch; Event::Asleep → header + info banner "Monitor asleep"; safe-mode banner with "Leave safe mode"; C3 text; B1 timer (60 s, plus on Resumed/Unlocked/DisplayOn) applying the preset with `Origin::Schedule` (quiet: no OSD, a toast if the window is open); Settings tab "Automatic Day/Night" (Off / At set times with two time fields / At sunrise and sunset with latitude and longitude); main: `console::attach()`, `--help`, `--get` (+ `--json`) headless read and print, `std::process::exit(cli::exit_code(..))` for headless runs, `url_scheme::register()` at resident start.

## Verification (deferred to the hardware session)

- Compile + clippy now: `cargo clippy --workspace --all-targets -- -D warnings`.
- With the monitor: turn the monitor off with its button and back on → reconnects within ~5 s, no writes while off; lock with Win+L → hotkeys show "asleep", unlock → values refresh; hotkey step after another PC changed brightness starts from the fresh value; Switch to this PC from the PC the monitor isn't showing (A3 test); pull the cable mid-drag → Gone → quick reconnect; kill the app during the finder → next start shows safe mode; Auto Day/Night at times a minute apart; `--get all --json`, exit codes with the monitor unplugged (1) and DDC off (5); `thinkvision://preset/night` from Run (Win+R).

## Open questions

- D2 (aspect ratio values) and D3 (which modes lock brightness and contrast) need the hardware session; nothing here depends on them.
- A4's 0x02 "something changed" flag: unproven; tested in the hardware session, not built.
- Linux lock/sleep signals need D-Bus; the platform agent picks the leanest way and reports the size cost.

## Results so far (2026-10-28, monitor attached)

- `cargo test --workspace`: 172 passed (core 129, app 22, ddc 21) in about 2 s. Three new engine tests were fixed (two discarded the events they then waited for; one deadlocked on two `FakeLink` locks in one expression); the debounce test got wider margins (Windows sleep overshoot).
- Real P32p-20 over HDMI (Rust dxva2 backend): connect 1 ms, capability string 2.3 s, each value read about 60 ms.
- **Change from the plan:** connecting now uses the cached capability string when there is one and reads only values; the live capability read runs once, 2 s after a resident app connects (`Command::ReadCapabilities`), never in one-shot command-line runs. An orderly exit clears the crash marker so a quit during that read isn't taken for a crash.
- `--get all --json` → `{"brightness":0,"contrast":20,"input":"HDMI","colourMode":"Neutral"}`, exit 0, about 2.0 s (was 4.2 s). `--step brightness=±5` and `contrast=±5` verified by read-back, exit 0, about 3.5 s (1.5 s of it is the OSD).
- The window connects in under 1.5 s with live values and firmware LB1.2 (`docs/previews/slint/real-display.png`).
- Still to do by hand: see the checklist in the session summary (sleep/lock, input switching from the other PC, hotkeys and tray, Automatic Day/Night, safe mode).
- **Display sleep (A1), real hardware, 2026-10-28.** Windows told to switch the display off (`SC_MONITORPOWER`, the same signal as its power timeout), held 60 s so the monitor itself went to standby (owner: a 4 s off only blanks it; standby is the white→yellow light). While off: `Power: DisplayOff (asleep: true)`, a `--step` handed to the running app was held back ("The monitor is asleep or this PC is locked"), nothing was sent. After a mouse nudge: `DisplayOn`, then "rechecked, monitor Connected" 4.4 s later (3 s wake delay + reads), unaided. The monitor's own power button is broken, so a button-driven standby stays untested; the app treats that case as a monitor that stopped answering (reconnect burst).
- **Change: safe mode no longer blocks writes.** Found while testing: killing the app during the 2.3 s background capability read (the screenshot tool does `Stop-Process -Force`) leaves the crash marker, and the next start came up in safe mode blocking every change. A kill can't be told from a crash, so safe mode now skips only what was risky — the live capability read and the finder — and allows writes; the banner is an Info note with "Read them now". An orderly exit clears the marker.
- **Fix: the kept OSD became the "main window".** It stays visible at alpha 0, so `CloseMainWindow` (installer upgrade, `taskkill` without `/F`) reached the OSD and the app didn't exit. A close request to the OSD now quits the app in order; verified (exit 0, no marker).

## Before release (2026-10-28)

Done:
- Slint credit: "Built with Slint" on the About card and the Made with Slint badge in the README (satisfies the royalty-free licence's attribution option too; distribution stays GPLv3).
- "Try again" shows "Looking for your monitor…" until the status changes or 16 s pass.
- The tray icon follows a live taskbar light/dark switch (`SystemEvent::ThemeChanged` from `WM_SETTINGCHANGE` "ImmersiveColorSet").
- Shift+digit shortcuts can be recorded (Slint reports the shifted symbol; mapped back to the digit key).
- The blanket `#![allow(dead_code)]` in `platform/` is gone; Linux/macOS-only helpers carry a per-item `cfg_attr` instead.
- The .NET app (`src/`, `tests/`, `ThinkVisionControl.slnx`, `Directory.Build.props`) removed from the private branch; never on `public` for 0.0.5.
- Internal doc references (INV §, plan ids, research paths) removed from code comments.
- README rewritten short and current (about 110 lines); Linux `install.sh` offers to set up monitor access with sudo; `make-macos-app.sh` installs into /Applications and starts the app.

Deferred, with reasons (listed under Known gaps in the release notes where users see them):
- Wayland global shortcuts and notification-style OSD: need an xdg-desktop-portal D-Bus client (a new dependency and size cost); X11 works, Wayland shows a setup hint with CLI commands to bind.
- Windows jump list: the tray menu and shortcuts cover the same actions.
- Linux binary 13.9 MB (target 10 MB): trimming means dropping X11 or Wayland support; decide with the Wayland work.
- Poll on window activation: the 3 s poll covers it; Slint exposes no activation callback.
- D2 (aspect ratio values): needs a non-native resolution on the test PC; D3 (which modes lock brightness/contrast) and the A4 `0x02` change-flag experiment: research, not release blockers.
- A button-driven monitor standby: the test monitor's power button is broken; the reconnect burst covers it.
- macOS: no Mac to build or test on; build-from-source script only.
