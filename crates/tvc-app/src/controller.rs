//! The UI-thread side of the app. Owns what the window, tray, hotkeys and OSD show, turns their actions into engine
//! commands, and routes engine events back by origin: window → card notes and toasts; hotkey, tray, CLI → OSD.
//!
//! Background-mode rules (measured on Slint): the main window is dropped on
//! close and rebuilt on open; values are pushed into it only while it is shown; the OSD window is kept.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use slint::winit_030::WinitWindowAccessor;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use tvc_core::cli::{self as tvc_cli, CliCommand, GetTarget, Parsed};
use tvc_core::engine::{
    Command, Event, FinderEvent, MonitorStatus, Origin, PowerState, ResolvedControls, WriteOutcome, WriteResult,
};
use tvc_core::hub::{HubCommand, HubEvent, HubHandle, Monitor, Target};
use tvc_core::identity::{BackendKind, ListError, MonitorId};
use tvc_core::result::ResultKind;
use tvc_core::schedule::{self, AutoMode, Phase};
use tvc_core::finder::CodeChange;
use tvc_core::hotkey::{Action, Gesture};
use tvc_core::link::{LinkFailure, VcpReading};
use tvc_core::profile::{ControlDefinition, ControlKind, Profile, SafetyClass, Section};
use tvc_core::store::{Preset, Settings, SettingsStore};
use tvc_core::crash::LeftOver;
use tvc_core::log;

use crate::osd::{Content, OsdHost};
use crate::localtime;
use crate::platform;
use crate::platform::instance::{Reply, Responder};
use crate::platform::monitor_events::SystemEvent;
use crate::view::{self, glyph, AttachedDisplay, BRIGHTNESS, COLOUR_MODE, CONTRAST, DISPLAY_CODES, INPUT};
use crate::{App, Dialog, MainWindow, Severity, ShortcutEditor, Tray};

const POLL_EVERY: Duration = Duration::from_secs(3);
const BANNER_HOLD: Duration = Duration::from_secs(1);
const KEY_DEBOUNCE: Duration = Duration::from_millis(300);
const TOAST_FOR: Duration = Duration::from_millis(2500);
const HOTKEY_STEP: i32 = 5;
const AUTO_EVERY: Duration = Duration::from_secs(60);
/// How long a one-shot command-line run may take from dispatch; longer when it must read the capabilities first.
const HEADLESS_BUDGET: Duration = Duration::from_secs(10);
const HEADLESS_COLD_BUDGET: Duration = Duration::from_secs(20);
const RETRY_FOR: Duration = Duration::from_secs(16);
const TIMED_OUT: &str = "Timed out: the monitor didn't answer in time";
const NEEDS_YES: &str = "Switching the input needs confirmation: run it again with --yes";
const NOT_CONFIRMED: &str = "The input switch wasn't confirmed on screen";

thread_local! {
    static APP: RefCell<Option<Rc<Controller>>> = const { RefCell::new(None) };
}

/// Runs `f` with the controller, if it exists (callbacks can arrive during shutdown).
pub fn with(f: impl FnOnce(&Rc<Controller>)) {
    if let Some(c) = APP.with(|a| a.borrow().clone()) {
        f(&c);
    }
}

pub fn install(c: Rc<Controller>) {
    APP.with(|a| *a.borrow_mut() = Some(c));
}

pub fn uninstall() -> Option<Rc<Controller>> {
    APP.with(|a| a.borrow_mut().take())
}

#[derive(Default)]
pub struct FinderState {
    pub busy: bool,
    pub status: Option<(Severity, String)>,
    pub snapshot_count: Option<usize>,
    pub changes: Vec<CodeChange>,
    pub selected: Option<usize>,
    pub name: String,
    /// (value, caption, label) rows of the naming form.
    pub values: Vec<(u16, String, String)>,
    /// Bumped whenever the naming form is refilled, so the UI resets its text fields.
    pub form: i32,
}

struct EditorState {
    action: Action,
    original: String,
    /// The gesture text being proposed ("" = cleared).
    result: String,
    held: (bool, bool, bool, bool),
}

pub struct State {
    pub settings: Settings,
    pub profile: Arc<Profile>,
    pub controls: Option<Arc<ResolvedControls>>,
    pub values: BTreeMap<u8, VcpReading>,
    /// The monitor was lost since `values` were read; cleared once a connect has re-read them.
    pub values_stale: bool,
    pub status: MonitorStatus,
    pub failure: Option<LinkFailure>,
    /// The displays the OS reported at the last rescan; None = the backend can't list them.
    pub displays: Option<Vec<AttachedDisplay>>,
    /// What the banner shows: non-connected states wait `BANNER_HOLD` so the bar never flashes.
    pub shown_status: (Option<MonitorStatus>, Option<LinkFailure>),
    pub range_note: String,
    pub notes: BTreeMap<u8, String>,
    pub revisions: BTreeMap<u8, i32>,
    pub restore_text: String,
    pub setup_hints: Vec<platform::setup::Hint>,
    pub hotkeys_available: bool,
    pub failed_hotkeys: Vec<Action>,
    pub finder: FinderState,
    /// Display off, session locked or PC asleep: the engine sends nothing.
    pub asleep: bool,
    /// Started after a crash during a risky monitor operation: no writes until the user leaves it.
    pub safe_mode: bool,
    /// Last error from the Automatic Day/Night form, shown under it.
    pub auto_note: String,
    /// "Try again" was pressed: the banner shows progress until the status changes or 16 s pass.
    pub retrying: bool,
    /// Why the last Day or Night preset from the window failed, shown on the preset cards; "" after a success.
    pub preset_note: String,
    /// A capability read was asked for and its `Controls` event hasn't come yet: the window shows progress.
    pub reading_capabilities: bool,
}

impl State {
    pub fn def(&self, code: u8) -> Option<&ControlDefinition> {
        self.controls.as_ref()?.controls.get(&code)
    }

    pub fn value(&self, code: u8) -> Option<u16> {
        self.values.get(&code).map(|r| r.current)
    }

    pub fn firmware(&self) -> Option<&str> {
        self.controls.as_ref()?.firmware.as_deref()
    }

    pub fn revision(&self, code: u8) -> i32 {
        self.revisions.get(&code).copied().unwrap_or(0)
    }

    fn bump(&mut self, code: u8) {
        *self.revisions.entry(code).or_default() += 1;
    }

    fn input_label(&self, code: u16) -> String {
        view::inputs(self).into_iter().find(|(c, _)| *c == code).map(|(_, l)| l).unwrap_or_else(|| format!("0x{code:02X}"))
    }
}

type Pending = Box<dyn FnOnce(&Rc<Controller>)>;

/// A command-line verb being run: this process's own, or one forwarded by a later launch.
pub struct CliJob {
    command: CliCommand,
    json: bool,
    yes: bool,
    /// The forwarding launch's responder; None for this process's own verb.
    reply: Option<Responder>,
}

impl CliJob {
    pub fn own(parsed: &Parsed) -> Self {
        Self { command: parsed.command, json: parsed.json, yes: parsed.yes, reply: None }
    }
}

/// A command-line `--input` waiting for the on-screen confirm, shown now or queued behind another dialog. Dropped
/// unconfirmed (Cancel, the window closed), it answers exit 3.
struct AwaitingConfirm(Option<CliJob>);

impl Drop for AwaitingConfirm {
    fn drop(&mut self) {
        if let Some(CliJob { command, json, reply: Some(reply), .. }) = self.0.take() {
            log::write(&format!("CLI {} (forwarded): exit {}, {NOT_CONFIRMED}", describe(&command), tvc_cli::EXIT_NEEDS_CONFIRMATION));
            let line = failure_line(&command, json, "NeedsConfirmation", NOT_CONFIRMED);
            reply.reply(Reply { exit: tvc_cli::EXIT_NEEDS_CONFIRMATION as u8, stdout: format!("{line}\n") });
        }
    }
}

pub struct Controller {
    /// Taken at exit by `take_hub`.
    hub: RefCell<Option<HubHandle>>,
    /// The one monitor this app drives until multi-display lands in the controller: the first attached DDC monitor,
    /// preferring the profile's EDID id. None = none attached.
    tracked: RefCell<Option<MonitorId>>,
    /// Sent to the tracked monitor first (`--get brightness` reads only what it prints); later ones get `Connect`.
    first_command: RefCell<Option<Command>>,
    store: SettingsStore,
    st: RefCell<State>,
    window: RefCell<Option<MainWindow>>,
    osd: Rc<RefCell<OsdHost>>,
    tray: RefCell<Option<Tray>>,
    hotkeys: RefCell<Option<platform::hotkeys::Hotkeys>>,
    confirm: RefCell<Option<Pending>>,
    /// Confirms asked while another was open, shown in turn once it is answered.
    dialogs: RefCell<VecDeque<(Dialog, Pending)>>,
    editor: RefCell<Option<EditorState>>,
    poll: slint::Timer,
    banner_hold: slint::Timer,
    toast: slint::Timer,
    debounce: RefCell<HashMap<u8, slint::Timer>>,
    scheme: i32,
    /// `--demo`: never touch the real autostart entry.
    demo: bool,
    /// A one-shot CLI run with no resident app: exit after the OSD.
    headless: Cell<bool>,
    /// The headless run's own verb and `--json`, for its timeout line.
    headless_job: Cell<Option<(CliCommand, bool)>>,
    /// The headless run's own verb has printed its result.
    done: Cell<bool>,
    /// Verbs that arrived before the first connect attempt finished.
    pending_cli: RefCell<Vec<CliJob>>,
    /// Verbs sent to the engine, oldest first: the engine answers `Origin::Cli` commands in order.
    waiting: RefCell<VecDeque<CliJob>>,
    watcher: RefCell<Option<platform::monitor_events::Watcher>>,
    /// Process exit code for a headless run.
    exit_code: Cell<i32>,
    /// Crash markers of dead processes: this run is in safe mode because of them. A resident start clears them.
    left_over: RefCell<Vec<LeftOver>>,
    retry_timer: slint::Timer,
    auto_timer: slint::Timer,
    /// The Day/Night phase last applied by the schedule; a preset is applied only when it changes.
    auto_phase: Cell<Option<Phase>>,
    /// A `Controls` event after the first one (from the cache or the profile) came: the capabilities are live.
    caps_live: Cell<bool>,
}

