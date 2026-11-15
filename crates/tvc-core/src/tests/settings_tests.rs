//! SettingsStoreTests, HotkeyGestureTests and BackupStoreTests.

use std::collections::BTreeMap;

use crate::hotkey::{Action, Gesture};
use crate::profile::{ControlDefinition, ControlKind, ProfileStore, SafetyClass, Section};
use crate::schedule::{AutoMode, Phase};
use crate::identity::{MonitorId, MonitorIdentity};
use crate::store::{
    BackupStore, CapabilitiesCache, HotkeyTarget, MonitorSettings, Preset, Settings, SettingsStore, DEFAULT_AUTO_TIMES,
};

use super::TempDir;

#[test]
fn missing_file_gives_defaults() {
    let dir = TempDir::new();
    let s = SettingsStore::new(dir.path()).load();

    assert_eq!(s.hotkeys[&Action::BrightnessUp], Action::BrightnessUp.default_gesture());
    assert_eq!(s.hotkeys[&Action::BrightnessDown], Action::BrightnessDown.default_gesture());
    assert_eq!(s.hotkeys[&Action::ContrastUp], Action::ContrastUp.default_gesture());
    assert_eq!(s.hotkeys[&Action::ContrastDown], Action::ContrastDown.default_gesture());
    assert_eq!(s.hotkeys.len(), Action::ALL.len());
    assert_eq!(s.night, Preset { name: "Night".into(), brightness: 0, contrast: 70, colour_mode: None });
    assert_eq!(s.day, Preset { name: "Day".into(), brightness: 60, contrast: 70, colour_mode: None });
    assert_eq!(s.this_machine_input, 0x11);
    assert!(!s.background_mode); // new installs exit on close
    assert!(!s.start_with_windows);
    assert_eq!(s.auto_day_night, AutoMode::Off);
    assert_eq!(s.auto_times, (7 * 60, 19 * 60));
    assert_eq!(s.auto_location, None);
}

#[test]
fn corrupt_file_gives_defaults_and_is_kept_as_bak() {
    let dir = TempDir::new();
    std::fs::write(dir.path().join("settings.json"), "{ broken").unwrap();

    let s = SettingsStore::new(dir.path()).load();

    assert_eq!(s, Settings::default());
    assert!(dir.path().join("settings.json.bak").exists());
    assert!(!dir.path().join("settings.json").exists());

    let null_presets = r#"{ "Day": null, "Night": null }"#;
    std::fs::write(dir.path().join("settings.json"), null_presets).unwrap();
    assert_eq!(SettingsStore::new(dir.path()).load(), Settings::default());
    assert_eq!(std::fs::read_to_string(dir.path().join("settings.json.bak")).unwrap(), null_presets);
}

#[test]
fn save_then_load_round_trips() {
    let dir = TempDir::new();
    let store = SettingsStore::new(dir.path());
    let mut saved = Settings::default();
    saved.hotkeys.insert(Action::SwitchToMe, "Ctrl+Shift+Win+K".into());
    saved.hotkeys.insert(Action::OpenPanel, String::new());
    saved.this_machine_input = 0x31;
    saved.start_with_windows = true;
    saved.background_mode = false;
    saved.day = Preset { name: "Day".into(), brightness: 55, contrast: 65, colour_mode: Some(0x01) };

    store.save(&saved).unwrap();

    assert_eq!(store.load(), saved);
    assert!(!dir.path().join("settings.json.tmp").exists());
}

