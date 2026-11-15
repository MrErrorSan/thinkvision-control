//! Support levels and the profile registry (w3-support).

use std::time::Duration;

use crate::fake::P32P20_CAPABILITIES;
use crate::identity::MonitorIdentity;
use crate::mccs::{self, MonitorCapabilities};
use crate::profile::ProfileStore;
use crate::support::{resolve, SupportLevel};

use super::TempDir;

fn ident(edid: &str) -> MonitorIdentity {
    MonitorIdentity { edid_id: edid.into(), ..Default::default() }
}

fn caps(text: &str) -> MonitorCapabilities {
    mccs::parse(text).unwrap()
}

fn verified(profile: &str) -> SupportLevel {
    SupportLevel::Verified { profile: profile.into() }
}

/// A Lenovo relative: a subset of the P32p-20's colour modes and overdrive values, one input the P32p-20 doesn't
/// label, a reset, an Enum with no list, its own name for 0xE0.
const T24: &str = "(prot(monitor)model(ThinkVision T24)vcp(04 10 14(01 0B) 60(11 13) E0(00 01) EA CA D6(01 05))\
vcpname(E0(Overdrive Level)))";

#[test]
fn resolve_len62a2_is_verified() {
    let p32 = caps(P32P20_CAPABILITIES);
    for (caps, failed) in [(None, false), (Some(&p32), false), (None, true), (Some(&p32), true)] {
        assert_eq!(resolve(&ident("LEN62A2"), caps, failed), verified("p32p20"), "{failed}");
    }
}

#[test]
fn resolve_len_subset_is_lenovo_family() {
    assert_eq!(resolve(&ident("LEN66B1"), Some(&caps(T24)), false), SupportLevel::LenovoFamily);
    assert_eq!(resolve(&ident("LEN66B1"), Some(&caps("(vcp(10 E0(00 02)))")), false), SupportLevel::LenovoFamily);
}

#[test]
fn resolve_len_without_subset_is_generic() {
    // Only Range codes, values outside the P32p-20's labels, an Enum with no list.
    for text in ["(vcp(10 12))", "(vcp(10 14(01 13) E0(00 07)))", "(vcp(10 EA 14()))"] {
        assert_eq!(resolve(&ident("LEN66B1"), Some(&caps(text)), false), SupportLevel::GenericMccs, "{text}");
    }
}

#[test]
fn resolve_del_is_generic() {
    assert_eq!(resolve(&ident("DEL4321"), Some(&caps(T24)), false), SupportLevel::GenericMccs);
    assert_eq!(resolve(&ident("DEL4321"), None, false), SupportLevel::GenericMccs);
    assert_eq!(resolve(&ident("LEN66B1"), None, false), SupportLevel::GenericMccs);
    assert_eq!(resolve(&ident(""), None, false), SupportLevel::GenericMccs);
}

#[test]
fn resolve_failed_read_is_read_only() {
    assert_eq!(resolve(&ident("DEL4321"), None, true), SupportLevel::ReadOnly);
    assert_eq!(resolve(&ident("LEN66B1"), None, true), SupportLevel::ReadOnly);
    // A cached string still counts.
    assert_eq!(resolve(&ident("LEN66B1"), Some(&caps(T24)), true), SupportLevel::LenovoFamily);
}

#[test]
fn for_level_verified_p32p20_equals_new() {
    let dir = TempDir::new();
    let p32 = caps(P32P20_CAPABILITIES);
    let new = ProfileStore::new(dir.path());
    for given in [None, Some(&p32)] {
        let store = ProfileStore::for_level(dir.path(), &verified("p32p20"), given);
        assert_eq!(store.profile(), new.profile());
        assert_eq!(store.resolve(&p32), new.resolve(&p32));
        assert_eq!(store.synthesize_capabilities(), new.synthesize_capabilities());
    }
}

#[test]
fn for_level_unknown_verified_name_is_generic() {
    let dir = TempDir::new();
    let store = ProfileStore::for_level(dir.path(), &verified("nope"), None);
    let generic = ProfileStore::for_level(dir.path(), &SupportLevel::GenericMccs, None);
    assert_eq!(store.profile().control_order, generic.profile().control_order);
    assert_eq!(store.synthesize_capabilities(), generic.synthesize_capabilities());
}

#[test]
fn for_level_backlight_is_brightness_only_zero_pacing() {
    let dir = TempDir::new();
    let store = ProfileStore::for_level(dir.path(), &verified("builtin-backlight"), None);
    let p = store.profile();

    assert_eq!((p.timing.gap, p.timing.settle), (Duration::ZERO, Duration::ZERO));
    assert_eq!(p.control_order, [0x10]);
    assert!(p.inputs.is_empty());
    assert_eq!(store.synthesize_capabilities().vcp.keys().copied().collect::<Vec<_>>(), [0x10]);
    let controls = store.resolve(&caps(P32P20_CAPABILITIES));
    assert_eq!(controls[&0x10].name, "Brightness");
    assert!(controls.values().filter(|d| d.code != 0x10).all(|d| !d.is_visible()));
}

