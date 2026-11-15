//! State → the window's `App` global. Pure mapping; the controller decides when to call it (only while the window
//! exists and is shown, per the background-mode rules). Models are updated row by row so an unchanged slider or
//! combo box is not rebuilt under the user's pointer.

use std::collections::BTreeMap;
use std::rc::Rc;

use slint::{Model, ModelRc, SharedString, VecModel};
use tvc_core::engine::MonitorStatus;
use tvc_core::i18n::{tr, trf, trn, LANGUAGES};
use tvc_core::identity::BackendKind;
use tvc_core::link::{DisplayKind, LinkFailure};
use tvc_core::profile::{ControlDefinition, ControlKind, Group, Section};
use tvc_core::support::SupportLevel;

use crate::controller::State;
use crate::{
    App, Banner, ControlGroup, ControlKind as UiKind, ControlRow, DisplayEntry, FinderChange, FinderValue, HotkeyRow,
    InputItem, PresetRow, RangeRow, Severity, SetupHint,
};

/// One Together-mode master slider, in percent (multi-display contract). `available` is false when no included
/// display has the control; `reason` says why it is greyed.
pub struct TogetherSlider {
    pub code: u8,
    pub label: String,
    pub glyph: String,
    pub value: f32,
    pub available: bool,
    pub reason: String,
}

/// Together mode: the master sliders replace `main-ranges`, and each slider's reason (why it's greyed, or which
/// display lacks it) goes in `range-note`, one line each, under the sliders.
pub fn render_together(app: &App, sliders: &[TogetherSlider]) {
    let rows: Vec<RangeRow> = sliders
        .iter()
        .map(|t| RangeRow {
            code: i32::from(t.code),
            label: s(&t.label),
            glyph: s(&t.glyph),
            value: t.value,
            maximum: 100.0,
            enabled: t.available,
            revision: 0,
        })
        .collect();
    sync(app.get_main_ranges(), rows, |m| app.set_main_ranges(m));
    let reasons: Vec<String> =
        sliders.iter().filter(|t| !t.reason.is_empty()).map(|t| trf("{0}: {1}", &[&t.label, &t.reason])).collect();
    app.set_range_note(s(reasons.join("\n")));
}

/// The display picker (multi-display contract): `entries` in picker order, `selected_id` "" for All together, which
/// also titles the header.
pub fn render_displays(app: &App, entries: Vec<DisplayEntry>, selected_id: &str, together_available: bool,
    experimental: bool) {
    if selected_id.is_empty() && together_available {
        let included = entries.iter().filter(|e| e.included).count();
        app.set_model_name(s(tr("All displays")));
        app.set_status_glyph(s(glyph::CONNECTED));
        app.set_status_text(s(trf("{0} of {1} included · the sliders move them together", &[&included, &entries.len()])));
    }
    sync(app.get_displays(), entries, |m| app.set_displays(m));
    app.set_selected_id(selected_id.into());
    app.set_together_available(together_available);
    app.set_experimental(experimental);
}

pub const BRIGHTNESS: u8 = 0x10;
pub const CONTRAST: u8 = 0x12;
pub const COLOUR_MODE: u8 = 0x14;
pub const GAINS: [u8; 3] = [0x16, 0x18, 0x1A];
pub const INPUT: u8 = 0x60;
pub const VOLUME: u8 = 0x62;
/// The codes the Display tab shows; refreshed on show and polled while visible.
pub const DISPLAY_CODES: [u8; 8] = [BRIGHTNESS, CONTRAST, COLOUR_MODE, 0x16, 0x18, 0x1A, INPUT, VOLUME];
/// The Display tab's sliders, and Together mode's master sliders: code, English label (translated where shown) and
/// glyph.
pub const MAIN_RANGES: [(u8, &str, &str); 3] =
    [(BRIGHTNESS, "Brightness", glyph::SUN), (CONTRAST, "Contrast", glyph::CONTRAST), (VOLUME, "Volume", glyph::SPEAKER)];

pub mod glyph {
    pub const SUN: &str = "\u{e1f8}";
    pub const MOON: &str = "\u{f87e}";
    pub const CONTRAST: &str = "\u{f2be}";
    pub const SPEAKER: &str = "\u{eb43}";
    pub const CONNECTED: &str = "\u{f295}";
    pub const CONNECTING: &str = "\u{f191}";
    pub const NOT_FOUND: &str = "\u{f60a}";
    pub const ERROR: &str = "\u{f3f2}";

    /// Profile icon names → Fluent glyphs (the names `theme.slint` uses).
    pub fn named(name: Option<&str>) -> &'static str {
        match name.unwrap_or("") {
            "brightness" => SUN,
            "contrast" => CONTRAST,
            "colour" => "\u{f2f6}",
            "input" => "\u{e99a}",
            "overdrive" => "\u{f831}",
            "dyn-contrast" => "\u{f33c}",
            "aspect" => "\u{e685}",
            "usb" => "\u{edd0}",
            "language" => "\u{f4f4}",
            "reset" => "\u{f1a0}",
            _ => "\u{f588}",
        }
    }
}

fn s(text: impl AsRef<str>) -> SharedString {
    text.as_ref().into()
}

/// A profile label (control name, value or input label) as the window shows it, through the catalog. The labels the
/// profile makes up for a code it has no name for ("Unknown 0x99", "Mode 0x0C", "Input 0x31") are translated around
/// their code; the command line prints them untranslated.
pub fn display_label(text: &str) -> String {
    let code = |prefix: &str| text.strip_prefix(prefix).filter(|c| c.starts_with("0x"));
    if let Some(c) = code("Unknown ") {
        trf("Unknown {0}", &[&c])
    } else if let Some(c) = code("Mode ") {
        trf("Mode {0}", &[&c])
    } else if let Some(c) = code("Input ") {
        trf("Input {0}", &[&c])
    } else {
        tr(text)
    }
}

/// Sets the rows that changed, then adds or removes rows at the end, so a repeater keeps every unchanged item.
fn update<T: Clone + PartialEq + 'static>(vec: &VecModel<T>, rows: Vec<T>) {
    let len = rows.len();
    for (i, row) in rows.into_iter().enumerate() {
        match vec.row_data(i) {
            Some(old) if old == row => {}
            Some(_) => vec.set_row_data(i, row),
            None => vec.push(row),
        }
    }
    while vec.row_count() > len {
        vec.remove(vec.row_count() - 1);
    }
}

