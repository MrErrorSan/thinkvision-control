//! Which monitor is which: the identity a backend reads (EDID id, serial, connector), the stable [`MonitorId`] the
//! app keys everything by, and [`Discovery`], the per-OS enumeration that hands out links bound to one display.
//!
//! Id format: `{EDID}-S{serial}` when the serial is unique among the attached monitors, else `{EDID}-P{connector}`,
//! else `{EDID}-N{ordinal}`. An `-N` id is unstable: nothing that enables writes is persisted under it.

use crate::link::{DisplayKind, LinkFailure, MonitorLink};

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MonitorId(pub String);

impl MonitorId {
    /// True for an `-N{ordinal}` id, which may change on the next rescan. The marker is the letter after the EDID
    /// id (which never holds a `-`: "LEN62A2", "UNKNOWN"), so a serial or connector with "-N" in it doesn't count.
    pub fn is_unstable(&self) -> bool {
        self.0.split_once('-').and_then(|(_, rest)| rest.strip_prefix('N'))
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    }

    /// A folder name for this id: ASCII letters, digits, `-` and `_`; everything else becomes `_`.
    pub fn folder_name(&self) -> String {
        self.0.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
    }
}

impl std::fmt::Display for MonitorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// What a backend can tell about a display. `edid_id` is PNP + product ("LEN62A2"), empty when unknown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MonitorIdentity {
    pub edid_id: String,
    /// The EDID serial: the 0xFF text descriptor, else the 32-bit number when non-zero.
    pub serial: Option<String>,
    /// The port, stable while the cable stays put ("DP-2", "card0-HDMI-A-1", a Windows target id).
    pub connector: Option<String>,
}

/// How the app talks to a display.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BackendKind {
    Ddc,
    /// A built-in panel's brightness through the OS (WMI, /sys/class/backlight).
    Backlight,
    /// Listed but not controllable (virtual screens, ports without DDC).
    None,
}

/// One display as a backend enumerates it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayInfo {
    /// Backend-opaque handle for [`Discovery::open`] (a device path, a bus, a service), valid until the next
    /// `enumerate`. Not an identity: the hub keys monitors by [`MonitorId`].
    pub target: String,
    pub identity: MonitorIdentity,
    /// The name the display gives itself (EDID), else the one the OS gives it; empty when neither.
    pub name: String,
    pub kind: DisplayKind,
    pub backend: BackendKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListError {
    /// This backend can't list displays.
    Unsupported,
    Failed(String),
}

/// The per-OS display list. Lives on the hub's thread.
pub trait Discovery: Send {
    /// Every display the OS sees right now, virtual ones included.
    fn enumerate(&mut self) -> Result<Vec<DisplayInfo>, ListError>;
    /// A link bound to exactly that display; its `connect` reopens the same target, or fails `NotFound` once it's
    /// gone. `target` comes from the latest `enumerate`.
    fn open(&mut self, target: &str) -> Result<Box<dyn MonitorLink>, LinkFailure>;
}

/// Ids for `identities` (in enumeration order), by the format in the module doc. `-N` ordinals count per EDID id
/// among the displays that have neither a unique serial nor a connector.
pub fn assign_ids(identities: &[MonitorIdentity]) -> Vec<MonitorId> {
    let edid = |i: &MonitorIdentity| if i.edid_id.is_empty() { "UNKNOWN".to_string() } else { i.edid_id.clone() };
    fn unique_serial<'a>(i: &'a MonitorIdentity, all: &[MonitorIdentity]) -> Option<&'a String> {
        i.serial.as_ref().filter(|s| !s.is_empty()).filter(|s| {
            all.iter().filter(|o| o.edid_id == i.edid_id && o.serial.as_ref() == Some(*s)).count() == 1
        })
    }
    let mut ordinals = std::collections::BTreeMap::<String, usize>::new();
    identities.iter().map(|i| {
        let e = edid(i);
        if let Some(serial) = unique_serial(i, identities) {
            MonitorId(format!("{e}-S{serial}"))
        } else if let Some(conn) = i.connector.as_ref().filter(|c| !c.is_empty()) {
            MonitorId(format!("{e}-P{conn}"))
        } else {
            let n = ordinals.entry(e.clone()).or_insert(0);
            *n += 1;
            MonitorId(format!("{e}-N{n}"))
        }
    }).collect()
}

/// The saved entry that belongs to a display: the exact id, then the same EDID id and connector, then the only
/// saved entry with that EDID id. None when nothing matches, or the last rule is ambiguous.
pub fn match_saved<'a>(id: &MonitorId, identity: &MonitorIdentity, saved: &'a [(MonitorId, MonitorIdentity)])
    -> Option<&'a MonitorId> {
    if let Some((s, _)) = saved.iter().find(|(s, _)| s == id) {
        return Some(s);
    }
    if identity.connector.is_some() {
        if let Some((s, _)) =
            saved.iter().find(|(_, i)| i.edid_id == identity.edid_id && i.connector == identity.connector) {
            return Some(s);
        }
    }
    let mut same = saved.iter().filter(|(_, i)| !identity.edid_id.is_empty() && i.edid_id == identity.edid_id);
    match (same.next(), same.next()) {
        (Some((s, _)), None) => Some(s),
        _ => None,
    }
}