impl Controller {
    pub fn new(hub: HubHandle, profile: Arc<Profile>, data_dir: PathBuf, scheme: i32, demo: bool,
        left_over: Vec<LeftOver>) -> Rc<Self> {
        let store = SettingsStore::new(&data_dir);
        let settings = store.load();
        let last_phase = settings.last_applied_phase;
        Rc::new(Self {
            hub: RefCell::new(Some(hub)),
            tracked: RefCell::new(None),
            first_command: RefCell::new(None),
            store,
            st: RefCell::new(State {
                settings,
                profile,
                controls: None,
                values: BTreeMap::new(),
                values_stale: false,
                status: MonitorStatus::Connecting,
                failure: None,
                displays: None,
                shown_status: (None, None),
                range_note: String::new(),
                notes: BTreeMap::new(),
                revisions: BTreeMap::new(),
                restore_text: "Put back the settings saved before the last reset".into(),
                setup_hints: vec![],
                hotkeys_available: true,
                failed_hotkeys: vec![],
                finder: FinderState::default(),
                asleep: false,
                safe_mode: false,
                auto_note: String::new(),
                retrying: false,
                preset_note: String::new(),
                reading_capabilities: false,
            }),
            window: RefCell::new(None),
            osd: Rc::new(RefCell::new(OsdHost::default())),
            tray: RefCell::new(None),
            hotkeys: RefCell::new(None),
            confirm: RefCell::new(None),
            dialogs: RefCell::new(VecDeque::new()),
            editor: RefCell::new(None),
            poll: slint::Timer::default(),
            banner_hold: slint::Timer::default(),
            toast: slint::Timer::default(),
            debounce: RefCell::new(HashMap::new()),
            scheme,
            demo,
            headless: Cell::new(false),
            headless_job: Cell::new(None),
            done: Cell::new(false),
            pending_cli: RefCell::new(vec![]),
            waiting: RefCell::new(VecDeque::new()),
            watcher: RefCell::new(None),
            exit_code: Cell::new(0),
            left_over: RefCell::new(left_over),
            retry_timer: slint::Timer::default(),
            auto_timer: slint::Timer::default(),
            auto_phase: Cell::new(last_phase),
            caps_live: Cell::new(false),
        })
    }

    pub fn background_mode(&self) -> bool {
        self.st.borrow().settings.background_mode
    }

    /// Sends to the tracked monitor's engine. With none attached, a reconnect request looks for one instead.
    pub fn send(&self, command: Command) {
        let hub = self.hub.borrow();
        let Some(hub) = hub.as_ref() else { return };
        match self.tracked.borrow().clone() {
            Some(id) => hub.send(HubCommand::Send { target: Target::One(id), op: hub.next_op(), command: Box::new(command) }),
            None if matches!(command, Command::RequestReconnect | Command::RecheckAfterEvent) => hub.send(HubCommand::Rescan),
            None => {}
        }
    }

    fn send_hub(&self, command: HubCommand) {
        if let Some(hub) = self.hub.borrow().as_ref() {
            hub.send(command);
        }
    }

    /// The command the tracked monitor gets first, once one is attached.
    pub fn connect_with(&self, command: Command) {
        *self.first_command.borrow_mut() = Some(command);
    }

    pub fn exit_code(&self) -> i32 {
        self.exit_code.get()
    }

    pub fn take_hub(&self) -> Option<HubHandle> {
        self.hub.borrow_mut().take()
    }

    // ---------------------------------------------------------------------------------------------------------
    // Lifecycle

    /// A resident start: tray and hotkeys per settings, connect, then the window unless starting to the tray.
    pub fn start_resident(self: &Rc<Self>, show_window: bool) {
        self.headless.set(false);
        for stale in self.left_over.take() {
            stale.clear(); // the safe-mode banner now speaks for it
        }
        if self.watcher.borrow().is_none() {
            // First resident start (also a headless run turned resident): retry burst and display/power events.
            self.send(Command::RequestReconnect);
            let watcher = platform::monitor_events::watch(move |event| {
                // None = the displays changed.
                let power = match event {
                    SystemEvent::DisplayChanged => None,
                    #[cfg(any(windows, target_os = "macos"))]
                    SystemEvent::DisplayOff => Some(PowerState::DisplayOff),
                    #[cfg(any(windows, target_os = "macos"))]
                    SystemEvent::DisplayOn => Some(PowerState::DisplayOn),
                    SystemEvent::Locked => Some(PowerState::Locked),
                    SystemEvent::Unlocked => Some(PowerState::Unlocked),
                    SystemEvent::Suspending => Some(PowerState::Suspending),
                    SystemEvent::Resumed => Some(PowerState::Resumed),
                    #[cfg(windows)]
                    SystemEvent::ThemeChanged => {
                        let _ = slint::invoke_from_event_loop(|| with(|c| c.update_tray()));
                        return;
                    }
                    #[cfg(windows)]
                    SystemEvent::SessionEnding => {
                        log::write("Closing: Windows asked (sign-out, shutdown or an installer)");
                        let _ = slint::invoke_from_event_loop(|| with(|c| c.quit()));
                        return;
                    }
                };
                let _ = slint::invoke_from_event_loop(move || with(|c| c.on_system(power)));
            });
            *self.watcher.borrow_mut() = Some(watcher);
            if !self.demo {
                platform::url_scheme::register();
            }
        }
        self.apply_settings();
        if show_window {
            self.show_window();
        } else if self.window.borrow().is_none() {
            platform::efficiency::set_window_visible(false);
        }
        self.arm_banner_hold();
    }

    /// One CLI command without a resident app: connect once, run, show the OSD, exit. The budget runs from here;
    /// `cold` (no cached capabilities, so the connect reads them first) gets the longer one.
    pub fn start_headless(self: &Rc<Self>, job: CliJob, cold: bool) {
        self.headless.set(true);
        self.headless_job.set(Some((job.command, job.json)));
        self.pending_cli.borrow_mut().push(job);
        let budget = if cold { HEADLESS_COLD_BUDGET } else { HEADLESS_BUDGET };
        slint::Timer::single_shot(budget, || with(|c| c.headless_timeout()));
    }

    /// A verb to run once the first connect attempt finishes (`--demo` runs resident).
    pub fn queue_cli(&self, job: CliJob) {
        self.pending_cli.borrow_mut().push(job);
    }

    fn own_job(&self) -> Option<CliJob> {
        let (command, json) = self.headless_job.get()?;
        Some(CliJob { command, json, yes: false, reply: None })
    }

    /// The budget ran out before the headless verb's result: say so and exit 8.
    fn headless_timeout(&self) {
        if !self.headless.get() || self.done.get() {
            return;
        }
        if let Some(job) = self.own_job() {
            let json = failure_json(&job.command, "TimedOut", TIMED_OUT);
            self.finish(job, tvc_cli::exit_code(None, true), TIMED_OUT, json, false);
        }
    }

    fn quit_if_headless(&self) {
        if self.headless.get() {
            self.quit();
        }
    }

    /// Answers a finished verb: replies to the launch that forwarded it, or prints this process's own result
    /// and exits (once the OSD has faded, with `osd`).
    fn finish(&self, job: CliJob, exit: i32, text: &str, json: String, osd: bool) {
        let line = if job.json { json } else { text.to_string() };
        let args = describe(&job.command);
        let logged = text.replace('\n', ", ");
        match job.reply {
            Some(reply) => {
                log::write(&format!("CLI {args} (forwarded): exit {exit}, {logged}"));
                reply.reply(Reply { exit: exit as u8, stdout: format!("{line}\n") });
            }
            None if self.headless.get() => {
                if self.done.replace(true) {
                    return;
                }
                log::write(&format!("CLI {args}: exit {exit}, {logged}"));
                println!("{line}");
                self.exit_code.set(exit);
                if osd {
                    self.quit_after_osd();
                } else {
                    self.quit();
                }
            }
            None => log::write(&format!("CLI {args}: {logged}")),
        }
    }

    /// Not connected: the verb ends with the text the OSD shows, exit 1 (not found) or 5.
    fn finish_unreachable(&self, job: CliJob) {
        let kind = match self.st.borrow().status {
            MonitorStatus::NotFound => ResultKind::NotFound,
            _ => ResultKind::NotResponding,
        };
        let text = self.not_connected_text();
        let json = failure_json(&job.command, &format!("{kind:?}"), &text);
        let osd = !matches!(job.command, CliCommand::Get(_));
        self.finish(job, tvc_cli::exit_code(Some(kind), true), &text, json, osd);
    }

