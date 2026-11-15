//! The monitor profile: built-in definitions (`profile/*.json`, schema v2) merged with the user overlay
//! `profile.user.json` (0.0.4's camelCase format; the overlay wins per code). The built-ins are the P32p-20
//! (`p32p20`), the laptop panel (`builtin-backlight`) and the generic MCCS monitor (`generic-mccs`).

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::engine::Timing;
use crate::mccs::{self, MonitorCapabilities};
use crate::support::SupportLevel;
use crate::{hex, parse_hex};

/// The built-in profiles by key; `SupportLevel::Verified { profile }` names one.
const BUILTIN: [(&str, &str); 3] = [
    ("p32p20", include_str!("profile/p32p20.json")),
    ("builtin-backlight", include_str!("profile/builtin-backlight.json")),
    ("generic-mccs", include_str!("profile/generic-mccs.json")),
];
/// The Lenovo profile a `LenovoFamily` monitor borrows its definitions from.
const LENOVO: &str = "p32p20";
const GENERIC: &str = "generic-mccs";
const COLOUR_MODE: u8 = 0x14;
const INPUT: u8 = 0x60;
const OVERLAY_FILE: &str = "profile.user.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ControlKind {
    Range,
    Enum,
    Action,
}

/// Main = Display tab, Advanced = Monitor tab, Hidden = only the finder sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Section {
    Main,
    Advanced,
    Hidden,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SafetyClass {
    Normal,
    ConfirmRequired,
    NeverWrite,
    ReadOnlyUntilNamed,
}

/// Monitor-tab group of an Advanced control. `None` on a definition = "More settings".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Group {
    Picture,
    Ports,
    Osd,
    Reset,
}

/// One VCP code as the app knows it. The v2 presentation fields are `None` for codes named in the finder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlDefinition {
    pub code: u8,
    pub name: String,
    pub kind: ControlKind,
    pub section: Section,
    pub safety: SafetyClass,
    pub value_labels: BTreeMap<u8, String>,
    /// Fallback confirm text (0.0.4 `confirm`); `confirm_title` takes precedence in the UI.
    pub confirm_message: Option<String>,
    pub group: Option<Group>,
    pub description: Option<String>,
    /// A glyph name from the UI's `Glyphs` global ("brightness", "reset",...).
    pub icon: Option<String>,
    /// May contain `{label}` (the target value's label), as the input confirm does.
    pub confirm_title: Option<String>,
    pub confirm_body: Option<String>,
    pub confirm_button: Option<String>,
}

impl ControlDefinition {
    pub fn new(code: u8, name: impl Into<String>, kind: ControlKind, section: Section, safety: SafetyClass,
        value_labels: BTreeMap<u8, String>) -> Self {
        Self {
            code,
            name: name.into(),
            kind,
            section,
            safety,
            value_labels,
            confirm_message: None,
            group: None,
            description: None,
            icon: None,
            confirm_title: None,
            confirm_body: None,
            confirm_button: None,
        }
    }

    /// Shown as a card: not Hidden and writable once named.
    pub fn is_visible(&self) -> bool {
        self.section != Section::Hidden
            && !matches!(self.safety, SafetyClass::NeverWrite | SafetyClass::ReadOnlyUntilNamed)
    }
}

/// One input button, in display order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputOption {
    pub code: u8,
    pub label: String,
    /// "hdmi", "dp" or "usbc"; "" for any other port.
    pub kind: String,
}

/// Monitor-level profile data the UI renders from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub edid_id: String,
    pub display_name: String,
    pub short_name: String,
    pub inputs: Vec<InputOption>,
    /// Colour-mode (0x14) display order; filter by the capability list.
    pub colour_mode_order: Vec<u8>,
    /// The colour mode that enables the RGB gains.
    pub custom_colour_mode: u8,
    /// Appended to "Your monitor kept brightness at {n}".
    pub brightness_lock_note: String,
    pub timing: Timing,
    /// Monitor-tab order of the codes (`controlOrder`, else the order of `controls`); unlisted codes go last.
    pub control_order: Vec<u8>,
}

/// A built-in profile plus the user overlay in `user_dir`. Cheap to clone.
#[derive(Clone, Debug)]
pub struct ProfileStore {
    dir: PathBuf,
    profile: Arc<Profile>,
    embedded: Arc<BTreeMap<u8, ControlDefinition>>,
}

impl ProfileStore {
    /// The P32p-20 profile.
    pub fn new(user_dir: impl Into<PathBuf>) -> Self {
        Self::with(user_dir, builtin(LENOVO).expect("the P32p-20 profile is built in"))
    }

