# ThinkVision Control: independent review of the areas around the crates

Branch feat/device-sourced-status. Read-only review. I did not open docs/plans/. Line numbers refer to the working tree.
Anything not confirmed in the code is marked **speculation**.

## Medium

1. **The `--get` process wipes or trips the resident app's crash marker.** `crates/tvc-app/src/main.rs:62-64,102`, with `engine.rs:619-621`.
   `--get` skips the single-instance step (main.rs:45) but still runs `marker.left_over()` and then `marker.clear()` on the shared `risky-operation.txt`.
   - Scenario A: the resident app is in its 2.3 s capability read (marker armed) when a Stream Deck or script runs `--get all`. The `--get` run logs "Safe mode: crashed during capability read", starts in safe mode, and deletes the resident's marker. If the resident then really crashes inside that read, the next start no longer comes up in safe mode.
   - Scenario B: the reverse. The resident starts at login while a `--get` run with no cached capabilities is inside its own read. The resident wrongly starts in safe mode.
   - Fix: use a per-process marker name (include the PID), or skip the marker check and clear for `--get`.

2. **On Linux without `XDG_RUNTIME_DIR`, the command socket is shared by all users.** `crates/tvc-app/src/platform/instance.rs:294-299`.
   The lock file falls back to a per-user path, but the socket is always `/tmp/CoreFxPipe_tvc-cmd`.
   - User A's app binds the socket first. When user B's app starts, its `remove_file` fails on A's socket (sticky `/tmp`). B's `bind` then fails and retries every 500 ms for ever (instance.rs:349-355).
   - B's later launches try to connect to A's 0600 socket, fail for 5 s, and exit with "Forwarded", so B's commands are silently lost.
   - A hostile local user can pre-bind that path and receive or swallow everyone's commands.
   - The `set_permissions(0o600)` after `bind` (line 357) also leaves a short window open.
   - Related: when `USER` is unset, the lock path becomes `/tmp/thinkvision-control-.lock`, shared by everyone.
   - Fix: use a per-uid directory with mode 0700 (e.g. `/tmp/thinkvision-control-<uid>/`) for both the lock and the socket.

3. **The default input shortcuts switch the Linux virtual console.** `crates/tvc-core/src/hotkey.rs:240-242` (defaults Ctrl+Alt+F1/F2/F3, used on every OS).
   - On X11, the X server/XKB handles Ctrl+Alt+F1–F3 as a VT switch before any client grab. Pressing the advertised "Input HDMI" shortcut drops the user to a text console or the greeter, and the grab never fires.
   - Ctrl+Alt+Left/Right/Up/Down (hotkey.rs:234-237) are also the workspace-switch keys in Xfce, Cinnamon and MATE, so those grabs fail or collide.
   - Fix: use different defaults on Linux (e.g. Ctrl+Alt+Shift+1/2/3), or leave the input shortcuts unset there.

4. **The shipped licence text doesn't match the stated licence of the binary.** `README.md:113` says "the distributed program is GPLv3" because of Slint's GPLv3 option. But `LICENSE` is GPLv2, and it is the only GPL text shipped:
   - `packaging/windows/ThinkVisionControl.iss:49`
   - `scripts/make-linux-tarball.sh:31`
   - `scripts/make-macos-app.sh:43`

   No notices are shipped for the ~560 MIT/Apache crates in Cargo.lock. Fix: ship a GPLv3 `COPYING` next to LICENSE, and generate a third-party notices file (e.g. `cargo about`) in all three packages.

5. **The advertised exit codes and `--get` output don't work from an interactive Windows shell.** `crates/tvc-app/src/main.rs:1` (`windows_subsystem = "windows"`) against `README.md:85-92` and `cli.rs:173-175`.
   - cmd.exe at the prompt and PowerShell (unless the output is piped or redirected) don't wait for a GUI-subsystem exe.
   - So `thinkvision-control --get all` prints after the prompt has already returned, and `%ERRORLEVEL%` / `$LASTEXITCODE` don't hold the documented codes.
   - Batch files and pipes do wait, so scripts mostly work. A user testing by hand sees broken behaviour.
   - Fix: say in the README and help to use `start /wait` or pipe the output, or ship a small console-subsystem `thinkvision-control-cli.exe`.

