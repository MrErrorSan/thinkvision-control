//! The DDC engine: one worker thread owns the link, the value cache, drag state, status and the retry burst.
//! The app sends [`Command`]s and receives [`Event`]s through its sink; nothing else touches monitor state.
//!
//! Pacing: ≥ `gap` between link calls; reads and capability reads wait until `settle` after the last
//! write (writes are not held, so drags stream); a call failing with [`LinkError::Transient`] is tried up to
//! `attempts` times, `retry` apart, except a Set of input (0x60) or an action code, which goes out once.
//! `Unsupported` is final; `Gone` marks the monitor lost and reconnects at once.
//!
//! Capabilities synthesized from the profile are for display only: writes wait for live or cached ones.
//!
//! While the display is off, the session is locked or the PC sleeps there is no monitor traffic at all.
//! After a crash inside a capability read or the finder the app starts the engine in safe mode.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::crash::{self, Marker};
use crate::finder::{self, CodeChange};
use crate::link::{LinkError, LinkFailure, MonitorLink, VcpReading};
use crate::mccs::{self, MonitorCapabilities};
use crate::profile::{ControlDefinition, ControlKind, Profile, ProfileStore, SafetyClass, Section};
use crate::result::ResultKind;
use crate::safety::{self, Verdict};
use crate::store::{BackupStore, CapabilitiesCache, Preset};
use crate::hub::OpId;
use crate::i18n::{tr, trf, trn};
use crate::identity::{MonitorId, MonitorIdentity};
use crate::support::{self, SupportLevel};
use crate::{hex, log};

const BRIGHTNESS: u8 = 0x10;
const CONTRAST: u8 = 0x12;
const COLOUR_MODE: u8 = 0x14;
const INPUT: u8 = 0x60;
const LANGUAGE: u8 = 0xCC;
/// Codes a Set writes once, with no `Transient` retry: factory resets (0x04, 0x05, 0x08) and input.
const SINGLE_WRITE: [u8; 4] = [0x04, 0x05, 0x08, INPUT];
/// How long [`EngineHandle::shutdown`] waits for the operation in flight (longer than an input confirm).
pub const SHUTDOWN_WAIT: Duration = Duration::from_secs(2);
/// A drag with no `DragTo` for this long is over, even without its `EndDrag`.
const DRAG_EXPIRY: Duration = Duration::from_secs(2);
/// Waits check for shutdown this often.
const PAUSE_SLICE: Duration = Duration::from_millis(50);

/// English, for the log; `not_connected` shows the same text translated.
const NO_CONTROLS_MESSAGE: &str = "The monitor answers but supports none of the controls this app uses";
/// Consecutive failed writes that make the monitor read-only until the next reconnect.
const READ_ONLY_AFTER: u32 = 3;

type Sink = Arc<Mutex<Box<dyn Fn(Event) + Send>>>;

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
        Self::blocked(ResultKind::Unsupported, trf("{0} isn't supported by this monitor.", &[&tr(name)]))
    }
}

/// The controls as resolved against the current capabilities (cached, synthesized or live).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedControls {
    pub profile: Arc<Profile>,
    pub capabilities: MonitorCapabilities,
    /// A definition for every code in `capabilities`.
    pub controls: BTreeMap<u8, ControlDefinition>,
    /// "LB1.2", from the 0x04 value list; only for a Verified monitor.
    pub firmware: Option<String>,
}

