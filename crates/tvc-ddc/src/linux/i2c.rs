//! DDC/CI over the kernel's i2c-dev nodes. A link from discovery is bound to one bus and checks on connect that its
//! connector still shows the same EDID in sysfs, then talks MCCS at 0x37 with plain write/read. Each write–sleep–read
//! sequence holds an advisory `flock` on the device file (what ddcutil 2.x locks too), so cooperating DDC clients
//! don't interleave on the bus.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::thread::sleep;
use std::time::{Duration, Instant};

use tvc_core::link::{LinkError, LinkFailure, MonitorLink, VcpReading};

use super::discovery::connector;
use super::{bus_name, edid_identity, writes_once};
use crate::{ascii, not_connected, packet};

const REPLY_DELAY: Duration = Duration::from_millis(50); // spec minimum is 40 ms between request and reply
const RETRY_GAP: Duration = Duration::from_millis(50);
const READ_ATTEMPTS: u32 = 10; // ddcutil's write-read default
const WRITE_ATTEMPTS: u32 = 4; // ddcutil's write-only default; 1 for `writes_once` codes
const FRAGMENT_ATTEMPTS: u32 = 8; // ddcutil's multi-part default
const LOCK_TIMEOUT: Duration = Duration::from_secs(2);
const EDID_ADDRESS: u16 = 0x50;

// <linux/i2c-dev.h>
const I2C_SLAVE: u32 = 0x0703;
const I2C_SLAVE_FORCE: u32 = 0x0706;

/// The i2c-dev link: one open device node while connected.
pub(crate) struct I2cLink {
    binding: Binding,
    held: Option<Bus>,
}

/// One connector's bus as discovery listed it, with the identity it showed then.
#[derive(Clone)]
pub(super) struct Binding {
    /// The sysfs connector name, `card0-DP-2`.
    pub(super) connector: String,
    /// `/dev/i2c-5`.
    pub(super) device: String,
    pub(super) edid_id: String,
    pub(super) serial: Option<String>,
}

struct Bus {
    file: File,
    path: String,
}

impl I2cLink {
    pub(super) fn bound(binding: Binding) -> Self {
        I2cLink { binding, held: None }
    }

    /// Runs `op` on the held bus; a `Gone` result drops the fd so the next connect opens it afresh.
    fn with_bus<T>(&mut self, op: impl FnOnce(&Bus) -> Result<T, LinkError>) -> Result<T, LinkError> {
        let result = op(self.held.as_ref().ok_or_else(not_connected)?);
        if matches!(result, Err(LinkError::Gone(_))) {
            self.held = None;
        }
        result
    }
}

impl MonitorLink for I2cLink {
    fn connect(&mut self) -> Result<(), LinkFailure> {
        self.held = None;
        self.held = Some(connect_bound(&self.binding)?);
        Ok(())
    }

    fn disconnect(&mut self) {
        self.held = None;
    }

    fn capabilities(&mut self) -> Result<String, LinkError> {
        self.with_bus(read_capabilities)
    }

    fn read(&mut self, code: u8) -> Result<VcpReading, LinkError> {
        self.with_bus(|bus| read_vcp(bus, code))
    }

    fn write(&mut self, code: u8, value: u16) -> Result<(), LinkError> {
        self.with_bus(|bus| write_vcp(bus, code, value))
    }
}

// ----- connecting -----

/// Reopens the bound bus, once its connector is still connected through that bus with the same EDID id and serial.
/// Reads only sysfs and opens only that one device node: no EDID traffic on any bus.
fn connect_bound(binding: &Binding) -> Result<Bus, LinkFailure> {
    let now = connector(&binding.connector, &[]).ok_or(LinkFailure::NotFound)?;
    let (edid_id, serial) = edid_identity(&now.edid);
    let same = now.bus.as_deref() == Some(bus_name(&binding.device))
        && edid_id == binding.edid_id
        && serial == binding.serial;
    if !same {
        return Err(LinkFailure::NotFound);
    }
    match open_fresh(&binding.device) {
        Ok(file) => Ok(Bus { file, path: binding.device.clone() }),
        Err(e) if is_permission(errno_of(&e)) => Err(LinkFailure::PermissionDenied),
        Err(_) if device_nodes().is_empty() => Err(LinkFailure::ModuleMissing),
        Err(_) => Err(LinkFailure::NotFound),
    }
}

