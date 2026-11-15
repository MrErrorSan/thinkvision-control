# Verification of audit-core-ddc sections B, D and E, and of the multi-display plan

Checked against commit 3913f5a, working tree clean under crates/. Every file named below was read; nothing was built. Paths are relative to `crates/`.

**Main finding about line numbers:** the claims about what the code does are almost all right. But the line numbers for 9 files are wrong, some by a lot:

| File | Cited | Actual |
|---|---|---|
| link.rs | ~+34 lines off | e.g. 40-46 → 6-12, 73-80 → 39-46, 94-104 → 48-70, 144-160 → 110-126 |
| mccs.rs | 186-191 | file is 134 lines |
| crash.rs | 168 | file is 53 lines |
| cli.rs | 402-403 | file is 202 lines |
| fake.rs | 304-310 | file is 132 lines |
| linux/i2c.rs | 146-200, 220-240, 110 | ranges point at the wrong code |
| macos/locator.rs | 476-488 | file is 113 lines |
| macos/link.rs | 241 | file is 232 lines |

The cited lines for engine.rs, profile.rs, store.rs, main.rs, dxva2.rs, hotkey.rs and macos/mod.rs are correct. The plan copies one wrong reference: "link.rs:74" is `VIRTUAL_VENDORS`, and the `DisplayKind` doc is at link.rs:40.

## 1. Verdict table

### Section B