/// Commands that would touch the monitor do nothing while asleep; writes then answer `kind: Asleep`.
#[derive(Clone, Debug)]
pub enum Command {
    /// One connect attempt: capabilities (when there is no usable cache, at most once per process, not in safe
    /// mode unless the cache was corrupt), then every Range/Enum code in Main/Advanced plus every cached code.
    /// Connected needs at least one code read. No retry burst; follow with `RequestReconnect` if that's wanted.
    Connect,
    /// Like `Connect`, but reads only `codes` after connecting (`--get brightness`).
    ConnectReading(Vec<u8>),
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
    /// Live slider position: coalesced (newest wins, up to a queued Set or Step for the code), written without
    /// read-back, no `Value` events for the code until `DRAG_EXPIRY` (2 s) after the last one or a status change.
    DragTo { code: u8, value: u16 },
    /// Slider released: writes and verifies once, then emits the actual `Value` and a `Result`.
    EndDrag { code: u8, value: u16, origin: Origin },
    /// Adds `delta` to the cached value (re-reading first if uncached or older than `stale_after`), clamped to 0..max.
    /// An Enum code moves `delta` places through the values the monitor advertises, confirmed like a Set.
    Step { code: u8, delta: i32, origin: Origin },
    /// Colour mode, brightness, contrast; stops at the first result that isn't Ok.
    ApplyPreset { preset: Preset, origin: Origin },
    /// Re-applies the latest backup in preset order (colour mode first), never input, language or actions,
    /// carrying on past values the monitor didn't take; the result counts what was restored.
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
    /// `op` is the hub's op id for the command, None when it came straight to this engine.
    Result { origin: Origin, code: Option<u8>, preset: Option<String>, result: WriteResult, op: Option<OpId> },
    Finder(FinderEvent),
    /// True while the display is off, the session is locked or the PC sleeps, until `wake_delay` after the last
    /// wake event; no monitor traffic meanwhile.
    Asleep(bool),
    /// Sent at start when spawned in safe mode, and whenever it changes.
    SafeMode(bool),
}

/// Everything one monitor's engine starts from.
pub struct EngineConfig {
    pub id: MonitorId,
    pub identity: MonitorIdentity,
    pub link: Box<dyn MonitorLink>,
    /// This monitor's folder: capabilities.txt, profile.user.json, backups, its crash marker.
    pub dir: PathBuf,
    /// None = the resolved profile's timing.
    pub timing: Option<Timing>,
    pub safe_mode: bool,
    /// Forced level (the built-in panel, tests); None = `support::resolve` from the identity and capabilities.
    pub level: Option<SupportLevel>,
}

pub struct Engine;

impl Engine {
    /// Starts the worker on a thread named `tvc-engine-<id>`. The first events are `Controls` (from the capability
    /// cache in `dir`, else synthesized from the profile when the level allows it), `Status(Connecting)` and, with
    /// `safe_mode`, `SafeMode(true)`; send `Connect` to start talking to the monitor. A thread that can't be
    /// spawned is logged and reported as `Status(NotResponding)`.
    pub fn spawn(config: EngineConfig, sink: Box<dyn Fn(Event) + Send>) -> EngineHandle {
        let EngineConfig { id, identity, link, dir, timing, safe_mode, level } = config;
        let (tx, rx) = mpsc::channel();
        let (done_tx, done) = mpsc::channel::<()>();
        let stop = Arc::new(AtomicBool::new(false));
        let sink: Sink = Arc::new(Mutex::new(sink));
        let caps_cache = CapabilitiesCache::new(&dir);
        let mut cached_raw = caps_cache.load();
        let cached = cached_raw.as_deref().map(mccs::parse);
        let corrupt_cache = matches!(cached, Some(Err(_)));
        if let Some(Err(e)) = &cached {
            log::write(&format!("[{id}] Capabilities cache doesn't parse ({e}); reading them from the monitor."));
            cached_raw = None;
        }
        let cached = cached.and_then(Result::ok);
        let forced_level = level.is_some();
        let level = level.unwrap_or_else(|| support::resolve(&identity, cached.as_ref(), false));
        let profiles = ProfileStore::for_level(&dir, &level, cached.as_ref());
        let timing = timing.unwrap_or_else(|| profiles.profile().timing.clone());
        let (caps, caps_known) = match cached {
            Some(caps) => {
                for warning in &caps.warnings {
                    log::write(&format!("[{id}] Capabilities (cached): {warning}"));
                }
                (caps, true)
            }
            None if support::may_synthesize(&level) => (profiles.synthesize_capabilities(), false),
            None => (MonitorCapabilities::default(), false),
        };
        let controls = profiles.resolve(&caps);
        let worker = Worker {
            id: id.clone(),
            identity,
            forced_level,
            link,
            profiles,
            caps_cache,
            crash: Marker::new(&dir, &crash::process_key()),
            level,
            backups: BackupStore::new(&dir),
            dir,
            timing,
            sink: sink.clone(),
            rx,
            stop: stop.clone(),
            pending: VecDeque::new(),
            op: None,
            cached_raw,
            live_caps: false,
            caps_known,
            corrupt_cache,
            caps_failed: false,
            no_controls: false,
            caps,
            controls,
            cache: BTreeMap::new(),
            read_at: BTreeMap::new(),
            drags: BTreeMap::new(),
            status: MonitorStatus::Connecting,
            failure: None,
            ready_at: Instant::now(),
            last_write: None,
            burst: None,
            snapshot: None,
            safe_mode,
            write_failures: 0,
            read_only: false,
            display_off: false,
            locked: false,
            suspended: false,
            resumed_at: None,
            wake_at: None,
        };
        let spawned = thread::Builder::new()
            .name(format!("tvc-engine-{id}"))
            .spawn(move || {
                let _done = done_tx;
                worker.run();
            });
        let dead = AtomicBool::new(false);
        if let Err(e) = spawned {
            log::write(&format!("[{id}] Couldn't start the engine thread ({e}); reporting the monitor as not responding."));
            dead.store(true, Ordering::SeqCst);
            call(&sink, Event::Status { status: MonitorStatus::NotResponding, failure: None });
        }
        EngineHandle { id, tx, stop, done, sink, dead }
    }
}

