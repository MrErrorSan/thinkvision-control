//! the crash marker.

use super::*;
use crate::crash::{process_key, Marker};

fn stale(dir: &Path) -> Vec<(String, String)> {
    Marker::left_over(dir, |_| false).into_iter().map(|l| (l.key, l.operation)).collect()
}

#[test]
fn marker_left_over_names_the_unfinished_operation() {
    let dir = TempDir::new();
    let marker = Marker::new(dir.path(), "pid-4000000001");
    assert!(stale(dir.path()).is_empty());

    marker.arm("finder snapshot");
    assert!(dir.path().join("risky-operation-pid-4000000001.txt").exists());
    assert_eq!(stale(dir.path()), [("pid-4000000001".to_string(), "finder snapshot".to_string())]);

    marker.clear();
    assert!(stale(dir.path()).is_empty());
}

#[test]
fn a_live_owner_and_this_process_are_not_left_over() {
    let dir = TempDir::new();
    Marker::new(dir.path(), "pid-4000000002").arm("capability read");
    Marker::new(dir.path(), &process_key()).arm("capability read");
    assert!(Marker::left_over(dir.path(), |pid| pid == 4_000_000_002).is_empty());
}

#[test]
fn the_0_0_8_marker_is_stale_and_clearable() {
    let dir = TempDir::new();
    std::fs::write(dir.path().join("risky-operation.txt"), "capability read").unwrap();
    let found = Marker::left_over(dir.path(), |_| true);
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].key.as_str(), found[0].operation.as_str()), ("", "capability read"));
    found[0].clear();
    assert!(stale(dir.path()).is_empty());
}

#[test]
fn marker_is_removed_after_the_operation() {
    let dir = TempDir::new();
    let marker = Marker::new(dir.path(), "pid-4000000003");
    marker.arm("capability read");
    marker.disarm();
    assert!(stale(dir.path()).is_empty());
}

#[test]
fn a_finished_connect_leaves_no_marker() {
    let dir = TempDir::new();
    let h = Harness::new(&FakeLink::new(), dir.path());
    h.connect();
    h.run(Command::FinderSnapshot);
    assert_eq!(h.link.state().capability_reads, 1);
    assert!(!dir.path().join(format!("risky-operation-{}.txt", process_key())).exists());
}

#[test]
fn regression_core_12_markers_are_per_owner() {
    let dir = TempDir::new();
    let a = Marker::new(dir.path(), "pid-4000000004");
    let b = Marker::new(dir.path(), "pid-4000000005");
    a.arm("capability read");
    b.arm("finder snapshot");
    b.disarm();
    assert_eq!(stale(dir.path()), [("pid-4000000004".to_string(), "capability read".to_string())]);
}

#[test]
fn named_markers_follow_their_owner_process() {
    let dir = TempDir::new();
    Marker::new(dir.path(), "pid-4000000006-LEN62A2-N1").arm("capability read");
    Marker::new(dir.path(), &format!("{}-LEN62A2-N2", process_key())).arm("capability read");
    Marker::new(dir.path(), &process_key()).arm("finder snapshot");
    // Alive: neither is left over; this process's are never left over.
    assert!(Marker::left_over(dir.path(), |pid| pid == 4_000_000_006).is_empty());
    assert_eq!(stale(dir.path()), [("pid-4000000006-LEN62A2-N1".to_string(), "capability read".to_string())]);

    // An orderly exit clears every marker of this process, and only those.
    Marker::clear_process(dir.path());
    assert!(!dir.path().join(format!("risky-operation-{}.txt", process_key())).exists());
    assert!(!dir.path().join(format!("risky-operation-{}-LEN62A2-N2.txt", process_key())).exists());
    let left = Marker::left_over(dir.path(), |_| false);
    assert_eq!(left.len(), 1);
    left[0].clear();
    assert!(stale(dir.path()).is_empty());
}
