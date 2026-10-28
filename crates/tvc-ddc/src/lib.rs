//! Per-OS DDC/CI backends behind `tvc_core::link::MonitorLink`: dxva2 on Windows, i2c-dev on Linux, IOAVService on
//! macOS (Apple Silicon). [`open`] picks the one for the OS this was built for.
//!
//! The pure parts (packet framing, EDID, Linux bus ordering, macOS service rules) compile everywhere so their tests run
//! on any OS; only the FFI modules are `cfg`-gated.

pub mod edid;
pub mod linux;
pub mod macos;
pub mod packet;

#[cfg(windows)]
mod dxva2;

use tvc_core::link::{LinkError, LinkFailure, MonitorLink, MonitorMatch, VcpReading};

/// The DDC link for this OS. Never fails: [`MonitorLink::connect`] reports what's wrong.
pub fn open(target: MonitorMatch) -> Box<dyn MonitorLink> {
    #[cfg(windows)]
    let link = dxva2::Dxva2Link::new(target);
    #[cfg(target_os = "linux")]
    let link = linux::I2cLink::new(target);
    #[cfg(target_os = "macos")]
    let link = macos::IoAvLink::new(target);
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    let link = {
        let _ = target;
        NoLink
    };
    Box::new(link)
}

/// The error every backend returns while it holds no connection.
pub(crate) fn not_connected() -> LinkError {
    LinkError::Gone("monitor not connected".into())
}

/// Bytes as `.NET Encoding.ASCII` decodes them (non-ASCII becomes '?'), for capability strings.
pub(crate) fn ascii(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| if b.is_ascii() { b as char } else { '?' }).collect()
}

/// The link on an OS without a backend: never finds the monitor.
#[allow(dead_code)]
struct NoLink;

impl MonitorLink for NoLink {
    fn connect(&mut self) -> Result<(), LinkFailure> {
        Err(LinkFailure::NotFound)
    }
    fn disconnect(&mut self) {}
    fn capabilities(&mut self) -> Result<String, LinkError> {
        Err(not_connected())
    }
    fn read(&mut self, _code: u8) -> Result<VcpReading, LinkError> {
        Err(not_connected())
    }
    fn write(&mut self, _code: u8, _value: u16) -> Result<(), LinkError> {
        Err(not_connected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_link_is_never_found() {
        let mut link = NoLink;
        assert_eq!(link.connect(), Err(LinkFailure::NotFound));
        assert!(matches!(link.read(0x10), Err(LinkError::Gone(_))));
    }

    #[test]
    fn ascii_replaces_non_ascii() {
        assert_eq!(ascii(b"ab\xFFc"), "ab?c");
    }
}
