//! One engine per attached monitor, behind one handle. The hub enumerates displays through [`Discovery`], spawns
//! and stops engines as monitors come and go, keeps each monitor's id stable while it stays attached, and fans
//! `Target::All` commands out as one batch.
//!
//! **Ids stay stable while a monitor is attached.** A monitor is known by its EDID id, serial and connector (else
//! its target); it keeps the id it got when it was attached for as long as it stays in the list. Fresh ids come
//! from [`identity::assign_ids`] and are handed only to newly attached monitors, so plugging in a second monitor
//! with the same serial doesn't change the first one's id: an `-S` id never becomes `-P` while attached.
//!
//! **Folders.** Each engine works in `<root>/monitors/<id folder>/`, with an `identity.json` there so saved state
//! can be matched later. An unstable `-N` id gets a folder in the temp directory instead, removed at shutdown,
//! so nothing that enables writes is persisted under it. The 0.0.8 files in the root move, once, into the folder
//! of the first LEN62A2 that [`identity::match_saved`] matches to them.
//!
//! **Engines start unconnected:** send `Connect` (or `ConnectReading`) through `Send` once a monitor is listed.
//! Each engine runs on its own thread; the hub thread never waits on a monitor, except at shutdown.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::engine::{
    Command, Engine, EngineConfig, EngineHandle, Event, Origin, PowerState, Timing, WriteOutcome, WriteResult,
    SHUTDOWN_WAIT,
};
use crate::identity::{self, BackendKind, DisplayInfo, Discovery, ListError, MonitorId, MonitorIdentity};
use crate::log;
use crate::result::ResultKind;
use crate::store::write_atomic;
use crate::support::SupportLevel;

/// The monitor the 0.0.8 files in the data directory belong to.
const LEGACY_EDID: &str = "LEN62A2";

/// Allocated by the hub, monotonic for the life of the process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OpId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    One(MonitorId),
    All,
}

/// An attached monitor as the hub reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Monitor {
    pub id: MonitorId,
    pub info: DisplayInfo,
}

pub enum HubCommand {
    /// `Target::All` is accepted only for Set, Step, DragTo, EndDrag and ApplyPreset; anything else with `All` is
    /// refused (a `Batch` with no results). Power, Flush and Shutdown never go through `Send` with `All`.
    Send { target: Target, op: OpId, command: Box<Command> },
    /// Enumerate again: spawn engines for new monitors, stop the ones that left.
    Rescan,
    /// Broadcast to every engine.
    Power(PowerState),
    Shutdown,
}

#[derive(Clone, Debug)]
pub enum HubEvent {
    /// The current list after every rescan, every display (uncontrollable ones too) in enumeration order; replaces
    /// `Event::Displays`. Sent after the rescan's `Detached` events and before its `Attached` ones.
    Monitors(Result<Vec<Monitor>, ListError>),
    Attached(Monitor),
    Detached(MonitorId),
    /// An engine's own event, tagged with its monitor. The results of a `Target::All` command come only in its
    /// `Batch`.
    Monitor { id: MonitorId, event: Event },
    /// Every result of one `Target::All` command, by monitor id, once all engines answered.
    Batch { op: OpId, origin: Origin, code: Option<u8>, results: Vec<(MonitorId, WriteResult)> },
}

/// How the hub starts.
pub struct HubConfig {
    pub discovery: Box<dyn Discovery>,
    /// The data directory; each monitor gets `monitors/<id folder>/`.
    pub root: PathBuf,
    pub safe_mode: bool,
}

pub struct MonitorHub;

impl MonitorHub {
    /// Starts the hub thread and returns once its first `Rescan` has run (so the 0.0.8 files have moved).
    pub fn spawn(config: HubConfig, sink: Box<dyn Fn(HubEvent) + Send>) -> HubHandle {
        let sink: HubSink = Arc::new(Mutex::new(sink));
        let (tx, rx) = mpsc::channel();
        let (done_tx, done) = mpsc::channel::<()>();
        let (ready_tx, ready) = mpsc::channel::<()>();
        let hub = Hub {
            discovery: config.discovery,
            root: config.root,
            safe_mode: config.safe_mode,
            sink: sink.clone(),
            batches: Arc::new(Mutex::new(BTreeMap::new())),
            running: Vec::new(),
            migrated: false,
            display_off: false,
            locked: false,
            suspended: false,
        };
        let spawned = thread::Builder::new().name("tvc-hub".into()).spawn(move || {
            let _done = done_tx;
            hub.run(&rx, ready_tx);
        });
        if let Err(e) = spawned {
            log::write(&format!("Hub: couldn't start its thread ({e})"));
            emit(&sink, HubEvent::Monitors(Err(ListError::Failed(format!("couldn't start the hub thread: {e}")))));
        }
        let _ = ready.recv();
        HubHandle { tx, next: AtomicU64::new(1), done }
    }
}

