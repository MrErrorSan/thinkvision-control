//! SettingsStoreTests, HotkeyGestureTests and BackupStoreTests.

use std::collections::BTreeMap;

use crate::hotkey::{Action, Gesture};
use crate::schedule::AutoMode;
use crate::store::{BackupStore, CapabilitiesCache, Preset, Settings, SettingsStore, DEFAULT_AUTO_TIMES};

use super::TempDir;

#[test]
fn missing_file_gives_defaults() {
    let dir = TempDir::new();
    let s = SettingsStore::new(dir.path()).load();

    assert_eq!(s.hotkeys[&Action::BrightnessUp], "Ctrl+Alt+Up");
    assert_eq!(s.hotkeys[&Action::BrightnessDown], "Ctrl+Alt+Down");
    assert_eq!(s.hotkeys[&Action::ContrastUp], "Ctrl+Alt+Left");
    assert_eq!(s.hotkeys[&Action::ContrastDown], "Ctrl+Alt+Right");
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

    assert_eq!(loaded.hotkeys[&Action::BrightnessUp], "Ctrl+Alt+Up");
    assert_eq!(loaded.hotkeys[&Action::BrightnessDown], "Ctrl+Alt+Down");
    assert_eq!(loaded.hotkeys[&Action::ContrastUp], "Ctrl+Alt+Left");
    assert_eq!(loaded.hotkeys[&Action::ContrastDown], "Ctrl+Alt+Right");
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
