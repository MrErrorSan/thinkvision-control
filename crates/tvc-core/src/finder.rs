//! Read-only discovery of what Lenovo's undocumented vendor codes control.

use std::collections::BTreeMap;

use crate::profile::{ControlDefinition, SafetyClass, Section};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodeChange {
    pub code: u8,
    pub before: u16,
    pub after: u16,
}

/// The codes a snapshot reads: Hidden and not NeverWrite (so never 0xCA or 0xD6).
pub fn snapshot_codes(controls: &BTreeMap<u8, ControlDefinition>) -> Vec<u8> {
    controls.values()
        .filter(|d| d.section == Section::Hidden && d.safety != SafetyClass::NeverWrite)
        .map(|d| d.code)
        .collect()
}

/// Codes present in both snapshots whose value changed, by code.
pub fn diff(before: &BTreeMap<u8, u16>, after: &BTreeMap<u8, u16>) -> Vec<CodeChange> {
    before.iter()
        .filter_map(|(&code, &b)| after.get(&code).filter(|&&a| a != b).map(|&a| CodeChange { code, before: b, after: a }))
        .collect()
}