6. **The Linux launcher entry depends on `~/.local/bin` being on the session PATH.** `packaging/linux/thinkvision-control.desktop:7,17,21,25`, copied unchanged by `make-linux-tarball.sh:46`.
   - `Exec=thinkvision-control` is a bare name. install.sh creates `~/.local/bin` itself (line 41) and even warns when it is not on PATH (line 57).
   - On Debian/Ubuntu, `~/.profile` adds that directory only if it existed at login. So the launcher, the Day/Night/Switch actions and `thinkvision://` links fail with "command not found" until the next login (forever on distros that never add it).
   - Fix: have install.sh rewrite `Exec=` to the absolute `$dest/thinkvision-control`.

7. **A panic kills the tray app without leaving anything in the log.** `Cargo.toml:31` (`panic = "abort"`). No `std::panic::set_hook` exists anywhere in `crates/` (I grepped).
   The release exe is GUI-subsystem, so the panic message has nowhere to go. Any `unwrap`/`expect` failure on any thread (UI, engine, instance listener, debouncer) makes the app vanish from the tray, and `log.txt` shows nothing.
   Fix: install a panic hook in `main` that writes the message and location through `tvc_core::log::write` before the abort.

## Low to medium

8. **Log and backup timestamps use a UTC offset frozen at start-up.** `crates/tvc-core/src/clock.rs:10` is set only once, at `main.rs:40`. `localtime.rs:1` claims "read fresh on every call", which is true only for the schedule.
   - After a DST change, a long-running tray app writes log lines and `backup-*.json` names one hour off until it restarts.
   - `BackupStore::load_latest` (`store.rs:353-364`) picks the newest backup by the local-time name. After an autumn fall-back plus a restart, a fresh backup can sort *before* the one made an hour earlier, and the "restore last backup" path then loads the older one.
   - Fix: refresh the offset on every `clock::now()` through an app-supplied callback, or stamp backups in UTC and order them by mtime.

9. **The uninstaller cleans up the elevating admin's HKCU, not the user's.** `packaging/windows/ThinkVisionControl.iss:128-133`.
   - With `PrivilegesRequired=admin`, a standard user who uninstalls with over-the-shoulder admin credentials runs the uninstaller as the admin. `RegDeleteValue(HKCU…\Run)` and `RegDeleteKeyIncludingSubkeys(HKCU…\Classes\thinkvision)` then hit the admin's hive.
   - The real user keeps a Run entry that points at a deleted exe, and an HKCU `thinkvision:` handler that shadows nothing. Other users of an all-users install are never cleaned up either.
   - `[InstallDelete] {userprograms}` / `{%TEMP}` (lines 55-56) have the same problem.
   - Fix: delete the Run value with the `runasoriginaluser`-style `ExecAsOriginalUser` (`reg delete`), or make the app remove its Run entry on first start when its exe is missing (documented limitation otherwise).

10. **After a Linux update, the autostart entry can point at "…(deleted)".** `crates/tvc-app/src/platform/autostart.rs:75`, with `make-linux-tarball.sh:42-43`.
    - install.sh runs `rm -f` on the running binary and copies a new one, but doesn't restart the app.
    - In the old process, `current_exe()` reads `/proc/self/exe`, which now ends in " (deleted)". `apply_settings` rewrites the autostart file on *every* settings save (controller.rs:438).
    - So any setting change before a restart writes `Exec="…/thinkvision-control (deleted)" --startup` and a `TryExec` that doesn't exist, and start-at-login silently stops working.
    - Fix: strip a trailing " (deleted)", or write the stable install path rather than `current_exe()`.

11. **A Shift-only global shortcut is accepted.** `crates/tvc-core/src/hotkey.rs:123,128`, used by the editor at `controller.rs:1032`.
    - `try_create`/`parse` accept any non-zero modifier set, so "Shift+A" is valid.
    - Recording Shift+A registers it with RegisterHotKey, and every capital A typed system-wide is swallowed.
    - The editor hint ("Start with Ctrl, Alt or the Windows key") implies this is rejected, but it isn't.
    - Fix: require Ctrl, Alt or Win (`mods & !MOD_SHIFT != 0`).

