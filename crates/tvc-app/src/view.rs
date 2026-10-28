//! State → the window's `App` global. Pure mapping; the controller decides when to call it (only while the window
//! exists and is shown, per the background-mode rules). Models are updated row by row so an unchanged slider or
//! combo box is not rebuilt under the user's pointer.

use std::rc::Rc;

use slint::{Model, ModelRc, SharedString, VecModel};
use tvc_core::engine::MonitorStatus;
use tvc_core::link::LinkFailure;
use tvc_core::profile::{ControlDefinition, ControlKind, Group, Section};

use crate::controller::State;
use crate::{
    App, Banner, ControlGroup, ControlKind as UiKind, ControlRow, FinderChange, FinderValue, HotkeyRow, InputItem,
    PresetRow, RangeRow, Severity, SetupHint,
};

pub const BRIGHTNESS: u8 = 0x10;
pub const CONTRAST: u8 = 0x12;
pub const COLOUR_MODE: u8 = 0x14;
pub const GAINS: [u8; 3] = [0x16, 0x18, 0x1A];
pub const INPUT: u8 = 0x60;
/// The codes the Display tab shows; refreshed on show and polled while visible.
pub const DISPLAY_CODES: [u8; 7] = [BRIGHTNESS, CONTRAST, COLOUR_MODE, 0x16, 0x18, 0x1A, INPUT];

pub mod glyph {
    pub const SUN: &str = "\u{e1f8}";
    pub const MOON: &str = "\u{f87e}";
    pub const CONTRAST: &str = "\u{f2be}";
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

fn strings<I: IntoIterator<Item = String>>(items: I) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(items.into_iter().map(SharedString::from).collect::<Vec<_>>()))
}

/// Replaces rows that changed; replaces the model only when the row count changed.
fn sync<T: Clone + PartialEq + 'static>(current: ModelRc<T>, rows: Vec<T>, set: impl FnOnce(ModelRc<T>)) {
    if let Some(vec) = current.as_any().downcast_ref::<VecModel<T>>() {
        if vec.row_count() == rows.len() {
            for (i, row) in rows.into_iter().enumerate() {
                if vec.row_data(i).as_ref() != Some(&row) {
                    vec.set_row_data(i, row);
                }
            }
            return;
        }
    }
    set(ModelRc::new(Rc::new(VecModel::from(rows))));
}

/// Value labels a control offers: the capability list for the code, else the profile's labels.
pub fn options(st: &State, def: &ControlDefinition) -> Vec<(u16, String)> {
    let Some(rc) = st.controls.as_ref() else { return vec![] };
    let values: Vec<u16> = match rc.capabilities.vcp.get(&def.code) {
        Some(Some(list)) if !list.is_empty() => list.iter().map(|v| *v as u16).collect(),
        _ => def.value_labels.keys().map(|v| *v as u16).collect(),
    };
    values.into_iter().map(|v| (v, label(def, v))).collect()
}

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

pub fn status_line(st: &State) -> (&'static str, String) {
    if st.asleep && st.status == MonitorStatus::Connected {
        return (glyph::MOON, "Asleep".into());
    }
    match st.status {
        MonitorStatus::Connected => {
            let input = st
                .value(INPUT)
                .and_then(|v| inputs(st).into_iter().find(|(c, _)| *c == v))
                .map(|(_, l)| format!("Connected over {l}"))
                .unwrap_or_else(|| "Connected".into());
            let fw = st.firmware().map(|f| format!(" · Firmware {f}")).unwrap_or_default();
            (glyph::CONNECTED, input + &fw)
        }
        MonitorStatus::Connecting => (glyph::CONNECTING, "Looking for your monitor…".into()),
        MonitorStatus::NotFound => (glyph::NOT_FOUND, "Not connected".into()),
        MonitorStatus::NotResponding => (glyph::ERROR, "Not responding".into()),
    }
}

