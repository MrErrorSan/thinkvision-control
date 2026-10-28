//! The DDC engine: one worker thread owns the link, the value cache, drag state, status and the retry burst.
//! The app sends [`Command`]s and receives [`Event`]s through its sink; nothing else touches monitor state.
//!
//! Pacing: ≥ `gap` between link calls; reads and capability reads wait until `settle` after the last
//! write (writes are not held, so drags stream); a call failing with [`LinkError::Transient`] is tried up to
//! `attempts` times, `retry` apart. `Unsupported` is final; `Gone` marks the monitor lost and reconnects at once.
//!
//! While the display is off, the session is locked or the PC sleeps there is no monitor traffic at all.
//! After a crash inside a capability read or the finder the app starts the engine in safe mode.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::crash::Marker;
use crate::finder::{self, CodeChange};
use crate::link::{LinkError, LinkFailure, MonitorLink, VcpReading};
use crate::mccs::{self, MonitorCapabilities};
use crate::profile::{ControlDefinition, ControlKind, Profile, ProfileStore, SafetyClass, Section};
use crate::result::ResultKind;
use crate::safety::{self, Verdict};
use crate::store::{BackupStore, CapabilitiesCache, Preset};
use crate::{hex, log};

const BRIGHTNESS: u8 = 0x10;
const CONTRAST: u8 = 0x12;
const COLOUR_MODE: u8 = 0x14;
const INPUT: u8 = 0x60;
/// How long [`EngineHandle::shutdown`] waits for the operation in flight.
pub const SHUTDOWN_WAIT: Duration = Duration::from_secs(1);

const ASLEEP_MESSAGE: &str = "The monitor is asleep or this PC is locked";

/// Engine pacing. `Default` = the P32p-20 values (also in the profile's `quirks.timing`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Timing {
    /// Minimum time from the end of one link call to the start of the next.
    pub gap: Duration,
    /// Reads wait until this long after the last write.
    pub settle: Duration,
    /// Pause between attempts of a transiently failed call.
    pub retry: Duration,
    pub attempts: u32,
    /// Retry-burst delays: a reconnect follows each one.
    pub burst: Vec<Duration>,
    /// Polls read nothing within this long of a write.
    pub poll_quiet: Duration,
    /// After the last wake event (display on, unlock, resume): no traffic for this long, then reconnect.
    pub wake_delay: Duration,
    /// After an input write: wait this long, then re-read once to confirm.
    pub input_confirm: Duration,
    /// A hotkey step re-reads a cached value older than this first.
    pub stale_after: Duration,
    /// "Permission denied" this soon after resume is retried by the burst, not reported (Linux, as ddcutil).
    pub resume_grace: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            gap: Duration::from_millis(50),
            settle: Duration::from_millis(400),
            retry: Duration::from_millis(150),
            attempts: 5,
            burst: [500, 1000, 2000, 4000, 8000, 15_000, 30_000].map(Duration::from_millis).to_vec(),
            poll_quiet: Duration::from_secs(1),
            wake_delay: Duration::from_secs(3),
            input_confirm: Duration::from_millis(1500),
            stale_after: Duration::from_secs(30),
            resume_grace: Duration::from_secs(10),
        }
    }
}

impl Timing {
    /// No waits (burst, wake delay and input confirm included), for tests and the demo. `poll_quiet`,
    /// `stale_after` and `resume_grace` keep their defaults: they are windows, not waits.
    pub fn zero() -> Self {
        let d = Self::default();
        Self {
            gap: Duration::ZERO,
            settle: Duration::ZERO,
            retry: Duration::ZERO,
            burst: vec![Duration::ZERO; d.burst.len()],
            wake_delay: Duration::ZERO,
            input_confirm: Duration::ZERO,
            ..d
        }
    }
}

/// Connecting = first attempt in progress; NotFound = not plugged in; NotResponding = found but DDC/CI silent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MonitorStatus {
    Connecting,
    NotFound,
    NotResponding,
    Connected,
}

/// Who asked: the controller routes feedback by it (window → card note; hotkey/tray/CLI → OSD).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Origin {
    Window,
    Hotkey,
    Tray,
    Cli,
    /// Automatic Day/Night.
    Schedule,
}

/// Display, session and system power changes the app forwards.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PowerState {
    DisplayOff,
    DisplayOn,
    Locked,
    Unlocked,
    Suspending,
    Resumed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WriteOutcome {
    Ok,
    NeedsConfirmation,
    Blocked,
    NotResponding,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteResult {
    pub outcome: WriteOutcome,
    /// Why it ended this way; set on every result.
    pub kind: ResultKind,
    /// User-facing text: "Brightness 60", "Input → HDMI", "Day preset", "Brightness remains 50;...".
    pub message: String,
    /// The value written, on Ok (single writes only).
    pub value: Option<u16>,
    /// The value the monitor kept when it ignored a write ("remains" / "stayed on" results).
    pub kept: Option<u16>,
}

impl WriteResult {
    fn ok(message: String, value: Option<u16>) -> Self {
        Self { outcome: WriteOutcome::Ok, kind: ResultKind::Ok, message, value, kept: None }
    }

    fn blocked(kind: ResultKind, message: String) -> Self {
        Self { outcome: WriteOutcome::Blocked, kind, message, value: None, kept: None }
    }

    fn unsupported(name: &str) -> Self {
        Self::blocked(ResultKind::Unsupported, format!("{name} isn't supported by this monitor."))
    }
}

/// The controls as resolved against the current capabilities (cached, synthesized or live).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedControls {
    pub profile: Arc<Profile>,
    pub capabilities: MonitorCapabilities,
    /// A definition for every code in `capabilities`.
    pub controls: BTreeMap<u8, ControlDefinition>,
    /// "LB1.2", from the 0x04 value list.
    pub firmware: Option<String>,
}

