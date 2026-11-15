//! Files in the data directory, in 0.0.4's names and formats: `settings.json`, `capabilities.txt`,
//! `backup-yyyyMMdd-HHmmss[-n].json`.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::de::Error as _;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::hotkey::Action;
use crate::schedule::{AutoMode, Phase};
use crate::{hex, parse_hex};

/// A Day/Night preset. Applied colour mode first, then brightness, then contrast.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preset {
    pub name: String,
    pub brightness: u32,
    pub contrast: u32,
    pub colour_mode: Option<u8>,
}

/// Not `Eq`: [`AutoMode::Sun`] holds floats.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// Gesture text per action; "" = not set. Always has all 11 actions.
    pub hotkeys: BTreeMap<Action, String>,
    pub day: Preset,
    pub night: Preset,
    pub this_machine_input: u8,
    pub start_with_windows: bool,
    pub background_mode: bool,
    /// automatic Day/Night; Off unless the user turns it on.
    pub auto_day_night: AutoMode,
    /// The last Day/Night times chosen (minutes after local midnight), kept while another mode is on so the
    /// settings UI can offer them again. Saved from `auto_day_night` when that is `Times`.
    pub auto_times: (u16, u16),
    /// The last location typed (latitude, longitude), kept like `auto_times`. Saved from `auto_day_night` when
    /// that is `Sun`.
    pub auto_location: Option<(f64, f64)>,
    /// The phase the schedule last applied (`"LastAppliedPhase"`); None until it first applies one.
    pub last_applied_phase: Option<Phase>,
}

/// Day 07:00, Night 19:00.
pub const DEFAULT_AUTO_TIMES: (u16, u16) = (7 * 60, 19 * 60);

impl Default for Settings {
    /// A new install: background mode off.
    fn default() -> Self {
        Self {
            hotkeys: Action::ALL.into_iter().map(|a| (a, a.default_gesture().to_string())).collect(),
            day: Preset { name: "Day".into(), brightness: 60, contrast: 70, colour_mode: None },
            night: Preset { name: "Night".into(), brightness: 0, contrast: 70, colour_mode: None },
            this_machine_input: 0x11,
            start_with_windows: false,
            background_mode: false,
            auto_day_night: AutoMode::Off,
            auto_times: DEFAULT_AUTO_TIMES,
            auto_location: None,
            last_applied_phase: None,
        }
    }
}

/// `settings.json` in a directory. Missing → defaults; corrupt → defaults and the file is renamed `settings.json.bak`
/// (unless that is newer); unreadable → defaults, and saving is refused so the file isn't overwritten.
#[derive(Clone, Debug)]
pub struct SettingsStore {
    dir: PathBuf,
    /// The last load couldn't read the file.
    unreadable: Arc<AtomicBool>,
}

