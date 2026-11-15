//! Pure write rules. A hard floor no profile can lift: OSD enable (0xCA) and power (0xD6) are never written, a
//! read-only monitor takes no writes, and below Verified factory reset (0x04), unnamed vendor codes (E0–FF) and
//! Enum codes with no advertised values are refused. Then: the profile's safety class, only values the monitor
//! advertises, Range clamping, and confirmation for guarded actions and input (0x60).

use crate::hex;
use crate::i18n::{tr, trf};
use crate::mccs::MonitorCapabilities;
use crate::profile::{ControlDefinition, ControlKind, SafetyClass};
use crate::support::SupportLevel;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    NeedsConfirmation,
    Block,
}

/// `value` is what to write (clamped, or forced to 1 for actions); `message` is empty on Allow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    pub verdict: Verdict,
    pub value: u16,
    pub message: String,
}

/// `max` is the Range maximum the monitor reported, if known (100 otherwise). `level` is how far the monitor is
/// trusted.
pub fn evaluate(def: Option<&ControlDefinition>, caps: &MonitorCapabilities, code: u8, value: u16, max: Option<u16>,
    level: &SupportLevel, confirmed: bool) -> Decision {
    let block = |message: String| Decision { verdict: Verdict::Block, value, message };
    let name = || def.map_or_else(|| hex(code), |d| tr(&d.name));
    if matches!(code, 0xCA | 0xD6) {
        return block(trf("{0} is never written.", &[&name()]));
    }
    if *level == SupportLevel::ReadOnly {
        return block(tr("This monitor is read-only."));
    }
    let verified = matches!(level, SupportLevel::Verified { .. });
    if !verified {
        if code == 0x04 {
            return block(trf("{0} is only allowed on verified monitors.", &[&name()]));
        }
        let named = def.is_some_and(|d| d.safety != SafetyClass::ReadOnlyUntilNamed);
        if code >= 0xE0 && !named {
            return block(tr("Unknown setting — identify it with the finder first."));
        }
    }
    let Some(def) = def.filter(|_| caps.supports(code)) else {
        return block(trf("This monitor does not support {0}.", &[&hex(code)]));
    };
    match def.safety {
        SafetyClass::NeverWrite => return block(trf("{0} is never written.", &[&tr(&def.name)])),
        SafetyClass::ReadOnlyUntilNamed => return block(tr("Unknown setting — identify it with the finder first.")),
        _ => {}
    }
    let value = match def.kind {
        ControlKind::Action => 1,
        ControlKind::Range => value.min(max.unwrap_or(100)),
        ControlKind::Enum if !verified && caps.vcp.get(&code) == Some(&None) => {
            return block(trf("The monitor lists no values for {0}.", &[&tr(&def.name)]));
        }
        ControlKind::Enum if !caps.allows_value(code, value) => {
            return block(trf("{0} is not a valid value for {1}.", &[&format!("0x{value:02X}"), &tr(&def.name)]));
        }
        ControlKind::Enum => value,
    };
    if (def.safety == SafetyClass::ConfirmRequired || code == 0x60) && !confirmed {
        let message = def.confirm_message.as_deref().map_or_else(|| trf("Change {0}?", &[&tr(&def.name)]), tr);
        return Decision { verdict: Verdict::NeedsConfirmation, value, message };
    }
    Decision { verdict: Verdict::Allow, value, message: String::new() }
}
