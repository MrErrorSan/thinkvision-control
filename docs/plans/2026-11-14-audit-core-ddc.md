# Audit: tvc-core + tvc-ddc (branch feat/device-sourced-status, 3913f5a)

Read in full: tvc-core/src/{lib,link,mccs,engine,profile,profile/p32p20.json,store,cli,hotkey,safety,finder,result,crash,log,clock,fake}.rs, tvc-ddc/src/{lib,edid,packet,dxva2,linux/mod,linux/i2c,macos/mod,macos/link,macos/locator}.rs (macos/ffi.rs skimmed for ownership), tvc-app/src/main.rs:40-105 (wiring only). Nothing built or run. Paths below are relative to `crates/`.

Legend: **[1]** multi-monitor, **[2]** unverified-profile support, **[3]** laptop backlight backend, **[4]** names from the device or OS. "ASSUMPTION" = not provable from the code alone.

---

## A. Bugs and risks

### A1. Correctness (high)
- **RestoreBackup writes in the wrong order and can switch input.** `backup_values()` (tvc-core/src/engine.rs:951-957) keeps every cached Range/Enum code with `SafetyClass::Normal`. That includes **0x60 Input** (p32p20.json: Input is `"safety": "Normal"`) and 0xCC language. `restore_backup()` (engine.rs:924-937) replays the BTreeMap in ascending order, so it writes 0x10/0x12 **before** 0x14 colour mode, which is the reverse of `apply_preset` (engine.rs:908-922). A colour mode that locks brightness then yields `Kept`, and the restore stops at the first non-Ok (engine.rs:932). Restore can also move the monitor to the input that was cached at backup time (away from this PC), with no confirmation.
- **A corrupt `capabilities.txt` disables the live capability read.** At spawn, `cached_raw` is kept even when it doesn't parse (engine.rs:265-267: the caps fall back to synthesized, but `cached_raw` stays `Some`). `reconnect()` then skips the live read because `cached_raw.is_none()` is false (engine.rs:600). The app keeps running on **synthesized P32p-20 caps** until `ReadCapabilities` succeeds, and in safe mode that never happens (engine.rs:508). The cache write is not atomic (tvc-core/src/store.rs:318-319), so a crash in the middle of the write produces exactly this state. Fix: drop `cached_raw` when it fails to parse, and write the file via temp + rename.
- **Synthesized caps authorize writes.** `safety::evaluate` gates on `caps.supports(code)` and `allows_value` (tvc-core/src/safety.rs:364,375). Before any live read, the caps are `synthesize_capabilities()` from the profile (tvc-core/src/profile.rs:176-186). This is harmless today only because the link matches LEN62A2. For **[2]** it would authorize Lenovo codes and values on any monitor. Writes must require live caps, or caps cached for *this* monitor identity.
- **An empty description hint matches any monitor (Windows).** main.rs:59 passes `Some(profile.short_name)`, and `short_name` defaults to `""` when the JSON omits it (profile.rs:147). In dxva2.rs:84-88, `"…".contains("")` is always true, so the fallback picks **the first physical monitor of any make**. A future profile without `shortName`, or an empty `edid_id`, would control the wrong screen. Blocks **[1][2]**.
- **One bad nested list makes the whole monitor "not responding".** `mccs::parse` errors on the full string (see C). With no cache, `reconnect()` sets `NotResponding` (engine.rs:600-602), even though the monitor works.
- **0x60 confirmation exists only in the UI.** The engine never asks for confirmation on input (Normal safety, engine.rs:738-740). `confirmTitle` in p32p20.json is UI text. The CLI (`--input`), hotkeys and RestoreBackup switch without confirmation. **[2]** requires the confirmation rule in core (safety.rs).
- **NeverWrite depends on the profile, not on safety.rs.** CA/D6 are protected only because p32p20.json marks them NeverWrite (profile.rs:191, 212). safety.rs has no hard deny list, and 0x04 factory reset is writable after confirmation (p32p20.json, ConfirmRequired). Under **[2]**, a generic or Lenovo-family profile that omits CA/D6, or a finder overlay naming E0–FF, would be writable. A model-independent floor in `safety::evaluate` is needed: deny D6/CA always; deny 04 and unnamed E0–FF unless the profile level is Verified.

