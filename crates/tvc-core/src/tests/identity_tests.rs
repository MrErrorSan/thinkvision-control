//! Monitor ids and matching saved state (multi-display contracts).

use crate::identity::{assign_ids, match_saved, MonitorId, MonitorIdentity};

fn ident(edid: &str, serial: Option<&str>, connector: Option<&str>) -> MonitorIdentity {
    MonitorIdentity { edid_id: edid.into(), serial: serial.map(Into::into), connector: connector.map(Into::into) }
}

fn ids(list: &[MonitorIdentity]) -> Vec<String> {
    assign_ids(list).into_iter().map(|id| id.0).collect()
}

#[test]
fn unique_serial_gives_s_duplicate_gives_p_neither_gives_n() {
    assert_eq!(ids(&[ident("LEN62A2", Some("V1"), Some("DP-1"))]), ["LEN62A2-SV1"]);
    assert_eq!(ids(&[ident("LEN62A2", Some("V1"), Some("DP-1")), ident("LEN62A2", Some("V1"), Some("DP-2"))]),
        ["LEN62A2-PDP-1", "LEN62A2-PDP-2"]);
    assert_eq!(ids(&[ident("DEL4321", None, None), ident("DEL4321", None, None), ident("", None, None)]),
        ["DEL4321-N1", "DEL4321-N2", "UNKNOWN-N1"]);
}

#[test]
fn the_same_serial_on_another_model_is_still_unique() {
    assert_eq!(ids(&[ident("LEN62A2", Some("1"), None), ident("DEL4321", Some("1"), None)]),
        ["LEN62A2-S1", "DEL4321-S1"]);
}

#[test]
fn only_n_ids_are_unstable_and_folders_are_safe() {
    assert!(MonitorId("DEL4321-N1".into()).is_unstable());
    assert!(!MonitorId("LEN62A2-SV1".into()).is_unstable());
    assert!(!MonitorId("LEN62A2-PDP-1".into()).is_unstable());
    assert!(MonitorId("UNKNOWN-N12".into()).is_unstable());
    // Dashes in a serial or connector: only the marker right after the EDID id counts.
    for stable in ["LEN62A2-SAB-N123", "LEN62A2-SN-1", "LEN62A2-SN12", "LEN62A2-Pcard0-N1", "LEN62A2-PDP-N2",
        "LEN62A2-SNX", "LEN62A2-N", "LEN62A2-N1-2", "LEN62A2-N1a", "LEN62A2", ""] {
        assert!(!MonitorId(stable.into()).is_unstable(), "{stable}");
    }
    assert_eq!(MonitorId(r"LEN62A2-P\?\DISPLAY#1".into()).folder_name(), "LEN62A2-P___DISPLAY_1");
}

#[test]
fn saved_state_matches_by_id_then_connector_then_the_only_same_model() {
    let saved = vec![
        (MonitorId("LEN62A2-SV1".into()), ident("LEN62A2", Some("V1"), Some("DP-1"))),
        (MonitorId("DEL4321-PHDMI-1".into()), ident("DEL4321", None, Some("HDMI-1"))),
        (MonitorId("DEL4321-PDP-3".into()), ident("DEL4321", None, Some("DP-3"))),
    ];
    let id = |s: &str| MonitorId(s.into());
    assert_eq!(match_saved(&id("LEN62A2-SV1"), &ident("LEN62A2", Some("V1"), None), &saved), Some(&saved[0].0));
    assert_eq!(match_saved(&id("DEL4321-N1"), &ident("DEL4321", None, Some("DP-3")), &saved), Some(&saved[2].0));
    // A new serial on the one saved LEN62A2: the 0.0.8 migration rule.
    assert_eq!(match_saved(&id("LEN62A2-SV9"), &ident("LEN62A2", Some("V9"), None), &saved), Some(&saved[0].0));
    // Two saved DEL4321 entries and no connector match: ambiguous.
    assert_eq!(match_saved(&id("DEL4321-N1"), &ident("DEL4321", None, None), &saved), None);
    assert_eq!(match_saved(&id("ACR0001-N1"), &ident("ACR0001", None, None), &saved), None);
}
