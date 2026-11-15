//! Linux display discovery: each connected DRM connector mapped to its i2c bus, identity from the sysfs EDID the
//! kernel already read. Nothing here touches the bus.

use std::fs;
use std::path::Path;

use tvc_core::identity::{Discovery, DisplayInfo, ListError};
use tvc_core::link::{LinkFailure, MonitorLink};

use super::i2c::{Binding, I2cLink};
use super::{bus_from_children, bus_from_link, connector_label, connector_order, connector_port, display_info};
use crate::edid;

const DRM: &str = "/sys/class/drm";

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
    Some(list)
}

/// The connector `name` when its status is `connected`; `all` is every connector folder, for its label.
pub(super) fn connector(name: &str, all: &[&str]) -> Option<Connector> {
    let dir = Path::new(DRM).join(name);
    let status = fs::read_to_string(dir.join("status")).ok()?;
    if status.trim() != "connected" {
        return None;
    }
    Some(Connector {
        name: name.to_owned(),
        label: connector_label(name, all),
        edid: read_edid(&dir.join("edid")),
        bus: ddc_bus(&dir),
    })
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