### A2. Races and stuck state
- **Drag state can stick forever.** `drag_to` inserts into `drags` (engine.rs:842) before coalescing. If the newest value fails `evaluate_drag` (engine.rs:846), or `EndDrag` never arrives (window hidden or closed mid-drag, controller bug), `drags` is never cleared. Effects: every `Poll` is skipped (engine.rs:701), and `Value` events for that code are suppressed (engine.rs:960), including changes made by a hotkey or Set during the drag. Nothing clears `drags` on status loss or reconnect.
- **Coalescing reorders commands.** `coalesce` (engine.rs:867-882) removes later `DragTo`s of the same code even when a `Set`/`Step` for that code sits between them, so the final value can be the later Set, not the latest drag. Minor.
- **Two processes can talk to the monitor at once.** `--get` runs its own engine while the resident copy runs (main.rs:43-51 comment). On Windows nothing serializes the two DDC streams. On Linux only per-transaction `flock` does (linux/i2c.rs:440-456). Both processes also share `capabilities.txt`, `risky-operation.txt` and the log in one data dir. ASSUMPTION: rare but real on a two-instance desktop.
- **Shutdown can abandon a call.** `EngineHandle::shutdown` waits 1 s (engine.rs:331-335), but `switch_input` sleeps 1.5 s (engine.rs:806), and `run_op` sleeps `retry`/`settle` without checking `stop`. The process can exit in the middle of a DDC transaction. This is acceptable by design, but note that the stop flag is checked only between attempts (engine.rs:1069).
- **One crash marker, single file** (crash.rs:168). With N engines, one engine's `disarm()` deletes another's armed marker, which defeats crash detection. Blocks **[1]**.

### A3. Error handling
- **An engine death is silent.** If the sink or worker panics, the thread dies. `EngineHandle::send` ignores the error (engine.rs:320-322), and no event tells the app. Add a `Dead` event, or check the `done` channel.
- `thread::Builder::spawn(...).expect` (engine.rs:307) panics on thread-spawn failure, which is acceptable for one engine but not for N engines spawned at hotplug time.
- `refresh()` counts `Unsupported` as answered (engine.rs:681), so a monitor that rejects every code is reported `Connected`. For **[2]**, decide whether "all unsupported" should mean ReadOnly or NotResponding.
- **Linux has no handling for the DDC null message.** A reply of `6E 80 BE` (null message, which some monitors send for an unsupported or busy code) fails both `parse_vcp_reply` and `is_unsupported_reply` (tvc-ddc/src/packet.rs:236-260). It is retried 10× (i2c.rs:252), then 5× more by the engine. That costs several seconds per code and ends `Transient`. A `Transient` on a verify read then marks the monitor lost (engine.rs:771-773 → 988-1007). ASSUMPTION: common on non-Lenovo monitors. This bites **[2]**.
- **The 1024-byte capability cap truncates silently** (linux/i2c.rs:108,314; macos/mod.rs:23; macos/link.rs:415). A longer string comes back cut off, and the parse fails with "Unbalanced parentheses". ASSUMPTION: some monitors exceed 1 KB, especially with `vcpname(...)`.
- **The macOS backend writes every Set VCP twice** (macos/link.rs:356-375, `WRITE_CYCLES=2`). The engine deliberately writes input once (engine.rs:796 comment: "a retry could switch twice"), but the backend defeats that, and action codes (04/05/08) are also sent twice.
- `SettingsStore::load`: a file missing `"Day"` or `"Night"` (no serde default, store.rs:133-134) counts as corrupt, resets everything, and overwrites the previous `.bak` (store.rs:80).
- `load_overlay`: one bad entry (an unparsable hex key) silently drops the **whole** overlay (profile.rs:231).

