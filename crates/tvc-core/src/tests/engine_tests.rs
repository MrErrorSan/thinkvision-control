//! ControlServiceTests, OfflineAndReconnectTests, StatusReasonTests and DdcQueueTests, against the engine.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use super::*;
use crate::engine::{Event, FinderEvent, PowerState, WriteOutcome, SHUTDOWN_WAIT};
use crate::finder::CodeChange;
use crate::link::{LinkError, LinkFailure, MonitorLink};
use crate::result::ResultKind;
use crate::profile::{ControlDefinition, ControlKind, SafetyClass, Section};
use crate::store::{BackupStore, Preset};

fn connected() -> (TempDir, Harness) {
    let dir = TempDir::new();
    let h = Harness::new(&FakeLink::new(), dir.path());
    assert_eq!(statuses(&h.connect()), [MonitorStatus::Connecting, MonitorStatus::Connected]);
    (dir, h)
}

fn reading(current: u16, max: u16) -> VcpReading {
    VcpReading { current, max }
}

#[test]
fn start_reads_capabilities_and_values() {
    let dir = TempDir::new();
    let h = Harness::new(&FakeLink::new(), dir.path());
    let events = h.connect();
    let values = values(&events);

    assert_eq!(statuses(&events).last(), Some(&MonitorStatus::Connected));
    assert!(values.contains(&(0x12, reading(70, 100))));
    assert!(values.contains(&(0xE0, reading(1, 2))));
    assert!(!values.iter().any(|(c, _)| *c == 0xEB)); // hidden codes are not read at start
    assert!(!values.iter().any(|(c, _)| *c == 0xD6)); // power is never touched
    let controls = events.iter().filter_map(|e| match e { Event::Controls(c) => Some(c.clone()), _ => None }).next_back().unwrap();
    assert_eq!(controls.capabilities.model, "ThinkVision P32p-20");
    assert_eq!(controls.firmware.as_deref(), Some("LB1.2"));
}

#[test]
fn step_clamps_at_max() {
    let link = FakeLink::new();
    link.state().values.insert(0x10, reading(95, 100));
    let dir = TempDir::new();
    let h = Harness::new(&link, dir.path());
    h.connect();

    let result = h.result(Command::Step { code: 0x10, delta: 10, origin: Origin::Hotkey });

    assert_eq!(result.outcome, WriteOutcome::Ok);
    assert_eq!(link.state().writes, [(0x10, 100)]);
    assert_eq!(result.message, "Brightness 100");
    assert_eq!(result.value, Some(100));
}

#[test]
fn step_reports_when_monitor_does_not_apply_brightness() {
    let (_dir, h) = connected();
    h.link.state().ignored_writes.insert(0x10);

    let result = h.result(Command::Step { code: 0x10, delta: 5, origin: Origin::Hotkey });

    assert_eq!(result.outcome, WriteOutcome::Blocked);
    assert!(result.message.contains("Brightness remains 0"), "{}", result.message);
    assert!(result.message.contains("color mode may lock brightness"));
    assert_eq!(result.kept, Some(0));
    assert_eq!(h.link.state().writes, [(0x10, 5)]);
}

#[test]
fn step_clamps_at_zero_and_refreshes_an_uncached_code() {
    let link = FakeLink::new();
    link.state().values.insert(0x16, reading(3, 100));
    let dir = TempDir::new();
    let h = Harness::new(&link, dir.path());
    h.connect();

    assert_eq!(h.result(Command::Step { code: 0x16, delta: -10, origin: Origin::Cli }).message, "Red gain 0");
    // 0xEB is never read at start; the step reads it first, then safety blocks the unnamed code.
    let unnamed = h.result(Command::Step { code: 0xEB, delta: 1, origin: Origin::Cli });
    assert_eq!(unnamed.outcome, WriteOutcome::Blocked);
    // A code that can't be read at all marks the monitor lost, as in 0.0.4.
    let missing = h.result(Command::Step { code: 0x02, delta: 1, origin: Origin::Cli });
    assert_eq!(missing.outcome, WriteOutcome::NotResponding);
}

#[test]
fn direct_range_write_verifies_the_monitor_applied_the_requested_value() {
    let (_dir, h) = connected();
    h.link.state().ignored_writes.insert(0x10);

    let result = h.set(0x10, 25, false);

    assert_eq!((result.outcome, result.kind), (WriteOutcome::Blocked, ResultKind::Kept));
    assert_eq!(result.message, "Brightness remains 0; the monitor did not apply 25. The current color mode may lock brightness.");
    assert_eq!(result.kept, Some(0));
}