/// Commands that would touch the monitor do nothing while asleep; writes then answer `kind: Asleep`.
#[derive(Clone, Debug)]
pub enum Command {
    /// One connect attempt: capabilities (once per process, never in safe mode), then every Range/Enum code in
    /// Main/Advanced plus every cached code. No retry burst; follow with `RequestReconnect` if that's wanted.
    Connect,
    /// After a display/device/power event: reconnect even while connected, then burst if still not connected.
    RecheckAfterEvent,
    /// Start (or restart) the retry burst; no-op while connected. Paused while asleep.
    RequestReconnect,
    /// Re-read codes; no-op unless connected. All failing marks the monitor lost.
    Refresh(Vec<u8>),
    /// Quiet re-read for a visible window: skipped during a drag or right after a write; stops at the first
    /// failure and never changes the status (unless the connection is gone).
    Poll(Vec<u8>),
    Set { code: u8, value: u16, confirmed: bool, origin: Origin },
    /// Live slider position: coalesced (newest wins), written without read-back, no `Value` events for the code.
    DragTo { code: u8, value: u16 },
    /// Slider released: writes and verifies once, then emits the actual `Value` and a `Result`.
    EndDrag { code: u8, value: u16, origin: Origin },
    /// Adds `delta` to the cached value (re-reading first if uncached or older than `stale_after`), clamped to 0..max.
    Step { code: u8, delta: i32, origin: Origin },
    /// Colour mode, brightness, contrast; stops at the first result that isn't Ok.
    ApplyPreset { preset: Preset, origin: Origin },
    /// Re-applies the latest backup, stopping at the first failure. Never asks for confirmation.
    RestoreBackup { origin: Origin },
    /// Reads every Hidden, non-NeverWrite code and keeps the values as the "before" snapshot.
    FinderSnapshot,
    /// Reads them again and reports what changed since the snapshot.
    FinderCompare,
    /// Saves a definition to the user overlay, then re-resolves and emits `Controls`.
    SaveOverlay(ControlDefinition),
    /// A display, session or system power change.
    Power(PowerState),
    /// Enter or leave safe mode. Leaving doesn't reconnect; the next reconnect reads the capabilities.
    SetSafeMode(bool),
    /// Reads the capability string from the monitor if this process hasn't yet. Connecting uses the cached copy
    /// when there is one (the live read takes ~2.3 s on the P32p-20), so a resident app sends this once it is
    /// connected and a one-shot command-line run never pays for it.
    ReadCapabilities,
    /// Replies once every command sent before it has been handled (tests, headless runs).
    Flush(Sender<()>),
    Shutdown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FinderEvent {
    /// "Snapshot taken ({count} codes)".
    Snapshot { count: usize },
    /// Codes whose value changed since the snapshot, by code.
    Changes(Vec<CodeChange>),
    /// Compare was asked for before any snapshot.
    NoSnapshot,
    /// Snapshot or compare refused: `SafeMode` or `Asleep`.
    Refused(ResultKind),
    /// `SaveOverlay` finished; the error text on failure.
    Saved { code: u8, result: Result<(), String> },
}

#[derive(Clone, Debug)]
pub enum Event {
    /// Sent at start and whenever status or failure changes. `failure` is set only with NotFound.
    Status { status: MonitorStatus, failure: Option<LinkFailure> },
    /// Sent at start, after the first live capability read, and after `SaveOverlay`.
    Controls(Arc<ResolvedControls>),
    /// A code's current value changed (never for a code being dragged, except the final value on release).
    Value { code: u8, reading: VcpReading },
    /// The answer to Set, EndDrag, Step (`code` set), ApplyPreset (`preset` = its name) or RestoreBackup (neither).
    Result { origin: Origin, code: Option<u8>, preset: Option<String>, result: WriteResult },
    Finder(FinderEvent),
    /// True while the display is off, the session is locked or the PC sleeps, until `wake_delay` after the last
    /// wake event; no monitor traffic meanwhile.
    Asleep(bool),
    /// Sent at start when spawned in safe mode, and whenever it changes.
    SafeMode(bool),
}

pub struct Engine;

impl Engine {
    /// Starts the worker. `dir` is the data directory (`capabilities.txt`, backups, the crash marker). The first
    /// events are `Controls` (from the capability cache, else synthesized from the profile), `Status(Connecting)`
    /// and, with `safe_mode`, `SafeMode(true)`; send `Connect` to start talking to the monitor.
    pub fn spawn(link: Box<dyn MonitorLink>, profiles: ProfileStore, dir: PathBuf, timing: Timing, safe_mode: bool,
        sink: Box<dyn Fn(Event) + Send>) -> EngineHandle {
        let (tx, rx) = mpsc::channel();
        let (done_tx, done) = mpsc::channel::<()>();
        let stop = Arc::new(AtomicBool::new(false));
        let caps_cache = CapabilitiesCache::new(&dir);
        let cached_raw = caps_cache.load();
        let caps = cached_raw.as_deref().and_then(|raw| mccs::parse(raw).ok())
            .unwrap_or_else(|| profiles.synthesize_capabilities());
        let controls = profiles.resolve(&caps);
        let worker = Worker {
            link,
            profiles,
            caps_cache,
            crash: Marker::new(&dir),
            backups: BackupStore::new(dir),
            timing,
            sink,
            rx,
            stop: stop.clone(),
            pending: VecDeque::new(),
            cached_raw,
            live_caps: false,
            caps,
            controls,
            cache: BTreeMap::new(),
            read_at: BTreeMap::new(),
            drags: BTreeSet::new(),
            status: MonitorStatus::Connecting,
            failure: None,
            ready_at: Instant::now(),
            last_write: None,
            burst: None,
            snapshot: None,
            safe_mode,
            display_off: false,
            locked: false,
            suspended: false,
            resumed_at: None,
            wake_at: None,
        };
        thread::Builder::new()
            .name("tvc-engine".into())
            .spawn(move || {
                let _done = done_tx;
                worker.run();
            })
            .expect("spawn the engine thread");
        EngineHandle { tx, stop, done }
    }
}

/// The app's end of the engine.
pub struct EngineHandle {
    tx: Sender<Command>,
    stop: Arc<AtomicBool>,
    done: Receiver<()>,
}

impl EngineHandle {
    pub fn send(&self, command: Command) {
        let _ = self.tx.send(command);
    }

