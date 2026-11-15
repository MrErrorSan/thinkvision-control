//! Built-in panel brightness through the OS, as a `MonitorLink` that offers only 0x10: the `\\.\LCD` brightness
//! IOCTLs on Windows, `/sys/class/backlight` on Linux, nothing elsewhere.
//!
//! A display is this backend's when its target starts with [`TARGET_PREFIX`]; the hub (or controller) then gives its
//! engine `EngineConfig.level = Some(Verified { profile: "builtin-backlight" })` (tvc-core's built-in panel profile)
//! instead of resolving one from the capabilities.
//! The link reads and writes brightness in percent (`max` is always 100) and answers every other code `Unsupported`.
//!
//! The level-mapping, scaling and sysfs-selection rules here are pure and compile on every OS.

#[cfg(any(windows, target_os = "linux"))]
mod link;
#[cfg(target_os = "linux")]
mod sysfs;
#[cfg(windows)]
mod lcd;

use std::cmp::Ordering;

use tvc_core::identity::{BackendKind, DisplayInfo, MonitorIdentity};
use tvc_core::link::{DisplayKind, LinkFailure, MonitorLink};

use crate::edid;
use crate::linux::{connector_kind, connector_order};

/// Every backlight target starts with this, so `discovery()` can route `open` here.
pub const TARGET_PREFIX: &str = "backlight:";

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

/// `percent` (clamped to 100) as a raw value of `max`: the nearest one for `Equal`; for a move up (`Greater`) the
/// first at or above `percent`, for a move down (`Less`) the last at or below it, so a step never snaps back.
pub fn from_percent(percent: u16, max: u32, toward: Ordering) -> u32 {
    let bias = match toward {
        Ordering::Greater => 99,
        Ordering::Equal => 50,
        Ordering::Less => 0,
    };
    ((u64::from(percent.min(100)) * u64::from(max) + bias) / 100) as u32
}