### A4. Resource use and leaks
- No real handle leaks found. dxva2 `Handle` drops via `DestroyPhysicalMonitors` (dxva2.rs:47-51), non-chosen handles are destroyed (dxva2.rs:95), and Linux fds and macOS CF/IO objects are RAII.
- Backups are never pruned (store.rs:338-351); each factory reset adds a file.
- Linux `connect()` reads the EDID over I²C on **every** candidate bus on every reconnect (i2c.rs:165-171), and the retry burst runs 7 reconnects. With N engines this becomes N × buses EDID probes, and those probes take no `flock` (i2c.rs:460-467), so they can interleave with another engine's write–sleep–read on the same bus.
- dxva2 `device_ids()` doesn't filter `EnumDisplayDevicesW` results by `DISPLAY_DEVICE_ACTIVE/ATTACHED` (dxva2.rs:242-245). It relies on the order matching `GetPhysicalMonitorsFromHMONITOR` (dxva2.rs:74-78). In clone or MST setups the index can point at the wrong physical monitor. ASSUMPTION about ordering. Blocks **[1]**.

---

## B. Single-monitor and model assumptions

| Where | What | Blocks |
|---|---|---|
| tvc-app main.rs:54-59 | One `ProfileStore`, one `MonitorMatch` built from the embedded profile's `edid_id` and `short_name`; one link, one engine (main.rs:65) | 1,2,4 |
| profile.rs:16 `EMBEDDED = include_str!("profile/p32p20.json")` | Exactly one profile, chosen at compile time; no profile registry and no matching by EDID or caps | 1,2 |
| profile.rs:111-123 `Profile` | `edid_id`, `display_name`, `short_name`, `inputs`, `colour_mode_order`, `brightness_lock_note` are hard-coded model data shown in the UI | 2,4 |
| profile.rs:152 | `custom_colour_mode` fallback `0x12` (a Lenovo value) | 2 |
| profile.rs:164-172 `resolve` | Every code outside the P32p-20 list becomes "Unknown 0xNN" ReadOnly/Hidden; there is no generic MCCS table (0x62 volume, 0x8D mute, 0x60 standard labels…) | 2,4 |
| profile.rs:176-186 | Synthesized caps come from the P32p-20 profile (see A1) | 2 |
| profile.rs:190-205, 228-232 | `profile.user.json` is one file per data dir. `edidId` is written (profile.rs:199) but **never checked on load**, so an overlay applies to whatever monitor is attached | 1,2 |
| p32p20.json (whole) | CA/D6 NeverWrite, 04/05/08 Action, 0x60 values 0x11/0x0F/0x31 (0x31 USB-C is Lenovo-specific, not MCCS), E0/EA/EC names | 2 |
| engine.rs:29-32 | `BRIGHTNESS/CONTRAST/COLOUR_MODE/INPUT` are standard MCCS (fine), but 0x14 preset values are model-specific | 2 |
| engine.rs:38, 62-77 | `Timing::default()` = P32p-20 pacing; `Timing` comes from the one profile (profile.rs:137-143, main.rs:69) | 1,2,3 |
| engine.rs:36 `ASLEEP_MESSAGE`, 980-981 "Monitor not found"/"not responding" | Messages assume one monitor | 1,4 |
| engine.rs:763 | Hard-coded "The current color mode may lock brightness." even though the profile already has `brightness_lock_note` (profile.rs:121), which the engine ignores | 2,4 |
| engine.rs:264, 273-274 | `CapabilitiesCache`, crash `Marker`, `BackupStore` all live in one dir with fixed file names (`capabilities.txt`, `risky-operation.txt`, `backup-*.json`) | 1 |
| engine.rs:233-251 `Event` | No monitor id on any event; `Displays` is engine-level and is sent only after a `NotFound` (engine.rs:593-595, 1016-1024) | 1 |
| engine.rs:174-216 `Command` | No target monitor; no correlation id for fan-out | 1 |
| engine.rs:302 | Thread name `tvc-engine` (one) | 1 |
| engine.rs:1111-1116 | `Controls` carries the single `profiles.profile()` | 1,2 |
| mccs.rs:186-191 `firmware()` | Reads the 0x04 value list as ASCII ("LB1.2"), a Lenovo quirk; generic firmware is VCP 0xC9 | 2,4 |
| store.rs:27-43, 48-62 `Settings` | One `this_machine_input` (default 0x11, store.rs:55,160); `day`/`night` presets with a model-specific `colour_mode` | 1,2 |
| cli.rs:402-403 `INPUTS` | `hdmi=0x11, dp=0x0F, usbc=0x31` hard-coded; no `--monitor` selector; `help()` text says "monitor" | 1,2,4 |
| hotkey.rs:106-108, 150-152 | `InputHdmi/InputDp/InputUsbC` actions tied to the P32p-20's three ports | 1,2 |
| link.rs:40-46 `MonitorMatch` | Target identified by `edid_id` (+ description hint) only, so two identical monitors are indistinguishable | 1 |
| link.rs:73-80 docs, `DisplayKind` | "Only `External` can be the monitor this app controls" | 3 |
| link.rs:94-104 `AttachedDisplay` | No connector, serial or OS path; `target: bool` means "is *the* target"; the `label()` fallbacks are generic English ("the built-in display") | 1,4 |
| link.rs:144-160 `MonitorLink` | DDC-shaped (capability string, VCP codes); backlight would need a synthetic caps string | 3 |
| dxva2.rs:70-81 | First physical monitor whose device path contains the EDID id wins | 1 |
| dxva2.rs:84-88 | Description-hint fallback ("P32p-20", or "" which matches everything) | 1,2,4 |
| dxva2.rs:104-116 `displays()` | `target` flag compares to the single `MonitorMatch`; drops `monitorDevicePath` and `connectorInstance` | 1,4 |
| linux/i2c.rs:146-200 | Scans all buses for the one EDID; with two identical monitors it picks the first that answers 0x10 (i2c.rs:173-193) | 1 |
| linux/i2c.rs:220-240 | `displays()` drops the connector name (`card0-DP-1`) and the ddc bus | 1,4 |
| macos/locator.rs:476-488 | `find` takes the first CG display with the vendor/model | 1 |
| macos/mod.rs:140-161 | `choose_service`: `SingleExternal` fallback can bind to a *different* monitor's proxy when the target display has none (e.g. a DisplayLink target and another monitor on USB-C) | 1 |
| macos link.rs | No `displays()` implementation (returns None) | 4 |
| macos/link.rs:241, i2c.rs:110 | Probe/verify code 0x10 ("every MCCS monitor answers it"); fine for monitors, but backlight is a different backend | 3 |
| examples/probe.rs:8-9 | Defaults "LEN62A2"/"P32p-20" (tooling only) | — |
| fake.rs:304-310 | Fake = P32p-20 only; tests can't cover generic or multi-monitor cases | 1,2 |