/// Updates the window's model in place; sets a new one only the first time.
fn sync<T: Clone + PartialEq + 'static>(current: ModelRc<T>, rows: Vec<T>, set: impl FnOnce(ModelRc<T>)) {
    match current.as_any().downcast_ref::<VecModel<T>>() {
        Some(vec) => update(vec, rows),
        None => set(ModelRc::new(Rc::new(VecModel::from(rows)))),
    }
}

/// A model for a row's nested list: the row's current one updated in place, else a new one. `ModelRc` compares by
/// pointer, so reusing it keeps the row equal and its inner repeater keeps its items (focus, open popups).
fn nested<T: Clone + PartialEq + 'static>(current: Option<ModelRc<T>>, rows: Vec<T>) -> ModelRc<T> {
    if let Some(model) = current {
        if let Some(vec) = model.as_any().downcast_ref::<VecModel<T>>() {
            update(vec, rows);
            return model;
        }
    }
    ModelRc::new(VecModel::from(rows))
}

fn strings<I: IntoIterator<Item = String>>(current: Option<ModelRc<SharedString>>, items: I) -> ModelRc<SharedString> {
    nested(current, items.into_iter().map(SharedString::from).collect())
}

/// Value labels a control offers, as the window shows them: the capability list for the code, else the profile's
/// labels.
pub fn options(st: &State, def: &ControlDefinition) -> Vec<(u16, String)> {
    let Some(rc) = st.controls.as_ref() else { return vec![] };
    let values: Vec<u16> = match rc.capabilities.vcp.get(&def.code) {
        Some(Some(list)) if !list.is_empty() => list.iter().map(|v| *v as u16).collect(),
        _ => def.value_labels.keys().map(|v| *v as u16).collect(),
    };
    values.into_iter().map(|v| (v, display_label(&label(def, v)))).collect()
}

/// A value's profile label, untranslated (the command line prints it).
pub fn label(def: &ControlDefinition, value: u16) -> String {
    u8::try_from(value)
        .ok()
        .and_then(|v| def.value_labels.get(&v).cloned())
        .unwrap_or_else(|| format!("0x{value:02X}"))
}

pub fn is_toggle(def: &ControlDefinition, opts: &[(u16, String)]) -> bool {
    def.value_labels.len() == 2
        && def.value_labels.get(&0).map(String::as_str) == Some("Off")
        && def.value_labels.get(&1).map(String::as_str) == Some("On")
        && opts.iter().all(|(v, _)| *v <= 1)
}

/// Colour modes in the profile's order, then any other value the monitor advertises (so another model's modes
/// never disappear).
pub fn colour_modes(st: &State) -> Vec<(u16, String)> {
    let Some(def) = st.def(COLOUR_MODE) else { return vec![] };
    let mut opts = options(st, def);
    let order = &st.profile.colour_mode_order;
    opts.sort_by_key(|(v, _)| order.iter().position(|o| *o as u16 == *v).unwrap_or(usize::MAX));
    opts
}

/// Inputs in the profile's order, limited to what the monitor advertises when it says.
pub fn inputs(st: &State) -> Vec<(u16, String)> {
    let advertised = st.controls.as_ref().and_then(|rc| rc.capabilities.vcp.get(&INPUT).cloned().flatten());
    st.profile
        .inputs
        .iter()
        .filter(|i| advertised.as_ref().is_none_or(|list| list.contains(&i.code)))
        .map(|i| (i.code as u16, i.label.clone()))
        .collect()
}

/// The monitor's own name as it reported it over DDC/CI (now, or when this PC last reached it); "" if no monitor
/// has ever answered.
pub fn monitor_name(st: &State) -> String {
    st.controls.as_ref().map(|rc| rc.capabilities.model.trim().to_owned()).unwrap_or_default()
}

/// The laptop's own panel: brightness through the OS backlight, nothing else.
pub fn built_in(st: &State) -> bool {
    st.monitor.as_ref().is_some_and(|m| m.info.backend == BackendKind::Backlight || m.info.kind == DisplayKind::BuiltIn)
}

/// The window's title line: the monitor's own name, else the one the OS gives it, else a plain description of the
/// state.
pub fn header_name(st: &State) -> String {
    let os_name = st.monitor.as_ref().map(|m| m.info.name.trim().to_owned()).unwrap_or_default();
    match monitor_name(st) {
        name if !name.is_empty() => name,
        _ if !os_name.is_empty() => os_name,
        _ if built_in(st) => tr("Built-in display"),
        _ if st.status == MonitorStatus::Connected => tr("Your display"),
        _ if st.status == MonitorStatus::NotFound => not_found(st).header,
        _ => tr("Display"),
    }
}

/// A display the OS reports, for explaining why the monitor can't be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachedDisplay {
    /// The name the display gives itself; empty when it gives none.
    pub name: String,
    /// PNP manufacturer id + product code ("LEN62A2"); empty when unknown.
    pub edid_id: String,
    pub kind: DisplayKind,
    /// True for the monitor the window controls: there, but it couldn't be reached.
    pub tracked: bool,
}

impl AttachedDisplay {
    /// The name the display gives, else what kind it is (with its EDID id when known).
    pub fn label(&self) -> String {
        match (self.name.trim(), self.edid_id.as_str(), self.kind) {
            ("", _, DisplayKind::BuiltIn) => tr("the built-in display"),
            ("", "", _) => tr("an unnamed display"),
            ("", id, _) => trf("a display ({0})", &[&id]),
            (name, _, _) => name.into(),
        }
    }
}

/// What a `NotFound` means, worked out from the displays the OS reports.
pub struct NotFound {
    /// The header's title when no monitor name is known.
    pub header: String,
    /// The header's status line.
    pub status: String,
    pub title: String,
    pub message: String,
}