#[test]
fn for_level_lenovo_filters_and_uses_vcpname() {
    let dir = TempDir::new();
    let t24 = caps(T24);
    let store = ProfileStore::for_level(dir.path(), &SupportLevel::LenovoFamily, Some(&t24));
    let p = store.profile();

    assert_eq!((p.edid_id.as_str(), p.display_name.as_str(), p.short_name.as_str()),
        ("", "ThinkVision T24", "ThinkVision T24"));
    // 0x60 advertises 0x13, which the P32p-20 doesn't label: the input stays, named from the MCCS table.
    assert_eq!(p.inputs.iter().map(|i| i.code).collect::<Vec<_>>(), [0x11, 0x13]);
    assert_eq!(p.inputs[1].label, "Input 0x13");
    let c = store.resolve(&t24);
    assert_eq!(c[&0xE0].name, "Overdrive Level");
    assert_eq!(c[&0xE0].value_labels[&0x01], "Normal");
    assert!(c[&0xE0].is_visible());
    assert_eq!(c[&0x14].name, "Colour mode");
    assert!(c[&0x10].is_visible());
    assert!(c[&0x60].is_visible());
    assert_eq!(c[&0x60].value_labels[&0x13], "Input 0x13");
    for code in [0x04, 0xEA] {
        assert!(!c[&code].is_visible(), "{code:#04X}");
        assert!(c[&code].name.starts_with("Unknown"), "{code:#04X}");
    }
    assert_eq!(c[&0xD6].name, "Power");
    assert!(!c[&0xD6].is_visible());
    // Range codes it doesn't advertise and P32p-20-only codes are gone.
    assert!(!store.synthesize_capabilities().supports(0x12));
    assert!(!store.synthesize_capabilities().supports(0xCC));
}

#[test]
fn for_level_lenovo_keeps_inputs_it_labels() {
    let dir = TempDir::new();
    let caps = caps("(vcp(10 60(11 0F)))");
    let store = ProfileStore::for_level(dir.path(), &SupportLevel::LenovoFamily, Some(&caps));
    let inputs: Vec<_> = store.profile().inputs.iter().map(|i| (i.code, i.label.as_str())).collect();
    assert_eq!(inputs, [(0x11, "HDMI"), (0x0F, "DisplayPort")]);
}

#[test]
fn for_level_lenovo_without_caps_enables_no_value_lists() {
    let dir = TempDir::new();
    let store = ProfileStore::for_level(dir.path(), &SupportLevel::LenovoFamily, None);
    let synthesized = store.synthesize_capabilities();
    assert_eq!(synthesized.vcp.keys().copied().collect::<Vec<_>>(), [0xCA, 0xD6]);
    assert_eq!(store.profile().display_name, "");
}

#[test]
fn for_level_generic_advertised_values_only() {
    let dir = TempDir::new();
    let caps = caps("(vcp(10 14(01 05 0E) 60(11 0F 31)))");
    for level in [SupportLevel::GenericMccs, SupportLevel::ReadOnly] {
        let store = ProfileStore::for_level(dir.path(), &level, Some(&caps));
        let c = store.resolve(&caps);

        let modes: Vec<_> = c[&0x14].value_labels.iter().map(|(k, v)| (*k, v.as_str())).collect();
        assert_eq!(modes, [(0x01, "sRGB"), (0x05, "6500 K"), (0x0E, "Mode 0x0E")], "{level:?}");
        let inputs: Vec<_> = c[&0x60].value_labels.iter().map(|(k, v)| (*k, v.clone())).collect();
        let expected = |v: u8| mccs::input_name(v).map_or_else(|| format!("Input 0x{v:02X}"), str::to_string);
        assert_eq!(inputs, [(0x0F, expected(0x0F)), (0x11, expected(0x11)), (0x31, "Input 0x31".into())]);
        let p = store.profile();
        let buttons: Vec<_> = p.inputs.iter().map(|i| (i.code, i.label.as_str(), i.kind.as_str())).collect();
        assert_eq!(buttons, [(0x11, "HDMI 1", "hdmi"), (0x0F, "DisplayPort 1", "dp"), (0x31, "Input 0x31", "")]);
        assert_eq!((p.display_name.as_str(), p.short_name.as_str()), ("", ""), "{level:?}");
        assert_eq!(p.edid_id, "");
    }
}

#[test]
fn for_level_generic_takes_the_model_text() {
    let dir = TempDir::new();
    let dell = caps("(model( DELL U2720Q )vcp(10))");
    let p = ProfileStore::for_level(dir.path(), &SupportLevel::GenericMccs, Some(&dell)).profile().clone();
    assert_eq!((p.display_name.as_str(), p.short_name.as_str()), ("DELL U2720Q", "DELL U2720Q"));
    let none = ProfileStore::for_level(dir.path(), &SupportLevel::GenericMccs, None);
    assert!(none.profile().inputs.is_empty());
    assert!(none.synthesize_capabilities().vcp[&0x14].as_ref().unwrap().is_empty());
}

#[test]
fn control_order_matches_the_profile() {
    let dir = TempDir::new();
    let order = |level: SupportLevel, caps: Option<&MonitorCapabilities>| {
        ProfileStore::for_level(dir.path(), &level, caps).profile().control_order.clone()
    };
    assert_eq!(order(verified("p32p20"), None),
        [0x10, 0x12, 0x14, 0x16, 0x18, 0x1A, 0x60, 0xE0, 0xEA, 0x86, 0xEC, 0xCC, 0x04, 0x05, 0x08, 0xCA, 0xD6]);
    assert_eq!(order(SupportLevel::GenericMccs, None),
        [0x10, 0x12, 0x14, 0x16, 0x18, 0x1A, 0x60, 0x62, 0x87, 0xCA, 0xD6]);
    assert_eq!(order(SupportLevel::LenovoFamily, Some(&caps(T24))), [0x10, 0x14, 0x60, 0xE0, 0xCA, 0xD6]);
}