pub struct HubHandle {
    tx: Sender<HubCommand>,
    next: AtomicU64,
    done: Receiver<()>,
}

impl HubHandle {
    /// A fresh op id for a `Send`.
    pub fn next_op(&self) -> OpId {
        OpId(self.next.fetch_add(1, Ordering::SeqCst))
    }

    pub fn send(&self, command: HubCommand) {
        let _ = self.tx.send(command);
    }

    /// Stops every engine (each waits up to `engine::SHUTDOWN_WAIT`, all at once) and the hub thread.
    pub fn shutdown(self) {
        let _ = self.tx.send(HubCommand::Shutdown);
        let _ = self.done.recv_timeout(SHUTDOWN_WAIT * 2);
    }
}

type HubSink = Arc<Mutex<Box<dyn Fn(HubEvent) + Send>>>;
type Batches = Arc<Mutex<BTreeMap<OpId, Pending>>>;

/// A `Target::All` command waiting for its engines.
struct Pending {
    origin: Origin,
    code: Option<u8>,
    waiting: BTreeSet<MonitorId>,
    results: Vec<(MonitorId, WriteResult)>,
}

struct Running {
    /// EDID id, serial and connector (else target): the same monitor on the same port.
    key: String,
    id: MonitorId,
    info: DisplayInfo,
    /// None for a display with no backend, or one whose link couldn't be opened (retried on the next rescan).
    engine: Option<EngineHandle>,
    /// Cleared when the monitor leaves: its engine's late events are dropped.
    alive: Arc<AtomicBool>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedIdentity {
    id: String,
    edid_id: String,
    serial: Option<String>,
    connector: Option<String>,
}

struct Hub {
    discovery: Box<dyn Discovery>,
    root: PathBuf,
    /// For engines spawned from now on; follows `SetSafeMode`.
    safe_mode: bool,
    sink: HubSink,
    batches: Batches,
    /// In attach order.
    running: Vec<Running>,
    /// The 0.0.8 root files were considered for a LEN62A2.
    migrated: bool,
    /// The power state so far, replayed to engines spawned while asleep.
    display_off: bool,
    locked: bool,
    suspended: bool,
}

impl Hub {
    fn run(mut self, rx: &Receiver<HubCommand>, ready: Sender<()>) {
        self.rescan();
        drop(ready);
        while let Ok(command) = rx.recv() {
            match command {
                HubCommand::Send { target, op, command } => self.send(target, op, *command),
                HubCommand::Rescan => self.rescan(),
                HubCommand::Power(state) => self.power(state),
                HubCommand::Shutdown => break,
            }
        }
        self.shutdown();
    }

