# Verification of audit sections A and C (tvc-core / tvc-ddc @ 3913f5a)

Worktree: `scratchpad/wt-verify` (detached, HEAD 3913f5a). Reproductions are in `crates/tvc-core/src/tests/verify_tests.rs`, registered in `tests/mod.rs`. The command `cargo test -p tvc-core verify -- --nocapture --test-threads=1` gives **17 passed, 0 failed**. The full output is in `scratchpad/verify-run.txt`. Nothing was fixed or committed.

**Line-number caveat.** The audit's engine.rs, store.rs, profile.rs, dxva2.rs and main.rs citations match HEAD. Its citations into **mccs.rs, safety.rs, crash.rs, packet.rs, linux/i2c.rs and macos/link.rs do not**: mccs.rs has 134 lines and safety.rs has 48, yet the audit cites mccs.rs:290 and safety.rs:364. The code content it describes is right. The correct lines are given below.

## Section A

| ID | Claim | Verdict | Evidence | Sev | Fix |
|---|---|---|---|---|---|
| A1.1 | RestoreBackup writes in ascending code order, so 0x10/0x12 go before 0x14, it stops at the first non-Ok, and it switches input 0x60 without confirmation | **CONFIRMED** | `backup_values` engine.rs:951-957 keeps every Normal Range/Enum value. Test backup = `{10,12,14,16,18,1A,60,86,CC,E0,EA,EC}`. Restore engine.rs:930-933 iterates the BTreeMap. With 0x10 locked, `writes [(0x10,0)]` and the result is "Brightness remains 50…" (Kept). 0x14 is never written. Unlocked, the writes are `10,12,14,…,(0x60,0x11),…` with no confirmation. Caveat: the backed-up 0x60 is the input cached at backup time, normally this PC's, so "away from this PC" needs a stale cache. | high | Exclude 0x60 (and probably 0xCC) from backups. Restore in `apply_preset` order (0x14 first) and carry on past Kept. |
| A1.2 | A corrupt `capabilities.txt` stops the live read; only ReadCapabilities fixes it, never in safe mode; the cache write is not atomic | **PARTIAL** | engine.rs:265-267 keeps `cached_raw`, and engine.rs:600 then skips the read. Test: `capability reads after Connect 0`, caps synthesized (model ""). safe=false: ReadCapabilities performs 1 read and recovers. safe=true: 0 reads, so it never recovers (engine.rs:508). The app sends ReadCapabilities **once**, 2 s after Connected, and never in headless/CLI runs (tvc-app controller.rs:630-633). store.rs:318-319 does a plain `fs::write`. So in the normal resident case it self-heals after about 2 s. | medium | Set `cached_raw = None` when the cache doesn't parse. Write the cache via temp + rename. |
| A1.3 | Synthesized caps authorize writes before any live read | **PARTIAL** | True only when no live read happens before Connected: safe mode, or a corrupt or foreign cache. In the normal path with no cache, engine.rs:600 reads live caps before `Connected`, and `set` requires Connected (engine.rs:719). Test (safe mode, monitor advertises only `vcp(10 12)`): engine caps claim 0x60 and 0x14, and `Set 60=0F -> Ok; Set 14=0E -> Ok; writes [(0x60,0x0F),(0x14,0x0E)]`. Gate at safety.rs:27 (`caps.supports`) and :38 (`allows_value`). | medium (high for [2]) | Track caps provenance. Allow writes only with live caps, or with cached caps for a matching monitor identity. |
| A1.4 | An empty description hint matches any monitor (Windows) | **CONFIRMED (latent)** | main.rs:59 passes `Some(profile.short_name.clone())`. profile.rs:147 uses `unwrap_or_default()`. dxva2.rs:84-88 uses `.contains(&hint)`, and `contains("")` is always true, so it picks the first physical monitor. dxva2.rs:72 also skips the EDID match when `edid_id` is empty. Not triggered today (`shortName` = "P32p-20"). | low now / high for [1][2] | Pass `None` when the hint is empty. Refuse to connect when `edid_id` is empty. |
| A1.5 | One bad nested list makes the monitor "not responding" | **CONFIRMED** | Test with the P40w-20 string, no cache: `statuses [Connecting, NotResponding], capability reads 1, value reads during connect: []`. The engine reports NotResponding without reading any value (engine.rs:600-602). Every burst step repeats the caps read. | high | Make the parser tolerant (skip or flag the bad entry). If caps don't parse, continue on synthesized or read-only caps instead of NotResponding. |
| A1.6 | 0x60 confirmation exists only in the UI | **CONFIRMED** | p32p20.json:37: Input is `"safety":"Normal"`. engine.rs:738-740 goes straight to `switch_input`. Test: `Set 60=0F unconfirmed -> Ok "Input → DisplayPort"`. A hotkey-origin `Step{0x60,+0x22}` gives `Ok "Input → USB-C"` with no confirmation. The only dialog is in controller.rs:898-913. | medium | Put the input-confirm rule in core: a confirm class for 0x60 that CLI and hotkey paths pass as confirmed explicitly. |
| A1.7 | NeverWrite depends on the profile, with no floor in safety.rs | **CONFIRMED** | safety.rs:31-34 checks only `def.safety`. The module doc (safety.rs:1) claims "never write 0xCA, 0xD6", but nothing hard-codes it. Test: `evaluate(D6 Normal,5)=Allow`, `evaluate(CA Normal,1)=Allow`. A finder overlay can name F7 as Normal, giving `Set F7=04 -> Ok, writes [(0xF7,4)]`. CA/D6 are protected today only by profile.rs:191 and :212. | medium (high for [2]) | Add a hard deny for D6/CA in `safety::evaluate`. Deny 0x04 and unnamed E0–FF unless the profile is Verified. |
| A2.1 | Drag state can stick forever | **PARTIAL** | The "newest value fails evaluate_drag" trigger is unreachable. For a Range code, `evaluate` blocks only on code-level conditions (unsupported, NeverWrite, ReadOnlyUntilNamed), and those already passed at engine.rs:838, with nothing changing them before the call at 846. The missing-EndDrag path is real. Test: after a DragTo with no EndDrag plus `RecheckAfterEvent`, `poll reads []`, and a hotkey `Set 10=70` emits **no** Value event for 0x10. engine.rs:701, :960, and only :462 clears `drags`. EndDrag depends on a debounce timer after `released` (controller.rs:921-929). | medium | Clear `drags` on status loss, reconnect and window hide. Add a drag timeout (for example 2 s since the last DragTo). |
| A2.2 | Coalescing reorders commands | **CONFIRMED** | engine.rs:867-882. Test: sent `DragTo 30, Set 50 (hotkey), DragTo 70`, observed writes `[(0x10,70),(0x10,50)]`, final value 50 (the user's last input was 70). | low | Stop the coalescing scan at the first non-DragTo command for the same code. |
| A2.3 | Two processes can talk to the monitor at once | **CONFIRMED** | main.rs:43-51: `--get` skips the single-instance check and runs its own engine and link. Windows has no cross-process lock. Linux uses a per-transaction `flock` only (i2c.rs:166, 199, 236; the audit's 440-456 is wrong). Both processes share `data_dir`. See also N1. | low | Forward `--get` to the resident copy, or take a named mutex around DDC ops. |
| A2.4 | Shutdown can abandon a call in flight | **CONFIRMED** | `SHUTDOWN_WAIT`=1 s (engine.rs:34, 331-335). `input_confirm` 1.5 s `sleep_until` (engine.rs:806). `stopping()` is checked only at the top of each attempt (engine.rs:1069), not in the settle and retry sleeps (1073, 1079). By design. | low | Use interruptible sleeps. |
| A2.5 | One crash marker, shared single file | **CONFIRMED** | crash.rs:11 `FILE = "risky-operation.txt"` (not :168). Test: A arms, B arms then disarms, and `A.left_over() = None`. | low now / high for [1] | Use a marker per engine (per-monitor dir). |
| A3.1 | An engine death is silent | **CONFIRMED** | engine.rs:320-322 discards the `send` error. The `done` channel is used only by shutdown. Test with a panicking sink: thread `tvc-engine` panicked, `send()` returned silently, `Flush reply = Err(Disconnected)`, no event. | medium | Expose `is_alive()`, or a `send` that returns `Err`. Have the app react. |
| A3.2 | `spawn(...).expect` panics | **CONFIRMED** | engine.rs:307. | low | Return a `Result` from `Engine::spawn`. |
| A3.3 | `refresh()` counts Unsupported as answered, so a monitor that rejects every code shows Connected | **CONFIRMED** | engine.rs:681. Test with every read Unsupported: `statuses [Connecting, Connected]`. | low (medium for [2]) | Require at least one Ok reading for Connected. |
| A3.4 | Linux has no DDC null-message handling, so each code costs seconds and ends Transient | **CONFIRMED (by reading)** | `6E 80 BE` fails `parse_vcp_reply` (packet.rs:127, which needs `reply[1]==0x88`) and `is_unsupported_reply` (packet.rs:144). The audit's 236-260 is wrong. `READ_ATTEMPTS=10` (i2c.rs:19, loop at 167-186), and `last_errno=0` gives Transient (i2c.rs:291-296). The engine then retries 5× (engine.rs:1078), about 50 transactions of at least 100 ms each, so more than 5 s per code. A Transient on the verify read leads to `lost()` (engine.rs:771-773, 998-1001). Capability fragments behave the same way: packet.rs:157-158 rejects length 0, retried 8× per fragment. How often monitors send null messages is an ASSUMPTION. | medium | Detect the null message in packet.rs: Unsupported for Get VCP, or "busy" with a short capped retry. |
| A3.5 | The 1024-byte cap truncates silently, then the parse fails with "Unbalanced parentheses" | **PARTIAL** | Silent truncation is confirmed: Linux i2c.rs:23 and :229 return `Ok` with about 1024 bytes; macOS mod.rs:23, link.rs:192. dxva2 has no cap. But the parse fails only if the cut lands **inside `vcp(...)`**: `find_section` only needs vcp's own close (mccs.rs:65, 76-89), and the outer wrapper is never checked. A cut in `vcpname` or `mccs_ver` after vcp parses silently. | low | Raise the cap, and log or flag truncation. |
| A3.6 | macOS writes every Set VCP twice | **CONFIRMED (by reading)** | `WRITE_CYCLES=2` (macos/mod.rs:17). `write_twice` (macos/link.rs:133-152; the audit's 356-375 is wrong) is used by `write` (link.rs:224-231). So the engine's single-write input path (engine.rs:796) and actions 04/05/08 go out twice. | medium | Use one write cycle for Set VCP, at least for 0x60 and Action codes. |
| A3.7 | Settings without "Day"/"Night" count as corrupt and overwrite the previous `.bak` | **CONFIRMED** | store.rs:133-134 (no `serde(default)`), :79-81 rename. Test: a file with only Night and `ThisMachineInput "0x0F"` loads as defaults (input 0x11). The previous `.bak` ("OLDER BACKUP") is replaced. | medium | Add `#[serde(default)]` on day/night. Use timestamped `.bak` names. |
| A3.8 | `load_overlay`: one bad entry drops the whole overlay | **CONFIRMED** | profile.rs:231 `collect::<Option<Vec<_>>>().unwrap_or_default()`. Test: overlay with a good 0xF7 and a bad `0xZZ` entry resolves 0xF7 to "Unknown 0xF7". | low | Use `filter_map` and log the skipped entries. |
| A4.1 | No real handle leaks | **CONFIRMED (agree)** | dxva2.rs:47-51, 92-95. Linux `File` and macOS `Cf` are RAII. | — | — |
| A4.2 | Backups are never pruned | **CONFIRMED** | store.rs:338-351. Test: 3 resets leave 3 `backup-*.json` files. | low | Keep the last N. |
| A4.3 | Linux reads EDID over I²C on every bus at every reconnect, without `flock` | **CONFIRMED (by reading)** | i2c.rs:80-86 (the audit's 165-171 is wrong). `read_edid` (i2c.rs:375-382) opens its own fd and takes no lock. | low (medium for [1]) | Prefer `/sys/class/drm/*/edid`. Lock around the EDID probe. |
| A4.4 | dxva2 `device_ids` doesn't filter by ACTIVE/ATTACHED and relies on ordering | **PARTIAL (unprovable)** | The code is as described: dxva2.rs:242-245 pushes every `EnumDisplayDevicesW` entry with no `StateFlags` check, and :74-78 maps by index after `take(physical.len())`. Whether the order diverges in clone or MST setups is an ASSUMPTION. | low (medium for [1]) | Filter `DISPLAY_DEVICE_ACTIVE`, or match via the DisplayConfig target path instead of the index. |

## Section C: MCCS parser (P40w-20 string)

Correct lines in mccs.rs: `parse` 46-53, `find_section` 56-74 (depth ≤ 1 at :59), `matching_close` 76-89, `parse_vcp` 91-109 (insert at :103/:106), `parse_values` 111-121, `skip_spaces` 123-127, `read_hex_byte` 129-134, `firmware` 27-31.

| ID | Claim | Verdict | Evidence | Sev | Fix |
|---|---|---|---|---|---|
| C-nested | A nested value list rejects the whole string | **CONFIRMED** | `parse(P40w-20) = Err("Expected a two-digit hex value at position 14 in the vcp section.")`. `parse_values("04 07 08 12 14(00 02 03) 15 17")` hits `(` at offset 14, `read_hex_byte` (:130-131) errors, and the error propagates through :103 and :52. Engine consequence: NotResponding (A1.5). | high | Use a tokenizer that accepts `XX(sub…)` inside value lists: keep the top-level value and flag `nested`. |
| C-dup60 | A duplicate `60(...)` with FF is not rejected; the last one wins silently | **CONFIRMED** | With only the nested list removed, the string parses. `0x60 = [0F 11 12 1B 31]` (the second list), so the first list with FF is lost. The duplicate alone parses OK. Reversed order gives `[0F 11 FF]`. FF never causes a rejection, so the reported failure comes from the nested F7 list. | medium | Merge duplicates and flag the code ambiguous: write only values present in every occurrence. Never offer FF as an input. |
| C1 | Tokens must be exactly 2 hex digits; 4 digits silently split | **CONFIRMED** | `60(1 3 F)` → Err at pos 0. `60(011)` → Err. `DF(0201)` → `Ok {DF:[02,01]}` (a misparse). | medium | Tokenize on whitespace and parens. Accept 1-2 digit tokens, and flag or skip longer ones per entry. |
| C2 | Only ASCII whitespace is skipped, so NUL and control bytes are fatal | **CONFIRMED** | `vcp(10 12\0 14)` → Err at pos 5. Tab/CRLF are fine. Correct lines: dxva2.rs:134 `trim_end_matches('\0')`, i2c.rs:273, macos/link.rs:208 cuts at the first NUL. | low | Treat bytes below 0x20 as separators. |
| C3 | A `model(...)` error is fatal | **CONFIRMED** | `(prot(monitor)vcp(10 12)model(P40w` → `Err("Unbalanced parentheses at position 29.")` although vcp parsed. mccs.rs:49 uses `?`. | low | Use `.ok().flatten()` for model. |
| C4 | An unbalanced earlier section or `vcp (` hides vcp | **CONFIRMED** | `(prot(monitor)cmds(01 02 vcp(10 12))` → "no vcp(...) section". `vcp (10 12)` → same. mccs.rs:59-61. | low | Allow whitespace before `(`. Fall back to the first `vcp(` at any depth. |
| C5 | Only the first `vcp(` is used | **CONFIRMED** | `(vcp(10 12)vcp(14(01 02)))` → `{10,12}`, and 0x14 is dropped (the `return` at mccs.rs:65). Whether real monitors do this is an ASSUMPTION. | low | Merge all depth-1 `vcp(` sections. |
| C6 | Truncation at 1024 gives an unbalanced-paren error | **PARTIAL** | See A3.5: only when the cut lands inside vcp. | low | As A3.5. |
| C7 | `vcpname`, `mccs_ver`, `type` and `cmds` are ignored | **CONFIRMED** | `MonitorCapabilities` holds only `model` and `vcp` (mccs.rs:8-11). | low (feature) | Parse them into optional fields. |
| C8 | Error positions are vcp-slice offsets, which is cosmetic | **PARTIAL (worse than claimed)** | Positions are offsets into the **innermost slice being parsed**: the value list for value errors. P40w reports "position 14", while the `(` is at 296 in the vcp slice and 363 in the whole string. Misleading in logs. | low | Carry a base offset through `parse_vcp` and `parse_values`. |
| C9 | `firmware()` assumes ASCII in 0x04 (a Lenovo quirk) | **CONFIRMED** | `vcp(04(41 42) 10)` → `firmware = "AB"`. mccs.rs:27-31. P40w has no 0x04 list, so `None`. | low | Apply it only with a Verified Lenovo profile. Otherwise read 0xC9. |

## New bugs found

| ID | Where | Bug | Sev | Fix |
|---|---|---|---|---|
| N1 | tvc-app main.rs:44-52, 62-64, 102 | A `--get` run bypasses the single-instance check, then **clears the crash marker** at start (:64) and at exit (:102). If the resident copy is mid capability read or finder op, its armed marker is deleted, which defeats crash detection. The `--get` process also starts itself in safe mode when it sees the resident's live marker. | medium | Leave the marker alone in `--get` runs, or use per-process markers. |
| N2 | tvc-app controller.rs:630-633 | `ReadCapabilities` is sent once per process (`caps_requested.replace(true)`). If that read fails (Transient, parse error, asleep), it is never retried, so the session stays on cached or synthesized caps. This compounds A1.2 and A1.3. | low-medium | Re-request on later Connected events until caps are live. |
| N3 | mccs.rs:21 + safety.rs:38-39 | An Enum code advertised **without** a value list (for example a bare `60`) is `Some(None)`, so `allows_value` is true for any value. Any byte can then be written to 0x60 or 0x14. | low now / medium for [2] | For Enum codes with no caps list, allow only profile-labelled values. |
| N4 | engine.rs:600-602 | When the live caps read fails or doesn't parse with no cache, the status is NotResponding without any value read (test: `value reads during connect: []`). Each of the 7 burst steps repeats the ~2.3 s caps read. | medium | Probe a value (0x10) to decide responsiveness. Remember the parse failure for the process. |
| N5 | engine.rs:930-935 | `restore_backup` stops at the first non-Ok (Kept, Blocked for a code no longer in caps, Unsupported). It reports only that result, with no indication of what was already restored. | low | Continue through all codes and return an aggregate result. |
| N6 | store.rs:354-365 | `load_latest` errors if the newest backup is corrupt and doesn't fall back to an older one. | low | Try the next newest on a read error. |
| N7 | engine.rs:904 | `Step` on 0x60 adds a raw delta to the input code and switches with no confirmation (test: `Step +0x22 -> "Input → USB-C"`). It is gated only by the advertised value list. | low | Reject `Step` for Enum codes in core. |

## Reproduction test source (`crates/tvc-core/src/tests/verify_tests.rs`)

Also added to `crates/tvc-core/src/tests/mod.rs`: `mod verify_tests;` after `mod settings_tests;`.

```rust
//! Throwaway reproductions for the audit claims (2026-11-14-audit-core-ddc.md, sections A and C).

use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::*;
use crate::crash::Marker;
use crate::engine::{Event, WriteOutcome};
use crate::link::{LinkError, LinkFailure, MonitorLink};
use crate::mccs;
use crate::profile::{ControlDefinition, ControlKind, SafetyClass, Section};
use crate::safety::{self, Verdict};
use crate::store::{BackupStore, SettingsStore};

const P40W: &str = "(prot(monitor)type(LCD)model(P40w-20)cmds(01 02 03 07 0C E3 F3)vcp(02 04 05 08 10 12 14(01 05 06 08 0B) 16 18 1A 52 60(0F 11 12 1B 31 FF) 62 6C 6E 70 86(01 02 05 0B) 87 8D(01 02) AC AE B2 B6 C0 C6 C8 C9 CA(01 02) CC(02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E 12 14 16 17 1A 1E 1F 20 24) D6(01 04 05) DC(00 01 02 03 05 08 0B) DF E0(00 01 02) E9 EA EC F7(04 07 08 12 14(00 02 03) 15 17) 60(0F 11 12 1B 31))mswhql(1)asset_eep(40)mccs_ver(2.2))";

/// A FakeLink with a chosen capability string and, optionally, every read answered "unsupported".
#[derive(Clone)]
struct CapsLink {
    fake: FakeLink,
    caps: String,
    unsupported: bool,
}

impl MonitorLink for CapsLink {
    fn connect(&mut self) -> Result<(), LinkFailure> {
        self.fake.connect()
    }
    fn disconnect(&mut self) {
        self.fake.disconnect();
    }
    fn capabilities(&mut self) -> Result<String, LinkError> {
        self.fake.capabilities().map(|_| self.caps.clone())
    }
    fn read(&mut self, code: u8) -> Result<VcpReading, LinkError> {
        if self.unsupported {
            self.fake.state().reads.push(code);
            return Err(LinkError::Unsupported(format!("0x{code:02X}")));
        }
        self.fake.read(code)
    }
    fn write(&mut self, code: u8, value: u16) -> Result<(), LinkError> {
        self.fake.write(code, value)
    }
}

fn caps_harness(caps: &str, unsupported: bool, safe: bool) -> (TempDir, Harness) {
    let dir = TempDir::new();
    let fake = FakeLink::new();
    let link = CapsLink { fake: fake.clone(), caps: caps.into(), unsupported };
    let h = Harness::spawn(Box::new(link), &fake, dir.path(), Timing::zero(), safe);
    (dir, h)
}

fn connected() -> (TempDir, Harness) {
    let dir = TempDir::new();
    let h = Harness::new(&FakeLink::new(), dir.path());
    assert_eq!(statuses(&h.connect()).last(), Some(&MonitorStatus::Connected));
    (dir, h)
}

fn controls(events: &[Event]) -> Option<std::sync::Arc<crate::engine::ResolvedControls>> {
    events.iter().rev().find_map(|e| match e { Event::Controls(c) => Some(c.clone()), _ => None })
}

// ---------- C. parser ----------

#[test]
fn verify_c_p40w_string_is_rejected_whole() {
    let r = mccs::parse(P40W);
    println!("C: parse(P40w-20) = {r:?}");
    assert!(r.is_err());
    let msg = r.unwrap_err().0;
    // offset is into the vcp slice, not the full string
    let vcp_start = P40W.find("vcp(").unwrap() + 4;
    let nested = P40W.find("14(00").unwrap() + 2;
    println!("C: error text {msg:?}; '(' of the nested list is at {} in the vcp slice, {} in the whole string", nested - vcp_start, nested);
}

#[test]
fn verify_c_p40w_without_nested_list_last_60_wins() {
    let flat = P40W.replace("14(00 02 03)", "14");
    let caps = mccs::parse(&flat).expect("parses once the nested list is gone");
    println!("C: 0x60 = {:?}", caps.vcp[&0x60]);
    println!("C: 0xF7 = {:?}", caps.vcp[&0xF7]);
    assert_eq!(caps.vcp[&0x60], Some(vec![0x0F, 0x11, 0x12, 0x1B, 0x31])); // second list; first (with FF) lost
    assert!(!caps.allows_value(0x60, 0xFF));
    assert_eq!(caps.model, "P40w-20");
    assert_eq!(caps.firmware(), None);
    // The duplicate 60 alone (no nested list) is not rejected.
    let dup_only = "(vcp(10 60(0F 11 12 1B 31 FF) 12 60(0F 11 12 1B 31)))";
    assert!(mccs::parse(dup_only).is_ok());
    // Reversed order: the FF list wins.
    let rev = mccs::parse("(vcp(60(0F 11) 60(0F 11 FF)))").unwrap();
    println!("C: reversed duplicate 0x60 = {:?}", rev.vcp[&0x60]);
}

#[test]
fn verify_c_fragility() {
    let p = |s: &str| mccs::parse(s);
    println!("C1 single digit: {:?}", p("(vcp(60(1 3 F)))"));
    println!("C1 three digit:  {:?}", p("(vcp(60(011)))"));
    println!("C1 four digit:   {:?}", p("(vcp(DF(0201)))").map(|c| c.vcp));
    println!("C2 NUL:          {:?}", p("(vcp(10 12\0 14))"));
    println!("C2 tab/CRLF:     {:?}", p("(vcp(10\t12\r\n14))").map(|c| c.vcp.len()));
    println!("C3 bad model:    {:?}", p("(prot(monitor)vcp(10 12)model(P40w"));
    println!("C4 unbalanced:   {:?}", p("(prot(monitor)cmds(01 02 vcp(10 12))"));
    println!("C4 'vcp (':      {:?}", p("(prot(monitor)vcp (10 12))"));
    println!("C5 two vcp:      {:?}", p("(vcp(10 12)vcp(14(01 02)))").map(|c| c.vcp));
    println!("C8 position:     {:?}", p("(prot(monitor)type(LCD)vcp(10 1))"));
    println!("C9 firmware:     {:?}", p("(vcp(04(41 42) 10))").map(|c| c.firmware()));
    assert!(p("(vcp(60(1 3 F)))").is_err());
    assert_eq!(p("(vcp(DF(0201)))").unwrap().vcp[&0xDF], Some(vec![0x02, 0x01]));
    assert!(p("(vcp(10 12\0 14))").is_err());
    assert!(p("(prot(monitor)vcp(10 12)model(P40w").is_err());
    assert!(p("(prot(monitor)cmds(01 02 vcp(10 12))").is_err());
    assert!(p("(prot(monitor)vcp (10 12))").is_err());
    assert!(!p("(vcp(10 12)vcp(14(01 02)))").unwrap().supports(0x14));
    assert_eq!(p("(vcp(04(41 42) 10))").unwrap().firmware().as_deref(), Some("AB"));
}

#[test]
fn verify_c_nested_list_makes_engine_not_responding() {
    let (_dir, h) = caps_harness(P40W, false, false);
    let events = h.connect();
    println!("A1.5: statuses {:?}, capability reads {}", statuses(&events), h.link.state().capability_reads);
    println!("A1.5: value reads during connect: {:?}", h.link.state().reads);
    assert_eq!(statuses(&events).last(), Some(&MonitorStatus::NotResponding));
}

// ---------- A1 ----------

#[test]
fn verify_a1_1_restore_order_and_input() {
    let (dir, h) = connected();
    h.set(0x04, 1, true); // factory reset: backup taken
    let backup = BackupStore::new(dir.path()).load_latest().unwrap().unwrap();
    println!("A1.1: backup = {backup:?}");
    assert!(backup.contains_key(&0x60) && backup.contains_key(&0xCC));

    // Monitor now on another input, colour mode locks brightness.
    {
        let mut s = h.link.state();
        s.values.get_mut(&0x60).unwrap().current = 0x0F;
        s.values.get_mut(&0x10).unwrap().current = 50;
        s.ignored_writes.insert(0x10);
        s.writes.clear();
    }
    let r = h.result(Command::RestoreBackup { origin: Origin::Window });
    println!("A1.1 locked: result {:?} / {:?}; writes {:?}", r.outcome, r.message, h.link.state().writes);
    assert_eq!(r.kind, crate::result::ResultKind::Kept);
    assert_eq!(h.link.state().writes, [(0x10, 0)]); // 0x14 never written

    h.link.state().ignored_writes.clear();
    h.link.state().writes.clear();
    let r = h.result(Command::RestoreBackup { origin: Origin::Window });
    let writes = h.link.state().writes.clone();
    println!("A1.1 unlocked: result {:?}; writes {writes:?}", r.outcome);
    let pos = |c: u8| writes.iter().position(|w| w.0 == c).unwrap();
    assert!(pos(0x10) < pos(0x14));
    assert!(writes.contains(&(0x60, 0x11))); // input switched, no confirmation
    assert_eq!(r.outcome, WriteOutcome::Ok);
}

#[test]
fn verify_a1_2_corrupt_cache_skips_live_read() {
    for safe in [false, true] {
        let dir = TempDir::new();
        std::fs::write(dir.path().join("capabilities.txt"), "(prot(monitor)vcp(10 12 14(01 05").unwrap();
        let fake = FakeLink::new();
        let h = Harness::spawn(Box::new(fake.clone()), &fake, dir.path(), Timing::zero(), safe);
        let events = h.connect();
        let c = controls(&events).or_else(|| None);
        let reads_after_connect = h.link.state().capability_reads;
        h.run(Command::ReadCapabilities);
        println!("A1.2 safe={safe}: status {:?}, capability reads after Connect {reads_after_connect}, after ReadCapabilities {}; caps model {:?}",
            statuses(&events), h.link.state().capability_reads, c.map(|c| c.capabilities.model.clone()));
        assert_eq!(reads_after_connect, 0);
        assert_eq!(h.link.state().capability_reads, if safe { 0 } else { 1 });
    }
}

#[test]
fn verify_a1_3_synth_caps_authorize_writes_in_safe_mode() {
    // A monitor that only advertises 10 and 12, engine in safe mode, no cache.
    let (_dir, h) = caps_harness("(prot(monitor)vcp(10 12))", false, true);
    let events = h.connect();
    let c = controls(&events);
    let cap_reads = h.link.state().capability_reads;
    println!("A1.3: status {:?}; capability reads {cap_reads}; engine caps supports(0x60)={:?} supports(0x14)={:?}",
        statuses(&events), c.as_ref().map(|c| c.capabilities.supports(0x60)), c.as_ref().map(|c| c.capabilities.supports(0x14)));
    let r60 = h.set(0x60, 0x0F, false);
    let r14 = h.set(0x14, 0x0E, false);
    println!("A1.3: Set 60=0F -> {:?}; Set 14=0E -> {:?}; writes {:?}", r60.outcome, r14.outcome, h.link.state().writes);
    assert!(h.link.state().writes.contains(&(0x60, 0x0F)));
    assert!(h.link.state().writes.contains(&(0x14, 0x0E)));
}

#[test]
fn verify_a1_6_input_switch_without_confirmation() {
    let (_dir, h) = connected();
    let r = h.set(0x60, 0x0F, false);
    let step = h.result(Command::Step { code: 0x60, delta: 0x22, origin: Origin::Hotkey }); // 0x0F + 0x22 = 0x31
    println!("A1.6: Set 60=0F unconfirmed -> {:?} {:?}; Step +0x22 -> {:?} {:?}; writes {:?}",
        r.outcome, r.message, step.outcome, step.message, h.link.state().writes);
    assert_eq!(r.outcome, WriteOutcome::Ok);
    assert!(h.link.state().writes.contains(&(0x60, 0x0F)));
}

#[test]
fn verify_a1_7_no_hard_floor_in_safety() {
    let caps = mccs::parse(crate::fake::P32P20_CAPABILITIES).unwrap();
    let d6 = ControlDefinition::new(0xD6, "Power", ControlKind::Enum, Section::Advanced, SafetyClass::Normal,
        [(5u8, "Off".to_string())].into());
    let ca = ControlDefinition::new(0xCA, "OSD", ControlKind::Enum, Section::Advanced, SafetyClass::Normal, Default::default());
    let d = safety::evaluate(Some(&d6), &caps, 0xD6, 5, None, false);
    let c = safety::evaluate(Some(&ca), &caps, 0xCA, 1, None, false);
    println!("A1.7: evaluate(D6 Normal, 5) = {:?}; evaluate(CA Normal, 1) = {:?}", d.verdict, c.verdict);
    assert_eq!(d.verdict, Verdict::Allow);
    assert_eq!(c.verdict, Verdict::Allow);

    // Finder overlay names an E0-FF vendor code: writable with no confirmation.
    let (_dir, h) = connected();
    let f7 = ControlDefinition::new(0xF7, "Vendor F7", ControlKind::Enum, Section::Advanced, SafetyClass::Normal,
        [(4u8, "x".to_string())].into());
    h.run(Command::SaveOverlay(f7));
    let r = h.set(0xF7, 0x04, false);
    println!("A1.7: overlay-named F7 Set 04 -> {:?}; writes {:?}", r.outcome, h.link.state().writes);
    assert!(h.link.state().writes.contains(&(0xF7, 4)));
}

// ---------- A2 ----------

#[test]
fn verify_a2_1_drag_without_end_sticks() {
    let (_dir, h) = connected();
    h.run(Command::DragTo { code: 0x10, value: 40 });
    h.run(Command::RecheckAfterEvent); // reconnect does not clear drags
    h.link.state().reads.clear();
    h.link.state().values.get_mut(&0x12).unwrap().current = 33;
    h.run(Command::Poll(vec![0x10, 0x12]));
    let poll_reads = h.link.state().reads.clone();
    let events = h.run(Command::Set { code: 0x10, value: 70, confirmed: false, origin: Origin::Hotkey });
    println!("A2.1: poll reads after stuck drag {poll_reads:?}; Value events for hotkey Set 10=70: {:?}; result {:?}",
        values(&events), results(&events).first().map(|r| r.outcome));
    assert!(poll_reads.is_empty());
    assert!(values(&events).iter().all(|(c, _)| *c != 0x10));
}

#[test]
fn verify_a2_2_coalesce_reorders_set() {
    let (_dir, h) = connected();
    h.link.state().delay = Duration::from_millis(150);
    h.link.state().writes.clear();
    h.send(Command::Set { code: 0x12, value: 50, confirmed: false, origin: Origin::Window }); // keeps the worker busy
    std::thread::sleep(Duration::from_millis(30));
    h.send(Command::DragTo { code: 0x10, value: 30 });
    h.send(Command::Set { code: 0x10, value: 50, confirmed: false, origin: Origin::Hotkey });
    h.send(Command::DragTo { code: 0x10, value: 70 });
    h.flush();
    let writes: Vec<_> = h.link.state().writes.iter().copied().filter(|w| w.0 == 0x10).collect();
    println!("A2.2: sent DragTo30, Set50, DragTo70 -> 0x10 writes {writes:?}; final {}", h.link.state().values[&0x10].current);
    assert_eq!(writes, [(0x10, 70), (0x10, 50)]);
}

#[test]
fn verify_a2_5_shared_crash_marker() {
    let dir = TempDir::new();
    let a = Marker::new(dir.path());
    let b = Marker::new(dir.path());
    a.arm("capability read");
    b.arm("finder snapshot");
    b.disarm();
    println!("A2.5: after A armed, B armed+disarmed: left_over = {:?}", a.left_over());
    assert_eq!(a.left_over(), None);
}

// ---------- A3 ----------

#[test]
fn verify_a3_1_engine_death_is_silent() {
    let dir = TempDir::new();
    let fake = FakeLink::new();
    let handle = Engine::spawn(Box::new(fake), ProfileStore::new(dir.path()), dir.path().to_path_buf(), Timing::zero(),
        false, Box::new(|_e| panic!("sink panics (deliberate)")));
    std::thread::sleep(Duration::from_millis(100));
    handle.send(Command::Connect); // no error surfaced
    let (tx, rx) = mpsc::channel();
    handle.send(Command::Flush(tx));
    let flush = rx.recv_timeout(Duration::from_secs(1));
    let t = Instant::now();
    handle.shutdown();
    println!("A3.1: send() after death returned (); Flush reply = {flush:?}; shutdown took {:?}", t.elapsed());
    assert!(flush.is_err());
}

#[test]
fn verify_a3_3_all_unsupported_is_connected() {
    let (_dir, h) = caps_harness(crate::fake::P32P20_CAPABILITIES, true, false);
    let events = h.connect();
    println!("A3.3: every read Unsupported -> statuses {:?}", statuses(&events));
    assert_eq!(statuses(&events).last(), Some(&MonitorStatus::Connected));
}

#[test]
fn verify_a3_7_settings_missing_day_resets_and_overwrites_bak() {
    let dir = TempDir::new();
    std::fs::write(dir.path().join("settings.json.bak"), "OLDER BACKUP").unwrap();
    std::fs::write(dir.path().join("settings.json"),
        r#"{"Night":{"Name":"Night","Brightness":5,"Contrast":50},"ThisMachineInput":"0x0F","StartWithWindows":true}"#).unwrap();
    let s = SettingsStore::new(dir.path()).load();
    let bak = std::fs::read_to_string(dir.path().join("settings.json.bak")).unwrap();
    println!("A3.7: input {:#04X}, start_with_windows {}, .bak now = {bak:?}", s.this_machine_input, s.start_with_windows);
    assert_eq!(s.this_machine_input, 0x11);
    assert!(bak.contains("Night"));
}

#[test]
fn verify_a3_8_one_bad_overlay_entry_drops_all() {
    let dir = TempDir::new();
    std::fs::write(dir.path().join("profile.user.json"), r#"{"edidId":"LEN62A2","controls":[
        {"code":"0xF7","name":"Good","kind":"Enum","section":"Advanced","safety":"Normal","values":{"0x04":"x"}},
        {"code":"0xZZ","name":"Bad","kind":"Enum","section":"Advanced","safety":"Normal"}]}"#).unwrap();
    let caps = mccs::parse(crate::fake::P32P20_CAPABILITIES).unwrap();
    let defs = ProfileStore::new(dir.path()).resolve(&caps);
    println!("A3.8: 0xF7 resolves to {:?}", defs[&0xF7].name);
    assert_eq!(defs[&0xF7].name, "Unknown 0xF7");
}

// ---------- A4 ----------

#[test]
fn verify_a4_2_backups_never_pruned() {
    let (dir, h) = connected();
    for _ in 0..3 {
        h.set(0x05, 1, true);
    }
    let n = std::fs::read_dir(dir.path()).unwrap().flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("backup-")).count();
    println!("A4.2: backup files after 3 resets: {n}");
    assert_eq!(n, 3);
}
```
