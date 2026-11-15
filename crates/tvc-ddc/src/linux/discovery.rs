//! Linux display discovery: each DRM connector mapped to its i2c bus, identity from the sysfs EDID.

/// DRM connectors with their i2c buses. Stub until w3-linux: lists nothing.
pub(crate) struct LinuxDiscovery;

impl LinuxDiscovery {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl tvc_core::identity::Discovery for LinuxDiscovery {
    fn enumerate(&mut self) -> Result<Vec<tvc_core::identity::DisplayInfo>, tvc_core::identity::ListError> {
        Err(tvc_core::identity::ListError::Unsupported)
    }

    fn open(&mut self, _target: &str) -> Result<Box<dyn tvc_core::link::MonitorLink>, tvc_core::link::LinkFailure> {
        Err(tvc_core::link::LinkFailure::NotFound)
    }
}