    /// `--get`. This process's own run prints what its connect just read; a forwarded one re-reads the codes
    /// first, since a resident copy's values can be old.
    fn run_get(self: &Rc<Self>, job: CliJob, target: GetTarget) {
        if job.reply.is_none() || self.st.borrow().status != MonitorStatus::Connected {
            self.answer_get(job, target);
            return;
        }
        self.send(Command::Refresh(get_codes(target)));
        let (tx, rx) = std::sync::mpsc::channel();
        self.send(Command::Flush(tx));
        std::thread::spawn(move || {
            let _ = rx.recv_timeout(HEADLESS_BUDGET);
            let _ = slint::invoke_from_event_loop(move || with(|c| c.answer_get(job, target)));
        });
    }

    fn answer_get(&self, job: CliJob, target: GetTarget) {
        if self.st.borrow().status != MonitorStatus::Connected {
            self.finish_unreachable(job);
            return;
        }
        let st = self.st.borrow();
        let wants = |t: GetTarget| target == GetTarget::All || target == t;
        let mut fields: Vec<(&str, String, String)> = vec![];
        for (t, code, key) in [(GetTarget::Brightness, BRIGHTNESS, "brightness"), (GetTarget::Contrast, CONTRAST, "contrast")] {
            if let Some(v) = st.value(code).filter(|_| wants(t)) {
                fields.push((key, v.to_string(), v.to_string()));
            }
        }
        if let Some(l) = st.value(INPUT).filter(|_| wants(GetTarget::Input)).map(|v| st.input_label(v)) {
            fields.push(("input", l.clone(), json_str(&l)));
        }
        let mode = st.value(COLOUR_MODE).filter(|_| wants(GetTarget::ColourMode));
        if let Some(l) = mode.and_then(|v| st.def(COLOUR_MODE).map(|d| view::label(d, v))) {
            fields.push(("colourMode", l.clone(), json_str(&l)));
        }
        drop(st);
        let text = fields.iter().map(|(k, v, _)| format!("{k}: {v}")).collect::<Vec<_>>().join("\n");
        let json = format!("{{{}}}", fields.iter().map(|(k, _, j)| format!("\"{k}\":{j}")).collect::<Vec<_>>().join(","));
        let kind = if fields.is_empty() { ResultKind::Unsupported } else { ResultKind::Ok };
        if !self.headless.get() {
            OsdHost::show(&self.osd, Content::Message(&text.replace('\n', " · ")));
        }
        self.finish(job, tvc_cli::exit_code(Some(kind), true), &text, json, false);
    }

    fn not_connected_text(&self) -> String {
        view::unreachable_text(&self.st.borrow())
    }

    /// Args forwarded by a second launch; `responder` gets the verb's exit code and output.
    pub fn on_forwarded(self: &Rc<Self>, args: Vec<String>, responder: Responder) {
        let parsed = match tvc_cli::parse(&args) {
            Ok(parsed) => parsed,
            Err(e) => {
                log::write(&format!("CLI (forwarded): {}", e.message));
                let stdout = format!("{}\n\n{}\n", e.message, tvc_cli::help());
                responder.reply(Reply { exit: tvc_cli::EXIT_USAGE as u8, stdout });
                return;
            }
        };
        if parsed.command == CliCommand::Help || parsed.version {
            let text = if parsed.command == CliCommand::Help { tvc_cli::help().to_string() } else { tvc_cli::version_line() };
            responder.reply(Reply { exit: 0, stdout: format!("{text}\n") });
            return;
        }
        // A forward during a headless run makes this process the resident app. A verb with Run in
        // background off stays headless: resident without a window or tray would linger invisibly.
        if self.headless.get() && (parsed.command == CliCommand::Show || self.background_mode()) {
            self.start_resident(false);
        }
        match parsed.command {
            CliCommand::Show => {
                if !parsed.startup || !self.background_mode() {
                    self.show_window();
                }
            }
            _ => self.run_cli(CliJob { reply: Some(responder), ..CliJob::own(&parsed) }),
        }
    }

    pub fn run_cli(self: &Rc<Self>, job: CliJob) {
        let status = self.st.borrow().status;
        if status == MonitorStatus::Connecting {
            self.pending_cli.borrow_mut().push(job);
            return;
        }
        let origin = Origin::Cli;
        let command = job.command;
        let cmd = match command {
            CliCommand::Show | CliCommand::Help => {
                self.show_window();
                return;
            }
            CliCommand::Get(target) => {
                self.run_get(job, target);
                return;
            }
            CliCommand::Preset { night } => {
                let st = self.st.borrow();
                let preset = if night { st.settings.night.clone() } else { st.settings.day.clone() };
                Command::ApplyPreset { preset, origin }
            }
            CliCommand::Input(code) => {
                let value = code.unwrap_or(self.st.borrow().settings.this_machine_input) as u16;
                if !job.yes && status == MonitorStatus::Connected {
                    self.confirm_cli_input(job, value);
                    return;
                }
                platform::wake::nudge();
                Command::Set { code: INPUT, value, confirmed: true, origin }
            }
            CliCommand::Step { code, delta } => Command::Step { code, delta, origin },
        };
        self.dispatch(job, cmd);
    }

    /// Sends a verb's command and waits for its result, or ends it at once when the monitor isn't connected.
    fn dispatch(&self, job: CliJob, cmd: Command) {
        if self.st.borrow().status == MonitorStatus::Connected {
            self.send(cmd);
            self.waiting.borrow_mut().push_back(job);
        } else {
            self.osd_not_connected();
            self.finish_unreachable(job);
        }
    }

    /// `--input` without `--yes`: a resident copy asks on screen; a headless run exits 3.
    fn confirm_cli_input(self: &Rc<Self>, job: CliJob, value: u16) {
        if self.headless.get() && job.reply.is_none() {
            OsdHost::show(&self.osd, Content::Message(NEEDS_YES));
            let json = failure_json(&job.command, "NeedsConfirmation", NEEDS_YES);
            self.finish(job, tvc_cli::EXIT_NEEDS_CONFIRMATION, NEEDS_YES, json, true);
            return;
        }
        self.show_window();
        let pending = AwaitingConfirm(Some(job));
        self.ask(
            self.input_dialog(value),
            Box::new(move |c: &Rc<Controller>| {
                let mut pending = pending;
                if let Some(job) = pending.0.take() {
                    platform::wake::nudge();
                    c.dispatch(job, Command::Set { code: INPUT, value, confirmed: true, origin: Origin::Cli });
                }
            }),
        );
    }

    fn quit(&self) {
        let _ = slint::quit_event_loop();
    }

    // ---------------------------------------------------------------------------------------------------------
    // Settings → tray, hotkeys, autostart (re-applied after every save, as 0.0.4)

    fn apply_settings(self: &Rc<Self>) {
        self.arm_auto();
        let (background, start) = {
            let st = self.st.borrow();
            (st.settings.background_mode, st.settings.start_with_windows)
        };
        if !self.demo {
            if let Err(e) = platform::autostart::apply(start) {
                log::write(&format!("Autostart: {e}"));
            }
        }
        if background {
            self.ensure_tray();
        } else {
            self.tray.borrow_mut().take();
        }
        self.register_hotkeys();
    }

    fn ensure_tray(self: &Rc<Self>) {
        if self.tray.borrow().is_some() {
            return;
        }
        let Ok(tray) = Tray::new() else {
            log::write("Tray: could not create the icon");
            return;
        };
        tray.set_brightness_items(cfg!(target_os = "linux"));
        tray.on_open(|| with(|c| c.show_window()));
        tray.on_preset(|i| with(|c| c.apply_preset(i as usize, Origin::Tray)));
        tray.on_switch_to_me(|| {
            with(|c| {
                let code = c.st.borrow().settings.this_machine_input as u16;
                platform::wake::nudge();
                c.send(Command::Set { code: INPUT, value: code, confirmed: false, origin: Origin::Tray });
            })
        });
        tray.on_step_brightness(|d| with(|c| c.send(Command::Step { code: BRIGHTNESS, delta: d, origin: Origin::Tray })));
        tray.on_quit(|| with(|c| c.quit()));
        *self.tray.borrow_mut() = Some(tray);
        self.update_tray();
    }

    fn update_tray(&self) {
        let tray = self.tray.borrow();
        let Some(tray) = tray.as_ref() else { return };
        let st = self.st.borrow();
        tray.set_connected(st.status == MonitorStatus::Connected);
        tray.set_tip(crate::tray::tooltip(st.status, &view::header_name(&st)).into());
        tray.set_image(crate::tray::icon(tray, st.status, platform::taskbar::is_light()));
    }

    /// Registers the shortcuts while Run in background is on; suspends them otherwise.
    fn register_hotkeys(self: &Rc<Self>) {
        if !self.background_mode() {
            if let Some(h) = self.hotkeys.borrow_mut().as_mut() {
                h.suspend();
            }
            return;
        }
        if self.hotkeys.borrow().is_none() {
            let created = platform::hotkeys::Hotkeys::new(|id| {
                let _ = slint::invoke_from_event_loop(move || with(|c| c.on_hotkey(id)));
            });
            self.st.borrow_mut().hotkeys_available = created.is_some();
            *self.hotkeys.borrow_mut() = created;
        }
        let map: Vec<(usize, platform::hotkeys::Gesture)> = {
            let st = self.st.borrow();
            Action::ALL
                .iter()
                .filter_map(|a| {
                    let g = Gesture::parse(st.settings.hotkeys.get(a)?)?;
                    Some((a.id() as usize, platform::hotkeys::Gesture { mods: g.mods, vk: g.vk }))
                })
                .collect()
        };
        let failed = match self.hotkeys.borrow_mut().as_mut() {
            Some(h) => h.register(&map),
            None => return,
        };
        self.st.borrow_mut().failed_hotkeys =
            Action::ALL.iter().copied().filter(|a| failed.contains(&(a.id() as usize))).collect();
    }

