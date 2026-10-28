//! SafetyPolicyTests, finder diff, clock and log.

use std::collections::BTreeMap;

use crate::fake::P32P20_CAPABILITIES;
use crate::finder::{diff, snapshot_codes, CodeChange};
use crate::mccs;
use crate::profile::ProfileStore;
use crate::safety::{evaluate, Decision, Verdict};

use super::TempDir;

fn eval(code: u8, value: u16, max: Option<u16>, confirmed: bool) -> Decision {
    let caps = mccs::parse(P32P20_CAPABILITIES).unwrap();
    let dir = TempDir::new();
    let controls = ProfileStore::new(dir.path()).resolve(&caps);
    evaluate(controls.get(&code), &caps, code, value, max, confirmed)
}

#[test]
fn osd_enable_is_never_written() {
    let d = eval(0xCA, 1, None, true);
    assert_eq!((d.verdict, d.message.as_str()), (Verdict::Block, "OSD enable is never written."));
}

#[test]
fn power_is_never_written() {
    assert_eq!(eval(0xD6, 0x05, None, true).verdict, Verdict::Block);
    assert_eq!(eval(0xD6, 0x01, None, false).verdict, Verdict::Block);
}

#[test]
fn input_value_outside_capability_list_is_blocked() {
    let d = eval(0x60, 0x13, None, false);
    assert_eq!((d.verdict, d.message.as_str()), (Verdict::Block, "0x13 is not a valid value for Input."));
}

#[test]
fn range_value_is_clamped_to_max() {
    let d = eval(0x10, 150, Some(100), false);
    assert_eq!((d.verdict, d.value), (Verdict::Allow, 100));
    assert_eq!(eval(0x10, 150, None, false).value, 100);
    assert_eq!(eval(0x10, 150, Some(200), false).value, 150);
}

#[test]
fn factory_reset_ignores_firmware_bytes_and_forces_value_1() {
    let d = eval(0x04, 1, None, false);
    assert_eq!(d.verdict, Verdict::NeedsConfirmation);
    assert!(d.message.starts_with("Reset ALL monitor settings"));
    let d = eval(0x04, 0x4C, None, true);
    assert_eq!((d.verdict, d.value), (Verdict::Allow, 1));
}

#[test]
fn unnamed_vendor_code_is_blocked() {
    let d = eval(0xEB, 1, None, true);
    assert_eq!((d.verdict, d.message.as_str()), (Verdict::Block, "Unknown setting — identify it with the finder first."));
}

#[test]
fn unsupported_code_is_blocked() {
    let d = eval(0x62, 10, None, false);
    assert_eq!((d.verdict, d.message.as_str()), (Verdict::Block, "This monitor does not support 0x62."));
}

#[test]
fn valid_input_is_allowed() {
    assert_eq!(eval(0x60, 0x31, None, false).verdict, Verdict::Allow);
}

#[test]
fn diff_lists_changed_codes_present_in_both() {
    let before = BTreeMap::from([(0xEB, 0), (0xC8, 5), (0x52, 1)]);
    let after = BTreeMap::from([(0xEB, 1), (0xC8, 5), (0xF6, 3), (0x02, 9)]);
    assert_eq!(diff(&before, &after), [CodeChange { code: 0xEB, before: 0, after: 1 }]);
}

#[test]
fn finder_never_reads_osd_enable_or_power() {
    let caps = mccs::parse(P32P20_CAPABILITIES).unwrap();
    let dir = TempDir::new();
    let codes = snapshot_codes(&ProfileStore::new(dir.path()).resolve(&caps));
    assert_eq!(codes, [0x02, 0x52, 0xAC, 0xAE, 0xB2, 0xB6, 0xC6, 0xC8, 0xC9, 0xDF, 0xEB, 0xF6, 0xF7, 0xF8, 0xFA]);
}

#[test]
fn civil_time_from_unix_millis() {
    let t = crate::clock::from_unix_millis(1_793_189_296_789); // 2026-10-28 12:08:16.789 UTC
    assert_eq!((t.year, t.month, t.day, t.hour, t.minute, t.second, t.millis), (2026, 10, 28, 12, 8, 16, 789));
    let t = crate::clock::from_unix_millis(951_782_400_000); // 2000-02-29
    assert_eq!((t.year, t.month, t.day), (2000, 2, 29));
}

#[test]
fn log_appends_stamped_lines_and_rolls_past_1_mb() {
    let dir = TempDir::new();
    crate::log::write_to(dir.path(), "hello");
    let text = std::fs::read_to_string(dir.path().join("log.txt")).unwrap();
    assert!(text.ends_with(" hello\n"));
    assert_eq!(text.find(' '), Some(10)); // "yyyy-MM-dd HH:mm:ss.fff "
    assert_eq!(text.len(), "yyyy-MM-dd HH:mm:ss.fff hello\n".len());

    std::fs::write(dir.path().join("log.txt"), vec![b'x'; 1024 * 1024 + 1]).unwrap();
    crate::log::write_to(dir.path(), "after roll");
    assert!(dir.path().join("log.1.txt").exists());
    assert!(std::fs::read_to_string(dir.path().join("log.txt")).unwrap().ends_with(" after roll\n"));

    crate::log::write_to(&dir.path().join("missing").join("deeper"), "no panic");
}
