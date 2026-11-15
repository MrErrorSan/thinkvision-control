//! The UI-thread side of the app. Owns what the window, tray, hotkeys and OSD show, turns their actions into engine
//! commands, and routes engine events back by origin: window → card notes and toasts; hotkey, tray, CLI → OSD.
//!
//! Background-mode rules (measured on Slint): the main window is dropped on
//! close and rebuilt on open; values are pushed into it only while it is shown; the OSD window is kept.
//!
//! **Several displays.** [`AppState`] holds the app-wide settings plus one [`State`] per controllable display, keyed
//! by its hub id. A command for several displays goes out as one `Target::One` send per display (raw values differ
//! per model) under a shared op id, and their results are handled together once all are in: one OSD, one
//! command-line answer. A display that leaves stays in the picker as disconnected for the rest of the session.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use slint::winit_030::WinitWindowAccessor;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use tvc_core::cli::{self as tvc_cli, CliCommand, CliError, GetTarget, InputChoice, Parsed};
use tvc_core::engine::{
    Command, Event, FinderEvent, MonitorStatus, Origin, PowerState, ResolvedControls, WriteOutcome, WriteResult,
};
use tvc_core::hub::{HubCommand, HubEvent, HubHandle, Monitor, OpId, Target};
use tvc_core::identity::{BackendKind, DisplayInfo, ListError, MonitorId};
use tvc_core::result::ResultKind;
use tvc_core::schedule::{self, AutoMode, Phase};
use tvc_core::finder::CodeChange;
use tvc_core::hotkey::{Action, Gesture};
use tvc_core::link::{DisplayKind, LinkFailure, VcpReading};
use tvc_core::profile::{ControlDefinition, ControlKind, Profile, SafetyClass, Section};
use tvc_core::store::{CapabilitiesCache, HotkeyTarget, MonitorSettings, Settings, SettingsStore};
use tvc_core::support::{self, SupportLevel};
use tvc_core::crash::LeftOver;
use tvc_core::log;

use crate::osd::{Content, OsdHost};
use crate::localtime;
use crate::platform;
use crate::platform::instance::{Reply, Responder};
use crate::platform::monitor_events::SystemEvent;
use crate::view::{self, glyph, AttachedDisplay, TogetherSlider, BRIGHTNESS, COLOUR_MODE, CONTRAST, DISPLAY_CODES, INPUT, VOLUME};
use crate::{App, Dialog, DisplayEntry, MainWindow, Severity, ShortcutEditor, Tray};

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
/// After a setup fix ran: long enough for the facts lookup and a reconnect.
const SETUP_CHECK_AFTER: Duration = Duration::from_secs(4);
const TIMED_OUT: &str = "Timed out: the monitor didn't answer in time";
const NEEDS_YES: &str = "Switching the input needs confirmation: run it again with --yes";
const NEEDS_SCREEN: &str =
    "A display without a verified profile switches input only when confirmed on screen: start ThinkVision Control, then run it again";
const NOT_CONFIRMED: &str = "The input switch wasn't confirmed on screen";
/// The master sliders of Together mode.
const TOGETHER_CODES: [(u8, &str, &str); 3] =
    [(BRIGHTNESS, "Brightness", glyph::SUN), (CONTRAST, "Contrast", glyph::CONTRAST), (VOLUME, "Volume", glyph::SPEAKER)];

/// A screen's work area: x, y, width, height in physical pixels.
type Rect = (i32, i32, i32, i32);

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

/// A setup card's fix: running, checking again after it ran, or its result line.
#[derive(Clone, Default)]
pub struct SetupRun {
    pub running: bool,
    pub checking: bool,
    pub result: String,
    pub error: bool,
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

/// One display's state, as the window renders it. `settings`, `setup_hints`, `setup_runs`, `hotkeys_available`,
/// `failed_hotkeys`, `auto_note` and `displays` are app-wide: [`AppState::view_of`] copies them in (with this
/// display's presets and input) right before a render.
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
    /// By hint id.
    pub setup_runs: BTreeMap<&'static str, SetupRun>,
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
    /// The display this state belongs to, None for the state shown while no display is attached.
    pub monitor: Option<Monitor>,
    /// How far the display is trusted; the window shows "Experimental" below Verified.
    pub level: SupportLevel,
    /// Set for a render: the window shows All together.
    together: bool,
    /// Listed at the last rescan; false once it left.
    attached: bool,
    /// The Day/Night phase this display last applied; a preset is applied only when the schedule's differs.
    auto_phase: Option<Phase>,
    /// A `Controls` event after the first one (from the cache or the profile) came: the capabilities are live.
    caps_live: bool,
    /// The work area of the screen it shows on (`tvc_ddc::screen_rect`), refreshed at every rescan.
    screen: Option<Rect>,
}

impl State {
    fn new(profile: Arc<Profile>) -> Self {
        Self {
            settings: Settings::default(),
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
            setup_runs: BTreeMap::new(),
            hotkeys_available: true,
            failed_hotkeys: vec![],
            finder: FinderState::default(),
            asleep: false,
            safe_mode: false,
            auto_note: String::new(),
            retrying: false,
            preset_note: String::new(),
            reading_capabilities: false,
            monitor: None,
            level: SupportLevel::Verified { profile: "p32p20".into() },
            together: false,
            attached: false,
            auto_phase: None,
            caps_live: false,
            screen: None,
        }
    }

    /// The window shows All together instead of one display.
    pub fn selected_together(&self) -> bool {
        self.together
    }

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

    fn connected(&self) -> bool {
        self.attached && self.status == MonitorStatus::Connected
    }

    /// Below Verified: every input switch is confirmed on screen, whatever asked for it.
    fn experimental(&self) -> bool {
        !matches!(self.level, SupportLevel::Verified { .. })
    }

    /// The input `choice` means on this display; None when it has no such input.
    fn input_for(&self, choice: &InputChoice, this_pc: u8) -> Option<u16> {
        if self.controls.is_some() && self.def(INPUT).is_none() {
            return None;
        }
        let shown = view::inputs(self);
        match choice {
            InputChoice::This => Some(u16::from(this_pc)),
            InputChoice::Slot(n) => shown.get(usize::from(*n).wrapping_sub(1)).map(|(c, _)| *c),
            InputChoice::Kind(kind) => self.profile.inputs.iter()
                .map(|i| (u16::from(i.code), &i.kind))
                .find(|(code, k)| *k == kind && shown.iter().any(|(c, _)| c == code))
                .map(|(code, _)| code),
        }
    }
}

#[cfg(test)]
impl State {
    /// A fresh state for unit tests (`view` tests can't reach `State::new`), belonging to `monitor`.
    pub(crate) fn for_test(profile: Arc<Profile>, monitor: Option<Monitor>) -> Self {
        Self { monitor, ..Self::new(profile) }
    }
}

/// The app-wide state and one [`State`] per controllable display.
struct AppState {
    settings: Settings,
    setup_hints: Vec<platform::setup::Hint>,
    /// Kept while its card shows (or its fix runs), so it survives refreshes.
    setup_runs: BTreeMap<&'static str, SetupRun>,
    hotkeys_available: bool,
    failed_hotkeys: Vec<Action>,
    auto_note: String,
    /// Every display at the last rescan, uncontrollable ones too, in enumeration order; None = can't list them.
    monitors: Option<Vec<Monitor>>,
    displays: BTreeMap<MonitorId, State>,
    /// The display the window shows, and the one Together mode renders the other tabs for; None while none is
    /// attached.
    selected: Option<MonitorId>,
    together: bool,
    /// Shown while no display is attached: `Connecting` until the first rescan, then `NotFound`.
    blank: State,
    /// The first rescan's list came.
    listed: bool,
}

impl AppState {
    /// The display's state (`blank` for None) with the app-wide fields copied in, ready for `view`.
    fn view_of(&mut self, id: Option<&MonitorId>) -> &State {
        let AppState { settings, setup_hints, setup_runs, hotkeys_available, failed_hotkeys, auto_note, monitors,
            displays, together, blank, .. } = self;
        let (st, id) = match id.and_then(|id| displays.get_mut(id).map(|st| (st, id))) {
            Some((st, id)) => (st, Some(id)),
            None => (blank, None),
        };
        st.settings.clone_from(settings);
        if let Some(id) = id {
            let own = settings.monitor(id);
            st.settings.day = own.day;
            st.settings.night = own.night;
            st.settings.this_machine_input = own.this_machine_input;
        }
        st.setup_hints.clone_from(setup_hints);
        st.setup_runs.clone_from(setup_runs);
        st.hotkeys_available = *hotkeys_available;
        st.failed_hotkeys.clone_from(failed_hotkeys);
        st.auto_note.clone_from(auto_note);
        st.displays = monitors.as_ref().map(|list| {
            list.iter()
                .map(|m| AttachedDisplay {
                    name: m.info.name.clone(),
                    edid_id: m.info.identity.edid_id.clone(),
                    kind: m.info.kind,
                    tracked: Some(&m.id) == id,
                })
                .collect()
        });
        st.together = *together;
        st
    }

    /// Every display's id, in enumeration order, the ones that left last.
    fn ordered(&self) -> Vec<MonitorId> {
        let mut ids: Vec<MonitorId> =
            self.monitors.iter().flatten().filter(|m| self.displays.contains_key(&m.id)).map(|m| m.id.clone()).collect();
        ids.extend(self.displays.keys().filter(|id| !ids.contains(id)).cloned().collect::<Vec<_>>());
        ids
    }

    fn attached(&self) -> Vec<MonitorId> {
        self.ordered().into_iter().filter(|id| self.displays.get(id).is_some_and(|st| st.attached)).collect()
    }

    fn connected(&self) -> Vec<MonitorId> {
        self.ordered().into_iter().filter(|id| self.displays.get(id).is_some_and(State::connected)).collect()
    }

    /// The attached displays that take part in Together mode.
    fn included(&self) -> Vec<MonitorId> {
        self.attached().into_iter().filter(|id| self.settings.monitor(id).included).collect()
    }

    /// The displays the window's actions go to: the included ones in Together mode, else the selected one.
    fn shown(&self) -> Vec<MonitorId> {
        if self.together { self.included() } else { self.selected.iter().cloned().collect() }
    }

    fn together_available(&self) -> bool {
        self.attached().len() >= 2
    }

    fn state(&self, id: Option<&MonitorId>) -> &State {
        id.and_then(|id| self.displays.get(id)).unwrap_or(&self.blank)
    }

    /// The name the user gave it, else the display's own, else its model, else what it is.
    fn name(&self, id: &MonitorId) -> String {
        let Some(st) = self.displays.get(id) else { return id.0.clone() };
        let info = st.monitor.as_ref().map(|m| &m.info);
        let own = self.settings.monitors.get(id).and_then(|m| m.name.clone());
        own.into_iter()
            .chain(info.map(|i| i.name.clone()))
            .chain([view::monitor_name(st)])
            .map(|n| n.trim().to_string())
            .find(|n| !n.is_empty())
            .unwrap_or_else(|| match info {
                Some(i) if i.kind == DisplayKind::BuiltIn => "Built-in display".into(),
                Some(i) if !i.identity.edid_id.is_empty() => i.identity.edid_id.clone(),
                _ => id.0.clone(),
            })
    }

    /// A name for the OSD, only when several displays could be meant.
    fn osd_name(&self, id: &MonitorId) -> String {
        if self.attached().len() >= 2 { self.name(id) } else { String::new() }
    }

    fn entry(&self, id: &MonitorId) -> Option<DisplayEntry> {
        let st = self.displays.get(id)?;
        let own = self.settings.monitor(id);
        let info = st.monitor.as_ref().map(|m| &m.info);
        let detail = info.map(|i| {
            [i.identity.edid_id.as_str(), i.identity.connector.as_deref().unwrap_or("")]
                .into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ")
        });
        let state = match st.status {
            _ if !st.attached => "disconnected",
            MonitorStatus::Connected if st.level == SupportLevel::ReadOnly => "read-only",
            MonitorStatus::Connected => "connected",
            MonitorStatus::Connecting => "connecting",
            MonitorStatus::NotFound => "unreachable",
            MonitorStatus::NotResponding => "not-responding",
        };
        let level = match st.level {
            SupportLevel::Verified { .. } => "verified",
            SupportLevel::ReadOnly => "read-only",
            SupportLevel::LenovoFamily | SupportLevel::GenericMccs => "experimental",
        };
        Some(DisplayEntry {
            id: id.0.as_str().into(),
            name: self.name(id).into(),
            detail: detail.unwrap_or_default().into(),
            state: state.into(),
            kind: if info.is_some_and(|i| i.kind == DisplayKind::BuiltIn) { "built-in" } else { "external" }.into(),
            level: level.into(),
            included: own.included,
            offset: own.offset,
            has_input: st.def(INPUT).is_some_and(ControlDefinition::is_visible),
            has_colour: st.def(COLOUR_MODE).is_some(),
            has_volume: st.def(VOLUME).is_some(),
        })
    }

