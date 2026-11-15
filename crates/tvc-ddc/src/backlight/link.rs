//! The `MonitorLink` over one OS route to a panel's brightness, in percent.

use tvc_core::link::{LinkError, LinkFailure, MonitorLink, VcpReading};

use super::CAPABILITIES;
use crate::not_connected;

const BRIGHTNESS: u8 = 0x10;

/// One OS route to a built-in panel's brightness.
pub(super) trait Panel: Send {
    /// Opens the panel, releasing anything held before.
    fn open(&mut self) -> Result<(), LinkFailure>;
    fn close(&mut self);
    /// The current brightness in percent.
    fn get(&mut self) -> Result<u16, LinkError>;
    /// Sets the brightness to `percent` (0..=100).
    fn set(&mut self, percent: u16) -> Result<(), LinkError>;
}

pub(super) struct BacklightLink<P> {
    panel: P,
    connected: bool,
}

impl<P: Panel> BacklightLink<P> {
    pub(super) fn new(panel: P) -> Self {
        Self { panel, connected: false }
    }

    fn ready(&self, code: u8) -> Result<(), LinkError> {
        if !self.connected {
            return Err(not_connected());
        }
        if code != BRIGHTNESS {
            return Err(LinkError::Unsupported(format!("VCP 0x{code:02X}: a built-in panel offers only brightness")));
        }
        Ok(())
    }
}

impl<P: Panel> MonitorLink for BacklightLink<P> {
    fn connect(&mut self) -> Result<(), LinkFailure> {
        self.disconnect();
        self.panel.open()?;
        self.connected = true;
        Ok(())
    }

    fn disconnect(&mut self) {
        if self.connected {
            self.panel.close();
            self.connected = false;
        }
    }

    fn capabilities(&mut self) -> Result<String, LinkError> {
        self.ready(BRIGHTNESS)?;
        Ok(CAPABILITIES.to_owned())
    }

    fn read(&mut self, code: u8) -> Result<VcpReading, LinkError> {
        self.ready(code)?;
        Ok(VcpReading { current: self.panel.get()?.min(100), max: 100 })
    }

    fn write(&mut self, code: u8, value: u16) -> Result<(), LinkError> {
        self.ready(code)?;
        self.panel.set(value.min(100))
    }
}