impl SettingsStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into(), unreadable: Arc::default() }
    }

    pub fn load(&self) -> Settings {
        let path = self.dir.join("settings.json");
        let read = std::fs::read(&path).or_else(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                return Err(e);
            }
            std::thread::sleep(Duration::from_millis(100));
            std::fs::read(&path)
        });
        self.unreadable.store(false, Ordering::Relaxed);
        let bytes = match read {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Settings::default(),
            Err(e) => {
                crate::log::write(&format!("settings.json couldn't be read, using defaults: {e}"));
                self.unreadable.store(true, Ordering::Relaxed);
                return Settings::default();
            }
        };
        let parsed = std::str::from_utf8(&bytes)
            .map_err(serde_json::Error::custom)
            .and_then(|text| serde_json::from_str::<SettingsFile>(text.strip_prefix('\u{feff}').unwrap_or(text)));
        let file = match parsed {
            Ok(file) => file,
            Err(e) => {
                crate::log::write(&format!("settings.json is corrupt, using defaults: {e}"));
                self.keep_corrupt(&path);
                return Settings::default();
            }
        };
        let saved = file.hotkeys.unwrap_or_default();
        let hotkeys = Action::ALL.into_iter().map(|action| {
            let default = action.default_gesture();
            let gesture = match (action, saved.get(action.key()).map(String::as_str)) {
                (Action::BrightnessUp, Some("Ctrl+Alt+PageUp"))
                | (Action::BrightnessDown, Some("Ctrl+Alt+PageDown"))
                | (Action::ContrastUp, Some("Ctrl+Alt+Shift+PageUp"))
                | (Action::ContrastDown, Some("Ctrl+Alt+Shift+PageDown"))
                | (_, None) => default,
                // CLI-01: 0.0.8 saved the same defaults on Linux, where Ctrl+Alt+F-keys and arrows clash.
                (_, Some(old)) if cfg!(target_os = "linux") && old == action.default_gesture_for(false) => default,
                (_, Some(custom)) => custom,
            };
            (action, gesture.to_string())
        }).collect();
        Settings {
            hotkeys,
            day: file.day.into(),
            night: file.night.into(),
            this_machine_input: file.this_machine_input.0,
            start_with_windows: file.start_with_windows,
            background_mode: file.background_mode,
            auto_day_night: file.auto_day_night.mode,
            auto_times: file.auto_day_night.times,
            auto_location: file.auto_day_night.location,
            last_applied_phase: file.last_applied_phase,
        }
    }

    /// Renames a corrupt `settings.json` to `settings.json.bak`, unless the `.bak` there is newer.
    fn keep_corrupt(&self, path: &Path) {
        let bak = self.dir.join("settings.json.bak");
        let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        if matches!((modified(&bak), modified(path)), (Some(b), Some(f)) if b > f) {
            return;
        }
        let _ = std::fs::rename(path, bak);
    }

    /// Atomic: writes `settings.json.tmp`, then moves it over `settings.json`. Refused after a load that couldn't
    /// read the file.
    pub fn save(&self, settings: &Settings) -> io::Result<()> {
        if self.unreadable.load(Ordering::Relaxed) {
            return Err(io::Error::other("settings.json couldn't be read, so it isn't overwritten"));
        }
        std::fs::create_dir_all(&self.dir)?;
        let file = SettingsFileOut {
            hotkeys: Hotkeys(&settings.hotkeys),
            day: PresetFile::from(&settings.day),
            night: PresetFile::from(&settings.night),
            this_machine_input: HexByte(settings.this_machine_input),
            start_with_windows: settings.start_with_windows,
            background_mode: settings.background_mode,
            auto_day_night: AutoFileOut::from(settings),
            last_applied_phase: settings.last_applied_phase.map(|p| match p {
                Phase::Day => "Day",
                Phase::Night => "Night",
            }),
        };
        write_atomic(&self.dir.join("settings.json"), &serde_json::to_string_pretty(&file).map_err(io::Error::other)?)
    }
}

/// Writes `<path>.tmp`, then moves it over `path`.
pub(crate) fn write_atomic(path: &Path, contents: &str) -> io::Result<()> {
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    std::fs::write(&temp, contents)?;
    std::fs::rename(&temp, path)
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SettingsFile {
    #[serde(default)]
    hotkeys: Option<BTreeMap<String, String>>,
    #[serde(default = "default_day")]
    day: PresetFile,
    #[serde(default = "default_night")]
    night: PresetFile,
    #[serde(default = "default_input")]
    this_machine_input: HexByte,
    #[serde(default)]
    start_with_windows: bool,
    /// Files saved before 0.0.4 lack it; those users had the tray, so it loads as true.
    #[serde(default = "yes")]
    background_mode: bool,
    /// New in 0.0.5 (0.0.4 ignores it). Missing or malformed → Off; never makes the file corrupt.
    #[serde(default, deserialize_with = "auto_lenient")]
    auto_day_night: AutoLoaded,
    /// New in 0.1.0 (0.0.4 and 0.0.8 ignore it). Missing or malformed → None; never makes the file corrupt.
    #[serde(default, deserialize_with = "phase_lenient")]
    last_applied_phase: Option<Phase>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct SettingsFileOut<'a> {
    hotkeys: Hotkeys<'a>,
    day: PresetFile,
    night: PresetFile,
    this_machine_input: HexByte,
    start_with_windows: bool,
    background_mode: bool,
    auto_day_night: AutoFileOut,
    last_applied_phase: Option<&'static str>,
}

fn default_day() -> PresetFile {
    PresetFile::from(&Settings::default().day)
}

fn default_night() -> PresetFile {
    PresetFile::from(&Settings::default().night)
}

fn default_input() -> HexByte {
    HexByte(0x11)
}

fn yes() -> bool {
    true
}

/// `"AutoDayNight"` as loaded: the active mode plus the remembered times and place.
struct AutoLoaded {
    mode: AutoMode,
    times: (u16, u16),
    location: Option<(f64, f64)>,
}

impl Default for AutoLoaded {
    fn default() -> Self {
        Self { mode: AutoMode::Off, times: DEFAULT_AUTO_TIMES, location: None }
    }
}

/// Reads any JSON: unknown modes, bad times or an invalid place fall back to Off and the defaults.
fn auto_lenient<'de, D: Deserializer<'de>>(d: D) -> Result<AutoLoaded, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    let field = |name: &str| value.get(name);
    let time = |name: &str| field(name).and_then(serde_json::Value::as_str).and_then(parse_hhmm);
    let number = |name: &str| match field(name)? {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.trim().parse().ok(),
        _ => None,
    };
    let (day, night) = (time("Day"), time("Night"));
    let location = match (number("Latitude"), number("Longitude")) {
        (Some(lat), Some(lon)) if valid_place(lat, lon) => Some((lat, lon)),
        _ => None,
    };
    let mode_name = field("Mode").and_then(serde_json::Value::as_str).unwrap_or("").trim().to_ascii_lowercase();
    let mode = match (mode_name.as_str(), day, night, location) {
        ("times", Some(day), Some(night), _) => AutoMode::Times { day, night },
        ("sun", _, _, Some((lat, lon))) => AutoMode::Sun { lat, lon },
        _ => AutoMode::Off,
    };
    let times = (day.unwrap_or(DEFAULT_AUTO_TIMES.0), night.unwrap_or(DEFAULT_AUTO_TIMES.1));
    Ok(AutoLoaded { mode, times, location })
}