/// The state banner, for the status the hold timer has let through.
pub fn banner(st: &State) -> Banner {
    if st.safe_mode {
        return Banner {
            visible: true,
            severity: Severity::Info,
            title: s("Safe mode"),
            message: s("ThinkVision Control closed while reading the monitor's details last time, so it's using the saved copy and the hidden-settings finder is off."),
            action: s("Read them now"),
        };
    }
    if st.asleep {
        return Banner {
            visible: true,
            severity: Severity::Info,
            title: s("Your monitor is asleep"),
            message: s("Changes wait until it wakes up or you unlock this PC."),
            action: s(""),
        };
    }
    if st.retrying && st.status != MonitorStatus::Connected {
        return Banner { visible: true, severity: Severity::Info, title: s("Looking for your monitor…"), ..Default::default() };
    }
    let (status, failure) = st.shown_status;
    let (severity, title, message, action): (Severity, String, &str, &str) = match (status, failure) {
        (None, _) | (Some(MonitorStatus::Connected), _) => return Banner::default(),
        (Some(MonitorStatus::Connecting), _) => (Severity::Info, "Looking for your monitor…".into(), "", ""),
        (_, Some(LinkFailure::PermissionDenied)) => (
            Severity::Warning,
            "This app isn't allowed to use the monitor controls".into(),
            "Your system blocks access to the monitor's DDC/CI bus. See Settings → System setup.",
            "Try again",
        ),
        (_, Some(LinkFailure::ModuleMissing)) => (
            Severity::Warning,
            "Monitor controls aren't turned on in your system".into(),
            "The i2c-dev kernel module isn't loaded, so the monitor can't be reached. See Settings → System setup.",
            "Try again",
        ),
        (_, Some(LinkFailure::UnsupportedPort)) => (
            Severity::Warning,
            "This port can't control the monitor".into(),
            "Connect the monitor to a USB-C or Thunderbolt port. See Settings → System setup.",
            "Try again",
        ),
        (Some(MonitorStatus::NotResponding), _) => (
            Severity::Error,
            "Your monitor isn't responding".into(),
            "DDC/CI might be off. Hold the monitor's Exit key for 10 seconds, then try again. If this started after a graphics driver update, the previous driver may help.",
            "Try again",
        ),
        (Some(MonitorStatus::NotFound), _) => (
            Severity::Info,
            format!("{} isn't connected", st.profile.display_name),
            "Connect it and we'll find it automatically.",
            "Check again",
        ),
    };
    Banner { visible: true, severity, title: s(title), message: s(message), action: s(action) }
}

fn range_row(st: &State, code: u8, label: &str, glyph: &str) -> RangeRow {
    let reading = st.values.get(&code);
    RangeRow {
        code: code as i32,
        label: s(label),
        glyph: s(glyph),
        value: reading.map_or(0.0, |r| r.current as f32),
        maximum: reading.map_or(100.0, |r| r.max.max(1) as f32),
        enabled: reading.is_some(),
        revision: st.revision(code),
    }
}

fn preset_summary(st: &State, p: &tvc_core::store::Preset) -> String {
    let mode = p
        .colour_mode
        .and_then(|m| st.def(COLOUR_MODE).map(|d| label(d, m as u16)))
        .map(|l| format!(", {l}"))
        .unwrap_or_default();
    format!("Brightness {}, contrast {}{mode}", p.brightness, p.contrast)
}

/// Where `code` is listed in the embedded profile's `controls` (tvc-core exposes no order); unknown codes after.
fn profile_rank(code: u8) -> (usize, u8) {
    const PROFILE: &str = include_str!("../../tvc-core/src/profile/p32p20.json");
    let controls = PROFILE.find("\"controls\"").map_or("", |i| &PROFILE[i..]);
    let needle = format!("\"code\": \"0x{code:02X}\"");
    let rank = controls.match_indices("\"code\": \"0x").position(|(i, _)| controls[i..].starts_with(&needle));
    (rank.unwrap_or(usize::MAX), code)
}

