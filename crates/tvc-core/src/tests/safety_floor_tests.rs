//! The hard write floor in safety::evaluate (w1-safety).

use std::collections::BTreeMap;

use crate::fake::P32P20_CAPABILITIES;
use crate::mccs::{self, MonitorCapabilities};
use crate::profile::{ControlDefinition, ControlKind, ProfileStore, SafetyClass, Section};
use crate::safety::{evaluate, Verdict};
use crate::support::SupportLevel;

use super::TempDir;

fn verified() -> SupportLevel {
    SupportLevel::Verified { profile: "p32p20".into() }
}

fn levels() -> [SupportLevel; 4] {
    [verified(), SupportLevel::LenovoFamily, SupportLevel::GenericMccs, SupportLevel::ReadOnly]
}

fn p32() -> MonitorCapabilities {
    mccs::parse(P32P20_CAPABILITIES).unwrap()
}

/// The profile's definitions resolved against `caps`.
fn resolved(caps: &MonitorCapabilities) -> BTreeMap<u8, ControlDefinition> {
    let dir = TempDir::new();
    ProfileStore::new(dir.path()).resolve(caps)
}

fn normal(code: u8, name: &str) -> ControlDefinition {
    ControlDefinition::new(code, name, ControlKind::Enum, Section::Advanced, SafetyClass::Normal,
        [(1u8, "On".to_string()), (5u8, "Off".to_string())].into())
}

#[test]
fn profile_marking_d6_writable_is_still_refused() {
    let def = normal(0xD6, "Power");
    for level in levels() {
        let d = evaluate(Some(&def), &p32(), 0xD6, 5, None, &level, true);
        assert_eq!((d.verdict, d.message.as_str()), (Verdict::Block, "Power is never written."), "{level:?}");
    }
}

#[test]
fn profile_marking_ca_writable_is_still_refused() {
    let def = normal(0xCA, "OSD");
    for level in levels() {
        let d = evaluate(Some(&def), &p32(), 0xCA, 1, None, &level, true);
        assert_eq!((d.verdict, d.message.as_str()), (Verdict::Block, "OSD is never written."), "{level:?}");
    }
}

#[test]
fn read_only_refuses_every_write() {
    let caps = p32();
    let defs = resolved(&caps);
    for (code, value) in [(0x10, 50), (0x14, 0x0B), (0x60, 0x0F), (0x04, 1), (0xE0, 1)] {
        let d = evaluate(defs.get(&code), &caps, code, value, None, &SupportLevel::ReadOnly, true);
        assert_eq!((d.verdict, d.message.as_str()), (Verdict::Block, "This monitor is read-only."), "{code:#04X}");
    }
}

#[test]
fn factory_reset_refused_below_verified() {
    let caps = p32();
    let defs = resolved(&caps);
    for level in [SupportLevel::LenovoFamily, SupportLevel::GenericMccs] {
        let d = evaluate(defs.get(&0x04), &caps, 0x04, 1, None, &level, true);
        assert_eq!((d.verdict, d.message.as_str()),
            (Verdict::Block, "Factory reset is only allowed on verified monitors."), "{level:?}");
    }
}

#[test]
fn factory_reset_allowed_at_verified() {
    let caps = p32();
    let d = evaluate(resolved(&caps).get(&0x04), &caps, 0x04, 0x4C, None, &verified(), true);
    assert_eq!((d.verdict, d.value), (Verdict::Allow, 1));
}

#[test]
fn unnamed_vendor_code_refused_below_verified() {
    let caps = mccs::parse("(vcp(10 E5(00 01)))").unwrap();
    let unknown = resolved(&caps)[&0xE5].clone();
    assert_eq!(unknown.safety, SafetyClass::ReadOnlyUntilNamed);
    for level in [SupportLevel::LenovoFamily, SupportLevel::GenericMccs] {
        for def in [None, Some(&unknown)] {
            let d = evaluate(def, &caps, 0xE5, 1, None, &level, true);
            assert_eq!((d.verdict, d.message.as_str()),
                (Verdict::Block, "Unknown setting — identify it with the finder first."), "{level:?} {def:?}");
        }
    }
}

#[test]
fn vcpname_names_a_vendor_code_but_never_unlocks_it() {
    let caps = mccs::parse("(vcp(10 E5(00 01))vcpname(E5(Eco mode)))").unwrap();
    let defs = resolved(&caps);
    assert_eq!(defs[&0xE5].name, "Eco mode");
    for level in [SupportLevel::Verified { profile: "p32p20".into() }, SupportLevel::GenericMccs] {
        let d = evaluate(defs.get(&0xE5), &caps, 0xE5, 1, None, &level, false);
        assert_eq!(d.verdict, Verdict::Block, "{level:?}");
    }
}

#[test]
fn profile_named_vendor_code_allowed_below_verified() {
    let caps = p32();
    let defs = resolved(&caps);
    for level in [SupportLevel::LenovoFamily, SupportLevel::GenericMccs] {
        let d = evaluate(defs.get(&0xE0), &caps, 0xE0, 2, None, &level, false);
        assert_eq!((d.verdict, d.value), (Verdict::Allow, 2), "{level:?}");
    }
}

#[test]
fn input_needs_confirmation_whatever_profile_says() {
    let caps = p32();
    let input = resolved(&caps)[&0x60].clone();
    assert_eq!(input.safety, SafetyClass::Normal);
    for level in [verified(), SupportLevel::LenovoFamily, SupportLevel::GenericMccs] {
        let d = evaluate(Some(&input), &caps, 0x60, 0x0F, None, &level, false);
        assert_eq!(d.verdict, Verdict::NeedsConfirmation, "{level:?}");
        assert_eq!(evaluate(Some(&input), &caps, 0x60, 0x0F, None, &level, true).verdict, Verdict::Allow, "{level:?}");
    }
}

#[test]
fn enum_with_no_value_list_refused_below_verified() {
    let caps = mccs::parse("(vcp(10 14 60(11 0F)))").unwrap();
    let defs = resolved(&caps);
    for level in [SupportLevel::LenovoFamily, SupportLevel::GenericMccs] {
        let d = evaluate(defs.get(&0x14), &caps, 0x14, 0x0B, None, &level, false);
        assert_eq!((d.verdict, d.message.as_str()), (Verdict::Block, "The monitor lists no values for Colour mode."),
            "{level:?}");
    }
    let d = evaluate(defs.get(&0x14), &caps, 0x14, 0x0B, None, &verified(), false);
    assert_eq!((d.verdict, d.value), (Verdict::Allow, 0x0B));
}