#[test]
fn range_write_is_clamped_to_the_reported_max() {
    let (_dir, h) = connected();
    assert_eq!(h.set(0x12, 150, false).message, "Contrast 100");
    assert_eq!(h.link.state().writes, [(0x12, 100)]);
}

#[test]
fn enum_write_reports_label_and_is_cached_without_read_back() {
    let (_dir, h) = connected();
    let reads = h.link.state().reads.len();

    let events = h.run(Command::Set { code: 0xE0, value: 2, confirmed: false, origin: Origin::Window });

    assert_eq!(results(&events)[0].message, "Over Drive → Extreme");
    assert_eq!(results(&events)[0].kind, ResultKind::Ok);
    assert_eq!(values(&events), [(0xE0, reading(2, 2))]);
    assert_eq!(h.link.state().reads.len(), reads);
    let invalid = h.set(0x60, 0x13, false);
    assert_eq!((invalid.kind, invalid.message.as_str()), (ResultKind::OutOfRange, "0x13 is not a valid value for Input."));
}

#[test]
fn dead_monitor_reports_not_responding_and_raises_event() {
    let (_dir, h) = connected();
    h.link.state().dead = true;

    let events = h.run(Command::Set { code: 0x10, value: 30, confirmed: false, origin: Origin::Window });

    assert_eq!(results(&events)[0].outcome, WriteOutcome::NotResponding);
    assert_eq!(statuses(&events), [MonitorStatus::NotResponding]);
}

#[test]
fn osd_enable_power_and_unnamed_codes_never_reach_the_monitor() {
    let (_dir, h) = connected();

    let osd = h.set(0xCA, 2, true);
    assert_eq!((osd.outcome, osd.kind), (WriteOutcome::Blocked, ResultKind::Refused));
    assert_eq!(h.set(0xEB, 1, true).outcome, WriteOutcome::Blocked);
    let power = h.set(0xD6, 5, true);
    assert_eq!((power.outcome, power.message.as_str()), (WriteOutcome::Blocked, "Power is never written."));
    assert!(h.link.state().writes.is_empty());
}

#[test]
fn factory_reset_needs_confirmation_and_backs_up_first() {
    let (dir, h) = connected();

    assert_eq!(h.set(0x04, 1, false).outcome, WriteOutcome::NeedsConfirmation);
    assert!(h.link.state().writes.is_empty());

    let reset = h.set(0x04, 1, true);
    assert_eq!((reset.outcome, reset.message.as_str()), (WriteOutcome::Ok, "Factory reset"));
    assert_eq!(h.link.state().writes, [(0x04, 1)]);
    let backup = BackupStore::new(dir.path()).load_latest().unwrap().unwrap();
    assert_eq!(backup[&0x12], 70);
    assert_eq!(backup[&0x60], 0x11);
    assert!(!backup.contains_key(&0xD6));
    assert!(!backup.contains_key(&0xCA));
}

#[test]
fn restore_reapplies_latest_backup() {
    let (_dir, h) = connected();
    assert_eq!(h.result(Command::RestoreBackup { origin: Origin::Window }).message, "No backup found.");
    h.set(0x05, 1, true); // backup taken with contrast 70
    h.set(0x12, 40, false);
    h.link.state().writes.clear();

    let result = h.result(Command::RestoreBackup { origin: Origin::Window });

    assert_eq!((result.outcome, result.message.as_str()), (WriteOutcome::Ok, "Backup restored"));
    assert!(h.link.state().writes.contains(&(0x12, 70)));
}

#[test]
fn preset_sets_colour_mode_then_brightness_then_contrast() {
    let (_dir, h) = connected();
    let preset = Preset { name: "Night".into(), brightness: 0, contrast: 60, colour_mode: Some(0x12) };

    let events = h.run(Command::ApplyPreset { preset, origin: Origin::Tray });

    let Some(Event::Result { origin, code, preset, result }) = events.iter().find(|e| matches!(e, Event::Result { .. })) else {
        panic!("no result")
    };
    assert_eq!((*origin, *code, preset.as_deref()), (Origin::Tray, None, Some("Night")));
    assert_eq!(result.message, "Night preset");
    assert_eq!(h.link.state().writes, [(0x14, 0x12), (0x10, 0), (0x12, 60)]);
}

#[test]
fn preset_stops_at_the_first_failure() {
    let (_dir, h) = connected();
    h.link.state().ignored_writes.insert(0x10);
    let preset = Preset { name: "Day".into(), brightness: 60, contrast: 70, colour_mode: None };

    let result = h.result(Command::ApplyPreset { preset, origin: Origin::Hotkey });

    assert!(result.message.starts_with("Brightness remains 0"));
    assert_eq!(h.link.state().writes, [(0x10, 60)]);
}