---

## C. MCCS capability parser (tvc-core/src/mccs.rs)

**Nested value lists: CONFIRMED rejected.**
- `parse_vcp` sees `F7`, then `(`. `matching_close` (mccs.rs:236-249) correctly finds the *outer* `)` using depth. `parse_values("04 07 08 12 14(00 02 03) 15 17")` (mccs.rs:271-281) reads `04 07 08 12 14`, then hits `(`. `read_hex_byte` fails (mccs.rs:290-291), and the `Err` propagates through `parse` (mccs.rs:212). **The whole capability string is rejected**, not just F7. Consequence: engine.rs:600-602 → NotResponding when there is no cache.

**Duplicated `60(...)` containing FF: NOT reproducible from the code.**
- No path rejects it. `FF` is valid hex (mccs.rs:290). A second `60(...)` simply **overwrites** the first through `BTreeMap::insert` (mccs.rs:263, 266): last wins, silently. So the bug is *silent data loss*, not rejection. If the first list held the real inputs, valid inputs become "not a valid value" (safety.rs:375-376) and are reported `OutOfRange` (engine.rs:969-975).
- ASSUMPTION: the observed rejection came from something else in the same string. The likeliest cause is that the same monitor's string also has the nested F7 list above, or a single-digit or 3+-digit token, a NUL, or a truncation (see list). Recommend adding the real string as a test before concluding.
- For safety under **[2]**: merge duplicates as a union for *display*, but flag the code as `ambiguous` and allow writes only to values present in every occurrence (or treat 0x60 as confirm-plus-read-back). Also never offer `FF` as an input target. ASSUMPTION: FF is a vendor "auto/none" sentinel.