pub fn not_found(st: &State) -> NotFound {
    let name = monitor_name(st);
    // Linux with NVIDIA's driver, which offers no control channel for the display: the setup card says what to do.
    let nvidia = st.setup_hints.iter().any(|h| h.id == "linux-nvidia");
    let nvidia_text = || tr("NVIDIA's driver isn't offering display controls for it. See Settings → System setup.");
    let title = match name.as_str() {
        "" => tr("No supported display found"),
        name => trf("{0} isn't connected", &[&name]),
    };
    let Some(displays) = st.displays.as_deref() else {
        // The backend can't list displays: say only what is known.
        return NotFound {
            header: tr("No supported display"),
            status: tr("Not connected"),
            title,
            message: if nvidia {
                nvidia_text()
            } else {
                tr("Connect a display that supports DDC/CI and it will be found automatically.")
            },
        };
    };
    if let Some(found) = displays.iter().find(|d| d.tracked) {
        let label = if name.is_empty() { found.label() } else { name };
        return NotFound {
            header: tr("Display can't be reached"),
            status: tr("Can't be reached"),
            title: trf("{0} can't be reached", &[&label]),
            message: if nvidia { nvidia_text() } else { tr("The system sees it, but isn't offering display controls for it. Try another cable or port, and check that DDC/CI is turned on in the display's own menu.") },
        };
    }
    // Every real display counts (a laptop panel too); only software screens are left out.
    let physical: Vec<String> = displays.iter().filter(|d| d.kind != DisplayKind::Virtual).map(|d| d.label()).collect();
    let message = match physical.as_slice() {
        [] if !displays.is_empty() => tr("This PC is only showing a virtual display, such as a remote session or a virtual machine. Display controls need a display connected to this PC."),
        [] => tr("No display is connected. Connect a display that supports DDC/CI and it will be found automatically."),
        [one] => trf("{0} is connected, but this app has no profile for it.", &[&capitalised(one)]),
        [rest @ .., last] => trf("{0} and {1} are connected, but this app has no profile for them.", &[&capitalised(&rest.join(", ")), last]),
    };
    NotFound { header: tr("No supported display"), status: tr("No supported display"), title, message }
}

/// "the built-in display" → "The built-in display" at the start of a sentence.
fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// The one-line reason the monitor can't be reached, for the OSD and (inside `i18n::english`) the command line.
pub fn unreachable_text(st: &State) -> String {
    match st.status {
        MonitorStatus::NotResponding => not_responding_title(st),
        _ => not_found(st).title,
    }
}

fn not_responding_title(st: &State) -> String {
    match monitor_name(st) {
        name if name.is_empty() => tr("Your display isn't responding"),
        name => trf("{0} isn't responding", &[&name]),
    }
}

pub fn status_line(st: &State) -> (&'static str, String) {
    if st.asleep && st.status == MonitorStatus::Connected {
        return (glyph::MOON, tr("Asleep"));
    }
    match st.status {
        MonitorStatus::Connected if built_in(st) => (glyph::CONNECTED, tr("Brightness through the system")),
        MonitorStatus::Connected => {
            let input = st
                .value(INPUT)
                .and_then(|v| inputs(st).into_iter().find(|(c, _)| *c == v))
                .map(|(_, l)| trf("Input: {0}", &[&display_label(&l)]))
                .unwrap_or_else(|| tr("Connected"));
            // Separate facts, one phrase each, in a " · " list.
            let fw = st.firmware().map(|f| trf("Firmware {0}", &[&f]));
            let unverified = experimental(st).then(|| tr("No verified profile"));
            (glyph::CONNECTED, [Some(input), fw, unverified].into_iter().flatten().collect::<Vec<_>>().join(" · "))
        }
        MonitorStatus::Connecting => (glyph::CONNECTING, tr("Looking for your monitor…")),
        MonitorStatus::NotFound => (glyph::NOT_FOUND, not_found(st).status),
        MonitorStatus::NotResponding => (glyph::ERROR, tr("Not responding")),
    }
}

/// How far the app supports a display, in the words the window uses.
pub fn support_label(level: &SupportLevel) -> String {
    match level {
        SupportLevel::Verified { .. } => tr("Verified"),
        SupportLevel::LenovoFamily | SupportLevel::GenericMccs => tr("Experimental"),
        SupportLevel::ReadOnly => tr("Read-only"),
    }
}

/// Below Verified, but trusted to write: standard (or same-family) controls that may not behave as labelled.
pub(crate) fn experimental(st: &State) -> bool {
    matches!(st.level, SupportLevel::LenovoFamily | SupportLevel::GenericMccs)
}

/// About: the display's own name (as it reported it over DDC/CI, else its EDID name) and how far the app supports it.
fn about_text(st: &State) -> String {
    let edid = st.displays.iter().flatten().find(|d| d.tracked).map(|d| d.name.trim().to_owned());
    let name = Some(monitor_name(st)).into_iter().chain(edid).find(|n| !n.is_empty());
    name.into_iter()
        .chain([trf("Support: {0}", &[&support_label(&st.level)])])
        .chain(st.firmware().map(|f| trf("Firmware {0}", &[&f])))
        .chain([tr("Built with Slint")])
        .collect::<Vec<_>>()
        .join(" · ")
}

/// The title and text of the card a tab shows when it has no controls.
fn empty_state(st: &State) -> (String, String) {
    match st.status {
        MonitorStatus::Connecting => (tr("Looking for your monitor…"), tr("This usually takes a second or two.")),
        MonitorStatus::Connected => (tr("No controls to show"), tr("The monitor didn't report any setting this app can change.")),
        _ => (tr("No controls yet"), tr("They appear here once the monitor answers.")),
    }
}

/// Why the monitor's cards are greyed out; "" while they aren't.
fn disabled_reason(st: &State) -> String {
    match st.status {
        MonitorStatus::Connected if st.level == SupportLevel::ReadOnly => tr("Read only on this display"),
        MonitorStatus::Connected => String::new(),
        MonitorStatus::Connecting => tr("Available once the monitor answers"),
        MonitorStatus::NotResponding => tr("Available when the monitor responds again"),
        MonitorStatus::NotFound => tr("Available when the monitor is connected"),
    }
}