    /// Another sender for other threads (hotkeys, tray).
    pub fn sender(&self) -> Sender<Command> {
        self.tx.clone()
    }

    /// Stops the worker, waiting at most [`SHUTDOWN_WAIT`] for the operation in flight, so exit never hangs on
    /// a monitor that doesn't answer.
    pub fn shutdown(self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.tx.send(Command::Shutdown);
        let _ = self.done.recv_timeout(SHUTDOWN_WAIT);
    }
}

struct Burst {
    step: usize,
    due: Instant,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OpKind {
    Read,
    Write,
}

struct Worker {
    link: Box<dyn MonitorLink>,
    profiles: ProfileStore,
    caps_cache: CapabilitiesCache,
    crash: Marker,
    backups: BackupStore,
    timing: Timing,
    sink: Box<dyn Fn(Event) + Send>,
    rx: Receiver<Command>,
    stop: Arc<AtomicBool>,
    /// Commands taken off the channel while coalescing a drag, handled before the channel.
    pending: VecDeque<Command>,
    cached_raw: Option<String>,
    live_caps: bool,
    caps: MonitorCapabilities,
    controls: BTreeMap<u8, ControlDefinition>,
    cache: BTreeMap<u8, VcpReading>,
    /// When each cached value was last read or written.
    read_at: BTreeMap<u8, Instant>,
    drags: BTreeSet<u8>,
    status: MonitorStatus,
    failure: Option<LinkFailure>,
    ready_at: Instant,
    last_write: Option<Instant>,
    burst: Option<Burst>,
    snapshot: Option<BTreeMap<u8, u16>>,
    safe_mode: bool,
    display_off: bool,
    locked: bool,
    suspended: bool,
    resumed_at: Option<Instant>,
    /// Set after the last wake event: the wake recheck runs then.
    wake_at: Option<Instant>,
}

impl Worker {
    fn run(mut self) {
        self.emit_controls();
        self.emit(Event::Status { status: self.status, failure: None });
        if self.safe_mode {
            self.emit(Event::SafeMode(true));
        }
        while let Some(command) = self.next() {
            match command {
                Command::Shutdown => break,
                command => self.handle(command),
            }
        }
        self.link.disconnect();
    }

    /// The next command, running the wake recheck and retry-burst steps as they fall due.
    fn next(&mut self) -> Option<Command> {
        loop {
            if self.stopping() {
                return None;
            }
            let now = Instant::now();
            if self.wake_at.is_some_and(|at| at <= now) {
                self.wake_at = None;
                self.emit(Event::Asleep(false));
                self.recheck();
                log::write(&format!("Awake: rechecked, monitor {:?}", self.status));
                continue;
            }
            // The burst is paused while asleep; the wake recheck restarts it.
            let burst_due = self.burst.as_ref().filter(|_| !self.asleep()).map(|b| b.due);
            if burst_due.is_some_and(|at| at <= now) {
                self.burst_step();
                continue;
            }
            if let Some(command) = self.pending.pop_front() {
                return Some(command);
            }
            let Some(due) = self.wake_at.into_iter().chain(burst_due).min() else {
                return self.rx.recv().ok();
            };
            match self.rx.recv_timeout(due - now) {
                Ok(command) => return Some(command),
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return None,
            }
        }
    }