    fn on_hotkey(self: &Rc<Self>, id: usize) {
        let Some(action) = Action::ALL.iter().copied().find(|a| a.id() as usize == id) else { return };
        if action == Action::OpenPanel {
            self.show_window();
            return;
        }
        if self.st.borrow().status != MonitorStatus::Connected {
            self.osd_not_connected();
            return;
        }
        let origin = Origin::Hotkey;
        let this_pc = self.st.borrow().settings.this_machine_input as u16;
        let input = |kind: &str| self.st.borrow().profile.inputs.iter().find(|i| i.kind == kind).map(|i| i.code as u16);
        let cmd = match action {
            Action::BrightnessUp => Command::Step { code: BRIGHTNESS, delta: HOTKEY_STEP, origin },
            Action::BrightnessDown => Command::Step { code: BRIGHTNESS, delta: -HOTKEY_STEP, origin },
            Action::ContrastUp => Command::Step { code: CONTRAST, delta: HOTKEY_STEP, origin },
            Action::ContrastDown => Command::Step { code: CONTRAST, delta: -HOTKEY_STEP, origin },
            Action::DayPreset | Action::NightPreset => {
                self.apply_preset(usize::from(action == Action::NightPreset), origin);
                return;
            }
            Action::InputHdmi | Action::InputDp | Action::InputUsbC => {
                let kind = match action {
                    Action::InputHdmi => "hdmi",
                    Action::InputDp => "dp",
                    _ => "usbc",
                };
                let Some(value) = input(kind) else { return };
                platform::wake::nudge();
                Command::Set { code: INPUT, value, confirmed: false, origin }
            }
            Action::SwitchToMe => {
                platform::wake::nudge();
                Command::Set { code: INPUT, value: this_pc, confirmed: false, origin }
            }
            Action::OpenPanel => unreachable!(),
        };
        self.send(cmd);
    }

    fn apply_preset(&self, index: usize, origin: Origin) {
        let preset = {
            let st = self.st.borrow();
            if index == 0 { st.settings.day.clone() } else { st.settings.night.clone() }
        };
        self.send(Command::ApplyPreset { preset, origin });
    }

    // ---------------------------------------------------------------------------------------------------------
    // Engine events

    /// A power change goes to every engine; a display change rescans and rechecks the tracked monitor.
    fn on_system(&self, power: Option<PowerState>) {
        match power {
            Some(state) => self.send_hub(HubCommand::Power(state)),
            None => {
                self.send_hub(HubCommand::Rescan);
                if self.tracked.borrow().is_some() {
                    self.send(Command::RecheckAfterEvent);
                }
            }
        }
    }

    /// Hub events: the tracked monitor's engine events go to `on_event`; the rest pick or drop the tracked monitor.
    pub fn on_hub(self: &Rc<Self>, event: HubEvent) {
        match event {
            HubEvent::Monitor { id, event } => {
                if self.tracked.borrow().as_ref() == Some(&id) {
                    self.on_event(event);
                }
            }
            HubEvent::Monitors(list) => self.on_monitors(list),
            HubEvent::Detached(id) => {
                if self.tracked.borrow().as_ref() == Some(&id) {
                    log::write(&format!("Monitor {id} detached"));
                    *self.tracked.borrow_mut() = None;
                    self.on_status(MonitorStatus::NotFound, Some(LinkFailure::NotFound));
                }
            }
            HubEvent::Attached(_) | HubEvent::Batch { .. } => {}
        }
    }

    /// Explains what is attached, and tracks the first DDC monitor when none is; none at all is `NotFound`.
    fn on_monitors(self: &Rc<Self>, list: Result<Vec<Monitor>, ListError>) {
        let monitors = list.ok();
        let edid = self.st.borrow().profile.edid_id.clone();
        if self.tracked.borrow().is_none() {
            let ddc: Vec<&Monitor> = monitors.iter().flatten().filter(|m| m.info.backend == BackendKind::Ddc).collect();
            let pick = ddc.iter().find(|m| m.info.identity.edid_id == edid).or(ddc.first());
            if let Some(m) = pick {
                log::write(&format!("Monitor {} attached", m.id));
                *self.tracked.borrow_mut() = Some(m.id.clone());
                let first = self.first_command.borrow_mut().take().unwrap_or(Command::Connect);
                self.send(first);
                if !self.headless.get() {
                    self.send(Command::RequestReconnect);
                }
            } else {
                let st = self.st.borrow();
                let changed = (st.status, st.failure) != (MonitorStatus::NotFound, Some(LinkFailure::NotFound));
                drop(st);
                if changed {
                    self.on_status(MonitorStatus::NotFound, Some(LinkFailure::NotFound));
                }
            }
        }
        let tracked = self.tracked.borrow().clone();
        self.st.borrow_mut().displays = monitors.map(|list| {
            list.iter()
                .map(|m| AttachedDisplay {
                    name: m.info.name.clone(),
                    edid_id: m.info.identity.edid_id.clone(),
                    kind: m.info.kind,
                    tracked: tracked.as_ref() == Some(&m.id),
                })
                .collect()
        });
        self.render();
    }

    pub fn on_event(self: &Rc<Self>, event: Event) {
        match event {
            Event::Status { status, failure } => self.on_status(status, failure),
            Event::Controls(rc) => {
                let mut st = self.st.borrow_mut();
                if st.controls.is_some() {
                    self.caps_live.set(true);
                }
                st.controls = Some(rc);
                st.reading_capabilities = false;
                drop(st);
                self.render();
            }
            Event::Value { code, reading } => {
                let mut st = self.st.borrow_mut();
                st.values.insert(code, reading);
                // A reading that disagrees with what the user just moved pushes the control back.
                st.bump(code);
                drop(st);
                self.render();
            }
            Event::Result { origin, code, preset, result, .. } => self.on_result(origin, code, preset, result),
            Event::Finder(f) => self.on_finder(f),
            Event::Asleep(asleep) => {
                self.st.borrow_mut().asleep = asleep;
                if !asleep {
                    self.auto_tick(); // catch up on a Day/Night change missed while asleep
                }
                self.update_tray();
                self.render();
            }
            Event::SafeMode(on) => {
                self.st.borrow_mut().safe_mode = on;
                self.render();
            }
        }
    }

    fn on_status(self: &Rc<Self>, status: MonitorStatus, failure: Option<LinkFailure>) {
        {
            let mut st = self.st.borrow_mut();
            if st.status != status {
                st.retrying = false;
            }
            if status != MonitorStatus::Connected {
                st.reading_capabilities = false;
            }
            st.status = status;
            st.failure = failure;
            match status {
                MonitorStatus::Connected => {
                    st.shown_status = (Some(status), None);
                    st.values_stale = false; // the connect re-read them
                    self.banner_hold.stop();
                }
                MonitorStatus::NotFound | MonitorStatus::NotResponding => {
                    st.range_note.clear();
                    st.notes.clear();
                    st.preset_note.clear();
                    st.finder = FinderState::default();
                    st.values_stale = true;
                }
                MonitorStatus::Connecting => {}
            }
        }
        if status != MonitorStatus::Connected {
            self.arm_banner_hold();
        }
        if status != MonitorStatus::Connecting {
            let pending = std::mem::take(&mut *self.pending_cli.borrow_mut());
            for job in pending {
                self.run_cli(job);
            }
            // Lost mid-verb (or the engine stopped): end the headless run now rather than at the timeout.
            if self.headless.get() && status != MonitorStatus::Connected && !self.done.get() {
                if let Some(job) = self.own_job() {
                    self.osd_not_connected();
                    self.finish_unreachable(job);
                }
            }
            if status == MonitorStatus::Connected {
                self.auto_tick();
                if !self.headless.get() && !self.caps_live.get() {
                    // Connected on the cached capabilities; refresh them from the monitor once things are quiet.
                    // Asked again on each connect until a live read lands (the engine ignores it after one).
                    slint::Timer::single_shot(Duration::from_secs(2), || with(|c| c.read_capabilities()));
                }
            }
        }
        if self.window.borrow().is_some() {
            self.refresh_setup_hints();
        }
        self.update_tray();
        self.render();
    }

    /// Asks the engine for a live capability read; the window shows progress until its `Controls` event, or until
    /// the engine has handled the command (it sends no `Controls` when it skips the read, the read fails or the
    /// string matches the cached one).
    fn read_capabilities(&self) {
        {
            let mut st = self.st.borrow_mut();
            if st.status != MonitorStatus::Connected {
                return; // lost in the meantime: the next connect asks again
            }
            st.reading_capabilities = true;
        }
        self.send(Command::ReadCapabilities);
        let (tx, rx) = std::sync::mpsc::channel();
        self.send(Command::Flush(tx));
        std::thread::spawn(move || {
            let _ = rx.recv();
            let _ = slint::invoke_from_event_loop(|| {
                with(|c| {
                    c.st.borrow_mut().reading_capabilities = false;
                    c.render();
                })
            });
        });
        self.render();
    }

    fn arm_banner_hold(&self) {
        self.banner_hold.start(slint::TimerMode::SingleShot, BANNER_HOLD, || {
            with(|c| {
                {
                    let mut st = c.st.borrow_mut();
                    st.shown_status = (Some(st.status), st.failure);
                }
                c.render();
            })
        });
    }

