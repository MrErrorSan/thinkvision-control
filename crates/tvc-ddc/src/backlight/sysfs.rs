//! `/sys/class/backlight`: one device's `brightness`, scaled by its `max_brightness`. Reading needs nothing; a write
//! goes to the file when the user may write it (the udev rule in packaging/linux/60-thinkvision-i2c.rules gives the
//! video group that), else through logind's `Session.SetBrightness` (systemd 243+), which any user may call for their
//! own session.

use std::fs;
use std::io;
use std::path::PathBuf;

use tvc_core::identity::DisplayInfo;
use tvc_core::link::{LinkError, LinkFailure, MonitorLink};
use zbus::blocking::Connection;

use super::link::{BacklightLink, Panel};
use super::{connector_port, display_info, from_percent, is_ddcci_backlight, panel_connector, pick_backlight, to_percent};

const BACKLIGHT: &str = "/sys/class/backlight";
const DRM: &str = "/sys/class/drm";
/// The caller's own session, or the user's graphical one when the caller has none (an autostart under systemd).
const LOGIND_SESSION: &str = "/org/freedesktop/login1/session/auto";

pub(super) fn displays() -> Vec<DisplayInfo> {
    let connectors: Vec<(String, bool)> = entries(DRM)
        .into_iter()
        .filter(|c| c.starts_with("card") && c.contains('-'))
        .map(|c| {
            let connected = fs::read_to_string(format!("{DRM}/{c}/status")).is_ok_and(|s| s.trim() == "connected");
            (c, connected)
        })
        .collect();
    // A `raw` device on a machine without a built-in connector is not a laptop panel.
    let built_in = panel_connector(None, &connectors).is_some();
    let devices: Vec<(String, String)> = entries(BACKLIGHT)
        .into_iter()
        .filter(|name| fs::read_to_string(device_dir(name).join("max_brightness")).is_ok())
        .filter(|name| {
            let device = fs::canonicalize(device_dir(name).join("device")).unwrap_or_default();
            !is_ddcci_backlight(name, &device.to_string_lossy())
        })
        .map(|name| {
            let kind = fs::read_to_string(device_dir(&name).join("type")).unwrap_or_default();
            (name, kind)
        })
        .filter(|(_, kind)| built_in || kind.trim() != "raw")
        .collect();
    let Some(name) = pick_backlight(&devices) else {
        return Vec::new();
    };
    let own = fs::canonicalize(device_dir(name).join("device"))
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
    let info = match panel_connector(own.as_deref(), &connectors) {
        Some(conn) => display_info(name, &fs::read(format!("{DRM}/{conn}/edid")).unwrap_or_default(), connector_port(conn)),
        None => display_info(name, &[], "internal"),
    };
    vec![info]
}

pub(super) fn open(key: &str) -> Result<Box<dyn MonitorLink>, LinkFailure> {
    if key.is_empty() || key.contains('/') || key.starts_with('.') {
        return Err(LinkFailure::NotFound);
    }
    Ok(Box::new(BacklightLink::new(Sysfs { dir: device_dir(key), name: key.to_owned(), max: 0, logind: None })))
}

fn device_dir(name: &str) -> PathBuf {
    PathBuf::from(BACKLIGHT).join(name)
}

/// The names in a sysfs class directory; empty when it can't be read.
fn entries(dir: &str) -> Vec<String> {
    fs::read_dir(dir)
        .map(|d| d.filter_map(Result::ok).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default()
}

struct Sysfs {
    dir: PathBuf,
    /// The device name, `intel_backlight`.
    name: String,
    /// `max_brightness`, read on open.
    max: u32,
    /// The system bus, when the file isn't writable and writes go through logind instead.
    logind: Option<Connection>,
}

impl Sysfs {
    fn brightness(&self) -> PathBuf {
        self.dir.join("brightness")
    }

    fn raw(&self) -> Result<u32, LinkError> {
        let text = fs::read_to_string(self.brightness()).map_err(|e| failure("read", e))?;
        text.trim().parse().map_err(|_| LinkError::Transient(format!("brightness reads {:?}", text.trim())))
    }
}

impl Panel for Sysfs {
    fn open(&mut self) -> Result<(), LinkFailure> {
        self.logind = None;
        self.max = read_number(&self.dir.join("max_brightness")).filter(|&m| m > 0).ok_or(LinkFailure::NotFound)?;
        match fs::OpenOptions::new().write(true).open(self.brightness()) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                // Checked by writing the current value back, which changes nothing on screen.
                let current = self.raw().map_err(|_| LinkFailure::PermissionDenied)?;
                let bus = Connection::system().map_err(|_| LinkFailure::PermissionDenied)?;
                logind_set(&bus, &self.name, current).map_err(|_| LinkFailure::PermissionDenied)?;
                self.logind = Some(bus);
                Ok(())
            }
            Err(_) => Err(LinkFailure::NotFound),
        }
    }

    fn close(&mut self) {
        self.logind = None;
    }

    fn get(&mut self) -> Result<u16, LinkError> {
        Ok(to_percent(self.raw()?, self.max))
    }

    fn set(&mut self, percent: u16) -> Result<(), LinkError> {
        let raw = from_percent(percent, self.max);
        match &self.logind {
            // Gone: the engine reconnects, and `open` checks again which route works.
            Some(bus) => logind_set(bus, &self.name, raw)
                .map_err(|e| LinkError::Gone(format!("backlight write through logind failed: {e}"))),
            None => fs::write(self.brightness(), raw.to_string()).map_err(|e| failure("write", e)),
        }
    }
}

/// logind's `SetBrightness("backlight", name, raw)` on the user's session.
fn logind_set(bus: &Connection, name: &str, raw: u32) -> zbus::Result<()> {
    bus.call_method(
        Some("org.freedesktop.login1"),
        LOGIND_SESSION,
        Some("org.freedesktop.login1.Session"),
        "SetBrightness",
        &("backlight", name, raw),
    )
    .map(drop)
}

fn read_number(path: &std::path::Path) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// A missing device or a denied write is `Gone`, so the engine reconnects and `open` reports `NotFound` or
/// `PermissionDenied`; anything else is worth retrying.
fn failure(op: &str, error: io::Error) -> LinkError {
    let message = format!("backlight {op} failed: {error}");
    match error.kind() {
        io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied => LinkError::Gone(message),
        _ => LinkError::Transient(message),
    }
}