| ID | Claim | Verdict | Evidence |
|---|---|---|---|
| B1 | main.rs:54-59: one ProfileStore, one MonitorMatch(edid_id, short_name); one engine at :65 | CONFIRMED | main.rs:54 `ProfileStore::new(&data_dir)`, :59 `MonitorMatch{edid_id, description_hint: Some(short_name)}`, :65 `Engine::spawn`. Also: one global crash `Marker` at main.rs:62-64 and :102; `Controller::new(engine, profile, …)` at main.rs:75 takes a single `EngineHandle` and `Arc<Profile>` |
| B2 | profile.rs:16: one EMBEDDED profile; no registry | CONFIRMED | profile.rs:16 `include_str!("profile/p32p20.json")`; `ProfileStore::new` at :134-157 parses only that file |
| B3 | profile.rs:111-123: hard-coded model data in `Profile` | CONFIRMED | Fields at :112-122; the app shows them (view.rs:105,113,408,410,462; controller.rs:528,690) |
| B4 | profile.rs:152: custom_colour_mode fallback 0x12 | CONFIRMED | `.unwrap_or(0x12)` |
| B5 | profile.rs:164-172: unknown codes become "Unknown 0xNN" ReadOnly/Hidden; no generic MCCS table | CONFIRMED | :167-169 `Enum, Hidden, ReadOnlyUntilNamed` |
| B6 | profile.rs:176-186: synthesized caps from the profile | CONFIRMED | Used at engine.rs:266-267 whenever there is no cache, or the cache doesn't parse |
| B7 | profile.rs:190-205, 228-232: one overlay; edidId written (:199), never checked on load | CONFIRMED | :199 writes `edid_id`; `load_overlay` (:228-232) reads only `file.controls` |
| B8 | p32p20.json: CA/D6 NeverWrite, 04/05/08 Action, 0x60 = 11/0F/31, E0/EA/EC named | CONFIRMED | Dumped: 0x60 Input is `Normal` with values 0x11/0x0F/0x31; 04/05/08 are `ConfirmRequired` Actions; CA/D6 `NeverWrite`; E0 "Over Drive", EA, EC are `Normal` |
| B9 | engine.rs:29-32 standard codes | CONFIRMED | |
| B10 | engine.rs:38, 62-77 Timing default = P32p-20; from profile (profile.rs:137-143, main.rs:69) | CONFIRMED | |
| B11 | engine.rs:36, 980-981: single-monitor messages | CONFIRMED | Also engine.rs:158 "isn't supported by this monitor" |
| B12 | engine.rs:763 hard-codes the lock note; the engine ignores `brightness_lock_note` | CONFIRMED | Only controller.rs:690 reads `brightness_lock_note`, so the note is doubled in the app |
| B13 | engine.rs:264, 273-274: caps cache, crash Marker and BackupStore share one dir | CONFIRMED | Backups are `backup-*.json` in the dir root (store.rs:342). There is no `backups/` subfolder today |
| B14 | engine.rs:233-251: no id on events; `Displays` sent only after NotFound | CONFIRMED | :593-595, :1016-1024 |
| B15 | engine.rs:174-216: Command has no target and no op id | CONFIRMED | |
| B16 | engine.rs:302: thread name `tvc-engine` | CONFIRMED | |
| B17 | engine.rs:1111-1116: Controls carries `profiles.profile()` | CONFIRMED | `ResolvedControls` is built only here |
| B18 | mccs.rs:186-191: `firmware()` reads 0x04 as ASCII | CONFIRMED content, REFUTED lines | Actually mccs.rs:26-31 |
| B19 | store.rs:27-43, 48-62: one this_machine_input (default 0x11 at :55, :160); presets carry colour_mode | CONFIRMED | |
| B20 | cli.rs:402-403: INPUTS hard-coded; no `--monitor`; help says "monitor" | CONFIRMED content, REFUTED lines | INPUTS at cli.rs:12-13 (also `displayport`, `usb-c`, `this`); help at :154-184. No `--yes` flag exists either, though the plan's tests rely on one |
| B21 | hotkey.rs:106-108, 150-152: InputHdmi/Dp/UsbC | CONFIRMED | They resolve through `profile.inputs` by kind at controller.rs:528, 538-546 |
| B22 | link.rs:40-46 MonitorMatch identifies by edid_id plus hint only | CONFIRMED content, REFUTED lines | link.rs:6-12 |
| B23 | link.rs:73-80: DisplayKind doc "Only External can be…" | CONFIRMED content, REFUTED lines | link.rs:39-46 (doc at :40) |
| B24 | link.rs:94-104: AttachedDisplay has no connector, serial or path; `target: bool`; English `label()` | CONFIRMED content, REFUTED lines | link.rs:48-70 |
| B25 | link.rs:144-160: MonitorLink is DDC-shaped | CONFIRMED content, REFUTED lines | link.rs:110-126 |
| B26 | dxva2.rs:70-81: first matching device path wins | CONFIRMED | Guarded by `target.is_none()` at :72; index math at :76-78 |
| B27 | dxva2.rs:84-88: hint fallback; "" matches everything | CONFIRMED | `Some("")`, then `.contains("")` is true, so it picks `all[0]`, the first physical monitor of any make |
| B28 | dxva2.rs:104-116: `target` compares to the single match; drops monitorDevicePath and connectorInstance | CONFIRMED | The path is used only for `edid_id_from_path`; `connectorInstance` is never read |
| B29 | i2c.rs:146-200: scans every bus for the one EDID; with twins picks the first that answers 0x10 (:173-193) | CONFIRMED content, REFUTED lines | connect at i2c.rs:61-115, scan at :80-86, choice at :88-108. A single match skips the probe (:101); with no answer it falls back to the first bus that opened (:108) |
| B30 | i2c.rs:220-240: `displays()` drops the connector and the ddc bus | CONFIRMED content, REFUTED lines | i2c.rs:133-155. The connector is only a sort key (:153-154) |
| B31 | macos/locator.rs:476-488: `find` takes the first CG display with that vendor/model | CONFIRMED content, REFUTED lines | locator.rs:19-33 |
| B32 | macos/mod.rs:140-161: SingleExternal fallback can bind another monitor's proxy | CONFIRMED | :160 |
| B33 | macOS link has no `displays()` | CONFIRMED | The `impl MonitorLink for IoAvLink` (link.rs:155-232) has no `displays`, so the trait default `None` applies |
| B34 | macos/link.rs:241, i2c.rs:110: probe with 0x10 | CONFIRMED content, REFUTED lines | macos/link.rs:18 `VERIFY_CODE`; i2c.rs:25 `PROBE_CODE` |
| B35 | examples/probe.rs:8-9: defaults LEN62A2/P32p-20 | CONFIRMED | It also calls `tvc_ddc::open(MonitorMatch…)` at :10, which breaks when `open` is removed |
| B36 | fake.rs:304-310: the fake is P32p-20 only | CONFIRMED content, REFUTED lines | fake.rs:10-16 constant caps, :107 always returns them, :40-61 values. No `displays()`, no identity, caps can't be configured |

### Section D

