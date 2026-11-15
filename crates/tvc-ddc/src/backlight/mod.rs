//! Built-in panel brightness through the OS, as a `MonitorLink` that offers only 0x10: WMI
//! (`WmiMonitorBrightness`) on Windows, `/sys/class/backlight` on Linux. Implemented in w3-backlight.

use tvc_core::identity::DisplayInfo;
use tvc_core::link::{LinkFailure, MonitorLink};

/// Every backlight target starts with this, so `discovery()` can route `open` here.
pub const TARGET_PREFIX: &str = "backlight:";

/// The built-in panels this OS lets the app dim, with `backend: BackendKind::Backlight` and a
/// `TARGET_PREFIX` target. Empty until w3-backlight.
pub fn displays() -> Vec<DisplayInfo> {
    Vec::new()
}

/// A link for a target from [`displays`].
pub fn open(target: &str) -> Result<Box<dyn MonitorLink>, LinkFailure> {
    let _ = target;
    Err(LinkFailure::NotFound)
}
