//! CapabilitiesParserTests and ProfileStoreTests, plus the v2 profile fields.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::fake::P32P20_CAPABILITIES;
use crate::mccs::{self, MonitorCapabilities};
use crate::profile::{ControlDefinition, ControlKind, Group, ProfileStore, SafetyClass, Section};

use super::TempDir;

fn caps() -> MonitorCapabilities {
    mccs::parse(P32P20_CAPABILITIES).unwrap()
}

#[test]
fn parses_real_p32p20_string() {
    let caps = caps();

    assert_eq!(caps.model, "ThinkVision P32p-20");
    assert_eq!(caps.vcp[&0x60], Some(vec![0x11, 0x0F, 0x31]));
    assert_eq!(caps.vcp[&0x10], None);
    assert_eq!(caps.vcp[&0xCA], Some(vec![0x01, 0x02]));
    assert_eq!(caps.vcp[&0xC6].as_ref().unwrap().len(), 17);
    assert!(caps.supports(0xFA));
    assert!(caps.allows_value(0x60, 0x31));
    assert!(!caps.allows_value(0x60, 0x13));
    assert!(caps.allows_value(0x10, 87));
    assert!(!caps.allows_value(0x60, 0x111));
    assert_eq!(caps.firmware().as_deref(), Some("LB1.2"));
}

#[test]
fn parses_tokens_without_spaces() {
    let caps = mccs::parse("(vcp(0210(01 02)12 60(0F11)))").unwrap();

    assert_eq!(caps.vcp.keys().copied().collect::<Vec<_>>(), [0x02, 0x10, 0x12, 0x60]);
    assert_eq!(caps.vcp[&0x60], Some(vec![0x0F, 0x11]));
}

#[test]
fn missing_vcp_section_or_bad_syntax_is_an_error() {
    assert!(mccs::parse("(prot(monitor)type(LCD))").is_err());
    assert!(mccs::parse("(vcp(10 12(01)").is_err());
    assert!(mccs::parse("(vcp(10 1G))").is_err());
    // "vcp(" inside another name or nested deeper doesn't count.
    assert!(mccs::parse("(xvcp(10))").is_err());
    assert!(mccs::parse("(a(b(vcp(10))))").is_err());
}

#[test]
fn resolves_known_and_unknown_codes() {
    let dir = TempDir::new();
    let controls = ProfileStore::new(dir.path()).resolve(&caps());

    let labels = &controls[&0x14].value_labels;
    for (code, label) in [(0x01, "DCI-P3"), (0x05, "Bluish"), (0x06, "Neutral"), (0x08, "Low Blue Light"), (0x0B, "sRGB"),
        (0x0E, "Reddish"), (0x0F, "BT.709"), (0x12, "User / Custom")] {
        assert_eq!(labels[&code], label);
    }
    assert_eq!(controls[&0xE0].name, "Over Drive");
    assert_eq!(controls[&0xE0].value_labels[&0x02], "Extreme");
    assert_eq!(controls[&0xEA].name, "Dynamic Contrast");
    assert_eq!(controls[&0xEC].name, "USB Charging");
    assert_eq!(controls[&0x60].value_labels[&0x31], "USB-C");
    assert_eq!(controls[&0xCA].safety, SafetyClass::NeverWrite);
    assert_eq!(controls[&0x04].safety, SafetyClass::ConfirmRequired);
    assert_eq!(controls[&0xEB].name, "Unknown 0xEB");
    assert_eq!(controls[&0xEB].section, Section::Hidden);
    assert_eq!(controls[&0xEB].safety, SafetyClass::ReadOnlyUntilNamed);
    assert_eq!(controls.len(), caps().vcp.len());
}

#[test]
fn power_is_hidden_and_never_written() {
    let dir = TempDir::new();
    let d6 = &ProfileStore::new(dir.path()).resolve(&caps())[&0xD6];
    assert_eq!((d6.name.as_str(), d6.section, d6.safety), ("Power", Section::Hidden, SafetyClass::NeverWrite));
    assert!(!d6.is_visible());
}

#[test]
fn profile_v2_fields() {
    let dir = TempDir::new();
    let store = ProfileStore::new(dir.path());
    let p = store.profile();

    assert_eq!((p.edid_id.as_str(), p.display_name.as_str(), p.short_name.as_str()), ("LEN62A2", "ThinkVision P32p-20", "P32p-20"));
    let inputs: Vec<_> = p.inputs.iter().map(|i| (i.code, i.label.as_str(), i.kind.as_str())).collect();
    assert_eq!(inputs, [(0x11, "HDMI", "hdmi"), (0x0F, "DisplayPort", "dp"), (0x31, "USB-C", "usbc")]);
    assert_eq!(p.colour_mode_order, [0x0B, 0x01, 0x0F, 0x06, 0x0E, 0x05, 0x08, 0x12]);
    assert_eq!(p.custom_colour_mode, 0x12);
    assert_eq!(p.brightness_lock_note, " — the current colour mode may lock it");
    assert_eq!((p.timing.gap, p.timing.settle, p.timing.retry, p.timing.attempts),
        (Duration::from_millis(50), Duration::from_millis(400), Duration::from_millis(150), 5));

    let c = store.resolve(&caps());
    assert_eq!(c[&0xE0].group, Some(Group::Picture));
    assert_eq!(c[&0xEC].group, Some(Group::Ports));
    assert_eq!(c[&0xCC].group, Some(Group::Osd));
    assert_eq!(c[&0x04].group, Some(Group::Reset));
    assert_eq!(c[&0x10].icon.as_deref(), Some("brightness"));
    assert_eq!(c[&0xEA].description.as_deref(), Some("Adjusts contrast to match what's on screen"));
    assert_eq!(c[&0x04].confirm_title.as_deref(), Some("Reset monitor to factory settings?"));
    assert_eq!(c[&0x05].confirm_button.as_deref(), Some("Reset"));
    assert_eq!(c[&0x60].confirm_title.as_deref(), Some("Switch to {label}?"));
    // Every embedded control parsed, and every icon is a known glyph name.
    let glyphs = ["overdrive", "dyn-contrast", "aspect", "usb", "language", "reset", "brightness", "contrast", "colour", "input"];
    let embedded = store.resolve(&store.synthesize_capabilities());
    assert_eq!(embedded.len(), 17);
    assert!(embedded.values().filter_map(|d| d.icon.as_deref()).all(|i| glyphs.contains(&i)));
}