    fn handle(&mut self, command: Command) {
        match command {
            Command::Connect => {
                if !self.asleep() {
                    self.reconnect();
                }
            }
            Command::RecheckAfterEvent => {
                if !self.asleep() {
                    self.recheck();
                }
            }
            Command::RequestReconnect => self.request_reconnect(),
            Command::Refresh(codes) => {
                if self.status == MonitorStatus::Connected && !self.asleep() {
                    self.refresh(&codes, true);
                }
            }
            Command::Poll(codes) => self.poll(&codes),
            Command::Set { code, value, confirmed, origin } => {
                let result = self.set(code, value, confirmed);
                self.emit_result(origin, Some(code), None, result);
            }
            Command::DragTo { code, value } => self.drag_to(code, value),
            Command::EndDrag { code, value, origin } => {
                let result = self.set(code, value, false);
                self.drags.remove(&code);
                if let Some(&reading) = self.cache.get(&code) {
                    self.emit(Event::Value { code, reading });
                }
                self.emit_result(origin, Some(code), None, result);
            }
            Command::Step { code, delta, origin } => {
                let result = self.step(code, delta);
                self.emit_result(origin, Some(code), None, result);
            }
            Command::ApplyPreset { preset, origin } => {
                let result = self.apply_preset(&preset);
                self.emit_result(origin, None, Some(preset.name), result);
            }
            Command::RestoreBackup { origin } => {
                let result = self.restore_backup();
                self.emit_result(origin, None, None, result);
            }
            Command::FinderSnapshot => {
                if let Some(kind) = self.held() {
                    self.emit(Event::Finder(FinderEvent::Refused(kind)));
                    return;
                }
                let snapshot = self.take_snapshot("finder snapshot");
                self.emit(Event::Finder(FinderEvent::Snapshot { count: snapshot.len() }));
                self.snapshot = Some(snapshot);
            }
            Command::FinderCompare => {
                let event = match (self.held(), self.snapshot.clone()) {
                    (Some(kind), _) => FinderEvent::Refused(kind),
                    (None, Some(before)) => FinderEvent::Changes(finder::diff(&before, &self.take_snapshot("finder compare"))),
                    (None, None) => FinderEvent::NoSnapshot,
                };
                self.emit(Event::Finder(event));
            }
            Command::SaveOverlay(def) => {
                let code = def.code;
                let result = self.profiles.save_overlay(&def).map_err(|e| e.to_string());
                if result.is_ok() {
                    self.controls = self.profiles.resolve(&self.caps);
                    self.emit_controls();
                }
                self.emit(Event::Finder(FinderEvent::Saved { code, result }));
            }
            Command::Power(state) => self.power(state),
            Command::ReadCapabilities => {
                if self.status == MonitorStatus::Connected && !self.live_caps && self.held().is_none() {
                    self.read_capabilities();
                }
            }
            Command::SetSafeMode(on) => {
                if on != self.safe_mode {
                    self.safe_mode = on;
                    self.emit(Event::SafeMode(on));
                }
            }
            Command::Flush(reply) => {
                let _ = reply.send(());
            }
            Command::Shutdown => {}
        }
    }

    /// True while there must be no monitor traffic: display off, locked, suspended, or within `wake_delay` of
    /// waking.
    fn asleep(&self) -> bool {
        self.display_off || self.locked || self.suspended || self.wake_at.is_some()
    }

    /// Why the risky reads (live capability read, finder) are held back, if they are: safe mode or asleep.
    fn held(&self) -> Option<ResultKind> {
        if self.safe_mode {
            Some(ResultKind::SafeMode)
        } else if self.asleep() {
            Some(ResultKind::Asleep)
        } else {
            None
        }
    }

    /// The result for a write that is held back: only while asleep. Safe mode skips the operations that
    /// crashed (capability read, finder), not ordinary writes, so a false alarm (the app killed mid-read) costs
    /// nothing the user notices.
    fn hold(&self) -> Option<WriteResult> {
        self.asleep().then(|| WriteResult::blocked(ResultKind::Asleep, ASLEEP_MESSAGE.into()))
    }

    fn power(&mut self, state: PowerState) {
        let was_asleep = self.asleep();
        match state {
            PowerState::DisplayOff => self.display_off = true,
            PowerState::DisplayOn => self.display_off = false,
            PowerState::Locked => self.locked = true,
            PowerState::Unlocked => self.locked = false,
            PowerState::Suspending => self.suspended = true,
            PowerState::Resumed => {
                self.suspended = false;
                self.resumed_at = Some(Instant::now());
            }
        }
        if self.display_off || self.locked || self.suspended {
            self.wake_at = None;
        } else if was_asleep || state == PowerState::Resumed {
            // The last wake event (or a resume whose suspend we missed): wait, then recheck.
            self.wake_at = Some(Instant::now() + self.timing.wake_delay);
        }
        log::write(&format!("Power: {state:?} (asleep: {})", self.asleep()));
        if self.asleep() != was_asleep {
            self.emit(Event::Asleep(self.asleep()));
        }
    }

    fn recheck(&mut self) {
        self.reconnect();
        if self.status != MonitorStatus::Connected {
            self.request_reconnect();
        }
    }

