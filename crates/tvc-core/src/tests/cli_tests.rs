//! CommandLineTests.

use crate::cli::{exit_code, format, help, parse, parse_url, version_line, CliCommand, GetTarget, Parsed};
use crate::hotkey::{Action, Gesture};
use crate::result::ResultKind;

fn command(args: &[&str]) -> CliCommand {
    parse(args).unwrap().command
}

#[test]
fn format_round_trips_through_parse() {
    let commands = [
        CliCommand::Preset { night: false }, CliCommand::Preset { night: true },
        CliCommand::Input(Some(0x11)), CliCommand::Input(Some(0x0F)), CliCommand::Input(Some(0x31)), CliCommand::Input(None),
        CliCommand::Step { code: 0x10, delta: 5 }, CliCommand::Step { code: 0x12, delta: -5 },
        CliCommand::Get(GetTarget::Brightness), CliCommand::Get(GetTarget::Contrast), CliCommand::Get(GetTarget::Input),
        CliCommand::Get(GetTarget::ColourMode), CliCommand::Get(GetTarget::All), CliCommand::Help,
    ];
    for c in commands {
        assert_eq!(parse(&format(&c).unwrap()).unwrap().command, c, "{c:?}");
    }
}

#[test]
fn canonical_format() {
    assert_eq!(format(&CliCommand::Input(Some(0x0F))).unwrap(), ["--input", "dp"]);
    assert_eq!(format(&CliCommand::Input(None)).unwrap(), ["--input", "this"]);
    assert_eq!(format(&CliCommand::Step { code: 0x10, delta: 5 }).unwrap(), ["--step", "brightness=+5"]);
    assert_eq!(format(&CliCommand::Step { code: 0x12, delta: 0 }).unwrap(), ["--step", "contrast=+0"]);
    assert_eq!(format(&CliCommand::Show).unwrap(), Vec::<String>::new());
    assert!(format(&CliCommand::Step { code: 0x10, delta: 500 }).is_none());
    assert!(format(&CliCommand::Input(Some(0x99))).is_none());
}

#[test]
fn accepts_both_value_forms_case_insensitively() {
    assert_eq!(command(&["--preset=NIGHT"]), CliCommand::Preset { night: true });
    assert_eq!(command(&["--preset", "night"]), CliCommand::Preset { night: true });
    assert_eq!(command(&["--INPUT", "USB-C"]), CliCommand::Input(Some(0x31)));
    assert_eq!(command(&["--input=displayport"]), CliCommand::Input(Some(0x0F)));
    assert_eq!(command(&["--step", "contrast=-7"]), CliCommand::Step { code: 0x12, delta: -7 });
    assert_eq!(command(&["--step=brightness=3"]), CliCommand::Step { code: 0x10, delta: 3 });
}

#[test]
fn unknown_or_malformed_input_is_an_error() {
    assert_eq!(command(&[]), CliCommand::Show);
    for args in [&["--bogus"][..], &["--preset", "noon"], &["--step", "brightness=+500"], &["--preset"],
        &["--step", "volume=+5"], &["preset", "day"]] {
        assert!(parse(args).is_err(), "{args:?}");
    }
}

#[test]
fn first_verb_wins_and_a_malformed_verb_is_an_error() {
    assert_eq!(command(&["--preset", "day", "--input", "hdmi"]), CliCommand::Preset { night: false });
    assert!(parse(&["--preset", "noon", "--input", "hdmi"]).is_err());
    assert!(parse(&["--bogus", "--input", "hdmi"]).is_err());
}

#[test]
fn startup_flag_combines_with_a_command() {
    assert_eq!(parse(&["--startup", "--input", "usb-c"]).unwrap(),
        Parsed { command: CliCommand::Input(Some(0x31)), startup: true, json: false, ..Parsed::default() });
    assert_eq!(parse(&["--startup"]).unwrap(), Parsed { command: CliCommand::Show, startup: true, json: false, ..Parsed::default() });
}

#[test]
fn get_reads_each_target() {
    assert_eq!(command(&["--get", "brightness"]), CliCommand::Get(GetTarget::Brightness));
    assert_eq!(command(&["--GET=Contrast"]), CliCommand::Get(GetTarget::Contrast));
    assert_eq!(command(&["--get", "input"]), CliCommand::Get(GetTarget::Input));
    assert_eq!(command(&["--get", "colour-mode"]), CliCommand::Get(GetTarget::ColourMode));
    assert_eq!(command(&["--get", "color-mode"]), CliCommand::Get(GetTarget::ColourMode));
    assert_eq!(command(&["--get=all"]), CliCommand::Get(GetTarget::All));
    assert_eq!(format(&CliCommand::Get(GetTarget::ColourMode)).unwrap(), ["--get", "colour-mode"]);
}