/// `"Day"` or `"Night"` (any case); anything else → None.
fn phase_lenient<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Phase>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(match value.as_str().map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("day") => Some(Phase::Day),
        Some("night") => Some(Phase::Night),
        _ => None,
    })
}

fn valid_place(lat: f64, lon: f64) -> bool {
    (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon)
}

/// `"HH:MM"` (24-hour, one or two hour digits) → minutes after midnight.
fn parse_hhmm(s: &str) -> Option<u16> {
    let (h, m) = s.trim().split_once(':')?;
    let digits = |p: &str, max: u16| -> Option<u16> {
        if !(1..=2).contains(&p.len()) || !p.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        p.parse().ok().filter(|v| *v <= max)
    };
    Some(digits(h, 23)? * 60 + digits(m, 59)?)
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct AutoFileOut {
    mode: &'static str,
    day: String,
    night: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
}

impl From<&Settings> for AutoFileOut {
    /// The active mode's values, else the remembered ones.
    fn from(s: &Settings) -> Self {
        let hhmm = |m: u16| format!("{:02}:{:02}", m % 1440 / 60, m % 60);
        let (mode, (day, night), location) = match s.auto_day_night {
            AutoMode::Off => ("Off", s.auto_times, s.auto_location),
            AutoMode::Times { day, night } => ("Times", (day, night), s.auto_location),
            AutoMode::Sun { lat, lon } => ("Sun", s.auto_times, Some((lat, lon))),
        };
        Self { mode, day: hhmm(day), night: hhmm(night), latitude: location.map(|l| l.0), longitude: location.map(|l| l.1) }
    }
}

/// Writes the hotkeys in action order (a JSON map would sort them).
struct Hotkeys<'a>(&'a BTreeMap<Action, String>);

impl Serialize for Hotkeys<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(self.0.len()))?;
        for (action, gesture) in self.0 {
            map.serialize_entry(action.key(), gesture)?;
        }
        map.end()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PresetFile {
    name: String,
    #[serde(default)]
    brightness: u32,
    #[serde(default)]
    contrast: u32,
    #[serde(default)]
    colour_mode: Option<HexByte>,
}

impl From<PresetFile> for Preset {
    fn from(p: PresetFile) -> Self {
        Self { name: p.name, brightness: p.brightness, contrast: p.contrast, colour_mode: p.colour_mode.map(|b| b.0) }
    }
}

impl From<&Preset> for PresetFile {
    fn from(p: &Preset) -> Self {
        Self { name: p.name.clone(), brightness: p.brightness, contrast: p.contrast, colour_mode: p.colour_mode.map(HexByte) }
    }
}

/// Written as `"0xNN"`; read from a number, a `"0x"` hex string or a decimal string.
#[derive(Clone, Copy)]
struct HexByte(u8);

impl Serialize for HexByte {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex(self.0))
    }
}

impl<'de> Deserialize<'de> for HexByte {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let byte = match serde_json::Value::deserialize(d)? {
            serde_json::Value::Number(n) => n.as_u64().and_then(|n| u8::try_from(n).ok()),
            serde_json::Value::String(s) if s.starts_with("0x") || s.starts_with("0X") => parse_hex(&s),
            serde_json::Value::String(s) if s.bytes().all(|b| b.is_ascii_digit()) => s.parse().ok(),
            _ => None,
        };
        byte.map(HexByte).ok_or_else(|| D::Error::custom("invalid byte"))
    }
}

/// `capabilities.txt`: the last capability string the monitor reported, so controls render offline.
#[derive(Clone, Debug)]
pub struct CapabilitiesCache {
    dir: PathBuf,
}

