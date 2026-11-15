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

/// `card0-eDP-1` → `eDP-1`; a name without the `cardN-` prefix (`eDP-1`) is returned as it is.
pub fn connector_port(connector: &str) -> &str {
    crate::linux::connector_port(connector).unwrap_or(connector)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edid::testing::p32p20;

    #[test]
    fn percent_rounds_to_nearest_and_clamps() {
        assert_eq!(to_percent(0, 255), 0);
        assert_eq!(to_percent(255, 255), 100);
        assert_eq!(to_percent(1, 3), 33);
        assert_eq!(to_percent(2, 3), 67);
        assert_eq!(to_percent(1, 200), 1); // 0.5 rounds up
        assert_eq!(to_percent(500, 100), 100); // above max clamps
        assert_eq!(to_percent(7, 0), 0);
        assert_eq!(to_percent(u32::MAX, u32::MAX), 100); // no overflow
    }

    #[test]
    fn raw_from_percent_rounds_to_nearest_and_clamps() {
        assert_eq!(from_percent(0, 255), 0);
        assert_eq!(from_percent(100, 255), 255);
        assert_eq!(from_percent(50, 255), 128); // 127.5 rounds up
        assert_eq!(from_percent(33, 3), 1);
        assert_eq!(from_percent(150, 255), 255); // above 100 clamps
        assert_eq!(from_percent(1, 1), 0);
        assert_eq!(from_percent(50, 1), 1);
        assert_eq!(from_percent(100, u32::MAX), u32::MAX); // no overflow
        assert_eq!(from_percent(50, 0), 0);
    }

    #[test]
    fn percent_round_trips_at_common_maxima() {
        for max in [100, 255, 937, 19393, 120000] {
            for percent in 0..=100 {
                assert_eq!(to_percent(from_percent(percent, max), max), percent, "max {max}, {percent}%");
            }
        }
    }

    #[test]
    fn nearest_level_takes_the_lower_on_a_tie() {
        let levels = [0, 10, 20, 50, 100];
        assert_eq!(nearest_level(&levels, 15), Some(10));
        assert_eq!(nearest_level(&levels, 16), Some(20));
        assert_eq!(nearest_level(&levels, 35), Some(20));
        assert_eq!(nearest_level(&levels, 36), Some(50));
        assert_eq!(nearest_level(&levels, 100), Some(100));
        assert_eq!(nearest_level(&levels, 300), Some(100));
        assert_eq!(nearest_level(&[60, 40], 50), Some(40)); // the list's order doesn't matter
        assert_eq!(nearest_level(&[], 50), None);
    }

    #[test]
    fn backlight_prefers_firmware_then_platform_then_raw() {
        let pick = |devices: &[(&str, &str)]| pick_backlight(devices).map(str::to_owned);
        assert_eq!(pick(&[("intel_backlight", "raw\n"), ("acpi_video0", "firmware\n")]).as_deref(), Some("acpi_video0"));
        assert_eq!(pick(&[("intel_backlight", "raw"), ("thinkpad_screen", "platform")]).as_deref(), Some("thinkpad_screen"));
        assert_eq!(pick(&[("amdgpu_bl0", "raw\n")]).as_deref(), Some("amdgpu_bl0"));
        // Unknown types are ignored.
        assert_eq!(pick(&[("ddcci0", "monitor"), ("amdgpu_bl1", "raw")]).as_deref(), Some("amdgpu_bl1"));
        assert_eq!(pick(&[("ddcci0", "monitor"), ("x", "")]), None);
        assert_eq!(pick(&[]), None);
        // Same type: natural order, so 2 comes before 10.
        assert_eq!(pick(&[("acpi_video10", "firmware"), ("acpi_video2", "firmware")]).as_deref(), Some("acpi_video2"));
    }

    #[test]
    fn panel_is_the_backlights_own_connector_then_a_connected_built_in_one() {
        let connectors = [
            ("card0-HDMI-A-1", true),
            ("card0-eDP-2", false),
            ("card0-eDP-1", false),
            ("card1-eDP-10", true),
            ("card1-eDP-9", true),
        ];
        assert_eq!(panel_connector(Some("card0-eDP-2"), &connectors), Some("card0-eDP-2"));
        // The backlight's own device counts only when it is a listed built-in connector.
        assert_eq!(panel_connector(Some("card0-HDMI-A-1"), &connectors), Some("card1-eDP-9"));
        assert_eq!(panel_connector(Some("card3-eDP-1"), &connectors), Some("card1-eDP-9"));
        assert_eq!(panel_connector(None, &connectors), Some("card1-eDP-9"));
        // None connected: the first built-in one in natural order.
        let unplugged = [("card1-eDP-1", false), ("card0-eDP-2", false), ("card0-eDP-1", false)];
        assert_eq!(panel_connector(None, &unplugged), Some("card0-eDP-1"));
        assert_eq!(panel_connector(None, &[("card0-DSI-1", false)]), Some("card0-DSI-1"));
        assert_eq!(panel_connector(None, &[("card0-DP-1", true), ("card0-HDMI-A-1", true)]), None);
        assert_eq!(panel_connector::<&str>(None, &[]), None);
    }

    #[test]
    fn connector_port_drops_only_the_card_prefix() {
        assert_eq!(connector_port("card0-eDP-1"), "eDP-1");
        assert_eq!(connector_port("card1-HDMI-A-2"), "HDMI-A-2");
        assert_eq!(connector_port("eDP-1"), "eDP-1");
        assert_eq!(connector_port("internal"), "internal");
    }

    #[test]
    fn display_info_defaults_without_an_edid() {
        let info = display_info("intel_backlight", &[], "eDP-1");
        assert_eq!(info.target, "backlight:intel_backlight");
        let identity = MonitorIdentity { edid_id: String::new(), serial: None, connector: Some("eDP-1".into()) };
        assert_eq!(info.identity, identity);
        assert_eq!(info.name, DEFAULT_NAME);
        assert_eq!(info.kind, DisplayKind::BuiltIn);
        assert_eq!(info.backend, BackendKind::Backlight);
    }

    #[test]
    fn display_info_takes_id_and_name_from_the_edid_but_never_a_serial() {
        let info = display_info("lcd", &p32p20(1234, Some("Panel")), "internal");
        assert_eq!(info.target, "backlight:lcd");
        assert_eq!(info.identity.edid_id, "LEN62A2");
        assert_eq!(info.identity.serial, None);
        assert_eq!(info.identity.connector.as_deref(), Some("internal"));
        assert_eq!(info.name, "Panel");
        assert_eq!(display_info("lcd", &p32p20(0, None), "internal").name, DEFAULT_NAME);
    }

    #[test]
    fn open_needs_the_prefix() {
        assert!(matches!(open("x"), Err(LinkFailure::NotFound)));
        assert!(matches!(open(""), Err(LinkFailure::NotFound)));
    }
}