fn monitor_groups(st: &State) -> Vec<ControlGroup> {
    let Some(rc) = st.controls.as_ref() else { return vec![] };
    let mut groups: Vec<(&str, Vec<ControlRow>)> =
        vec![("Picture", vec![]), ("Ports", vec![]), ("On-screen menu", vec![]), ("More settings", vec![]), ("Reset and restore", vec![])];
    // Known codes in profile order, the rest by code.
    let mut defs: Vec<&ControlDefinition> = rc.controls.values().collect();
    defs.sort_by_cached_key(|d| profile_rank(d.code));
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
            ControlKind::Action => (UiKind::Action, "Reset"),
            _ if is_toggle(def, &opts) => (UiKind::Toggle, ""),
            _ => (UiKind::Choice, ""),
        };
        let current = st.value(def.code);
        let index = match kind {
            UiKind::Toggle => current.map_or(-1, |v| v as i32),
            _ => current.and_then(|v| opts.iter().position(|(o, _)| *o == v)).map_or(-1, |i| i as i32),
        };
        let description = st.notes.get(&def.code).cloned().or_else(|| def.description.clone()).unwrap_or_default();
        groups[slot].1.push(ControlRow {
            code: def.code as i32,
            kind,
            label: s(&def.name),
            description: s(description),
            glyph: s(glyph::named(def.icon.as_deref())),
            options: strings(opts.into_iter().map(|(_, l)| l)),
            index,
            button: s(button),
            enabled: true,
            revision: st.revision(def.code),
        });
    }
    groups
        .into_iter()
        .filter(|(_, rows)| !rows.is_empty())
        .map(|(title, rows)| ControlGroup { title: s(title), rows: ModelRc::new(VecModel::from(rows)) })
        .collect()
}

pub fn render(app: &App, st: &State) {
    let (status_glyph, status_text) = status_line(st);
    app.set_model_name(s(&st.profile.display_name));
    app.set_status_glyph(s(status_glyph));
    app.set_status_text(s(status_text));
    app.set_connected(st.status == MonitorStatus::Connected);
    app.set_banner(banner(st));

    // Display tab
    let inputs = inputs(st);
    // The Display tab's input buttons only while 0x60 is a visible control.
    let input_visible = st.def(INPUT).is_some_and(ControlDefinition::is_visible);
    sync(
        app.get_inputs(),
        inputs
            .iter()
            .filter(|_| input_visible)
            .map(|(code, l)| InputItem { code: *code as i32, label: s(l), this_pc: *code == st.settings.this_machine_input as u16 })
            .collect(),
        |m| app.set_inputs(m),
    );
    app.set_active_input(st.value(INPUT).map_or(-1, |v| v as i32));
    let mut ranges = vec![];
    if st.def(BRIGHTNESS).is_some() {
        ranges.push(range_row(st, BRIGHTNESS, "Brightness", glyph::SUN));
    }
    if st.def(CONTRAST).is_some() {
        ranges.push(range_row(st, CONTRAST, "Contrast", glyph::CONTRAST));
    }
    sync(app.get_main_ranges(), ranges, |m| app.set_main_ranges(m));
    app.set_range_note(s(&st.range_note));
    sync(
        app.get_presets(),
        vec![
            PresetRow { name: s(&st.settings.day.name), glyph: s(glyph::SUN), summary: s(preset_summary(st, &st.settings.day)) },
            PresetRow { name: s(&st.settings.night.name), glyph: s(glyph::MOON), summary: s(preset_summary(st, &st.settings.night)) },
        ],
        |m| app.set_presets(m),
    );
    let modes = colour_modes(st);
    let mode = st.value(COLOUR_MODE);
    app.set_colour_mode_index(mode.and_then(|m| modes.iter().position(|(v, _)| *v == m)).map_or(-1, |i| i as i32));
    app.set_colour_revision(st.revision(COLOUR_MODE));
    sync(app.get_colour_modes(), modes.into_iter().map(|(_, l)| s(l)).collect(), |m| app.set_colour_modes(m));
    let custom = mode == Some(st.profile.custom_colour_mode as u16);
    app.set_custom_colour_available(custom);
    let custom_label = st.def(COLOUR_MODE).map(|d| label(d, st.profile.custom_colour_mode as u16)).unwrap_or_default();
    app.set_custom_colour_hint(s(if custom {
        "Red, green and blue gain".to_string()
    } else {
        format!("Available in {custom_label} colour mode")
    }));
    let gains: Vec<RangeRow> = GAINS
        .iter()
        .zip(["Red", "Green", "Blue"])
        .filter(|(c, _)| st.def(**c).is_some())
        .map(|(c, l)| range_row(st, *c, l, ""))
        .collect();
    sync(app.get_custom_colour(), gains, |m| app.set_custom_colour(m));

    // Monitor tab
    sync(app.get_monitor_groups(), monitor_groups(st), |m| app.set_monitor_groups(m));
    app.set_restore_text(s(&st.restore_text));

    // Settings tab
    sync(
        app.get_setup_hints(),
        st.setup_hints
            .iter()
            .map(|h| SetupHint { title: s(&h.title), message: s(&h.message), command: s(&h.command) })
            .collect(),
        |m| app.set_setup_hints(m),
    );
    app.set_background_mode(st.settings.background_mode);
    app.set_background_description(s(if cfg!(target_os = "macos") {
        "Keep the menu bar icon and keyboard shortcuts running after you close the window"
    } else {
        "Keep the tray icon and keyboard shortcuts running after you close the window"
    }));
    app.set_start_label(s(crate::platform::autostart::label()));
    let when = if cfg!(windows) { " when you sign in" } else { " when you log in" };
    let what = if st.settings.background_mode { "Starts in the background" } else { "Opens the window" };
    app.set_start_description(s(format!("{what}{when}")));
    app.set_start_with_os(st.settings.start_with_windows);
    sync(app.get_input_labels(), inputs.iter().map(|(_, l)| s(l)).collect(), |m| app.set_input_labels(m));
    app.set_this_pc_index(
        inputs.iter().position(|(c, _)| *c == st.settings.this_machine_input as u16).map_or(-1, |i| i as i32),
    );
    app.set_shortcuts_description(s(if !st.hotkeys_available {
        "This desktop doesn't let apps register global shortcuts. See System setup above."
    } else if st.settings.background_mode {
        "Shortcuts work whenever ThinkVision Control is running."
    } else {
        "Shortcuts work while Run in background is on. Turn it on to use them."
    }));
    sync(app.get_hotkeys(), hotkey_rows(st), |m| app.set_hotkeys(m));
    app.set_version(s(env!("CARGO_PKG_VERSION")));
    let fw = st.firmware().map(|f| format!(" · Firmware {f}")).unwrap_or_default();
    app.set_profile_text(s(format!("Monitor profile: {}{fw} · Built with Slint", st.profile.display_name)));

    render_auto(app, st);
    render_finder(app, st);
}