#[test]
fn malformed_get_is_an_error_and_first_verb_still_wins() {
    assert!(parse(&["--get"]).is_err());
    assert!(parse(&["--get", "volume"]).is_err());
    assert!(parse(&["--get", "--json"]).is_err());
    assert_eq!(command(&["--get", "all", "--preset", "day"]), CliCommand::Get(GetTarget::All));
    assert_eq!(command(&["--preset", "day", "--get", "all"]), CliCommand::Preset { night: false });
}

#[test]
fn json_combines_with_any_verb() {
    assert_eq!(parse(&["--get", "all", "--json"]).unwrap(),
        Parsed { command: CliCommand::Get(GetTarget::All), startup: false, json: true, ..Parsed::default() });
    assert_eq!(parse(&["--JSON", "--preset=night"]).unwrap(),
        Parsed { command: CliCommand::Preset { night: true }, startup: false, json: true, ..Parsed::default() });
    assert_eq!(parse(&["--json"]).unwrap(), Parsed { command: CliCommand::Show, startup: false, json: true, ..Parsed::default() });
    assert!(!parse(&["--input", "dp"]).unwrap().json);
}

#[test]
fn help_wins_wherever_it_appears() {
    for args in [&["--help"][..], &["-h"], &["/?"], &["--HELP"], &["--preset", "day", "--help"], &["--preset", "noon", "-h"]] {
        assert_eq!(command(args), CliCommand::Help, "{args:?}");
    }
    assert!(parse(&["--help", "--json"]).unwrap().json);
    let text = help();
    for needle in ["--preset", "--input", "--step", "--get", "--json", "thinkvision://", "Exit codes"] {
        assert!(text.contains(needle), "{needle}");
    }
}

#[test]
fn urls_parse_case_insensitively_with_either_form() {
    let cases = [
        ("thinkvision://open", CliCommand::Show),
        ("thinkvision://", CliCommand::Show),
        ("thinkvision://preset/day", CliCommand::Preset { night: false }),
        ("ThinkVision://Preset/Night/", CliCommand::Preset { night: true }),
        ("thinkvision:preset/night", CliCommand::Preset { night: true }),
        ("thinkvision://input/hdmi", CliCommand::Input(Some(0x11))),
        ("thinkvision://input/dp", CliCommand::Input(Some(0x0F))),
        ("thinkvision://input/displayport", CliCommand::Input(Some(0x0F))),
        ("thinkvision://input/usbc", CliCommand::Input(Some(0x31))),
        ("thinkvision://input/USB-C", CliCommand::Input(Some(0x31))),
        ("thinkvision://input/this", CliCommand::Input(None)),
        ("thinkvision://step/brightness/+5", CliCommand::Step { code: 0x10, delta: 5 }),
        ("thinkvision://step/contrast/-10/", CliCommand::Step { code: 0x12, delta: -10 }),
        ("thinkvision://step/brightness/%2B5", CliCommand::Step { code: 0x10, delta: 5 }),
        ("thinkvision://get/brightness", CliCommand::Get(GetTarget::Brightness)),
        ("thinkvision://get/color-mode", CliCommand::Get(GetTarget::ColourMode)),
        ("thinkvision:get/all", CliCommand::Get(GetTarget::All)),
    ];
    for (url, expected) in cases {
        assert_eq!(parse_url(url), Some(expected), "{url}");
    }
}

#[test]
fn bad_urls_are_none() {
    for url in ["thinkvision://preset/noon", "thinkvision://preset", "thinkvision://step/brightness/+500",
        "thinkvision://step/volume/+5", "thinkvision://get", "thinkvision://bogus", "thinkvision://preset/day/extra",
        "https://preset/day", "preset/day", "thinkvisio"] {
        assert_eq!(parse_url(url), None, "{url}");
    }
}

#[test]
fn a_lone_url_argument_is_a_verb() {
    assert_eq!(command(&["thinkvision://preset/night"]), CliCommand::Preset { night: true });
    assert_eq!(command(&["THINKVISION:input/this"]), CliCommand::Input(None));
    assert!(parse(&["thinkvision://nonsense"]).is_err());
    assert_eq!(parse(&["--startup", "thinkvision://get/all", "--json"]).unwrap(),
        Parsed { command: CliCommand::Get(GetTarget::All), startup: true, json: true, ..Parsed::default() });
}

