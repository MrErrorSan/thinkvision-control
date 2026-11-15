//! The contract every DDC backend implements (`tvc-ddc`). The engine is the only caller and owns the link on its
//! own thread, so implementations need `Send` but not `Sync`.

use std::fmt;

/// Why a link can't reach the monitor. Drives the banner and the setup hints.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LinkFailure {
    /// No matching monitor is attached.
    NotFound,
    /// The OS denied access to the monitor bus (Linux: /dev/i2c-* permissions).
    PermissionDenied,
    /// The OS lacks the bus driver (Linux: i2c-dev not loaded).
    ModuleMissing,
    /// The port the monitor is on can't carry DDC/CI (macOS: some built-in HDMI ports).
    UnsupportedPort,
}

/// Where a display the OS reports is: the laptop's own panel, a monitor plugged into a port, or a screen that
/// exists only in software (a virtual machine, a remote session). Only `External` displays are driven over DDC/CI;
/// a built-in panel can be driven through its backlight.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DisplayKind {
    BuiltIn,
    External,
    Virtual,
}

/// PNP manufacturer ids of the screens hypervisors and remote-desktop drivers present: Hyper-V, QEMU/virtio,
/// VirtualBox, VMware, Parallels, Citrix, Microsoft's remote display.
pub const VIRTUAL_VENDORS: [&str; 8] = ["MSH", "RHT", "QEM", "VBX", "VMW", "PRL", "CTX", "RDP"];

/// True when `edid_id` starts with one of [`VIRTUAL_VENDORS`].
pub fn is_virtual_vendor(edid_id: &str) -> bool {
    edid_id.get(..3).is_some_and(|pnp| VIRTUAL_VENDORS.iter().any(|v| v.eq_ignore_ascii_case(pnp)))
}

/// A failed command. The engine retries only `Transient`; `Unsupported` is final and `Gone` means reconnect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkError {
    /// Worth retrying: no reply, bad checksum, bus busy.
    Transient(String),
    /// Final: the monitor says the code is unsupported.
    Unsupported(String),
    /// The connection is gone (handle invalid, device removed); reconnect before trying again.
    Gone(String),
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LinkError::Transient(m) => write!(f, "{m}"),
            LinkError::Unsupported(m) => write!(f, "unsupported: {m}"),
            LinkError::Gone(m) => write!(f, "connection lost: {m}"),
        }
    }
}

impl std::error::Error for LinkError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VcpReading {
    pub current: u16,
    pub max: u16,
}

/// Raw DDC/CI access to one monitor. Not thread-safe; the engine serialises and paces every call
/// (50 ms gap, 400 ms settle after writes, retries), so implementations do only their own transport-level waits.
pub trait MonitorLink: Send {
    /// Finds the monitor and opens it, releasing any handle held before. Ok = ready for commands.
    fn connect(&mut self) -> Result<(), LinkFailure>;
    /// Releases the handle; the next command fails with `Gone` until `connect` succeeds again.
    fn disconnect(&mut self);
    /// The raw capability string, e.g. "(prot(monitor)type(LCD)model(P32p-20)...vcp(02 04...))".
    fn capabilities(&mut self) -> Result<String, LinkError>;
    fn read(&mut self, code: u8) -> Result<VcpReading, LinkError>;
    fn write(&mut self, code: u8, value: u16) -> Result<(), LinkError>;
}
