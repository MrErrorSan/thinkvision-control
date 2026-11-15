//! MonitorHub: one engine per monitor, ids, migration (w3-hub).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::*;
use crate::engine::{PowerState, ResolvedControls, WriteOutcome};
use crate::fake::{FakeDiscovery, P32P20_CAPABILITIES};
use crate::hub::{HubCommand, HubConfig, HubEvent, HubHandle, Monitor, MonitorHub, OpId};
use crate::identity::{BackendKind, DisplayInfo};
use crate::link::{LinkError, LinkFailure};
use crate::result::ResultKind;

const WAIT: Duration = Duration::from_secs(10);

/// A hub on a `FakeDiscovery`, with its events collected.
struct Hub {
    discovery: FakeDiscovery,
    handle: Option<HubHandle>,
    rx: Receiver<HubEvent>,
    /// Every event received so far.
    seen: Vec<HubEvent>,
}

impl Hub {
    fn new(root: &Path, displays: Vec<(DisplayInfo, FakeLink)>) -> Self {
        let discovery = FakeDiscovery::new(displays);
        let (tx, rx) = mpsc::channel();
        let config = HubConfig { discovery: Box::new(discovery.clone()), root: root.to_path_buf(), safe_mode: false };
        let handle = MonitorHub::spawn(config, Box::new(move |e| {
            let _ = tx.send(e);
        }));
        Self { discovery, handle: Some(handle), rx, seen: Vec::new() }
    }

    fn handle(&self) -> &HubHandle {
        self.handle.as_ref().unwrap()
    }

    /// Sends `command` to monitor `id` with a fresh op id.
    fn to(&self, id: &str, command: Command) -> OpId {
        let op = self.handle().next_op();
        self.handle().send(HubCommand::Send { id: MonitorId(id.into()), op, command: Box::new(command) });
        op
    }

    fn rescan(&self) {
        self.handle().send(HubCommand::Rescan);
    }

    /// Waits up to `WAIT` for the first event since the last wait that `pick` accepts.
    fn wait<T>(&mut self, mut pick: impl FnMut(&HubEvent) -> Option<T>) -> T {
        let deadline = Instant::now() + WAIT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let event = self.rx.recv_timeout(left).expect("the hub event in time");
            let picked = pick(&event);
            self.seen.push(event);
            if let Some(t) = picked {
                return t;
            }
        }
    }

    /// Like `wait`, but an event already seen counts too.
    fn seen_or_wait<T>(&mut self, mut pick: impl FnMut(&HubEvent) -> Option<T>) -> T {
        if let Some(t) = self.seen.iter().find_map(&mut pick) {
            return t;
        }
        self.wait(pick)
    }

    fn connect(&mut self, id: &str) {
        self.to(id, Command::Connect);
        self.wait_status(id, MonitorStatus::Connected);
    }

    fn wait_status(&mut self, id: &str, want: MonitorStatus) {
        self.wait(|e| match e {
            HubEvent::Monitor { id: i, event: Event::Status { status, .. } } if i.0 == id && *status == want => Some(()),
            _ => None,
        });
    }

    fn wait_result(&mut self, op: OpId) -> (MonitorId, WriteResult) {
        self.wait(|e| match e {
            HubEvent::Monitor { id, event: Event::Result { op: Some(o), result, .. } } if *o == op => {
                Some((id.clone(), result.clone()))
            }
            _ => None,
        })
    }

    fn wait_monitors(&mut self) -> Vec<String> {
        self.wait(|e| match e {
            HubEvent::Monitors(Ok(list)) => Some(list.iter().map(|m| m.id.0.clone()).collect()),
            _ => None,
        })
    }

    /// The last `Controls` seen for `id`.
    fn controls(&self, id: &str) -> Arc<ResolvedControls> {
        self.seen.iter().rev().find_map(|e| match e {
            HubEvent::Monitor { id: i, event: Event::Controls(c) } if i.0 == id => Some(c.clone()),
            _ => None,
        }).expect("a Controls event")
    }

    fn set(&self, id: &str, value: u16) -> OpId {
        self.to(id, Command::Set { code: 0x10, value, confirmed: false, origin: Origin::Window })
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.shutdown();
        }
    }
}

fn display(edid: &str, serial: Option<&str>, connector: Option<&str>) -> DisplayInfo {
    let mut d = FakeDiscovery::display(edid, serial, connector);
    d.target = format!("fake:{edid}:{}:{}", serial.unwrap_or(""), connector.unwrap_or(""));
    d
}

fn id(s: &str) -> MonitorId {
    MonitorId(s.into())
}

