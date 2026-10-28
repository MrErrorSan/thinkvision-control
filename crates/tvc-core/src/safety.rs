//! Pure write rules: never write OSD-enable (0xCA), power (0xD6) or unnamed vendor codes, only values the monitor
//! advertises, clamp ranges, and confirm guarded actions. Rule order is

use crate::hex;
use crate::mccs::MonitorCapabilities;
use crate::profile::{ControlDefinition, ControlKind, SafetyClass};

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

/// `max` is the Range maximum the monitor reported, if known (100 otherwise).
pub fn evaluate(def: Option<&ControlDefinition>, caps: &MonitorCapabilities, code: u8, value: u16, max: Option<u16>,
    confirmed: bool) -> Decision {
    let block = |message: String| Decision { verdict: Verdict::Block, value, message };
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
        ControlKind::Enum if !caps.allows_value(code, value) => {
            return block(format!("0x{value:02X} is not a valid value for {}.", def.name));
        }
        ControlKind::Enum => value,
    };
    if def.safety == SafetyClass::ConfirmRequired && !confirmed {
        let message = def.confirm_message.clone().unwrap_or_else(|| format!("Change {}?", def.name));
        return Decision { verdict: Verdict::NeedsConfirmation, value, message };
    }
    Decision { verdict: Verdict::Allow, value, message: String::new() }
}