    fn rescan(&mut self) {
        let displays = match self.discovery.enumerate() {
            Ok(displays) => displays,
            Err(e) => {
                log::write(&format!("Hub: listing displays failed: {e:?}"));
                emit(&self.sink, HubEvent::Monitors(Err(e)));
                return;
            }
        };
        let keys: Vec<String> = displays.iter().map(key).collect();
        let (stay, gone): (Vec<Running>, Vec<Running>) =
            std::mem::take(&mut self.running).into_iter().partition(|r| keys.contains(&r.key));
        self.running = stay;
        for r in gone {
            log::write(&format!("Hub: {} detached", r.id));
            r.alive.store(false, Ordering::SeqCst);
            if let Some(engine) = &r.engine {
                engine.stop(); // not joined: a monitor that left must not hold up the others
            }
            self.forget(&r.id);
            emit(&self.sink, HubEvent::Detached(r.id));
        }

        let identities: Vec<MonitorIdentity> = displays.iter().map(|d| d.identity.clone()).collect();
        let fresh = identity::assign_ids(&identities);
        let mut used: BTreeSet<MonitorId> = self.running.iter().map(|r| r.id.clone()).collect();
        let mut attached = Vec::new();
        for ((info, key), fresh) in displays.into_iter().zip(&keys).zip(fresh) {
            if let Some(r) = self.running.iter_mut().find(|r| &r.key == key) {
                r.info = info;
                continue;
            }
            let id = unused(fresh, &info.identity, &used);
            used.insert(id.clone());
            log::write(&format!("Hub: {id} attached ({:?}, {:?})", info.kind, info.backend));
            attached.push(self.running.len());
            self.running.push(Running { key: key.clone(), id, info, engine: None, alive: Arc::new(AtomicBool::new(true)) });
        }

        let list = keys.iter()
            .filter_map(|k| self.running.iter().find(|r| &r.key == k))
            .map(|r| Monitor { id: r.id.clone(), info: r.info.clone() })
            .collect();
        emit(&self.sink, HubEvent::Monitors(Ok(list)));
        for i in 0..self.running.len() {
            let r = &self.running[i];
            if attached.contains(&i) {
                emit(&self.sink, HubEvent::Attached(Monitor { id: r.id.clone(), info: r.info.clone() }));
            }
            if r.engine.is_none() && r.info.backend != BackendKind::None {
                self.start(i);
            }
        }
    }

    /// Opens the monitor's link and spawns its engine; a link that can't be opened is reported `NotFound` with the
    /// failure and tried again on the next rescan.
    fn start(&mut self, i: usize) {
        let (id, info, alive) = {
            let r = &self.running[i];
            (r.id.clone(), r.info.clone(), r.alive.clone())
        };
        let link = match self.discovery.open(&info.target) {
            Ok(link) => link,
            Err(failure) => {
                log::write(&format!("Hub: couldn't open {id}: {failure:?}"));
                let event = Event::Status { status: crate::engine::MonitorStatus::NotFound, failure: Some(failure) };
                emit(&self.sink, HubEvent::Monitor { id, event });
                return;
            }
        };
        let dir = self.dir_for(&id, &info.identity);
        // The built-in panel: its own profile and no DDC pacing.
        let backlight = info.backend == BackendKind::Backlight;
        let config = EngineConfig {
            id: id.clone(),
            identity: info.identity,
            link,
            dir,
            timing: backlight.then(|| Timing { gap: Duration::ZERO, settle: Duration::ZERO, ..Timing::default() }),
            safe_mode: self.safe_mode,
            level: backlight.then(|| SupportLevel::Verified { profile: "builtin-backlight".into() }),
        };
        let (sink, batches) = (self.sink.clone(), self.batches.clone());
        let tag = id.clone();
        let engine = Engine::spawn(config, Box::new(move |event| {
            if !alive.load(Ordering::SeqCst) {
                return;
            }
            if let Event::Result { op: Some(op), result, .. } = &event {
                if answer(&batches, &sink, *op, &tag, Some(result.clone())) {
                    return;
                }
            }
            emit(&sink, HubEvent::Monitor { id: tag.clone(), event });
        }));
        let asleep = [(self.display_off, PowerState::DisplayOff), (self.locked, PowerState::Locked),
            (self.suspended, PowerState::Suspending)];
        for (_, state) in asleep.into_iter().filter(|(on, _)| *on) {
            engine.send(Command::Power(state));
        }
        self.running[i].engine = Some(engine);
    }

    /// The engine's folder; a stable id's gets `identity.json` and, maybe, the 0.0.8 files.
    fn dir_for(&mut self, id: &MonitorId, identity: &MonitorIdentity) -> PathBuf {
        if id.is_unstable() {
            return session_dir().join(id.folder_name());
        }
        let dir = self.root.join("monitors").join(id.folder_name());
        if let Err(e) = fs::create_dir_all(&dir) {
            log::write(&format!("Hub: couldn't create {}: {e}", dir.display()));
        }
        self.migrate(id, identity, &dir);
        let saved = SavedIdentity {
            id: id.0.clone(),
            edid_id: identity.edid_id.clone(),
            serial: identity.serial.clone(),
            connector: identity.connector.clone(),
        };
        let written = serde_json::to_string_pretty(&saved).map_err(std::io::Error::other)
            .and_then(|json| write_atomic(&dir.join("identity.json"), &json));
        if let Err(e) = written {
            log::write(&format!("Hub: couldn't write {id}'s identity.json: {e}"));
        }
        dir
    }