| ID | Claim | Verdict | Evidence |
|---|---|---|---|
| D1 | `Engine::spawn(link, profiles, dir, timing, safe_mode, sink)` at engine.rs:259-310; Worker owns all state (:349-384) | CONFIRMED | |
| D2 | send/sender at :319-327; one sink at :1102-1104; the app sink is invoke_from_event_loop (main.rs:71-73) | CONFIRMED | |
| D3 | MonitorStatus :98-103, changed only in set_status (:1026-1038) | CONFIRMED | The only other assignment is the initial value at :287 |
| D4 | MonitorLink (link.rs:146-160) is called only inside run_op/connect (:1040-1096) | PARTIAL | Lines wrong (link.rs:110-126). The link is also called outside those paths: `self.link.displays()` at :1018 and `self.link.disconnect()` at :399, neither paced |
| D5 | Power per engine :549-572; discovery folded into the link and reported only on NotFound | CONFIRMED | |
| D6 | "(b) engine.rs:264, 273-274" | CONFIRMED | |
| D7 | `ResolvedControls` at engine.rs:164-171 | CONFIRMED | It derives `Clone, Debug, PartialEq, Eq`, so any added field needs those derives too |
| D8 | "`DisplayKind::BuiltIn` … (link.rs:74 doc)" | REFUTED line | The doc is at link.rs:40; :74 is `VIRTUAL_VENDORS` |
| D9 | "a 1.5 s input confirm" | CONFIRMED | `Timing::input_confirm` at engine.rs:72; sleep at :806 |
| D10 | "Concurrent dxva2 on different handles is safe" | ASSUMPTION (unverifiable) | `Handle` is only `Send` (dxva2.rs:44-45); nothing serializes across links |

### Section E

| ID | Claim | Verdict | Evidence |
|---|---|---|---|
| E1 | ProfileStore is keyed by the data dir only; overlay edidId ignored | CONFIRMED | profile.rs:127-131 (`dir` field), :199, :228-232 |
| E2 | settings.json (store.rs:76-125) is global and mixes app-wide and per-monitor fields | CONFIRMED | store.rs:27-43. Note `save` is already atomic (temp + rename, :109-125); `CapabilitiesCache::save` (:318-319), `BackupStore::save` (:349) and `save_overlay` (profile.rs:204) are not |
| E3 | capabilities.txt (store.rs:303-324), backups (:326-366), risky-operation.txt (crash.rs:168) have fixed names | CONFIRMED content, crash line REFUTED | crash.rs:11 `const FILE`, :20-22 `Marker::new` |
| E4 | The Windows EDID registry is not read anywhere | CONFIRMED | No `Reg*` or `SetupDi*` calls anywhere in tvc-ddc. windows-sys features are only `Win32_Foundation`, `Win32_Graphics_Gdi`, `Win32_Devices_Display` (tvc-ddc/Cargo.toml) |
| E5 | dxva2/DisplayConfig give only manufacturer and product ids | CONFIRMED | `edid_id_from_path` (dxva2.rs:293-296); the name comes from `monitorFriendlyDeviceName` (:112) |
| E6 | Windows connector = instance segment, "the same string dxva2.rs:229/292 already parses" | PARTIAL | :229 is a doc comment. `edid_id_from_path` takes only `split('#').nth(1)`, the EDID id. The instance segment (`nth(2)`) is parsed nowhere |
| E7 | Linux identity source `/sys/class/drm/*/edid` | CONFIRMED | Already read without I²C in i2c.rs:145 |
| E8 | macOS `CGDisplaySerialNumber` or IODisplay EDID | UNSUPPORTED by code | ffi.rs binds only `CGDisplayVendorNumber`, `ModelNumber` and `IsBuiltin` (ffi.rs:36-38). No serial or EDID binding exists, so p2-discovery-macos must add one |
| E9 | "Rule 3 migrates today's single-monitor files" | PARTIAL | Rule 3 covers settings only. Root-level `capabilities.txt`, `profile.user.json`, `backup-*.json` and `risky-operation.txt` have no move step in the plan |

## 2. Write-set problems

### Phase 0