#[test]
fn finder_snapshot_reads_hidden_codes_and_diff_shows_change() {
    let (_dir, h) = connected();
    assert!(matches!(h.run(Command::FinderCompare)[..], [Event::Finder(FinderEvent::NoSnapshot)]));
    let reads = h.link.state().reads.len();

    let snapshot = h.run(Command::FinderSnapshot);
    assert!(snapshot.iter().any(|e| matches!(e, Event::Finder(FinderEvent::Snapshot { count: 1 }))), "{snapshot:?}");
    // The 15 hidden vendor codes; never 0xCA or 0xD6.
    let read: std::collections::BTreeSet<u8> = h.link.state().reads[reads..].iter().copied().collect();
    let hidden = [0x02, 0x52, 0xAC, 0xAE, 0xB2, 0xB6, 0xC6, 0xC8, 0xC9, 0xDF, 0xEB, 0xF6, 0xF7, 0xF8, 0xFA];
    assert_eq!(read, hidden.into());
    h.link.state().values.insert(0xEB, reading(1, 1)); // changed with the monitor's buttons
    let compare = h.run(Command::FinderCompare);

    let changes = compare.iter().find_map(|e| match e { Event::Finder(FinderEvent::Changes(c)) => Some(c.clone()), _ => None });
    assert_eq!(changes.unwrap(), [CodeChange { code: 0xEB, before: 0, after: 1 }]);
    assert!(h.link.state().writes.is_empty());
}

#[test]
fn finder_naming_makes_a_code_writable_but_never_power() {
    let (_dir, h) = connected();
    let def = ControlDefinition::new(0xEB, "Eco mode", ControlKind::Enum, Section::Advanced, SafetyClass::Normal,
        [(0, "Off".to_string()), (1, "On".to_string())].into());

    let events = h.run(Command::SaveOverlay(def));
    assert!(events.iter().any(|e| matches!(e, Event::Finder(FinderEvent::Saved { code: 0xEB, result: Ok(()) }))));
    let Some(Event::Controls(controls)) = events.iter().find(|e| matches!(e, Event::Controls(_))) else { panic!() };
    assert_eq!(controls.controls[&0xEB].name, "Eco mode");
    assert_eq!(h.set(0xEB, 1, false).message, "Eco mode → On");

    let power = ControlDefinition::new(0xD6, "Power", ControlKind::Enum, Section::Advanced, SafetyClass::Normal,
        [(5, "Off".to_string())].into());
    let events = h.run(Command::SaveOverlay(power));
    assert!(events.iter().any(|e| matches!(e, Event::Finder(FinderEvent::Saved { code: 0xD6, result: Err(_) }))));
    assert_eq!(h.set(0xD6, 5, true).outcome, WriteOutcome::Blocked);
    assert!(!h.link.state().writes.contains(&(0xD6, 5)));
}

#[test]
fn status_is_connecting_before_start() {
    let dir = TempDir::new();
    let h = Harness::new(&FakeLink::new(), dir.path());
    assert_eq!(statuses(&h.flush()), [MonitorStatus::Connecting]);
}

#[test]
fn unplugged_monitor_is_reported_as_not_found() {
    let link = FakeLink::new();
    link.state().absent = true;
    let dir = TempDir::new();
    let h = Harness::new(&link, dir.path());

    assert_eq!(statuses(&h.connect()), [MonitorStatus::Connecting, MonitorStatus::NotFound]);
}

#[test]
fn found_monitor_without_ddc_is_reported_as_not_responding() {
    let link = FakeLink::new();
    link.state().dead = true;
    let dir = TempDir::new();
    let h = Harness::new(&link, dir.path());

    assert_eq!(statuses(&h.connect()).last(), Some(&MonitorStatus::NotResponding));
}

#[test]
fn monitor_plugged_in_later_connects_on_reconnect() {
    let link = FakeLink::new();
    link.state().absent = true;
    let dir = TempDir::new();
    let h = Harness::new(&link, dir.path());
    h.connect();
    link.state().absent = false;

    assert_eq!(statuses(&h.connect()), [MonitorStatus::Connected]);
}

#[test]
fn unplugging_after_connect_is_reported_as_not_found() {
    let (_dir, h) = connected();
    {
        let mut s = h.link.state();
        s.absent = true;
        s.dead = true;
    }
    assert_eq!(statuses(&h.connect()), [MonitorStatus::NotFound]);
}

#[test]
fn recheck_after_event_reconnects_even_while_connected() {
    let (_dir, h) = connected();
    let connects = h.link.state().connects;
    h.run(Command::RecheckAfterEvent);
    assert_eq!(h.link.state().connects, connects + 1);
}

