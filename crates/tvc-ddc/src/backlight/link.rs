//! The `MonitorLink` over one OS route to a panel's brightness, in percent.

use std::cmp::Ordering;

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
    /// Sets the brightness to the panel's level for `percent` (0..=100): the nearest for `Equal`, else the first
    /// at or past it `toward` the move (`nearest_level`, `from_percent`). The percent `get` will then read.
    fn set(&mut self, percent: u16, toward: Ordering) -> Result<u16, LinkError>;
}

/// The panel snaps a write to one of its levels. The link moves a write toward its direction (so a step from 50 by
/// +5 on 10% levels goes to 60, not back to 50) and reads back the percent asked for while the panel is still at the
/// level that write set, so the engine's read-back check doesn't report a snap as "kept".
pub(super) struct BacklightLink<P> {
    panel: P,
    connected: bool,
    /// The last percent read or written, on the requested scale: what a move's direction is measured from.
    last: Option<u16>,
    /// The last write's percent and the panel level (in percent) it set.
    requested: Option<(u16, u16)>,
}

impl<P: Panel> BacklightLink<P> {
    pub(super) fn new(panel: P) -> Self {
        Self { panel, connected: false, last: None, requested: None }
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
        let level = self.panel.get()?.min(100);
        let current = match self.requested {
            Some((asked, set)) if set == level => asked,
            _ => {
                self.requested = None; // moved since (the OS, a key): the panel's own value
                level
            }
        };
        self.last = Some(current);
        Ok(VcpReading { current, max: 100 })
    }

    fn write(&mut self, code: u8, value: u16) -> Result<(), LinkError> {
        self.ready(code)?;
        let value = value.min(100);
        let toward = self.last.map_or(Ordering::Equal, |last| value.cmp(&last));
        let set = self.panel.set(value, toward)?.min(100);
        self.requested = Some((value, set));
        self.last = Some(value);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backlight::nearest_level;

    /// A panel with 10% levels.
    struct Stepped(u16);

    impl Panel for Stepped {
        fn open(&mut self) -> Result<(), LinkFailure> {
            Ok(())
        }

        fn close(&mut self) {}

        fn get(&mut self) -> Result<u16, LinkError> {
            Ok(self.0)
        }

        fn set(&mut self, percent: u16, toward: Ordering) -> Result<u16, LinkError> {
            let levels: Vec<u8> = (0..=10).map(|n| n * 10).collect();
            self.0 = nearest_level(&levels, percent, toward).unwrap().into();
            Ok(self.0)
        }
    }

    fn link(at: u16) -> BacklightLink<Stepped> {
        let mut link = BacklightLink::new(Stepped(at));
        link.connect().unwrap();
        link
    }

    #[test]
    fn steps_move_through_the_levels_and_read_back_as_asked() {
        let mut link = link(50);
        assert_eq!(link.read(BRIGHTNESS).unwrap().current, 50);
        let mut seen = vec![];
        for _ in 0..4 {
            let current = link.read(BRIGHTNESS).unwrap().current;
            link.write(BRIGHTNESS, current + 5).unwrap();
            seen.push((link.read(BRIGHTNESS).unwrap().current, link.panel.0));
        }
        // Every step reads back what it asked (no "kept"), and the panel never falls below it.
        assert_eq!(seen, [(55, 60), (60, 60), (65, 70), (70, 70)]);
        link.write(BRIGHTNESS, 65).unwrap();
        assert_eq!((link.read(BRIGHTNESS).unwrap().current, link.panel.0), (65, 60));
    }

    #[test]
    fn a_change_made_elsewhere_reads_as_the_panels_level() {
        let mut link = link(50);
        link.read(BRIGHTNESS).unwrap();
        link.write(BRIGHTNESS, 55).unwrap();
        link.panel.0 = 30; // the OS's brightness keys
        assert_eq!(link.read(BRIGHTNESS).unwrap().current, 30);
        link.write(BRIGHTNESS, 25).unwrap();
        assert_eq!((link.read(BRIGHTNESS).unwrap().current, link.panel.0), (25, 20));
    }
}
