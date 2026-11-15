//! `/sys/class/backlight`: one device's `brightness`, scaled by its `max_brightness`. Writing needs the udev rule
//! in packaging/linux/60-thinkvision-i2c.rules (video group).

use std::fs;
use std::io;
use std::path::PathBuf;

use tvc_core::identity::DisplayInfo;
use tvc_core::link::{LinkError, LinkFailure, MonitorLink};

use super::link::{BacklightLink, Panel};
use super::{connector_port, display_info, from_percent, panel_connector, pick_backlight, to_percent};

const BACKLIGHT: &str = "/sys/class/backlight";
const DRM: &str = "/sys/class/drm";

pub(super) fn displays() -> Vec<DisplayInfo> {
    let devices: Vec<(String, String)> = entries(BACKLIGHT)
        .into_iter()
        .filter(|name| fs::read_to_string(device_dir(name).join("max_brightness")).is_ok())
        .map(|name| {
            let kind = fs::read_to_string(device_dir(&name).join("type")).unwrap_or_default();
            (name, kind)
        })
        .collect();
    let Some(name) = pick_backlight(&devices) else {
        return Vec::new();
    };
    let own = fs::canonicalize(device_dir(name).join("device"))
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
    let connectors: Vec<(String, bool)> = entries(DRM)
        .into_iter()
        .filter(|c| c.starts_with("card") && c.contains('-'))
        .map(|c| {
            let connected = fs::read_to_string(format!("{DRM}/{c}/status")).is_ok_and(|s| s.trim() == "connected");
            (c, connected)
        })
        .collect();
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
    Ok(Box::new(BacklightLink::new(Sysfs { dir: device_dir(key), max: 0 })))
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
    /// `max_brightness`, read on open.
    max: u32,
}

impl Sysfs {
    fn brightness(&self) -> PathBuf {
        self.dir.join("brightness")
    }
}

impl Panel for Sysfs {
    fn open(&mut self) -> Result<(), LinkFailure> {
        self.max = read_number(&self.dir.join("max_brightness")).filter(|&m| m > 0).ok_or(LinkFailure::NotFound)?;
        match fs::OpenOptions::new().write(true).open(self.brightness()) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => Err(LinkFailure::PermissionDenied),
            Err(_) => Err(LinkFailure::NotFound),
        }
    }

    fn close(&mut self) {}

    fn get(&mut self) -> Result<u16, LinkError> {
        let text = fs::read_to_string(self.brightness()).map_err(|e| failure("read", e))?;
        let raw = text.trim().parse().map_err(|_| LinkError::Transient(format!("brightness reads {:?}", text.trim())))?;
        Ok(to_percent(raw, self.max))
    }

    fn set(&mut self, percent: u16) -> Result<(), LinkError> {
        fs::write(self.brightness(), from_percent(percent, self.max).to_string()).map_err(|e| failure("write", e))
    }
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