#[test]
fn link_failure_reason_is_reported_with_the_status() {
    let link = FakeLink::new();
    {
        let mut s = link.state();
        s.absent = true;
        s.failure_when_absent = LinkFailure::PermissionDenied;
    }
    let dir = TempDir::new();
    let h = Harness::new(&link, dir.path());

    let events = h.connect();
    assert!(matches!(events.last(),
        Some(Event::Status { status: MonitorStatus::NotFound, failure: Some(LinkFailure::PermissionDenied) })));

    link.state().absent = false;
    let events = h.connect();
    assert!(matches!(events.last(), Some(Event::Status { status: MonitorStatus::Connected, failure: None })));
}

#[test]
fn poll_reports_a_change_made_elsewhere() {
    let (_dir, h) = connected();
    h.link.state().values.insert(0x60, reading(0x0F, 49)); // the other PC switched to DisplayPort

    let events = h.run(Command::Poll(vec![0x10, 0x60]));

    assert_eq!(values(&events), [(0x60, reading(0x0F, 49))]);
}

#[test]
fn poll_reads_nothing_during_a_drag() {
    let dir = TempDir::new();
    let h = Harness::with_timing(&FakeLink::new(), dir.path(), Timing { poll_quiet: Duration::ZERO, ..Timing::zero() });
    h.connect();
    h.run(Command::DragTo { code: 0x10, value: 5 });
    let reads = h.link.state().reads.len();

    h.run(Command::Poll(vec![0x12, 0x60]));
    assert_eq!(h.link.state().reads.len(), reads);

    h.run(Command::EndDrag { code: 0x10, value: 5, origin: Origin::Window });
    let reads = h.link.state().reads.len();
    h.run(Command::Poll(vec![0x12, 0x60]));
    assert_eq!(h.link.state().reads.len(), reads + 2);
}

#[test]
fn poll_reads_nothing_right_after_a_write() {
    let (_dir, h) = connected();
    h.set(0x12, 50, false);
    let reads = h.link.state().reads.len();

    h.run(Command::Poll(vec![0x10, 0x60]));

    assert_eq!(h.link.state().reads.len(), reads);
}

#[test]
fn failed_poll_keeps_the_monitor_connected() {
    let (_dir, h) = connected();
    let connects = h.link.state().connects;
    h.link.state().dead = true; // standby, or its own menu is open

    let events = h.run(Command::Poll(vec![0x10, 0x12]));
    std::thread::sleep(Duration::from_millis(100));

    assert!(statuses(&events).is_empty());
    assert!(statuses(&h.flush()).is_empty());
    assert_eq!(h.link.state().connects, connects);
}

#[test]
fn without_cache_controls_come_from_the_profile_and_status_is_connecting() {
    let dir = TempDir::new();
    let h = Harness::new(&FakeLink::new(), dir.path());
    let events = h.flush();

    let Some(Event::Controls(c)) = events.first() else { panic!("Controls first") };
    assert_eq!(c.controls[&0x60].value_labels[&0x31], "USB-C");
    assert!(c.capabilities.supports(0x10));
    assert!(c.capabilities.allows_value(0x60, 0x0F));
    assert_eq!(c.profile.short_name, "P32p-20");
    assert_eq!(statuses(&events), [MonitorStatus::Connecting]);
}

#[test]
fn connecting_caches_capabilities_for_the_next_launch() {
    let dir = TempDir::new();
    Harness::new(&FakeLink::new(), dir.path()).connect();
    assert!(dir.path().join("capabilities.txt").exists());

    let absent = FakeLink::new();
    absent.state().absent = true;
    let offline = Harness::new(&absent, dir.path());
    let Some(Event::Controls(c)) = offline.flush().into_iter().next() else { panic!("Controls first") };

    assert_eq!(c.capabilities.model, "ThinkVision P32p-20");
    assert!(c.controls.contains_key(&0xEB)); // vendor code known only from the real string
}

#[test]
fn capabilities_are_read_once_per_process() {
    let (_dir, h) = connected();
    h.connect();
    h.run(Command::RecheckAfterEvent);
    assert_eq!(h.link.state().capability_reads, 1);
}

#[test]
fn writes_are_refused_until_connected() {
    let link = FakeLink::new();
    link.state().absent = true;
    let dir = TempDir::new();
    let h = Harness::new(&link, dir.path());
    h.connect();

    let result = h.set(0x10, 30, false);
    assert_eq!((result.outcome, result.kind), (WriteOutcome::NotResponding, ResultKind::NotFound));
    assert!(link.state().writes.is_empty());
}

#[test]
fn refresh_does_no_ddc_traffic_while_not_connected() {
    let link = FakeLink::new();
    link.state().absent = true;
    let dir = TempDir::new();
    let h = Harness::new(&link, dir.path());
    h.connect();

    h.run(Command::Refresh(vec![0x10, 0x12]));

    assert!(link.state().reads.is_empty());
}