fn hotkey_rows(st: &State) -> Vec<HotkeyRow> {
    use tvc_core::hotkey::Action;
    Action::ALL
        .iter()
        .map(|a| {
            let text = st.settings.hotkeys.get(a).cloned().unwrap_or_default();
            let failed = st.settings.background_mode && st.hotkeys_available && st.failed_hotkeys.contains(a);
            HotkeyRow {
                label: s(a.label()),
                keys: strings(keycaps(&text)),
                warning: s(if failed { "Another app is using this shortcut" } else { "" }),
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
        (_, true) => Banner { visible: true, severity: Severity::Info, title: s("Reading the monitor…"), ..Default::default() },
        (Some((severity, text)), _) => Banner { visible: true, severity: *severity, title: s(text), ..Default::default() },
        (None, _) => Banner::default(),
    });
    app.set_finder_snapshot_text(s(match f.snapshot_count {
        Some(n) => format!("Snapshot taken ({n} codes)"),
        None => "Reads the current value of every code the monitor reports".into(),
    }));
    app.set_finder_can_compare(f.snapshot_count.is_some());
    sync(
        app.get_finder_changes(),
        f.changes
            .iter()
            .map(|c| {
                let name = st.def(c.code).map(|d| d.name.clone()).unwrap_or_else(|| format!("Unknown 0x{:02X}", c.code));
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
        AutoMode::Times { day, night } => format!("Day preset at {}, Night preset at {}.", hhmm(day), hhmm(night)),
        AutoMode::Sun { lat, lon } => {
            let offset = crate::localtime::utc_offset();
            let today = civil_date((crate::localtime::unix_now() + offset as i64).div_euclid(86_400));
            match tvc_core::solar::sun_times(today.0, today.1, today.2, lat, lon) {
                Some((rise, set)) => {
                    let local = |m: u32| hhmm(((m as i64 + offset as i64 / 60).rem_euclid(1440)) as u16);
                    format!("Today: Day preset at sunrise {}, Night preset at sunset {}.", local(rise), local(set))
                }
                None => "No sunrise or sunset there today; nothing changes until there is.".into(),
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
