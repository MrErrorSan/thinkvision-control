//! Ports of the 0.0.4 xUnit tests, run against `FakeLink`.

mod cli_tests;
mod crash_tests;
mod engine_tests;
mod misc_tests;
mod profile_tests;
mod schedule_tests;
mod settings_tests;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use crate::engine::{Command, Engine, EngineHandle, Event, MonitorStatus, Origin, Timing, WriteResult};
use crate::fake::FakeLink;
use crate::link::{MonitorLink, VcpReading};
use crate::profile::ProfileStore;

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
        let handle = Engine::spawn(engine_link, ProfileStore::new(dir), dir.to_path_buf(), timing, safe_mode,
            Box::new(move |e| {
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
