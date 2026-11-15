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