#[test]
fn synthesized_capabilities_follow_the_profile() {
    let dir = TempDir::new();
    let caps = ProfileStore::new(dir.path()).synthesize_capabilities();
    assert_eq!(caps.vcp[&0x10], None);
    assert_eq!(caps.vcp[&0x60], Some(vec![0x0F, 0x11, 0x31]));
    assert_eq!(caps.vcp[&0x04], Some(vec![1]));
    assert!(!caps.supports(0xEB));
}

#[test]
fn overlay_names_a_vendor_code() {
    let dir = TempDir::new();
    let store = ProfileStore::new(dir.path());
    let labels = BTreeMap::from([(0, "Off".to_string()), (1, "Normal".to_string()), (2, "Extreme".to_string())]);

    store.save_overlay(&ControlDefinition::new(0xE0, "Over Drive", ControlKind::Enum, Section::Advanced, SafetyClass::Normal, labels))
        .unwrap();
    store.save_overlay(&ControlDefinition::new(0xEB, "Eco", ControlKind::Enum, Section::Advanced, SafetyClass::Normal, BTreeMap::new()))
        .unwrap();
    let controls = ProfileStore::new(dir.path()).resolve(&caps());

    assert_eq!(controls[&0xE0].name, "Over Drive");
    assert_eq!(controls[&0xE0].safety, SafetyClass::Normal);
    assert_eq!(controls[&0xE0].value_labels[&2], "Extreme");
    assert_eq!(controls[&0xE0].group, Some(Group::Picture)); // inherited from the embedded entry
    assert_eq!(controls[&0xEB].name, "Eco");
    assert_eq!(controls[&0xEB].group, None);

    let text = std::fs::read_to_string(dir.path().join("profile.user.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["edidId"], "LEN62A2");
    assert_eq!(json["controls"][0]["code"], "0xE0");
    assert_eq!(json["controls"][0]["values"]["0x02"], "Extreme");
    assert_eq!(json["controls"][1]["confirm"], serde_json::Value::Null);
    assert!(json.get("displayName").is_none());
}

#[test]
fn old_format_overlay_loads() {
    let dir = TempDir::new();
    let overlay = r#"{
  "edidId": "LEN62A2",
  "controls": [
    { "code": "0xC8", "name": "Game mode", "kind": "Enum", "section": "Advanced", "safety": "Normal",
      "values": { "0x00": "Off", "0x01": "On" }, "confirm": null },
    { "code": "0xB2", "name": "Light sensor", "kind": "Enum", "section": "Advanced", "safety": "Normal" }
  ]
}"#;
    std::fs::write(dir.path().join("profile.user.json"), overlay).unwrap();

    let controls = ProfileStore::new(dir.path()).resolve(&caps());

    assert_eq!(controls[&0xC8].name, "Game mode");
    assert_eq!(controls[&0xC8].value_labels[&1], "On");
    assert!(controls[&0xC8].is_visible());
    assert_eq!(controls[&0xB2].name, "Light sensor");
}

#[test]
fn overlay_can_never_make_osd_enable_or_power_writable() {
    let dir = TempDir::new();
    let overlay = r#"{ "edidId": "LEN62A2", "controls": [
    { "code": "0xD6", "name": "Power", "kind": "Enum", "section": "Advanced", "safety": "Normal", "values": {} },
    { "code": "0xCA", "name": "OSD", "kind": "Enum", "section": "Advanced", "safety": "Normal", "values": {} } ] }"#;
    std::fs::write(dir.path().join("profile.user.json"), overlay).unwrap();
    let store = ProfileStore::new(dir.path());

    let controls = store.resolve(&caps());
    assert_eq!(controls[&0xD6].safety, SafetyClass::NeverWrite);
    assert_eq!(controls[&0xCA].safety, SafetyClass::NeverWrite);
    let power = ControlDefinition::new(0xD6, "Power", ControlKind::Enum, Section::Advanced, SafetyClass::Normal, BTreeMap::new());
    assert!(store.save_overlay(&power).is_err());
}

#[test]
fn corrupt_overlay_is_ignored() {
    let dir = TempDir::new();
    std::fs::write(dir.path().join("profile.user.json"), "{ not json").unwrap();
    assert_eq!(ProfileStore::new(dir.path()).resolve(&caps())[&0x10].name, "Brightness");

    let bad_code = r#"{ "edidId": "LEN62A2", "controls": [
    { "code": "0xC8", "name": "Game mode", "kind": "Enum", "section": "Advanced", "safety": "Normal" },
    { "code": "0xZZ", "name": "x", "kind": "Enum", "section": "Main", "safety": "Normal" } ] }"#;
    std::fs::write(dir.path().join("profile.user.json"), bad_code).unwrap();
    assert_eq!(ProfileStore::new(dir.path()).resolve(&caps())[&0xC8].name, "Unknown 0xC8");
}