fn two() -> Vec<(DisplayInfo, FakeLink)> {
    vec![(display("LEN62A2", Some("A"), None), FakeLink::new()), (display("LEN62A2", Some("B"), None), FakeLink::new())]
}

#[test]
fn slow_monitor_does_not_delay_another() {
    let root = TempDir::new();
    let (fast, slow) = (FakeLink::new(), FakeLink::new());
    slow.state().delay = Duration::from_secs(3);
    let mut hub = Hub::new(root.path(),
        vec![(display("LEN62A2", Some("F"), None), fast.clone()), (display("LEN62A2", Some("S"), None), slow)]);
    hub.to("LEN62A2-SS", Command::Connect); // busy for a while
    hub.connect("LEN62A2-SF");

    let started = Instant::now();
    let op = hub.set("LEN62A2-SF", 40);
    let (from, result) = hub.wait_result(op);

    assert!(started.elapsed() < Duration::from_millis(2000), "{:?}", started.elapsed());
    assert_eq!((from, result.outcome), (id("LEN62A2-SF"), WriteOutcome::Ok));
    assert_eq!(fast.state().writes, [(0x10, 40)]);
}

#[test]
fn detach_leaves_the_other_connected() {
    let root = TempDir::new();
    let displays = two();
    let a = displays[0].1.clone();
    let mut hub = Hub::new(root.path(), displays);
    hub.connect("LEN62A2-SA");
    hub.connect("LEN62A2-SB");
    a.state().delay = Duration::from_secs(2); // A is still busy when it leaves
    hub.set("LEN62A2-SA", 35);
    hub.discovery.state().displays.remove(0);
    hub.rescan();
    let gone = hub.seen_or_wait(|e| match e {
        HubEvent::Detached(id) => Some(id.clone()),
        _ => None,
    });

    assert_eq!(gone, id("LEN62A2-SA"));
    let op = hub.set("LEN62A2-SB", 45);
    assert_eq!(hub.wait_result(op).1.outcome, WriteOutcome::Ok);
    let late = hub.seen.iter().skip_while(|e| !matches!(e, HubEvent::Detached(_)))
        .any(|e| matches!(e, HubEvent::Monitor { id, .. } if id.0 == "LEN62A2-SA"));
    assert!(!late);
}

#[test]
fn ids_s_p_n() {
    let root = TempDir::new();
    let mut hub = Hub::new(root.path(), vec![
        (display("LEN62A2", Some("V1"), None), FakeLink::new()),
        (display("DEL4321", Some("D"), Some("DP-1")), FakeLink::new()),
        (display("DEL4321", Some("D"), Some("DP-2")), FakeLink::new()),
        (display("ACR0001", None, None), FakeLink::new()),
    ]);

    assert_eq!(hub.wait_monitors(), ["LEN62A2-SV1", "DEL4321-PDP-1", "DEL4321-PDP-2", "ACR0001-N1"]);
}

#[test]
fn s_id_stable_when_duplicate_attached() {
    let root = TempDir::new();
    let mut hub = Hub::new(root.path(), vec![(display("LEN62A2", Some("V1"), Some("DP-1")), FakeLink::new())]);
    assert_eq!(hub.wait_monitors(), ["LEN62A2-SV1"]);

    hub.discovery.state().displays.push((display("LEN62A2", Some("V1"), Some("DP-2")), FakeLink::new()));
    hub.rescan();

    assert_eq!(hub.wait_monitors(), ["LEN62A2-SV1", "LEN62A2-PDP-2"]);
    let attached = hub.wait(|e| match e {
        HubEvent::Attached(Monitor { id, .. }) if id.0 != "LEN62A2-SV1" => Some(id.clone()),
        _ => None,
    });
    assert_eq!(attached, id("LEN62A2-PDP-2"));
    assert!(!hub.seen.iter().any(|e| matches!(e, HubEvent::Detached(_))));
    hub.connect("LEN62A2-SV1");
}