#[test]
fn refresh_that_fails_completely_marks_the_monitor_lost_and_retries() {
    let (_dir, h) = connected();
    h.link.state().dead = true;
    let connects = h.link.state().connects;

    assert_eq!(statuses(&h.run(Command::Refresh(vec![0x10]))), [MonitorStatus::NotResponding]);
    wait_until(|| h.link.state().connects == connects + Timing::zero().burst.len());
}

#[test]
fn retry_burst_runs_each_step_then_stops() {
    assert_eq!(Timing::default().burst, [500, 1000, 2000, 4000, 8000, 15_000, 30_000].map(Duration::from_millis));
    let link = FakeLink::new();
    link.state().absent = true;
    let dir = TempDir::new();
    let timing = Timing { burst: [1, 2, 3, 4, 5].map(Duration::from_millis).to_vec(), ..Timing::zero() };
    let h = Harness::with_timing(&link, dir.path(), timing);
    h.connect();
    let after_start = link.state().connects;

    h.send(Command::RequestReconnect);
    wait_until(|| link.state().connects == after_start + 5);
    std::thread::sleep(Duration::from_millis(100));

    assert_eq!(link.state().connects, after_start + 5);
    assert!(statuses(&h.flush()).is_empty()); // still NotFound
}

#[test]
fn a_new_request_restarts_the_burst() {
    let link = FakeLink::new();
    link.state().absent = true;
    let dir = TempDir::new();
    let timing = Timing { burst: vec![Duration::from_millis(400); 5], ..Timing::zero() };
    let h = Harness::with_timing(&link, dir.path(), timing);
    h.connect();
    let after_start = link.state().connects;

    h.send(Command::RequestReconnect);
    std::thread::sleep(Duration::from_millis(200));
    h.send(Command::RequestReconnect); // restarts the 400 ms wait
    std::thread::sleep(Duration::from_millis(300));

    assert_eq!(link.state().connects, after_start);
}

#[test]
fn monitor_plugged_in_during_a_burst_connects_and_ends_it() {
    let link = FakeLink::new();
    {
        let mut s = link.state();
        s.absent = true;
        s.plug_in_at_connect = Some(3); // start, then the second burst attempt
    }
    let dir = TempDir::new();
    let h = Harness::new(&link, dir.path());
    h.connect();

    h.send(Command::RequestReconnect);
    wait_until(|| statuses(&h.flush()).contains(&MonitorStatus::Connected));
    std::thread::sleep(Duration::from_millis(100));

    assert_eq!(link.state().connects, 3);
}

#[test]
fn burst_is_a_no_op_while_connected() {
    let (_dir, h) = connected();
    let connects = h.link.state().connects;
    h.run(Command::RequestReconnect);
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(h.link.state().connects, connects);
}

