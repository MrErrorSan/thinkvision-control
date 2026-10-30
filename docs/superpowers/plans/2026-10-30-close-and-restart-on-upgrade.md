---
name: Close and restart the app on upgrade (0.0.8)
overview: The app answers Windows Restart Manager's "please close" so a silent setup (winget upgrade/uninstall) can replace it while it runs; the setup restarts it in the tray afterwards.
todos:
  - id: app-endsession
    content: monitor_events.rs + controller.rs — WM_QUERYENDSESSION/WM_ENDSESSION → SystemEvent::SessionEnding → orderly quit
    status: completed
    parallel: true
  - id: installer
    content: ThinkVisionControl.iss — drop AppMutex; CloseApplications=force; restart if it was running; uninstall closes it
    status: completed
    parallel: true
  - id: release
    content: Version 0.0.8, release notes, TODO
    status: pending
    depends-on: app-endsession, installer
---

# Close and restart the app on upgrade

## Goal

- `winget upgrade` / a silent setup succeeds while the app runs in the tray (today: exit 1, nothing installed —
  Inno's `AppMutex` prompt defaults to Cancel when suppressed).
- The app shuts down cleanly (engine finishes its call, crash marker cleared), then comes back in the tray.
- A silent uninstall (`winget uninstall`) succeeds while the app runs.
- Bonus: an orderly exit at Windows sign-out/shutdown instead of being killed mid-call.

## Target Files

- [crates/tvc-app/src/platform/monitor_events.rs](../../../crates/tvc-app/src/platform/monitor_events.rs) — the hidden
  window answers `WM_QUERYENDSESSION` (yes) and turns `WM_ENDSESSION` into `SystemEvent::SessionEnding`.
- [crates/tvc-app/src/controller.rs](../../../crates/tvc-app/src/controller.rs) — `SessionEnding` → log it, `quit()`.
- [packaging/windows/ThinkVisionControl.iss](../../../packaging/windows/ThinkVisionControl.iss) — installer changes.
- `Cargo.toml` / `Cargo.lock` — 0.0.8.

## Steps

1. **monitor_events.rs (Windows `imp`)**: `WM_QUERYENDSESSION` → return 1. `WM_ENDSESSION` with `wParam != 0` →
   `send(SessionEnding)`, then block (≤ 5 s) instead of returning: once this returns, Windows may end the process,
   and main needs ~1 s for `engine.shutdown()`. `Transitions::accept` delivers it like `ThemeChanged`.
2. **controller.rs**: the watcher closure maps `SessionEnding` to `invoke_from_event_loop(quit)` plus a log line, so the
   log shows whether a close was orderly.
3. **ThinkVisionControl.iss**:
   - Remove `AppMutex` (it is what aborts). Keep the mutex *name* in `[Code]`.
   - `CloseApplications=force` (Restart Manager: polite close first, so 0.0.8+ exits cleanly; kills 0.0.7 and older,
     which ignore it — safe, settings are written atomically). `RestartApplications=no`.
   - `InitializeSetup`: remember whether the mutex exists (= the app was running in this session).
   - `[Run]`: if it was running, start `{app}\thinkvision-control.exe --startup` as the original user, also when silent.
     The "Launch" checkbox only when it wasn't running (no double start).
   - `InitializeUninstall`: if running, post `WM_ENDSESSION` to the window class `ThinkVisionControl.MonitorEvents`,
     wait ≤ 5 s for the mutex to go; still running → interactive: message and stop; silent: stop (non-zero exit).

## Key design decisions

- **Restart Manager over a private `--quit` verb.** One mechanism serves every installer (Inno now, MSI/MSIX later) and
  sign-out/shutdown. A running 0.0.7 treats unknown arguments as "open the window", so a verb couldn't close it anyway.
- **The setup restarts the app, not `RegisterApplicationRestart`.** The setup runs elevated; `[Run]` with
  `runasoriginaluser` guarantees the restarted app is not elevated, and needs no restart registration (which would also
  relaunch it after Windows Update reboots, duplicating the Run-key autostart).
- **Uninstall uses our own close, not Restart Manager** — not confirmed that Inno's uninstaller does Restart Manager.
  The uninstaller is the one installed with the version, so only 0.0.8+ installs need to understand `WM_ENDSESSION`.
- **Mutex name and window class are now an installer contract** (`ThinkVisionControl.SingleInstance`,
  `ThinkVisionControl.MonitorEvents`): note it at both ends.

## Verification

Windows, locally built setup, then the CI build:
1. 0.0.7 running from Program Files → silent setup 0.0.8 → exit 0, files 0.0.8, app running again, not elevated (the
   real upgrade path; 0.0.7 gets force-closed).
2. 0.0.8 running → silent setup again → exit 0; log shows the orderly close; app back in the tray, not elevated.
3. Per-user 0.0.8 running (machine copy stopped) → silent uninstall → exit 0, folder gone, no process left.
4. `cargo test --workspace`; release workflow as for 0.0.7.

## Open questions

- Assumed (to confirm in test 1): Inno closes apps automatically in silent mode with `CloseApplications=force`.
- Not testable here: sign-out/shutdown path (same handler as the installer's; low risk).