    /// Together mode's master sliders: each at the first included display's value less its offset.
    fn together_sliders(&self) -> Vec<TogetherSlider> {
        let included: Vec<(MonitorId, &State)> =
            self.included().into_iter().filter_map(|id| self.displays.get(&id).map(|st| (id, st))).collect();
        let live: Vec<&(MonitorId, &State)> = included.iter().filter(|(_, st)| st.connected()).collect();
        TOGETHER_CODES
            .iter()
            .map(|&(code, label, glyph)| {
                let with: Vec<_> = live.iter().filter(|(_, st)| st.values.contains_key(&code)).collect();
                let value = with.first().and_then(|(id, st)| {
                    let r = st.values.get(&code)?;
                    Some(master_percent(r.current, r.max, self.settings.monitor(id).offset))
                });
                let lacking: Vec<String> = live.iter()
                    .filter(|(_, st)| st.controls.is_some() && st.def(code).is_none())
                    .map(|(id, _)| self.name(id))
                    .collect();
                let reason = if included.is_empty() {
                    "No display is included".to_string()
                } else if live.is_empty() {
                    "No included display is connected".into()
                } else if with.is_empty() {
                    "No included display has it".into()
                } else if !lacking.is_empty() {
                    format!("{} doesn't have it", lacking.join(", "))
                } else {
                    String::new()
                };
                TogetherSlider {
                    code,
                    label: label.into(),
                    glyph: glyph.into(),
                    value: value.unwrap_or(0.0),
                    available: !with.is_empty(),
                    reason,
                }
            })
            .collect()
    }
}

/// The display's own settings entry, created from the inherited values when it has none.
fn monitor_entry<'a>(settings: &'a mut Settings, id: &MonitorId) -> &'a mut MonitorSettings {
    let inherited = settings.monitor(id);
    settings.monitors.entry(id.clone()).or_insert(inherited)
}

/// What the engine resolved, as far as the app can tell without asking it: the built-in panel is forced Verified;
/// other displays follow `support::resolve` on their capabilities (none known reads as a failed read).
fn level_for(info: &DisplayInfo, controls: Option<&ResolvedControls>) -> SupportLevel {
    if info.backend == BackendKind::Backlight {
        return SupportLevel::Verified { profile: "builtin-backlight".into() };
    }
    match controls {
        None => support::resolve(&info.identity, None, false),
        Some(rc) => {
            let caps = Some(&rc.capabilities).filter(|c| !c.vcp.is_empty());
            support::resolve(&info.identity, caps, caps.is_none())
        }
    }
}

/// Together mode: a display's raw value for the master slider at `master` percent. Its offset is added and the
/// sum clipped to 0–100 % (the offset itself is kept), then scaled to the display's own maximum.
pub fn together_value(master: f32, offset: i32, max: u16) -> u16 {
    let percent = (master + offset as f32).clamp(0.0, 100.0);
    (percent * f32::from(max) / 100.0).round() as u16
}

/// The master slider position a display's reading implies: its value in percent less its offset, within 0–100.
pub fn master_percent(value: u16, max: u16, offset: i32) -> f32 {
    if max == 0 {
        return 0.0;
    }
    (f32::from(value) * 100.0 / f32::from(max) - offset as f32).clamp(0.0, 100.0)
}

/// The display whose screen contains `point`; None when none does.
pub fn display_at(point: (i32, i32), screens: &[(MonitorId, Rect)]) -> Option<MonitorId> {
    let (px, py) = point;
    screens.iter().find(|(_, (x, y, w, h))| px >= *x && px < x + w && py >= *y && py < y + h).map(|(id, _)| id.clone())
}

/// The displays a verb acts on, from `(id, name, may support the verb)` in display order. `--display` is resolved
/// by `cli::resolve_display`; without it, or with `all`, every display that may support the verb (every display
/// when none does, so each answers for itself).
pub fn cli_targets(wanted: Option<&str>, displays: &[(String, String, bool)]) -> Result<Vec<String>, CliError> {
    let named: Vec<(String, String)> = displays.iter().map(|(id, name, _)| (id.clone(), name.clone())).collect();
    let wanted = wanted.unwrap_or("all");
    let ids: Vec<String> = tvc_cli::resolve_display(wanted, &named)?.into_iter().map(String::from).collect();
    if !wanted.trim().eq_ignore_ascii_case("all") {
        return Ok(ids);
    }
    let able: Vec<String> = displays.iter().filter(|(_, _, able)| *able).map(|(id, _, _)| id.clone()).collect();
    Ok(if able.is_empty() { ids } else { able })
}

/// Whether a display may support a verb: it does unless its controls are known and lack the verb's code.
fn may_run(st: &State, command: &CliCommand) -> bool {
    let code = match command {
        CliCommand::Step { code, .. } => *code,
        CliCommand::Input(_) => INPUT,
        _ => return true,
    };
    st.controls.is_none() || st.def(code).is_some()
}

type Pending = Box<dyn FnOnce(&Rc<Controller>)>;
/// Write results by display.
type Answers = Vec<(MonitorId, WriteResult)>;

/// A command-line verb being run: this process's own, or one forwarded by a later launch.
pub struct CliJob {
    command: CliCommand,
    json: bool,
    yes: bool,
    /// `--display`; None = the default (every display that supports the verb).
    display: Option<String>,
    /// The forwarding launch's responder; None for this process's own verb.
    reply: Option<Responder>,
}

