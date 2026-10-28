//! An in-memory P32p-20 for tests and `--demo`: the real capability string and the values read on 2026-10-03.
//! Clones share state, so a test keeps one clone to script and inspect the link the engine owns.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use crate::link::{LinkError, LinkFailure, MonitorLink, VcpReading};

pub const P32P20_CAPABILITIES: &str = concat!(
    "(prot(monitor)type(LCD)model(ThinkVision P32p-20)cmds(01 02 03 07 0C E3 F3)vcp(02 04(4C 42 31 2E 32) 05 08 10 12 ",
    "14(01 05 06 08 0B 0E 0F 12) 16 18 1A 52 60(11 0F 31) 86(02 05) AC AE B2 B6 ",
    "C6(02 01 C3 00 A0 CA 00 F8 00 EA 00 00 DC 00 46 00 00) C8 C9 CA(01 02) CC(02 03 04 05 06 09 0A 0D) D6(01 04 05) DF ",
    "E0(00 01 02) EA(00 01) EB(00 01) EC(00 01) F6(07) F7(02 04 07 08 0D) F8(02 04 07 08 0D) FA(10 12 EA 14))",
    "mswhql(1)asset_eep(40)mccs_ver(2.2))",
);

#[derive(Debug)]
pub struct FakeState {
    pub values: BTreeMap<u8, VcpReading>,
    pub writes: Vec<(u8, u16)>,
    /// Every read attempt, by code (retries included).
    pub reads: Vec<u8>,
    pub capability_reads: usize,
    pub connects: usize,
    /// Writes to these codes are accepted but not applied (e.g. brightness locked by a colour mode).
    pub ignored_writes: BTreeSet<u8>,
    /// Every DDC call fails, like a monitor with DDC/CI switched off.
    pub dead: bool,
    /// Not plugged in: connect fails with `failure_when_absent`.
    pub absent: bool,
    pub failure_when_absent: LinkFailure,
    /// The monitor is plugged in on this connect attempt (1-based).
    pub plug_in_at_connect: Option<usize>,
    /// Every call takes this long (a slow monitor).
    pub delay: Duration,
    pub connected: bool,
}

impl Default for FakeState {
    fn default() -> Self {
        let r = |current, max| VcpReading { current, max };
        Self {
            values: BTreeMap::from([
                (0x10, r(0, 100)), (0x12, r(70, 100)), (0x14, r(6, 14)), (0x16, r(100, 100)), (0x18, r(100, 100)),
                (0x1A, r(100, 100)), (0x60, r(0x11, 49)), (0x86, r(2, 5)), (0xCC, r(2, 13)), (0xD6, r(1, 5)),
                (0xE0, r(1, 2)), (0xEA, r(0, 1)), (0xEB, r(0, 1)), (0xEC, r(0, 1)),
            ]),
            writes: Vec::new(),
            reads: Vec::new(),
            capability_reads: 0,
            connects: 0,
            ignored_writes: BTreeSet::new(),
            dead: false,
            absent: false,
            failure_when_absent: LinkFailure::NotFound,
            plug_in_at_connect: None,
            delay: Duration::ZERO,
            connected: false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct FakeLink {
    state: Arc<Mutex<FakeState>>,
}

impl FakeLink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(&self) -> MutexGuard<'_, FakeState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Sleeps `delay` without holding the lock, then locks.
    fn call(&self) -> MutexGuard<'_, FakeState> {
        let delay = self.state().delay;
        std::thread::sleep(delay);
        self.state()
    }
}

impl MonitorLink for FakeLink {
    fn connect(&mut self) -> Result<(), LinkFailure> {
        let mut s = self.call();
        s.connects += 1;
        if s.plug_in_at_connect == Some(s.connects) {
            s.absent = false;
        }
        s.connected = !s.absent;
        if s.absent { Err(s.failure_when_absent) } else { Ok(()) }
    }

    fn disconnect(&mut self) {
        self.state().connected = false;
    }

    fn capabilities(&mut self) -> Result<String, LinkError> {
        let mut s = self.call();
        s.capability_reads += 1;
        if s.dead || !s.connected {
            return Err(LinkError::Transient("no reply".into()));
        }
        Ok(P32P20_CAPABILITIES.to_string())
    }

    fn read(&mut self, code: u8) -> Result<VcpReading, LinkError> {
        let mut s = self.call();
        s.reads.push(code);
        match s.values.get(&code) {
            Some(&r) if !s.dead && s.connected => Ok(r),
            _ => Err(LinkError::Transient(format!("read 0x{code:02X}"))),
        }
    }

    fn write(&mut self, code: u8, value: u16) -> Result<(), LinkError> {
        let mut s = self.call();
        if s.dead || !s.connected {
            return Err(LinkError::Transient(format!("write 0x{code:02X}")));
        }
        s.writes.push((code, value));
        if !s.ignored_writes.contains(&code) {
            if let Some(r) = s.values.get_mut(&code) {
                r.current = value;
            }
        }
        Ok(())
    }
}
