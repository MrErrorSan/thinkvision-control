//! the crash marker.

use super::*;
use crate::crash::Marker;

#[test]
fn marker_left_over_names_the_unfinished_operation() {
    let dir = TempDir::new();
    let marker = Marker::new(dir.path());
    assert_eq!(marker.left_over(), None);

    marker.arm("finder snapshot");
    assert!(dir.path().join("risky-operation.txt").exists());
    assert_eq!(Marker::new(dir.path()).left_over().as_deref(), Some("finder snapshot"));

    marker.clear();
    assert_eq!(marker.left_over(), None);
}

#[test]
fn marker_is_removed_after_the_operation() {
    let dir = TempDir::new();
    let marker = Marker::new(dir.path());
    marker.arm("capability read");
    marker.disarm();
    assert_eq!(marker.left_over(), None);
}

#[test]
fn a_finished_connect_leaves_no_marker() {
    let dir = TempDir::new();
    let h = Harness::new(&FakeLink::new(), dir.path());
    h.connect();
    h.run(Command::FinderSnapshot);
    assert_eq!(h.link.state().capability_reads, 1);
    assert_eq!(Marker::new(dir.path()).left_over(), None);
}