/// The supported level for `percent`: the closest for `Equal` (the lower one on a tie); for a move up
/// (`Greater`) the lowest at or above `percent`, for a move down (`Less`) the highest at or below it, falling back
/// to the closest when there is none that far. None when `levels` is empty.
pub fn nearest_level(levels: &[u8], percent: u16, toward: Ordering) -> Option<u8> {
    let past = match toward {
        Ordering::Greater => levels.iter().copied().filter(|&l| u16::from(l) >= percent).min(),
        Ordering::Less => levels.iter().copied().filter(|&l| u16::from(l) <= percent).max(),
        Ordering::Equal => None,
    };
    past.or_else(|| levels.iter().copied().min_by_key(|&l| (u16::from(l).abs_diff(percent), l)))
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

/// A backlight that the kernel ddcci driver (`ddcci-driver-linux`) registers for an external monitor: named
/// `ddcci…`, or with its canonical `device` path under an i2c adapter. It is not a built-in panel.
pub fn is_ddcci_backlight(name: &str, device_path: &str) -> bool {
    name.starts_with("ddcci") || device_path.contains("/i2c-")
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
        let near = |p, max| from_percent(p, max, Ordering::Equal);
        assert_eq!(near(0, 255), 0);
        assert_eq!(near(100, 255), 255);
        assert_eq!(near(50, 255), 128); // 127.5 rounds up
        assert_eq!(near(33, 3), 1);
        assert_eq!(near(150, 255), 255); // above 100 clamps
        assert_eq!(near(1, 1), 0);
        assert_eq!(near(50, 1), 1);
        assert_eq!(near(100, u32::MAX), u32::MAX); // no overflow
        assert_eq!(near(50, 0), 0);
    }

    #[test]
    fn raw_from_percent_rounds_in_a_steps_direction() {
        // max 7: 55% is raw 3.85 and 45% raw 3.15; up gives 4 for both, down 3.
        assert_eq!(from_percent(55, 7, Ordering::Greater), 4);
        assert_eq!(from_percent(45, 7, Ordering::Greater), 4);
        assert_eq!(from_percent(55, 7, Ordering::Less), 3);
        assert_eq!(from_percent(45, 7, Ordering::Less), 3);
        // Exact values stay put, and the ends clamp.
        assert_eq!(from_percent(100, 7, Ordering::Greater), 7);
        assert_eq!(from_percent(0, 7, Ordering::Less), 0);
        assert_eq!(from_percent(150, 7, Ordering::Greater), 7);
        assert_eq!(from_percent(100, u32::MAX, Ordering::Greater), u32::MAX); // no overflow
        // A step from 57% (raw 4) up by 5 reaches raw 5 instead of snapping back to 4.
        assert_eq!(to_percent(from_percent(62, 7, Ordering::Greater), 7), 71);
        assert_eq!(to_percent(from_percent(62, 7, Ordering::Equal), 7), 57);
        for max in [100, 255, 937, 120000] {
            for percent in 0..=100 {
                for toward in [Ordering::Less, Ordering::Greater] {
                    assert_eq!(to_percent(from_percent(percent, max, toward), max), percent, "max {max}, {percent}%");
                }
            }
        }
    }

    #[test]
    fn percent_round_trips_at_common_maxima() {
        for max in [100, 255, 937, 19393, 120000] {
            for percent in 0..=100 {
                assert_eq!(to_percent(from_percent(percent, max, Ordering::Equal), max), percent, "max {max}, {percent}%");
            }
        }
    }

    #[test]
    fn nearest_level_takes_the_lower_on_a_tie() {
        let levels = [0, 10, 20, 50, 100];
        let near = |levels: &[u8], p| nearest_level(levels, p, Ordering::Equal);
        assert_eq!(near(&levels, 15), Some(10));
        assert_eq!(near(&levels, 16), Some(20));
        assert_eq!(near(&levels, 35), Some(20));
        assert_eq!(near(&levels, 36), Some(50));
        assert_eq!(near(&levels, 100), Some(100));
        assert_eq!(near(&levels, 300), Some(100));
        assert_eq!(near(&[60, 40], 50), Some(40)); // the list's order doesn't matter
        assert_eq!(near(&[], 50), None);
    }

    #[test]
    fn nearest_level_in_a_steps_direction_never_snaps_back() {
        let levels: Vec<u8> = (0..=10).map(|n| n * 10).collect();
        // At 50, +5 asks 55: up gives 60 (nearest would give 50 again); -5 asks 45: down gives 40.
        assert_eq!(nearest_level(&levels, 55, Ordering::Greater), Some(60));
        assert_eq!(nearest_level(&levels, 45, Ordering::Less), Some(40));
        assert_eq!(nearest_level(&levels, 55, Ordering::Equal), Some(50));
        // A level hit exactly is kept; past the ends, the end level.
        assert_eq!(nearest_level(&levels, 60, Ordering::Greater), Some(60));
        assert_eq!(nearest_level(&levels, 300, Ordering::Greater), Some(100));
        assert_eq!(nearest_level(&[20, 50], 10, Ordering::Less), Some(20));
        assert_eq!(nearest_level(&[60, 40], 45, Ordering::Greater), Some(60)); // order doesn't matter
        assert_eq!(nearest_level(&[], 50, Ordering::Greater), None);
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
    fn ddcci_backlights_are_external_monitors() {
        assert!(is_ddcci_backlight("ddcci5", ""));
        assert!(is_ddcci_backlight("x", "/sys/devices/pci0000:00/0000:00:02.0/i2c-5/5-0037"));
        assert!(!is_ddcci_backlight("intel_backlight", "/sys/devices/pci0000:00/0000:00:02.0/drm/card1/card1-eDP-1"));
        assert!(!is_ddcci_backlight("acpi_video0", "/sys/devices/pci0000:00/0000:00:02.0"));
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