#[test]
fn drag_coalesces_writes_reads_nothing_and_verifies_once_on_release() {
    let dir = TempDir::new();
    // Writes are 50 ms apart, so 20 positions 10 ms apart coalesce to well under 20 writes.
    let timing = Timing { gap: Duration::from_millis(50), ..Timing::zero() };
    let h = Harness::with_timing(&FakeLink::new(), dir.path(), timing);
    h.connect();
    let reads = h.link.state().reads.len();

    for v in 1..=20 {
        h.send(Command::DragTo { code: 0x10, value: v });
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(h.link.state().reads.len(), reads);
    let events = h.run(Command::EndDrag { code: 0x10, value: 20, origin: Origin::Window });

    assert_eq!(results(&events)[0].outcome, WriteOutcome::Ok);
    let writes = h.link.state().writes.clone();
    assert!(writes.len() <= 12, "{} writes", writes.len());
    assert_eq!(writes.last(), Some(&(0x10, 20)));
    assert_eq!(h.link.state().reads.len(), reads + 1);
    assert_eq!(values(&events), [(0x10, reading(20, 100))]);
}

#[test]
fn drag_release_reports_a_value_the_monitor_kept() {
    let (_dir, h) = connected();
    h.link.state().ignored_writes.insert(0x10);

    h.send(Command::DragTo { code: 0x10, value: 60 });
    let events = h.run(Command::EndDrag { code: 0x10, value: 60, origin: Origin::Window });

    let result = &results(&events)[0];
    assert_eq!(result.outcome, WriteOutcome::Blocked);
    assert!(result.message.contains("remains"));
    assert_eq!(result.kept, Some(0));
    assert_eq!(values(&events), [(0x10, reading(0, 100))]);
}

#[test]
fn drag_is_ignored_for_non_range_codes() {
    let (_dir, h) = connected();
    h.run(Command::DragTo { code: 0x60, value: 0x0F });
    h.run(Command::DragTo { code: 0xD6, value: 5 });
    assert!(h.link.state().writes.is_empty());
}

#[test]
fn shutdown_does_not_wait_for_a_stuck_monitor_command() {
    let (_dir, h) = connected();
    h.link.state().delay = Duration::from_secs(4);
    h.send(Command::Refresh(vec![0x10]));
    std::thread::sleep(Duration::from_millis(200));

    let clock = Instant::now();
    h.shutdown();
    let elapsed = clock.elapsed();

    assert!(elapsed >= SHUTDOWN_WAIT - Duration::from_millis(100), "{elapsed:?}");
    assert!(elapsed < SHUTDOWN_WAIT + Duration::from_millis(500), "{elapsed:?}");
}

#[test]
fn idle_shutdown_is_immediate() {
    let (_dir, h) = connected();
    let clock = Instant::now();
    h.shutdown();
    assert!(clock.elapsed() < Duration::from_millis(500));
}

#[test]
fn reads_wait_for_writes_to_settle_but_writes_do_not() {
    let dir = TempDir::new();
    let timing = Timing { settle: Duration::from_millis(300), ..Timing::zero() };
    let h = Harness::with_timing(&FakeLink::new(), dir.path(), timing);
    h.connect();

    let clock = Instant::now();
    h.run(Command::DragTo { code: 0x12, value: 10 });
    h.run(Command::DragTo { code: 0x12, value: 11 });
    assert!(clock.elapsed() < Duration::from_millis(200));
    h.run(Command::EndDrag { code: 0x12, value: 12, origin: Origin::Window }); // write + verify read
    assert!(clock.elapsed() >= Duration::from_millis(300));
}

#[test]
fn failed_calls_are_retried_five_times() {
    let (_dir, h) = connected();
    h.link.state().dead = true;
    let reads = h.link.state().reads.len();
    h.run(Command::Poll(vec![0x10]));
    assert_eq!(h.link.state().reads.len(), reads + 5);
}

// ---------------------------------------------------------------------------------------------------------------
// Asleep, error classes, input switch, stale steps, safe mode.

/// Errors a test scripts per code, in front of a `FakeLink`.
#[derive(Default)]
struct Script {
    read: BTreeMap<u8, LinkError>,
    write: BTreeMap<u8, LinkError>,
    write_calls: usize,
}

#[derive(Clone)]
struct ScriptedLink {
    fake: FakeLink,
    script: Arc<Mutex<Script>>,
}

impl ScriptedLink {
    fn script(&self) -> MutexGuard<'_, Script> {
        self.script.lock().unwrap()
    }
}

impl MonitorLink for ScriptedLink {
    fn connect(&mut self) -> Result<(), LinkFailure> {
        self.fake.connect()
    }

    fn disconnect(&mut self) {
        self.fake.disconnect();
    }

    fn capabilities(&mut self) -> Result<String, LinkError> {
        self.fake.capabilities()
    }

    fn read(&mut self, code: u8) -> Result<VcpReading, LinkError> {
        let error = self.script().read.get(&code).cloned();
        match error {
            Some(e) => Err(e),
            None => self.fake.read(code),
        }
    }

    fn write(&mut self, code: u8, value: u16) -> Result<(), LinkError> {
        let error = {
            let mut s = self.script();
            s.write_calls += 1;
            s.write.get(&code).cloned()
        };
        match error {
            Some(e) => Err(e),
            None => self.fake.write(code, value),
        }
    }
}

fn scripted() -> (TempDir, Harness, ScriptedLink) {
    let dir = TempDir::new();
    let fake = FakeLink::new();
    let link = ScriptedLink { fake: fake.clone(), script: Arc::default() };
    let h = Harness::spawn(Box::new(link.clone()), &fake, dir.path(), Timing::zero(), false);
    assert_eq!(statuses(&h.connect()).last(), Some(&MonitorStatus::Connected));
    (dir, h, link)
}

fn asleep_events(events: &[Event]) -> Vec<bool> {
    events.iter().filter_map(|e| match e { Event::Asleep(a) => Some(*a), _ => None }).collect()
}