#[test]
fn file_format_matches_0_0_4() {
    let dir = TempDir::new();
    let mut settings = Settings::default();
    settings.night.colour_mode = Some(0x12);
    SettingsStore::new(dir.path()).save(&settings).unwrap();
    let text = std::fs::read_to_string(dir.path().join("settings.json")).unwrap();

    let keys = ["\"Hotkeys\"", "\"BrightnessUp\"", "\"OpenPanel\"", "\"Day\"", "\"Night\"", "\"ThisMachineInput\"",
        "\"StartWithWindows\"", "\"BackgroundMode\""];
    let order: Vec<usize> = keys.iter().map(|k| text.find(k).unwrap()).collect();
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{text}");
    assert!(text.contains(r#""ThisMachineInput": "0x11""#));
    assert!(text.contains(r#""ColourMode": "0x12""#));
    assert!(text.contains(r#""ColourMode": null"#));
    assert!(text.contains(r#""Name": "Day""#));
}

#[test]
fn bytes_read_from_numbers_hex_or_decimal_strings() {
    let dir = TempDir::new();
    let preset = |mode: &str| format!(r#"{{ "Name": "Day", "Brightness": 1, "Contrast": 2, "ColourMode": {mode} }}"#);
    let json = format!(r#"{{ "Day": {}, "Night": {}, "ThisMachineInput": 15 }}"#, preset("\"0x0B\""), preset("\"18\""));
    std::fs::write(dir.path().join("settings.json"), json).unwrap();

    let s = SettingsStore::new(dir.path()).load();

    assert_eq!((s.this_machine_input, s.day.colour_mode, s.night.colour_mode), (0x0F, Some(0x0B), Some(18)));
}

#[test]
fn existing_settings_without_background_mode_default_to_background() {
    let dir = TempDir::new();
    let store = SettingsStore::new(dir.path());
    store.save(&Settings::default()).unwrap();
    let path = dir.path().join("settings.json");
    let mut json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    json.as_object_mut().unwrap().remove("BackgroundMode");
    std::fs::write(&path, json.to_string()).unwrap();

    assert!(store.load().background_mode);
}

#[test]
fn replaces_legacy_default_slider_shortcuts_but_preserves_custom_shortcuts() {
    let dir = TempDir::new();
    let store = SettingsStore::new(dir.path());
    let mut settings = Settings::default();
    for (action, gesture) in [(Action::BrightnessUp, "Ctrl+Alt+PageUp"), (Action::BrightnessDown, "Ctrl+Alt+PageDown"),
        (Action::ContrastUp, "Ctrl+Alt+Shift+PageUp"), (Action::ContrastDown, "Ctrl+Alt+Shift+PageDown"),
        (Action::DayPreset, "Ctrl+Shift+D")] {
        settings.hotkeys.insert(action, gesture.into());
    }
    settings.hotkeys.remove(&Action::OpenPanel);
    store.save(&settings).unwrap();

    let loaded = store.load();

    assert_eq!(loaded.hotkeys[&Action::BrightnessUp], Action::BrightnessUp.default_gesture());
    assert_eq!(loaded.hotkeys[&Action::BrightnessDown], Action::BrightnessDown.default_gesture());
    assert_eq!(loaded.hotkeys[&Action::ContrastUp], Action::ContrastUp.default_gesture());
    assert_eq!(loaded.hotkeys[&Action::ContrastDown], Action::ContrastDown.default_gesture());
    assert_eq!(loaded.hotkeys[&Action::DayPreset], "Ctrl+Shift+D");
    assert_eq!(loaded.hotkeys[&Action::OpenPanel], "Ctrl+Alt+O"); // a missing action gets its default
}

#[test]
fn gestures_parse_to_win32_codes() {
    const C: u8 = Gesture::MOD_CONTROL;
    const A: u8 = Gesture::MOD_ALT;
    let cases = [("Ctrl+Alt+PageUp", C | A, 0x21), ("ctrl + alt + shift + pagedown", C | A | Gesture::MOD_SHIFT, 0x22),
        ("Win+Ctrl+F3", C | Gesture::MOD_WIN, 0x72), ("Ctrl+Alt+M", C | A, 0x4D), ("Ctrl+Alt+Up", C | A, 0x26),
        ("Ctrl+Alt+W", C | A, 0x57), ("Control+F24", C, 0x87), ("Alt+pgdn", A, 0x22), ("Ctrl+7", C, 0x37)];
    for (text, mods, vk) in cases {
        assert_eq!(Gesture::parse(text), Some(Gesture { mods, vk }), "{text}");
    }
}

#[test]
fn gestures_reject_invalid() {
    for text in ["PageUp", "Ctrl+Alt", "Ctrl+A+B", "Ctrl+Banana", "", "Ctrl+F25", "Ctrl+F0", "Ctrl+F01", "Ctrl+"] {
        assert_eq!(Gesture::parse(text), None, "{text}");
    }
}

#[test]
fn gestures_create_only_supported_modified_keys() {
    let ca = Gesture::MOD_CONTROL | Gesture::MOD_ALT;
    assert!(Gesture::try_create(ca, 0x26).is_some());
    assert!(Gesture::try_create(ca, 0x57).is_some());
    assert!(Gesture::try_create(0, 0x26).is_none());
    assert!(Gesture::try_create(Gesture::MOD_CONTROL, 0x00).is_none());
    assert!(Gesture::try_create(0x10, 0x26).is_none());
}

#[test]
fn gestures_format_canonically() {
    assert_eq!(Gesture::parse("shift+win+alt+ctrl+pgup").unwrap().to_string(), "Ctrl+Alt+Shift+Win+PageUp");
    assert_eq!(Gesture { mods: Gesture::MOD_ALT, vk: 0x13 }.to_string(), "Alt+0x13");
}

#[test]
fn all_default_hotkeys_round_trip() {
    for action in Action::ALL {
        let text = action.default_gesture();
        assert_eq!(Gesture::parse(text).map(|g| g.to_string()).as_deref(), Some(text));
        assert_eq!(Action::from_key(action.key()), Some(action));
    }
    assert_eq!(Action::OpenPanel.label(), "Open ThinkVision Control");
    assert_eq!((Action::BrightnessUp.id(), Action::OpenPanel.id()), (1, 11));
}

#[test]
fn latest_backup_wins_including_same_second_suffix() {
    let dir = TempDir::new();
    let store = BackupStore::new(dir.path());

    let paths: Vec<_> = (1..=3).map(|v| store.save(&BTreeMap::from([(0x10, v)])).unwrap()).collect();

    assert_eq!(store.load_latest().unwrap().unwrap()[&0x10], 3);
    let name = paths[0].file_name().unwrap().to_str().unwrap().to_string();
    assert!(name.starts_with("backup-") && name.ends_with(".json") && name.len() == "backup-yyyyMMdd-HHmmss.json".len());
}

#[test]
fn latest_backup_orders_by_stamp_then_numeric_suffix() {
    let dir = TempDir::new();
    for (name, v) in [("backup-20261028-100000-9.json", 1), ("backup-20261028-100000-10.json", 2),
        ("backup-20261027-235959-99.json", 3), ("backup-notes.json", 4)] {
        std::fs::write(dir.path().join(name), format!(r#"{{ "0x10": {v} }}"#)).unwrap();
    }
    assert_eq!(BackupStore::new(dir.path()).load_latest().unwrap().unwrap()[&0x10], 2);
}

#[test]
fn no_backup_returns_none() {
    let dir = TempDir::new();
    assert!(BackupStore::new(dir.path()).load_latest().unwrap().is_none());
    assert!(BackupStore::new(dir.path().join("missing")).load_latest().unwrap().is_none());
}

#[test]
fn capabilities_cache_round_trips() {
    let dir = TempDir::new();
    let cache = CapabilitiesCache::new(dir.path().join("sub"));
    assert_eq!(cache.load(), None);
    cache.save("(vcp(10))");
    assert_eq!(cache.load().as_deref(), Some("(vcp(10))"));
}

/// Writes `settings.json` with the default settings plus the given `"AutoDayNight"` JSON, then loads it.
fn load_with_auto(auto: Option<&str>) -> (TempDir, Settings) {
    let dir = TempDir::new();
    let store = SettingsStore::new(dir.path());
    store.save(&Settings::default()).unwrap();
    let path = dir.path().join("settings.json");
    let mut json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let object = json.as_object_mut().unwrap();
    object.remove("AutoDayNight");
    if let Some(auto) = auto {
        object.insert("AutoDayNight".into(), serde_json::from_str(auto).unwrap());
    }
    std::fs::write(&path, json.to_string()).unwrap();
    let loaded = store.load();
    (dir, loaded)
}

#[test]
fn auto_day_night_round_trips_in_every_mode() {
    let modes = [
        (AutoMode::Off, (6 * 60 + 30, 20 * 60), Some((31.5, 74.25))),
        (AutoMode::Times { day: 6 * 60 + 30, night: 60 }, (6 * 60 + 30, 60), None),
        (AutoMode::Sun { lat: -33.875, lon: 151.25 }, DEFAULT_AUTO_TIMES, Some((-33.875, 151.25))),
    ];
    for (mode, times, location) in modes {
        let dir = TempDir::new();
        let store = SettingsStore::new(dir.path());
        let saved = Settings { auto_day_night: mode, auto_times: times, auto_location: location, ..Settings::default() };
        store.save(&saved).unwrap();
        assert_eq!(store.load(), saved, "{mode:?}");
    }
}

#[test]
fn auto_day_night_file_format() {
    let dir = TempDir::new();
    let settings = Settings { auto_day_night: AutoMode::Sun { lat: 31.5, lon: 74.25 }, ..Settings::default() };
    SettingsStore::new(dir.path()).save(&settings).unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("settings.json")).unwrap()).unwrap();

    assert_eq!(json["AutoDayNight"], serde_json::json!({
        "Mode": "Sun", "Day": "07:00", "Night": "19:00", "Latitude": 31.5, "Longitude": 74.25
    }));
    let off = Settings::default();
    SettingsStore::new(dir.path()).save(&off).unwrap();
    let text = std::fs::read_to_string(dir.path().join("settings.json")).unwrap();
    assert!(text.contains(r#""Mode": "Off""#) && text.contains(r#""Latitude": null"#), "{text}");
}

#[test]
fn a_0_0_4_file_without_auto_day_night_loads_as_off() {
    let (_dir, s) = load_with_auto(None);
    assert_eq!((s.auto_day_night, s.auto_times, s.auto_location), (AutoMode::Off, DEFAULT_AUTO_TIMES, None));
}

#[test]
fn malformed_auto_day_night_is_off_and_never_corrupt() {
    for auto in ["null", "42", r#""Sun""#, "[]", r#"{"Mode":"Weekly"}"#, r#"{"Mode":"Times","Day":"7am","Night":"19:00"}"#,
        r#"{"Mode":"Times","Day":"24:00","Night":"19:00"}"#, r#"{"Mode":"Times","Night":"19:00"}"#,
        r#"{"Mode":"Sun","Latitude":95,"Longitude":10}"#, r#"{"Mode":"Sun","Latitude":"north","Longitude":10}"#,
        r#"{"Mode":"Sun","Latitude":31.5}"#, r#"{"Mode":7}"#] {
        let (dir, s) = load_with_auto(Some(auto));
        assert_eq!(s.auto_day_night, AutoMode::Off, "{auto}");
        assert!(!dir.path().join("settings.json.bak").exists(), "{auto}");
        assert_eq!(s.day, Settings::default().day, "{auto}");
    }
}

#[test]
fn auto_day_night_reads_leniently_and_remembers_values() {
    let (_dir, s) = load_with_auto(Some(r#"{"Mode":"times","Day":"6:05","Night":" 23:59 "}"#));
    assert_eq!(s.auto_day_night, AutoMode::Times { day: 6 * 60 + 5, night: 23 * 60 + 59 });

    let (_dir, s) = load_with_auto(Some(r#"{"Mode":"SUN","Latitude":"31.5","Longitude":-0.25}"#));
    assert_eq!(s.auto_day_night, AutoMode::Sun { lat: 31.5, lon: -0.25 });
    assert_eq!(s.auto_times, DEFAULT_AUTO_TIMES);

    // Off keeps what was typed for the settings UI.
    let (_dir, s) = load_with_auto(Some(r#"{"Mode":"Off","Day":"08:15","Night":"21:00","Latitude":51.5,"Longitude":0}"#));
    assert_eq!((s.auto_day_night, s.auto_times, s.auto_location), (AutoMode::Off, (8 * 60 + 15, 21 * 60), Some((51.5, 0.0))));
}

/// Writes `settings.json` with the default settings plus `"key": value`, then loads it.
fn load_with_field(key: &str, value: &str) -> (TempDir, Settings) {
    let dir = TempDir::new();
    let store = SettingsStore::new(dir.path());
    store.save(&Settings::default()).unwrap();
    let path = dir.path().join("settings.json");
    let mut json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    json.as_object_mut().unwrap().insert(key.into(), serde_json::from_str(value).unwrap());
    std::fs::write(&path, json.to_string()).unwrap();
    let loaded = store.load();
    (dir, loaded)
}

fn file_names(dir: &TempDir) -> Vec<String> {
    std::fs::read_dir(dir.path()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()
}

#[test]
fn settings_with_bom_loads() {
    let dir = TempDir::new();
    std::fs::write(dir.path().join("settings.json"), "\u{feff}{\"ThisMachineInput\":\"0x0F\",\"StartWithWindows\":true}")
        .unwrap();

    let s = SettingsStore::new(dir.path()).load();

    assert_eq!((s.this_machine_input, s.start_with_windows), (0x0F, true));
    assert!(!dir.path().join("settings.json.bak").exists());
}

#[test]
fn missing_day_or_night_takes_default_and_keeps_other_fields() {
    let mine = Preset { name: "Mine".into(), brightness: 5, contrast: 50, colour_mode: None };
    for kept in ["Night", "Day"] {
        let dir = TempDir::new();
        let json = format!(
            r#"{{"{kept}":{{"Name":"Mine","Brightness":5,"Contrast":50}},"ThisMachineInput":"0x0F","StartWithWindows":true}}"#);
        std::fs::write(dir.path().join("settings.json"), json).unwrap();

        let s = SettingsStore::new(dir.path()).load();

        let defaults = Settings::default();
        let expected = if kept == "Night" { (defaults.day, mine.clone()) } else { (mine.clone(), defaults.night) };
        assert_eq!((s.day, s.night), expected, "{kept}");
        assert_eq!((s.this_machine_input, s.start_with_windows), (0x0F, true), "{kept}");
        assert!(!dir.path().join("settings.json.bak").exists(), "{kept}");
    }
}

#[test]
fn parse_failure_keeps_newer_bak() {
    let dir = TempDir::new();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, "{ broken").unwrap();
    let hour_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    std::fs::File::options().write(true).open(&path).unwrap().set_modified(hour_ago).unwrap();
    std::fs::write(dir.path().join("settings.json.bak"), "NEWER").unwrap();

    assert_eq!(SettingsStore::new(dir.path()).load(), Settings::default());

    assert_eq!(std::fs::read_to_string(dir.path().join("settings.json.bak")).unwrap(), "NEWER");
    assert!(path.exists());
}

#[test]
fn unreadable_settings_runs_on_defaults_and_refuses_save() {
    let dir = TempDir::new();
    std::fs::create_dir(dir.path().join("settings.json")).unwrap();
    let store = SettingsStore::new(dir.path());

    assert_eq!(store.load(), Settings::default());

    assert!(store.save(&Settings::default()).is_err());
    assert!(dir.path().join("settings.json").is_dir());
}

#[test]
fn last_applied_phase_round_trips() {
    for (phase, text) in [(Some(Phase::Day), r#""LastAppliedPhase": "Day""#),
        (Some(Phase::Night), r#""LastAppliedPhase": "Night""#), (None, r#""LastAppliedPhase": null"#)] {
        let dir = TempDir::new();
        let store = SettingsStore::new(dir.path());
        let saved = Settings { last_applied_phase: phase, ..Settings::default() };
        store.save(&saved).unwrap();

        assert_eq!(store.load(), saved, "{phase:?}");
        let file = std::fs::read_to_string(dir.path().join("settings.json")).unwrap();
        assert!(file.contains(text), "{file}");
    }
}

#[test]
fn malformed_last_applied_phase_is_none_and_never_corrupt() {
    for value in ["5", r#""Dusk""#, "{}"] {
        let (dir, s) = load_with_field("LastAppliedPhase", value);
        assert_eq!(s.last_applied_phase, None, "{value}");
        assert_eq!(s.day, Settings::default().day, "{value}");
        assert!(!dir.path().join("settings.json.bak").exists(), "{value}");
    }
}

#[test]
fn overlay_bad_entry_is_skipped() {
    let dir = TempDir::new();
    std::fs::write(dir.path().join("profile.user.json"), r#"{"edidId":"LEN62A2","controls":[
        {"code":"0xF7","name":"Good","kind":"Enum","section":"Advanced","safety":"Normal","values":{"0x04":"x"}},
        {"code":"0xZZ","name":"Bad code","kind":"Enum","section":"Advanced","safety":"Normal"},
        {"code":"0xF8","name":"Bad kind","kind":"Bogus","section":"Advanced","safety":"Normal"}]}"#).unwrap();
    let caps = crate::mccs::parse(crate::fake::P32P20_CAPABILITIES).unwrap();

    let defs = ProfileStore::new(dir.path()).resolve(&caps);

    assert_eq!(defs[&0xF7].name, "Good");
    assert_eq!(defs[&0xF8].name, "Unknown 0xF8");
}

/// The 0x10 value of every backup file in `dir`, sorted.
fn backup_values(dir: &TempDir) -> Vec<u16> {
    let mut values: Vec<u16> = file_names(dir).iter().filter(|n| n.starts_with("backup-")).map(|n| {
        let text = std::fs::read_to_string(dir.path().join(n)).unwrap();
        serde_json::from_str::<BTreeMap<String, u16>>(&text).unwrap()["0x10"]
    }).collect();
    values.sort();
    values
}

#[test]
fn backups_pruned_to_newest_10() {
    let dir = TempDir::new();
    for v in 1..=11 {
        std::fs::write(dir.path().join(format!("backup-202601{v:02}-100000.json")), format!(r#"{{ "0x10": {v} }}"#)).unwrap();
    }
    let store = BackupStore::new(dir.path());

    store.save(&BTreeMap::from([(0x10, 12)])).unwrap();

    assert_eq!(backup_values(&dir), (3..=12).collect::<Vec<_>>());
    assert_eq!(store.load_latest().unwrap().unwrap()[&0x10], 12);
}

#[test]
fn same_second_backups_are_pruned_oldest_first() {
    let dir = TempDir::new();
    let store = BackupStore::new(dir.path());

    for v in 1..=12 {
        store.save(&BTreeMap::from([(0x10, v)])).unwrap();
    }

    assert_eq!(backup_values(&dir), (3..=12).collect::<Vec<_>>());
    assert_eq!(store.load_latest().unwrap().unwrap()[&0x10], 12);
}

#[test]
fn load_latest_falls_back_when_newest_corrupt() {
    let dir = TempDir::new();
    std::fs::write(dir.path().join("backup-20261028-100000.json"), r#"{ "0x10": 1 }"#).unwrap();
    std::fs::write(dir.path().join("backup-20261029-100000.json"), "{ broken").unwrap();
    let store = BackupStore::new(dir.path());

    assert_eq!(store.load_latest().unwrap().unwrap()[&0x10], 1);

    std::fs::write(dir.path().join("backup-20261028-100000.json"), r#"{ "0xZZ": 1 }"#).unwrap();
    assert!(store.load_latest().is_err());
}

#[test]
fn stores_leave_no_tmp_files() {
    let dir = TempDir::new();
    SettingsStore::new(dir.path()).save(&Settings::default()).unwrap();
    CapabilitiesCache::new(dir.path()).save("(vcp(10))");
    BackupStore::new(dir.path()).save(&BTreeMap::from([(0x10, 1)])).unwrap();
    let eco = ControlDefinition::new(0xEB, "Eco", ControlKind::Enum, Section::Advanced, SafetyClass::Normal, BTreeMap::new());
    ProfileStore::new(dir.path()).save_overlay(&eco).unwrap();

    let names = file_names(&dir);
    assert_eq!(names.len(), 4, "{names:?}");
    assert!(!names.iter().any(|n| n.ends_with(".tmp")), "{names:?}");
}

#[test]
#[cfg(target_os = "linux")]
fn linux_old_default_shortcuts_migrate() {
    let dir = TempDir::new();
    let store = SettingsStore::new(dir.path());
    let mut settings = Settings::default();
    settings.hotkeys.insert(Action::InputHdmi, "Ctrl+Alt+F1".into());
    settings.hotkeys.insert(Action::InputDp, "Ctrl+Shift+Win+D".into());
    store.save(&settings).unwrap();

    let loaded = store.load();

    assert_eq!(loaded.hotkeys[&Action::InputHdmi], "Ctrl+Win+1");
    assert_eq!(loaded.hotkeys[&Action::InputDp], "Ctrl+Shift+Win+D");
}

fn id(s: &str) -> MonitorId {
    MonitorId(s.into())
}

fn len62a2() -> MonitorIdentity {
    MonitorIdentity { edid_id: "LEN62A2".into(), serial: Some("V1".into()), connector: None }
}

/// A 0.0.8 file: no `"LegacyAdopted"`, an input and preset colour modes set for the P32p-20.
fn legacy_settings() -> Settings {
    let dir = TempDir::new();
    let mut s = Settings { this_machine_input: 0x0F, ..Settings::default() };
    s.day.colour_mode = Some(0x0B);
    s.night.colour_mode = Some(0x08);
    SettingsStore::new(dir.path()).save(&s).unwrap();
    let path = dir.path().join("settings.json");
    let mut json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    json.as_object_mut().unwrap().remove("LegacyAdopted");
    std::fs::write(&path, json.to_string()).unwrap();
    SettingsStore::new(dir.path()).load()
}

#[test]
fn monitor_inherits_without_colour_mode() {
    let mut s = Settings { this_machine_input: 0x31, ..Settings::default() };
    s.day = Preset { name: "Day".into(), brightness: 55, contrast: 65, colour_mode: Some(0x0B) };

    let m = s.monitor(&id("DEL4321-SX"));

    assert_eq!(m, MonitorSettings {
        name: None,
        this_machine_input: 0x31,
        day: Preset { name: "Day".into(), brightness: 55, contrast: 65, colour_mode: None },
        night: s.night.clone(),
        included: true,
        offset: 0,
        last_applied_phase: None,
    });
}

#[test]
fn monitors_round_trip() {
    let dir = TempDir::new();
    let store = SettingsStore::new(dir.path());
    let mut saved = Settings { hotkey_target: HotkeyTarget::UnderPointer, legacy_adopted: false, ..Settings::default() };
    let entry = MonitorSettings {
        name: Some("Left".into()),
        this_machine_input: 0x0F,
        day: Preset { name: "Day".into(), brightness: 80, contrast: 60, colour_mode: Some(0x0B) },
        night: Preset { name: "Night".into(), brightness: 10, contrast: 50, colour_mode: None },
        included: false,
        offset: -15,
        last_applied_phase: Some(Phase::Night),
    };
    saved.monitors.insert(id("LEN62A2-SV1"), entry.clone());
    saved.monitors.insert(id("DEL4321-PDP-2"), saved.monitor(&id("DEL4321-PDP-2")));
    let mut with_unstable = saved.clone();
    with_unstable.monitors.insert(id("ACR0001-N1"), entry);
    store.save(&with_unstable).unwrap();

    let loaded = store.load();

    assert_eq!(loaded, saved);
    let text = std::fs::read_to_string(dir.path().join("settings.json")).unwrap();
    assert!(!text.contains("ACR0001-N1"), "{text}");
    assert!(text.contains(r#""HotkeyTarget": "UnderPointer""#) && text.contains(r#""LegacyAdopted": false"#), "{text}");
}

#[test]
fn malformed_monitor_entry_is_skipped_never_corrupt() {
    for value in ["42", "null", "[]", r#""LEN62A2-SV1""#] {
        let (dir, s) = load_with_field("Monitors", value);
        assert!(s.monitors.is_empty(), "{value}");
        assert_eq!(s.day, Settings::default().day, "{value}");
        assert!(!dir.path().join("settings.json.bak").exists(), "{value}");
    }
    let (dir, s) = load_with_field("Monitors",
        r#"{ "LEN62A2-SV1": 42, "DEL4321-SX": { "Offset": "far" }, "": {}, "DEL4321-SY": { "Offset": 5 } }"#);
    assert_eq!(s.monitors.keys().cloned().collect::<Vec<_>>(), [id("DEL4321-SY")]);
    assert_eq!(s.monitors[&id("DEL4321-SY")].offset, 5);
    assert!(!dir.path().join("settings.json.bak").exists());
}

#[test]
fn partial_monitor_entry_inherits_missing_fields() {
    let dir = TempDir::new();
    let json = r#"{ "ThisMachineInput": "0x31",
        "Day": { "Name": "Day", "Brightness": 33, "Contrast": 44, "ColourMode": "0x0B" },
        "Monitors": { "DEL4321-SX": { "Name": "Right", "Offset": 7, "LastAppliedPhase": "day" } } }"#;
    std::fs::write(dir.path().join("settings.json"), json).unwrap();

    let s = SettingsStore::new(dir.path()).load();

    let m = &s.monitors[&id("DEL4321-SX")];
    assert_eq!((m.name.as_deref(), m.offset, m.last_applied_phase), (Some("Right"), 7, Some(Phase::Day)));
    assert_eq!((m.this_machine_input, m.included), (0x31, true));
    assert_eq!(m.day, Preset { name: "Day".into(), brightness: 33, contrast: 44, colour_mode: None });
    assert_eq!(m.night, Settings::default().night);
}

#[test]
fn a_partial_monitor_preset_inherits_its_missing_fields() {
    let dir = TempDir::new();
    let json = r#"{ "Day": { "Name": "Day", "Brightness": 33, "Contrast": 44 },
        "Monitors": { "DEL4321-SX": { "Day": { "Brightness": 80 } } } }"#;
    std::fs::write(dir.path().join("settings.json"), json).unwrap();

    let s = SettingsStore::new(dir.path()).load();

    assert_eq!(s.monitors[&id("DEL4321-SX")].day,
        Preset { name: "Day".into(), brightness: 80, contrast: 44, colour_mode: None });
}

#[test]
fn hotkey_target_lenient() {
    let cases = [(r#""UnderPointer""#, HotkeyTarget::UnderPointer), (r#"" underpointer ""#, HotkeyTarget::UnderPointer),
        (r#""All""#, HotkeyTarget::All), (r#""Mouse""#, HotkeyTarget::All), ("3", HotkeyTarget::All),
        ("null", HotkeyTarget::All)];
    for (value, expected) in cases {
        let (dir, s) = load_with_field("HotkeyTarget", value);
        assert_eq!(s.hotkey_target, expected, "{value}");
        assert!(!dir.path().join("settings.json.bak").exists(), "{value}");
    }
}

#[test]
fn adopt_legacy_once_into_first_len62a2() {
    let mut s = legacy_settings();
    assert!(!s.legacy_adopted);

    assert!(s.adopt_legacy(&id("LEN62A2-SV1"), &len62a2()));

    let m = &s.monitors[&id("LEN62A2-SV1")];
    assert_eq!((m.this_machine_input, m.day.colour_mode, m.night.colour_mode), (0x0F, Some(0x0B), Some(0x08)));
    assert!(s.legacy_adopted);
    assert_eq!((s.this_machine_input, s.day.colour_mode), (0x0F, Some(0x0B))); // the app-wide fields stay
    let second = MonitorIdentity { serial: Some("V2".into()), ..len62a2() };
    assert!(!s.adopt_legacy(&id("LEN62A2-SV2"), &second));
    assert!(!s.monitors.contains_key(&id("LEN62A2-SV2")));
}

#[test]
fn adopt_legacy_keeps_the_rest_of_an_existing_entry() {
    let mut s = legacy_settings();
    let mut entry = s.monitor(&id("LEN62A2-SV1"));
    entry.name = Some("Desk".into());
    entry.offset = 4;
    s.monitors.insert(id("LEN62A2-SV1"), entry);

    assert!(s.adopt_legacy(&id("LEN62A2-SV1"), &len62a2()));

    let m = &s.monitors[&id("LEN62A2-SV1")];
    assert_eq!((m.name.as_deref(), m.offset, m.day.colour_mode), (Some("Desk"), 4, Some(0x0B)));
}

#[test]
fn adopt_legacy_skips_unstable_and_other_models() {
    let mut s = legacy_settings();
    let unstable = MonitorIdentity { serial: None, ..len62a2() };
    assert!(!s.adopt_legacy(&id("LEN62A2-N1"), &unstable));
    let dell = MonitorIdentity { edid_id: "DEL4321".into(), ..len62a2() };
    assert!(!s.adopt_legacy(&id("DEL4321-SV1"), &dell));
    let lenovo = MonitorIdentity { edid_id: "LEN66B1".into(), ..len62a2() };
    assert!(!s.adopt_legacy(&id("LEN66B1-SV1"), &lenovo));

    assert!(s.monitors.is_empty());
    assert!(!s.legacy_adopted);
    assert!(s.adopt_legacy(&id("LEN62A2-PDP-1"), &len62a2())); // still waiting for a stable LEN62A2
}

#[test]
fn new_install_needs_no_adoption() {
    let dir = TempDir::new();
    let store = SettingsStore::new(dir.path());
    assert!(store.load().legacy_adopted);
    store.save(&Settings::default()).unwrap();

    let mut s = store.load();

    assert!(s.legacy_adopted);
    assert!(!s.adopt_legacy(&id("LEN62A2-SV1"), &len62a2()));
    assert!(s.monitors.is_empty());
}

#[test]
fn language_round_trips_and_unknown_follows_the_system() {
    let dir = TempDir::new();
    let store = SettingsStore::new(dir.path());
    assert_eq!(store.load().language, None);
    let settings = Settings { language: Some("de".into()), ..Settings::default() };
    store.save(&settings).unwrap();
    assert_eq!(store.load().language.as_deref(), Some("de"));
    for (value, expected) in [(r#""System""#, None), (r#""klingon""#, None), ("7", None), (r#""zh_CN""#, Some("zh_CN"))] {
        let (_dir, s) = load_with_field("Language", value);
        assert_eq!(s.language.as_deref(), expected, "{value}");
    }
}
