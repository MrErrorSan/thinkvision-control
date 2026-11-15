//! Linux i2c-dev backend. The connector and bus rules here are pure and compile on every OS; the link itself is Linux-only.

#[cfg(target_os = "linux")]
mod discovery;
#[cfg(target_os = "linux")]
mod i2c;
#[cfg(target_os = "linux")]
pub(crate) use discovery::LinuxDiscovery;

use std::cmp::Ordering;

use tvc_core::identity::{BackendKind, DisplayInfo, MonitorIdentity};
use tvc_core::link::{is_virtual_vendor, DisplayKind};

use crate::edid;

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
    use crate::edid::testing::p32p20;

    #[test]
    fn connectors_sort_into_built_in_external_and_virtual() {
        assert_eq!(connector_kind("card1-eDP-1", "BOE0A1B"), DisplayKind::BuiltIn);
        assert_eq!(connector_kind("card0-HDMI-A-2", "LEN62A2"), DisplayKind::External);
        assert_eq!(connector_kind("card0-DP-3", ""), DisplayKind::External);
        assert_eq!(connector_kind("card0-Virtual-1", "RHT1234"), DisplayKind::Virtual);
        assert_eq!(connector_kind("card0-HDMI-A-1", "VBX0000"), DisplayKind::Virtual);
    }

    #[test]
    fn connector_port_is_the_name_after_the_card() {
        assert_eq!(connector_port("card0-DP-2"), Some("DP-2"));
        assert_eq!(connector_port("card1-eDP-1"), Some("eDP-1"));
        assert_eq!(connector_port("card10-HDMI-A-1"), Some("HDMI-A-1"));
        assert_eq!(connector_port("card0"), None);
        assert_eq!(connector_port("renderD128"), None);
        assert_eq!(connector_port("version"), None);
        assert_eq!(connector_port("card-DP-1"), None);
        assert_eq!(connector_port("cardX-DP-1"), None);
        assert_eq!(connector_port("card0-"), None);
        assert_eq!(connector_port("eDP-1"), None);
    }

    #[test]
    fn connector_label_keeps_the_card_only_when_the_port_repeats() {
        let one_gpu = ["card0-DP-1", "card0-DP-2", "card0-HDMI-A-1", "card0", "renderD128"];
        assert_eq!(connector_label("card0-DP-1", &one_gpu), "DP-1");
        assert_eq!(connector_label("card0-HDMI-A-1", &one_gpu), "HDMI-A-1");
        let two_gpus = ["card0-DP-1", "card1-DP-1", "card1-eDP-1"];
        assert_eq!(connector_label("card0-DP-1", &two_gpus), "card0-DP-1");
        assert_eq!(connector_label("card1-DP-1", &two_gpus), "card1-DP-1");
        assert_eq!(connector_label("card1-eDP-1", &two_gpus), "eDP-1");
        // The connector itself needn't be in the list, and `all` may be empty (a reconnect check).
        assert_eq!(connector_label("card2-DP-3", &[]), "DP-3");
        assert_eq!(connector_label("card2-DP-1", &two_gpus), "card2-DP-1");
    }

    #[test]
    fn bus_from_link_takes_only_an_i2c_adapter() {
        assert_eq!(bus_from_link("../../../i2c-5"), Some("i2c-5"));
        assert_eq!(bus_from_link("../../../../devices/pci0000:00/0000:00:02.0/i2c-12"), Some("i2c-12"));
        assert_eq!(bus_from_link("../../i2c-5/"), Some("i2c-5"));
        assert_eq!(bus_from_link("../../i2c-dev"), None);
        assert_eq!(bus_from_link("../../card0"), None);
        assert_eq!(bus_from_link(""), None);
    }

    #[test]
    fn bus_from_children_takes_the_lowest_numbered_adapter() {
        let entries = ["edid", "i2c-7", "status", "i2c-3", "drm_dp_aux0", "ddc"];
        assert_eq!(bus_from_children(entries), Some("i2c-3"));
        assert_eq!(bus_from_children(["i2c-10", "i2c-9"]), Some("i2c-9"));
        assert_eq!(bus_from_children(["edid", "i2c-dev", "status"]), None);
        assert_eq!(bus_from_children([]), None);
    }

    #[test]
    fn edid_identity_needs_a_header() {
        assert_eq!(edid_identity(&p32p20(1234, None)), ("LEN62A2".into(), Some("1234".into())));
        assert_eq!(edid_identity(&p32p20(0, None)), ("LEN62A2".into(), None));
        assert_eq!(edid_identity(&[]), (String::new(), None));
        let mut no_header = p32p20(1234, None);
        no_header[0] = 0x01;
        assert_eq!(edid_identity(&no_header), (String::new(), None));
    }

    #[test]
    fn display_info_drives_an_external_display_with_a_bus() {
        let info = display_info("card0-DP-2", "DP-2".into(), &p32p20(1234, Some("P32p-20")), Some("i2c-5"));
        assert_eq!(info.target, "/dev/i2c-5");
        let identity =
            MonitorIdentity { edid_id: "LEN62A2".into(), serial: Some("1234".into()), connector: Some("DP-2".into()) };
        assert_eq!(info.identity, identity);
        assert_eq!(info.name, "P32p-20");
        assert_eq!(info.kind, DisplayKind::External);
        assert_eq!(info.backend, BackendKind::Ddc);
    }

    #[test]
    fn display_info_without_a_bus_targets_the_connector() {
        let info = display_info("card0-DP-2", "DP-2".into(), &[], None);
        assert_eq!(info.target, "card0-DP-2");
        assert_eq!(info.identity, MonitorIdentity { edid_id: String::new(), serial: None, connector: Some("DP-2".into()) });
        assert_eq!(info.name, "");
        assert_eq!(info.kind, DisplayKind::External);
        assert_eq!(info.backend, BackendKind::None);
    }

    #[test]
    fn built_in_and_virtual_displays_are_not_ddc() {
        let panel = display_info("card1-eDP-1", "eDP-1".into(), &[], Some("i2c-2"));
        assert_eq!((panel.kind, panel.backend), (DisplayKind::BuiltIn, BackendKind::None));
        assert_eq!(panel.target, "/dev/i2c-2");
        let virtio = display_info("card0-Virtual-1", "Virtual-1".into(), &[], Some("i2c-0"));
        assert_eq!((virtio.kind, virtio.backend), (DisplayKind::Virtual, BackendKind::None));
        // A hypervisor's EDID on an ordinary-looking port.
        let qemu = edid::testing::edid([0x49, 0x14, 0x34, 0x12], 0, None); // RHT1234
        let vm = display_info("card0-HDMI-A-1", "HDMI-A-1".into(), &qemu, Some("i2c-1"));
        assert_eq!(vm.identity.edid_id, "RHT1234");
        assert_eq!((vm.kind, vm.backend), (DisplayKind::Virtual, BackendKind::None));
    }

    #[test]
    fn writes_once_covers_input_and_resets() {
        for code in [0x04, 0x05, 0x08, 0x60] {
            assert!(writes_once(code), "0x{code:02X}");
        }
        assert!(!writes_once(0x10));
        assert!(!writes_once(0x12));
    }

    #[test]
    fn connectors_sort_naturally() {
        let mut names = vec!["card0-DP-10", "card1-DP-1", "card0-DP-2", "card0-HDMI-A-1"];
        names.sort_by(|a, b| connector_order(a, b));
        assert_eq!(names, ["card0-DP-2", "card0-DP-10", "card0-HDMI-A-1", "card1-DP-1"]);
    }
}