/// A command and the hub's op id for it, if it came through the hub.
struct Envelope {
    op: Option<OpId>,
    command: Command,
}

/// The app's end of the engine.
pub struct EngineHandle {
    id: MonitorId,
    tx: Sender<Envelope>,
    stop: Arc<AtomicBool>,
    done: Receiver<()>,
    sink: Sink,
    /// The worker is gone; reported once.
    dead: AtomicBool,
}

impl EngineHandle {
    /// A worker that has died is logged once and reported through the sink as `Status(NotResponding)`.
    pub fn send(&self, command: Command) {
        self.post(None, command);
    }

    /// Sends a command from the hub; its `Result` carries `op`. False when the worker is gone.
    pub fn send_op(&self, op: OpId, command: Command) -> bool {
        self.post(Some(op), command)
    }

    fn post(&self, op: Option<OpId>, command: Command) -> bool {
        let sent = self.tx.send(Envelope { op, command }).is_ok();
        if !sent && !self.dead.swap(true, Ordering::SeqCst) {
            log::write(&format!("[{}] Engine thread stopped unexpectedly; reporting the monitor as not responding.",
                self.id));
            call(&self.sink, Event::Status { status: MonitorStatus::NotResponding, failure: None });
        }
        sent
    }

    /// Stops the worker, waiting at most [`SHUTDOWN_WAIT`] for the operation in flight, so exit never hangs on
    /// a monitor that doesn't answer.
    pub fn shutdown(self) {
        self.stop();
        self.join(Instant::now() + SHUTDOWN_WAIT);
    }