1. **p0-safety-floor vs p0-core-bugs and p0-app-bugs (hidden overlap).**
   - Adding a level parameter to `safety::evaluate` changes the calls at engine.rs:725 and :861, and the helper at tests/misc_tests.rs:17. engine.rs belongs to p0-core-bugs.
   - Making 0x60 need confirmation breaks every input switch the app sends today. All five 0x60 sends pass `confirmed: false`:
     - controller.rs:401 (CLI)
     - controller.rs:465 (tray "Switch to me")
     - controller.rs:546 (hotkey input)
     - controller.rs:550 (hotkey switch to me)
     - controller.rs:898 (the window's own input tile, sent after its own dialog)
   - controller.rs belongs to p0-app-bugs.
   - The same change breaks engine_tests.rs:798-835, which belongs to p0-core-bugs.
   - Refusing 0x04 always breaks engine_tests.rs:151-156 (a confirmed factory reset succeeds) and misc_tests.rs:48-51. It also removes a working Verified feature: the Reset group in p32p20.json. The audit said "deny 04 … unless Verified"; the plan says "always". That is a contradiction to settle.
   - The `--yes` CLI flag (Verification: "0x60 from the CLI without `--yes`") doesn't exist, and cli.rs has no Phase-0 owner.
2. **p0-parser "its tests"** are not in an mccs test file. They are in tests/profile_tests.rs (e.g. the no-spaces test at :35). p0-safety-floor's tests are in tests/misc_tests.rs. Name those files in the table.
3. **p0-core-bugs lists profile.rs**, but none of its six bullets needs profile.rs. The empty-hint fix belongs in dxva2.rs or main.rs:59. Either drop profile.rs or say why it's there. If the fix is "don't pass `Some("")`", that is in main.rs, which p0-app-bugs owns.
4. **Frontmatter vs diagram:** `p1-contracts depends-on: p0-core-bugs` only, but the mermaid graph shows all of P0 → p1. p1 doesn't conflict with the others' files, but it would build on a safety.rs whose signature is still in flux.

### Phase 1

5. **p1-contracts** adds `identity.rs` and `hub.rs`. It lists lib.rs (OK) but writes in two crates (tvc-core plus tvc-ddc/edid.rs). Fine, but the id-assignment function (serial uniqueness, then `-S`/`-P`/`-N`) needs a home and a signature. It can't live in tvc-ddc, because tvc-core can't depend on tvc-ddc, and the identity test in the Verification list needs it.

### Phase 2

6. **p2-hub** misses several files:
   - **tests/mod.rs**: needs `mod hub_tests;`, and Harness calls `Engine::spawn` at tests/mod.rs:63, which breaks if the signature changes to `EngineConfig`.
   - **engine_tests.rs**: 24 Command/Event::Result literals break once `op` is added.
   - **log.rs**: `log::write` is a global with no context (log.rs:19-24). An `[id]` prefix means a thread-local or a per-engine logger.
   - **controller.rs and view.rs**: removing `Event::Displays` breaks controller.rs:593-596; view.rs:147-170 renders `st.displays: Option<Vec<AttachedDisplay>>`. Adding `op` to Set/EndDrag/Step/ApplyPreset/RestoreBackup breaks 20 literals in controller.rs.
   - **main.rs**: `Engine::spawn` at main.rs:65.
   - CI runs `cargo test --workspace` and clippy on the whole workspace, so Phase 2 can't be green unless p2-hub edits tvc-app, or keeps the old `spawn`/`Displays` until p3.
7. **p2-hub vs p2-profiles (same phase, both need engine.rs).** These p2-profiles items are engine changes, and engine.rs belongs to p2-hub:
   - "Synthesized capabilities only for Verified" is engine.rs:266-267.
   - "ReadOnly after 3 consecutive write failures" is engine state.
   - `ResolvedControls.level` is set in `emit_controls` (engine.rs:1111).
   - The resolver must run after each caps read (engine.rs:637).

   Also, p2-hub's `MonitorHub::spawn(discovery, profiles, …)` takes "profiles", whose type p2-profiles is changing at the same time. Today's `ProfileStore` is bound to one dir (overlay path), so a per-monitor overlay means a factory per monitor.
8. **p2-discovery-windows, -linux, -macos all need tvc-ddc/src/lib.rs**, which none of them lists. `open()` (lib.rs:17-30) is the only public entry point. A `discovery()` constructor, the `mod` lines and the removal of `open` all go there, so three parallel tasks would collide on it. Also:
   - examples/probe.rs:10 calls `open`.
   - p2-discovery-windows needs **tvc-ddc/Cargo.toml**: windows-sys features `Win32_System_Registry`, plus `Win32_Devices_DeviceAndDriverInstallation` if it uses SetupAPI. Adding a new crate instead (e.g. winreg) changes Cargo.lock, which breaks `--locked` in ci.yml.
9. **p2-profiles** "per-monitor settings with migration from 0.0.8": the rule "first LEN62A2 seen" needs runtime knowledge of attached monitors. That is controller or hub code, not `SettingsStore::load`. Pin the API, e.g. `Settings::adopt_legacy(&MonitorId)`.

### Phase 3

10. **p3-tray-osd-cli vs p3-controller (same phase, both need controller.rs).** Each p3-tray-osd-cli feature needs controller code:
    - Hotkey dispatch is controller.rs:520-555.
    - CLI dispatch is :394-410, plus `print_get` (the `--get` array).
    - Tray wiring is :455-470.
    - A hotkey target setting needs store.rs, which has no Phase-3 owner.

    Separately, `CliCommand` and `Parsed` derive `Copy` (cli.rs:29, :43). A `--display <name>` String breaks `Copy`, and main.rs:31-44 and controller.rs depend on it.
11. **p3-ui** writes view.rs and `ui/*.slint` after p3-app-contract wrote `app.slint` and the view.rs stub. That is sequential, so OK. But p3-controller also changes view.rs inputs (the `State` split), which conflicts with p3-ui's view.rs.

### Phase 4

12. **p4-backlight** lists only `backlight/`. It also needs tvc-ddc lib.rs (`mod`), Cargo.toml (WMI: `Win32_System_Wmi` and `Win32_System_Com`, or a crate), and changes in the Windows and Linux discovery code so `enumerate()` reports built-in panels with `BackendKind::Backlight` and `open` returns the backlight link.

## 3. Contract problems

1. **The names `NeedsConfirm` and `p0 level` don't exist.**
   - Code has `safety::Verdict::NeedsConfirmation` (safety.rs:11) and `WriteOutcome::NeedsConfirmation` (engine.rs:130).
   - "A new parameter that defaults to `Verified`" can't compile: Rust has no default arguments, and `SupportLevel` is only added in p1. Phase 0 needs either a temporary enum, or `evaluate` kept plus a new `evaluate_at(level, …)`.
2. **The engine-death contracts can't fire in release builds.** Cargo.toml has `[profile.release] panic = "abort"`. So "An engine thread panic is logged and emitted as `Event::Status{NotResponding}`" (p0-core-bugs) and `HubEvent::EngineDied` can only happen in debug builds and tests. A panic aborts the whole app. Either change the profile (binary size, and the crash-marker semantics in crash.rs:5 depend on it) or drop or re-scope these. The `.expect("spawn the engine thread")` at engine.rs:307 also aborts on hotplug.
3. **`Discovery::open -> Option<Box<dyn MonitorLink>>` loses `LinkFailure`.**
   - PermissionDenied and ModuleMissing (Linux, i2c.rs:67-70, :112) and UnsupportedPort (macOS) drive the setup hints today.
   - `enumerate() -> Vec` loses today's "backend can't list" (`Option<Vec<…>>`, link.rs:121-125, view.rs:149-156), and the plan maps "nothing listed" to `Monitors([])`.
   - Suggest `open -> Result<Box<dyn MonitorLink>, LinkFailure>` and `enumerate -> Result<Vec<DisplayInfo>, LinkFailure>` (or `Option`).
   - The bound link's own `connect()` still returns a `LinkFailure`, which is fine.
4. **`EngineConfig` is used by p2-hub but not pinned** (the audit had it; the plan dropped it). The engine also needs `identity` and `level` to fill `ResolvedControls.identity`/`.level`, so `EngineConfig` must carry `identity: MonitorIdentity` (and the resolver or level). Pin it, or p2-hub and p2-profiles will each invent one.
5. **Derive requirements.**
   - `ResolvedControls` derives `Clone, Debug, PartialEq, Eq`, so `SupportLevel` and `MonitorIdentity` must derive at least those four.
   - `HubEvent` holds `Event`, which is only `Clone, Debug` (engine.rs:232). So `HubEvent` can't derive `PartialEq`, and tests must match on it.
   - `DisplayInfo` must be `Clone + Debug`; `DisplayKind` is already `Copy, Eq, Hash`.
   - `OpId` should be `Copy, Eq, Hash, Debug`, because `Command` derives `Clone, Debug`.
   - `MonitorId` with serde works as a JSON map key (newtype String).
6. **`HubEvent::Batch { op, results: Vec<(MonitorId, WriteResult)> }` drops the routing fields.** `Event::Result` carries `origin`, `code` and `preset` (engine.rs:241); the controller routes window → card note and hotkey/tray/CLI → OSD by `origin`. Also unstated:
   - whether the per-monitor `Monitor{Result}` events are still sent alongside the Batch;
   - who allocates the `OpId` (caller or hub);
   - what happens for `Target::All` with `op: None`.
7. **`HubCommand::Send(Target, Command)` admits nonsense.** `Command::Shutdown` and `Command::Power` overlap `HubCommand::Shutdown` and `HubCommand::Power`. `Command::Flush(Sender<()>)` fanned out gives N replies on one sender (Harness::flush waits for one). There is no error event for an unknown `Target::One(id)`. Restrict the inner type, or document how each case is handled.
8. **`DisplayInfo` (plan) dropped the audit's `manufacturer`.** That's fine, since `identity.edid_id[..3]` has it. But the relation to the existing `AttachedDisplay`, `MonitorMatch` and `MonitorLink::displays()` is unstated: are they kept, deprecated or removed? Discovery backends and view.rs `not_found` both use them.
9. **`Timing` per monitor.** `Timing` is derived inside `ProfileStore::new` (profile.rs:137-143). The plan says timing comes from the matched profile, but no contract says who resolves it before `EngineConfig.timing` is filled. The hub gets `profiles` of an unspecified type.
10. **Identity can change while a monitor is attached.** `-S` requires the serial to be unique "among attached monitors". If a second monitor with the same serial is plugged in, the first monitor's id flips from `-S…` to `-P…`. The hub then sees Detached plus Attached for a monitor that never moved, and restarts its engine. The plan needs a "sticky id while attached" rule, or ids computed against saved state.

## 4. Windows device paths, the EDID serial, and dxva2 order

- **Two sources that are never joined today.**
  - `connect()` walks `EnumDisplayMonitors` → `GetPhysicalMonitorsFromHMONITOR`, and pairs each physical monitor with `EnumDisplayDevicesW(szDevice, i, EDD_GET_DEVICE_INTERFACE_NAME).DeviceID` by index (dxva2.rs:70-79, 230-248).
  - `displays()` uses `QueryDisplayConfig` + `DISPLAYCONFIG_TARGET_DEVICE_NAME.monitorDevicePath` (dxva2.rs:251-290).
  - Both give paths of the form `\\?\DISPLAY#LEN62A2#<instance>#{e6f07b5f-…}`. A bound `open(id)` can join them by **case-insensitive string equality** on the path, so it needs no QueryDisplayConfig ordering. To map a path to its HMONITOR, also query `DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME` (`viewGdiDeviceName` == `MONITORINFOEXW.szDevice`). The code doesn't do that today.
- **Order: there is no documented guarantee.**
  - The GetPhysicalMonitorsFromHMONITOR docs say only that one HMONITOR can have several physical monitors. Nothing about order, and nothing linking it to EnumDisplayDevices or QueryDisplayConfig.
  - Community code (node-ddcci, and Q&A answers) pairs physical monitors with EnumDisplayDevices entries by index under the same `szDevice`. That is observed behaviour, not a contract.
  - QueryDisplayConfig path order is unrelated to either, and nothing should depend on it.
  - The index pairing matters only when one HMONITOR has more than one physical monitor (clone mode). In that case, two identical clones can't be told apart by any dxva2 data anyway; `szPhysicalMonitorDescription` is often "Generic PnP Monitor".
- **No active-state filter.** `device_ids()` (dxva2.rs:242-245) doesn't filter by `DISPLAY_DEVICE_ACTIVE` in `StateFlags`. `take(physical.len())` only truncates the list; it doesn't skip an inactive entry in the middle. The audit claim (A4) is CONFIRMED, and the fix belongs in p2-discovery-windows ("skip inactive monitors").
- **Registry EDID.** `HKLM\SYSTEM\CurrentControlSet\Enum\DISPLAY\<EDIDID>\<instance>\Device Parameters\EDID` is the standard location. `<instance>` is path segment 3 (`5&2a1b&0&UID4352`); in effect, take the path between `\\?\` and `#{GUID}` and replace `#` with `\`.
  - Today nothing reads it (E4). It is readable without admin rights.
  - The cleaner route is SetupAPI: `SetupDiOpenDeviceInterfaceW(path)` → `SetupDiOpenDevRegKey(DIREG_DEV, KEY_READ)` → `RegQueryValueExW("EDID")`.
  - Either route needs new windows-sys features (see write-set item 8).
  - Caveats: the key can hold an override EDID (an `EDID_OVERRIDE` subkey), and the base block holds the 0xFF descriptor serial.
  - The plan correctly flags this as unverified on the P32p-20.

## 5. Missing work

1. A Phase-0 owner for the knock-on edits:
   - controller.rs: confirmed flags on the 5 input sends, or a deliberate exemption for "Switch to me".
   - cli.rs: `--yes`.
   - engine.rs: the safety call sites.
   - The engine tests in item 1 of section 2.
   - Merge p0-safety-floor's engine-facing part into p0-core-bugs, or make it sequential.
2. Decide 0x04: "always" (plan) vs "unless Verified" (audit). "Always" removes the P32p-20 factory reset, which the UI and tests use.
3. A migration of the root-level per-monitor files: `capabilities.txt`, `profile.user.json`, `backup-*.json` into `backups/`, and `risky-operation.txt`, into `monitors/<id>/`. Only settings.json migration is planned.
4. Per-monitor safe mode wiring. main.rs:62-64 reads and clears the root marker, and main.rs:102 clears it on exit. With N markers, the hub must read each monitor's marker when it spawns that engine, and on exit clear each one. `--get` runs a second process (main.rs:43-45) against the same per-monitor dirs.
5. Fake support for multi-monitor tests: a `FakeLink` with configurable caps and identity, and a `FakeDiscovery`. Both go under `#[cfg(any(test, feature = "fake"))]` (lib.rs:8-9). Without them, the Verification scenarios (LenovoFamily, GenericMccs, DEL, two links) can't run; fake.rs:107 always returns the P32p-20 string.
6. An id-assignment function in tvc-core (pure, taking `&[MonitorIdentity]` plus saved ids), and a sticky-id rule (contract item 10).
7. The fate of `MonitorMatch`, `AttachedDisplay`, `MonitorLink::displays()` and `tvc_ddc::open` (still used by examples/probe.rs), and a staged removal that keeps CI green per phase.
8. Linux: the bound link should take the bus from `/sys/class/drm/<conn>/ddc` and skip the per-reconnect EDID scan of every bus (i2c.rs:80-86). Otherwise N engines run unlocked EDID probes that interleave on shared buses.
9. macOS: ffi.rs bindings for the serial or EDID (`CGDisplaySerialNumber`, or an IOAVService/DCP EDID property). None exist, and "identity from the IODisplay EDID" isn't something Apple-Silicon IOKit is known to expose in the plane the code walks (locator.rs:74-107). Mark it unverified.
10. Stop the engine panicking at hotplug: `thread::Builder::spawn(...).expect` at engine.rs:307 must become a `Result` for the hub.
11. The `log.rs` context for `[id]` prefixes. Also the brightness-lock note is shown twice (engine.rs:763 hard-coded, plus controller.rs:690 from the profile). Remove it from the engine in p0 or p2.
12. Hotkey target and "Together" settings need store.rs fields, and the CLI exit code with several displays (the worst result? the first?) needs defining in cli.rs `exit_code`.

Sources: [GetPhysicalMonitorsFromHMONITOR (Microsoft Learn)](https://learn.microsoft.com/en-us/windows/win32/api/physicalmonitorenumerationapi/nf-physicalmonitorenumerationapi-getphysicalmonitorsfromhmonitor), [EnumDisplayDevicesW (Microsoft Learn)](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-enumdisplaydevicesw), [node-ddcci ddcci.cc](https://github.com/hensm/node-ddcci/blob/master/ddcci.cc), [Q&A: associating PHYSICAL_MONITOR with DeviceID](https://www.anycodings.com/1questions/2541728/how-to-associate-physicalmonitor-with-monitor-deviceid).
