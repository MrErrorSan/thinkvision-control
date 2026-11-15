//! Built-in panel brightness through the OS, as a `MonitorLink` that offers only 0x10: the `\\.\LCD` brightness
//! IOCTLs on Windows, `/sys/class/backlight` on Linux, nothing elsewhere.
//!
//! A display is this backend's when its target starts with [`TARGET_PREFIX`]; the hub (or controller) then gives its
//! engine `EngineConfig.level = Some(Verified { profile: PROFILE })` instead of resolving one from the capabilities.
//! The link reads and writes brightness in percent (`max` is always 100) and answers every other code `Unsupported`.
//!
//! The level-mapping, scaling and sysfs-selection rules here are pure and compile on every OS.

#[cfg(any(windows, target_os = "linux"))]
mod link;
#[cfg(target_os = "linux")]
mod sysfs;
#[cfg(windows)]
mod lcd;

use tvc_core::identity::{BackendKind, DisplayInfo, MonitorIdentity};
use tvc_core::link::{DisplayKind, LinkFailure, MonitorLink};

use crate::edid;
use crate::linux::{connector_kind, connector_order};

/// Every backlight target starts with this, so `discovery()` can route `open` here.
pub const TARGET_PREFIX: &str = "backlight:";

/// The profile the engine runs a built-in panel at (`SupportLevel::Verified`).
pub const PROFILE: &str = "builtin-backlight";

/// The capability string a backlight link reports: brightness only.
pub const CAPABILITIES: &str = "(prot(monitor)type(LCD)vcp(10))";

/// The name a panel gets when its EDID gives none.
pub const DEFAULT_NAME: &str = "Built-in display";

/// The built-in panels this OS lets the app dim, with `backend: BackendKind::Backlight` and a [`TARGET_PREFIX`]
/// target. At most one.
pub fn displays() -> Vec<DisplayInfo> {
    #[cfg(target_os = "linux")]
    {
        sysfs::displays()
    }
    #[cfg(windows)]
    {
        lcd::displays()
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Vec::new()
    }
}

/// A link for a target from [`displays`].
pub fn open(target: &str) -> Result<Box<dyn MonitorLink>, LinkFailure> {
    let key = target.strip_prefix(TARGET_PREFIX).ok_or(LinkFailure::NotFound)?;
    #[cfg(target_os = "linux")]
    {
        sysfs::open(key)
    }
    #[cfg(windows)]
    {
        lcd::open(key)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = key;
        Err(LinkFailure::NotFound)
    }
}

/// The entry for a panel reached by backend `key`: name and EDID id from `edid` when it carries them, `connector`
/// as the port ("eDP-1", "internal"). No serial, so the id stays `{EDID}-P{connector}` whether or not the EDID
/// gives one.
pub fn display_info(key: &str, edid: &[u8], connector: &str) -> DisplayInfo {
    DisplayInfo {
        target: format!("{TARGET_PREFIX}{key}"),
        identity: MonitorIdentity {
            edid_id: edid::id(edid).unwrap_or_default(),
            serial: None,
            connector: Some(connector.to_owned()),
        },
        name: edid::display_name(edid).unwrap_or_else(|| DEFAULT_NAME.to_owned()),
        kind: DisplayKind::BuiltIn,
        backend: BackendKind::Backlight,
    }
}

/// `raw` of `max` as a rounded percent; 0 when `max` is 0.
pub fn to_percent(raw: u32, max: u32) -> u16 {
    if max == 0 {
        return 0;
    }
    ((u64::from(raw.min(max)) * 100 + u64::from(max) / 2) / u64::from(max)) as u16
}

/// `percent` (clamped to 100) as the nearest raw value of `max`.
pub fn from_percent(percent: u16, max: u32) -> u32 {
    ((u64::from(percent.min(100)) * u64::from(max) + 50) / 100) as u32
}

/// The supported level closest to `percent`, the lower one on a tie. None when `levels` is empty.
pub fn nearest_level(levels: &[u8], percent: u16) -> Option<u8> {
    levels.iter().copied().min_by_key(|&l| (u16::from(l).abs_diff(percent), l))
}

/// The `/sys/class/backlight` device to use, from `(name, type)` pairs: `firmware` over `platform` over `raw`, then
/// the first name in natural order. Devices of any other type are ignored.
pub fn pick_backlight<N: AsRef<str>, T: AsRef<str>>(devices: &[(N, T)]) -> Option<&str> {
    let rank = |kind: &str| ["firmware", "platform", "raw"].iter().position(|k| *k == kind.trim());
    devices
        .iter()
        .filter_map(|(name, kind)| rank(kind.as_ref()).map(|r| (r, name.as_ref())))
        .min_by(|(ra, a), (rb, b)| ra.cmp(rb).then_with(|| connector_order(a, b)))
        .map(|(_, name)| name)
}

/// The DRM connector (`card0-eDP-1`) that is the panel, from `(name, connected)` pairs: the backlight's own `device`
/// when that is a built-in connector, else the first built-in one that is connected, else the first built-in one.
pub fn panel_connector<'a, N: AsRef<str>>(device: Option<&str>, connectors: &'a [(N, bool)]) -> Option<&'a str> {
    let mut builtin: Vec<(&str, bool)> = connectors
        .iter()
        .map(|(name, connected)| (name.as_ref(), *connected))
        .filter(|(name, _)| connector_kind(name, "") == DisplayKind::BuiltIn)
        .collect();
    if let Some(own) = builtin.iter().find(|(name, _)| Some(*name) == device) {
        return Some(own.0);
    }
    builtin.sort_by(|(a, ca), (b, cb)| cb.cmp(ca).then_with(|| connector_order(a, b)));
    builtin.first().map(|(name, _)| *name)
}

/// `card0-eDP-1` → `eDP-1`.
pub fn connector_port(connector: &str) -> &str {
    connector.split_once('-').map_or(connector, |(_, rest)| rest)
}