/// The 0.0.8 files, written in `root`.
fn legacy_files(root: &Path) -> [&'static str; 4] {
    let names = ["capabilities.txt", "profile.user.json", "backup-20260101-100000.json", "risky-operation.txt"];
    std::fs::write(root.join(names[0]), P32P20_CAPABILITIES).unwrap();
    std::fs::write(root.join(names[1]), r#"{"edidId":"LEN62A2","controls":[]}"#).unwrap();
    std::fs::write(root.join(names[2]), r#"{ "0x10": 5 }"#).unwrap();
    std::fs::write(root.join(names[3]), "capability read").unwrap();
    names
}

#[test]
fn legacy_files_migrate_once() {
    let root = TempDir::new();
    let names = legacy_files(root.path());
    let hub = Hub::new(root.path(), vec![
        (display("DEL4321", Some("D"), None), FakeLink::new()),
        (display("LEN62A2", Some("V1"), None), FakeLink::new()),
        (display("LEN62A2", Some("V2"), None), FakeLink::new()),
    ]);

    let monitors = root.path().join("monitors");
    for name in names {
        assert!(!root.path().join(name).exists(), "{name}");
        assert!(monitors.join("LEN62A2-SV1").join(name).exists(), "{name}");
        assert!(!monitors.join("LEN62A2-SV2").join(name).exists(), "{name}");
        assert!(!monitors.join("DEL4321-SD").join(name).exists(), "{name}");
    }
    drop(hub);
    // A later start leaves them where they are.
    let _hub = Hub::new(root.path(), vec![(display("LEN62A2", Some("V2"), None), FakeLink::new())]);
    assert!(monitors.join("LEN62A2-SV1").join(names[0]).exists());
    assert!(!monitors.join("LEN62A2-SV2").join(names[0]).exists());
}

#[test]
fn n_id_never_adopts_legacy_files() {
    let root = TempDir::new();
    let names = legacy_files(root.path());
    let mut hub = Hub::new(root.path(), vec![(display("LEN62A2", None, None), FakeLink::new())]);
    assert_eq!(hub.wait_monitors(), ["LEN62A2-N1"]);

    for name in names {
        assert!(root.path().join(name).exists(), "{name}");
    }
    assert!(!root.path().join("monitors").join("LEN62A2-N1").exists());
}

#[test]
fn n_id_keeps_its_crash_marker_where_the_next_start_looks() {
    let root = TempDir::new();
    let link = FakeLink::new();
    link.state().delay = Duration::from_millis(300);
    let mut hub = Hub::new(root.path(), vec![(display("LEN62A2", None, None), link)]);
    assert_eq!(hub.wait_monitors(), ["LEN62A2-N1"]);
    hub.to("LEN62A2-N1", Command::Connect);

    // During the capability read: in `monitors/_session` (main.rs's `state_dirs`), named for the monitor.
    let marker = root.path().join("monitors").join(crate::crash::SESSION_MARKERS)
        .join(format!("risky-operation-{}-LEN62A2-N1.txt", crate::crash::process_key()));
    let deadline = Instant::now() + WAIT;
    while !marker.exists() {
        assert!(Instant::now() < deadline, "no marker at {}", marker.display());
        std::thread::sleep(Duration::from_millis(5));
    }
    hub.wait_status("LEN62A2-N1", MonitorStatus::Connected);
    assert!(!marker.exists());
}

#[test]
fn identity_json_written() {
    let root = TempDir::new();
    let _hub = Hub::new(root.path(), vec![(display("LEN62A2", Some("V1"), Some("DP-1")), FakeLink::new())]);

    let text = std::fs::read_to_string(root.path().join("monitors/LEN62A2-SV1/identity.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json, serde_json::json!({ "id": "LEN62A2-SV1", "edidId": "LEN62A2", "serial": "V1", "connector": "DP-1" }));
}

#[test]
fn op_id_echoed_for_one_target() {
    let root = TempDir::new();
    let mut hub = Hub::new(root.path(), two());
    hub.connect("LEN62A2-SA");

    let op = hub.set("LEN62A2-SA", 50);

    assert_eq!(hub.wait_result(op).0, id("LEN62A2-SA"));
}

#[test]
fn next_op_monotonic() {
    let root = TempDir::new();
    let hub = Hub::new(root.path(), Vec::new());
    let ops: Vec<OpId> = (0..5).map(|_| hub.handle().next_op()).collect();
    assert!(ops.windows(2).all(|w| w[0] < w[1]), "{ops:?}");
}

#[test]
fn send_to_unknown_monitor_answers_not_responding() {
    let root = TempDir::new();
    let mut hub = Hub::new(root.path(), two());

    hub.to("DEL4321-SNOPE", Command::Connect); // answers nothing
    let op = hub.to("DEL4321-SNOPE", Command::Set { code: 0x10, value: 1, confirmed: false, origin: Origin::Cli });
    let (from, result) = hub.wait_result(op);

    assert_eq!(from, id("DEL4321-SNOPE"));
    assert_eq!((result.outcome, result.kind), (WriteOutcome::NotResponding, ResultKind::NotResponding));
    let others = hub.seen.iter().filter(|e| matches!(e, HubEvent::Monitor { id, .. } if id.0 == "DEL4321-SNOPE")).count();
    assert_eq!(others, 1);
}

#[test]
fn power_broadcast_and_replayed() {
    let root = TempDir::new();
    let mut hub = Hub::new(root.path(), vec![(display("LEN62A2", Some("A"), None), FakeLink::new())]);

    hub.handle().send(HubCommand::Power(PowerState::DisplayOff));
    let asleep = |want: &'static str| move |e: &HubEvent| match e {
        HubEvent::Monitor { id, event: Event::Asleep(true) } if id.0 == want => Some(()),
        _ => None,
    };
    hub.wait(asleep("LEN62A2-SA"));
    hub.discovery.state().displays.push((display("LEN62A2", Some("B"), None), FakeLink::new()));
    hub.rescan();

    hub.wait(asleep("LEN62A2-SB"));
    let op = hub.set("LEN62A2-SB", 10);
    assert_eq!(hub.wait_result(op).1.kind, ResultKind::Asleep);
}

#[test]
fn none_backend_listed_without_engine() {
    let root = TempDir::new();
    let mut virtual_screen = display("MSH0001", Some("1"), None);
    virtual_screen.backend = BackendKind::None;
    let mut hub = Hub::new(root.path(), vec![(virtual_screen, FakeLink::new())]);
    assert_eq!(hub.wait_monitors(), ["MSH0001-S1"]);

    let op = hub.to("MSH0001-S1", Command::Set { code: 0x10, value: 1, confirmed: false, origin: Origin::Window });

    assert_eq!(hub.wait_result(op).1.outcome, WriteOutcome::NotResponding);
    let engine_events = hub.seen.iter()
        .filter(|e| matches!(e, HubEvent::Monitor { event, .. } if !matches!(event, Event::Result { .. })))
        .count();
    assert_eq!(engine_events, 0);
    assert!(hub.seen.iter().any(|e| matches!(e, HubEvent::Attached(m) if m.id.0 == "MSH0001-S1")));
}

#[test]
fn backlight_forced_verified_zero_pacing() {
    let root = TempDir::new();
    let mut panel = display("BOE0A1B", Some("P"), None);
    panel.backend = BackendKind::Backlight;
    let link = FakeLink::new();
    let mut hub = Hub::new(root.path(), vec![(panel, link.clone())]);
    hub.connect("BOE0A1B-SP");

    let controls = hub.controls("BOE0A1B-SP");
    assert_eq!(controls.profile.control_order, [0x10]);
    assert_eq!(controls.firmware.as_deref(), Some("LB1.2")); // only a Verified monitor reports it
    assert!(controls.controls.values().filter(|d| d.code != 0x10).all(|d| !d.is_visible()));

    let started = Instant::now();
    let op = hub.set("BOE0A1B-SP", 70);
    assert_eq!(hub.wait_result(op).1.outcome, WriteOutcome::Ok);
    // The P32p-20 pacing would wait 400 ms to verify.
    assert!(started.elapsed() < Duration::from_millis(300), "{:?}", started.elapsed());
    assert_eq!(link.state().writes, [(0x10, 70)]);
}

/// A `FakeLink` whose writes fail while `failing` is set.
struct FailingWrites {
    link: FakeLink,
    failing: Arc<AtomicBool>,
}

impl MonitorLink for FailingWrites {
    fn connect(&mut self) -> Result<(), LinkFailure> {
        self.link.connect()
    }

    fn disconnect(&mut self) {
        self.link.disconnect();
    }

    fn capabilities(&mut self) -> Result<String, LinkError> {
        self.link.capabilities()
    }

    fn read(&mut self, code: u8) -> Result<VcpReading, LinkError> {
        self.link.read(code)
    }

    fn write(&mut self, code: u8, value: u16) -> Result<(), LinkError> {
        if self.failing.load(Ordering::SeqCst) {
            return Err(LinkError::Transient(format!("write 0x{code:02X}")));
        }
        self.link.write(code, value)
    }
}

#[test]
fn read_only_after_three_failed_writes() {
    let dir = TempDir::new();
    let fake = FakeLink::new();
    let failing = Arc::new(AtomicBool::new(true));
    let link = FailingWrites { link: fake.clone(), failing: failing.clone() };
    let h = Harness::spawn(Box::new(link), &fake, dir.path(), Timing::zero(), false);
    h.connect();

    // Each failure loses the monitor; the zero-delay retry burst reconnects it before the next Set.
    for value in [10, 11, 12] {
        assert_eq!(h.set(0x10, value, false).outcome, WriteOutcome::NotResponding, "{value}");
    }
    failing.store(false, Ordering::SeqCst);
    let refused = h.set(0x10, 13, false);
    assert_eq!((refused.outcome, refused.kind), (WriteOutcome::Blocked, ResultKind::Refused));
    assert_eq!(refused.message,
        "Changes are off for this monitor after repeated failed writes; reconnect it to try again.");
    assert!(fake.state().writes.is_empty());

    h.connect();
    assert_eq!(h.set(0x10, 14, false).outcome, WriteOutcome::Ok);
}

#[test]
fn a_good_write_resets_the_failure_count() {
    let dir = TempDir::new();
    let fake = FakeLink::new();
    let failing = Arc::new(AtomicBool::new(true));
    let link = FailingWrites { link: fake.clone(), failing: failing.clone() };
    let h = Harness::spawn(Box::new(link), &fake, dir.path(), Timing::zero(), false);
    h.connect();

    for value in [10, 11] {
        h.set(0x10, value, false);
    }
    failing.store(false, Ordering::SeqCst);
    assert_eq!(h.set(0x10, 12, false).outcome, WriteOutcome::Ok);
    failing.store(true, Ordering::SeqCst);
    h.set(0x10, 13, false);
    failing.store(false, Ordering::SeqCst);

    assert_eq!(h.set(0x10, 14, false).outcome, WriteOutcome::Ok);
}

#[test]
fn support_levels_through_the_engine() {
    let root = TempDir::new();
    let lenovo = FakeLink::new();
    lenovo.state().capabilities =
        Some("(prot(monitor)model(ThinkVision T24)vcp(10 12 14(01 0B) 60(11 0F) E0(00 01) D6(01 05)))".into());
    let dell = FakeLink::new();
    dell.state().capabilities = Some("(prot(monitor)model(DELL U2720Q)vcp(10 12 14(01 05) 60(11 0F) E0(00 01)))".into());
    let mut hub = Hub::new(root.path(), vec![
        (display("LEN62A2", Some("V"), None), FakeLink::new()),
        (display("LEN66B1", Some("L"), None), lenovo),
        (display("DEL4321", Some("D"), None), dell),
    ]);
    for id in ["LEN62A2-SV", "LEN66B1-SL", "DEL4321-SD"] {
        hub.connect(id);
    }

    // Verified: the P32p-20 profile and its firmware.
    let p32 = hub.controls("LEN62A2-SV");
    assert_eq!((p32.profile.display_name.as_str(), p32.firmware.as_deref()), ("ThinkVision P32p-20", Some("LB1.2")));
    // LenovoFamily: the P32p-20's definitions, the monitor's model.
    let t24 = hub.controls("LEN66B1-SL");
    assert_eq!((t24.profile.display_name.as_str(), t24.firmware.as_deref()), ("ThinkVision T24", None));
    assert_eq!(t24.controls[&0xE0].name, "Over Drive");
    assert_eq!(t24.controls[&0x14].value_labels[&0x0B], "sRGB");
    // GenericMccs: the generic definitions; a vendor code stays unknown.
    let u27 = hub.controls("DEL4321-SD");
    assert_eq!(u27.profile.display_name, "DELL U2720Q");
    assert_eq!(u27.controls[&0xE0].name, "Unknown 0xE0");
    assert_eq!(u27.controls[&0x14].value_labels[&0x05], "6500 K");
    let op = hub.to("DEL4321-SD", Command::Set { code: 0xE0, value: 1, confirmed: true, origin: Origin::Window });
    assert_eq!(hub.wait_result(op).1.outcome, WriteOutcome::Blocked);
    let op = hub.to("LEN66B1-SL", Command::Set { code: 0xE0, value: 1, confirmed: false, origin: Origin::Window });
    assert_eq!(hub.wait_result(op).1.outcome, WriteOutcome::Ok);
}

#[test]
fn failed_capability_read_makes_the_monitor_read_only() {
    let root = TempDir::new();
    let link = FakeLink::new();
    link.state().capabilities = Some("(prot(monitor)type(LCD))".into()); // no vcp(...): doesn't parse
    let mut hub = Hub::new(root.path(), vec![(display("DEL4321", Some("D"), None), link)]);
    hub.connect("DEL4321-SD");

    // GenericMccs at start, ReadOnly after the failed read: Controls again, with nothing to show.
    let controls = hub.seen.iter()
        .filter(|e| matches!(e, HubEvent::Monitor { event: Event::Controls(_), .. }))
        .count();
    assert_eq!(controls, 2);
    assert!(hub.controls("DEL4321-SD").controls.is_empty());
    let op = hub.to("DEL4321-SD", Command::Set { code: 0x10, value: 5, confirmed: false, origin: Origin::Window });
    let result = hub.wait_result(op).1;
    assert_eq!((result.outcome, result.kind), (WriteOutcome::Blocked, ResultKind::Refused));
}