impl CliJob {
    pub fn own(parsed: &Parsed) -> Self {
        Self { command: parsed.command.clone(), json: parsed.json, yes: parsed.yes, display: parsed.display.clone(), reply: None }
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

/// Commands sent to several displays under one op id, waiting for every result.
struct Batch {
    origin: Origin,
    code: Option<u8>,
    waiting: BTreeSet<MonitorId>,
    results: Answers,
    job: Option<CliJob>,
}

pub struct Controller {
    /// Taken at exit by `take_hub`.
    hub: RefCell<Option<HubHandle>>,
    /// Sent to each display as it is attached (`--get brightness` reads only what it prints); None = `Connect`.
    first_command: RefCell<Option<Command>>,
    store: SettingsStore,
    data_dir: PathBuf,
    app: RefCell<AppState>,
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
    debounce: RefCell<HashMap<(MonitorId, u8), slint::Timer>>,
    scheme: i32,
    /// `--demo`: never touch the real autostart entry.
    demo: bool,
    /// A one-shot CLI run with no resident app: exit after the OSD.
    headless: Cell<bool>,
    /// The headless run's own verb and `--json`, for its timeout line.
    headless_job: RefCell<Option<(CliCommand, bool)>>,
    /// The headless run's own verb has printed its result.
    done: Cell<bool>,
    /// Verbs waiting for the display list, or for a target's connect attempt to finish.
    pending_cli: RefCell<Vec<CliJob>>,
    /// Sends to several displays whose results are handled together.
    batches: RefCell<HashMap<OpId, Batch>>,
    watcher: RefCell<Option<platform::monitor_events::Watcher>>,
    /// Process exit code for a headless run.
    exit_code: Cell<i32>,
    /// Crash markers of dead processes: this run is in safe mode because of them. A resident start clears them.
    left_over: RefCell<Vec<LeftOver>>,
    retry_timer: slint::Timer,
    auto_timer: slint::Timer,
    /// Linux: polls for a tray host before the tray is created (`tray_host_ready`).
    #[cfg(target_os = "linux")]
    tray_wait: slint::Timer,
}

impl Controller {
    pub fn new(hub: HubHandle, profile: Arc<Profile>, data_dir: PathBuf, scheme: i32, demo: bool,
        left_over: Vec<LeftOver>) -> Rc<Self> {
        let store = SettingsStore::new(&data_dir);
        let settings = store.load();
        Rc::new(Self {
            hub: RefCell::new(Some(hub)),
            first_command: RefCell::new(None),
            store,
            data_dir,
            app: RefCell::new(AppState {
                settings,
                setup_hints: vec![],
                setup_runs: BTreeMap::new(),
                hotkeys_available: true,
                failed_hotkeys: vec![],
                auto_note: String::new(),
                monitors: None,
                displays: BTreeMap::new(),
                selected: None,
                together: false,
                blank: State::new(profile),
                listed: false,
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
            headless_job: RefCell::new(None),
            done: Cell::new(false),
            pending_cli: RefCell::new(vec![]),
            batches: RefCell::new(HashMap::new()),
            watcher: RefCell::new(None),
            exit_code: Cell::new(0),
            left_over: RefCell::new(left_over),
            retry_timer: slint::Timer::default(),
            auto_timer: slint::Timer::default(),
            #[cfg(target_os = "linux")]
            tray_wait: slint::Timer::default(),
        })
    }

    pub fn background_mode(&self) -> bool {
        self.app.borrow().settings.background_mode
    }

    /// Sends to one display's engine.
    fn send_to(&self, id: &MonitorId, command: Command) {
        if let Some(hub) = self.hub.borrow().as_ref() {
            hub.send(HubCommand::Send { target: Target::One(id.clone()), op: hub.next_op(), command: Box::new(command) });
        }
    }

    fn send_hub(&self, command: HubCommand) {
        if let Some(hub) = self.hub.borrow().as_ref() {
            hub.send(command);
        }
    }

    /// Sends to every display the window shows.
    fn send_shown(&self, command: &Command) {
        for id in self.app.borrow().shown() {
            self.send_to(&id, command.clone());
        }
    }

    /// Sends one command per display. Results of a command-line verb, or of a hotkey or tray action on several
    /// displays, are handled together once all are in; `done` are targets already answered (not connected, no
    /// such input). Window commands answer one by one.
    fn send_batch(self: &Rc<Self>, origin: Origin, code: Option<u8>, sends: Vec<(MonitorId, Command)>,
        done: Answers, job: Option<CliJob>) {
        if sends.is_empty() {
            self.on_results(origin, code, None, done, job);
            return;
        }
        if job.is_none() && done.is_empty() && (origin == Origin::Window || sends.len() == 1) {
            for (id, command) in sends {
                self.send_to(&id, command);
            }
            return;
        }
        let hub = self.hub.borrow();
        let Some(hub) = hub.as_ref() else { return };
        let op = hub.next_op();
        let waiting = sends.iter().map(|(id, _)| id.clone()).collect();
        self.batches.borrow_mut().insert(op, Batch { origin, code, waiting, results: done, job });
        for (id, command) in sends {
            hub.send(HubCommand::Send { target: Target::One(id), op, command: Box::new(command) });
        }
    }

    /// Records a result for batch `op`, handling the batch once complete. Gives the result back when `op` isn't a
    /// batch.
    fn collect(self: &Rc<Self>, op: OpId, id: &MonitorId, result: WriteResult) -> Option<WriteResult> {
        let complete = {
            let mut batches = self.batches.borrow_mut();
            let Some(batch) = batches.get_mut(&op) else { return Some(result) };
            if batch.waiting.remove(id) {
                batch.results.push((id.clone(), result));
            }
            if !batch.waiting.is_empty() {
                return None;
            }
            batches.remove(&op)
        };
        if let Some(b) = complete {
            self.on_results(b.origin, b.code, None, b.results, b.job);
        }
        None
    }

    /// The command each display gets first, once it is attached.
    pub fn connect_with(&self, command: Command) {
        *self.first_command.borrow_mut() = Some(command);
    }

    pub fn exit_code(&self) -> i32 {
        self.exit_code.get()
    }

    pub fn take_hub(&self) -> Option<HubHandle> {
        self.hub.borrow_mut().take()
    }

    fn connected(&self, id: &MonitorId) -> bool {
        self.app.borrow().displays.get(id).is_some_and(State::connected)
    }

    fn is_experimental(&self, id: &MonitorId) -> bool {
        self.app.borrow().displays.get(id).is_some_and(State::experimental)
    }

    // ---------------------------------------------------------------------------------------------------------
    // Lifecycle

    /// A resident start: tray and hotkeys per settings, connect, then the window unless starting to the tray.
    pub fn start_resident(self: &Rc<Self>, show_window: bool) {
        self.headless.set(false);
        *self.first_command.borrow_mut() = None;
        for stale in self.left_over.take() {
            stale.clear(); // the safe-mode banner now speaks for it
        }
        if self.watcher.borrow().is_none() {
            // First resident start (also a headless run turned resident): retry burst and display/power events.
            let (attached, listed) = {
                let app = self.app.borrow();
                (app.attached(), app.listed)
            };
            for id in &attached {
                self.send_to(id, Command::RequestReconnect);
            }
            if attached.is_empty() && listed {
                self.send_hub(HubCommand::Rescan);
            }
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
        *self.headless_job.borrow_mut() = Some((job.command.clone(), job.json));
        self.pending_cli.borrow_mut().push(job);
        let budget = if cold { HEADLESS_COLD_BUDGET } else { HEADLESS_BUDGET };
        slint::Timer::single_shot(budget, || with(|c| c.headless_timeout()));
    }

    /// A verb to run once the first connect attempt finishes (`--demo` runs resident).
    pub fn queue_cli(&self, job: CliJob) {
        self.pending_cli.borrow_mut().push(job);
    }

    /// The budget ran out before the headless verb's result: say so and exit 8.
    fn headless_timeout(&self) {
        if !self.headless.get() || self.done.get() {
            return;
        }
        if let Some((command, json)) = self.headless_job.borrow().clone() {
            let text = failure_json(&command, "TimedOut", TIMED_OUT);
            let job = CliJob { command, json, yes: false, display: None, reply: None };
            self.finish(job, tvc_cli::exit_code(None, true), TIMED_OUT, text, false);
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

    /// No display to run on: the verb ends with the text the OSD shows, exit 1.
    fn finish_unreachable(&self, job: CliJob) {
        let text = self.not_connected_text();
        let json = failure_json(&job.command, "NotFound", &text);
        let osd = !matches!(job.command, CliCommand::Get(_));
        if osd {
            OsdHost::show(&self.osd, Content::Message(&text));
        }
        self.finish(job, tvc_cli::exit_code(Some(ResultKind::NotFound), true), &text, json, osd);
    }

    /// A target that isn't connected answers with why.
    fn unreachable_result(&self, id: &MonitorId) -> WriteResult {
        let kind = match self.app.borrow().displays.get(id).map(|st| (st.attached, st.status)) {
            Some((true, MonitorStatus::NotResponding)) => ResultKind::NotResponding,
            _ => ResultKind::NotFound,
        };
        let message = self.with_view(Some(id), view::unreachable_text);
        WriteResult { outcome: WriteOutcome::NotResponding, kind, message, value: None, kept: None }
    }

    /// Splits targets into the connected ones and answers for the rest.
    fn split_connected(&self, targets: Vec<MonitorId>) -> (Vec<MonitorId>, Answers) {
        let (live, rest): (Vec<MonitorId>, Vec<MonitorId>) = targets.into_iter().partition(|id| self.connected(id));
        let done = rest.into_iter().map(|id| {
            let r = self.unreachable_result(&id);
            (id, r)
        }).collect();
        (live, done)
    }

    /// `--get`. This process's own run prints what its connect just read; a forwarded one re-reads the codes
    /// first, since a resident copy's values can be old.
    fn run_get(self: &Rc<Self>, job: CliJob, target: GetTarget, targets: Vec<MonitorId>) {
        let live: Vec<MonitorId> = targets.iter().filter(|id| self.connected(id)).cloned().collect();
        if job.reply.is_none() || live.is_empty() {
            self.answer_get(job, target, &targets);
            return;
        }
        let mut flushed = vec![];
        for id in &live {
            self.send_to(id, Command::Refresh(get_codes(target)));
            let (tx, rx) = std::sync::mpsc::channel();
            self.send_to(id, Command::Flush(tx));
            flushed.push(rx);
        }
        std::thread::spawn(move || {
            let deadline = Instant::now() + HEADLESS_BUDGET;
            for rx in flushed {
                let _ = rx.recv_timeout(deadline.saturating_duration_since(Instant::now()));
            }
            let _ = slint::invoke_from_event_loop(move || with(|c| c.answer_get(job, target, &targets)));
        });
    }

    /// Prints the values: the 0.0.8 single object for one target, an array of objects with `id` and `name` for
    /// several.
    fn answer_get(&self, job: CliJob, target: GetTarget, targets: &[MonitorId]) {
        struct Row {
            id: MonitorId,
            name: String,
            exit: i32,
            text: String,
            json: String,
        }
        let rows: Vec<Row> = targets.iter().map(|id| {
            let (name, fields) = {
                let app = self.app.borrow();
                let fields = app.displays.get(id).filter(|st| st.connected()).map(|st| get_fields(st, target));
                (app.name(id), fields)
            };
            let (exit, text, json) = match fields {
                Some(fields) => {
                    let text = fields.iter().map(|(k, v, _)| format!("{k}: {v}")).collect::<Vec<_>>().join("\n");
                    let json = format!("{{{}}}", fields.iter().map(|(k, _, j)| format!("\"{k}\":{j}")).collect::<Vec<_>>().join(","));
                    let kind = if fields.is_empty() { ResultKind::Unsupported } else { ResultKind::Ok };
                    (tvc_cli::exit_code(Some(kind), true), text, json)
                }
                None => {
                    let r = self.unreachable_result(id);
                    (tvc_cli::exit_code(Some(r.kind), true), r.message.clone(), failure_json(&job.command, "", &r.message))
                }
            };
            Row { id: id.clone(), name, exit, text, json }
        }).collect();
        let (exit, text, json) = match rows.as_slice() {
            [row] => (row.exit, row.text.clone(), row.json.clone()),
            _ => (
                tvc_cli::combined_exit_code(&rows.iter().map(|r| r.exit).collect::<Vec<_>>()),
                rows.iter().map(|r| format!("{} ({})\n{}", r.name, r.id, r.text)).collect::<Vec<_>>().join("\n\n"),
                format!("[{}]", rows.iter().map(|r| tagged(&r.id, &r.name, &r.json)).collect::<Vec<_>>().join(",")),
            ),
        };
        if !self.headless.get() && exit == 0 {
            OsdHost::show(&self.osd, Content::Message(&text.replace('\n', " · ")));
        }
        self.finish(job, exit, &text, json, false);
    }

    /// Why the shown display can't be used, for the OSD and the command line.
    fn not_connected_text(&self) -> String {
        let selected = self.app.borrow().selected.clone();
        self.with_view(selected.as_ref(), view::unreachable_text)
    }

    /// Runs `f` on a display's state as the window would render it.
    fn with_view<R>(&self, id: Option<&MonitorId>, f: impl FnOnce(&State) -> R) -> R {
        f(self.app.borrow_mut().view_of(id))
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

    /// The verb's targets; None until the display list is in.
    fn cli_targets(&self, job: &CliJob) -> Option<Result<Vec<MonitorId>, CliError>> {
        let app = self.app.borrow();
        if !app.listed {
            return None;
        }
        let attached = app.attached();
        if attached.is_empty() {
            return Some(Ok(vec![])); // not found (exit 1), whatever `--display` said
        }
        let rows: Vec<(String, String, bool)> = attached.iter()
            .map(|id| (id.0.clone(), app.name(id), app.displays.get(id).is_some_and(|st| may_run(st, &job.command))))
            .collect();
        Some(cli_targets(job.display.as_deref(), &rows).map(|ids| ids.into_iter().map(MonitorId).collect()))
    }

    pub fn run_cli(self: &Rc<Self>, job: CliJob) {
        if matches!(job.command, CliCommand::Show | CliCommand::Help) {
            self.show_window();
            return;
        }
        let targets = match self.cli_targets(&job) {
            None => {
                self.pending_cli.borrow_mut().push(job);
                return;
            }
            Some(Err(e)) => {
                let json = failure_json(&job.command, "Usage", &e.message);
                self.finish(job, tvc_cli::EXIT_USAGE, &e.message, json, false);
                return;
            }
            Some(Ok(targets)) => targets,
        };
        let connecting = {
            let app = self.app.borrow();
            targets.iter().any(|id| app.displays.get(id).is_some_and(|st| st.status == MonitorStatus::Connecting))
        };
        if connecting {
            self.pending_cli.borrow_mut().push(job);
            return;
        }
        if targets.is_empty() {
            self.finish_unreachable(job);
            return;
        }
        let origin = Origin::Cli;
        match job.command.clone() {
            CliCommand::Show | CliCommand::Help => {}
            CliCommand::Get(target) => self.run_get(job, target, targets),
            CliCommand::Preset { night } => {
                let (live, done) = self.split_connected(targets);
                let sends = self.presets(night, live, origin);
                self.send_batch(origin, None, sends, done, Some(job));
            }
            CliCommand::Input(choice) => self.run_input(job, &choice, targets),
            CliCommand::Step { code, delta } => {
                let (live, done) = self.split_connected(targets);
                let sends = live.into_iter().map(|id| (id, Command::Step { code, delta, origin })).collect();
                self.send_batch(origin, Some(code), sends, done, Some(job));
            }
        }
    }

    /// `--input`. One on-screen confirm covers every target; it is needed without `--yes`, and always when a
    /// target has no verified profile. A headless run can't ask, so it exits 3.
    fn run_input(self: &Rc<Self>, job: CliJob, choice: &InputChoice, targets: Vec<MonitorId>) {
        let (live, mut done) = self.split_connected(targets);
        let mut sends = vec![];
        for id in live {
            match self.input_value(&id, choice) {
                Some(value) => sends.push((id, value)),
                None => {
                    let name = self.app.borrow().name(&id);
                    let message = format!("{name} has no such input.");
                    done.push((id, WriteResult {
                        outcome: WriteOutcome::Blocked,
                        kind: ResultKind::Unsupported,
                        message,
                        value: None,
                        kept: None,
                    }));
                }
            }
        }
        let experimental = sends.iter().any(|(id, _)| self.is_experimental(id));
        if sends.is_empty() || (job.yes && !experimental) {
            self.send_inputs(Origin::Cli, sends, done, Some(job));
            return;
        }
        if self.headless.get() && job.reply.is_none() {
            let text = if job.yes { NEEDS_SCREEN } else { NEEDS_YES };
            OsdHost::show(&self.osd, Content::Message(text));
            let json = failure_json(&job.command, "NeedsConfirmation", text);
            self.finish(job, tvc_cli::EXIT_NEEDS_CONFIRMATION, text, json, true);
            return;
        }
        self.show_window();
        let dialog = self.inputs_dialog(&sends);
        let pending = AwaitingConfirm(Some(job));
        self.ask(
            dialog,
            Box::new(move |c: &Rc<Controller>| {
                let mut pending = pending;
                if let Some(job) = pending.0.take() {
                    c.send_inputs(Origin::Cli, sends, done, Some(job));
                }
            }),
        );
    }

    /// The input `choice` means on a display, None when it has none.
    fn input_value(&self, id: &MonitorId, choice: &InputChoice) -> Option<u16> {
        let app = self.app.borrow();
        let this_pc = app.settings.monitor(id).this_machine_input;
        app.displays.get(id)?.input_for(choice, this_pc)
    }

    /// Input switches the app has confirmed as its rules require.
    fn send_inputs(self: &Rc<Self>, origin: Origin, sends: Vec<(MonitorId, u16)>, done: Answers,
        job: Option<CliJob>) {
        if !sends.is_empty() {
            platform::wake::nudge();
        }
        let sends = sends.into_iter()
            .map(|(id, value)| (id, Command::Set { code: INPUT, value, confirmed: true, origin }))
            .collect();
        self.send_batch(origin, Some(INPUT), sends, done, job);
    }

    /// Switches inputs, after one confirm for all of them when `confirm`.
    fn switch_inputs(self: &Rc<Self>, origin: Origin, sends: Vec<(MonitorId, u16)>, confirm: bool) {
        if sends.is_empty() {
            return;
        }
        if !confirm {
            self.send_inputs(origin, sends, vec![], None);
            return;
        }
        if origin != Origin::Window {
            self.show_window();
        }
        let dialog = self.inputs_dialog(&sends);
        self.ask(dialog, Box::new(move |c: &Rc<Controller>| c.send_inputs(origin, sends, vec![], None)));
    }

    /// A hotkey or tray input action, on the displays it targets; a display without a verified profile asks first.
    fn input_action(self: &Rc<Self>, origin: Origin, choice: &InputChoice) {
        let targets = self.pointer_targets();
        if targets.is_empty() {
            self.osd_not_connected();
            return;
        }
        let sends: Vec<(MonitorId, u16)> = targets.into_iter()
            .filter_map(|id| {
                let value = self.input_value(&id, choice)?;
                Some((id, value))
            })
            .collect();
        let confirm = sends.iter().any(|(id, _)| self.is_experimental(id));
        self.switch_inputs(origin, sends, confirm);
    }

    /// Each display's own Day or Night preset.
    fn presets(&self, night: bool, targets: Vec<MonitorId>, origin: Origin) -> Vec<(MonitorId, Command)> {
        let app = self.app.borrow();
        targets.into_iter()
            .map(|id| {
                let own = app.settings.monitor(&id);
                let preset = if night { own.night } else { own.day };
                (id, Command::ApplyPreset { preset, origin })
            })
            .collect()
    }

    fn quit(&self) {
        let _ = slint::quit_event_loop();
    }

    // ---------------------------------------------------------------------------------------------------------
    // Settings → tray, hotkeys, autostart (re-applied after every save, as 0.0.4)

    fn apply_settings(self: &Rc<Self>) {
        self.arm_auto();
        let (background, start) = {
            let app = self.app.borrow();
            (app.settings.background_mode, app.settings.start_with_windows)
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
        #[cfg(target_os = "linux")]
        {
            if !self.tray_host_ready() {
                return;
            }
            // The panel's light or dark scheme picks the glyph; follow its changes.
            platform::taskbar::watch(|| {
                let _ = slint::invoke_from_event_loop(|| with(|c| c.update_tray()));
            });
        }
        let Ok(tray) = Tray::new() else {
            log::write("Tray: could not create the icon");
            return;
        };
        tray.set_brightness_items(cfg!(target_os = "linux"));
        tray.on_open(|| with(|c| c.show_window()));
        tray.on_preset(|i| with(|c| c.apply_preset(i as usize, Origin::Tray)));
        tray.on_switch_to_me(|| with(|c| c.input_action(Origin::Tray, &InputChoice::This)));
        tray.on_step_brightness(|d| {
            with(|c| {
                let targets = c.pointer_targets();
                c.step(Origin::Tray, BRIGHTNESS, d, targets);
            })
        });
        tray.on_quit(|| with(|c| c.quit()));
        *self.tray.borrow_mut() = Some(tray);
        self.update_tray();
    }

    /// Linux: whether to create the tray now. ksni gives up for good when no StatusNotifier host is on the session
    /// bus, so a tray made at login before the panel is up would never appear. Without a host this polls every 2 s
    /// and creates the tray once one appears; after 60 s without one it opens the window, so the app never runs
    /// unseen. True also when the bus can't be asked (the tray is made as before).
    #[cfg(target_os = "linux")]
    fn tray_host_ready(self: &Rc<Self>) -> bool {
        if platform::dbus::tray_host_present() != Some(false) {
            self.tray_wait.stop();
            return true;
        }
        if !self.tray_wait.running() {
            log::write("Tray: no tray host on the session bus yet; waiting for one");
            let mut waited = 0u32;
            self.tray_wait.start(slint::TimerMode::Repeated, Duration::from_secs(2), move || {
                waited += 2;
                with(|c| {
                    if !c.background_mode() || c.tray.borrow().is_some() {
                        c.tray_wait.stop();
                    } else if platform::dbus::tray_host_present() != Some(false) {
                        c.tray_wait.stop();
                        c.ensure_tray();
                    } else if waited == 60 && c.window.borrow().is_none() {
                        log::write("Tray: still no tray host after 60 s; opening the window");
                        c.show_window();
                    }
                })
            });
        }
        false
    }

    /// The tray's tip and badge from the shown display (another one when that isn't connected).
    fn update_tray(&self) {
        let tray = self.tray.borrow();
        let Some(tray) = tray.as_ref() else { return };
        let app = self.app.borrow();
        let selected = app.state(app.selected.as_ref());
        let shown = if selected.connected() {
            selected
        } else {
            app.connected().first().map_or(selected, |id| app.state(Some(id)))
        };
        let name = Some(shown.profile.short_name.trim())
            .filter(|n| !n.is_empty())
            .or_else(|| shown.monitor.as_ref().map(|m| m.info.name.trim()))
            .unwrap_or("")
            .to_string();
        tray.set_connected(!app.connected().is_empty());
        tray.set_tip(crate::tray::tooltip(shown.status, &name).into());
        tray.set_image(crate::tray::icon(tray, shown.status, platform::taskbar::is_light()));
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
            self.app.borrow_mut().hotkeys_available = created.is_some();
            *self.hotkeys.borrow_mut() = created;
        }
        let map: Vec<(usize, platform::hotkeys::Gesture)> = {
            let app = self.app.borrow();
            Action::ALL
                .iter()
                .filter_map(|a| {
                    let g = Gesture::parse(app.settings.hotkeys.get(a)?)?;
                    Some((a.id() as usize, platform::hotkeys::Gesture { mods: g.mods, vk: g.vk }))
                })
                .collect()
        };
        let failed = match self.hotkeys.borrow_mut().as_mut() {
            Some(h) => h.register(&map),
            None => return,
        };
        self.app.borrow_mut().failed_hotkeys =
            Action::ALL.iter().copied().filter(|a| failed.contains(&(a.id() as usize))).collect();
    }

    /// The connected displays a hotkey or tray action goes to: all of them, or the one under the pointer when that
    /// is set and known (else all).
    fn pointer_targets(&self) -> Vec<MonitorId> {
        let app = self.app.borrow();
        let connected = app.connected();
        if app.settings.hotkey_target == HotkeyTarget::UnderPointer {
            let screens: Vec<_> =
                connected.iter().filter_map(|id| Some((id.clone(), app.displays.get(id)?.screen?))).collect();
            if let Some(id) = platform::hotkeys::pointer().and_then(|p| display_at(p, &screens)) {
                return vec![id];
            }
        }
        connected
    }

    fn on_hotkey(self: &Rc<Self>, id: usize) {
        let Some(action) = Action::ALL.iter().copied().find(|a| a.id() as usize == id) else { return };
        if action == Action::OpenPanel {
            self.show_window();
            return;
        }
        if let Some(choice) = action.input_choice() {
            self.input_action(Origin::Hotkey, &choice);
            return;
        }
        if matches!(action, Action::DayPreset | Action::NightPreset) {
            self.apply_preset(usize::from(action == Action::NightPreset), Origin::Hotkey);
            return;
        }
        let (code, delta) = match action {
            Action::BrightnessUp => (BRIGHTNESS, HOTKEY_STEP),
            Action::BrightnessDown => (BRIGHTNESS, -HOTKEY_STEP),
            Action::ContrastUp => (CONTRAST, HOTKEY_STEP),
            Action::ContrastDown => (CONTRAST, -HOTKEY_STEP),
            _ => return,
        };
        let targets = self.pointer_targets();
        self.step(Origin::Hotkey, code, delta, targets);
    }

    fn step(self: &Rc<Self>, origin: Origin, code: u8, delta: i32, targets: Vec<MonitorId>) {
        if targets.is_empty() {
            self.osd_not_connected();
            return;
        }
        let sends = targets.into_iter().map(|id| (id, Command::Step { code, delta, origin })).collect();
        self.send_batch(origin, Some(code), sends, vec![], None);
    }

    /// Day (0) or Night (1): from the window on the shown displays, else on the hotkey targets.
    fn apply_preset(self: &Rc<Self>, index: usize, origin: Origin) {
        let targets = if origin == Origin::Window { self.app.borrow().shown() } else { self.pointer_targets() };
        if targets.is_empty() && origin != Origin::Window {
            self.osd_not_connected();
            return;
        }
        let sends = self.presets(index == 1, targets, origin);
        self.send_batch(origin, None, sends, vec![], None);
    }

    // ---------------------------------------------------------------------------------------------------------
    // Hub and engine events

    /// A power change goes to every engine; a display change rescans and rechecks every display.
    fn on_system(&self, power: Option<PowerState>) {
        match power {
            Some(state) => self.send_hub(HubCommand::Power(state)),
            None => {
                self.send_hub(HubCommand::Rescan);
                for id in self.app.borrow().attached() {
                    self.send_to(&id, Command::RecheckAfterEvent);
                }
            }
        }
    }

    pub fn on_hub(self: &Rc<Self>, event: HubEvent) {
        match event {
            HubEvent::Monitor { id, event } => self.on_event(&id, event),
            HubEvent::Monitors(list) => self.on_monitors(list),
            HubEvent::Attached(m) => self.on_attached(&m),
            HubEvent::Detached(id) => self.on_detached(&id),
            HubEvent::Batch { op, origin, code, results } => {
                let batch = self.batches.borrow_mut().remove(&op);
                match batch {
                    Some(mut b) => {
                        b.results.extend(results);
                        self.on_results(b.origin, b.code, None, b.results, b.job);
                    }
                    None => self.on_results(origin, code, None, results, None),
                }
            }
        }
    }

    /// The list after a rescan: every controllable display gets a state (connected on `Attached`); the shown
    /// display is kept while attached, else the first one, preferring the built-in profile's model.
    fn on_monitors(self: &Rc<Self>, list: Result<Vec<Monitor>, ListError>) {
        let blank_lost = {
            let mut app = self.app.borrow_mut();
            let AppState { settings, displays, blank, .. } = &mut *app;
            if let Ok(list) = &list {
                for m in list.iter().filter(|m| m.info.backend != BackendKind::None) {
                    let st = displays.entry(m.id.clone()).or_insert_with(|| {
                        let mut st = State::new(blank.profile.clone());
                        st.auto_phase = settings.monitor(&m.id).last_applied_phase;
                        st.level = level_for(&m.info, None);
                        st
                    });
                    if !st.attached {
                        st.attached = true;
                        st.status = MonitorStatus::Connecting;
                        st.failure = None;
                    }
                    st.screen = tvc_ddc::screen_rect(&m.info.target);
                    st.monitor = Some(m.clone());
                }
                for (id, st) in displays.iter_mut() {
                    if st.attached && !list.iter().any(|m| &m.id == id) {
                        st.attached = false;
                    }
                }
            }
            app.monitors = list.ok();
            app.listed = true;
            let attached = app.attached();
            if app.selected.as_ref().is_none_or(|s| !attached.contains(s)) {
                let edid = app.blank.profile.edid_id.clone();
                let preferred = attached.iter().find(|id| {
                    app.displays.get(id).and_then(|st| st.monitor.as_ref()).is_some_and(|m| m.info.identity.edid_id == edid)
                });
                app.selected = preferred.or(attached.first()).cloned();
            }
            if !app.together_available() {
                app.together = false;
            }
            let lost = attached.is_empty() && (app.blank.status, app.blank.failure) != (MonitorStatus::NotFound, Some(LinkFailure::NotFound));
            if lost {
                app.blank.status = MonitorStatus::NotFound;
                app.blank.failure = Some(LinkFailure::NotFound);
            }
            lost
        };
        if blank_lost {
            self.arm_banner_hold();
        }
        if self.window.borrow().is_some() {
            self.refresh_setup_hints();
        }
        self.run_pending();
        self.update_tray();
        self.render();
    }

    /// A display was attached: its first connect, the 0.0.8 settings it inherits, a retry burst when resident.
    fn on_attached(self: &Rc<Self>, m: &Monitor) {
        if m.info.backend == BackendKind::None {
            return;
        }
        log::write(&format!("Monitor {} attached", m.id));
        let adopted = {
            let mut app = self.app.borrow_mut();
            let adopted = app.settings.adopt_legacy(&m.id, &m.info.identity);
            if adopted {
                if let Err(e) = self.store.save(&app.settings) {
                    log::write(&format!("Settings save failed: {e}"));
                }
                // The inherited 0.0.8 phase now belongs to this display.
                let phase = app.settings.monitor(&m.id).last_applied_phase;
                if let Some(st) = app.displays.get_mut(&m.id) {
                    st.auto_phase = phase;
                }
            }
            adopted
        };
        if adopted {
            log::write(&format!("Settings: {} takes the 0.0.8 input and colour modes", m.id));
        }
        let first = self.first_command.borrow().clone().unwrap_or(Command::Connect);
        self.send_to(&m.id, first);
        if !self.headless.get() {
            self.send_to(&m.id, Command::RequestReconnect);
        }
    }

    /// A display left: it stays listed as disconnected, and batches stop waiting for it.
    fn on_detached(self: &Rc<Self>, id: &MonitorId) {
        log::write(&format!("Monitor {id} detached"));
        match self.app.borrow_mut().displays.get_mut(id) {
            Some(st) => st.attached = false,
            None => return,
        }
        self.on_status(id, MonitorStatus::NotFound, Some(LinkFailure::NotFound));
        let gone = self.unreachable_result(id);
        let ops: Vec<OpId> =
            self.batches.borrow().iter().filter(|(_, b)| b.waiting.contains(id)).map(|(op, _)| *op).collect();
        for op in ops {
            self.collect(op, id, gone.clone());
        }
    }

    fn on_event(self: &Rc<Self>, id: &MonitorId, event: Event) {
        if !self.app.borrow().displays.contains_key(id) {
            return;
        }
        match event {
            Event::Status { status, failure } => self.on_status(id, status, failure),
            Event::Controls(rc) => {
                {
                    let mut app = self.app.borrow_mut();
                    let Some(st) = app.displays.get_mut(id) else { return };
                    if st.controls.is_some() {
                        st.caps_live = true;
                    }
                    if let Some(m) = &st.monitor {
                        st.level = level_for(&m.info, Some(&rc));
                    }
                    st.profile = rc.profile.clone();
                    st.controls = Some(rc);
                    st.reading_capabilities = false;
                }
                self.update_tray();
                self.render();
            }
            Event::Value { code, reading } => {
                if let Some(st) = self.app.borrow_mut().displays.get_mut(id) {
                    st.values.insert(code, reading);
                    // A reading that disagrees with what the user just moved pushes the control back.
                    st.bump(code);
                }
                self.render();
            }
            Event::Result { origin, code, preset, result, op } => {
                let result = match op {
                    Some(op) => self.collect(op, id, result),
                    None => Some(result),
                };
                if let Some(result) = result {
                    self.on_results(origin, code, preset, vec![(id.clone(), result)], None);
                }
            }
            Event::Finder(f) => self.on_finder(id, f),
            Event::Asleep(asleep) => {
                if let Some(st) = self.app.borrow_mut().displays.get_mut(id) {
                    st.asleep = asleep;
                }
                if !asleep {
                    self.auto_tick(); // catch up on a Day/Night change missed while asleep
                }
                self.update_tray();
                self.render();
            }
            Event::SafeMode(on) => {
                if let Some(st) = self.app.borrow_mut().displays.get_mut(id) {
                    st.safe_mode = on;
                }
                self.render();
            }
        }
    }

    fn on_status(self: &Rc<Self>, id: &MonitorId, status: MonitorStatus, failure: Option<LinkFailure>) {
        let read_caps = {
            let mut app = self.app.borrow_mut();
            let Some(st) = app.displays.get_mut(id) else { return };
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
            status == MonitorStatus::Connected && !st.caps_live
        };
        if status != MonitorStatus::Connected {
            self.arm_banner_hold();
        }
        if status != MonitorStatus::Connecting {
            self.run_pending();
            if status == MonitorStatus::Connected {
                self.auto_tick();
                if !self.headless.get() && read_caps {
                    // Connected on the cached capabilities; refresh them from the monitor once things are quiet.
                    // Asked again on each connect until a live read lands (the engine ignores it after one).
                    let id = id.clone();
                    slint::Timer::single_shot(Duration::from_secs(2), move || with(|c| c.read_capabilities(&id)));
                }
            }
        }
        if self.window.borrow().is_some() {
            self.refresh_setup_hints();
        }
        self.update_tray();
        self.render();
    }

    /// Runs the verbs that were waiting; each waits again if its targets still aren't ready.
    fn run_pending(self: &Rc<Self>) {
        let pending = std::mem::take(&mut *self.pending_cli.borrow_mut());
        for job in pending {
            self.run_cli(job);
        }
    }

    /// Asks the engine for a live capability read; the window shows progress until its `Controls` event, or until
    /// the engine has handled the command (it sends no `Controls` when it skips the read, the read fails or the
    /// string matches the cached one).
    fn read_capabilities(&self, id: &MonitorId) {
        {
            let mut app = self.app.borrow_mut();
            let Some(st) = app.displays.get_mut(id).filter(|st| st.connected()) else {
                return; // lost in the meantime: the next connect asks again
            };
            st.reading_capabilities = true;
        }
        self.send_to(id, Command::ReadCapabilities);
        let (tx, rx) = std::sync::mpsc::channel();
        self.send_to(id, Command::Flush(tx));
        let id = id.clone();
        std::thread::spawn(move || {
            let _ = rx.recv();
            let _ = slint::invoke_from_event_loop(move || {
                with(|c| {
                    if let Some(st) = c.app.borrow_mut().displays.get_mut(&id) {
                        st.reading_capabilities = false;
                    }
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
                    let mut app = c.app.borrow_mut();
                    let AppState { displays, blank, .. } = &mut *app;
                    for st in displays.values_mut().chain([blank]) {
                        st.shown_status = (Some(st.status), st.failure);
                    }
                }
                c.render();
            })
        });
    }

    /// Results by origin; several come from one batch, in display order.
    fn on_results(self: &Rc<Self>, origin: Origin, code: Option<u8>, preset: Option<String>,
        mut results: Answers, job: Option<CliJob>) {
        let order = self.app.borrow().ordered();
        results.sort_by_key(|(id, _)| order.iter().position(|o| o == id));
        let what = match (code, &preset) {
            (Some(c), _) => format!("0x{c:02X}"),
            (None, Some(name)) => format!("preset {name}"),
            (None, None) if origin == Origin::Window => "restore".into(),
            (None, None) => "preset".into(),
        };
        for (id, r) in &results {
            log::write(&format!("{} {what} [{id}]: {:?}, {}", origin_name(origin), r.kind, r.message));
        }
        match origin {
            Origin::Window => {
                for (id, r) in &results {
                    self.result_in_window(id, code, preset.clone(), r);
                }
            }
            // automatic presets stay quiet; only a failure is worth a word, and only in an open window.
            Origin::Schedule => {
                for (id, r) in &results {
                    if r.outcome != WriteOutcome::Ok {
                        if let Some(st) = self.app.borrow_mut().displays.get_mut(id) {
                            st.auto_phase = None; // try again on the next tick
                        }
                        self.show_toast(&format!("Automatic Day/Night: {}", r.message));
                    } else {
                        self.remember_phase(id);
                    }
                }
            }
            Origin::Cli => {
                self.osd_results(code, &results);
                if let Some(job) = job {
                    self.finish_results(job, code, &results);
                }
            }
            Origin::Hotkey | Origin::Tray => self.osd_results(code, &results),
        }
    }

    /// A write verb's answer: the 0.0.8 line or object for one target; for several a line each, or an array of
    /// objects with `id` and `name`, and the first failure's exit code.
    fn finish_results(&self, job: CliJob, code: Option<u8>, results: &[(MonitorId, WriteResult)]) {
        struct Row {
            id: MonitorId,
            name: String,
            exit: i32,
            text: String,
            json: String,
        }
        let rows: Vec<Row> = results.iter().map(|(id, r)| {
            let text = self.result_text(id, code, r);
            let json = result_json(r.outcome == WriteOutcome::Ok, &format!("{:?}", r.kind), &text);
            Row { id: id.clone(), name: self.app.borrow().name(id), exit: tvc_cli::exit_code(Some(r.kind), true), text, json }
        }).collect();
        let (exit, text, json) = match rows.as_slice() {
            [row] => (row.exit, row.text.clone(), row.json.clone()),
            _ => (
                tvc_cli::combined_exit_code(&rows.iter().map(|r| r.exit).collect::<Vec<_>>()),
                rows.iter().map(|r| format!("{}: {}", r.name, r.text)).collect::<Vec<_>>().join("\n"),
                format!("[{}]", rows.iter().map(|r| tagged(&r.id, &r.name, &r.json)).collect::<Vec<_>>().join(",")),
            ),
        };
        self.finish(job, exit, &text, json, true);
    }

    fn result_in_window(&self, id: &MonitorId, code: Option<u8>, preset: Option<String>, result: &WriteResult) {
        let ok = result.outcome == WriteOutcome::Ok;
        let mut toast = None;
        let mut refresh = None;
        {
            let mut app = self.app.borrow_mut();
            let Some(st) = app.displays.get_mut(id) else { return };
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
                            refresh = Some(codes);
                        }
                    } else if result.outcome != WriteOutcome::NotResponding {
                        st.notes.insert(c, result.message.clone());
                        if view::GAINS.contains(&c) || c == INPUT || c == COLOUR_MODE {
                            toast = Some(result.message.clone());
                        }
                    }
                    st.bump(c);
                }
                None if preset.is_some() => {
                    if ok {
                        st.preset_note.clear();
                    } else if result.outcome != WriteOutcome::NotResponding {
                        st.preset_note = result.message.clone();
                        toast = Some(result.message.clone());
                    }
                }
                None => st.restore_text = if ok { "Backup restored".into() } else { result.message.clone() },
            }
        }
        if let Some(codes) = refresh {
            self.send_to(id, Command::Refresh(codes));
        }
        if let Some(text) = toast {
            self.show_toast(&text);
        }
        self.render();
    }

    /// One OSD for a set of results: a level for a single display (on its screen) or for several that all moved;
    /// otherwise the messages, the failures first.
    fn osd_results(&self, code: Option<u8>, results: &[(MonitorId, WriteResult)]) {
        let [(first, r), ..] = results else { return };
        if results.len() == 1 {
            self.osd_result(first, code, r);
            return;
        }
        let failed: Vec<&(MonitorId, WriteResult)> = results.iter().filter(|(_, r)| r.outcome != WriteOutcome::Ok).collect();
        let level = matches!(code, Some(BRIGHTNESS | CONTRAST)) && failed.is_empty();
        match (level, r.value) {
            (true, Some(v)) => {
                let max = self.app.borrow().displays.get(first).and_then(|st| st.values.get(&code?)).map_or(100, |r| r.max);
                let (glyph, label) = level_glyph(code);
                let display = format!("{} displays", results.len());
                OsdHost::show_on(&self.osd, Content::Level { glyph, label, value: v, max, display: &display }, None);
            }
            _ => {
                let shown: Vec<&(MonitorId, WriteResult)> = if failed.is_empty() { results.iter().collect() } else { failed };
                let text = shown.iter()
                    .map(|(id, r)| format!("{}: {}", self.app.borrow().name(id), self.result_text(id, code, r)))
                    .collect::<Vec<_>>()
                    .join(" · ");
                OsdHost::show(&self.osd, Content::Message(&text));
            }
        }
    }

    fn osd_result(&self, id: &MonitorId, code: Option<u8>, result: &WriteResult) {
        let (connected, max, name, screen) = {
            let app = self.app.borrow();
            let st = app.state(Some(id));
            let max = code.and_then(|c| st.values.get(&c)).map_or(100, |r| r.max);
            (st.connected(), max, app.osd_name(id), st.screen)
        };
        if result.outcome == WriteOutcome::NotResponding && !connected {
            let text = self.with_view(Some(id), view::unreachable_text);
            OsdHost::show_on(&self.osd, Content::Message(&text), screen);
            return;
        }
        match (code, result.outcome, result.value) {
            (Some(BRIGHTNESS | CONTRAST), WriteOutcome::Ok, Some(v)) => {
                let (glyph, label) = level_glyph(code);
                OsdHost::show_on(&self.osd, Content::Level { glyph, label, value: v, max, display: &name }, screen);
            }
            _ => {
                let text = self.result_text(id, code, result);
                let text = if name.is_empty() { text } else { format!("{name}: {text}") };
                OsdHost::show_on(&self.osd, Content::Message(&text), screen);
            }
        }
    }

    /// A result's text, plus the display's lock note when the monitor kept its brightness.
    fn result_text(&self, id: &MonitorId, code: Option<u8>, result: &WriteResult) -> String {
        let app = self.app.borrow();
        let note = &app.state(Some(id)).profile.brightness_lock_note;
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
        let mut app = self.app.borrow_mut();
        let st = app.state(app.selected.as_ref());
        let failure = if st.connected() { None } else { st.failure };
        let backlight = st.monitor.as_ref().is_some_and(|m| m.info.backend == BackendKind::Backlight);
        let hints = platform::setup::hints(failure, backlight, app.hotkeys_available);
        app.setup_runs.retain(|id, run| run.running || run.checking || hints.iter().any(|h| h.id == *id));
        app.setup_hints = hints;
    }

    /// A setup card's button: runs its fix on a worker thread (Linux asks for the password through polkit).
    fn run_setup_fix(self: &Rc<Self>, id: &str) {
        let (id, fix) = {
            let mut app = self.app.borrow_mut();
            let Some((id, fix)) =
                app.setup_hints.iter().find(|h| h.id == id).and_then(|h| Some((h.id, h.fix.clone()?)))
            else {
                return;
            };
            let run = app.setup_runs.entry(id).or_default();
            if run.running || run.checking {
                return;
            }
            *run = SetupRun { running: true, result: "Working…".into(), ..SetupRun::default() };
            (id, fix)
        };
        self.render();
        let still = fix.still;
        let spawned = std::thread::Builder::new().name("tvc-setup-fix".into()).spawn(move || {
            let outcome = platform::privileged::run(&fix);
            let _ = slint::invoke_from_event_loop(move || with(|c| c.setup_fix_done(id, still, &outcome)));
        });
        if let Err(e) = spawned {
            self.setup_fix_done(id, still, &platform::privileged::Outcome::Failed(e.to_string()));
        }
    }

    /// A fix ended. After a successful one: look again (facts, displays, the link) and say what's left.
    fn setup_fix_done(self: &Rc<Self>, id: &'static str, still: &str, outcome: &platform::privileged::Outcome) {
        use platform::privileged::Outcome;
        let run = match outcome.text(still) {
            Some((result, error)) => SetupRun { result, error, ..SetupRun::default() },
            None => SetupRun { checking: true, result: "Checking…".into(), ..SetupRun::default() },
        };
        self.app.borrow_mut().setup_runs.insert(id, run);
        if matches!(outcome, Outcome::Done | Outcome::AfterLogin) {
            platform::setup::recheck();
            self.send_hub(HubCommand::Rescan);
            for display in self.app.borrow().attached() {
                self.send_to(&display, Command::RequestReconnect);
            }
            slint::Timer::single_shot(SETUP_CHECK_AFTER, move || with(|c| c.setup_check_done(id)));
        }
        self.render();
    }

    /// The check after a fix: its card gone means fixed; still there, it says what's left to do.
    fn setup_check_done(self: &Rc<Self>, id: &'static str) {
        self.refresh_setup_hints();
        let fixed = {
            let mut app = self.app.borrow_mut();
            let still = app.setup_hints.iter().find(|h| h.id == id).map(|h| h.fix.as_ref().map_or("", |f| f.still));
            match (still, app.setup_runs.get_mut(id)) {
                (Some(still), Some(run)) if run.checking => {
                    *run = SetupRun { result: still.into(), ..SetupRun::default() };
                    false
                }
                (None, Some(run)) if run.checking => {
                    app.setup_runs.remove(id);
                    true
                }
                _ => false,
            }
        };
        if fixed {
            self.show_toast("Setup done");
        }
        self.render();
    }

    // ---------------------------------------------------------------------------------------------------------
    // Window

    pub fn show_window(self: &Rc<Self>) {
        if self.headless.get() {
            self.start_resident(false);
        }
        #[cfg(target_os = "linux")]
        let created = self.window.borrow().is_none();
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
            #[cfg(target_os = "linux")]
            if !created {
                raise_on_wayland(ui);
            }
        }
        platform::efficiency::set_window_visible(true);
        self.send_shown(&Command::Refresh(DISPLAY_CODES.to_vec()));
        self.poll.start(slint::TimerMode::Repeated, POLL_EVERY, || {
            with(|c| {
                let minimized = c.window.borrow().as_ref().is_some_and(|w| w.window().is_minimized());
                if !minimized {
                    c.send_shown(&Command::Poll(DISPLAY_CODES.to_vec()));
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
        // No tray and no tray host to make one: a hidden window would leave the app running unseen.
        #[cfg(target_os = "linux")]
        if self.tray.borrow().is_none() && platform::dbus::tray_host_present() == Some(false) {
            log::write("Tray: no tray host on this desktop, so closing the window exits");
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

    /// Pushes state into the window: the selected display, the picker, and in Together mode the master sliders.
    /// The window exists only from just before its show until it is closed, so this never touches a hidden one.
    fn render(&self) {
        let window = self.window.borrow();
        let Some(ui) = window.as_ref() else { return };
        let g = ui.global::<App>();
        let mut app = self.app.borrow_mut();
        let selected = app.selected.clone();
        view::render(&g, app.view_of(selected.as_ref()));
        let entries: Vec<DisplayEntry> = app.ordered().iter().filter_map(|id| app.entry(id)).collect();
        let experimental = matches!(app.state(selected.as_ref()).level, SupportLevel::LenovoFamily | SupportLevel::GenericMccs);
        let selected_id = match &selected {
            Some(id) if !app.together => id.0.as_str(),
            _ => "",
        };
        view::render_displays(&g, entries, selected_id, app.together_available(), experimental);
        if app.together {
            view::render_together(&g, &app.together_sliders());
        }
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
            // Cancelled: snap every control back to the monitors' values.
            for st in self.app.borrow_mut().displays.values_mut() {
                let codes: Vec<u8> = st.values.keys().copied().collect();
                for c in codes {
                    st.bump(c);
                }
            }
            self.render();
        }
        let next = self.dialogs.borrow_mut().pop_front();
        if let Some((dialog, on_yes)) = next {
            self.ask(dialog, on_yes);
        }
    }

    // ---------------------------------------------------------------------------------------------------------
    // Window actions

    /// The displays the window shows that have this input button. A switch is confirmed unless it is to this PC on
    /// verified displays. The engine reads the input first and skips a switch that is already done, so a stale
    /// value can't block one.
    fn select_input(self: &Rc<Self>, code: u16) {
        let (sends, confirm) = {
            let app = self.app.borrow();
            let mut confirm = false;
            let mut sends = vec![];
            for id in app.shown() {
                let Some(st) = app.displays.get(&id).filter(|st| st.connected()) else { continue };
                if !view::inputs(st).iter().any(|(c, _)| *c == code) {
                    continue;
                }
                confirm |= st.experimental() || code != u16::from(app.settings.monitor(&id).this_machine_input);
                sends.push((id, code));
            }
            (sends, confirm)
        };
        self.switch_inputs(Origin::Window, sends, confirm);
    }

    /// "Switch to HDMI?", from the display's input definition; for several displays, one confirm listing them.
    fn inputs_dialog(&self, sends: &[(MonitorId, u16)]) -> Dialog {
        let app = self.app.borrow();
        if let [(id, code)] = sends {
            let st = app.state(Some(id));
            let label = st.input_label(*code);
            let def = st.def(INPUT);
            let title = def.and_then(|d| d.confirm_title.clone()).unwrap_or_else(|| "Switch to {label}?".into()).replace("{label}", &label);
            let body = def.and_then(|d| d.confirm_body.clone()).unwrap_or_default();
            let button = def.and_then(|d| d.confirm_button.clone()).unwrap_or_else(|| "Switch".into());
            return Dialog { title: title.into(), message: body.into(), primary: button.into(), secondary: "Cancel".into(), visible: true };
        }
        let lines: Vec<String> = sends.iter()
            .map(|(id, code)| format!("{} → {}", app.name(id), app.state(Some(id)).input_label(*code)))
            .collect();
        Dialog {
            title: format!("Switch the input on {} displays?", sends.len()).into(),
            message: lines.join("\n").into(),
            primary: "Switch".into(),
            secondary: "Cancel".into(),
            visible: true,
        }
    }

    /// A slider's displays and raw values: the selected display's own value, or in Together mode the master
    /// percent turned into each included display's value.
    fn range_targets(&self, code: u8, value: u16) -> Vec<(MonitorId, u16)> {
        let app = self.app.borrow();
        if !app.together {
            return app.selected.iter().map(|id| (id.clone(), value)).collect();
        }
        app.included()
            .into_iter()
            .filter_map(|id| {
                let st = app.displays.get(&id).filter(|st| st.connected())?;
                let max = st.values.get(&code)?.max;
                let raw = together_value(f32::from(value), app.settings.monitor(&id).offset, max);
                Some((id, raw))
            })
            .collect()
    }

    fn range_changed(&self, code: u8, value: u16) {
        for (id, value) in self.range_targets(code, value) {
            self.send_to(&id, Command::DragTo { code, value });
        }
    }

    /// Slint's slider reports `released` on pointer up and on every arrow/Home/End key release. Finish the change
    /// 300 ms after the last one, so a run of key presses (or a click on the track) is one verified write.
    fn range_released(&self, code: u8, value: u16) {
        for (id, value) in self.range_targets(code, value) {
            let timer = slint::Timer::default();
            let target = id.clone();
            // A fired timer stays in the map (not dropped inside its own callback) until the next release replaces it.
            timer.start(slint::TimerMode::SingleShot, KEY_DEBOUNCE, move || {
                with(|c| c.send_to(&target, Command::EndDrag { code, value, origin: Origin::Window }))
            });
            self.debounce.borrow_mut().insert((id, code), timer);
        }
    }

    fn selected_id(&self) -> Option<MonitorId> {
        self.app.borrow().selected.clone()
    }

    fn colour_mode_selected(&self, index: usize) {
        let Some(id) = self.selected_id() else { return };
        let value = {
            let app = self.app.borrow();
            view::colour_modes(app.state(Some(&id))).get(index).map(|(v, _)| *v)
        };
        if let Some(value) = value {
            self.send_to(&id, Command::Set { code: COLOUR_MODE, value, confirmed: false, origin: Origin::Window });
        }
    }

    fn control_changed(&self, code: u8, index: usize) {
        let Some(id) = self.selected_id() else { return };
        let value = {
            let app = self.app.borrow();
            let st = app.state(Some(&id));
            let Some(def) = st.def(code) else { return };
            let opts = view::options(st, def);
            // A switch reports its value (0/1); a combo box its option index.
            if view::is_toggle(def, &opts) {
                index as u16
            } else {
                match opts.get(index) {
                    Some((v, _)) => *v,
                    None => return,
                }
            }
        };
        self.send_to(&id, Command::Set { code, value, confirmed: false, origin: Origin::Window });
    }

    fn control_action(self: &Rc<Self>, code: u8) {
        let Some(id) = self.selected_id() else { return };
        let dialog = {
            let app = self.app.borrow();
            let Some(def) = app.state(Some(&id)).def(code) else { return };
            (def.safety == SafetyClass::ConfirmRequired).then(|| {
                let title = def.confirm_title.clone().or_else(|| def.confirm_message.clone()).unwrap_or_else(|| format!("Change {}?", def.name));
                let body = def.confirm_body.clone().unwrap_or_default();
                let button = def.confirm_button.clone().unwrap_or_else(|| "Continue".into());
                Dialog { title: title.into(), message: body.into(), primary: button.into(), secondary: "Cancel".into(), visible: true }
            })
        };
        let send = move |c: &Rc<Controller>| c.send_to(&id, Command::Set { code, value: 1, confirmed: true, origin: Origin::Window });
        match dialog {
            Some(dialog) => self.ask(dialog, Box::new(send)),
            None => send(self),
        }
    }

    /// Saves the selected display's current values as its own Day or Night preset.
    fn save_preset(self: &Rc<Self>, index: usize) {
        let Some(id) = self.selected_id() else { return };
        let name = {
            let mut app = self.app.borrow_mut();
            let st = app.state(Some(&id));
            if st.values_stale || !st.connected() {
                None
            } else {
                let cached = |code| st.values.get(&code).map(|r: &VcpReading| r.current);
                let (b, c, m) = (cached(BRIGHTNESS), cached(CONTRAST), cached(COLOUR_MODE));
                let entry = monitor_entry(&mut app.settings, &id);
                let old = if index == 0 { &mut entry.day } else { &mut entry.night };
                old.brightness = b.map_or(old.brightness, u32::from);
                old.contrast = c.map_or(old.contrast, u32::from);
                old.colour_mode = m.and_then(|m| u8::try_from(m).ok()).or(old.colour_mode);
                Some(old.name.clone())
            }
        };
        let Some(name) = name else {
            self.show_toast("Connect the monitor to save its current settings");
            return;
        };
        let saved = self.save_settings();
        self.show_toast(&if saved { format!("Saved as {name}") } else { format!("Couldn't save {name}") });
    }

    /// Saves, re-applies and re-renders. False when the file couldn't be written.
    fn save_settings(self: &Rc<Self>) -> bool {
        let result = self.store.save(&self.app.borrow().settings);
        if let Err(e) = &result {
            log::write(&format!("Settings save failed: {e}"));
            self.show_toast(&format!("Couldn't save settings: {e}"));
        }
        self.apply_settings();
        self.update_tray();
        self.render();
        result.is_ok()
    }

    /// A display id shows that display; "" shows All together, when two or more can be controlled.
    fn select_display(self: &Rc<Self>, id: &str) {
        {
            let mut app = self.app.borrow_mut();
            if id.is_empty() {
                if !app.together_available() {
                    return;
                }
                app.together = true;
            } else {
                let id = MonitorId(id.to_string());
                if !app.displays.contains_key(&id) {
                    return;
                }
                app.selected = Some(id);
                app.together = false;
            }
        }
        self.after_selection();
    }

    fn after_selection(self: &Rc<Self>) {
        self.refresh_setup_hints();
        self.send_shown(&Command::Refresh(DISPLAY_CODES.to_vec()));
        self.update_tray();
        self.render();
    }

    fn set_included(self: &Rc<Self>, id: &str, on: bool) {
        monitor_entry(&mut self.app.borrow_mut().settings, &MonitorId(id.to_string())).included = on;
        self.save_settings();
    }

    fn set_offset(self: &Rc<Self>, id: &str, offset: i32) {
        monitor_entry(&mut self.app.borrow_mut().settings, &MonitorId(id.to_string())).offset = offset.clamp(-100, 100);
        self.save_settings();
    }

    /// Copies the selected display's identity, level, capability warnings and raw capability string.
    fn copy_capability_report(&self) {
        let report = {
            let app = self.app.borrow();
            let Some(id) = app.selected.clone() else { return };
            let raw = CapabilitiesCache::new(self.data_dir.join("monitors").join(id.folder_name())).load();
            capability_report(&app.name(&id), &id, app.state(Some(&id)), raw.as_deref())
        };
        let ok = crate::clipboard::set(&report);
        self.show_toast(if ok { "Capability report copied" } else { "Couldn't copy" });
    }

    /// The banner's button: leave safe mode on every display in it, or try the shown display again.
    fn banner_action(&self) {
        let (safe, retry) = {
            let app = self.app.borrow();
            let safe: Vec<MonitorId> =
                app.displays.iter().filter(|(_, st)| st.attached && st.safe_mode).map(|(id, _)| id.clone()).collect();
            (safe, app.selected.clone())
        };
        let shown_safe = self.app.borrow().state(retry.as_ref()).safe_mode;
        if shown_safe {
            for id in safe {
                self.send_to(&id, Command::SetSafeMode(false));
                self.send_to(&id, Command::RecheckAfterEvent); // read the capabilities skipped in safe mode
            }
            return;
        }
        match &retry {
            Some(id) => {
                if let Some(st) = self.app.borrow_mut().displays.get_mut(id) {
                    st.retrying = true;
                }
                self.send_to(id, Command::RequestReconnect);
            }
            None => self.send_hub(HubCommand::Rescan),
        }
        self.render();
        self.retry_timer.start(slint::TimerMode::SingleShot, RETRY_FOR, || {
            with(|c| {
                for st in c.app.borrow_mut().displays.values_mut() {
                    st.retrying = false;
                }
                c.render();
            })
        });
    }

    // ---------------------------------------------------------------------------------------------------------
    // Shortcut editor

    fn edit_hotkey(self: &Rc<Self>, index: usize) {
        let Some(action) = Action::ALL.get(index).copied() else { return };
        let original = self.app.borrow().settings.hotkeys.get(&action).cloned().unwrap_or_default();
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
        let app = self.app.borrow();
        if !ed.result.is_empty() {
            if let Some(other) = Action::ALL.iter().find(|a| {
                **a != ed.action && app.settings.hotkeys.get(a).is_some_and(|g| g.eq_ignore_ascii_case(&ed.result))
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
            self.app.borrow_mut().settings.hotkeys.insert(ed.action, ed.result);
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
            let app = self.app.borrow();
            let other = Action::ALL
                .iter()
                .copied()
                .find(|a| *a != ed.action && app.settings.hotkeys.get(a).is_some_and(|g| g.eq_ignore_ascii_case(default)));
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
    // Finder (the selected display only)

    /// Runs `f` on the selected display's state, then renders.
    fn with_selected(&self, f: impl FnOnce(&mut State)) {
        let Some(id) = self.selected_id() else { return };
        if let Some(st) = self.app.borrow_mut().displays.get_mut(&id) {
            f(st);
        }
        self.render();
    }

    fn finder_command(&self, command: Command) {
        self.with_selected(|st| st.finder.busy = true);
        if let Some(id) = self.selected_id() {
            self.send_to(&id, command);
        }
    }

    fn on_finder(&self, id: &MonitorId, event: FinderEvent) {
        let select_first = {
            let mut app = self.app.borrow_mut();
            let shown = app.selected.as_ref() == Some(id) && matches!(event, FinderEvent::Changes(_));
            let Some(st) = app.displays.get_mut(id) else { return };
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
                }
                FinderEvent::Saved { code, result } => {
                    let name = f.name.clone();
                    f.status = Some(match result {
                        Ok(()) => (Severity::Success, format!("Saved \"{name}\" (0x{code:02X}). It now appears in Monitor settings.")),
                        Err(e) => (Severity::Error, format!("Couldn't save: {e}")),
                    });
                }
            }
            shown && st.finder.changes.len() == 1
        };
        if select_first {
            self.finder_select(0);
            return;
        }
        self.render();
    }

    fn finder_select(&self, index: usize) {
        self.with_selected(|st| {
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
        });
    }

    fn finder_save(&self) {
        let Some(id) = self.selected_id() else { return };
        let def = {
            let mut app = self.app.borrow_mut();
            let Some(st) = app.displays.get_mut(&id) else { return };
            let f = &mut st.finder;
            let Some(change) = f.selected.and_then(|i| f.changes.get(i)).cloned() else { return };
            let name = f.name.trim().to_string();
            if name.is_empty() {
                f.status = Some((Severity::Warning, "Enter a name for the setting first.".into()));
                None
            } else {
                let labels: BTreeMap<u8, String> = f
                    .values
                    .iter()
                    .filter(|(_, _, l)| !l.trim().is_empty())
                    .filter_map(|(v, _, l)| Some((u8::try_from(*v).ok()?, l.trim().to_string())))
                    .collect();
                f.name = name.clone();
                f.busy = true;
                Some(ControlDefinition::new(change.code, name, ControlKind::Enum, Section::Advanced, SafetyClass::Normal, labels))
            }
        };
        if let Some(def) = def {
            self.send_to(&id, Command::SaveOverlay(def));
        }
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
        if self.app.borrow().settings.auto_day_night == AutoMode::Off {
            self.auto_timer.stop();
            self.forget_phases();
        } else if !self.auto_timer.running() {
            self.auto_timer.start(slint::TimerMode::Repeated, AUTO_EVERY, || with(|c| c.auto_tick()));
        }
    }

    /// The next tick applies the scheduled preset on every display, whatever each applied last.
    fn forget_phases(&self) {
        for st in self.app.borrow_mut().displays.values_mut() {
            st.auto_phase = None;
        }
    }

    /// Applies Day or Night on each connected display whose last applied phase differs from the schedule's, with
    /// that display's own preset. The latest event at or before now wins, so a change missed during sleep, or
    /// while a display was unplugged, is caught up on wake or connect.
    fn auto_tick(&self) {
        if self.headless.get() {
            return;
        }
        let sends = {
            let mut app = self.app.borrow_mut();
            let AppState { settings, displays, .. } = &mut *app;
            let Some(phase) = schedule::current(&settings.auto_day_night, localtime::unix_now(), localtime::utc_offset()) else {
                return;
            };
            let mut sends = vec![];
            for (id, st) in displays.iter_mut() {
                if !st.connected() || st.asleep || st.safe_mode || st.auto_phase == Some(phase) {
                    continue;
                }
                st.auto_phase = Some(phase);
                let own = settings.monitor(id);
                sends.push((id.clone(), if phase == Phase::Night { own.night } else { own.day }));
            }
            sends
        };
        for (id, preset) in sends {
            self.send_to(&id, Command::ApplyPreset { preset, origin: Origin::Schedule });
        }
    }

    /// Saves the phase the schedule just applied on a display, so the next start catches up only when it changed.
    fn remember_phase(&self, id: &MonitorId) {
        let mut app = self.app.borrow_mut();
        let Some(phase) = app.displays.get(id).and_then(|st| st.auto_phase) else { return };
        let entry = monitor_entry(&mut app.settings, id);
        if entry.last_applied_phase == Some(phase) {
            return;
        }
        entry.last_applied_phase = Some(phase);
        if let Err(e) = self.store.save(&app.settings) {
            log::write(&format!("Settings save failed: {e}"));
        }
    }

    /// 0 = off, 1 = at set times, 2 = at sunrise and sunset (uses the remembered times or place).
    fn set_auto_mode(self: &Rc<Self>, index: i32) {
        {
            let mut app = self.app.borrow_mut();
            app.auto_note.clear();
            let (day, night) = app.settings.auto_times;
            app.settings.auto_day_night = match index {
                1 => AutoMode::Times { day, night },
                2 => match app.settings.auto_location {
                    Some((lat, lon)) => AutoMode::Sun { lat, lon },
                    None => {
                        app.auto_note = "Enter your latitude and longitude, then Save.".into();
                        AutoMode::Off
                    }
                },
                _ => AutoMode::Off,
            };
        }
        self.forget_phases();
        self.save_settings();
        self.auto_tick();
    }

    fn save_auto_times(self: &Rc<Self>, day: &str, night: &str) {
        let (Some(day), Some(night)) = (parse_hhmm(day), parse_hhmm(night)) else {
            self.app.borrow_mut().auto_note = "Use 24-hour times like 07:00 and 19:30.".into();
            self.render();
            return;
        };
        if day == night {
            self.app.borrow_mut().auto_note = "Day and Night need different times.".into();
            self.render();
            return;
        }
        {
            let mut app = self.app.borrow_mut();
            app.auto_note.clear();
            app.settings.auto_times = (day, night);
            app.settings.auto_day_night = AutoMode::Times { day, night };
        }
        self.forget_phases();
        self.save_settings();
        self.auto_tick();
    }

    fn save_auto_location(self: &Rc<Self>, lat: &str, lon: &str) {
        let parse = |t: &str, limit: f64| t.trim().replace(',', ".").parse::<f64>().ok().filter(|v| v.abs() <= limit);
        let (Some(lat), Some(lon)) = (parse(lat, 90.0), parse(lon, 180.0)) else {
            self.app.borrow_mut().auto_note =
                "Latitude is -90 to 90 and longitude -180 to 180, in degrees (north and east are positive).".into();
            self.render();
            return;
        };
        {
            let mut app = self.app.borrow_mut();
            app.auto_note.clear();
            app.settings.auto_location = Some((lat, lon));
            app.settings.auto_day_night = AutoMode::Sun { lat, lon };
        }
        self.forget_phases();
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

/// `--get`'s (key, text, JSON value) fields for one display.
fn get_fields(st: &State, target: GetTarget) -> Vec<(&'static str, String, String)> {
    let wants = |t: GetTarget| target == GetTarget::All || target == t;
    let mut fields = vec![];
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
    fields
}

fn level_glyph(code: Option<u8>) -> (&'static str, &'static str) {
    if code == Some(BRIGHTNESS) { (glyph::SUN, "Brightness") } else { (glyph::CONTRAST, "Contrast") }
}

/// Wayland ignores `focus_window`, and a window only takes focus with the launcher's activation token when it is
/// created: for one that is already open, drop the token and ask for attention instead (GNOME shows "is ready",
/// KDE highlights its taskbar entry).
#[cfg(target_os = "linux")]
fn raise_on_wayland(ui: &MainWindow) {
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use slint::winit_030::winit::window::UserAttentionType;
    let _ = platform::instance::take_activation_token();
    ui.window().with_winit_window(|w| {
        if matches!(w.window_handle().map(|h| h.as_raw()), Ok(RawWindowHandle::Wayland(_))) {
            w.request_user_attention(Some(UserAttentionType::Informational));
        }
    });
}

/// The text "Copy capability report" puts on the clipboard.
fn capability_report(name: &str, id: &MonitorId, st: &State, raw: Option<&str>) -> String {
    let identity = st.monitor.as_ref().map(|m| m.info.identity.clone()).unwrap_or_default();
    let known = |v: Option<&str>| v.filter(|v| !v.is_empty()).unwrap_or("unknown").to_string();
    let mut lines = vec![
        format!("{} capability report", tvc_cli::version_line()),
        format!("Display: {name}"),
        format!("Id: {id}"),
        format!("EDID id: {}", known(Some(&identity.edid_id))),
        format!("Serial: {}", known(identity.serial.as_deref())),
        format!("Connector: {}", known(identity.connector.as_deref())),
        format!("Support: {} ({:?})", view::support_label(&st.level), st.level),
    ];
    let caps = st.controls.as_ref().map(|rc| &rc.capabilities);
    if let Some(caps) = caps {
        lines.push(format!("Model: {}", known(Some(caps.model.trim()))));
        lines.push(format!("MCCS version: {}", known(Some(&caps.mccs_ver))));
    }
    match caps.map(|c| c.warnings.as_slice()).unwrap_or_default() {
        [] => lines.push("Warnings: none".into()),
        warnings => {
            lines.push("Warnings:".into());
            lines.extend(warnings.iter().map(|w| format!("- {w}")));
        }
    }
    lines.push("Capabilities:".into());
    lines.push(raw.unwrap_or("(not read from the display yet)").to_string());
    lines.join("\n")
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

/// `object` (a JSON object) with the display's `id` and `name` first.
fn tagged(id: &MonitorId, name: &str, object: &str) -> String {
    let rest = object.strip_prefix('{').unwrap_or(object);
    let sep = if rest.starts_with('}') { "" } else { "," };
    format!("{{\"id\":{},\"name\":{}{sep}{rest}", json_str(&id.0), json_str(name))
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
    app.on_banner_action(|| with(|c| c.banner_action()));
    app.on_set_auto_mode(|i| with(|c| c.set_auto_mode(i)));
    app.on_save_auto_times(|day, night| with(|c| c.save_auto_times(&day, &night)));
    app.on_save_auto_location(|lat, lon| with(|c| c.save_auto_location(&lat, &lon)));
    app.on_restore_backup(|| {
        with(|c| {
            if let Some(id) = c.selected_id() {
                c.send_to(&id, Command::RestoreBackup { origin: Origin::Window });
            }
        })
    });
    app.on_select_display(|id| with(|c| c.select_display(&id)));
    app.on_set_included(|id, on| with(|c| c.set_included(&id, on)));
    app.on_set_offset(|id, offset| with(|c| c.set_offset(&id, offset)));
    app.on_copy_capability_report(|| with(|c| c.copy_capability_report()));
    app.on_dialog_result(|yes| with(|c| c.on_dialog_result(yes)));
    app.on_set_background(|on| {
        with(|c| {
            c.app.borrow_mut().settings.background_mode = on;
            c.save_settings();
        })
    });
    app.on_set_start(|on| {
        with(|c| {
            c.app.borrow_mut().settings.start_with_windows = on;
            c.save_settings();
        })
    });
    // This PC's input on the selected display.
    app.on_set_this_pc(|i| {
        with(|c| {
            let Some(id) = c.selected_id() else { return };
            let code = view::inputs(c.app.borrow().state(Some(&id))).get(i as usize).map(|(code, _)| *code);
            if let Some(code) = code.and_then(|v| u8::try_from(v).ok()) {
                monitor_entry(&mut c.app.borrow_mut().settings, &id).this_machine_input = code;
                c.save_settings();
            }
        })
    });
    app.on_run_setup_fix(|id| with(|c| c.run_setup_fix(&id)));
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
    app.on_find_hidden(|| with(|c| c.with_selected(|st| st.finder = FinderState::default())));
    app.on_finder_snapshot(|| with(|c| c.finder_command(Command::FinderSnapshot)));
    app.on_finder_compare(|| with(|c| c.finder_command(Command::FinderCompare)));
    app.on_finder_select(|i| with(|c| c.finder_select(i as usize)));
    app.on_finder_name_edited(|t| {
        with(|c| {
            let Some(id) = c.selected_id() else { return };
            if let Some(st) = c.app.borrow_mut().displays.get_mut(&id) {
                st.finder.name = t.to_string();
            }
        })
    });
    app.on_finder_value_edited(|i, t| {
        with(|c| {
            let Some(id) = c.selected_id() else { return };
            if let Some(row) = c.app.borrow_mut().displays.get_mut(&id).and_then(|st| st.finder.values.get_mut(i as usize)) {
                row.2 = t.to_string();
            }
        })
    });
    app.on_finder_save(|| with(|c| c.finder_save()));
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use tvc_core::engine::ResolvedControls;
    use tvc_core::identity::{BackendKind, DisplayInfo, MonitorId, MonitorIdentity};
    use tvc_core::link::DisplayKind;
    use tvc_core::mccs::MonitorCapabilities;
    use tvc_core::profile::ProfileStore;
    use tvc_core::support::SupportLevel;

    use super::{cli_targets, display_at, level_for, master_percent, tagged, together_value};

    fn close(actual: f32, expected: f32, tolerance: f32) -> bool {
        (actual - expected).abs() <= tolerance
    }

    #[test]
    fn together_value_adds_the_offset_and_clips() {
        assert_eq!(together_value(50.0, 0, 100), 50);
        assert_eq!(together_value(50.0, 10, 100), 60);
        assert_eq!(together_value(50.0, -10, 100), 40);
        // The sum is clipped at 0 and 100 %.
        assert_eq!(together_value(95.0, 10, 100), 100);
        assert_eq!(together_value(5.0, -10, 100), 0);
        assert_eq!(together_value(0.0, -200, 100), 0);
        assert_eq!(together_value(100.0, 200, 100), 100);
        assert_eq!(together_value(150.0, 0, 100), 100);
        assert_eq!(together_value(-5.0, 0, 100), 0);
    }

    #[test]
    fn together_value_scales_to_the_display_max() {
        assert_eq!(together_value(100.0, 0, 255), 255);
        assert_eq!(together_value(0.0, 0, 255), 0);
        // 127.5 rounds away from zero.
        assert_eq!(together_value(50.0, 0, 255), 128);
        assert_eq!(together_value(40.0, 10, 50), 25);
        assert_eq!(together_value(100.0, 50, 50), 50);
        assert_eq!(together_value(33.3, 0, 100), 33);
    }

    #[test]
    fn together_value_max_zero_is_zero() {
        assert_eq!(together_value(0.0, 0, 0), 0);
        assert_eq!(together_value(50.0, 0, 0), 0);
        assert_eq!(together_value(100.0, 50, 0), 0);
    }

    #[test]
    fn master_percent_removes_the_offset_and_clamps() {
        assert!(close(master_percent(50, 100, 0), 50.0, 1e-4));
        assert!(close(master_percent(60, 100, 10), 50.0, 1e-4));
        assert!(close(master_percent(40, 100, -10), 50.0, 1e-4));
        assert!(close(master_percent(128, 255, 0), 50.196, 1e-3));
        assert!(close(master_percent(255, 255, 0), 100.0, 1e-4));
        // Clamped to 0–100.
        assert!(close(master_percent(100, 100, -10), 100.0, 1e-4));
        assert!(close(master_percent(0, 100, 10), 0.0, 1e-4));
        assert!(close(master_percent(5, 100, 50), 0.0, 1e-4));
    }

    #[test]
    fn master_percent_max_zero_is_zero() {
        assert!(close(master_percent(0, 0, 0), 0.0, 0.0));
        assert!(close(master_percent(5, 0, 0), 0.0, 0.0));
        assert!(close(master_percent(0, 0, -50), 0.0, 0.0));
    }

    #[test]
    fn master_percent_inverts_together_value() {
        for max in [100u16, 255, 50] {
            // One raw step, in percent: what rounding to the display's own scale can lose.
            let step = 100.0 / f32::from(max);
            for offset in [-20, 0, 20] {
                for master in (0..=100).step_by(5).map(|m| m as f32) {
                    let sum = master + offset as f32;
                    let value = together_value(master, offset, max);
                    let back = master_percent(value, max, offset);
                    // Where the sum isn't clipped, the master comes back.
                    let expected = if (0.0..=100.0).contains(&sum) { master } else { (sum.clamp(0.0, 100.0) - offset as f32).clamp(0.0, 100.0) };
                    assert!(close(back, expected, step / 2.0 + 1e-3), "max {max} offset {offset} master {master}: {back}");
                }
            }
        }
    }

    fn id(text: &str) -> MonitorId {
        MonitorId(text.into())
    }

    #[test]
    fn display_at_finds_the_screen() {
        let screens = [(id("A"), (0, 0, 1920, 1080)), (id("B"), (1920, 0, 2560, 1440))];
        assert_eq!(display_at((100, 100), &screens), Some(id("A")));
        assert_eq!(display_at((0, 0), &screens), Some(id("A")));
        assert_eq!(display_at((1919, 1079), &screens), Some(id("A")));
        assert_eq!(display_at((2000, 1200), &screens), Some(id("B")));
        assert_eq!(display_at((4479, 1439), &screens), Some(id("B")));
    }

    #[test]
    fn display_at_right_and_bottom_edges_are_exclusive() {
        let screens = [(id("A"), (0, 0, 1920, 1080)), (id("B"), (1920, 0, 2560, 1440))];
        // x = 1920 is B's first column, not A's last.
        assert_eq!(display_at((1920, 10), &screens), Some(id("B")));
        // y = 1080 is below A; B is taller, so (10, 1080) is on neither.
        assert_eq!(display_at((10, 1080), &screens), None);
        assert_eq!(display_at((4480, 10), &screens), None);
        assert_eq!(display_at((2000, 1440), &screens), None);
    }

    #[test]
    fn display_at_outside_every_screen() {
        let screens = [(id("A"), (0, 0, 1920, 1080)), (id("L"), (-1280, -200, 1280, 1024))];
        assert_eq!(display_at((-1, -201), &screens), None);
        assert_eq!(display_at((5000, 5000), &screens), None);
        assert_eq!(display_at((-1, 0), &screens), Some(id("L")));
        assert_eq!(display_at((-1280, -200), &screens), Some(id("L")));
        assert_eq!(display_at((-1281, 0), &screens), None);
        assert_eq!(display_at((0, 0), &[]), None);
    }

    #[test]
    fn display_at_overlapping_screens_first_wins() {
        let screens = [(id("A"), (0, 0, 100, 100)), (id("B"), (50, 50, 100, 100))];
        assert_eq!(display_at((60, 60), &screens), Some(id("A")));
        assert_eq!(display_at((120, 120), &screens), Some(id("B")));
        let swapped = [screens[1].clone(), screens[0].clone()];
        assert_eq!(display_at((60, 60), &swapped), Some(id("B")));
    }

    fn rows(list: &[(&str, &str, bool)]) -> Vec<(String, String, bool)> {
        list.iter().map(|(id, name, able)| (id.to_string(), name.to_string(), *able)).collect()
    }

    #[test]
    fn cli_targets_all_skips_unsupported() {
        let list = rows(&[("LEN-1", "Left", true), ("DEL-9", "Dell", false), ("LEN-2", "Right", true)]);
        assert_eq!(cli_targets(None, &list).unwrap(), ["LEN-1", "LEN-2"]);
        assert_eq!(cli_targets(Some("all"), &list).unwrap(), ["LEN-1", "LEN-2"]);
        assert_eq!(cli_targets(Some("ALL"), &list).unwrap(), ["LEN-1", "LEN-2"]);
        assert_eq!(cli_targets(Some(" All "), &list).unwrap(), ["LEN-1", "LEN-2"]);
    }

    #[test]
    fn cli_targets_all_keeps_every_display_when_none_supports_the_verb() {
        let list = rows(&[("LEN-1", "Left", false), ("DEL-9", "Dell", false)]);
        assert_eq!(cli_targets(None, &list).unwrap(), ["LEN-1", "DEL-9"]);
        assert_eq!(cli_targets(Some("all"), &list).unwrap(), ["LEN-1", "DEL-9"]);
        assert!(cli_targets(None, &[]).unwrap().is_empty());
    }

    #[test]
    fn cli_targets_named_display() {
        let list = rows(&[("LEN-1", "Left", true), ("DEL-9", "Dell", false), ("LEN-2", "Right", true)]);
        // A named display is a target even when it may not support the verb: it answers for itself.
        assert_eq!(cli_targets(Some("DEL-9"), &list).unwrap(), ["DEL-9"]);
        assert_eq!(cli_targets(Some("dell"), &list).unwrap(), ["DEL-9"]);
        assert_eq!(cli_targets(Some("RIGHT"), &list).unwrap(), ["LEN-2"]);
        // An exact id wins over another display's name.
        let tricky = rows(&[("A", "B", true), ("B", "x", true)]);
        assert_eq!(cli_targets(Some("B"), &tricky).unwrap(), ["B"]);
    }

    #[test]
    fn cli_targets_ambiguous_or_unknown_is_an_error() {
        let list = rows(&[("LEN-1", "P32p-20", true), ("LEN-2", "p32p-20", true)]);
        let message = cli_targets(Some("P32P-20"), &list).unwrap_err().message;
        assert!(message.contains("several") && message.contains("LEN-1") && message.contains("LEN-2"), "{message}");
        let message = cli_targets(Some("Middle"), &list).unwrap_err().message;
        assert!(message.contains("Middle") && message.contains("P32p-20 (LEN-1)"), "{message}");
        assert!(cli_targets(Some("Middle"), &[]).is_err());
    }

    #[test]
    fn tagged_puts_id_and_name_first() {
        assert_eq!(tagged(&id("LEN-1"), "Left", "{}"), r#"{"id":"LEN-1","name":"Left"}"#);
        assert_eq!(tagged(&id("LEN-1"), "Left", r#"{"ok":true,"kind":"Ok"}"#),
            r#"{"id":"LEN-1","name":"Left","ok":true,"kind":"Ok"}"#);
        // Both are JSON strings, escaped.
        assert_eq!(tagged(&id("a\"b"), "C:\\x", "{}"), r#"{"id":"a\"b","name":"C:\\x"}"#);
    }

    fn info(edid: &str, backend: BackendKind) -> DisplayInfo {
        DisplayInfo {
            target: String::new(),
            identity: MonitorIdentity { edid_id: edid.into(), serial: None, connector: None },
            name: String::new(),
            kind: if backend == BackendKind::Backlight { DisplayKind::BuiltIn } else { DisplayKind::External },
            backend,
        }
    }

    fn controls(vcp: &[(u8, Option<Vec<u8>>)]) -> ResolvedControls {
        let profile = ProfileStore::new(std::env::temp_dir().join("tvc-app-test-no-overlay")).profile().clone();
        let capabilities = MonitorCapabilities { vcp: vcp.iter().cloned().collect(), ..Default::default() };
        ResolvedControls { profile, capabilities, controls: BTreeMap::new(), firmware: None }
    }

    #[test]
    fn level_for_built_in_panel_is_verified() {
        let panel = info("", BackendKind::Backlight);
        let verified = SupportLevel::Verified { profile: "builtin-backlight".into() };
        assert_eq!(level_for(&panel, None), verified);
        assert_eq!(level_for(&panel, Some(&controls(&[]))), verified);
        // Whatever its EDID says.
        assert_eq!(level_for(&info("LEN62A2", BackendKind::Backlight), None), verified);
    }

    #[test]
    fn level_for_empty_caps_is_read_only_unless_verified() {
        assert_eq!(level_for(&info("DEL4321", BackendKind::Ddc), Some(&controls(&[]))), SupportLevel::ReadOnly);
        assert_eq!(level_for(&info("LEN1234", BackendKind::Ddc), Some(&controls(&[]))), SupportLevel::ReadOnly);
        assert_eq!(level_for(&info("", BackendKind::Ddc), Some(&controls(&[]))), SupportLevel::ReadOnly);
        // No controls yet reads as not read yet.
        assert_eq!(level_for(&info("DEL4321", BackendKind::Ddc), None), SupportLevel::GenericMccs);
        assert_eq!(level_for(&info("DEL4321", BackendKind::Ddc), Some(&controls(&[(0x10, None)]))), SupportLevel::GenericMccs);
        assert_eq!(level_for(&info("LEN1234", BackendKind::Ddc), Some(&controls(&[(0x60, Some(vec![0x11, 0x0F]))]))),
            SupportLevel::LenovoFamily);
    }

    #[test]
    fn level_for_len62a2_is_verified() {
        let p32p20 = SupportLevel::Verified { profile: "p32p20".into() };
        let display = info("LEN62A2", BackendKind::Ddc);
        assert_eq!(level_for(&display, None), p32p20);
        assert_eq!(level_for(&display, Some(&controls(&[]))), p32p20);
        assert_eq!(level_for(&display, Some(&controls(&[(0x10, None)]))), p32p20);
    }
}