    fn on_result(self: &Rc<Self>, origin: Origin, code: Option<u8>, preset: Option<String>, result: WriteResult) {
        let what = match (code, &preset) {
            (Some(c), _) => format!("0x{c:02X}"),
            (None, Some(name)) => format!("preset {name}"),
            (None, None) => "restore".into(),
        };
        log::write(&format!("{} {what}: {:?}, {}", origin_name(origin), result.kind, result.message));
        match origin {
            Origin::Window => self.result_in_window(code, preset, &result),
            // automatic presets stay quiet; only a failure is worth a word, and only in an open window.
            Origin::Schedule => {
                if result.outcome != WriteOutcome::Ok {
                    self.auto_phase.set(None); // try again on the next tick
                    self.show_toast(&format!("Automatic Day/Night: {}", result.message));
                } else {
                    self.remember_phase();
                }
            }
            Origin::Cli => {
                self.osd_result(code, &result);
                let job = self.waiting.borrow_mut().pop_front();
                if let Some(job) = job {
                    let ok = result.outcome == WriteOutcome::Ok;
                    let text = self.result_text(code, &result);
                    let json = result_json(ok, &format!("{:?}", result.kind), &text);
                    self.finish(job, tvc_cli::exit_code(Some(result.kind), true), &text, json, true);
                }
            }
            Origin::Hotkey | Origin::Tray => self.osd_result(code, &result),
        }
    }

    fn result_in_window(self: &Rc<Self>, code: Option<u8>, preset: Option<String>, result: &WriteResult) {
        let ok = result.outcome == WriteOutcome::Ok;
        let mut st = self.st.borrow_mut();
        match code {
            Some(c @ (BRIGHTNESS | CONTRAST)) => {
                match (result.outcome, result.kept) {
                    (WriteOutcome::Ok, _) => st.range_note.clear(),
                    (WriteOutcome::Blocked, Some(kept)) => {
                        let name = if c == BRIGHTNESS { "brightness" } else { "contrast" };
                        let lock = if c == BRIGHTNESS { st.profile.brightness_lock_note.clone() } else { String::new() };
                        st.range_note = format!("Your monitor kept {name} at {kept}{lock}");
                    }
                    (WriteOutcome::NotResponding, _) => {}
                    _ => st.range_note = result.message.clone(),
                }
                if !ok {
                    st.bump(c); // snap the slider back to the monitor's value
                }
            }
            Some(c) => {
                let action = st.def(c).is_some_and(|d| d.kind == ControlKind::Action);
                if ok {
                    st.notes.remove(&c);
                    if action {
                        // A reset changes many values: re-read every non-action card.
                        let codes: Vec<u8> = st
                            .controls
                            .iter()
                            .flat_map(|rc| rc.controls.values())
                            .filter(|d| d.kind != ControlKind::Action && d.section != Section::Hidden)
                            .map(|d| d.code)
                            .collect();
                        self.send(Command::Refresh(codes));
                    }
                } else if result.outcome != WriteOutcome::NotResponding {
                    st.notes.insert(c, result.message.clone());
                    if view::GAINS.contains(&c) || c == INPUT || c == COLOUR_MODE {
                        drop(st);
                        self.show_toast(&result.message);
                        st = self.st.borrow_mut();
                    }
                }
                st.bump(c);
            }
            None if preset.is_some() => {
                if ok {
                    st.preset_note.clear();
                } else if result.outcome != WriteOutcome::NotResponding {
                    st.preset_note = result.message.clone();
                    drop(st);
                    self.show_toast(&result.message);
                    st = self.st.borrow_mut();
                }
            }
            None => st.restore_text = if ok { "Backup restored".into() } else { result.message.clone() },
        }
        drop(st);
        self.render();
    }

    fn osd_result(&self, code: Option<u8>, result: &WriteResult) {
        let st = self.st.borrow();
        if result.outcome == WriteOutcome::NotResponding && st.status != MonitorStatus::Connected {
            drop(st);
            self.osd_not_connected();
            return;
        }
        match (code, result.outcome, result.value) {
            (Some(c @ (BRIGHTNESS | CONTRAST)), WriteOutcome::Ok, Some(v)) => {
                let max = st.values.get(&c).map_or(100, |r| r.max);
                let (glyph, label) = if c == BRIGHTNESS { (glyph::SUN, "Brightness") } else { (glyph::CONTRAST, "Contrast") };
                drop(st);
                OsdHost::show(&self.osd, Content::Level { glyph, label, value: v, max });
            }
            _ => {
                drop(st);
                OsdHost::show(&self.osd, Content::Message(&self.result_text(code, result)));
            }
        }
    }

    /// A result's text, plus the profile's lock note when the monitor kept its brightness.
    fn result_text(&self, code: Option<u8>, result: &WriteResult) -> String {
        let st = self.st.borrow();
        let note = &st.profile.brightness_lock_note;
        if result.kind == ResultKind::Kept && code == Some(BRIGHTNESS) && !note.is_empty() {
            format!("{}{note}", result.message.trim_end_matches('.'))
        } else {
            result.message.clone()
        }
    }

    fn osd_not_connected(&self) {
        let msg = self.not_connected_text();
        OsdHost::show(&self.osd, Content::Message(&msg));
    }

    fn quit_after_osd(&self) {
        // The OSD stays 1.2 s after its last update; exit once it has faded.
        slint::Timer::single_shot(Duration::from_millis(1500), || with(|c| c.quit_if_headless()));
    }

    fn refresh_setup_hints(&self) {
        let mut st = self.st.borrow_mut();
        let failure = if st.status == MonitorStatus::Connected { None } else { st.failure };
        st.setup_hints = platform::setup::hints(failure, st.hotkeys_available);
    }

    // ---------------------------------------------------------------------------------------------------------
    // Window

    pub fn show_window(self: &Rc<Self>) {
        if self.headless.get() {
            self.start_resident(false);
        }
        if self.window.borrow().is_none() {
            let Ok(ui) = MainWindow::new() else {
                log::write("Could not create the main window");
                return;
            };
            ui.set_forced_scheme(self.scheme);
            wire(&ui);
            ui.window().on_close_requested(|| {
                with(|c| c.on_window_closed());
                slint::CloseRequestResponse::HideWindow
            });
            *self.window.borrow_mut() = Some(ui);
        }
        self.refresh_setup_hints();
        self.render();
        // The Linux facts behind the hints are looked up on a worker thread: show them once they're in.
        #[cfg(target_os = "linux")]
        slint::Timer::single_shot(Duration::from_millis(1500), || {
            with(|c| {
                if c.window.borrow().is_some() {
                    c.refresh_setup_hints();
                    c.render();
                }
            })
        });
        if let Some(ui) = self.window.borrow().as_ref() {
            let _ = ui.show();
            ui.window().set_minimized(false);
            ui.window().with_winit_window(|w| w.focus_window());
        }
        platform::efficiency::set_window_visible(true);
        self.send(Command::Refresh(DISPLAY_CODES.to_vec()));
        self.poll.start(slint::TimerMode::Repeated, POLL_EVERY, || {
            with(|c| {
                let minimized = c.window.borrow().as_ref().is_some_and(|w| w.window().is_minimized());
                if !minimized {
                    c.send(Command::Poll(DISPLAY_CODES.to_vec()));
                }
            })
        });
    }

    fn on_window_closed(self: &Rc<Self>) {
        self.poll.stop();
        // Unanswered confirms count as Cancel (a command-line one answers exit 3 as it drops).
        self.confirm.borrow_mut().take();
        self.dialogs.borrow_mut().clear();
        if self.editor.borrow_mut().take().is_some() {
            self.register_hotkeys();
        }
        platform::efficiency::set_window_visible(false);
        if !self.background_mode() {
            self.quit();
            return;
        }
        // Detach it now so nothing renders into the hidden window; drop the component on the next loop turn,
        // not inside its own callback.
        let window = self.window.borrow_mut().take();
        slint::Timer::single_shot(Duration::ZERO, move || drop(window));
    }