impl CapabilitiesCache {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn load(&self) -> Option<String> {
        std::fs::read_to_string(self.dir.join("capabilities.txt")).ok()
    }

    pub fn save(&self, raw: &str) {
        let result = std::fs::create_dir_all(&self.dir).and_then(|_| write_atomic(&self.dir.join("capabilities.txt"), raw));
        if let Err(e) = result {
            crate::log::write(&format!("Capabilities cache save failed: {e}"));
        }
    }
}

/// How many backups [`BackupStore::save`] keeps.
const KEEP_BACKUPS: usize = 10;

/// Timestamped `backup-yyyyMMdd-HHmmss[-n].json` files of `{"0xNN": value}`.
#[derive(Clone, Debug)]
pub struct BackupStore {
    dir: PathBuf,
}

impl BackupStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Returns the path written. Backups beyond the newest [`KEEP_BACKUPS`] are deleted.
    pub fn save(&self, values: &BTreeMap<u8, u16>) -> io::Result<PathBuf> {
        std::fs::create_dir_all(&self.dir)?;
        let t = crate::clock::now();
        let stem = format!("backup-{:04}{:02}{:02}-{:02}{:02}{:02}", t.year, t.month, t.day, t.hour, t.minute, t.second);
        // After the highest suffix used this second, never a freed lower one: that would sort oldest and be pruned.
        let stamp = &stem["backup-".len()..];
        let last = self.names().iter().filter_map(|n| backup_key(n)).filter(|(s, _)| s == stamp).map(|(_, n)| n).max();
        let path = match last {
            None => self.dir.join(format!("{stem}.json")),
            Some(n) => self.dir.join(format!("{stem}-{}.json", n + 1)),
        };
        let json: serde_json::Map<String, serde_json::Value> = values.iter().map(|(&k, &v)| (hex(k), v.into())).collect();
        write_atomic(&path, &serde_json::to_string_pretty(&json).map_err(io::Error::other)?)?;
        for name in self.names().into_iter().rev().skip(KEEP_BACKUPS) {
            if let Err(e) = std::fs::remove_file(self.dir.join(&name)) {
                crate::log::write(&format!("Couldn't delete old backup {name}: {e}"));
            }
        }
        Ok(path)
    }

    /// The newest backup by stamp, then suffix, falling back to older ones that can be read; `Ok(None)` when there
    /// is none, the newest one's error when none can be read.
    pub fn load_latest(&self) -> io::Result<Option<BTreeMap<u8, u16>>> {
        let mut first_error = None;
        for name in self.names().into_iter().rev() {
            match read_backup(&self.dir.join(&name)) {
                Ok(values) => return Ok(Some(values)),
                Err(e) => {
                    crate::log::write(&format!("Backup {name} can't be read: {e}"));
                    first_error.get_or_insert(e);
                }
            }
        }
        first_error.map_or(Ok(None), Err)
    }

    /// Backup file names, oldest first.
    fn names(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else { return Vec::new() };
        let mut keyed: Vec<_> = entries
            .filter_map(|e| {
                let name = e.ok()?.file_name().into_string().ok()?;
                Some((backup_key(&name)?, name))
            })
            .collect();
        keyed.sort();
        keyed.into_iter().map(|(_, name)| name).collect()
    }
}

fn read_backup(path: &Path) -> io::Result<BTreeMap<u8, u16>> {
    let json: BTreeMap<String, u32> = serde_json::from_str(&std::fs::read_to_string(path)?).map_err(io::Error::other)?;
    json.into_iter()
        .map(|(k, v)| match (parse_hex(&k), u16::try_from(v)) {
            (Some(code), Ok(value)) => Ok((code, value)),
            _ => Err(io::Error::new(io::ErrorKind::InvalidData, format!("Bad backup entry {k}."))),
        })
        .collect()
}

/// (stamp, suffix) for `backup-yyyyMMdd-HHmmss[-n].json`, else None.
fn backup_key(name: &str) -> Option<(String, u32)> {
    let rest = name.strip_prefix("backup-")?.strip_suffix(".json")?;
    let (stamp, suffix) = rest.split_at_checked(15)?;
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    let stamp_ok = stamp.as_bytes()[8] == b'-' && digits(&stamp[..8]) && digits(&stamp[9..]);
    let suffix = match suffix.strip_prefix('-') {
        None if suffix.is_empty() => 0,
        Some(n) if !n.is_empty() && digits(n) => n.parse().ok()?,
        _ => return None,
    };
    stamp_ok.then(|| (stamp.to_string(), suffix))
}