    fn reconnect(&mut self) {
        if let Err(failure) = self.connect() {
            let just_resumed = self.resumed_at.is_some_and(|at| at.elapsed() < self.timing.resume_grace);
            if failure == LinkFailure::PermissionDenied && just_resumed {
                // The bus isn't ready yet after resume (ddcutil): keep trying instead of asking for setup.
                log::write("Connect: permission denied right after resume; retrying.");
                self.set_status(MonitorStatus::Connecting, None);
                if self.burst.is_none() {
                    self.request_reconnect();
                }
                return;
            }
            self.set_status(MonitorStatus::NotFound, Some(failure));
            return;
        }
        // With a cached capability string, connect on values alone; `ReadCapabilities` refreshes it later.
        if !self.live_caps && !self.safe_mode && self.cached_raw.is_none() && !self.read_capabilities() {
            self.set_status(MonitorStatus::NotResponding, None);
            return;
        }
        let mut codes: BTreeSet<u8> = self.controls.values()
            .filter(|d| matches!(d.kind, ControlKind::Range | ControlKind::Enum)
                && matches!(d.section, Section::Main | Section::Advanced))
            .map(|d| d.code)
            .collect();
        codes.extend(self.cache.keys());
        if self.refresh(&codes.into_iter().collect::<Vec<_>>(), false) {
            self.set_status(MonitorStatus::Connected, None);
        }
    }

    /// Reads and caches the capability string, re-resolving the controls; reads values for codes it adds. Guarded
    /// by the crash marker. False if the monitor didn't answer or the string didn't parse.
    fn read_capabilities(&mut self) -> bool {
        let attempts = self.timing.attempts;
        self.crash.arm("capability read");
        let raw = self.run_op(OpKind::Read, attempts, |l| l.capabilities());
        self.crash.disarm();
        let read = raw.map_err(|e| e.to_string())
            .and_then(|raw| mccs::parse(&raw).map(|caps| (raw, caps)).map_err(|e| e.to_string()));
        let (raw, caps) = match read {
            Ok(read) => read,
            Err(e) => {
                log::write(&format!("Capability read failed: {e}"));
                return false;
            }
        };
        self.live_caps = true;
        if self.cached_raw.as_deref() == Some(raw.as_str()) {
            return true;
        }
        self.caps_cache.save(&raw);
        self.cached_raw = Some(raw);
        self.controls = self.profiles.resolve(&caps);
        self.caps = caps;
        self.emit_controls();
        if self.status == MonitorStatus::Connected {
            let missing: Vec<u8> = self.controls.values()
                .filter(|d| matches!(d.kind, ControlKind::Range | ControlKind::Enum)
                    && matches!(d.section, Section::Main | Section::Advanced)
                    && !self.cache.contains_key(&d.code))
                .map(|d| d.code)
                .collect();
            self.refresh(&missing, true);
        }
        true
    }

    fn request_reconnect(&mut self) {
        if self.status == MonitorStatus::Connected {
            return;
        }
        self.burst = self.timing.burst.first().map(|&d| Burst { step: 0, due: Instant::now() + d });
    }

    fn burst_step(&mut self) {
        let Some(burst) = self.burst.take() else { return };
        self.reconnect();
        if self.status != MonitorStatus::Connected {
            let step = burst.step + 1;
            self.burst = self.timing.burst.get(step).map(|&d| Burst { step, due: Instant::now() + d });
        }
    }

    /// Reads `codes` into the cache. False if none answered (an "unsupported" reply counts as an answer); then the
    /// monitor is NotResponding, and with `recover` the retry burst starts. Stops at a `Gone`.
    fn refresh(&mut self, codes: &[u8], recover: bool) -> bool {
        let mut answered = 0;
        for &code in codes {
            match self.read(code) {
                Ok(reading) => {
                    answered += 1;
                    self.update_cache(code, reading);
                }
                Err(e) => {
                    log::write(&format!("Read {} failed: {e}", hex(code)));
                    match e {
                        LinkError::Unsupported(_) => answered += 1,
                        LinkError::Gone(_) => return false, // `run_op` already marked the monitor lost
                        LinkError::Transient(_) => {}
                    }
                }
            }
        }
        if !codes.is_empty() && answered == 0 {
            if recover {
                self.lost();
            } else {
                self.set_status(MonitorStatus::NotResponding, None);
            }
            return false;
        }
        true
    }

    fn poll(&mut self, codes: &[u8]) {
        for &code in codes {
            let changing = !self.drags.is_empty() || self.last_write.is_some_and(|t| t.elapsed() < self.timing.poll_quiet);
            if self.status != MonitorStatus::Connected || changing || self.asleep() {
                return;
            }
            match self.read(code) {
                Ok(reading) => self.update_cache(code, reading),
                Err(e) => {
                    log::write(&format!("Poll {} failed: {e}", hex(code)));
                    return;
                }
            }
        }
    }