/// Reads the EDID base block at 0x50 on `bus` (`i2c-5`) under the bus lock, for the NVIDIA fallback in discovery.
/// Err is the errno.
pub(super) fn read_edid_block(bus: &str) -> Result<Vec<u8>, i32> {
    let path = format!("/dev/{bus}");
    let file = open_rw(&path).map_err(|e| errno_of(&e))?;
    let bus = Bus { file, path };
    let _lock = lock(&bus).map_err(|_| libc::EBUSY)?;
    set_slave(&bus.file, EDID_ADDRESS)?;
    write_all(&bus.file, &[0])?; // offset 0
    let mut block = vec![0u8; 128];
    match (&bus.file).read(&mut block) {
        Ok(128) => Ok(block),
        Ok(_) => Err(libc::EIO),
        Err(e) => Err(errno_of(&e)),
    }
}

// ----- commands -----

fn read_vcp(bus: &Bus, code: u8) -> Result<VcpReading, LinkError> {
    let request = request(&packet::get_vcp(code, true));
    let mut reply = [0u8; packet::VCP_REPLY_LEN];
    let mut last_errno = 0;
    let mut last_error = String::from("no valid reply");
    let mut nulls = 0;
    {
        let _lock = lock(bus)?;
        for attempt in 1..=READ_ATTEMPTS {
            if attempt > 1 {
                sleep(RETRY_GAP);
            }
            if let Err(e) = transaction(&bus.file, &request, &mut reply) {
                (last_errno, last_error) = (e, describe(e));
                if is_gone(e) {
                    break;
                }
                continue;
            }
            last_errno = 0;
            if let Some(reading) = packet::parse_vcp_reply(&reply, code) {
                return Ok(reading);
            }
            if packet::is_unsupported_reply(&reply, code) {
                return Err(LinkError::Unsupported(format!("VCP 0x{code:02X} is not supported by the monitor")));
            }
            if packet::is_null_message(&reply) {
                nulls += 1;
                if nulls == packet::NULL_REPLY_LIMIT {
                    return Err(LinkError::Unsupported(format!("VCP 0x{code:02X}: the monitor sends only null replies")));
                }
            }
            last_error = format!("invalid reply {}", hex(&reply));
        }
    }
    Err(fail(
        format!("Get VCP 0x{code:02X} on {} failed after {READ_ATTEMPTS} attempts ({last_error})", bus.path),
        last_errno,
    ))
}

fn write_vcp(bus: &Bus, code: u8, value: u16) -> Result<(), LinkError> {
    // No sleep after the write: the engine's post-write settle covers it.
    let request = request(&packet::set_vcp(code, value, true));
    let attempts = if writes_once(code) { 1 } else { WRITE_ATTEMPTS };
    let mut last_errno = 0;
    {
        let _lock = lock(bus)?;
        for attempt in 1..=attempts {
            if attempt > 1 {
                sleep(RETRY_GAP);
            }
            let sent = set_slave(&bus.file, packet::MONITOR_ADDRESS.into()).and_then(|()| write_all(&bus.file, &request));
            match sent {
                Ok(()) => return Ok(()),
                Err(e) => {
                    last_errno = e;
                    if is_gone(e) {
                        break;
                    }
                }
            }
        }
    }
    Err(fail(
        format!(
            "Set VCP 0x{code:02X}={value} on {} failed after {attempts} attempt(s) ({})",
            bus.path,
            describe(last_errno)
        ),
        last_errno,
    ))
}