/// The state banner, for the status the hold timer has let through.
pub fn banner(st: &State) -> Banner {
    if st.safe_mode {
        return Banner {
            visible: true,
            severity: Severity::Info,
            title: s(tr("Safe mode")),
            message: s(tr("ThinkVision Control closed while reading the monitor's details last time, so it's using the saved copy and the hidden-settings finder is off.")),
            action: s(tr("Read them now")),
        };
    }
    if st.asleep {
        return Banner {
            visible: true,
            severity: Severity::Info,
            title: s(tr("Your monitor is asleep")),
            message: s(tr("Changes wait until it wakes up or you unlock this PC.")),
            action: s(""),
        };
    }
    if st.retrying && st.status != MonitorStatus::Connected {
        return Banner { visible: true, severity: Severity::Info, title: s(tr("Looking for your monitor…")), ..Default::default() };
    }
    let (status, failure) = st.shown_status;
    let (severity, title, message, action): (Severity, String, String, String) = match (status, failure) {
        (Some(MonitorStatus::Connected), _) => return support_banner(st),
        (None, _) => return Banner::default(),
        (Some(MonitorStatus::Connecting), _) => (Severity::Info, tr("Looking for your monitor…"), String::new(), String::new()),
        (_, Some(LinkFailure::PermissionDenied)) => (
            Severity::Warning,
            tr("This app isn't allowed to use the monitor controls"),
            tr("Your system blocks access to the monitor's DDC/CI bus. See Settings → System setup."),
            tr("Try again"),
        ),
        (_, Some(LinkFailure::ModuleMissing)) => (
            Severity::Warning,
            tr("Monitor controls aren't turned on in your system"),
            tr("The i2c-dev kernel module isn't loaded, so the monitor can't be reached. See Settings → System setup."),
            tr("Try again"),
        ),
        (_, Some(LinkFailure::UnsupportedPort)) => (
            Severity::Warning,
            tr("This port can't control the monitor"),
            tr("Connect the monitor to a USB-C or Thunderbolt port. See Settings → System setup."),
            tr("Try again"),
        ),
        (Some(MonitorStatus::NotResponding), _) => (
            Severity::Error,
            not_responding_title(st),
            tr("It's connected but isn't answering monitor controls (DDC/CI). Check that DDC/CI is turned on in the monitor's own menu, then try again. If this started after a graphics driver update, the previous driver may help."),
            tr("Try again"),
        ),
        (Some(MonitorStatus::NotFound), _) => {
            let NotFound { title, message, .. } = not_found(st);
            (Severity::Info, title, message, tr("Check again"))
        }
    };
    Banner { visible: true, severity, title: s(title), message: s(message), action: s(action) }
}

/// The bar a connected display gets for how far it's supported; none when Verified.
fn support_banner(st: &State) -> Banner {
    let name = match header_name(st) {
        name if name == tr("Your display") => tr("this display"),
        name => name,
    };
    let (severity, title, message) = match st.level {
        _ if built_in(st) => (
            Severity::Info,
            tr("Brightness only"),
            tr("The built-in display isn't controlled over DDC/CI. Its brightness is set through the system, the same way as the brightness keys."),
        ),
        SupportLevel::Verified { .. } => return Banner::default(),
        SupportLevel::LenovoFamily | SupportLevel::GenericMccs => (
            Severity::Warning,
            tr("Experimental"),
            trf("There's no verified profile for {0}, so only standard controls are shown. Some may not behave as labelled.", &[&name]),
        ),
        SupportLevel::ReadOnly => (
            Severity::Warning,
            tr("Read only"),
            trf("{0} didn't confirm which settings it supports, so its values are shown but nothing is changed.", &[&capitalised(&name)]),
        ),
    };
    Banner { visible: true, severity, title: s(title), message: s(message), action: s("") }
}

/// `enabled` means the value is known: read while connected, or kept from before the display stopped responding.
/// The page greys the row whenever the display isn't connected.
fn range_row(st: &State, code: u8, label: &str, glyph: &str) -> RangeRow {
    let reading = st.values.get(&code);
    RangeRow {
        code: code as i32,
        label: s(label),
        glyph: s(glyph),
        value: reading.map_or(0.0, |r| r.current as f32),
        maximum: reading.map_or(100.0, |r| r.max.max(1) as f32),
        enabled: reading.is_some() && (!st.values_stale || st.status == MonitorStatus::NotResponding),
        revision: st.revision(code),
    }
}

fn preset_summary(st: &State, p: &tvc_core::store::Preset) -> String {
    let mode = p.colour_mode.and_then(|m| st.def(COLOUR_MODE).map(|d| display_label(&label(d, m as u16))));
    match mode {
        Some(mode) => trf("Brightness {0}, contrast {1}, {2}", &[&p.brightness, &p.contrast, &mode]),
        None => trf("Brightness {0}, contrast {1}", &[&p.brightness, &p.contrast]),
    }
}

/// The Monitor tab's groups. `shown` is what the window shows now: each group keeps its rows model and each card
/// its options model, so a render updates only the cards whose content changed.
fn monitor_groups(st: &State, shown: &ModelRc<ControlGroup>) -> Vec<ControlGroup> {
    let Some(rc) = st.controls.as_ref() else { return vec![] };
    let shown: Vec<ControlGroup> = shown.iter().collect();
    let shown_options: BTreeMap<i32, ModelRc<SharedString>> =
        shown.iter().flat_map(|g| g.rows.iter()).map(|r| (r.code, r.options)).collect();
    let mut groups: Vec<(String, Vec<ControlRow>)> = vec![
        (tr("Picture"), vec![]),
        (tr("Ports"), vec![]),
        (tr("On-screen menu"), vec![]),
        (tr("More settings"), vec![]),
        (tr("Reset and restore"), vec![]),
    ];
    // Codes in the profile's order, the rest by code.
    let order = &st.profile.control_order;
    let mut defs: Vec<&ControlDefinition> = rc.controls.values().collect();
    defs.sort_by_key(|d| (order.iter().position(|c| *c == d.code).unwrap_or(usize::MAX), d.code));
    let writable = st.level != SupportLevel::ReadOnly;
    for def in defs {
        if !def.is_visible() || DISPLAY_CODES.contains(&def.code) || def.kind == ControlKind::Range {
            continue;
        }
        let slot = match (def.kind, def.group) {
            (ControlKind::Action, _) | (_, Some(Group::Reset)) => 4,
            (_, Some(Group::Picture)) => 0,
            (_, Some(Group::Ports)) => 1,
            (_, Some(Group::Osd)) => 2,
            _ if def.section == Section::Hidden => continue,
            _ => 3,
        };
        let opts = options(st, def);
        let (kind, button) = match def.kind {
            ControlKind::Action => (UiKind::Action, tr("Reset")),
            _ if is_toggle(def, &opts) => (UiKind::Toggle, String::new()),
            _ => (UiKind::Choice, String::new()),
        };
        let current = st.value(def.code);
        let index = match kind {
            UiKind::Toggle => current.map_or(-1, |v| v as i32),
            _ => current.and_then(|v| opts.iter().position(|(o, _)| *o == v)).map_or(-1, |i| i as i32),
        };
        let code = def.code as i32;
        groups[slot].1.push(ControlRow {
            code,
            kind,
            label: s(display_label(&def.name)),
            description: s(def.description.as_deref().map(tr).unwrap_or_default()),
            note: s(st.notes.get(&def.code).map_or("", String::as_str)),
            glyph: s(glyph::named(def.icon.as_deref())),
            options: strings(shown_options.get(&code).cloned(), opts.into_iter().map(|(_, l)| l)),
            index,
            button: s(button),
            enabled: writable,
            revision: st.revision(def.code),
        });
    }
    groups
        .into_iter()
        .filter(|(_, rows)| !rows.is_empty())
        .map(|(title, rows)| {
            let rows_model = shown.iter().find(|g| g.title == title.as_str()).map(|g| g.rows.clone());
            ControlGroup { title: s(title), rows: nested(rows_model, rows) }
        })
        .collect()
}