    fn set(&mut self, code: u8, value: u16, confirmed: bool) -> WriteResult {
        if let Some(result) = self.hold() {
            return result;
        }
        if self.status != MonitorStatus::Connected {
            return self.not_connected();
        }
        let def = self.controls.get(&code).cloned();
        let cached = self.cache.get(&code).copied();
        let max = cached.filter(|_| def.as_ref().is_some_and(|d| d.kind == ControlKind::Range)).map(|c| c.max);
        let decision = safety::evaluate(def.as_ref(), &self.caps, code, value, max, confirmed);
        let def = match (decision.verdict, def) {
            (Verdict::Allow, Some(def)) => def,
            (Verdict::NeedsConfirmation, _) => {
                return WriteResult {
                    outcome: WriteOutcome::NeedsConfirmation,
                    ..WriteResult::blocked(ResultKind::Refused, decision.message)
                };
            }
            (_, def) => return WriteResult::blocked(self.refusal_kind(def.as_ref(), code, value), decision.message),
        };
        let v = decision.value;

        if code == INPUT && def.kind == ControlKind::Enum {
            return self.switch_input(&def, v);
        }

        if def.kind == ControlKind::Action {
            if let Err(e) = self.backups.save(&self.backup_values()) {
                log::write(&format!("Backup failed: {e}"));
                return WriteResult::blocked(ResultKind::Refused, format!("Backup failed — {} cancelled.", def.name));
            }
        }

        if let Err(e) = self.write(code, v) {
            log::write(&format!("Write {}={v} failed: {e}", hex(code)));
            return self.failed_result(&def.name, &e);
        }

        match def.kind {
            ControlKind::Range => match self.read(code) {
                Ok(actual) => {
                    self.update_cache(code, actual);
                    if code == BRIGHTNESS {
                        log::write(&format!("Brightness DDC write verified: requested={v}, readback={}, max={}.",
                            actual.current, actual.max));
                    }
                    if actual.current != v {
                        let detail = if code == BRIGHTNESS { " The current color mode may lock brightness." } else { "" };
                        return WriteResult {
                            kept: Some(actual.current),
                            ..WriteResult::blocked(ResultKind::Kept, format!(
                                "{} remains {}; the monitor did not apply {v}.{detail}", def.name, actual.current))
                        };
                    }
                }
                Err(e) => {
                    log::write(&format!("Verify read {} failed: {e}", hex(code)));
                    return self.failed_result(&def.name, &e);
                }
            },
            ControlKind::Enum => self.update_cache(code, VcpReading { current: v, max: cached.map_or(0, |c| c.max) }),
            ControlKind::Action => {}
        }
        WriteResult::ok(describe(&def, v), Some(v))
    }

    /// read first and skip the write if the monitor is already there; write once (the link often drops
    /// mid-switch, so a retry could switch twice); confirm with one re-read after `input_confirm`.
    fn switch_input(&mut self, def: &ControlDefinition, v: u16) -> WriteResult {
        match self.read(INPUT) {
            Ok(reading) => {
                self.update_cache(INPUT, reading);
                if reading.current == v {
                    return WriteResult::ok(describe(def, v), Some(v));
                }
            }
            Err(LinkError::Gone(_)) => return self.not_connected(),
            // The monitor may answer only the PC it shows: switch anyway.
            Err(e) => log::write(&format!("Read {} before switching failed: {e}", hex(INPUT))),
        }
        let written = self.run_op(OpKind::Write, 1, |l| l.write(INPUT, v));
        match &written {
            Ok(()) => {}
            Err(LinkError::Transient(e)) => log::write(&format!("Write {}={v} failed: {e}; checking with a re-read",
                hex(INPUT))),
            Err(e) => {
                log::write(&format!("Write {}={v} failed: {e}", hex(INPUT)));
                return self.failed_result(&def.name, e);
            }
        }
        sleep_until(self.last_write.unwrap_or_else(Instant::now) + self.timing.input_confirm);
        let max = self.cache.get(&INPUT).map_or(0, |c| c.max);
        match self.read_with(INPUT, 1) {
            Ok(reading) => {
                self.update_cache(INPUT, reading);
                if reading.current == v {
                    WriteResult::ok(describe(def, v), Some(v))
                } else {
                    WriteResult {
                        kept: Some(reading.current),
                        ..WriteResult::blocked(ResultKind::Kept,
                            format!("The monitor stayed on {}", value_label(def, reading.current)))
                    }
                }
            }
            // On a two-PC desk the monitor usually stops answering this PC once it shows the other one.
            Err(e) if written.is_ok() => {
                log::write(&format!("Confirm read {} failed: {e}; taking the switch as done", hex(INPUT)));
                self.update_cache(INPUT, VcpReading { current: v, max });
                WriteResult::ok(describe(def, v), Some(v))
            }
            Err(e) => {
                log::write(&format!("Confirm read {} failed: {e}", hex(INPUT)));
                self.failed_result(&def.name, &e)
            }
        }
    }

    fn drag_to(&mut self, code: u8, value: u16) {
        if self.asleep()
            || self.status != MonitorStatus::Connected
            || !self.controls.get(&code).is_some_and(|d| d.kind == ControlKind::Range)
            || self.evaluate_drag(code, value).is_none()
        {
            return;
        }
        self.drags.insert(code);
        sleep_until(self.ready_at);
        // Let newer positions arrive during the gap, then write only the newest.
        let Some(newest) = self.coalesce(code, value) else { return };
        let Some(v) = self.evaluate_drag(code, newest) else { return };
        match self.write(code, v) {
            Ok(()) => {
                let max = self.cache.get(&code).map_or(100, |c| c.max);
                self.update_cache(code, VcpReading { current: v, max });
            }
            Err(e) => {
                log::write(&format!("Drag write {}={v} failed: {e}", hex(code)));
                self.failed(&e);
            }
        }
    }