    /// Tells the worker to stop, without waiting.
    pub(crate) fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.tx.send(Envelope { op: None, command: Command::Shutdown });
    }

    /// Waits for a stopped worker until `deadline`.
    pub(crate) fn join(self, deadline: Instant) {
        let _ = self.done.recv_timeout(deadline.saturating_duration_since(Instant::now()));
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
    id: MonitorId,
    identity: MonitorIdentity,
    /// `EngineConfig.level` was given: never re-resolved.
    forced_level: bool,
    /// This monitor's folder (profile overlay).
    dir: PathBuf,
    link: Box<dyn MonitorLink>,
    profiles: ProfileStore,
    caps_cache: CapabilitiesCache,
    crash: Marker,
    backups: BackupStore,
    timing: Timing,
    sink: Sink,
    rx: Receiver<Envelope>,
    stop: Arc<AtomicBool>,
    /// Commands taken off the channel while coalescing a drag, handled before the channel.
    pending: VecDeque<Envelope>,
    /// The hub's op id of the command being handled.
    op: Option<OpId>,
    /// The cached capability string, only when it parses.
    cached_raw: Option<String>,
    live_caps: bool,
    /// `caps` came from the monitor (live or cached), not from the profile; writes need this.
    caps_known: bool,
    /// The cache didn't parse: connecting reads the capabilities live, even in safe mode.
    corrupt_cache: bool,
    /// A connect's capability read failed: later connects don't repeat it (`ReadCapabilities` still does).
    caps_failed: bool,
    /// The last connect got only "unsupported" answers.
    no_controls: bool,
    /// For display; trusted for writes only with `caps_known`.
    caps: MonitorCapabilities,
    /// How far this monitor is trusted: forced, or `support::resolve` at start and after each capability read.
    level: SupportLevel,
    controls: BTreeMap<u8, ControlDefinition>,
    cache: BTreeMap<u8, VcpReading>,
    /// When each cached value was last read or written.
    read_at: BTreeMap<u8, Instant>,
    /// Codes being dragged, with the time of their last `DragTo`.
    drags: BTreeMap<u8, Instant>,
    status: MonitorStatus,
    failure: Option<LinkFailure>,
    ready_at: Instant,
    last_write: Option<Instant>,
    burst: Option<Burst>,
    snapshot: Option<BTreeMap<u8, u16>>,
    safe_mode: bool,
    /// Consecutive write commands that failed on the link (not refusals).
    write_failures: u32,
    /// After `READ_ONLY_AFTER` failed writes: writes are refused until the next reconnect.
    read_only: bool,
    display_off: bool,
    locked: bool,
    suspended: bool,
    resumed_at: Option<Instant>,
    /// Set after the last wake event: the wake recheck runs then.
    wake_at: Option<Instant>,
}

impl Worker {
    fn run(mut self) {
        log::set_prefix(format!("[{}] ", self.id));
        self.emit_controls();
        self.emit(Event::Status { status: self.status, failure: None });
        if self.safe_mode {
            self.emit(Event::SafeMode(true));
        }
        while let Some(Envelope { op, command }) = self.next() {
            self.op = op;
            match command {
                Command::Shutdown => break,
                command => self.handle(command),
            }
        }
        self.link.disconnect();
    }

