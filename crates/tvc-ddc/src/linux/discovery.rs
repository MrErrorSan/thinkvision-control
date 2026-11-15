//! Linux display discovery: each connected DRM connector mapped to its i2c bus, identity from the sysfs EDID the
//! kernel already read. Nothing here touches the bus, except the NVIDIA fallback below.
//!
//! NVIDIA's proprietary driver links neither a `ddc` symlink nor an `i2c-N` folder to its connectors; its adapters
//! hang off the GPU. For a connected external connector left without a bus while that driver is loaded, each of
//! the GPU's unclaimed adapters is read once at 0x50 and bound when its EDID equals the connector's (what ddcutil
//! does). The result is kept per connector and EDID, so later enumerations and reconnects don't probe again.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};

use tvc_core::identity::{Discovery, DisplayInfo, ListError};
use tvc_core::link::{DisplayKind, LinkFailure, MonitorLink};

use super::i2c::{is_permission, read_edid_block, Binding, I2cLink};
use super::{
    bus_from_children, bus_from_link, bus_number, connector_kind, connector_label, connector_order, connector_port,
    display_info, edid_matches, is_nvidia_adapter,
};
use crate::edid;

const DRM: &str = "/sys/class/drm";
const I2C_DEVICES: &str = "/sys/bus/i2c/devices";

/// A connected DRM connector as sysfs shows it.
pub(super) struct Connector {
    /// The sysfs folder name, `card0-DP-2`.
    pub(super) name: String,
    /// Its [`connector_label`].
    pub(super) label: String,
    pub(super) edid: Vec<u8>,
    /// The i2c adapter carrying its DDC lines, `i2c-5`.
    pub(super) bus: Option<String>,
}

/// The connected connectors in [`connector_order`]. None when DRM lists no connectors at all (no KMS driver, e.g.
/// nvidia without `modeset=1`).
pub(super) fn connectors() -> Option<Vec<Connector>> {
    let names: Vec<String> = fs::read_dir(DRM)
        .ok()?
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| connector_port(name).is_some())
        .collect();
    if names.is_empty() {
        return None;
    }
    let all: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut list: Vec<Connector> = all.iter().filter_map(|name| connector(name, &all)).collect();
    list.sort_by(|a, b| connector_order(&a.name, &b.name));
    if nvidia_loaded() {
        bind_nvidia(&mut list);
    }
    Some(list)
}

/// The connector `name` when its status is `connected`; `all` is every connector folder, for its label.
pub(super) fn connector(name: &str, all: &[&str]) -> Option<Connector> {
    let dir = Path::new(DRM).join(name);
    let status = fs::read_to_string(dir.join("status")).ok()?;
    if status.trim() != "connected" {
        return None;
    }
    let edid = read_edid(&dir.join("edid"));
    let bus = ddc_bus(&dir).or_else(|| probed_bus(name, &edid));
    Some(Connector { name: name.to_owned(), label: connector_label(name, all), edid, bus })
}

/// The adapter behind the connector's `ddc` symlink, else the `i2c-N` folder inside the connector's own folder.
fn ddc_bus(dir: &Path) -> Option<String> {
    let linked = fs::read_link(dir.join("ddc")).ok();
    if let Some(bus) = linked.as_ref().and_then(|t| t.to_str()).and_then(bus_from_link) {
        return Some(bus.to_owned());
    }
    let entries: Vec<String> =
        fs::read_dir(dir).ok()?.flatten().filter_map(|e| e.file_name().into_string().ok()).collect();
    bus_from_children(entries.iter().map(String::as_str)).map(str::to_owned)
}

// ----- NVIDIA proprietary driver -----

/// One connector the NVIDIA fallback probed: the bus it found, or none among `candidates`.
struct Probed {
    connector: String,
    edid: Vec<u8>,
    bus: Option<String>,
    candidates: Vec<String>,
}

static PROBED: Mutex<Vec<Probed>> = Mutex::new(Vec::new());

/// The last probe couldn't open an NVIDIA adapter for permission: the udev rule is missing, not a driver option.
static DENIED: AtomicBool = AtomicBool::new(false);

fn probed() -> MutexGuard<'static, Vec<Probed>> {
    PROBED.lock().unwrap_or_else(|e| e.into_inner())
}

fn nvidia_loaded() -> bool {
    Path::new("/sys/module/nvidia").exists()
}

/// The bus an earlier probe bound to this connector and EDID, while that adapter is still an NVIDIA one.
fn probed_bus(connector: &str, edid: &[u8]) -> Option<String> {
    let bus = probed().iter().find(|p| p.connector == connector && p.edid == edid)?.bus.clone()?;
    adapter_name(&bus).is_some_and(|n| is_nvidia_adapter(&n)).then_some(bus)
}

fn adapter_name(bus: &str) -> Option<String> {
    fs::read_to_string(Path::new(I2C_DEVICES).join(bus).join("name")).ok()
}

