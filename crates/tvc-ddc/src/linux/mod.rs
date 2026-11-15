//! Linux i2c-dev backend. The bus-selection rules here are pure and compile on every OS; the link itself is Linux-only.

#[cfg(target_os = "linux")]
mod discovery;
#[cfg(target_os = "linux")]
mod i2c;
#[cfg(target_os = "linux")]
pub(crate) use discovery::LinuxDiscovery;
#[cfg(target_os = "linux")]
pub(crate) use i2c::I2cLink;

use std::cmp::Ordering;

use tvc_core::identity::{BackendKind, DisplayInfo, MonitorIdentity};
use tvc_core::link::{is_virtual_vendor, DisplayKind};

use crate::edid;

/// Adapter-name prefixes (`/sys/bus/i2c/devices/i2c-N/name`) that are never a display's DDC lines, as ddcutil matches
/// them. Probing these would write an address byte to unrelated chips (SPD EEPROMs sit at 0x50 on SMBus).
const IGNORABLE_ADAPTER_PREFIXES: [&str; 8] =
    ["SMBus", "i2c-designware", "Synopsys DesignWare", "AMDGPU SMU", "smu", "soc:i2cdsi", "mac-io", "u4"];

/// True for adapter names that are never a display's DDC bus (SMBus controllers and the like).
pub fn is_ignorable_adapter(adapter_name: &str) -> bool {
    IGNORABLE_ADAPTER_PREFIXES.iter().any(|p| adapter_name.starts_with(p))
}

/// Orders the buses to probe: those named by a `/sys/class/drm/*/ddc` link first, then the rest; each group sorted by
/// bus number. `devices` are device nodes (`/dev/i2c-4`, or a bare `i2c-4`) and are returned as given, deduplicated;
/// `linked` are the adapter names the drm `ddc` links point at (`i2c-4`).
pub fn candidate_buses<D: AsRef<str>, L: AsRef<str>>(devices: &[D], linked: &[L]) -> Vec<String> {
    let is_linked = |device: &str| linked.iter().any(|l| bus_name(l.as_ref()) == bus_name(device));
    let mut buses: Vec<String> = devices.iter().map(|d| d.as_ref().to_owned()).collect();
    buses.sort_by(|a, b| {
        let key = |d: &str| (!is_linked(d), bus_number(d).unwrap_or(u32::MAX));
        key(a).cmp(&key(b)).then_with(|| a.cmp(b))
    });
    buses.dedup();
    buses
}

/// `/dev/i2c-4` → `i2c-4`.
pub fn bus_name(device_or_name: &str) -> &str {
    device_or_name.trim_end_matches('/').rsplit('/').next().unwrap_or_default()
}

/// What a DRM connector is, from its sysfs name (`card1-eDP-1`, `card0-HDMI-A-2`, `card0-Virtual-1`) and EDID id:
/// laptop panels (eDP, LVDS, DSI) are built in; virtio/QXL connectors and hypervisor vendors are virtual.
pub fn connector_kind(connector: &str, edid_id: &str) -> DisplayKind {
    let port = connector_port(connector).unwrap_or(connector);
    if ["eDP", "LVDS", "DSI"].iter().any(|p| port.starts_with(p)) {
        DisplayKind::BuiltIn
    } else if port.starts_with("Virtual") || is_virtual_vendor(edid_id) {
        DisplayKind::Virtual
    } else {
        DisplayKind::External
    }
}

/// The port of a DRM connector's sysfs name: `card0-DP-2` → `DP-2`. None for the other `/sys/class/drm` entries
/// (`card0`, `renderD128`, `version`).
pub fn connector_port(name: &str) -> Option<&str> {
    let (card, port) = name.strip_prefix("card")?.split_once('-')?;
    (!card.is_empty() && card.bytes().all(|b| b.is_ascii_digit()) && !port.is_empty()).then_some(port)
}

/// The connector as identity records it: the port alone (`DP-2`), since card numbers can swap between boots on
/// hybrid-graphics machines; the full sysfs name when the same port exists on more than one card in `all` (every
/// connector folder, connected or not), so two GPUs' `DP-1` stay apart.
pub fn connector_label(name: &str, all: &[&str]) -> String {
    let port = connector_port(name).unwrap_or(name);
    let shared = all.iter().any(|other| *other != name && connector_port(other) == Some(port));
    if shared { name } else { port }.to_owned()
}

