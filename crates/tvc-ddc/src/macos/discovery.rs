//! macOS display discovery: one IOAVService per display, identity from the IODisplay EDID bindings.

/// One entry per IOAVService display. Stub until w3-macos: lists nothing.
pub(crate) struct MacDiscovery;

impl MacDiscovery {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl tvc_core::identity::Discovery for MacDiscovery {
    fn enumerate(&mut self) -> Result<Vec<tvc_core::identity::DisplayInfo>, tvc_core::identity::ListError> {
        Err(tvc_core::identity::ListError::Unsupported)
    }

    fn open(&mut self, _target: &str) -> Result<Box<dyn tvc_core::link::MonitorLink>, tvc_core::link::LinkFailure> {
        Err(tvc_core::link::LinkFailure::NotFound)
    }
}
