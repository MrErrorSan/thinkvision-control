//! Per-OS DDC/CI backends behind `tvc_core::link::MonitorLink`: dxva2 on Windows, i2c-dev on Linux, IOAVService on
//! macOS (Apple Silicon). [`discovery`] picks the one for the OS this was built for.
//!
//! The pure parts (packet framing, EDID, Linux bus ordering, macOS service rules) compile everywhere so their tests run
//! on any OS; only the FFI modules are `cfg`-gated.

pub mod backlight;
pub mod edid;
pub mod linux;
pub mod macos;
pub mod packet;

#[cfg(windows)]
mod dxva2;
// The pure half of the Windows join, so its tests run on every OS.
#[cfg(all(test, not(windows)))]
#[path = "dxva2/join.rs"]
mod dxva2_join;

use tvc_core::identity::{BackendKind, Discovery, DisplayInfo, ListError};
use tvc_core::link::{DisplayKind, LinkError, LinkFailure, MonitorLink};

/// Every display this OS reports, external monitors through the OS's DDC backend plus built-in panels through
/// [`backlight`]. Targets starting with [`backlight::TARGET_PREFIX`] belong to the backlight backend.
pub fn discovery() -> Box<dyn Discovery> {
    #[cfg(windows)]
    let os: Box<dyn Discovery> = Box::new(dxva2::Dxva2Discovery::new());
    #[cfg(target_os = "linux")]
    let os: Box<dyn Discovery> = Box::new(linux::LinuxDiscovery::new());
    #[cfg(target_os = "macos")]
    let os: Box<dyn Discovery> = Box::new(macos::MacDiscovery::new());
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    let os: Box<dyn Discovery> = Box::new(NoDiscovery);
    Box::new(Combined { os })
}

#[cfg(windows)]
pub use dxva2::screen_rect;

/// The work area (x, y, width, height, physical pixels) of the screen a display from [`discovery`] shows, for placing
/// the OSD on it. None = unknown, and the OSD uses the primary screen. Only Windows can tell: here it is always None.
#[cfg(not(windows))]
pub fn screen_rect(_target: &str) -> Option<(i32, i32, i32, i32)> {
    None
}

/// The OS backend plus the built-in panels.
struct Combined {
    os: Box<dyn Discovery>,
}

impl Discovery for Combined {
    fn enumerate(&mut self) -> Result<Vec<DisplayInfo>, ListError> {
        let panels = backlight::displays();
        match self.os.enumerate() {
            Ok(mut list) => {
                // A panel the backlight backend controls replaces the OS backend's uncontrollable entry for it.
                if !panels.is_empty() {
                    list.retain(|d| !(d.kind == DisplayKind::BuiltIn && d.backend == BackendKind::None));
                }
                list.extend(panels);
                Ok(list)
            }
            Err(ListError::Unsupported) if !panels.is_empty() => Ok(panels),
            Err(e) => Err(e),
        }
    }

    fn open(&mut self, target: &str) -> Result<Box<dyn MonitorLink>, LinkFailure> {
        if target.starts_with(backlight::TARGET_PREFIX) {
            return backlight::open(target);
        }
        self.os.open(target)
    }
}

/// Discovery on an OS without a backend: lists nothing.
#[allow(dead_code)]
struct NoDiscovery;

impl Discovery for NoDiscovery {
    fn enumerate(&mut self) -> Result<Vec<DisplayInfo>, ListError> {
        Err(ListError::Unsupported)
    }
    fn open(&mut self, _target: &str) -> Result<Box<dyn MonitorLink>, LinkFailure> {
        Err(LinkFailure::NotFound)
    }
}

/// The error every backend returns while it holds no connection.
pub(crate) fn not_connected() -> LinkError {
    LinkError::Gone("monitor not connected".into())
}

/// Bytes as `.NET Encoding.ASCII` decodes them (non-ASCII becomes '?'), for capability strings.
pub(crate) fn ascii(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| if b.is_ascii() { b as char } else { '?' }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_discovery_lists_and_opens_nothing() {
        let mut d = NoDiscovery;
        assert_eq!(d.enumerate(), Err(ListError::Unsupported));
        assert!(matches!(d.open("x"), Err(LinkFailure::NotFound)));
    }

    #[test]
    fn ascii_replaces_non_ascii() {
        assert_eq!(ascii(b"ab\xFFc"), "ab?c");
    }
}