12. **An unreadable or BOM-prefixed settings.json resets all settings.** `crates/tvc-core/src/store.rs:78-82`.
    - A file saved by a text editor with a UTF-8 BOM fails `serde_json`. It is then renamed to `.bak` (overwriting any earlier `.bak`), and everything goes back to defaults, hotkeys included.
    - A transient read failure (sharing violation from antivirus or sync software) also returns defaults. The next save (`controller.rs:995`, which runs on any settings change) then overwrites the real file.
    - Fix: strip a leading `\u{FEFF}`. On a non-NotFound read error, keep the file and don't save over it.

13. **The Windows command pipe can be squatted by another local account.** `crates/tvc-app/src/platform/instance.rs:225-234,145-162`.
    - The pipe name is predictable (`ThinkVisionControl.Commands.{user}.{session}`), and the server re-creates it after every client without `FILE_FLAG_FIRST_PIPE_INSTANCE`.
    - Another account can create that name in a gap. The real server then fails or joins the squatter's pipe and retries every 500 ms.
    - The client never checks who owns the server, so it sends its arguments and calls `AllowSetForegroundWindow(pid)` for the squatter.
    - The payload has little value, so the impact is mainly denial of service and a foreground-permission grant.
    - Fix: add `FILE_FLAG_FIRST_PIPE_INSTANCE` on the first `CreateNamedPipeW`, and on the client check `GetNamedPipeServerSessionId` and the token user before writing.

14. **One slow client blocks the instance listener.** `instance.rs:244-252` (Windows) and `instance.rs:358-363` (Unix).
    The read has no timeout and is single-threaded. A client that connects and never closes blocks all later forwards; each later launch gives up after 5 s and exits 0 without doing anything. Same-user only.
    Fix: set a read timeout (Unix `set_read_timeout`; Windows overlapped I/O with a timeout).

## Low

15. **install.sh never updates the udev rule.** `scripts/make-linux-tarball.sh:60-63`. The rule is installed once and skipped when the file exists, so a corrected rule in a later tarball never reaches existing users. Fix: compare with `cmp -s` and offer the update when the files differ.

16. **`udevadm trigger` re-triggers every device on the machine.** `make-linux-tarball.sh:82`, and the hint at `platform/setup.rs:82`. Without a filter it causes needless churn (network, input and storage change events). Fix: `sudo udevadm trigger --subsystem-match=i2c-dev`.

17. **There is no uninstall path on Linux or macOS, and the README never mentions uninstalling.** Leftovers:
    - `~/.local/share/thinkvision-control`
    - the `~/.local/bin` symlink
    - the `.desktop` file and the icon
    - `~/.config/autostart/thinkvision-control.desktop`
    - `/etc/udev/rules.d/60-thinkvision-i2c.rules` and `/etc/modules-load.d/i2c-dev.conf`

    The Windows uninstaller also leaves `%APPDATA%\ThinkVisionControl` (likely intentional). Fix: ship an `uninstall.sh`, and add an Uninstall section to the README.

18. **The default Day/Night preset is re-applied on every start, overriding a manual change.** `controller.rs:221` (`auto_phase` starts as `None`) and `controller.rs:630`. The first connect always applies the scheduled preset, so a manual brightness change is undone by every reboot, sign-in or installer upgrade. This contradicts schedule.rs:1-2, "a manual change in between sticks". Fix: persist the last applied phase in settings.

19. **The release builds aren't reproducible.**
    - `.github/workflows/release.yml:20,37` use `dtolnay/rust-toolchain@stable` (unpinned).
    - The build scripts call `cargo build` without `--locked` (`make-linux-tarball.sh:25`, `make-windows-setup.ps1:16`, `make-macos-app.sh:36`), while CI uses `--locked`.
    - The actions are pinned by tag, not SHA, including the job with `contents: write`.

    Fix: pin the toolchain in `rust-toolchain.toml`, add `--locked`, and pin the actions to SHAs.

20. **CI never checks the size budget or the packaging.** `Cargo.toml:25` says "must stay under 10 MB", and README:38 says 9 MB. `ci.yml` builds only debug tests, and the packaging scripts run only on a tag or a manual dispatch. Fix: add a release build step that fails over 10 MB, and run `make-linux-tarball.sh` in CI.

