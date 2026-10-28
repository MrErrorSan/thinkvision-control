//! DDC/CI over the kernel's i2c-dev nodes. Finds the target by EDID (manufacturer + product code) at 0x50, then talks
//! MCCS at 0x37 with plain write/read. Each write–sleep–read sequence holds an advisory `flock` on the device file (what
//! ddcutil 2.x locks too), so cooperating DDC clients don't interleave on the bus.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use tvc_core::link::{LinkError, LinkFailure, MonitorLink, MonitorMatch, VcpReading};

use super::{bus_name, candidate_buses, is_ignorable_adapter};
use crate::{ascii, edid, not_connected, packet};

const REPLY_DELAY: Duration = Duration::from_millis(50); // spec minimum is 40 ms between request and reply
const RETRY_GAP: Duration = Duration::from_millis(50);
const READ_ATTEMPTS: u32 = 10; // ddcutil's write-read default
const WRITE_ATTEMPTS: u32 = 4; // ddcutil's write-only default
const FRAGMENT_ATTEMPTS: u32 = 8; // ddcutil's multi-part default
const PROBE_ATTEMPTS: u32 = 3;
const MAX_CAPABILITIES_LEN: usize = 1024;
const LOCK_TIMEOUT: Duration = Duration::from_secs(2);
const PROBE_CODE: u8 = 0x10; // brightness: every MCCS monitor answers it
const EDID_ADDRESS: u16 = 0x50;
const EDID_LEN: usize = 128;

// <linux/i2c-dev.h>
const I2C_SLAVE: u32 = 0x0703;
const I2C_SLAVE_FORCE: u32 = 0x0706;

/// The i2c-dev link: one open device node while connected.
pub(crate) struct I2cLink {
    target: MonitorMatch,
    held: Option<Bus>,
}

struct Bus {
    file: File,
    path: String,
}

impl I2cLink {
    pub(crate) fn new(target: MonitorMatch) -> Self {
        I2cLink { target, held: None }
    }

    /// Runs `op` on the held bus; a `Gone` result drops the fd so the next connect re-scans (bus numbers can change
    /// after a dock replug).
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
        let Some((pnp, product)) = self.target.pnp_and_product() else {
            return Err(LinkFailure::NotFound);
        };

        let nodes = device_nodes();
        if nodes.is_empty() {
            return Err(LinkFailure::ModuleMissing);
        }

        let linked = drm_ddc_links();
        let probe: Vec<&String> = nodes
            .iter()
            .filter(|d| linked.iter().any(|l| l == bus_name(d)) || !is_ignorable_adapter(&adapter_name(d)))
            .collect();

        let mut permission_denied = false;
        let mut matches = Vec::new();
        for bus in candidate_buses(&probe, &linked) {
            match read_edid(&bus) {
                Ok(Some(data)) if edid::matches(&data, pnp, product) => matches.push(bus),
                Err(e) if is_permission(e) => permission_denied = true,
                _ => {}
            }
        }

        // Behind an MST hub the same EDID can show up on more than one bus while only one carries DDC/CI:
        // keep the first that answers a brightness read, else the first that opened.
        let mut opened = Vec::new();
        let mut chosen = None;
        for path in &matches {
            let file = match open_rw(path) {
                Ok(f) => f,
                Err(e) => {
                    permission_denied |= is_permission(errno_of(&e));
                    continue;
                }
            };
            let bus = Bus { file, path: path.clone() };
            if matches.len() == 1 || probe_bus(&bus) {
                chosen = Some(bus);
                break;
            }
            opened.push(bus);
        }
        // The rest close on drop.
        self.held = chosen.or_else(|| (!opened.is_empty()).then(|| opened.swap_remove(0)));

        match self.held {
            Some(_) => Ok(()),
            None if permission_denied => Err(LinkFailure::PermissionDenied),
            None => Err(LinkFailure::NotFound),
        }
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

// ----- commands -----

fn read_vcp(bus: &Bus, code: u8) -> Result<VcpReading, LinkError> {
    let request = request(&packet::get_vcp(code, true));
    let mut reply = [0u8; packet::VCP_REPLY_LEN];
    let mut last_errno = 0;
    let mut last_error = String::from("no valid reply");
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
    let mut last_errno = 0;
    {
        let _lock = lock(bus)?;
        for attempt in 1..=WRITE_ATTEMPTS {
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
            "Set VCP 0x{code:02X}={value} on {} failed after {WRITE_ATTEMPTS} attempts ({})",
            bus.path,
            describe(last_errno)
        ),
        last_errno,
    ))
}

fn read_capabilities(bus: &Bus) -> Result<String, LinkError> {
    let mut text = Vec::new();
    let mut reply = [0u8; packet::FRAGMENT_REPLY_LEN];
    while text.len() < MAX_CAPABILITIES_LEN {
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

/// Does the bus answer a Get VCP brightness? Used only to choose between buses with the same EDID.
fn probe_bus(bus: &Bus) -> bool {
    let Ok(_lock) = lock(bus) else { return false };
    let request = request(&packet::get_vcp(PROBE_CODE, true));
    let mut reply = [0u8; packet::VCP_REPLY_LEN];
    (1..=PROBE_ATTEMPTS).any(|attempt| {
        if attempt > 1 {
            sleep(RETRY_GAP);
        }
        transaction(&bus.file, &request, &mut reply).is_ok() && packet::parse_vcp_reply(&reply, PROBE_CODE).is_some()
    })
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

/// Reads the 128-byte EDID base block at 0x50: `I2C_SLAVE 0x50`, write offset `00`, read 128. `Ok(None)` on a short
/// read; `Err(errno)` when a call fails.
fn read_edid(device: &str) -> Result<Option<Vec<u8>>, i32> {
    let file = open_rw(device).map_err(|e| errno_of(&e))?;
    set_slave(&file, EDID_ADDRESS)?;
    write_all(&file, &[0x00])?;
    let mut data = vec![0u8; EDID_LEN];
    let read = (&file).read(&mut data).map_err(|e| errno_of(&e))?;
    Ok((read == EDID_LEN).then_some(data))
}

// ----- discovery -----

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

/// The adapter names (`i2c-N`) that `/sys/class/drm/*/ddc` symlinks point at.
fn drm_ddc_links() -> Vec<String> {
    let Ok(entries) = fs::read_dir("/sys/class/drm") else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|e| fs::read_link(e.path().join("ddc")).ok())
        .filter_map(|target| target.file_name()?.to_str().map(str::to_owned))
        .filter(|name| name.starts_with("i2c-"))
        .collect()
}

fn adapter_name(device: &str) -> String {
    let path = Path::new("/sys/bus/i2c/devices").join(bus_name(device)).join("name");
    fs::read_to_string(path).map(|s| s.trim().to_owned()).unwrap_or_default()
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

fn is_permission(errno: i32) -> bool {
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
