//! Pure write rules. A hard floor no profile can lift: OSD enable (0xCA) and power (0xD6) are never written, a
//! read-only monitor takes no writes, and below Verified factory reset (0x04), unnamed vendor codes (E0–FF) and
//! Enum codes with no advertised values are refused. Then: the profile's safety class, only values the monitor
//! advertises, Range clamping, and confirmation for guarded actions and input (0x60).

use crate::hex;
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
    let name = || def.map_or_else(|| hex(code), |d| d.name.clone());
    if matches!(code, 0xCA | 0xD6) {
        return block(format!("{} is never written.", name()));
    }
    if *level == SupportLevel::ReadOnly {
        return block("This monitor is read-only.".into());
    }
    let verified = matches!(level, SupportLevel::Verified { .. });
    if !verified {
        if code == 0x04 {
            return block(format!("{} is only allowed on verified monitors.", name()));
        }
        let named = def.is_some_and(|d| d.safety != SafetyClass::ReadOnlyUntilNamed) || caps.names.contains_key(&code);
        if code >= 0xE0 && !named {
            return block("Unknown setting — identify it with the finder first.".into());
        }
    }
    let Some(def) = def.filter(|_| caps.supports(code)) else {
        return block(format!("This monitor does not support {}.", hex(code)));
    };
    match def.safety {
        SafetyClass::NeverWrite => return block(format!("{} is never written.", def.name)),
        SafetyClass::ReadOnlyUntilNamed => return block("Unknown setting — identify it with the finder first.".into()),
        _ => {}
    }
    let value = match def.kind {
        ControlKind::Action => 1,
        ControlKind::Range => value.min(max.unwrap_or(100)),
        ControlKind::Enum if !verified && caps.vcp.get(&code) == Some(&None) => {
            return block(format!("The monitor lists no values for {}.", def.name));
        }
        ControlKind::Enum if !caps.allows_value(code, value) => {
            return block(format!("0x{value:02X} is not a valid value for {}.", def.name));
        }
        ControlKind::Enum => value,
    };
    if (def.safety == SafetyClass::ConfirmRequired || code == 0x60) && !confirmed {
        let message = def.confirm_message.clone().unwrap_or_else(|| format!("Change {}?", def.name));
        return Decision { verdict: Verdict::NeedsConfirmation, value, message };
    }
    Decision { verdict: Verdict::Allow, value, message: String::new() }
}