    fn visible_window(&self) -> Option<std::cell::Ref<'_, MainWindow>> {
        std::cell::Ref::filter_map(self.window.borrow(), |w| w.as_ref().filter(|w| w.window().is_visible())).ok()
    }

    /// Pushes state into the window. It exists only from just before its show until it is closed, so this never
    /// touches a hidden window.
    fn render(&self) {
        let window = self.window.borrow();
        let Some(ui) = window.as_ref() else { return };
        let st = self.st.borrow();
        view::render(&ui.global::<App>(), &st);
    }

    fn show_toast(&self, text: &str) {
        let Some(ui) = self.visible_window() else { return };
        ui.global::<App>().set_toast(text.into());
        drop(ui);
        self.toast.start(slint::TimerMode::SingleShot, TOAST_FOR, || {
            with(|c| {
                if let Some(ui) = c.visible_window() {
                    ui.global::<App>().set_toast(SharedString::new());
                }
            })
        });
    }

    /// Shows a confirm; while another is open it waits its turn. Without a visible window it is dropped (Cancel).
    fn ask(&self, dialog: Dialog, on_yes: Pending) {
        let Some(ui) = self.visible_window() else { return };
        let app = ui.global::<App>();
        if app.get_dialog().visible {
            self.dialogs.borrow_mut().push_back((dialog, on_yes));
            return;
        }
        app.set_dialog(Dialog { visible: true, ..dialog });
        *self.confirm.borrow_mut() = Some(on_yes);
    }

    fn on_dialog_result(self: &Rc<Self>, yes: bool) {
        if let Some(ui) = self.visible_window() {
            ui.global::<App>().set_dialog(Dialog::default());
        }
        let pending = self.confirm.borrow_mut().take();
        if let (true, Some(f)) = (yes, pending) {
            f(self);
        } else {
            // Cancelled: snap every control back to the monitor's values.
            let codes: Vec<u8> = self.st.borrow().values.keys().copied().collect();
            let mut st = self.st.borrow_mut();
            for c in codes {
                st.bump(c);
            }
            drop(st);
            self.render();
        }
        let next = self.dialogs.borrow_mut().pop_front();
        if let Some((dialog, on_yes)) = next {
            self.ask(dialog, on_yes);
        }
    }

    // ---------------------------------------------------------------------------------------------------------
    // Window actions

    /// The engine reads the input first and skips a switch that is already done, so a stale value can't block one.
    fn select_input(self: &Rc<Self>, code: u16) {
        let st = self.st.borrow();
        if st.status != MonitorStatus::Connected {
            return;
        }
        let this_pc = code == st.settings.this_machine_input as u16;
        drop(st);
        let set = move |c: &Rc<Controller>| {
            platform::wake::nudge();
            c.send(Command::Set { code: INPUT, value: code, confirmed: false, origin: Origin::Window });
        };
        if this_pc {
            set(self);
            return;
        }
        self.ask(self.input_dialog(code), Box::new(set));
    }

    /// "Switch to HDMI?", from the profile's input definition.
    fn input_dialog(&self, code: u16) -> Dialog {
        let st = self.st.borrow();
        let label = st.input_label(code);
        let def = st.def(INPUT);
        let title = def.and_then(|d| d.confirm_title.clone()).unwrap_or_else(|| "Switch to {label}?".into()).replace("{label}", &label);
        let body = def.and_then(|d| d.confirm_body.clone()).unwrap_or_default();
        let button = def.and_then(|d| d.confirm_button.clone()).unwrap_or_else(|| "Switch".into());
        Dialog { title: title.into(), message: body.into(), primary: button.into(), secondary: "Cancel".into(), visible: true }
    }

    fn range_changed(&self, code: u8, value: u16) {
        self.send(Command::DragTo { code, value });
    }

    /// Slint's slider reports `released` on pointer up and on every arrow/Home/End key release. Finish the change
    /// 300 ms after the last one, so a run of key presses (or a click on the track) is one verified write.
    fn range_released(&self, code: u8, value: u16) {
        let timer = slint::Timer::default();
        // A fired timer stays in the map (not dropped inside its own callback) until the next release replaces it.
        timer.start(slint::TimerMode::SingleShot, KEY_DEBOUNCE, move || {
            with(|c| c.send(Command::EndDrag { code, value, origin: Origin::Window }))
        });
        self.debounce.borrow_mut().insert(code, timer);
    }

    fn colour_mode_selected(&self, index: usize) {
        let st = self.st.borrow();
        let Some((value, _)) = view::colour_modes(&st).get(index).cloned() else { return };
        drop(st);
        self.send(Command::Set { code: COLOUR_MODE, value, confirmed: false, origin: Origin::Window });
    }

    fn control_changed(&self, code: u8, index: usize) {
        let st = self.st.borrow();
        let Some(def) = st.def(code) else { return };
        let opts = view::options(&st, def);
        // A switch reports its value (0/1); a combo box its option index.
        let value = if view::is_toggle(def, &opts) {
            index as u16
        } else {
            match opts.get(index) {
                Some((v, _)) => *v,
                None => return,
            }
        };
        drop(st);
        self.send(Command::Set { code, value, confirmed: false, origin: Origin::Window });
    }

    fn control_action(self: &Rc<Self>, code: u8) {
        let st = self.st.borrow();
        let Some(def) = st.def(code) else { return };
        let send = move |c: &Rc<Controller>| c.send(Command::Set { code, value: 1, confirmed: true, origin: Origin::Window });
        if def.safety != SafetyClass::ConfirmRequired {
            drop(st);
            send(self);
            return;
        }
        let title = def.confirm_title.clone().or_else(|| def.confirm_message.clone()).unwrap_or_else(|| format!("Change {}?", def.name));
        let body = def.confirm_body.clone().unwrap_or_default();
        let button = def.confirm_button.clone().unwrap_or_else(|| "Continue".into());
        drop(st);
        self.ask(
            Dialog { title: title.into(), message: body.into(), primary: button.into(), secondary: "Cancel".into(), visible: true },
            Box::new(send),
        );
    }

    fn save_preset(self: &Rc<Self>, index: usize) {
        let mut st = self.st.borrow_mut();
        if st.values_stale || st.status != MonitorStatus::Connected {
            drop(st);
            self.show_toast("Connect the monitor to save its current settings");
            return;
        }
        let cached = |code| st.values.get(&code).map(|r: &VcpReading| r.current);
        let (b, c, m) = (cached(BRIGHTNESS), cached(CONTRAST), cached(COLOUR_MODE));
        let old = if index == 0 { &st.settings.day } else { &st.settings.night };
        let preset = Preset {
            name: old.name.clone(),
            brightness: b.map_or(old.brightness, u32::from),
            contrast: c.map_or(old.contrast, u32::from),
            colour_mode: m.and_then(|m| u8::try_from(m).ok()).or(old.colour_mode),
        };
        let name = preset.name.clone();
        if index == 0 { st.settings.day = preset } else { st.settings.night = preset }
        drop(st);
        let saved = self.save_settings();
        self.show_toast(&if saved { format!("Saved as {name}") } else { format!("Couldn't save {name}") });
    }

    /// Saves, re-applies and re-renders. False when the file couldn't be written.
    fn save_settings(self: &Rc<Self>) -> bool {
        let result = self.store.save(&self.st.borrow().settings);
        if let Err(e) = &result {
            log::write(&format!("Settings save failed: {e}"));
            self.show_toast(&format!("Couldn't save settings: {e}"));
        }
        self.apply_settings();
        self.render();
        result.is_ok()
    }

    // ---------------------------------------------------------------------------------------------------------
    // Shortcut editor

    fn edit_hotkey(self: &Rc<Self>, index: usize) {
        let Some(action) = Action::ALL.get(index).copied() else { return };
        let original = self.st.borrow().settings.hotkeys.get(&action).cloned().unwrap_or_default();
        if let Some(h) = self.hotkeys.borrow_mut().as_mut() {
            h.suspend();
        }
        *self.editor.borrow_mut() =
            Some(EditorState { action, result: original.clone(), original, held: (false, false, false, false) });
        self.render_editor();
    }

    fn editor_key(self: &Rc<Self>, text: &str, mods: (bool, bool, bool, bool), pressed: bool) {
        {
            let mut ed = self.editor.borrow_mut();
            let Some(ed) = ed.as_mut() else { return };
            ed.held = mods;
            if pressed {
                if let Some(vk) = vk_of(text) {
                    let (ctrl, alt, shift, meta) = mods;
                    let bit = |on: bool, m: u8| if on { m } else { 0 };
                    let bits = bit(alt, Gesture::MOD_ALT)
                        | bit(ctrl, Gesture::MOD_CONTROL)
                        | bit(shift, Gesture::MOD_SHIFT)
                        | bit(meta, Gesture::MOD_WIN);
                    ed.result = match Gesture::try_create(bits, vk) {
                        Some(g) => g.to_string(),
                        None => format!("!{bits}:{vk}"), // invalid: rendered as an error below
                    };
                    ed.held = (false, false, false, false);
                } else if !is_modifier(text) {
                    ed.result = "!key".into();
                }
            }
        }
        self.render_editor();
    }

    fn editor_validation(&self, ed: &EditorState) -> (Severity, String, String, bool) {
        let keys_hint = if cfg!(target_os = "macos") {
            "Control, Option or Command"
        } else if cfg!(target_os = "linux") {
            "Ctrl, Alt or Super"
        } else {
            "Ctrl, Alt or the Windows key"
        };
        if ed.result == "!key" {
            return (
                Severity::Error,
                "This key can't be used".into(),
                "Use a letter, number, arrow, Home, End, Page Up, Page Down, Insert, Delete or function key.".into(),
                false,
            );
        }
        if ed.result.starts_with('!') {
            return (Severity::Error, "This shortcut isn't valid".into(), format!("Start with {keys_hint}."), false);
        }
        let st = self.st.borrow();
        if !ed.result.is_empty() {
            if let Some(other) = Action::ALL.iter().find(|a| {
                **a != ed.action && st.settings.hotkeys.get(a).is_some_and(|g| g.eq_ignore_ascii_case(&ed.result))
            }) {
                return (Severity::Warning, format!("{} already uses this shortcut", other.label()), "Choose a different combination.".into(), false);
            }
        }
        let changed = !ed.result.eq_ignore_ascii_case(&ed.original);
        (Severity::Info, String::new(), String::new(), changed)
    }

    fn render_editor(&self) {
        let Some(ui) = self.visible_window() else { return };
        let ed = self.editor.borrow();
        let app = ui.global::<App>();
        let Some(ed) = ed.as_ref() else {
            app.set_editor(ShortcutEditor::default());
            return;
        };
        let (severity, title, message, can_save) = self.editor_validation(ed);
        let (ctrl, alt, shift, meta) = ed.held;
        let live = ctrl || alt || shift || meta;
        let keys: Vec<String> = if live {
            let mut k = vec![];
            if ctrl { k.push("Ctrl".to_string()) }
            if alt { k.push(view::keycaps("Alt").remove(0)) }
            if shift { k.push("Shift".to_string()) }
            if meta { k.push(view::keycaps("Win").remove(0)) }
            k
        } else if ed.result.starts_with('!') {
            vec![]
        } else {
            view::keycaps(&ed.result)
        };
        let keys_hint = if cfg!(target_os = "macos") { "Control, Option or Command" } else if cfg!(target_os = "linux") { "Ctrl, Alt or Super" } else { "Ctrl, Alt or the Windows key" };
        app.set_editor(ShortcutEditor {
            visible: true,
            action: ed.action.label().into(),
            keys: ModelRc::new(VecModel::from(keys.into_iter().map(SharedString::from).collect::<Vec<_>>())),
            live,
            severity,
            error_title: title.into(),
            error_message: message.into(),
            hint: format!("Shortcuts start with {keys_hint}, and can include Shift.").into(),
            can_save,
            can_clear: !ed.result.is_empty() && !ed.original.is_empty(),
        });
    }

    fn editor_close(self: &Rc<Self>, save: bool) {
        let ed = self.editor.borrow_mut().take();
        if let (true, Some(ed)) = (save, ed) {
            self.st.borrow_mut().settings.hotkeys.insert(ed.action, ed.result);
            self.save_settings(); // re-registers
        } else {
            self.register_hotkeys();
        }
        self.render_editor();
        self.render();
    }

    fn editor_reset(self: &Rc<Self>) {
        let conflict = {
            let mut ed = self.editor.borrow_mut();
            let Some(ed) = ed.as_mut() else { return };
            let default = ed.action.default_gesture();
            let st = self.st.borrow();
            let other = Action::ALL
                .iter()
                .copied()
                .find(|a| *a != ed.action && st.settings.hotkeys.get(a).is_some_and(|g| g.eq_ignore_ascii_case(default)));
            if other.is_none() {
                ed.result = default.to_string();
                ed.held = (false, false, false, false);
            }
            other.map(|o| (ed.action, o))
        };
        self.render_editor();
        if let Some((this, other)) = conflict {
            // The conflict dialog: one OK button, the result is left unchanged.
            let dialog = Dialog {
                visible: true,
                title: format!("{} can't use its default shortcut", this.label()).into(),
                message: format!(
                    "{} already uses {}. Change that shortcut first.",
                    other.label(),
                    view::keycaps(this.default_gesture()).join("+")
                )
                .into(),
                primary: SharedString::new(),
                secondary: "OK".into(),
            };
            self.ask(dialog, Box::new(|_| {}));
        }
    }

    // ---------------------------------------------------------------------------------------------------------
    // Finder

    fn on_finder(&self, event: FinderEvent) {
        let mut st = self.st.borrow_mut();
        let f = &mut st.finder;
        f.busy = false;
        match event {
            FinderEvent::Snapshot { count } => {
                f.snapshot_count = Some(count);
                f.status = None;
            }
            FinderEvent::NoSnapshot => f.status = Some((Severity::Warning, "Take a snapshot first.".into())),
            FinderEvent::Refused(kind) => {
                let text = match kind {
                    ResultKind::SafeMode => "Safe mode is on, so the finder doesn't read the monitor. Leave safe mode first.",
                    ResultKind::Asleep => "The monitor is asleep or this PC is locked. Try again once it's awake.",
                    _ => "The monitor didn't answer. Try again.",
                };
                f.status = Some((Severity::Warning, text.into()));
            }
            FinderEvent::Changes(changes) => {
                f.status = Some(if changes.is_empty() {
                    (Severity::Info, "Nothing changed. Make sure the setting really changed on screen, then compare again.".into())
                } else {
                    (Severity::Info, "Select a change to name it.".into())
                });
                f.changes = changes;
                f.selected = None;
                if f.changes.len() == 1 {
                    drop(st);
                    self.finder_select(0);
                    return;
                }
            }
            FinderEvent::Saved { code, result } => {
                let name = f.name.clone();
                f.status = Some(match result {
                    Ok(()) => (Severity::Success, format!("Saved \"{name}\" (0x{code:02X}). It now appears in Monitor settings.")),
                    Err(e) => (Severity::Error, format!("Couldn't save: {e}")),
                });
            }
        }
        drop(st);
        self.render();
    }

    fn finder_select(&self, index: usize) {
        let mut st = self.st.borrow_mut();
        let Some(change) = st.finder.changes.get(index).cloned() else { return };
        let def = st.def(change.code).cloned();
        let named = def.as_ref().filter(|d| d.safety != SafetyClass::ReadOnlyUntilNamed);
        let listed: Option<Vec<u16>> = st
            .controls
            .as_ref()
            .and_then(|rc| rc.capabilities.vcp.get(&change.code).cloned().flatten())
            .map(|l| l.into_iter().map(u16::from).collect());
        let values: Vec<u16> = listed.unwrap_or_else(|| vec![change.before, change.after]);
        let rows = values
            .into_iter()
            .map(|v| {
                let tag = if v == change.before { " (before)" } else if v == change.after { " (now)" } else { "" };
                let label = named.and_then(|d| u8::try_from(v).ok().and_then(|b| d.value_labels.get(&b).cloned())).unwrap_or_default();
                (v, format!("0x{v:02X}{tag}"), label)
            })
            .collect();
        let f = &mut st.finder;
        f.selected = Some(index);
        f.name = named.map(|d| d.name.clone()).unwrap_or_default();
        f.values = rows;
        f.form += 1;
        drop(st);
        self.render();
    }

    fn finder_save(&self) {
        let mut st = self.st.borrow_mut();
        let f = &mut st.finder;
        let Some(change) = f.selected.and_then(|i| f.changes.get(i)).cloned() else { return };
        let name = f.name.trim().to_string();
        if name.is_empty() {
            f.status = Some((Severity::Warning, "Enter a name for the setting first.".into()));
            drop(st);
            self.render();
            return;
        }
        let labels: BTreeMap<u8, String> = f
            .values
            .iter()
            .filter(|(_, _, l)| !l.trim().is_empty())
            .filter_map(|(v, _, l)| Some((u8::try_from(*v).ok()?, l.trim().to_string())))
            .collect();
        f.name = name.clone();
        f.busy = true;
        drop(st);
        let def = ControlDefinition::new(change.code, name, ControlKind::Enum, Section::Advanced, SafetyClass::Normal, labels);
        self.send(Command::SaveOverlay(def));
        self.render();
    }
}