    /// The profile for `level` with the user overlay in `user_dir`:
    /// - `Verified`: the built-in profile it names (an unknown name gets the generic one);
    /// - `LenovoFamily`: the P32p-20 definitions of the codes [`lenovo_family_codes`] accepts in `caps`, plus the
    ///   Range codes `caps` advertises, input (unlabelled inputs named from the MCCS table) and the NeverWrite codes;
    ///   `vcpname` names win;
    /// - `GenericMccs` and `ReadOnly`: the generic MCCS profile, with the colour modes and inputs `caps` advertises.
    ///
    /// Without `caps` a Lenovo or generic profile enables no value lists, so build it again after the live read.
    pub fn for_level(user_dir: impl Into<PathBuf>, level: &SupportLevel, caps: Option<&MonitorCapabilities>) -> Self {
        let key = match level {
            SupportLevel::Verified { profile } if BUILTIN.iter().any(|(k, _)| k == profile) => profile.as_str(),
            SupportLevel::Verified { profile } => {
                crate::log::write(&format!("No built-in profile \"{profile}\"; using the generic MCCS one."));
                GENERIC
            }
            SupportLevel::LenovoFamily => LENOVO,
            SupportLevel::GenericMccs | SupportLevel::ReadOnly => GENERIC,
        };
        let (mut profile, mut defs) = builtin(key).expect("the key is built in");
        if *level == SupportLevel::LenovoFamily {
            let qualified = caps.map(|c| qualifying_codes(&defs, c)).unwrap_or_default();
            let inputs = advertised(caps, INPUT);
            defs.retain(|code, d| d.safety == SafetyClass::NeverWrite || qualified.contains(code)
                || (*code == INPUT && !inputs.is_empty())
                || (d.kind == ControlKind::Range && caps.is_some_and(|c| c.supports(*code))));
            // Inputs the Lenovo profile doesn't label keep the control, named from the MCCS table.
            if let Some(d) = defs.get_mut(&INPUT) {
                d.value_labels = inputs.iter()
                    .map(|&v| (v, d.value_labels.get(&v).cloned().unwrap_or_else(|| input_label(v))))
                    .collect();
            }
            for (code, name) in caps.map(|c| &c.names).into_iter().flatten() {
                if let Some(d) = defs.get_mut(code) {
                    d.name = name.clone();
                }
            }
            let known = std::mem::take(&mut profile.inputs);
            profile.inputs = inputs.iter().map(|&v| known.iter().find(|i| i.code == v).cloned()
                .unwrap_or_else(|| InputOption { code: v, label: input_label(v), kind: input_kind(v).into() }))
                .collect();
            profile.control_order.retain(|c| defs.contains_key(c));
            profile.edid_id.clear();
        } else if key == GENERIC {
            let modes = advertised(caps, COLOUR_MODE);
            if let Some(d) = defs.get_mut(&COLOUR_MODE) {
                d.value_labels = modes.iter().map(|&v| {
                    (v, d.value_labels.get(&v).cloned().unwrap_or_else(|| format!("Mode {}", hex(v))))
                }).collect();
            }
            let inputs = advertised(caps, INPUT);
            if let Some(d) = defs.get_mut(&INPUT) {
                d.value_labels = inputs.iter().map(|&v| (v, input_label(v))).collect();
            }
            profile.inputs = inputs.iter()
                .map(|&v| InputOption { code: v, label: input_label(v), kind: input_kind(v).into() })
                .collect();
        }
        if !matches!(level, SupportLevel::Verified { .. }) {
            let model = caps.map(|c| c.model.trim().to_string()).unwrap_or_default();
            profile.display_name.clone_from(&model);
            profile.short_name = model;
        }
        Self::with(user_dir, (profile, defs))
    }

    fn with(user_dir: impl Into<PathBuf>, (profile, embedded): (Profile, BTreeMap<u8, ControlDefinition>)) -> Self {
        Self { dir: user_dir.into(), profile: Arc::new(profile), embedded: Arc::new(embedded) }
    }

    pub fn profile(&self) -> &Arc<Profile> {
        &self.profile
    }

    /// A definition for every code in `caps`. Unknown codes become Enum, Hidden, ReadOnlyUntilNamed, named by the
    /// monitor's own `vcpname` when it gives one, else "Unknown 0xNN". A `vcpname` name never makes a code writable.
    pub fn resolve(&self, caps: &MonitorCapabilities) -> BTreeMap<u8, ControlDefinition> {
        let known = self.known();
        caps.vcp.keys().map(|&code| {
            let def = known.get(&code).cloned().unwrap_or_else(|| ControlDefinition::new(
                code, caps.names.get(&code).cloned().unwrap_or_else(|| format!("Unknown {}", hex(code))),
                ControlKind::Enum, Section::Hidden,
                SafetyClass::ReadOnlyUntilNamed, BTreeMap::new()));
            (code, def)
        }).collect()
    }