#[test]
fn nothing_reaches_the_monitor_while_asleep_and_waking_rechecks() {
    let (_dir, h) = connected();
    let (reads, connects) = { let s = h.link.state(); (s.reads.len(), s.connects) };

    assert_eq!(asleep_events(&h.run(Command::Power(PowerState::Locked))), [true]);
    let result = h.set(0x10, 30, false);
    assert_eq!((result.outcome, result.kind), (WriteOutcome::Blocked, ResultKind::Asleep));
    assert_eq!(result.message, "The monitor is asleep or this PC is locked");
    let step = h.result(Command::Step { code: 0x10, delta: 5, origin: Origin::Hotkey });
    assert_eq!(step.kind, ResultKind::Asleep);
    h.run(Command::Refresh(vec![0x10]));
    h.run(Command::Poll(vec![0x10]));
    h.run(Command::RecheckAfterEvent);
    h.run(Command::DragTo { code: 0x10, value: 40 });
    assert!(matches!(h.run(Command::FinderSnapshot)[..], [Event::Finder(FinderEvent::Refused(ResultKind::Asleep))]));
    assert!(h.link.state().writes.is_empty());
    assert_eq!({ let s = h.link.state(); (s.reads.len(), s.connects) }, (reads, connects));

    // Display off while locked, then on: still locked, so still asleep.
    h.run(Command::Power(PowerState::DisplayOff));
    assert!(asleep_events(&h.run(Command::Power(PowerState::DisplayOn))).is_empty());
    assert_eq!(h.link.state().connects, connects);

    // The last wake event: after `wake_delay` (zero here), reconnect and re-read once.
    assert_eq!(asleep_events(&h.run(Command::Power(PowerState::Unlocked))), [false]);
    assert_eq!(h.link.state().connects, connects + 1);
    assert!(h.link.state().reads.len() > reads);
    assert_eq!(h.set(0x10, 30, false).kind, ResultKind::Ok);
}

#[test]
fn the_wake_recheck_waits_for_the_wake_delay() {
    let dir = TempDir::new();
    let timing = Timing { wake_delay: Duration::from_millis(300), ..Timing::zero() };
    let h = Harness::with_timing(&FakeLink::new(), dir.path(), timing);
    h.connect();
    let connects = h.link.state().connects;

    h.run(Command::Power(PowerState::Suspending));
    h.run(Command::Power(PowerState::Resumed));
    assert_eq!(h.set(0x12, 50, false).kind, ResultKind::Asleep); // still waking
    assert_eq!(h.link.state().connects, connects);

    wait_until(|| h.link.state().connects == connects + 1);
    assert_eq!(h.set(0x12, 50, false).kind, ResultKind::Ok);
}

#[test]
fn the_retry_burst_is_paused_while_asleep() {
    let link = FakeLink::new();
    link.state().absent = true;
    let dir = TempDir::new();
    let h = Harness::new(&link, dir.path());
    h.connect();
    let connects = link.state().connects;

    h.run(Command::Power(PowerState::DisplayOff));
    h.run(Command::RequestReconnect);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(link.state().connects, connects);

    link.state().absent = false;
    let mut seen = statuses(&h.run(Command::Power(PowerState::DisplayOn)));
    wait_until(|| {
        seen.extend(statuses(&h.flush()));
        seen.contains(&MonitorStatus::Connected)
    });
}

#[test]
fn permission_denied_right_after_resume_keeps_retrying_instead_of_reporting() {
    let link = FakeLink::new();
    {
        let mut s = link.state();
        s.absent = true;
        s.failure_when_absent = LinkFailure::PermissionDenied;
    }
    let dir = TempDir::new();
    let h = Harness::new(&link, dir.path());
    let mut events = h.run(Command::Power(PowerState::Resumed));
    wait_until(|| link.state().connects >= 3);
    events.extend(h.flush());

    assert!(!events.iter().any(|e| matches!(e, Event::Status { failure: Some(LinkFailure::PermissionDenied), .. })));
}

#[test]
fn unsupported_is_final_and_keeps_the_monitor_connected() {
    let (_dir, h, link) = scripted();
    link.script().write.insert(0x12, LinkError::Unsupported("0x12".into()));

    let events = h.run(Command::Set { code: 0x12, value: 50, confirmed: false, origin: Origin::Window });

    let result = &results(&events)[0];
    assert_eq!((result.outcome, result.kind), (WriteOutcome::Blocked, ResultKind::Unsupported));
    assert_eq!(result.message, "Contrast isn't supported by this monitor.");
    assert_eq!(link.script().write_calls, 1);
    assert!(statuses(&events).is_empty());
}

#[test]
fn a_gone_connection_is_not_retried_and_reconnects_at_once() {
    let (_dir, h, link) = scripted();
    link.script().write.insert(0x12, LinkError::Gone("handle invalid".into()));

    let events = h.run(Command::Set { code: 0x12, value: 50, confirmed: false, origin: Origin::Window });

    assert_eq!(results(&events)[0].kind, ResultKind::NotResponding);
    assert_eq!(link.script().write_calls, 1);
    let mut seen = statuses(&events);
    assert!(seen.contains(&MonitorStatus::NotResponding));
    wait_until(|| {
        seen.extend(statuses(&h.flush()));
        seen.contains(&MonitorStatus::Connected)
    });
}

