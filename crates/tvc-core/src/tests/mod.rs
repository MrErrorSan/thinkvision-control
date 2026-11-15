//! Ports of the 0.0.4 xUnit tests, run against `FakeLink`.

mod cli_tests;
mod crash_tests;
mod engine_tests;
mod hub_tests;
mod i18n_tests;
mod identity_tests;
mod misc_tests;
mod parser_tests;
mod profile_tests;
mod safety_floor_tests;
mod schedule_tests;
mod settings_tests;
mod support_tests;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use crate::engine::{Command, Engine, EngineConfig, EngineHandle, Event, MonitorStatus, Origin, Timing, WriteResult};
use crate::fake::FakeLink;
use crate::identity::{MonitorId, MonitorIdentity};
use crate::link::{MonitorLink, VcpReading};
use crate::support::SupportLevel;

/// A real P40w-20: a nested list inside F7 and 0x60 advertised twice.
pub const P40W20_CAPABILITIES: &str = concat!(
    "(prot(monitor)type(LCD)model(P40w-20)cmds(01 02 03 07 0C E3 F3)vcp(02 04 05 08 10 12 14(01 05 06 08 0B) 16 18 ",
    "1A 52 60(0F 11 12 1B 31 FF) 62 6C 6E 70 86(01 02 05 0B) 87 8D(01 02) AC AE B2 B6 C0 C6 C8 C9 CA(01 02) ",
    "CC(02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E 12 14 16 17 1A 1E 1F 20 24) D6(01 04 05) DC(00 01 02 03 05 08 0B) DF ",
    "E0(00 01 02) E9 EA EC F7(04 07 08 12 14(00 02 03) 15 17) 60(0F 11 12 1B 31))mswhql(1)asset_eep(40)mccs_ver(2.2))",
);

pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir()
            .join(format!("tvc-core-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::SeqCst)));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// An engine on a `FakeLink`, with its events collected.
pub struct Harness {
    pub link: FakeLink,
    handle: Option<EngineHandle>,
    events: Receiver<Event>,
}

impl Harness {
    pub fn new(link: &FakeLink, dir: &Path) -> Self {
        Self::with_timing(link, dir, Timing::zero())
    }

    pub fn with_timing(link: &FakeLink, dir: &Path, timing: Timing) -> Self {
        Self::spawn(Box::new(link.clone()), link, dir, timing, false)
    }

    /// The engine on `engine_link` (a `FakeLink` or a wrapper around `fake`), optionally in safe mode.
    pub fn spawn(engine_link: Box<dyn MonitorLink>, fake: &FakeLink, dir: &Path, timing: Timing, safe_mode: bool) -> Self {
        let (tx, events) = mpsc::channel();
        let handle = Engine::spawn(engine_config(engine_link, dir, timing, safe_mode), Box::new(move |e| {
            let _ = tx.send(e);
        }));
        Self { link: fake.clone(), handle: Some(handle), events }
    }

    pub fn send(&self, command: Command) {
        self.handle.as_ref().unwrap().send(command);
    }

    /// Waits until every command sent so far is handled; returns the events since the last call.
    pub fn flush(&self) -> Vec<Event> {
        let (tx, rx) = mpsc::channel();
        self.send(Command::Flush(tx));
        rx.recv_timeout(Duration::from_secs(20)).expect("engine flushed");
        self.events.try_iter().collect()
    }

    pub fn run(&self, command: Command) -> Vec<Event> {
        self.send(command);
        self.flush()
    }

    pub fn connect(&self) -> Vec<Event> {
        self.run(Command::Connect)
    }

    /// Runs a command and returns its `Result`.
    pub fn result(&self, command: Command) -> WriteResult {
        results(&self.run(command)).pop().expect("a Result event")
    }

    pub fn set(&self, code: u8, value: u16, confirmed: bool) -> WriteResult {
        self.result(Command::Set { code, value, confirmed, origin: Origin::Window })
    }

    pub fn shutdown(mut self) {
        self.handle.take().unwrap().shutdown();
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.shutdown();
        }
    }
}

/// A P32p-20 engine config with the Verified level forced, in `dir`.
pub fn engine_config(link: Box<dyn MonitorLink>, dir: &Path, timing: Timing, safe_mode: bool) -> EngineConfig {
    EngineConfig {
        id: MonitorId("LEN62A2-STEST".into()),
        identity: MonitorIdentity { edid_id: "LEN62A2".into(), serial: Some("TEST".into()), connector: None },
        link,
        dir: dir.to_path_buf(),
        timing: Some(timing),
        safe_mode,
        level: Some(SupportLevel::Verified { profile: "p32p20".into() }),
        crash: None,
    }
}

pub fn statuses(events: &[Event]) -> Vec<MonitorStatus> {
    events.iter().filter_map(|e| match e { Event::Status { status, .. } => Some(*status), _ => None }).collect()
}

pub fn values(events: &[Event]) -> Vec<(u8, VcpReading)> {
    events.iter().filter_map(|e| match e { Event::Value { code, reading } => Some((*code, *reading)), _ => None }).collect()
}

pub fn results(events: &[Event]) -> Vec<WriteResult> {
    events.iter().filter_map(|e| match e { Event::Result { result, .. } => Some(result.clone()), _ => None }).collect()
}

pub fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "condition not met in 5 s");
        std::thread::sleep(Duration::from_millis(10));
    }
}