    /// Moves the 0.0.8 root files into `dir` when this is the first LEN62A2 and `match_saved` picks the root
    /// (a saved entry with that EDID id and no serial or connector) for it.
    fn migrate(&mut self, id: &MonitorId, identity: &MonitorIdentity, dir: &Path) {
        if self.migrated || identity.edid_id != LEGACY_EDID {
            return;
        }
        self.migrated = true;
        let files = legacy_files(&self.root);
        if files.is_empty() {
            return;
        }
        let root_id = MonitorId::default();
        let mut saved = saved_identities(&self.root.join("monitors"));
        saved.push((root_id.clone(), MonitorIdentity { edid_id: LEGACY_EDID.into(), ..Default::default() }));
        if identity::match_saved(id, identity, &saved) != Some(&root_id) {
            log::write(&format!("Hub: the 0.0.8 files don't match {id} unambiguously; left in place"));
            return;
        }
        for from in files {
            let Some(name) = from.file_name() else { continue };
            let to = dir.join(name);
            if to.exists() {
                continue;
            }
            match fs::rename(&from, &to) {
                Ok(()) => log::write(&format!("Hub: moved {} to {id}", name.to_string_lossy())),
                Err(e) => log::write(&format!("Hub: couldn't move {}: {e}", from.display())),
            }
        }
    }

    fn send(&mut self, target: Target, op: OpId, command: Command) {
        if let Command::SetSafeMode(on) = command {
            self.safe_mode = on;
        }
        match target {
            Target::One(id) => {
                let engine = self.running.iter().find(|r| r.id == id).and_then(|r| r.engine.as_ref());
                let answers = answers(&command);
                if !engine.is_some_and(|e| e.send_op(op, command)) {
                    if let Some((origin, code, preset)) = answers {
                        let event = Event::Result { origin, code, preset, result: not_responding(), op: Some(op) };
                        emit(&self.sink, HubEvent::Monitor { id, event });
                    }
                }
            }
            Target::All => self.fan_out(op, command),
        }
    }

    fn fan_out(&mut self, op: OpId, command: Command) {
        let answers = answers(&command);
        let (origin, code) = answers.as_ref().map_or((Origin::Window, None), |(o, c, _)| (*o, *c));
        let fans = matches!(command, Command::Set { .. } | Command::Step { .. } | Command::DragTo { .. }
            | Command::EndDrag { .. } | Command::ApplyPreset { .. });
        let engines: Vec<(&MonitorId, &EngineHandle)> =
            self.running.iter().filter_map(|r| r.engine.as_ref().map(|e| (&r.id, e))).collect();
        if !fans || (answers.is_some() && engines.is_empty()) {
            if !fans {
                log::write(&format!("Hub: {command:?} can't go to every monitor"));
            }
            emit(&self.sink, HubEvent::Batch { op, origin, code, results: Vec::new() });
            return;
        }
        if answers.is_some() {
            let waiting = engines.iter().map(|(id, _)| (*id).clone()).collect();
            lock(&self.batches).insert(op, Pending { origin, code, waiting, results: Vec::new() });
        }
        for (id, engine) in engines {
            if !engine.send_op(op, command.clone()) && answers.is_some() {
                answer(&self.batches, &self.sink, op, id, Some(not_responding()));
            }
        }
    }

    fn power(&mut self, state: PowerState) {
        match state {
            PowerState::DisplayOff => self.display_off = true,
            PowerState::DisplayOn => self.display_off = false,
            PowerState::Locked => self.locked = true,
            PowerState::Unlocked => self.locked = false,
            PowerState::Suspending => self.suspended = true,
            PowerState::Resumed => self.suspended = false,
        }
        for engine in self.running.iter().filter_map(|r| r.engine.as_ref()) {
            engine.send(Command::Power(state));
        }
    }

    /// A monitor left: no batch waits for it any more.
    fn forget(&self, id: &MonitorId) {
        let ops: Vec<OpId> = lock(&self.batches).keys().copied().collect();
        for op in ops {
            answer(&self.batches, &self.sink, op, id, None);
        }
    }