// -------------------------------------------------------------------------------------------------------------
// Automatic Day/Night

impl Controller {
    /// Runs the 60 s schedule tick while a mode is on; stops it otherwise.
    fn arm_auto(&self) {
        if self.headless.get() {
            return; // a one-shot run never applies Day/Night
        }
        if self.st.borrow().settings.auto_day_night == AutoMode::Off {
            self.auto_timer.stop();
            self.auto_phase.set(None);
        } else if !self.auto_timer.running() {
            self.auto_timer.start(slint::TimerMode::Repeated, AUTO_EVERY, || with(|c| c.auto_tick()));
        }
    }

    /// Applies Day or Night when the scheduled phase differs from the one applied last. The latest event at or
    /// before now wins, so a change missed during sleep is caught up on wake.
    fn auto_tick(&self) {
        let st = self.st.borrow();
        if self.headless.get() || st.status != MonitorStatus::Connected || st.asleep || st.safe_mode {
            return;
        }
        let Some(phase) = schedule::current(&st.settings.auto_day_night, localtime::unix_now(), localtime::utc_offset()) else {
            return;
        };
        if self.auto_phase.get() == Some(phase) {
            return;
        }
        let preset = if phase == Phase::Night { st.settings.night.clone() } else { st.settings.day.clone() };
        drop(st);
        self.auto_phase.set(Some(phase));
        self.send(Command::ApplyPreset { preset, origin: Origin::Schedule });
    }

    /// Saves the phase the schedule just applied, so the next start catches up only when it changed.
    fn remember_phase(&self) {
        let mut st = self.st.borrow_mut();
        let phase = self.auto_phase.get();
        if phase.is_none() || st.settings.last_applied_phase == phase {
            return;
        }
        st.settings.last_applied_phase = phase;
        if let Err(e) = self.store.save(&st.settings) {
            log::write(&format!("Settings save failed: {e}"));
        }
    }

    /// 0 = off, 1 = at set times, 2 = at sunrise and sunset (uses the remembered times or place).
    fn set_auto_mode(self: &Rc<Self>, index: i32) {
        {
            let mut st = self.st.borrow_mut();
            st.auto_note.clear();
            let (day, night) = st.settings.auto_times;
            st.settings.auto_day_night = match index {
                1 => AutoMode::Times { day, night },
                2 => match st.settings.auto_location {
                    Some((lat, lon)) => AutoMode::Sun { lat, lon },
                    None => {
                        st.auto_note = "Enter your latitude and longitude, then Save.".into();
                        AutoMode::Off
                    }
                },
                _ => AutoMode::Off,
            };
        }
        self.auto_phase.set(None);
        self.save_settings();
        self.auto_tick();
    }

    fn save_auto_times(self: &Rc<Self>, day: &str, night: &str) {
        let (Some(day), Some(night)) = (parse_hhmm(day), parse_hhmm(night)) else {
            self.st.borrow_mut().auto_note = "Use 24-hour times like 07:00 and 19:30.".into();
            self.render();
            return;
        };
        if day == night {
            self.st.borrow_mut().auto_note = "Day and Night need different times.".into();
            self.render();
            return;
        }
        {
            let mut st = self.st.borrow_mut();
            st.auto_note.clear();
            st.settings.auto_times = (day, night);
            st.settings.auto_day_night = AutoMode::Times { day, night };
        }
        self.auto_phase.set(None);
        self.save_settings();
        self.auto_tick();
    }