/// One display's state. In Together mode `render_displays` titles the header and `render_together` fills the
/// sliders, so this leaves them alone and shows a banner only for safe mode.
pub fn render(app: &App, st: &State) {
    let together = st.selected_together();
    if !together {
        let (status_glyph, status_text) = status_line(st);
        app.set_model_name(s(header_name(st)));
        app.set_status_glyph(s(status_glyph));
        app.set_status_text(s(status_text));
    }
    app.set_connected(st.status == MonitorStatus::Connected);
    app.set_values_stale(st.values_stale);
    app.set_banner(if together && !st.safe_mode { Banner::default() } else { banner(st) });
    app.set_loading(
        !together
            && (st.status == MonitorStatus::Connecting
                || (st.retrying && st.status != MonitorStatus::Connected)
                || st.reading_capabilities),
    );
    app.set_disabled_reason(s(disabled_reason(st)));
    let (empty_title, empty_message) = empty_state(st);
    app.set_empty_title(s(empty_title));
    app.set_empty_message(s(empty_message));
    app.set_has_controls(st.controls.as_ref().is_some_and(|rc| !rc.controls.is_empty()));

    // Display tab
    let inputs = inputs(st);
    // The Display tab's input buttons only while 0x60 is a visible control.
    let input_visible = st.def(INPUT).is_some_and(ControlDefinition::is_visible);
    sync(
        app.get_inputs(),
        inputs
            .iter()
            .filter(|_| input_visible)
            .map(|(code, l)| InputItem { code: *code as i32, label: s(display_label(l)), this_pc: *code == st.settings.this_machine_input as u16 })
            .collect(),
        |m| app.set_inputs(m),
    );
    app.set_active_input(st.value(INPUT).map_or(-1, |v| v as i32));
    app.set_input_note(s(st.notes.get(&INPUT).map_or("", String::as_str)));
    if !together {
        let ranges = MAIN_RANGES
            .into_iter()
            .filter(|(code, ..)| st.def(*code).is_some())
            .map(|(code, label, glyph)| range_row(st, code, &tr(label), glyph))
            .collect();
        sync(app.get_main_ranges(), ranges, |m| app.set_main_ranges(m));
        app.set_range_note(s(&st.range_note));
    }
    // The built-in panel has no Day and Night presets: it keeps the brightness set for it.
    let presets = if built_in(st) {
        vec![]
    } else {
        vec![
            PresetRow { name: s(tr(&st.settings.day.name)), glyph: s(glyph::SUN), summary: s(preset_summary(st, &st.settings.day)) },
            PresetRow { name: s(tr(&st.settings.night.name)), glyph: s(glyph::MOON), summary: s(preset_summary(st, &st.settings.night)) },
        ]
    };
    sync(app.get_presets(), presets, |m| app.set_presets(m));
    app.set_preset_note(s(&st.preset_note));
    let modes = colour_modes(st);
    let mode = st.value(COLOUR_MODE);
    app.set_colour_mode_index(mode.and_then(|m| modes.iter().position(|(v, _)| *v == m)).map_or(-1, |i| i as i32));
    app.set_colour_revision(st.revision(COLOUR_MODE));
    sync(app.get_colour_modes(), modes.into_iter().map(|(_, l)| s(l)).collect(), |m| app.set_colour_modes(m));
    let custom = mode == Some(st.profile.custom_colour_mode as u16);
    app.set_custom_colour_available(custom);
    let custom_label = st.def(COLOUR_MODE).map(|d| display_label(&label(d, st.profile.custom_colour_mode as u16)));
    app.set_custom_colour_hint(s(match custom_label {
        _ if custom => tr("Red, green and blue gain"),
        Some(l) => trf("Available in {0} colour mode", &[&l]),
        None => String::new(),
    }));
    app.set_colour_note(s(
        [COLOUR_MODE].iter().chain(&GAINS).find_map(|c| st.notes.get(c)).map_or("", String::as_str),
    ));
    let gains: Vec<RangeRow> = GAINS
        .iter()
        .zip([tr("Red"), tr("Green"), tr("Blue")])
        .filter(|(c, _)| st.def(**c).is_some())
        .map(|(c, l)| range_row(st, *c, &l, ""))
        .collect();
    sync(app.get_custom_colour(), gains, |m| app.set_custom_colour(m));

    // Monitor tab
    let groups = monitor_groups(st, &app.get_monitor_groups());
    sync(app.get_monitor_groups(), groups, |m| app.set_monitor_groups(m));
    app.set_restore_text(s(if st.restore_text.is_empty() {
        tr("Put back the settings saved before the last reset")
    } else {
        st.restore_text.clone()
    }));

    // Settings tab
    sync(
        app.get_setup_hints(),
        st.setup_hints
            .iter()
            .map(|h| {
                let run = st.setup_runs.get(h.id);
                SetupHint {
                    id: s(h.id),
                    title: s(&h.title),
                    message: s(&h.message),
                    command: s(&h.command),
                    action: s(h.fix.as_ref().map_or("", |f| f.label.as_str())),
                    busy: run.is_some_and(|r| r.running || r.checking),
                    result: s(run.map_or("", |r| r.result.as_str())),
                    result_error: run.is_some_and(|r| r.error),
                }
            })
            .collect(),
        |m| app.set_setup_hints(m),
    );
    app.set_background_mode(st.settings.background_mode);
    app.set_background_description(s(if cfg!(target_os = "macos") {
        tr("Keep the menu bar icon and keyboard shortcuts running after you close the window")
    } else {
        tr("Keep the tray icon and keyboard shortcuts running after you close the window")
    }));
    app.set_start_label(s(crate::platform::autostart::label()));
    app.set_start_description(s(match (st.settings.background_mode, cfg!(windows)) {
        (true, true) => tr("Starts in the background when you sign in"),
        (true, false) => tr("Starts in the background when you log in"),
        (false, true) => tr("Opens the window when you sign in"),
        (false, false) => tr("Opens the window when you log in"),
    }));
    app.set_start_with_os(st.settings.start_with_windows);
    sync(app.get_input_labels(), inputs.iter().map(|(_, l)| s(display_label(l))).collect(), |m| app.set_input_labels(m));
    app.set_this_pc_index(
        inputs.iter().position(|(c, _)| *c == st.settings.this_machine_input as u16).map_or(-1, |i| i as i32),
    );
    app.set_shortcuts_description(s(if !st.hotkeys_available {
        tr("This desktop doesn't let apps register global shortcuts. See System setup above.")
    } else if st.settings.background_mode {
        tr("Shortcuts work whenever ThinkVision Control is running.")
    } else {
        tr("Shortcuts work while Run in background is on. Turn it on to use them.")
    }));
    let hotkeys = hotkey_rows(st, &app.get_hotkeys());
    sync(app.get_hotkeys(), hotkeys, |m| app.set_hotkeys(m));
    app.set_version(s(env!("CARGO_PKG_VERSION")));
    app.set_profile_text(s(about_text(st)));
    render_languages(app, st);

    render_auto(app, st);
    render_finder(app, st);
}

