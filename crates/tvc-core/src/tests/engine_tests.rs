//! ControlServiceTests, OfflineAndReconnectTests, StatusReasonTests and DdcQueueTests, against the engine.

use std::time::{Duration, Instant};

use super::*;
use crate::engine::{Event, FinderEvent, WriteOutcome, SHUTDOWN_WAIT};
use crate::finder::CodeChange;
use crate::link::LinkFailure;
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

    assert_eq!(result.outcome, WriteOutcome::Blocked);
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

    let events = h.run(Command::Set { code: 0x60, value: 0x0F, confirmed: false, origin: Origin::Window });

    assert_eq!(results(&events)[0].message, "Input → DisplayPort");
    assert_eq!(values(&events), [(0x60, reading(0x0F, 49))]);
    assert_eq!(h.link.state().reads.len(), reads);
    assert_eq!(h.set(0x60, 0x13, false).message, "0x13 is not a valid value for Input.");
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

    assert_eq!(h.set(0xCA, 2, true).outcome, WriteOutcome::Blocked);
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

    assert_eq!(h.set(0x10, 30, false).outcome, WriteOutcome::NotResponding);
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
    wait_until(|| h.link.state().connects == connects + 5);
}

#[test]
fn retry_burst_runs_five_attempts_then_stops() {
    assert_eq!(Timing::default().burst, [500, 1000, 2000, 4000, 8000].map(Duration::from_millis));
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