    /// The next command, running the wake recheck and retry-burst steps as they fall due.
    fn next(&mut self) -> Option<Envelope> {
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

    /// A command from the command line answers in English (its result goes to stdout and JSON); the OSD that run
    /// shows the same English text.
    fn handle(&mut self, command: Command) {
        let cli = matches!(command,
            Command::Set { origin: Origin::Cli, .. } | Command::Step { origin: Origin::Cli, .. }
            | Command::EndDrag { origin: Origin::Cli, .. } | Command::ApplyPreset { origin: Origin::Cli, .. }
            | Command::RestoreBackup { origin: Origin::Cli });
        if cli {
            crate::i18n::english(|| self.handle_command(command));
        } else {
            self.handle_command(command);
        }
    }

    fn handle_command(&mut self, command: Command) {
        if matches!(command, Command::Connect | Command::ConnectReading(_) | Command::RecheckAfterEvent) {
            self.clear_read_only();
        }
        match command {
            Command::Connect => {
                if !self.asleep() {
                    self.reconnect();
                }
            }
            Command::ConnectReading(codes) => {
                if !self.asleep() {
                    self.connect_reading(Some(&codes));
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
        self.asleep().then(|| WriteResult::blocked(ResultKind::Asleep, tr("The monitor is asleep or this PC is locked")))
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
        self.connect_reading(None);
    }

    /// One connect attempt, then reads `only` (else every Main/Advanced Range/Enum code plus the cached ones).
    fn connect_reading(&mut self, only: Option<&[u8]>) {
        self.no_controls = false;
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
            if failure == LinkFailure::NotFound {
                self.clear_read_only(); // unplugged: it starts over when it's back
            }
            self.set_status(MonitorStatus::NotFound, Some(failure));
            return;
        }
        // With a cached capability string, connect on values alone; `ReadCapabilities` refreshes it later. A failed
        // read isn't repeated by later connects, and the value reads decide whether the monitor answers.
        let may_read = !self.safe_mode || self.corrupt_cache;
        if !self.live_caps && self.cached_raw.is_none() && !self.caps_failed && may_read && !self.read_capabilities() {
            self.caps_failed = true;
            let level = self.level.clone();
            self.resolve_level(None);
            if self.level != level {
                self.emit_controls();
            }
        }
        let codes: Vec<u8> = match only {
            Some(codes) => codes.to_vec(),
            None => {
                let mut codes: BTreeSet<u8> = self.controls.values()
                    .filter(|d| matches!(d.kind, ControlKind::Range | ControlKind::Enum)
                        && matches!(d.section, Section::Main | Section::Advanced))
                    .map(|d| d.code)
                    .collect();
                codes.extend(self.cache.keys());
                codes.into_iter().collect()
            }
        };
        match self.refresh(&codes, false) {
            Some(0) if !codes.is_empty() => {
                log::write(&format!("Connect: every read was unsupported. {NO_CONTROLS_MESSAGE}."));
                self.no_controls = true;
                self.set_status(MonitorStatus::NotResponding, None);
            }
            Some(_) => self.set_status(MonitorStatus::Connected, None),
            None => {}
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
        self.caps_known = true;
        self.corrupt_cache = false;
        let cached = self.cached_raw.as_deref() == Some(raw.as_str());
        let same = if cached { ", matches the cache" } else { "" };
        log::write(&format!("Capabilities read (live, {} bytes{same})", raw.len()));
        if cached {
            return true; // its warnings were logged when the cache loaded
        }
        log_warnings("live", &caps);
        self.caps_cache.save(&raw);
        self.cached_raw = Some(raw);
        self.caps = caps;
        self.resolve_level(Some(self.caps.clone()));
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

    /// Re-resolves the support level from `caps` (None: the live read failed and nothing is cached), rebuilds the
    /// profile for it and re-resolves the controls. A forced level keeps its profile.
    fn resolve_level(&mut self, caps: Option<MonitorCapabilities>) {
        if !self.forced_level {
            let level = support::resolve(&self.identity, caps.as_ref(), caps.is_none());
            if level != self.level {
                log::write(&format!("Support level: {level:?}"));
            }
            self.profiles = ProfileStore::for_level(&self.dir, &level, caps.as_ref());
            self.level = level;
        }
        self.controls = self.profiles.resolve(&self.caps);
    }

    fn clear_read_only(&mut self) {
        self.write_failures = 0;
        self.read_only = false;
    }

    /// A write command failed on the link; the third in a row makes the monitor read-only.
    fn note_write_failure(&mut self) {
        self.write_failures += 1;
        if self.write_failures >= READ_ONLY_AFTER && !self.read_only {
            log::write(&format!("{READ_ONLY_AFTER} writes in a row failed; read-only until the next reconnect."));
            self.read_only = true;
        }
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

    /// Reads `codes` into the cache; the number read. None if none answered (an "unsupported" reply counts as an
    /// answer); then the monitor is NotResponding, and with `recover` the retry burst starts. Stops at a `Gone`.
    fn refresh(&mut self, codes: &[u8], recover: bool) -> Option<usize> {
        let (mut read, mut answered) = (0, 0);
        for &code in codes {
            match self.read(code) {
                Ok(reading) => {
                    read += 1;
                    answered += 1;
                    self.update_cache(code, reading);
                }
                Err(e) => {
                    log::write(&format!("Read {} failed: {e}", hex(code)));
                    match e {
                        LinkError::Unsupported(_) => answered += 1,
                        LinkError::Gone(_) => return None, // `run_op` already marked the monitor lost
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
            return None;
        }
        Some(read)
    }

    /// True while `code` is being dragged: its last `DragTo` is under `DRAG_EXPIRY` old.
    fn dragging(&self, code: u8) -> bool {
        self.drags.get(&code).is_some_and(|at| at.elapsed() < DRAG_EXPIRY)
    }

    fn poll(&mut self, codes: &[u8]) {
        for &code in codes {
            let dragging = self.drags.values().any(|at| at.elapsed() < DRAG_EXPIRY);
            let changing = dragging || self.last_write.is_some_and(|t| t.elapsed() < self.timing.poll_quiet);
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

    /// Why no write can go out now, if none can: asleep, not connected, or no live or cached capabilities yet.
    fn write_gate(&self) -> Option<WriteResult> {
        if let Some(result) = self.hold() {
            return Some(result);
        }
        if self.status != MonitorStatus::Connected {
            return Some(self.not_connected());
        }
        if self.read_only {
            return Some(WriteResult::blocked(ResultKind::Refused, tr("Changes are off for this monitor after repeated failed writes; reconnect it to try again.")));
        }
        (!self.caps_known).then(|| WriteResult::blocked(ResultKind::Refused,
            tr("The monitor's capabilities haven't been read yet.")))
    }

    fn set(&mut self, code: u8, value: u16, confirmed: bool) -> WriteResult {
        if let Some(result) = self.write_gate() {
            return result;
        }
        let def = self.controls.get(&code).cloned();
        let cached = self.cache.get(&code).copied();
        let max = cached.filter(|_| def.as_ref().is_some_and(|d| d.kind == ControlKind::Range)).map(|c| c.max);
        let decision = safety::evaluate(def.as_ref(), &self.caps, code, value, max, &self.level, confirmed);
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
                return WriteResult::blocked(ResultKind::Refused, trf("Backup failed — {0} cancelled.", &[&tr(&def.name)]));
            }
        }

        // A retried reset, action or input write could act twice (the Linux backend uses the same list).
        let once = SINGLE_WRITE.contains(&code) || def.kind == ControlKind::Action;
        let attempts = if once { 1 } else { self.timing.attempts };
        if let Err(e) = self.write(code, v, attempts) {
            log::write(&format!("Write {}={v} failed: {e}", hex(code)));
            return self.failed_result(&def.name, &e);
        }

        match def.kind {
            ControlKind::Range => match self.read(code) {
                Ok(actual) => {
                    self.update_cache(code, actual);
                    log::write(&format!("{} DDC write verified: requested={v}, readback={}, max={}.", def.name,
                        actual.current, actual.max));
                    if actual.current != v {
                        return WriteResult {
                            kept: Some(actual.current),
                            ..WriteResult::blocked(ResultKind::Kept, trf(
                                "{0} remains {1}; the monitor did not apply {2}.", &[&tr(&def.name), &actual.current, &v]))
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
        self.pause_until(self.last_write.unwrap_or_else(Instant::now) + self.timing.input_confirm);
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
                            trf("The monitor stayed on {0}", &[&tr(&value_label(def, reading.current))]))
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
        self.drags.insert(code, Instant::now());
        self.pause_until(self.ready_at);
        // Let newer positions arrive during the gap, then write only the newest.
        let Some(newest) = self.coalesce(code, value) else { return };
        let Some(v) = self.evaluate_drag(code, newest) else { return };
        let attempts = self.timing.attempts;
        match self.write(code, v, attempts) {
            Ok(()) => {
                let max = self.cache.get(&code).map_or(100, |c| c.max);
                self.update_cache(code, VcpReading { current: v, max });
            }
            Err(e) => {
                log::write(&format!("Drag write {}={v} failed: {e}", hex(code)));
                if !matches!(e, LinkError::Unsupported(_)) {
                    self.note_write_failure();
                }
                self.failed(&e);
            }
        }
    }

    fn evaluate_drag(&self, code: u8, value: u16) -> Option<u16> {
        if !self.caps_known || self.read_only {
            return None;
        }
        let max = self.cache.get(&code).map(|c| c.max);
        let d = safety::evaluate(self.controls.get(&code), &self.caps, code, value, max, &self.level, false);
        (d.verdict == Verdict::Allow).then_some(d.value)
    }

    /// Drains the channel and removes queued `DragTo`s for `code` up to the first Set or Step for it, returning the
    /// newest value; None when an `EndDrag` for `code` is queued before that (it writes the final value).
    fn coalesce(&mut self, code: u8, value: u16) -> Option<u16> {
        self.pending.extend(self.rx.try_iter());
        let mut newest = value;
        let mut i = 0;
        while i < self.pending.len() {
            match self.pending[i].command {
                Command::DragTo { code: c, value: v } if c == code => {
                    newest = v;
                    self.pending.remove(i);
                }
                Command::EndDrag { code: c, .. } if c == code => return None,
                Command::Set { code: c, .. } | Command::Step { code: c, .. } if c == code => break,
                _ => i += 1,
            }
        }
        Some(newest)
    }

    fn step(&mut self, code: u8, delta: i32) -> WriteResult {
        if let Some(result) = self.write_gate() {
            return result;
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
        if self.controls.get(&code).is_some_and(|d| d.kind == ControlKind::Enum) {
            let target = self.enum_step(code, reading.current, delta);
            return self.set(code, target, false);
        }
        let target = (i64::from(reading.current) + i64::from(delta)).clamp(0, i64::from(reading.max));
        self.set(code, target as u16, false)
    }

    /// The value `delta` places from `current` among those the monitor allows for `code`, stopping at the last one.
    fn enum_step(&self, code: u8, current: u16, delta: i32) -> u16 {
        let allowed = |v: &u16| self.caps.allows_value(code, *v);
        let places = delta.unsigned_abs() as usize;
        let found = if delta > 0 {
            (current.saturating_add(1)..=0xFF).filter(allowed).take(places).last()
        } else {
            (0..current.min(0x100)).rev().filter(allowed).take(places).last()
        };
        found.unwrap_or(current)
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
        WriteResult::ok(trf("{0} preset", &[&tr(&preset.name)]), None)
    }

    fn restore_backup(&mut self) -> WriteResult {
        let mut values = match self.backups.load_latest() {
            Ok(Some(values)) => values,
            Ok(None) => return WriteResult::blocked(ResultKind::Refused, tr("No backup found.")),
            Err(e) => return WriteResult::blocked(ResultKind::Refused, trf("Couldn't read the backup: {0}", &[&e])),
        };
        if let Some(result) = self.write_gate() {
            return result;
        }
        values.retain(|&code, _| self.restorable(code));
        // As `apply_preset`: colour mode first, since it can lock brightness and contrast.
        let mut steps: Vec<(u8, u16)> = [COLOUR_MODE, BRIGHTNESS, CONTRAST].into_iter()
            .filter_map(|code| values.remove(&code).map(|v| (code, v)))
            .collect();
        steps.extend(values);
        let total = steps.len();
        let (mut restored, mut kind, mut notes) = (0, None, Vec::new());
        for (code, value) in steps {
            let result = self.set(code, value, false);
            match result.outcome {
                WriteOutcome::Ok => restored += 1,
                WriteOutcome::NotResponding => {
                    return WriteResult { message: trn("{0}; restored {1} of {2} settings.", "{0}; restored {1} of {2} settings.",
                        total as u64, &[&result.message, &restored, &total]),
                        ..result };
                }
                _ => {
                    kind.get_or_insert(result.kind);
                    notes.push(result.message);
                }
            }
        }
        match kind {
            None => WriteResult::ok(tr("Backup restored"), None),
            Some(kind) => WriteResult::blocked(kind,
                trn("Restored {0} of {1} settings. {2}", "Restored {0} of {1} settings. {2}", total as u64,
                    &[&restored, &total, &notes.join(" ")])),
        }
    }

    /// Backed up and restored: not input (it would switch away from this PC), not the OSD language, no action.
    fn restorable(&self, code: u8) -> bool {
        !matches!(code, INPUT | LANGUAGE) && self.controls.get(&code).is_none_or(|d| d.kind != ControlKind::Action)
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

    /// Cached Range and Enum values of restorable Normal-safety controls.
    fn backup_values(&self) -> BTreeMap<u8, u16> {
        self.cache.iter()
            .filter(|(&code, _)| self.restorable(code) && self.controls.get(&code).is_some_and(|d|
                matches!(d.kind, ControlKind::Range | ControlKind::Enum) && d.safety == SafetyClass::Normal))
            .map(|(&code, r)| (code, r.current))
            .collect()
    }

    fn update_cache(&mut self, code: u8, reading: VcpReading) {
        let changed = self.cache.get(&code).is_none_or(|old| old.current != reading.current) && !self.dragging(code);
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
            MonitorStatus::NotFound => (ResultKind::NotFound, tr("Monitor not found")),
            MonitorStatus::NotResponding if self.no_controls => (ResultKind::NotResponding,
                tr("The monitor answers but supports none of the controls this app uses")),
            _ => (ResultKind::NotResponding, tr("Monitor not responding")),
        };
        WriteResult { outcome: WriteOutcome::NotResponding, kind, message, value: None, kept: None }
    }

    /// The result of a failed call on control `name`: Unsupported is final for the control; otherwise the
    /// monitor is lost.
    fn failed_result(&mut self, name: &str, e: &LinkError) -> WriteResult {
        if let LinkError::Unsupported(_) = e {
            return WriteResult::unsupported(name);
        }
        self.note_write_failure();
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
            self.drags.clear();
            self.status = status;
            self.failure = failure;
            self.emit(Event::Status { status, failure });
        }
    }

    fn connect(&mut self) -> Result<(), LinkFailure> {
        self.pause_until(self.ready_at);
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

    fn write(&mut self, code: u8, value: u16, attempts: u32) -> Result<(), LinkError> {
        let result = self.run_op(OpKind::Write, attempts, |l| l.write(code, value));
        if result.is_ok() {
            self.write_failures = 0;
        }
        result
    }

    /// One link call, paced; only `Transient` failures are retried. A `Gone` marks the monitor lost. Every wait
    /// ends early on shutdown, and no call starts after it.
    fn run_op<T>(&mut self, kind: OpKind, attempts: u32,
        mut op: impl FnMut(&mut dyn MonitorLink) -> Result<T, LinkError>) -> Result<T, LinkError> {
        self.pause_until(self.ready_at);
        let mut attempt = 1;
        let result = loop {
            if let (OpKind::Read, Some(at)) = (kind, self.last_write) {
                self.pause_until(at + self.timing.settle);
            }
            if self.stopping() {
                break Err(LinkError::Gone("shutting down".into()));
            }
            if kind == OpKind::Write {
                self.last_write = Some(Instant::now());
            }
            match op(self.link.as_mut()) {
                Err(LinkError::Transient(_)) if attempt < attempts => {
                    self.pause_until(Instant::now() + self.timing.retry);
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

    /// Sleeps until `at`, or until shutdown.
    fn pause_until(&self, at: Instant) {
        while !self.stopping() {
            let now = Instant::now();
            if at <= now {
                return;
            }
            thread::sleep((at - now).min(PAUSE_SLICE));
        }
    }

    fn emit(&self, event: Event) {
        call(&self.sink, event);
    }

    fn emit_result(&self, origin: Origin, code: Option<u8>, preset: Option<String>, result: WriteResult) {
        self.emit(Event::Result { origin, code, preset, result, op: self.op });
    }

    fn emit_controls(&self) {
        self.emit(Event::Controls(Arc::new(ResolvedControls {
            profile: self.profiles.profile().clone(),
            capabilities: self.caps.clone(),
            controls: self.controls.clone(),
            firmware: matches!(self.level, SupportLevel::Verified { .. }).then(|| self.caps.firmware()).flatten(),
        })));
    }
}

/// Logs what the parser skipped in a capability string (`source`: "live" or "cached").
fn log_warnings(source: &str, caps: &MonitorCapabilities) {
    for warning in &caps.warnings {
        log::write(&format!("Capabilities ({source}): {warning}"));
    }
}

fn call(sink: &Sink, event: Event) {
    (sink.lock().unwrap_or_else(|e| e.into_inner()))(event);
}

fn describe(def: &ControlDefinition, value: u16) -> String {
    match def.kind {
        ControlKind::Range => format!("{} {value}", tr(&def.name)),
        ControlKind::Enum => format!("{} → {}", tr(&def.name), tr(&value_label(def, value))),
        ControlKind::Action => tr(&def.name),
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