#[test]
fn exit_codes_follow_powertoys() {
    let cases = [(Some(ResultKind::Ok), 0), (Some(ResultKind::NotFound), 1), (Some(ResultKind::OutOfRange), 2),
        (Some(ResultKind::Unsupported), 4), (Some(ResultKind::Refused), 4), (Some(ResultKind::Kept), 4),
        (Some(ResultKind::NotResponding), 5), (Some(ResultKind::Asleep), 5), (Some(ResultKind::SafeMode), 5)];
    for (kind, code) in cases {
        assert_eq!(exit_code(kind, true), code, "{kind:?}");
    }
    assert_eq!(exit_code(None, true), 8);
    assert_eq!(exit_code(None, false), 1);
    assert_eq!(exit_code(Some(ResultKind::Ok), false), 0);
}

#[test]
fn cli_error_messages_name_the_argument() {
    let cases = [(&["--bogus"][..], "--bogus"), (&["-x"], "-x"), (&["stray"], "stray"), (&["--preset", "Noon"], "Noon"),
        (&["--preset"], "--preset"), (&["--input=vga"], "vga"), (&["--step", "volume=+5"], "volume"),
        (&["--step", "brightness=lots"], "lots"), (&["--get", "volume"], "volume")];
    for (args, needle) in cases {
        let message = parse(args).unwrap_err().message;
        assert!(message.contains(needle), "{args:?}: {message}");
    }
}

#[test]
fn help_wins_over_errors() {
    for args in [&["--bogus", "--help"][..], &["stray", "-h"], &["--json=1", "/?"], &["--preset", "/?"]] {
        assert_eq!(command(args), CliCommand::Help, "{args:?}");
    }
}

#[test]
fn developer_flags_are_accepted() {
    for flag in ["--dark", "--light", "--demo", "--DEMO"] {
        assert_eq!(command(&[flag]), CliCommand::Show, "{flag}");
    }
    assert_eq!(command(&["--demo", "--preset", "night"]), CliCommand::Preset { night: true });
}

#[test]
fn flags_reject_values() {
    for arg in ["--json=1", "--yes=true", "--startup=", "--version=2", "--demo=1"] {
        let message = parse(&[arg]).unwrap_err().message;
        assert!(message.contains("takes no value"), "{arg}: {message}");
    }
}

#[test]
fn second_valid_verb_is_ignored_invalid_is_error() {
    assert_eq!(command(&["--input", "dp", "--preset", "night"]), CliCommand::Input(Some(0x0F)));
    assert_eq!(command(&["--get", "all", "thinkvision://preset/day"]), CliCommand::Get(GetTarget::All));
    assert!(parse(&["--preset", "day", "--input", "vga"]).is_err());
    assert!(parse(&["--preset", "day", "thinkvision://nonsense"]).is_err());
}

#[test]
fn version_and_yes_flags() {
    assert_eq!(parse(&["--version"]).unwrap(), Parsed { version: true, ..Parsed::default() });
    assert_eq!(parse(&["--input", "this", "--YES"]).unwrap(),
        Parsed { command: CliCommand::Input(None), yes: true, ..Parsed::default() });
    assert_eq!(version_line(), format!("thinkvision-control {}", env!("CARGO_PKG_VERSION")));
}

#[test]
fn help_text_lists_new_items() {
    let text = help();
    for needle in ["--version", "--yes", "tvc", "3 input switch needs --yes", "--startup", "colour-mode"] {
        assert!(text.contains(needle), "{needle}");
    }
}

#[test]
fn shift_only_gestures_rejected() {
    assert_eq!(Gesture::parse("Shift+A"), None);
    assert_eq!(Gesture::try_create(Gesture::MOD_SHIFT, 0x41), None);
    assert_eq!(Gesture::parse("Shift+Win+A"), Some(Gesture { mods: Gesture::MOD_SHIFT | Gesture::MOD_WIN, vk: 0x41 }));
}

#[test]
fn linux_defaults_avoid_vt_and_workspace_keys() {
    let clashes = |g: &str| {
        g.starts_with("Ctrl+Alt+F") || ["Ctrl+Alt+Left", "Ctrl+Alt+Right", "Ctrl+Alt+Up", "Ctrl+Alt+Down"].contains(&g)
    };
    for action in Action::ALL {
        let linux = action.default_gesture_for(true);
        assert!(!clashes(linux), "{action:?}: {linux}");
        for text in [linux, action.default_gesture_for(false)] {
            assert_eq!(Gesture::parse(text).map(|g| g.to_string()).as_deref(), Some(text), "{action:?}");
        }
    }
    assert_eq!(Action::InputHdmi.default_gesture_for(false), "Ctrl+Alt+F1");
    assert_eq!(Action::InputHdmi.default_gesture_for(true), "Ctrl+Win+1");
    assert_eq!(Action::BrightnessUp.default_gesture(), Action::BrightnessUp.default_gesture_for(cfg!(target_os = "linux")));
}