/// The i2c adapter a connector's `ddc` symlink points at: `../../../i2c-5` → `i2c-5`. None unless it is `i2c-<N>`.
pub fn bus_from_link(target: &str) -> Option<&str> {
    let name = bus_name(target);
    bus_number(name).map(|_| name)
}

/// The connector's own i2c adapter among the entries of its sysfs folder (amdgpu and DP-AUX adapters sit there
/// instead of behind a `ddc` link): the lowest-numbered `i2c-<N>`.
pub fn bus_from_children<'a>(entries: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    entries.into_iter().filter_map(|e| Some((bus_number(e)?, e))).min().map(|(_, e)| e)
}

/// EDID id and serial from a connector's EDID: empty and None without a valid header.
pub fn edid_identity(data: &[u8]) -> (String, Option<String>) {
    match edid::id(data) {
        Some(id) => (id, edid::serial(data)),
        None => (String::new(), None),
    }
}

/// One connected DRM connector as discovery lists it. `connector` is the sysfs name, `label` its
/// [`connector_label`], `bus` the adapter (`i2c-5`) when it has one. The target is the bus's device node, else the
/// sysfs name; only an external display with a bus is driven over DDC.
pub fn display_info(connector: &str, label: String, data: &[u8], bus: Option<&str>) -> DisplayInfo {
    let (edid_id, serial) = edid_identity(data);
    let kind = connector_kind(connector, &edid_id);
    let backend = if bus.is_some() && kind == DisplayKind::External { BackendKind::Ddc } else { BackendKind::None };
    DisplayInfo {
        target: bus.map_or_else(|| connector.to_owned(), |b| format!("/dev/{b}")),
        identity: MonitorIdentity { edid_id, serial, connector: Some(label) },
        name: edid::display_name(data).unwrap_or_default(),
        kind,
        backend,
    }
}

/// Codes whose Set VCP goes out once, never retried: an input switch and the restore-defaults actions (0x04, 0x05,
/// 0x08) act on every copy that arrives, and the engine decides about a second try.
pub fn writes_once(code: u8) -> bool {
    matches!(code, 0x04 | 0x05 | 0x08 | 0x60)
}

/// Orders DRM connector names with their numbers compared as numbers: `card0-DP-2` before `card0-DP-10`.
pub fn connector_order(a: &str, b: &str) -> Ordering {
    natural_key(a).cmp(&natural_key(b))
}

/// Runs of digits (as numbers) and of everything else.
fn natural_key(name: &str) -> Vec<Result<u64, &str>> {
    let mut key = Vec::new();
    let mut rest = name;
    while let Some(first) = rest.chars().next() {
        let digits = first.is_ascii_digit();
        let end = rest.find(|c: char| c.is_ascii_digit() != digits).unwrap_or(rest.len());
        let (run, tail) = rest.split_at(end);
        key.push(if digits { run.parse().map_err(|_| run) } else { Err(run) });
        rest = tail;
    }
    key
}

fn bus_number(device: &str) -> Option<u32> {
    bus_name(device).strip_prefix("i2c-")?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bus_order_prefers_drm_ddc_links_then_numeric() {
        let buses = candidate_buses(
            &["/dev/i2c-10", "/dev/i2c-2", "/dev/i2c-1", "/dev/i2c-7", "/dev/i2c-3", "/dev/i2c-2"],
            &["i2c-7", "i2c-3", "i2c-99"],
        );
        assert_eq!(buses, ["/dev/i2c-3", "/dev/i2c-7", "/dev/i2c-1", "/dev/i2c-2", "/dev/i2c-10"]);
    }

    #[test]
    fn connectors_sort_into_built_in_external_and_virtual() {
        assert_eq!(connector_kind("card1-eDP-1", "BOE0A1B"), DisplayKind::BuiltIn);
        assert_eq!(connector_kind("card0-HDMI-A-2", "LEN62A2"), DisplayKind::External);
        assert_eq!(connector_kind("card0-DP-3", ""), DisplayKind::External);
        assert_eq!(connector_kind("card0-Virtual-1", "RHT1234"), DisplayKind::Virtual);
        assert_eq!(connector_kind("card0-HDMI-A-1", "VBX0000"), DisplayKind::Virtual);
    }

    #[test]
    fn memory_and_sensor_buses_are_skipped() {
        assert!(is_ignorable_adapter("SMBus I801 adapter at efa0"));
        assert!(!is_ignorable_adapter("i915 gmbus dpb"));
        assert!(!is_ignorable_adapter("AMDGPU DM i2c hw bus 1"));
    }
}