    fn evaluate_drag(&self, code: u8, value: u16) -> Option<u16> {
        let max = self.cache.get(&code).map(|c| c.max);
        let d = safety::evaluate(self.controls.get(&code), &self.caps, code, value, max, false);
        (d.verdict == Verdict::Allow).then_some(d.value)
    }

    /// Drains the channel and removes queued `DragTo`s for `code`, returning the newest value; None when an
    /// `EndDrag` for `code` is already queued (it writes the final value).
    fn coalesce(&mut self, code: u8, value: u16) -> Option<u16> {
        self.pending.extend(self.rx.try_iter());
        let mut newest = value;
        let mut i = 0;
        while i < self.pending.len() {
            match self.pending[i] {
                Command::DragTo { code: c, value: v } if c == code => {
                    newest = v;
                    self.pending.remove(i);
                }
                Command::EndDrag { code: c, .. } if c == code => return None,
                _ => i += 1,
            }
        }
        Some(newest)
    }

    fn step(&mut self, code: u8, delta: i32) -> WriteResult {
        if let Some(result) = self.hold() {
            return result;
        }
        if self.status != MonitorStatus::Connected {
            return self.not_connected();
        }
        // another PC or the monitor's buttons may have changed it since (uncached codes have no time).
        let fresh = self.read_at.get(&code).is_some_and(|at| at.elapsed() < self.timing.stale_after);
        if !fresh {
            match self.read(code) {
                Ok(reading) => self.update_cache(code, reading),
                Err(e) => {
                    log::write(&format!("Read {} before step failed: {e}", hex(code)));
                    let name = self.controls.get(&code).map_or_else(|| hex(code), |d| d.name.clone());
                    return self.failed_result(&name, &e);
                }
            }
        }
        let Some(&reading) = self.cache.get(&code) else { return self.not_connected() };
        let target = (i64::from(reading.current) + i64::from(delta)).clamp(0, i64::from(reading.max));
        self.set(code, target as u16, false)
    }

    fn apply_preset(&mut self, preset: &Preset) -> WriteResult {
        let mut steps = Vec::with_capacity(3);
        if let Some(mode) = preset.colour_mode {
            steps.push((COLOUR_MODE, u16::from(mode)));
        }
        steps.push((BRIGHTNESS, saturate(preset.brightness)));
        steps.push((CONTRAST, saturate(preset.contrast)));
        for (code, value) in steps {
            let result = self.set(code, value, false);
            if result.outcome != WriteOutcome::Ok {
                return result;
            }
        }
        WriteResult::ok(format!("{} preset", preset.name), None)
    }

    fn restore_backup(&mut self) -> WriteResult {
        let values = match self.backups.load_latest() {
            Ok(Some(values)) => values,
            Ok(None) => return WriteResult::blocked(ResultKind::Refused, "No backup found.".into()),
            Err(e) => return WriteResult::blocked(ResultKind::Refused, format!("Couldn't read the backup: {e}")),
        };
        for (code, value) in values {
            let result = self.set(code, value, false);
            if result.outcome != WriteOutcome::Ok {
                return result;
            }
        }
        WriteResult::ok("Backup restored".into(), None)
    }

    /// Reads the finder's codes inside the crash marker.
    fn take_snapshot(&mut self, operation: &str) -> BTreeMap<u8, u16> {
        let codes = finder::snapshot_codes(&self.controls);
        if self.status == MonitorStatus::Connected {
            self.crash.arm(operation);
            self.refresh(&codes, true);
            self.crash.disarm();
        }
        codes.iter().filter_map(|c| self.cache.get(c).map(|r| (*c, r.current))).collect()
    }

    /// Cached Range and Enum values of Normal-safety controls.
    fn backup_values(&self) -> BTreeMap<u8, u16> {
        self.cache.iter()
            .filter(|(code, _)| self.controls.get(code).is_some_and(|d|
                matches!(d.kind, ControlKind::Range | ControlKind::Enum) && d.safety == SafetyClass::Normal))
            .map(|(&code, r)| (code, r.current))
            .collect()
    }

    fn update_cache(&mut self, code: u8, reading: VcpReading) {
        let changed = self.cache.get(&code).is_none_or(|old| old.current != reading.current) && !self.drags.contains(&code);
        self.cache.insert(code, reading);
        self.read_at.insert(code, Instant::now());
        if changed {
            self.emit(Event::Value { code, reading });
        }
    }

    /// Why the safety policy blocked a write: an Enum value the monitor doesn't list is OutOfRange, the rest Refused.
    fn refusal_kind(&self, def: Option<&ControlDefinition>, code: u8, value: u16) -> ResultKind {
        let out_of_range = def.is_some_and(|d| d.kind == ControlKind::Enum
            && !matches!(d.safety, SafetyClass::NeverWrite | SafetyClass::ReadOnlyUntilNamed))
            && self.caps.supports(code)
            && !self.caps.allows_value(code, value);
        if out_of_range { ResultKind::OutOfRange } else { ResultKind::Refused }
    }