    fn save_auto_location(self: &Rc<Self>, lat: &str, lon: &str) {
        let parse = |t: &str, limit: f64| t.trim().replace(',', ".").parse::<f64>().ok().filter(|v| v.abs() <= limit);
        let (Some(lat), Some(lon)) = (parse(lat, 90.0), parse(lon, 180.0)) else {
            self.st.borrow_mut().auto_note =
                "Latitude is -90 to 90 and longitude -180 to 180, in degrees (north and east are positive).".into();
            self.render();
            return;
        };
        {
            let mut st = self.st.borrow_mut();
            st.auto_note.clear();
            st.settings.auto_location = Some((lat, lon));
            st.settings.auto_day_night = AutoMode::Sun { lat, lon };
        }
        self.auto_phase.set(None);
        self.save_settings();
        self.auto_tick();
    }
}

/// "7:05" / "07:05" → minutes after midnight.
fn parse_hhmm(text: &str) -> Option<u16> {
    let (h, m) = text.trim().split_once(':')?;
    let (h, m): (u16, u16) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// The codes `--get` reads.
pub fn get_codes(target: GetTarget) -> Vec<u8> {
    match target {
        GetTarget::Brightness => vec![BRIGHTNESS],
        GetTarget::Contrast => vec![CONTRAST],
        GetTarget::Input => vec![INPUT],
        GetTarget::ColourMode => vec![COLOUR_MODE],
        GetTarget::All => vec![BRIGHTNESS, CONTRAST, INPUT, COLOUR_MODE],
    }
}

/// A failed verb's line: `text`, or with `--json` the verb's JSON shape (`{"error":…}` for `--get`).
pub fn failure_line(command: &CliCommand, json: bool, kind: &str, text: &str) -> String {
    if json { failure_json(command, kind, text) } else { text.to_string() }
}

fn failure_json(command: &CliCommand, kind: &str, text: &str) -> String {
    match command {
        CliCommand::Get(_) => format!("{{\"error\":{}}}", json_str(text)),
        _ => result_json(false, kind, text),
    }
}

fn result_json(ok: bool, kind: &str, message: &str) -> String {
    format!("{{\"ok\":{ok},\"kind\":{},\"message\":{}}}", json_str(kind), json_str(message))
}

/// A verb as its canonical arguments, for log lines.
fn describe(command: &CliCommand) -> String {
    tvc_cli::format(command).map(|args| args.join(" ")).unwrap_or_default()
}

fn origin_name(origin: Origin) -> &'static str {
    match origin {
        Origin::Window => "Window",
        Origin::Hotkey => "Shortcut",
        Origin::Tray => "Tray",
        Origin::Cli => "CLI",
        Origin::Schedule => "Automatic Day/Night",
    }
}

/// A JSON string literal.
fn json_str(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// -------------------------------------------------------------------------------------------------------------
// Key mapping for the shortcut editor: Slint key text → Win32 virtual key.

fn is_modifier(text: &str) -> bool {
    matches!(text.chars().next(), Some('\u{10}'..='\u{18}'))
}

fn vk_of(text: &str) -> Option<u16> {
    let mut chars = text.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    let named = match c {
        '\u{F700}' => 0x26, // Up
        '\u{F701}' => 0x28, // Down
        '\u{F702}' => 0x25, // Left
        '\u{F703}' => 0x27, // Right
        '\u{F704}'..='\u{F71B}' => 0x70 + (c as u16 - 0xF704), // F1–F24
        '\u{F727}' => 0x2D, // Insert
        '\u{7F}' => 0x2E,   // Delete
        '\u{F729}' => 0x24, // Home
        '\u{F72B}' => 0x23, // End
        '\u{F72C}' => 0x21, // PageUp
        '\u{F72D}' => 0x22, // PageDown
        _ => 0,
    };
    if named != 0 {
        return Some(named);
    }
    // Slint reports the character the key typed ("!" for Shift+1, "&" for 1 on AZERTY): ask the layout which key.
    layout_vk(c).or(match c {
        'a'..='z' => Some(c.to_ascii_uppercase() as u16),
        'A'..='Z' | '0'..='9' => Some(c as u16),
        _ => None,
    })
}

/// The letter or digit key that types `c` on the active keyboard layout (`VkKeyScanExW`).
#[cfg(windows)]
fn layout_vk(c: char) -> Option<u16> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardLayout, VkKeyScanExW};
    let mut buf = [0u16; 2];
    let &mut [unit] = c.encode_utf16(&mut buf) else { return None };
    // SAFETY: plain Win32 calls with no pointers; layout 0 = this thread's.
    let scan = unsafe { VkKeyScanExW(unit, GetKeyboardLayout(0)) };
    let vk = (scan as u16) & 0xFF;
    (scan != -1 && matches!(vk, 0x30..=0x39 | 0x41..=0x5A)).then_some(vk)
}

/// No layout API here: the US layout's shifted digits map back to their digit keys.
#[cfg(not(windows))]
fn layout_vk(c: char) -> Option<u16> {
    ")!@#$%^&*(".find(c).map(|i| 0x30 + i as u16)
}

// -------------------------------------------------------------------------------------------------------------
// Window callbacks

fn wire(ui: &MainWindow) {
    let app = ui.global::<App>();
    app.on_select_input(|code| with(|c| c.select_input(code as u16)));
    app.on_range_changed(|code, v| with(|c| c.range_changed(code as u8, v.round() as u16)));
    app.on_range_released(|code, v| with(|c| c.range_released(code as u8, v.round() as u16)));
    app.on_colour_mode_selected(|i| with(|c| c.colour_mode_selected(i as usize)));
    app.on_control_changed(|code, i| with(|c| c.control_changed(code as u8, i as usize)));
    app.on_control_action(|code| with(|c| c.control_action(code as u8)));
    app.on_apply_preset(|i| with(|c| c.apply_preset(i as usize, Origin::Window)));
    app.on_save_preset(|i| with(|c| c.save_preset(i as usize)));
    app.on_banner_action(|| {
        with(|c| {
            if c.st.borrow().safe_mode {
                c.send(Command::SetSafeMode(false));
                c.send(Command::RecheckAfterEvent); // read the capabilities skipped in safe mode
            } else {
                c.st.borrow_mut().retrying = true;
                c.render();
                c.send(Command::RequestReconnect);
                c.retry_timer.start(slint::TimerMode::SingleShot, RETRY_FOR, || {
                    with(|c| {
                        c.st.borrow_mut().retrying = false;
                        c.render();
                    })
                });
            }
        })
    });
    app.on_set_auto_mode(|i| with(|c| c.set_auto_mode(i)));
    app.on_save_auto_times(|day, night| with(|c| c.save_auto_times(&day, &night)));
    app.on_save_auto_location(|lat, lon| with(|c| c.save_auto_location(&lat, &lon)));
    app.on_restore_backup(|| with(|c| c.send(Command::RestoreBackup { origin: Origin::Window })));
    app.on_dialog_result(|yes| with(|c| c.on_dialog_result(yes)));
    app.on_set_background(|on| {
        with(|c| {
            c.st.borrow_mut().settings.background_mode = on;
            c.save_settings();
        })
    });
    app.on_set_start(|on| {
        with(|c| {
            c.st.borrow_mut().settings.start_with_windows = on;
            c.save_settings();
        })
    });
    app.on_set_this_pc(|i| {
        with(|c| {
            let code = view::inputs(&c.st.borrow()).get(i as usize).map(|(code, _)| *code);
            if let Some(code) = code.and_then(|v| u8::try_from(v).ok()) {
                c.st.borrow_mut().settings.this_machine_input = code;
                c.save_settings();
            }
        })
    });
    app.on_copy_text(|text| {
        with(|c| {
            let ok = crate::clipboard::set(&text);
            c.show_toast(if ok { "Copied" } else { "Couldn't copy" });
        })
    });
    app.on_edit_hotkey(|i| with(|c| c.edit_hotkey(i as usize)));
    app.on_editor_key(|text, ctrl, alt, shift, meta, pressed| with(|c| c.editor_key(&text, (ctrl, alt, shift, meta), pressed)));
    app.on_editor_save(|| with(|c| c.editor_close(true)));
    app.on_editor_cancel(|| with(|c| c.editor_close(false)));
    app.on_editor_clear(|| {
        with(|c| {
            if let Some(ed) = c.editor.borrow_mut().as_mut() {
                ed.result.clear();
            }
            c.editor_close(true);
        })
    });
    app.on_editor_reset(|| with(|c| c.editor_reset()));
    app.on_find_hidden(|| {
        with(|c| {
            c.st.borrow_mut().finder = FinderState::default();
            c.render();
        })
    });
    app.on_finder_snapshot(|| {
        with(|c| {
            c.st.borrow_mut().finder.busy = true;
            c.render();
            c.send(Command::FinderSnapshot);
        })
    });
    app.on_finder_compare(|| {
        with(|c| {
            c.st.borrow_mut().finder.busy = true;
            c.render();
            c.send(Command::FinderCompare);
        })
    });
    app.on_finder_select(|i| with(|c| c.finder_select(i as usize)));
    app.on_finder_name_edited(|t| with(|c| c.st.borrow_mut().finder.name = t.to_string()));
    app.on_finder_value_edited(|i, t| {
        with(|c| {
            if let Some(row) = c.st.borrow_mut().finder.values.get_mut(i as usize) {
                row.2 = t.to_string();
            }
        })
    });
    app.on_finder_save(|| with(|c| c.finder_save()));
}