/// Settings → Language: System, then each shipped language in its own name (never translated).
fn render_languages(app: &App, st: &State) {
    let names = std::iter::once(tr("System")).chain(LANGUAGES.iter().map(|(_, name)| (*name).to_string()));
    sync(app.get_languages(), names.map(SharedString::from).collect(), |m| app.set_languages(m));
    let index = st
        .settings
        .language
        .as_deref()
        .and_then(|code| LANGUAGES.iter().position(|(c, _)| *c == code))
        .map_or(0, |i| i as i32 + 1);
    app.set_language_index(index);
}

/// Each row keeps the keycap model the window shows at its place in `shown`.
fn hotkey_rows(st: &State, shown: &ModelRc<HotkeyRow>) -> Vec<HotkeyRow> {
    use tvc_core::hotkey::Action;
    Action::ALL
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let text = st.settings.hotkeys.get(a).cloned().unwrap_or_default();
            let failed = st.settings.background_mode && st.hotkeys_available && st.failed_hotkeys.contains(a);
            HotkeyRow {
                label: s(tr(a.label())),
                keys: strings(shown.row_data(i).map(|r| r.keys), keycaps(&text)),
                warning: s(if failed { tr("Another app is using this shortcut") } else { String::new() }),
            }
        })
        .collect()
}

/// "Ctrl+Alt+Up" → ["Ctrl", "Alt", "↑"], with per-OS modifier names.
pub fn keycaps(gesture: &str) -> Vec<String> {
    gesture
        .split('+')
        .filter(|p| !p.is_empty())
        .map(|p| {
            match p {
                "Up" => "↑",
                "Down" => "↓",
                "Left" => "←",
                "Right" => "→",
                "Win" if cfg!(target_os = "macos") => "Cmd",
                "Win" if cfg!(target_os = "linux") => "Super",
                "Alt" if cfg!(target_os = "macos") => "Option",
                other => other,
            }
            .to_string()
        })
        .collect()
}

fn render_finder(app: &App, st: &State) {
    let f = &st.finder;
    app.set_finder_busy(f.busy);
    app.set_finder_status(match (&f.status, f.busy) {
        (_, true) => Banner { visible: true, severity: Severity::Info, title: s(tr("Reading the monitor…")), ..Default::default() },
        (Some((severity, text)), _) => Banner { visible: true, severity: *severity, title: s(text), ..Default::default() },
        (None, _) => Banner::default(),
    });
    app.set_finder_snapshot_text(s(match f.snapshot_count {
        Some(n) => trn("Snapshot taken ({0} code)", "Snapshot taken ({0} codes)", n as u64, &[&n]),
        None => tr("Reads the current value of every code the monitor reports"),
    }));
    app.set_finder_can_compare(f.snapshot_count.is_some());
    sync(
        app.get_finder_changes(),
        f.changes
            .iter()
            .map(|c| {
                let name = st.def(c.code).map(|d| display_label(&d.name)).unwrap_or_else(|| trf("Unknown {0}", &[&format!("0x{:02X}", c.code)]));
                FinderChange { code: c.code as i32, text: s(format!("0x{:02X} ({name}): {} → {}", c.code, c.before, c.after)) }
            })
            .collect(),
        |m| app.set_finder_changes(m),
    );
    app.set_finder_selected(f.selected.map_or(-1, |i| i as i32));
    app.set_finder_name(s(&f.name));
    // A refilled form gets a fresh model, so the label fields are rebuilt instead of keeping text typed for the
    // previous change (an edited LineEdit no longer follows its binding). The name field listens to `finder-form`.
    let refilled = app.get_finder_form() != f.form;
    app.set_finder_form(f.form);
    let values: Vec<FinderValue> =
        f.values.iter().map(|(_, caption, label)| FinderValue { caption: s(caption), label: s(label) }).collect();
    if refilled {
        app.set_finder_values(ModelRc::new(VecModel::from(values)));
    } else {
        sync(app.get_finder_values(), values, |m| app.set_finder_values(m));
    }
}