21. **The README's build-from-source line skips the Linux system packages.** `README.md:107`. `cargo run --release -p tvc-app` fails on a clean Linux without `pkg-config libfontconfig1-dev libudev-dev libxkbcommon-dev`, which CI installs at `ci.yml:28`. Fix: list them in the README.

22. **`subset-icon-font.py` writes the font before checking for missing glyphs.** `scripts/subset-icon-font.py:38-43`. The check runs after `save_font`, so a typo in EXTRA overwrites the tracked font with a broken subset and only then fails. Fix: move the check before `save_font`. I checked EXTRA against the Rust `\u{…}` constants and every icon used from Rust is listed.

23. **The Linux binary carries two copies of tiny-skia.** `Cargo.lock`: tiny-skia 0.11.4 (via sctk-adwaita) and 0.12.0 (via resvg). Linux only. A `zune-jpeg` decoder is also locked in via `image`; whether it is actually linked is **speculation**. Fix: check with `cargo bloat`, and trim the `image` features if Slint allows it.

24. **The Linux setup hint can blame permissions when the monitor is simply missing** (**speculation**, depends on hardware). `crates/tvc-ddc/src/linux/i2c.rs:83,112`. The shipped udev rule grants only GPU buses. Any non-GPU adapter that isn't in `IGNORABLE_ADAPTER_PREFIXES` stays root-only and sets `permission_denied`. If the monitor is unplugged or DDC/CI is off, the app reports PermissionDenied and tells the user to install a rule they already have. Fix: report PermissionDenied only when a DRM-linked bus was denied.

## Speculation (not confirmed against external tools)

25. **Inno Setup version claim.** `ThinkVisionControl.iss:29` uses `WizardStyle=modern dynamic`. I believe `dynamic` (dark mode) came in Inno Setup 6.6, while the script header and `make-windows-setup.ps1:2` say 6.5+. If so, compiling on 6.5 fails. CI installs whatever choco serves (`release.yml:22`). Fix: confirm the version and update the comment or add `#if Ver < …` guards.

26. **Re-running the macOS script while the app runs.** `scripts/make-macos-app.sh:99-101`. `ditto` merges into an existing `/Applications/…app` (stale files stay), and may rewrite the signed executable in place while it is running. Apple Silicon can then SIGKILL the old or new binary because of cached code-signing pages. `open` just activates the old running copy. Fix: quit the running app, `rm -rf` the old bundle, then copy.

27. **Concurrent DDC from two processes on Windows.** `--get` runs a second dxva2 connection (main.rs:43-45) beside the resident app. Windows has no cross-process bus lock, unlike Linux's `flock`, so a `--get` read during a resident write or read could get the other request's reply.

28. **Intel graphics hotkeys.** On Windows machines whose Intel driver still has hotkeys enabled, Ctrl+Alt+Arrow rotates the screen, so the brightness and contrast defaults would either fail to register or rotate the display.

## Checked and found fine

- **`cli.rs` parsing.** The URL prefix slice is safe: the first 12 bytes are ASCII-checked. `+N` parses, `%2B` is handled, and the step bound is enforced on both parse and format.
- **`solar.rs` and `schedule.rs`.** Polar day/night returns None, and the neighbouring-UTC-date scan is correct. Fixed times across a DST jump resolve sensibly, and the schedule reads the offset fresh.
- **`log.rs` rotation.** Capped at about 2 MB total; `rename` replaces on Windows.
- **`store.rs`.** Backward-compatible field defaults; `backup_key` slicing is panic-safe.
- **Linux autostart Exec quoting.**
- **Windows pipe.** DACL is current-user only, with `PIPE_REJECT_REMOTE_CLIENTS`.
- **Windows mutex.** Session-local; a NULL handle falls back to primary.
- **Installer close handling.** The WM_ENDSESSION path matches the `ThinkVisionControl.MonitorEvents` class, and the mutex name matches `instance.rs`.
- **Small platform modules.** `efficiency.rs`, `taskbar.rs`, `console.rs` and `wake.rs` are correct as written.
- **udev rule match.** `ATTRS{class}=="0x03*"` with `uaccess` at priority 60 is valid.
- **README against the code.** Shortcuts, CLI verbs, links, log paths and the "only P32p-20" statement match. The P32p-20 hard-coding in the desktop Comment and README is consistent with the code, which matches only that EDID.