fn read_capabilities(bus: &Bus) -> Result<String, LinkError> {
    let mut text = Vec::new();
    let mut reply = [0u8; packet::FRAGMENT_REPLY_LEN];
    loop {
        if text.len() >= packet::MAX_CAPABILITIES_LEN {
            return Err(LinkError::Transient(format!(
                "capabilities too long (over {} bytes on {})",
                packet::MAX_CAPABILITIES_LEN,
                bus.path
            )));
        }
        let offset = text.len() as u16;
        let request = request(&packet::capabilities_request(offset, true));
        let mut last_errno = 0;
        let mut last_error = String::from("no valid reply");
        let mut fragment = None;
        {
            let _lock = lock(bus)?;
            for attempt in 1..=FRAGMENT_ATTEMPTS {
                if attempt > 1 {
                    sleep(RETRY_GAP);
                }
                if let Err(e) = transaction(&bus.file, &request, &mut reply) {
                    (last_errno, last_error) = (e, describe(e));
                    if is_gone(e) {
                        break;
                    }
                    continue;
                }
                last_errno = 0;
                match packet::parse_capabilities_fragment(&reply, offset) {
                    Some(data) => {
                        fragment = Some(data.to_vec());
                        break;
                    }
                    None => last_error = format!("invalid reply {}", hex(&reply)),
                }
            }
        }
        let Some(fragment) = fragment else {
            return Err(fail(
                format!(
                    "Capabilities at offset {offset} on {} failed after {FRAGMENT_ATTEMPTS} attempts ({last_error})",
                    bus.path
                ),
                last_errno,
            ));
        };
        if fragment.is_empty() {
            break;
        }
        text.extend_from_slice(&fragment);
        sleep(RETRY_GAP); // inter-command gap between fragments (the engine only spaces whole operations)
    }
    Ok(ascii(&text).trim_end_matches('\0').to_owned())
}

/// A command that ran out of attempts. A final bus error means the bus stopped acknowledging or vanished: `Gone`, so
/// the link drops the fd and the engine reconnects. Anything else is worth retrying.
fn fail(message: String, last_errno: i32) -> LinkError {
    if is_bus_failure(last_errno) {
        LinkError::Gone(message)
    } else {
        LinkError::Transient(message)
    }
}

// ----- transport -----

/// The bytes written after `I2C_SLAVE 0x37`: the 0x51 host source byte, then the packet.
fn request(payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(payload.len() + 1);
    bytes.push(packet::HOST_SOURCE);
    bytes.extend_from_slice(payload);
    bytes
}

/// Write the request, wait, read the reply. read(2) at 0x37 returns the reply starting with the monitor's 0x6E source
/// byte. The reply length is fixed by the caller: the I²C master clocks out that many bytes whatever the monitor sends.
fn transaction(file: &File, request: &[u8], reply: &mut [u8]) -> Result<(), i32> {
    reply.fill(0);
    set_slave(file, packet::MONITOR_ADDRESS.into())?;
    write_all(file, request)?;
    sleep(REPLY_DELAY);
    (&*file).read(reply).map(drop).map_err(|e| errno_of(&e))
}

fn write_all(file: &File, bytes: &[u8]) -> Result<(), i32> {
    match (&*file).write(bytes) {
        Ok(n) if n == bytes.len() => Ok(()),
        Ok(_) => Err(libc::EIO), // i2c-dev writes all or nothing; treat a short write as a NAK
        Err(e) => Err(errno_of(&e)),
    }
}

/// Points the fd at a 7-bit slave address, forcing it if a kernel driver already claims the address (as ddcutil does).
fn set_slave(file: &File, address: u16) -> Result<(), i32> {
    match ioctl(file, I2C_SLAVE, address) {
        Err(libc::EBUSY) => ioctl(file, I2C_SLAVE_FORCE, address),
        other => other,
    }
}

