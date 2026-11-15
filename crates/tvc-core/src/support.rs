//! How far the app trusts a monitor: a verified profile, a Lenovo relative, a generic MCCS monitor, or read-only.
//! `safety::evaluate` takes the level; the rules that pick it arrive with multi-display support.

use crate::identity::MonitorIdentity;
use crate::mccs::MonitorCapabilities;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SupportLevel {
    /// An exact profile match, e.g. `profile: "p32p20"`.
    Verified { profile: String },
    LenovoFamily,
    GenericMccs,
    ReadOnly,
}

/// The level for a display, from its identity and (once read) its capabilities:
/// - an exact EDID match with a built-in profile gives `Verified`;
/// - PNP `LEN` whose advertised values for a code are a subset of a known Lenovo profile's labels gives `LenovoFamily`;
/// - otherwise `GenericMccs`;
/// - `caps_failed` (the live read failed and nothing is cached) gives `ReadOnly`.
///
/// Build the matching profile with `ProfileStore::for_level`. Implemented in w3-support; until then everything is
/// Verified as the P32p-20.
pub fn resolve(identity: &MonitorIdentity, caps: Option<&MonitorCapabilities>, caps_failed: bool) -> SupportLevel {
    let _ = (identity, caps, caps_failed);
    SupportLevel::Verified { profile: "p32p20".into() }
}

/// Whether the engine may synthesise capabilities from the profile before the monitor answers (display only).
pub fn may_synthesize(level: &SupportLevel) -> bool {
    matches!(level, SupportLevel::Verified { .. })
}