    /// Capabilities implied by the merged profile, for rendering before the monitor ever answered:
    /// Range is continuous, Enum allows its labelled values, Action allows 1.
    pub fn synthesize_capabilities(&self) -> MonitorCapabilities {
        let vcp = self.known().into_values().map(|d| {
            let values = match d.kind {
                ControlKind::Range => None,
                ControlKind::Enum => Some(d.value_labels.keys().copied().collect()),
                ControlKind::Action => Some(vec![1]),
            };
            (d.code, values)
        }).collect();
        MonitorCapabilities { vcp, ..Default::default() }
    }

    /// Adds or replaces one definition in the user overlay. Codes the embedded profile marks NeverWrite
    /// (0xCA, 0xD6) can't be overridden.
    pub fn save_overlay(&self, def: &ControlDefinition) -> io::Result<()> {
        if self.embedded.get(&def.code).is_some_and(|d| d.safety == SafetyClass::NeverWrite) {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, format!("{} can't be changed.", hex(def.code))));
        }
        let mut controls: Vec<ControlDefinition> =
            self.load_overlay().into_iter().filter(|d| d.code != def.code).collect();
        controls.push(def.clone());
        controls.sort_by_key(|d| d.code);
        let file = ProfileFile {
            edid_id: self.profile.edid_id.clone(),
            controls: controls.iter().map(to_entry).collect(),
            ..ProfileFile::default()
        };
        std::fs::create_dir_all(&self.dir)?;
        crate::store::write_atomic(&self.dir.join(OVERLAY_FILE), &serde_json::to_string_pretty(&file).map_err(io::Error::other)?)
    }

    /// Embedded definitions with the overlay applied. An overlay entry inherits the v2 presentation fields it lacks.
    fn known(&self) -> BTreeMap<u8, ControlDefinition> {
        let mut known = (*self.embedded).clone();
        for mut def in self.load_overlay() {
            if let Some(base) = self.embedded.get(&def.code) {
                if base.safety == SafetyClass::NeverWrite {
                    continue;
                }
                def.group = def.group.or(base.group);
                def.description = def.description.or_else(|| base.description.clone());
                def.icon = def.icon.or_else(|| base.icon.clone());
                def.confirm_title = def.confirm_title.or_else(|| base.confirm_title.clone());
                def.confirm_body = def.confirm_body.or_else(|| base.confirm_body.clone());
                def.confirm_button = def.confirm_button.or_else(|| base.confirm_button.clone());
            }
            known.insert(def.code, def);
        }
        known
    }

    /// The overlay's definitions; a missing or corrupt file gives none, and an entry that doesn't parse is skipped.
    fn load_overlay(&self) -> Vec<ControlDefinition> {
        let Ok(text) = std::fs::read_to_string(self.dir.join(OVERLAY_FILE)) else { return Vec::new() };
        let Ok(file) = serde_json::from_str::<OverlayFile>(&text) else { return Vec::new() };
        file.controls.into_iter().enumerate().filter_map(|(i, value)| {
            let def = serde_json::from_value::<ControlEntry>(value).ok().as_ref().and_then(to_definition);
            if def.is_none() {
                crate::log::write(&format!("Skipped {OVERLAY_FILE} control {}: it doesn't parse.", i + 1));
            }
            def
        }).collect()
    }
}

/// The key of the built-in profile made for exactly this EDID id ("LEN62A2" gives "p32p20").
pub(crate) fn verified_profile(edid_id: &str) -> Option<&'static str> {
    if edid_id.is_empty() {
        return None;
    }
    BUILTIN.iter().map(|(key, _)| *key).find(|key| builtin(key).is_some_and(|(p, _)| p.edid_id == edid_id))
}

/// The codes a Lenovo relative shares with the P32p-20: each one the P32p-20 labels (NeverWrite and confirm-only
/// codes aside) whose advertised value list is non-empty and inside those labels.
pub(crate) fn lenovo_family_codes(caps: &MonitorCapabilities) -> BTreeSet<u8> {
    builtin(LENOVO).map(|(_, defs)| qualifying_codes(&defs, caps)).unwrap_or_default()
}

fn qualifying_codes(defs: &BTreeMap<u8, ControlDefinition>, caps: &MonitorCapabilities) -> BTreeSet<u8> {
    defs.values()
        .filter(|d| d.safety == SafetyClass::Normal && !d.value_labels.is_empty())
        .filter(|d| matches!(caps.vcp.get(&d.code), Some(Some(values))
            if !values.is_empty() && values.iter().all(|v| d.value_labels.contains_key(v))))
        .map(|d| d.code)
        .collect()
}