/// Binds each connected external connector without a bus to the NVIDIA adapter whose EDID at 0x50 is its own.
/// Connectors that already have a bus are left as they are, and their buses are never probed.
fn bind_nvidia(list: &mut [Connector]) {
    let mut taken: Vec<String> = list.iter().filter_map(|c| c.bus.clone()).collect();
    for c in list.iter_mut() {
        if c.bus.is_some() || c.edid.len() < 128 || connector_kind(&c.name, "") != DisplayKind::External {
            continue;
        }
        let candidates: Vec<String> =
            nvidia_adapters(card_of(&c.name)).into_iter().filter(|b| !taken.contains(b)).collect();
        let tried = probed().iter().any(|p| p.connector == c.name && p.edid == c.edid && p.candidates == candidates);
        if candidates.is_empty() || tried {
            continue;
        }
        let mut complete = true; // every candidate could be read (none refused for permission)
        let found = candidates
            .iter()
            .find(|bus| match read_edid_block(bus) {
                Ok(block) => edid_matches(&block, &c.edid),
                Err(errno) => {
                    complete &= !is_permission(errno);
                    false
                }
            })
            .cloned();
        DENIED.store(!complete, Ordering::Relaxed);
        if found.is_some() || complete {
            let mut cache = probed();
            cache.retain(|p| p.connector != c.name);
            cache.push(Probed { connector: c.name.clone(), edid: c.edid.clone(), bus: found.clone(), candidates });
        }
        if let Some(bus) = found {
            taken.push(bus.clone());
            c.bus = Some(bus);
        }
    }
}

/// `card1-DP-2` → `card1`.
fn card_of(connector: &str) -> &str {
    connector.split_once('-').map_or(connector, |(card, _)| card)
}

/// The NVIDIA i2c adapters that have a device node, lowest number first: those on `card`'s GPU when both sides
/// resolve, else all of them.
fn nvidia_adapters(card: &str) -> Vec<String> {
    let gpu = fs::canonicalize(Path::new(DRM).join(card).join("device")).ok();
    let Ok(entries) = fs::read_dir(I2C_DEVICES) else { return Vec::new() };
    let mut buses: Vec<String> = entries
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|bus| bus_number(bus).is_some() && Path::new("/dev").join(bus).exists())
        .filter(|bus| adapter_name(bus).is_some_and(|n| is_nvidia_adapter(&n)))
        .filter(|bus| {
            let parent =
                fs::canonicalize(Path::new(I2C_DEVICES).join(bus)).ok().and_then(|p| p.parent().map(PathBuf::from));
            gpu.is_none() || parent.is_none() || parent == gpu
        })
        .collect();
    buses.sort_by_key(|b| bus_number(b));
    buses
}

/// For the setup card: NVIDIA's driver is loaded, and DRM lists no connectors (no `nvidia-drm.modeset=1`) or a
/// connected external one has no bus even after the fallback. Reads sysfs and earlier probe results only.
pub fn nvidia_without_ddc() -> bool {
    if !nvidia_loaded() {
        return false;
    }
    let names: Vec<String> = fs::read_dir(DRM)
        .map(|d| d.flatten().filter_map(|e| e.file_name().into_string().ok()).collect())
        .unwrap_or_default();
    let names: Vec<&String> = names.iter().filter(|n| connector_port(n).is_some()).collect();
    names.is_empty()
        || names.iter().filter_map(|n| connector(n, &[])).any(|c| {
            c.bus.is_none() && !c.edid.is_empty() && connector_kind(&c.name, "") == DisplayKind::External
        })
}

/// For the setup card: the last NVIDIA probe was refused access to an adapter, so the I²C permission card applies.
pub fn nvidia_access_denied() -> bool {
    DENIED.load(Ordering::Relaxed)
}

/// The sysfs EDID. A bad base-block checksum is read once more and the second read used as it comes, since some
/// monitors ship a wrong checksum. Empty when unreadable.
fn read_edid(path: &Path) -> Vec<u8> {
    let read = || fs::read(path).unwrap_or_default();
    let data = read();
    if data.is_empty() || edid::checksum_ok(&data) { data } else { read() }
}

/// DRM connectors with their i2c buses; `open` hands out links bound to one bus.
pub(crate) struct LinuxDiscovery {
    /// The connectors with a bus from the latest `enumerate`.
    bound: Vec<Binding>,
}

impl LinuxDiscovery {
    pub(crate) fn new() -> Self {
        Self { bound: Vec::new() }
    }
}

impl Discovery for LinuxDiscovery {
    fn enumerate(&mut self) -> Result<Vec<DisplayInfo>, ListError> {
        let list = connectors().ok_or(ListError::Unsupported)?;
        self.bound.clear();
        Ok(list
            .into_iter()
            .map(|c| {
                let info = display_info(&c.name, c.label, &c.edid, c.bus.as_deref());
                if c.bus.is_some() {
                    self.bound.push(Binding {
                        connector: c.name,
                        device: info.target.clone(),
                        edid_id: info.identity.edid_id.clone(),
                        serial: info.identity.serial.clone(),
                    });
                }
                info
            })
            .collect())
    }

    fn open(&mut self, target: &str) -> Result<Box<dyn MonitorLink>, LinkFailure> {
        let binding = self.bound.iter().find(|b| b.device == target).ok_or(LinkFailure::NotFound)?;
        Ok(Box::new(I2cLink::bound(binding.clone())))
    }
}