/// Automatic Day/Night: the mode, the remembered times and place, and a line saying what happens next.
fn render_auto(app: &App, st: &State) {
    use tvc_core::schedule::AutoMode;
    let hhmm = |m: u16| format!("{:02}:{:02}", m / 60, m % 60);
    let (day, night) = st.settings.auto_times;
    app.set_auto_mode(match st.settings.auto_day_night {
        AutoMode::Off => 0,
        AutoMode::Times { .. } => 1,
        AutoMode::Sun { .. } => 2,
    });
    app.set_auto_day(s(hhmm(day)));
    app.set_auto_night(s(hhmm(night)));
    let (lat, lon) = st.settings.auto_location.map_or((String::new(), String::new()), |(a, b)| (format!("{a:.4}"), format!("{b:.4}")));
    app.set_auto_lat(s(lat));
    app.set_auto_lon(s(lon));
    let summary = match st.settings.auto_day_night {
        AutoMode::Off => String::new(),
        AutoMode::Times { day, night } => trf("Day preset at {0}, Night preset at {1}.", &[&hhmm(day), &hhmm(night)]),
        AutoMode::Sun { lat, lon } => {
            let offset = crate::localtime::utc_offset();
            let today = civil_date((crate::localtime::unix_now() + offset as i64).div_euclid(86_400));
            match tvc_core::solar::sun_times(today.0, today.1, today.2, lat, lon) {
                Some((rise, set)) => {
                    let local = |m: u32| hhmm(((m as i64 + offset as i64 / 60).rem_euclid(1440)) as u16);
                    trf("Today: Day preset at sunrise {0}, Night preset at sunset {1}.", &[&local(rise), &local(set)])
                }
                None => tr("No sunrise or sunset there today; nothing changes until there is."),
            }
        }
    };
    app.set_auto_note(s(if st.auto_note.is_empty() { summary } else { st.auto_note.clone() }));
}