    /// The result while not connected, by status.
    fn not_connected(&self) -> WriteResult {
        let (kind, message) = match self.status {
            MonitorStatus::NotFound => (ResultKind::NotFound, "Monitor not found"),
            _ => (ResultKind::NotResponding, "Monitor not responding"),
        };
        WriteResult { outcome: WriteOutcome::NotResponding, kind, message: message.into(), value: None, kept: None }
    }

    /// The result of a failed call on control `name`: Unsupported is final for the control; otherwise the
    /// monitor is lost.
    fn failed_result(&mut self, name: &str, e: &LinkError) -> WriteResult {
        if let LinkError::Unsupported(_) = e {
            return WriteResult::unsupported(name);
        }
        self.failed(e);
        self.not_connected()
    }

    /// After a failed call: a Transient failure marks the monitor lost; `run_op` already handled Gone; Unsupported
    /// is about the code, not the link.
    fn failed(&mut self, e: &LinkError) {
        if let LinkError::Transient(_) = e {
            self.lost();
        }
    }

    /// A failure during normal operation: report it and start recovering.
    fn lost(&mut self) {
        self.set_status(MonitorStatus::NotResponding, None);
        self.request_reconnect();
    }

    /// The connection is gone: report it and reconnect at once, then on the burst's schedule.
    fn gone(&mut self) {
        self.set_status(MonitorStatus::NotResponding, None);
        self.burst = Some(Burst { step: 0, due: Instant::now() });
    }

    fn set_status(&mut self, status: MonitorStatus, failure: Option<LinkFailure>) {
        if self.stopping() {
            return;
        }
        if status == MonitorStatus::Connected {
            self.burst = None;
        }
        if self.status != status || self.failure != failure {
            self.status = status;
            self.failure = failure;
            self.emit(Event::Status { status, failure });
        }
    }

    fn connect(&mut self) -> Result<(), LinkFailure> {
        sleep_until(self.ready_at);
        let result = self.link.connect();
        self.ready_at = Instant::now() + self.timing.gap;
        result
    }

    fn read(&mut self, code: u8) -> Result<VcpReading, LinkError> {
        let attempts = self.timing.attempts;
        self.read_with(code, attempts)
    }

    /// Every input (0x60) reading keeps only its low byte: some monitors put junk in the high byte.
    fn read_with(&mut self, code: u8, attempts: u32) -> Result<VcpReading, LinkError> {
        self.run_op(OpKind::Read, attempts, |l| l.read(code))
            .map(|r| if code == INPUT { VcpReading { current: r.current & 0xFF, ..r } } else { r })
    }

    fn write(&mut self, code: u8, value: u16) -> Result<(), LinkError> {
        let attempts = self.timing.attempts;
        self.run_op(OpKind::Write, attempts, |l| l.write(code, value))
    }

    /// One link call, paced; only `Transient` failures are retried. A `Gone` marks the monitor lost.
    fn run_op<T>(&mut self, kind: OpKind, attempts: u32,
        mut op: impl FnMut(&mut dyn MonitorLink) -> Result<T, LinkError>) -> Result<T, LinkError> {
        sleep_until(self.ready_at);
        let mut attempt = 1;
        let result = loop {
            if self.stopping() {
                break Err(LinkError::Gone("shutting down".into()));
            }
            match (kind, self.last_write) {
                (OpKind::Read, Some(at)) => sleep_until(at + self.timing.settle),
                (OpKind::Read, None) => {}
                (OpKind::Write, _) => self.last_write = Some(Instant::now()),
            }
            match op(self.link.as_mut()) {
                Err(LinkError::Transient(_)) if attempt < attempts => {
                    thread::sleep(self.timing.retry);
                    attempt += 1;
                }
                result => break result,
            }
        };
        if kind == OpKind::Write {
            self.last_write = Some(Instant::now());
        }
        self.ready_at = Instant::now() + self.timing.gap;
        if let Err(LinkError::Gone(e)) = &result {
            if !self.stopping() {
                log::write(&format!("Connection gone: {e}"));
                self.gone();
            }
        }
        result
    }

    fn stopping(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    fn emit(&self, event: Event) {
        (self.sink)(event);
    }

    fn emit_result(&self, origin: Origin, code: Option<u8>, preset: Option<String>, result: WriteResult) {
        self.emit(Event::Result { origin, code, preset, result });
    }

    fn emit_controls(&self) {
        self.emit(Event::Controls(Arc::new(ResolvedControls {
            profile: self.profiles.profile().clone(),
            capabilities: self.caps.clone(),
            controls: self.controls.clone(),
            firmware: self.caps.firmware(),
        })));
    }
}

fn describe(def: &ControlDefinition, value: u16) -> String {
    match def.kind {
        ControlKind::Range => format!("{} {value}", def.name),
        ControlKind::Enum => format!("{} → {}", def.name, value_label(def, value)),
        ControlKind::Action => def.name.clone(),
    }
}

/// The value's label ("HDMI"), else `0xNN`.
fn value_label(def: &ControlDefinition, value: u16) -> String {
    u8::try_from(value).ok().and_then(|v| def.value_labels.get(&v)).cloned()
        .unwrap_or_else(|| format!("0x{value:02X}"))
}

fn saturate(value: u32) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

fn sleep_until(at: Instant) {
    let now = Instant::now();
    if at > now {
        thread::sleep(at - now);
    }
}