fn advertised(caps: Option<&MonitorCapabilities>, code: u8) -> Vec<u8> {
    caps.and_then(|c| c.vcp.get(&code)?.clone()).unwrap_or_default()
}

fn input_label(value: u8) -> String {
    mccs::input_name(value).map_or_else(|| format!("Input {}", hex(value)), str::to_string)
}

fn input_kind(value: u8) -> &'static str {
    match value {
        0x11 | 0x12 => "hdmi",
        0x0F | 0x10 => "dp",
        _ => "",
    }
}

/// A built-in profile by key, parsed.
fn builtin(key: &str) -> Option<(Profile, BTreeMap<u8, ControlDefinition>)> {
    let (_, json) = BUILTIN.iter().find(|(k, _)| *k == key)?;
    let file: ProfileFile = serde_json::from_str(json).expect("built-in profiles are valid");
    let embedded = file.controls.iter().filter_map(to_definition).map(|d| (d.code, d)).collect();
    let control_order = match &file.control_order {
        Some(order) => order.iter().filter_map(|c| parse_hex(c)).collect(),
        None => file.controls.iter().filter_map(|c| parse_hex(&c.code)).collect(),
    };
    let timing = file.quirks.and_then(|q| q.timing).map(|t| Timing {
        gap: Duration::from_millis(t.gap_ms),
        settle: Duration::from_millis(t.settle_ms),
        retry: Duration::from_millis(t.retry_ms),
        attempts: t.attempts,
        ..Timing::default()
    });
    let profile = Profile {
        edid_id: file.edid_id,
        display_name: file.display_name.unwrap_or_default(),
        short_name: file.short_name.unwrap_or_default(),
        inputs: file.inputs.unwrap_or_default().into_iter()
            .filter_map(|i| Some(InputOption { code: parse_hex(&i.code)?, label: i.label, kind: i.kind }))
            .collect(),
        colour_mode_order: file.colour_mode_order.unwrap_or_default().iter().filter_map(|c| parse_hex(c)).collect(),
        custom_colour_mode: file.custom_colour_mode.as_deref().and_then(parse_hex).unwrap_or(0x12),
        brightness_lock_note: file.brightness_lock_note.unwrap_or_default(),
        timing: timing.unwrap_or_default(),
        control_order,
    };
    Some((profile, embedded))
}

fn to_definition(e: &ControlEntry) -> Option<ControlDefinition> {
    let mut labels = BTreeMap::new();
    for (k, v) in e.values.iter().flatten() {
        labels.insert(parse_hex(k)?, v.clone());
    }
    Some(ControlDefinition {
        confirm_message: e.confirm.clone(),
        group: e.group,
        description: e.description.clone(),
        icon: e.icon.clone(),
        confirm_title: e.confirm_title.clone(),
        confirm_body: e.confirm_body.clone(),
        confirm_button: e.confirm_button.clone(),
        ..ControlDefinition::new(parse_hex(&e.code)?, e.name.clone(), e.kind, e.section, e.safety, labels)
    })
}

fn to_entry(d: &ControlDefinition) -> ControlEntry {
    ControlEntry {
        code: hex(d.code),
        name: d.name.clone(),
        kind: d.kind,
        section: d.section,
        safety: d.safety,
        values: Some(d.value_labels.iter().map(|(&k, v)| (hex(k), v.clone())).collect()),
        confirm: d.confirm_message.clone(),
        group: d.group,
        description: d.description.clone(),
        icon: d.icon.clone(),
        confirm_title: d.confirm_title.clone(),
        confirm_body: d.confirm_body.clone(),
        confirm_button: d.confirm_button.clone(),
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileFile {
    edid_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    short_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    inputs: Option<Vec<InputEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    colour_mode_order: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    custom_colour_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    brightness_lock_note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    quirks: Option<Quirks>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    control_order: Option<Vec<String>>,
    controls: Vec<ControlEntry>,
}

/// `profile.user.json` as loaded: each control is parsed on its own.
#[derive(Deserialize)]
struct OverlayFile {
    controls: Vec<serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ControlEntry {
    code: String,
    name: String,
    kind: ControlKind,
    section: Section,
    safety: SafetyClass,
    #[serde(default)]
    values: Option<BTreeMap<String, String>>,
    #[serde(default)]
    confirm: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    group: Option<Group>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    confirm_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    confirm_body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    confirm_button: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct InputEntry {
    code: String,
    label: String,
    kind: String,
}

#[derive(Serialize, Deserialize)]
struct Quirks {
    #[serde(default)]
    timing: Option<TimingEntry>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TimingEntry {
    gap_ms: u64,
    settle_ms: u64,
    retry_ms: u64,
    attempts: u32,
}
