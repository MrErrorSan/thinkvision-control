//! How far the app trusts a monitor: a verified profile, a Lenovo relative, a generic MCCS monitor, or read-only.
//! `safety::evaluate` takes the level; the rules that pick it arrive with multi-display support.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SupportLevel {
    /// An exact profile match, e.g. `profile: "p32p20"`.
    Verified { profile: String },
    LenovoFamily,
    GenericMccs,
    ReadOnly,
}
