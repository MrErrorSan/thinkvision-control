//! How far the app trusts a monitor: a verified profile, a Lenovo relative, a generic MCCS monitor, or read-only.
//! `safety::evaluate` takes the level.

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
/// - an exact EDID match with a built-in profile gives `Verified`, whatever the capabilities;
/// - PNP `LEN` where at least one code the P32p-20 labels is advertised with values inside those labels gives
///   `LenovoFamily` (see `profile::lenovo_family_codes`);
/// - otherwise `GenericMccs`;
/// - `caps_failed` with no `caps` (the live read failed and nothing is cached) gives `ReadOnly`.
///
/// Before the first read (`caps` None, `caps_failed` false) it is `Verified` for an exact match, else
/// `GenericMccs`. Call it again once the live capabilities arrive or the read fails: a `LEN` monitor can then
/// become `LenovoFamily`, and any monitor without an exact match `ReadOnly`. Build the matching profile with
/// `ProfileStore::for_level`.
pub fn resolve(identity: &MonitorIdentity, caps: Option<&MonitorCapabilities>, caps_failed: bool) -> SupportLevel {
    if let Some(profile) = crate::profile::verified_profile(&identity.edid_id) {
        return SupportLevel::Verified { profile: profile.into() };
    }
    match caps {
        None if caps_failed => SupportLevel::ReadOnly,
        Some(caps) if identity.edid_id.starts_with("LEN") && !crate::profile::lenovo_family_codes(caps).is_empty() => {
            SupportLevel::LenovoFamily
        }
        _ => SupportLevel::GenericMccs,
    }
}

/// Whether the engine may synthesise capabilities from the profile before the monitor answers (display only).
pub fn may_synthesize(level: &SupportLevel) -> bool {
    matches!(level, SupportLevel::Verified { .. })
}