**Other fragility**
1. Tokens must be exactly 2 hex digits (mccs.rs:289-293). `60(1 3 F)` or a 3-digit token fails the whole parse; a 4-digit token `DF(0201)` is silently split into `02 01`, a misparse (the "no spaces" test at profile_tests.rs:35 relies on this pairing).
2. Only ASCII whitespace is skipped (mccs.rs:283-287). Embedded NULs or other control bytes in the vcp section are fatal. dxva2 trims trailing NULs only (dxva2.rs:134), Linux likewise (i2c.rs:358); macOS cuts at the first NUL (macos/link.rs:431), which can hide the tail.
3. A `model(...)` error is fatal even though `model` is optional: `find_section(s, b"model")?` (mccs.rs:209) propagates "Unbalanced parentheses" after vcp parsed fine.
4. `find_section` only looks at depth ≤ 1 (mccs.rs:219). An unbalanced earlier section (e.g. `cmds(01 02` missing `)`) pushes `vcp(` to depth 2, which gives "no vcp section". `vcp (` with a space before the paren is also not found.
5. Only the first `vcp(` is used (mccs.rs:216-234). ASSUMPTION: some monitors split their codes across two `vcp(...)` sections, and the second would be ignored.
6. Truncation at 1024 bytes in the Linux and macOS backends leads to an unbalanced-paren error (A3).
7. Ignored sections that **[2][4]** need: `vcpname(...)` (the device's own names for codes; the right source for **[4]**), `mccs_ver(...)` (generic level gate), `type(...)`, `cmds(...)` (whether E3/F3 capabilities and 03 set are supported).
8. Error positions are offsets into the vcp slice, not into the whole string (mccs.rs:291). Cosmetic.
9. `firmware()` assumes the Lenovo ASCII-in-0x04 convention (mccs.rs:186-191). On another monitor a 0x04 value list of printable bytes would be shown as "firmware".

**Proposed fix shape:** a tokenizer producing `Vec<VcpEntry { code, values: Option<Vec<ValueEntry { value, sub: Option<Vec<u8>> }>> }>`; keep top-level `values` for `allows_value`; record `nested`/`duplicate` flags per code so **[2]** can force such codes to read-only. Make `model` errors non-fatal. Make per-entry errors skip that entry and log it, instead of failing the whole string (keep "no vcp section" fatal).

---

## D. Engine architecture

**Today**
- `Engine::spawn(link, profiles, dir, timing, safe_mode, sink) -> EngineHandle` (engine.rs:259-310). One `Worker` on thread `tvc-engine` owns `Box<dyn MonitorLink>`, the `ProfileStore`, the caps cache, the crash marker, backups, the value cache, drags, status, burst and power flags (engine.rs:349-384).
- The app sends `Command`s over an mpsc channel (`EngineHandle::send/sender`, engine.rs:319-327). The worker emits `Event`s through one `Box<dyn Fn(Event)+Send>` sink (engine.rs:1102-1104), which in the app is `slint::invoke_from_event_loop` (main.rs:71-73).
- `MonitorStatus` (engine.rs:98-103) is `Worker.status`, changed only in `set_status` (engine.rs:1026-1038) and emitted as `Event::Status{status, failure}`.
- `MonitorLink` (link.rs:146-160) is called only inside `run_op`/`connect` (engine.rs:1040-1096), which apply gap, settle, retry and Gone handling.
- Power is per engine (engine.rs:549-572). Discovery of "what's attached" is folded into the link (`displays()`) and reported only on NotFound.

**Option A: N engines, one per monitor, under a hub (RECOMMENDED)**
- Each monitor has its own DDC channel (dxva2 handle, i2c bus or IOAVService). One slow monitor (2.3 s caps read, 1.5 s input confirm, 5 s+ null-message retries on Linux) must not block the others. The worker is already self-contained per link, so little of engine.rs changes. Per-monitor timing, safe mode and support level fall out naturally.
- Required changes:
  - (a) Discovery leaves the link. The hub enumerates once per display event and gives each engine a link **bound to a concrete target** (device path, bus or registry id), not an EDID search.
  - (b) Per-monitor data dir, so the caps cache, marker and backups stop colliding (engine.rs:264, 273-274).
  - (c) Events tagged with the monitor id by the hub's sink wrapper.
  - (d) An op id on result-producing commands, for "all" fan-out.
  - (e) The hub broadcasts `Power`.
  - (f) `Event::Displays` and `report_displays` (engine.rs:1016-1024) move to the hub.
  - (g) Thread name includes the id; log lines get an `[id]` prefix.
  - (h) The ProfileStore is resolved per monitor (support level).
- ASSUMPTION: concurrent dxva2 calls on *different* physical-monitor handles are safe. Most tools serialize them anyway; the hub could optionally hold a global "Windows DDC" mutex if testing shows I²C errors.

**Option B: one engine managing N links**
- Serializes all monitors on one thread, so the worst monitor's latency hits everyone. Every `Worker` field (cache, drags, burst, status, last_write, ready_at…) would have to move into a per-link struct, and the drag coalescing in `next()` gets complicated. Only advantage: free global serialization. Not recommended.

**Proposed public contracts (tvc-core)**
```rust
// identity.rs
pub struct MonitorId(pub String);              // stable key, see E; Ord + Hash + Display
pub struct MonitorIdentity {
    pub edid_id: String,                       // "LEN62A2"
    pub serial: Option<String>,                // EDID 0xFF descriptor, else bytes 12-15 when non-trivial
    pub connector: Option<String>,             // OS port: Windows device-interface instance, Linux "card0-DP-1", macOS IODisplayLocation
}

// discovery (trait in core, impl in tvc-ddc / backlight crate)
pub struct DisplayInfo {
    pub id: MonitorId,
    pub identity: MonitorIdentity,
    pub name: String,            // EDID 0xFC or OS friendly name; never from a profile
    pub manufacturer: String,    // PNP id ("LEN"); UI may map to a vendor name table
    pub kind: DisplayKind,       // BuiltIn | External | Virtual
    pub backend: BackendKind,    // Ddc | Backlight | None
}
pub trait Discovery: Send {
    fn enumerate(&mut self) -> Vec<DisplayInfo>;
    fn open(&mut self, id: &MonitorId) -> Option<Box<dyn MonitorLink>>; // bound to that target
}

pub enum SupportLevel { Verified { profile: String }, LenovoFamily, GenericMccs, ReadOnly }

pub struct EngineConfig {
    pub id: MonitorId, pub link: Box<dyn MonitorLink>, pub profiles: ProfileStore,
    pub dir: PathBuf /* per monitor */, pub timing: Timing, pub safe_mode: bool,
}
Engine::spawn(cfg: EngineConfig, sink: Box<dyn Fn(Event) + Send>) -> EngineHandle; // unchanged Event inside

pub struct OpId(pub u64);    // add `op: Option<OpId>` to Set/EndDrag/Step/ApplyPreset/RestoreBackup and Event::Result

pub enum Target { One(MonitorId), All }
pub enum HubCommand {
    Send(Target, Command),   // All: fan out; relative Step per monitor; input switch needs one confirm for the batch
    Rescan,                  // display/device event
    Power(PowerState),       // broadcast
    Shutdown,
}
pub enum HubEvent {
    Monitors(Vec<DisplayInfo>),                       // full list after each rescan (replaces Event::Displays)
    Attached(DisplayInfo), Detached(MonitorId),
    Monitor { id: MonitorId, event: Event },          // Status/Controls/Value/Result/Finder/Asleep/SafeMode
    Batch { op: OpId, results: Vec<(MonitorId, WriteResult)> }, // aggregate for Target::All
    EngineDied(MonitorId),
}
```
- `ResolvedControls` (engine.rs:164-171) gains `level: SupportLevel` and `identity: MonitorIdentity`. `profile.display_name` stops being the UI name; the UI uses `DisplayInfo.name`, then the caps `model`, then `edid_id`.
- **[3] backlight:** implement `MonitorLink` for WMI (`WmiMonitorBrightness`/`…Methods`) and `/sys/class/backlight/*` (brightness/max_brightness). `capabilities()` returns the synthetic `"(type(LCD)vcp(10))"`, `read/write(0x10)` scale to 0..max. Use a "builtin-backlight" profile with `Timing::zero()`-like pacing. `DisplayKind::BuiltIn` stops meaning "not controllable" (link.rs:74 doc). ASSUMPTION: no other codes needed.

---

## E. Profiles, settings and per-monitor persistence

**How things are keyed today**
- Profile: compile-time single JSON (profile.rs:16). `ProfileStore` is keyed by the data dir only. The overlay `profile.user.json` is one file per dir, and `edidId` is written but ignored on load (profile.rs:199 vs 228-232).
- Settings: `settings.json` (store.rs:76-125) is global. It mixes app-global items (hotkeys, start_with_windows, background_mode, auto_day_night) with **per-monitor** items: `this_machine_input`, and `day/night.colour_mode` (a model-specific value space).
- `capabilities.txt` (store.rs:303-324), `backup-*.json` (store.rs:326-366) and `risky-operation.txt` (crash.rs:168) each have fixed names in the one dir.

**Proposed identity (`MonitorId` string)**
1. `"{EDID_ID}-S{serial}"` when a serial exists **and** is unique among attached monitors with that EDID id.
   - serial = EDID descriptor tag 0xFF text (trimmed), else EDID bytes 12–15 as `%08X` when not `00000000`/`01010101`. ASSUMPTION: those two are the common placeholder values.
   - Sources: Linux `/sys/class/drm/*/edid` (full EDID, no I²C traffic). Windows registry `…\Enum\DISPLAY\<id>\<instance>\Device Parameters\EDID` via SetupAPI (ASSUMPTION: not read anywhere today; dxva2/DisplayConfig give only the manufacturer and product ids). macOS `CGDisplaySerialNumber` or the IODisplay EDID.
2. Fallback `"{EDID_ID}-P{connector}"` when there is no usable serial, or the serial is duplicated (cloned EDIDs).
   - connector: Windows = the instance segment of `monitorDevicePath` (`5&2a1b&0&UID4352`, the same string dxva2.rs:229/292 already parses); Linux = DRM connector (`card0-DP-1`); macOS = IODisplayLocation path or the CG display UUID.
   - This is stable per port, but changes if the cable moves.
3. Last resort `"{EDID_ID}-N{ordinal}"` (enumeration order) when there is neither a serial nor a connector, or no EDID: `"UNKNOWN-N{n}"`. Mark it unstable; don't persist writes-enabling state under it.

**Matching saved monitors on load**
1. Exact id.
2. Same `edid_id` + same connector.
3. Exactly one saved entry with that `edid_id`: adopt it and rewrite the id.

Rule 3 also migrates today's single-monitor files to the first LEN62A2 seen.

**Storage layout**
- `settings.json`: keep the app-global fields (0.0.4 compatibility, PascalCase). Add `"Monitors": { "<id>": { "Name": last-seen-name, "ThisMachineInput": "0x11", "Day": {...}, "Night": {...}, "Included": true } }`, with per-monitor preset overrides; a missing entry inherits the global preset minus `colour_mode`.
- `monitors/<sanitized id>/`: `capabilities.txt`, `profile.user.json` (verify the stored `edidId` on load; ignore the file on mismatch), `backups/`, `risky-operation.txt`. The safe mode a crash triggers then applies only to that monitor.
- Hotkeys stay global but need a target: `Target::All`, or a "focused monitor" setting. ASSUMPTION: product decision. The input hotkeys (hotkey.rs:106-108) should become "input slot 1/2/3" resolved per monitor from caps and profile, or be limited to Verified monitors.
- Profiles: a registry `Vec<ProfileSource>` matched by `edid_id` exact (Verified). Lenovo family = PNP `LEN` **and** caps values ⊆ the profile's labelled values, per code. Otherwise a GenericMccs table, otherwise ReadOnly. `Timing` comes from the matched profile, else the defaults.