/// Days since 1970-01-01 → (year, month, day), proleptic Gregorian (H. Hinnant's civil_from_days).
fn civil_date(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use slint::{Model, ModelRc};
    use tvc_core::engine::{MonitorStatus, ResolvedControls};
    use tvc_core::hub::Monitor;
    use tvc_core::identity::{BackendKind, DisplayInfo, MonitorId, MonitorIdentity};
    use tvc_core::link::{DisplayKind, VcpReading};
    use tvc_core::mccs::MonitorCapabilities;
    use tvc_core::profile::{Profile, ProfileStore};
    use tvc_core::support::SupportLevel;

    use super::{
        built_in, disabled_reason, glyph, header_name, monitor_groups, status_line, support_label, AttachedDisplay,
        DISPLAY_CODES, INPUT, VOLUME,
    };
    use crate::controller::State;

    fn store() -> ProfileStore {
        ProfileStore::new(std::env::temp_dir().join("tvc-app-test-no-overlay"))
    }

    fn profile() -> Arc<Profile> {
        store().profile().clone()
    }

    fn monitor(name: &str, kind: DisplayKind, backend: BackendKind) -> Monitor {
        Monitor {
            id: MonitorId("LEN62A2-S1".into()),
            info: DisplayInfo {
                target: String::new(),
                identity: MonitorIdentity { edid_id: "LEN62A2".into(), serial: Some("S1".into()), connector: None },
                name: name.into(),
                kind,
                backend,
            },
        }
    }

    fn state(monitor: Option<Monitor>) -> State {
        State::for_test(profile(), monitor)
    }

    /// Controls as the P32p-20 profile resolves `vcp`, with the monitor's own `model` name.
    fn controls(model: &str, vcp: &[(u8, Option<Vec<u8>>)], firmware: Option<&str>) -> Arc<ResolvedControls> {
        let store = store();
        let capabilities =
            MonitorCapabilities { model: model.into(), vcp: vcp.iter().cloned().collect(), ..Default::default() };
        Arc::new(ResolvedControls {
            profile: store.profile().clone(),
            controls: store.resolve(&capabilities),
            capabilities,
            firmware: firmware.map(String::from),
        })
    }

    #[test]
    fn support_label_words() {
        assert_eq!(support_label(&SupportLevel::Verified { profile: "p32p20".into() }), "Verified");
        assert_eq!(support_label(&SupportLevel::Verified { profile: "builtin-backlight".into() }), "Verified");
        assert_eq!(support_label(&SupportLevel::LenovoFamily), "Experimental");
        assert_eq!(support_label(&SupportLevel::GenericMccs), "Experimental");
        assert_eq!(support_label(&SupportLevel::ReadOnly), "Read-only");
    }

    #[test]
    fn header_name_prefers_the_monitor_then_the_os_name() {
        let mut st = state(Some(monitor(" DELL U2720Q ", DisplayKind::External, BackendKind::Ddc)));
        assert_eq!(header_name(&st), "DELL U2720Q");
        // The name the monitor reported over DDC/CI wins.
        st.controls = Some(controls(" P32p-20 ", &[(0x10, None)], None));
        assert_eq!(header_name(&st), "P32p-20");
    }

    #[test]
    fn header_name_without_a_name() {
        let mut st = state(None);
        assert_eq!(st.status, MonitorStatus::Connecting);
        assert_eq!(header_name(&st), "Display");
        st.status = MonitorStatus::Connected;
        assert_eq!(header_name(&st), "Your display");
        st.status = MonitorStatus::NotResponding;
        assert_eq!(header_name(&st), "Display");
        st.status = MonitorStatus::NotFound;
        assert_eq!(header_name(&st), "No supported display");
        st.displays = Some(vec![AttachedDisplay {
            name: String::new(),
            edid_id: "LEN62A2".into(),
            kind: DisplayKind::External,
            tracked: true,
        }]);
        assert_eq!(header_name(&st), "Display can't be reached");
        let panel = state(Some(monitor("", DisplayKind::BuiltIn, BackendKind::Backlight)));
        assert_eq!(header_name(&panel), "Built-in display");
    }

    #[test]
    fn status_line_connected() {
        let mut st = state(Some(monitor("", DisplayKind::External, BackendKind::Ddc)));
        st.status = MonitorStatus::Connected;
        assert_eq!(status_line(&st), (glyph::CONNECTED, "Connected".to_string()));
        st.values.insert(INPUT, VcpReading { current: 0x11, max: 0 });
        assert_eq!(status_line(&st), (glyph::CONNECTED, "Input: HDMI".to_string()));
        st.controls = Some(controls("P32p-20", &[(INPUT, Some(vec![0x11, 0x0F]))], Some("LB1.2")));
        assert_eq!(status_line(&st), (glyph::CONNECTED, "Input: HDMI · Firmware LB1.2".to_string()));
        // An input the monitor doesn't advertise has no label.
        st.values.insert(INPUT, VcpReading { current: 0x31, max: 0 });
        assert_eq!(status_line(&st), (glyph::CONNECTED, "Connected · Firmware LB1.2".to_string()));
    }

    #[test]
    fn status_line_by_level_and_state() {
        let mut st = state(Some(monitor("", DisplayKind::External, BackendKind::Ddc)));
        st.status = MonitorStatus::Connected;
        st.level = SupportLevel::GenericMccs;
        assert_eq!(status_line(&st), (glyph::CONNECTED, "Connected · No verified profile".to_string()));
        st.level = SupportLevel::LenovoFamily;
        assert_eq!(status_line(&st), (glyph::CONNECTED, "Connected · No verified profile".to_string()));
        st.level = SupportLevel::ReadOnly;
        assert_eq!(status_line(&st), (glyph::CONNECTED, "Connected".to_string()));
        st.asleep = true;
        assert_eq!(status_line(&st), (glyph::MOON, "Asleep".to_string()));
        st.status = MonitorStatus::Connecting;
        assert_eq!(status_line(&st), (glyph::CONNECTING, "Looking for your monitor…".to_string()));
        st.status = MonitorStatus::NotResponding;
        assert_eq!(status_line(&st), (glyph::ERROR, "Not responding".to_string()));
        st.status = MonitorStatus::NotFound;
        assert_eq!(status_line(&st), (glyph::NOT_FOUND, "Not connected".to_string()));
        let mut panel = state(Some(monitor("", DisplayKind::BuiltIn, BackendKind::Backlight)));
        panel.status = MonitorStatus::Connected;
        assert_eq!(status_line(&panel), (glyph::CONNECTED, "Brightness through the system".to_string()));
    }

    #[test]
    fn disabled_reason_by_state() {
        let mut st = state(None);
        let cases = [
            (MonitorStatus::Connected, SupportLevel::ReadOnly, "Read only on this display"),
            (MonitorStatus::Connected, SupportLevel::Verified { profile: "p32p20".into() }, ""),
            (MonitorStatus::Connected, SupportLevel::GenericMccs, ""),
            (MonitorStatus::Connecting, SupportLevel::ReadOnly, "Available once the monitor answers"),
            (MonitorStatus::NotResponding, SupportLevel::GenericMccs, "Available when the monitor responds again"),
            (MonitorStatus::NotFound, SupportLevel::LenovoFamily, "Available when the monitor is connected"),
        ];
        for (status, level, reason) in cases {
            st.status = status;
            st.level = level.clone();
            assert_eq!(disabled_reason(&st), reason, "{status:?} {level:?}");
        }
    }

    /// The Monitor tab as (group title, codes).
    fn groups(st: &State) -> Vec<(String, Vec<i32>)> {
        monitor_groups(st, &ModelRc::default())
            .into_iter()
            .map(|g| (g.title.to_string(), g.rows.iter().map(|r| r.code).collect()))
            .collect()
    }

    fn monitor_tab_state() -> State {
        let mut st = state(Some(monitor("", DisplayKind::External, BackendKind::Ddc)));
        st.status = MonitorStatus::Connected;
        // Ranges, Display-tab codes, hidden and unknown codes stay off the Monitor tab.
        st.controls = Some(controls("P32p-20", &[(0x10, None), (INPUT, Some(vec![0x11])), (0x86, None), (0xE0, None),
            (0xEA, None), (0xEC, None), (0x04, Some(vec![1])), (0xCA, None), (0x99, None)], None));
        st
    }

    #[test]
    fn monitor_groups_follow_the_profile_order() {
        let mut st = monitor_tab_state();
        // The P32p-20 order: E0, EA, 86 (not by code), then EC, then the resets.
        assert_eq!(groups(&st), [
            ("Picture".to_string(), vec![0xE0, 0xEA, 0x86]),
            ("Ports".to_string(), vec![0xEC]),
            ("Reset and restore".to_string(), vec![0x04]),
        ]);
        let mut reordered = (*st.profile).clone();
        reordered.control_order = vec![0x86, 0xEA, 0xE0];
        st.profile = Arc::new(reordered.clone());
        assert_eq!(groups(&st)[0], ("Picture".to_string(), vec![0x86, 0xEA, 0xE0]));
        // Unlisted codes go last, by code.
        reordered.control_order = vec![0xEA];
        st.profile = Arc::new(reordered);
        assert_eq!(groups(&st)[0], ("Picture".to_string(), vec![0xEA, 0x86, 0xE0]));
    }

    #[test]
    fn monitor_groups_read_only_rows_are_disabled() {
        let mut st = monitor_tab_state();
        let enabled = |st: &State| -> Vec<bool> {
            monitor_groups(st, &ModelRc::default()).iter().flat_map(|g| g.rows.iter().map(|r| r.enabled)).collect()
        };
        assert!(enabled(&st).iter().all(|e| *e));
        st.level = SupportLevel::ReadOnly;
        let rows = enabled(&st);
        assert_eq!(rows.len(), 5);
        assert!(rows.iter().all(|e| !*e));
        st.controls = None;
        assert!(groups(&st).is_empty());
    }

    #[test]
    fn built_in_panel_gets_no_presets() {
        // `render` leaves the Day and Night preset cards out exactly when `built_in` holds.
        assert!(built_in(&state(Some(monitor("", DisplayKind::BuiltIn, BackendKind::Backlight)))));
        assert!(built_in(&state(Some(monitor("", DisplayKind::BuiltIn, BackendKind::Ddc)))));
        assert!(built_in(&state(Some(monitor("", DisplayKind::External, BackendKind::Backlight)))));
        assert!(!built_in(&state(Some(monitor("", DisplayKind::External, BackendKind::Ddc)))));
        assert!(!built_in(&state(None)));
    }

    #[test]
    fn display_codes_include_volume() {
        assert_eq!(VOLUME, 0x62);
        assert!(DISPLAY_CODES.contains(&0x62));
        assert!(DISPLAY_CODES.contains(&INPUT));
    }
}
