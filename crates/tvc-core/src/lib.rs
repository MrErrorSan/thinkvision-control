//! ThinkVision Control core: monitor model, safety rules, the DDC engine, settings. No OS calls, no UI.
#![forbid(unsafe_code)]

pub mod cli;
pub mod clock;
pub mod crash;
pub mod engine;
#[cfg(any(test, feature = "fake"))]
pub mod fake;
pub mod finder;
pub mod hotkey;
pub mod link;
pub mod log;
pub mod mccs;
pub mod profile;
pub mod result;
pub mod safety;
pub mod schedule;
pub mod solar;
pub mod store;

#[cfg(test)]
mod tests;

/// `0xNN` with upper-case hex, the format every file and message uses.
pub(crate) fn hex(code: u8) -> String {
    format!("0x{code:02X}")
}

/// Accepts `0xNN`, `0XNN` or bare hex.
pub(crate) fn parse_hex(s: &str) -> Option<u8> {
    let digits = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    u8::from_str_radix(digits, 16).ok()
}