fn ioctl(file: &File, request: u32, arg: u16) -> Result<(), i32> {
    // SAFETY: I2C_SLAVE(_FORCE) takes the address as a plain integer argument; the fd is owned by `file`.
    let ret = unsafe { libc::ioctl(file.as_raw_fd(), request as libc::Ioctl, libc::c_ulong::from(arg)) };
    if ret == 0 { Ok(()) } else { Err(last_errno()) }
}

/// Holds the advisory `flock` on a device file until dropped.
struct BusLock<'a>(Option<&'a File>);

impl Drop for BusLock<'_> {
    fn drop(&mut self) {
        if let Some(file) = self.0 {
            // SAFETY: plain flock on an fd owned by `file`, which outlives the guard.
            unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
        }
    }
}

/// Takes the advisory lock on the device file, polling so a stuck peer can't hang the DDC thread. Carries on without
/// a lock if the file system doesn't support flock.
fn lock(bus: &Bus) -> Result<BusLock<'_>, LinkError> {
    let start = Instant::now();
    loop {
        // SAFETY: plain flock on an fd owned by `bus.file`.
        if unsafe { libc::flock(bus.file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(BusLock(Some(&bus.file)));
        }
        let errno = last_errno();
        if errno != libc::EWOULDBLOCK && errno != libc::EINTR {
            return Ok(BusLock(None));
        }
        if start.elapsed() > LOCK_TIMEOUT {
            return Err(LinkError::Transient(format!("{} is busy (another DDC client holds its lock)", bus.path)));
        }
        sleep(Duration::from_millis(10));
    }
}

// ----- device nodes -----

/// All `/dev/i2c-*` nodes; none means the i2c-dev module isn't loaded.
fn device_nodes() -> Vec<String> {
    let Ok(entries) = fs::read_dir("/dev") else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| name.starts_with("i2c-"))
        .map(|name| format!("/dev/{name}"))
        .collect()
}

/// Opens the node; a permission error on a node created in the last few seconds (a dock just plugged in) is retried
/// for up to 2 s, since the kernel's hotplug event can arrive before udev has given the session its ACL.
fn open_fresh(path: &str) -> io::Result<File> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match open_rw(path) {
            Err(e) if is_permission(errno_of(&e)) && is_new_node(path) && Instant::now() < deadline => {
                sleep(Duration::from_millis(250));
            }
            other => return other,
        }
    }
}

/// The node's inode changed (created, or its ACL set) less than 10 s ago.
fn is_new_node(path: &str) -> bool {
    use std::os::unix::fs::MetadataExt;
    let Ok(meta) = fs::metadata(path) else { return false };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    now.saturating_sub(u64::try_from(meta.ctime()).unwrap_or(0)) < 10
}

// ----- errno helpers -----

fn open_rw(path: &str) -> io::Result<File> {
    OpenOptions::new().read(true).write(true).open(path) // std opens with O_CLOEXEC
}

fn errno_of(e: &io::Error) -> i32 {
    e.raw_os_error().unwrap_or(libc::EIO)
}

fn last_errno() -> i32 {
    errno_of(&io::Error::last_os_error())
}

fn describe(errno: i32) -> String {
    io::Error::from_raw_os_error(errno).to_string()
}

pub(super) fn is_permission(errno: i32) -> bool {
    errno == libc::EACCES || errno == libc::EPERM
}

/// The adapter itself is gone (unplugged dock, driver unbound): retrying is pointless.
fn is_gone(errno: i32) -> bool {
    errno == libc::ENODEV || errno == libc::ENOENT
}

/// The bus stopped acknowledging or vanished, as opposed to garbled-but-present replies.
fn is_bus_failure(errno: i32) -> bool {
    matches!(errno, libc::ENXIO | libc::EIO | libc::EREMOTEIO | libc::ENODEV | libc::ENOENT)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}
