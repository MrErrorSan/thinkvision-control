//! The MCCS capability parser on real-world strings (w1-parser).

use crate::fake::P32P20_CAPABILITIES;
use crate::mccs::{self, input_name, MonitorCapabilities};

use super::P40W20_CAPABILITIES;

fn parse(s: &str) -> MonitorCapabilities {
    mccs::parse(s).unwrap()
}

fn codes(caps: &MonitorCapabilities) -> Vec<u8> {
    caps.vcp.keys().copied().collect()
}

#[test]
fn p40w20_parses() {
    let caps = parse(P40W20_CAPABILITIES);

    assert_eq!(caps.model, "P40w-20");
    assert_eq!(caps.vcp[&0xF7], Some(vec![0x04, 0x07, 0x08, 0x12, 0x14, 0x15, 0x17]));
    assert!(caps.flags[&0xF7].nested);
    assert_eq!(caps.mccs_ver, "2.2");
    assert_eq!(caps.cmds, [0x01, 0x02, 0x03, 0x07, 0x0C, 0xE3, 0xF3]);
    assert!(caps.warnings.is_empty(), "{:?}", caps.warnings);
}

#[test]
fn duplicate_60_union_and_intersection() {
    let caps = parse("(vcp(60(0F 11 FF) 60(0F 12)))");

    assert_eq!(caps.vcp[&0x60], Some(vec![0x0F, 0x11, 0x12]));
    assert!(!caps.allows_value(0x60, 0x11));
    assert!(caps.allows_value(0x60, 0x0F));
    assert!(!caps.allows_value(0x60, 0xFF));
    assert!(caps.flags[&0x60].duplicate);
}

#[test]
fn ff_never_an_input() {
    assert_eq!(parse("(vcp(60(0F FF)))").vcp[&0x60], Some(vec![0x0F]));
}

#[test]
fn single_digit_tokens() {
    assert_eq!(parse("(vcp(60(1 3 F)))").vcp[&0x60], Some(vec![0x01, 0x03, 0x0F]));
}

#[test]
fn three_digit_token_skips_entry() {
    let caps = parse("(vcp(10 60(011)))");
    assert_eq!(codes(&caps), [0x10]);
    assert_eq!(caps.warnings.len(), 1);
}

#[test]
fn nul_is_a_separator() {
    assert_eq!(codes(&parse("(vcp(10 12\0 14))")), [0x10, 0x12, 0x14]);
}

#[test]
fn bad_model_is_empty() {
    let caps = parse("(prot(monitor)vcp(10 12)model(P40w");
    assert_eq!(caps.model, "");
    assert_eq!(caps.warnings.len(), 1);
}

#[test]
fn unbalanced_earlier_section() {
    assert!(parse("(prot(monitor)cmds(01 02 vcp(10 12))").supports(0x10));
}

#[test]
fn vcp_with_space() {
    assert_eq!(codes(&parse("(prot(monitor)vcp (10 12))")), [0x10, 0x12]);
}

#[test]
fn two_vcp_sections_merge() {
    let caps = parse("(vcp(10 12)vcp(14(01 02)))");
    assert!(caps.supports(0x10));
    assert_eq!(caps.vcp[&0x14], Some(vec![0x01, 0x02]));
}

#[test]
fn positions_are_absolute() {
    let s = "(prot(monitor)type(LCD)vcp(10 1G))";
    let caps = parse(s);
    let at = format!("position {}", s.find("1G").unwrap());
    assert!(caps.warnings.iter().any(|w| w.contains(&at)), "{:?}", caps.warnings);
}

#[test]
fn vcpname_parsed() {
    assert_eq!(parse("(vcp(E0)vcpname(E0(Over Drive)))").names[&0xE0], "Over Drive");
}

#[test]
fn input_name_table() {
    assert_eq!(input_name(0x11), Some("HDMI 1"));
    assert_eq!(input_name(0x0F), Some("DisplayPort 1"));
    assert_eq!(input_name(0x31), None);
}

#[test]
fn bare_code_has_no_list() {
    let caps = parse("(vcp(60))");
    assert!(!caps.has_value_list(0x60));
    assert!(caps.allows_value(0x60, 5));
}

#[test]
fn p32p20_unchanged() {
    let caps = parse(P32P20_CAPABILITIES);

    assert!(caps.warnings.is_empty(), "{:?}", caps.warnings);
    assert!(caps.flags.is_empty());
    assert!(caps.writable.is_empty());
    assert_eq!(caps.vcp.len(), 32);
    assert_eq!(caps.vcp[&0x60], Some(vec![0x11, 0x0F, 0x31]));
    assert_eq!(caps.vcp[&0x14], Some(vec![0x01, 0x05, 0x06, 0x08, 0x0B, 0x0E, 0x0F, 0x12]));
    assert_eq!(caps.vcp[&0x10], None);
    assert_eq!(caps.vcp[&0xFA], Some(vec![0x10, 0x12, 0xEA, 0x14]));
    assert_eq!(caps.firmware().as_deref(), Some("LB1.2"));
    assert_eq!((caps.mccs_ver.as_str(), caps.cmds.len()), ("2.2", 7));
}

#[test]
fn regression_core_02_duplicate_60_keeps_every_input() {
    let caps = parse(P40W20_CAPABILITIES);
    assert_eq!(caps.vcp[&0x60], Some(vec![0x0F, 0x11, 0x12, 0x1B, 0x31]));
    assert!(caps.allows_value(0x60, 0x1B));
    assert!(!caps.allows_value(0x60, 0xFF));

    let reversed = parse("(vcp(60(0F 11) 60(0F 11 FF)))");
    assert_eq!(reversed.vcp[&0x60], Some(vec![0x0F, 0x11]));
}