    fn shutdown(&mut self) {
        let running = std::mem::take(&mut self.running);
        for engine in running.iter().filter_map(|r| r.engine.as_ref()) {
            engine.stop();
        }
        let deadline = Instant::now() + SHUTDOWN_WAIT;
        for engine in running.into_iter().filter_map(|r| r.engine) {
            engine.join(deadline);
        }
        let _ = fs::remove_dir_all(session_dir());
    }
}

/// Records `id`'s answer (None: it left) to batch `op` and sends the batch once no engine is left to answer.
/// False when `op` isn't a batch.
fn answer(batches: &Batches, sink: &HubSink, op: OpId, id: &MonitorId, result: Option<WriteResult>) -> bool {
    let done = {
        let mut map = lock(batches);
        let Some(pending) = map.get_mut(&op) else { return false };
        if pending.waiting.remove(id) {
            if let Some(result) = result {
                pending.results.push((id.clone(), result));
            }
        }
        if !pending.waiting.is_empty() {
            return true;
        }
        map.remove(&op)
    };
    if let Some(mut pending) = done {
        pending.results.sort_by(|a, b| a.0.cmp(&b.0));
        emit(sink, HubEvent::Batch { op, origin: pending.origin, code: pending.code, results: pending.results });
    }
    true
}

/// The origin, code and preset name of a command that answers with a `Result`.
fn answers(command: &Command) -> Option<(Origin, Option<u8>, Option<String>)> {
    match command {
        Command::Set { code, origin, .. } | Command::EndDrag { code, origin, .. } | Command::Step { code, origin, .. } => {
            Some((*origin, Some(*code), None))
        }
        Command::ApplyPreset { preset, origin } => Some((*origin, None, Some(preset.name.clone()))),
        Command::RestoreBackup { origin } => Some((*origin, None, None)),
        _ => None,
    }
}

fn not_responding() -> WriteResult {
    WriteResult {
        outcome: WriteOutcome::NotResponding,
        kind: ResultKind::NotResponding,
        message: "Monitor not responding".into(),
        value: None,
        kept: None,
    }
}

fn key(info: &DisplayInfo) -> String {
    let i = &info.identity;
    let port = i.connector.as_deref().filter(|c| !c.is_empty()).unwrap_or(&info.target);
    format!("{}|{}|{port}", i.edid_id, i.serial.as_deref().unwrap_or(""))
}

/// `fresh`, unless an attached monitor already has it: then the first free `-N` ordinal for that EDID id.
fn unused(fresh: MonitorId, identity: &MonitorIdentity, used: &BTreeSet<MonitorId>) -> MonitorId {
    if !used.contains(&fresh) {
        return fresh;
    }
    let edid = if identity.edid_id.is_empty() { "UNKNOWN" } else { &identity.edid_id };
    (1..).map(|n| MonitorId(format!("{edid}-N{n}"))).find(|id| !used.contains(id)).unwrap_or(fresh)
}

/// The 0.0.8 files in the data directory: capabilities.txt, profile.user.json, backup-*.json, risky-operation*.txt.
fn legacy_files(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else { return Vec::new() };
    entries.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_file())).filter(|e| {
        let name = e.file_name().to_string_lossy().into_owned();
        matches!(name.as_str(), "capabilities.txt" | "profile.user.json")
            || (name.starts_with("backup-") && name.ends_with(".json"))
            || (name.starts_with("risky-operation") && name.ends_with(".txt"))
    }).map(|e| e.path()).collect()
}

/// The identities saved in `monitors/*/identity.json`.
fn saved_identities(monitors: &Path) -> Vec<(MonitorId, MonitorIdentity)> {
    let Ok(entries) = fs::read_dir(monitors) else { return Vec::new() };
    entries.flatten().filter_map(|e| {
        let text = fs::read_to_string(e.path().join("identity.json")).ok()?;
        let s: SavedIdentity = serde_json::from_str(&text).ok()?;
        Some((MonitorId(s.id), MonitorIdentity { edid_id: s.edid_id, serial: s.serial, connector: s.connector }))
    }).collect()
}

/// This process's folder for monitors with unstable ids.
fn session_dir() -> PathBuf {
    std::env::temp_dir().join(format!("thinkvision-control-{}", std::process::id()))
}

fn emit(sink: &HubSink, event: HubEvent) {
    (lock(sink))(event);
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