#[test]
fn input_switch_is_skipped_when_already_there() {
    let (_dir, h) = connected();
    let result = h.set(0x60, 0x11, false);
    assert_eq!((result.kind, result.message.as_str()), (ResultKind::Ok, "Input → HDMI"));
    assert!(h.link.state().writes.is_empty());
}

#[test]
fn input_switch_writes_once_and_confirms_with_a_re_read() {
    let (_dir, h) = connected();
    let reads = h.link.state().reads.len();

    let events = h.run(Command::Set { code: 0x60, value: 0x0F, confirmed: false, origin: Origin::Window });

    assert_eq!(results(&events)[0].message, "Input → DisplayPort");
    assert_eq!(values(&events), [(0x60, reading(0x0F, 49))]);
    assert_eq!(h.link.state().writes, [(0x60, 0x0F)]);
    assert_eq!(h.link.state().reads.len(), reads + 2); // before and after
}

#[test]
fn input_switch_reports_when_the_monitor_stayed() {
    let (_dir, h) = connected();
    h.link.state().ignored_writes.insert(0x60);

    let result = h.set(0x60, 0x0F, false);

    assert_eq!((result.outcome, result.kind), (WriteOutcome::Blocked, ResultKind::Kept));
    assert_eq!(result.message, "The monitor stayed on HDMI");
    assert_eq!(result.kept, Some(0x11));
    assert_eq!(h.link.state().writes, [(0x60, 0x0F)]);
}

#[test]
fn input_switch_counts_a_silent_monitor_afterwards_as_done() {
    let (_dir, h, link) = scripted();
    // This PC can't read the input at all, as when the monitor shows the other PC.
    link.script().read.insert(0x60, LinkError::Transient("no reply".into()));

    let events = h.run(Command::Set { code: 0x60, value: 0x0F, confirmed: false, origin: Origin::Window });

    let result = &results(&events)[0];
    assert_eq!((result.kind, result.message.as_str()), (ResultKind::Ok, "Input → DisplayPort"));
    assert_eq!(h.link.state().writes, [(0x60, 0x0F)]);
    assert!(statuses(&events).is_empty());
}

#[test]
fn input_readings_keep_only_the_low_byte() {
    let link = FakeLink::new();
    link.state().values.insert(0x60, reading(0x0311, 49));
    let dir = TempDir::new();
    let h = Harness::new(&link, dir.path());

    assert!(values(&h.connect()).contains(&(0x60, reading(0x11, 49))));
}

#[test]
fn step_re_reads_a_stale_value_first() {
    let dir = TempDir::new();
    let timing = Timing { stale_after: Duration::ZERO, ..Timing::zero() };
    let h = Harness::with_timing(&FakeLink::new(), dir.path(), timing);
    h.connect();
    h.link.state().values.insert(0x10, reading(40, 100)); // the other PC changed it

    let result = h.result(Command::Step { code: 0x10, delta: 5, origin: Origin::Hotkey });

    assert_eq!(result.message, "Brightness 45");
    assert_eq!(h.link.state().writes, [(0x10, 45)]);
}

#[test]
fn safe_mode_skips_the_capability_read_and_the_finder_but_still_writes() {
    let dir = TempDir::new();
    let fake = FakeLink::new();
    let h = Harness::spawn(Box::new(fake.clone()), &fake, dir.path(), Timing::zero(), true);

    let events = h.connect();
    assert!(events.iter().any(|e| matches!(e, Event::SafeMode(true))));
    assert_eq!(statuses(&events).last(), Some(&MonitorStatus::Connected));
    assert_eq!(fake.state().capability_reads, 0);

    assert_eq!(h.set(0x12, 50, false).kind, ResultKind::Ok);
    assert!(matches!(h.run(Command::FinderSnapshot)[..], [Event::Finder(FinderEvent::Refused(ResultKind::SafeMode))]));
    h.run(Command::ReadCapabilities);
    assert_eq!(fake.state().capability_reads, 0);

    assert!(matches!(h.run(Command::SetSafeMode(false))[..], [Event::SafeMode(false)]));
    h.run(Command::ReadCapabilities);
    assert_eq!(fake.state().capability_reads, 1);
}

#[test]
fn schedule_origin_is_passed_through() {
    let (_dir, h) = connected();
    let preset = Preset { name: "Night".into(), brightness: 10, contrast: 60, colour_mode: None };
    let events = h.run(Command::ApplyPreset { preset, origin: Origin::Schedule });
    assert!(events.iter().any(|e| matches!(e, Event::Result { origin: Origin::Schedule, .. })));
}
