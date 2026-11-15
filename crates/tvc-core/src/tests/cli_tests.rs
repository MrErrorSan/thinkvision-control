//! CommandLineTests.

use crate::cli::{
    combined_exit_code, command_line, exit_code, format, help, parse, parse_url, resolve_display, usage_error,
    version_line, CliCommand, GetTarget, InputChoice, Parsed,
};
use crate::hotkey::{Action, Gesture};
use crate::result::ResultKind;

fn command(args: &[&str]) -> CliCommand {
    parse(args).unwrap().command
}

#[test]
fn format_round_trips_through_parse() {
    let commands = [
        CliCommand::Preset { night: false }, CliCommand::Preset { night: true },
        CliCommand::Input(InputChoice::Kind("hdmi".into())), CliCommand::Input(InputChoice::Kind("dp".into())), CliCommand::Input(InputChoice::Kind("usbc".into())), CliCommand::Input(InputChoice::This),
        CliCommand::Input(InputChoice::Slot(1)), CliCommand::Input(InputChoice::Slot(3)),
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
    assert_eq!(format(&CliCommand::Input(InputChoice::Kind("dp".into()))).unwrap(), ["--input", "dp"]);
    assert_eq!(format(&CliCommand::Input(InputChoice::This)).unwrap(), ["--input", "this"]);
    assert_eq!(format(&CliCommand::Step { code: 0x10, delta: 5 }).unwrap(), ["--step", "brightness=+5"]);
    assert_eq!(format(&CliCommand::Step { code: 0x12, delta: 0 }).unwrap(), ["--step", "contrast=+0"]);
    assert_eq!(format(&CliCommand::Show).unwrap(), Vec::<String>::new());
    assert!(format(&CliCommand::Step { code: 0x10, delta: 500 }).is_none());
    assert!(format(&CliCommand::Input(InputChoice::Kind("vga".into()))).is_none());
}

#[test]
fn accepts_both_value_forms_case_insensitively() {
    assert_eq!(command(&["--preset=NIGHT"]), CliCommand::Preset { night: true });
    assert_eq!(command(&["--preset", "night"]), CliCommand::Preset { night: true });
    assert_eq!(command(&["--INPUT", "USB-C"]), CliCommand::Input(InputChoice::Kind("usbc".into())));
    assert_eq!(command(&["--input=displayport"]), CliCommand::Input(InputChoice::Kind("dp".into())));
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
        Parsed { command: CliCommand::Input(InputChoice::Kind("usbc".into())), startup: true, json: false, ..Parsed::default() });
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
        ("thinkvision://input/hdmi", CliCommand::Input(InputChoice::Kind("hdmi".into()))),
        ("thinkvision://input/dp", CliCommand::Input(InputChoice::Kind("dp".into()))),
        ("thinkvision://input/displayport", CliCommand::Input(InputChoice::Kind("dp".into()))),
        ("thinkvision://input/usbc", CliCommand::Input(InputChoice::Kind("usbc".into()))),
        ("thinkvision://input/USB-C", CliCommand::Input(InputChoice::Kind("usbc".into()))),
        ("thinkvision://input/this", CliCommand::Input(InputChoice::This)),
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
    assert_eq!(command(&["THINKVISION:input/this"]), CliCommand::Input(InputChoice::This));
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
    assert_eq!(command(&["--input", "dp", "--preset", "night"]), CliCommand::Input(InputChoice::Kind("dp".into())));
    assert_eq!(command(&["--get", "all", "thinkvision://preset/day"]), CliCommand::Get(GetTarget::All));
    assert!(parse(&["--preset", "day", "--input", "vga"]).is_err());
    assert!(parse(&["--preset", "day", "thinkvision://nonsense"]).is_err());
}

#[test]
fn version_and_yes_flags() {
    assert_eq!(parse(&["--version"]).unwrap(), Parsed { version: true, ..Parsed::default() });
    assert_eq!(parse(&["--input", "this", "--YES"]).unwrap(),
        Parsed { command: CliCommand::Input(InputChoice::This), yes: true, ..Parsed::default() });
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

// ---------------------------------------------------------------------------------------------------------------
// Wave 4: --display, display links, input slots, resolve_display and the multi-display helpers.

fn display(args: &[&str]) -> Option<String> {
    parse(args).unwrap().display
}

#[test]
fn display_flag_both_forms() {
    for args in [&["--display", "LEN-S1", "--preset", "day"][..], &["--display=LEN-S1", "--preset", "day"],
        &["--preset", "day", "--DISPLAY", "LEN-S1"]] {
        let parsed = parse(args).unwrap();
        assert_eq!(parsed.command, CliCommand::Preset { night: false }, "{args:?}");
        assert_eq!(parsed.display.as_deref(), Some("LEN-S1"), "{args:?}");
    }
    // Kept as typed (case and inner spaces), trimmed at the ends.
    assert_eq!(display(&["--display", " My Screen ", "--get", "all"]).as_deref(), Some("My Screen"));
    assert_eq!(display(&["--display=all", "--get", "all"]).as_deref(), Some("all"));
    assert_eq!(display(&["--get", "all"]), None);
}

#[test]
fn display_needs_value() {
    for args in [&["--display"][..], &["--display="], &["--display", "--json"], &["--preset", "day", "--display"],
        &["--display=", "--preset", "day"], &["--display", "  ", "--preset", "day"]] {
        let message = parse(args).unwrap_err().message;
        assert!(message.contains("--display"), "{args:?}: {message}");
    }
}

#[test]
fn display_without_verb_is_error() {
    let message = parse(&["--display", "x"]).unwrap_err().message;
    assert!(message.contains("--display x") && message.contains("needs a verb"), "{message}");
    assert!(parse(&["--display", "x", "--json"]).is_err());
    assert!(parse(&["--display", "x", "thinkvision://open"]).is_err());
    assert_eq!(command(&["--display", "x", "--help"]), CliCommand::Help);
    assert_eq!(command(&["-h", "--display", "x"]), CliCommand::Help);
    let parsed = parse(&["--display", "x", "--version"]).unwrap();
    assert!(parsed.version);
    assert_eq!(parsed.command, CliCommand::Show);
}

#[test]
fn first_display_wins() {
    assert_eq!(display(&["--display", "a", "--display", "b", "--preset", "day"]).as_deref(), Some("a"));
    assert_eq!(display(&["--display=a", "--preset", "day", "--display=b"]).as_deref(), Some("a"));
    // A flag before a display link wins over the link's display, and a link's display over a later flag.
    assert_eq!(display(&["--display", "a", "thinkvision://display/b/preset/day"]).as_deref(), Some("a"));
    assert_eq!(display(&["thinkvision://display/b/preset/day", "--display", "a"]).as_deref(), Some("b"));
}

#[test]
fn display_url() {
    let parsed = parse(&["thinkvision://display/LEN-S1/step/brightness/+5"]).unwrap();
    assert_eq!(parsed.command, CliCommand::Step { code: 0x10, delta: 5 });
    assert_eq!(parsed.display.as_deref(), Some("LEN-S1"));
    // The scheme and the "display" segment in any case; the verb lowercased as usual, the name kept.
    let parsed = parse(&["THINKVISION:Display/Left/Preset/NIGHT/"]).unwrap();
    assert_eq!(parsed.command, CliCommand::Preset { night: true });
    assert_eq!(parsed.display.as_deref(), Some("Left"));
    let parsed = parse(&["thinkvision://display/ALL/input/2"]).unwrap();
    assert_eq!(parsed.command, CliCommand::Input(InputChoice::Slot(2)));
    assert_eq!(parsed.display.as_deref(), Some("ALL"));
}

#[test]
fn display_url_decodes() {
    assert_eq!(display(&["thinkvision://display/My%20Screen/get/all"]).as_deref(), Some("My Screen"));
    assert_eq!(display(&["thinkvision://display/a%2Fb/get/all"]).as_deref(), Some("a/b"));
    assert_eq!(display(&["thinkvision://display/%C3%A9cran/get/all"]).as_deref(), Some("\u{e9}cran"));
    // A malformed escape, invalid UTF-8, or a name that is only spaces is not a link.
    for url in ["thinkvision://display/bad%2/get/all", "thinkvision://display/%ZZ/get/all",
        "thinkvision://display/%FF/get/all", "thinkvision://display/%20/get/all"] {
        assert!(parse(&[url]).is_err(), "{url}");
        assert_eq!(parse_url(url), None, "{url}");
    }
}

#[test]
fn display_url_rejects() {
    for url in ["thinkvision://display/x/open", "thinkvision://display/x", "thinkvision://display//preset/day",
        "thinkvision://display/x/", "thinkvision://display/", "thinkvision://display/x/bogus/day"] {
        assert_eq!(parse_url(url), None, "{url}");
        let message = parse(&[url]).unwrap_err().message;
        assert!(message.contains("not a known thinkvision: link"), "{url}: {message}");
    }
}

#[test]
fn display_url_parse_url_drops_display() {
    assert_eq!(parse_url("thinkvision://display/LEN-S1/step/brightness/+5"), Some(CliCommand::Step { code: 0x10, delta: 5 }));
    assert_eq!(parse_url("thinkvision://display/all/get/all"), Some(CliCommand::Get(GetTarget::All)));
    assert_eq!(parse_url("thinkvision:display/x/preset/day"), Some(CliCommand::Preset { night: false }));
}

#[test]
fn ignored_url_display() {
    let parsed = parse(&["--preset", "day", "thinkvision://display/b/get/all"]).unwrap();
    assert_eq!(parsed.command, CliCommand::Preset { night: false });
    assert_eq!(parsed.display, None);
    let parsed = parse(&["thinkvision://display/a/get/all", "thinkvision://display/b/preset/day"]).unwrap();
    assert_eq!(parsed.command, CliCommand::Get(GetTarget::All));
    assert_eq!(parsed.display.as_deref(), Some("a"));
}

#[test]
fn input_slots() {
    assert_eq!(command(&["--input", "1"]), CliCommand::Input(InputChoice::Slot(1)));
    assert_eq!(command(&["--input=3"]), CliCommand::Input(InputChoice::Slot(3)));
    assert_eq!(command(&["thinkvision://input/2"]), CliCommand::Input(InputChoice::Slot(2)));
    assert_eq!(parse_url("thinkvision://input/2"), Some(CliCommand::Input(InputChoice::Slot(2))));
    for args in [&["--input", "0"][..], &["--input", "4"], &["--input=0"], &["--input", "-1"], &["thinkvision://input/4"]] {
        assert!(parse(args).is_err(), "{args:?}");
    }
    assert_eq!(parse_url("thinkvision://input/0"), None);
    assert!(parse(&["--input", "4"]).unwrap_err().message.contains('4'));
}

#[test]
fn format_out_of_range_slot() {
    assert_eq!(format(&CliCommand::Input(InputChoice::Slot(1))).unwrap(), ["--input", "1"]);
    assert_eq!(format(&CliCommand::Input(InputChoice::Slot(3))).unwrap(), ["--input", "3"]);
    assert!(format(&CliCommand::Input(InputChoice::Slot(0))).is_none());
    assert!(format(&CliCommand::Input(InputChoice::Slot(4))).is_none());
    assert!(format(&CliCommand::Input(InputChoice::Slot(255))).is_none());
}

fn displays(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs.iter().map(|(id, name)| (id.to_string(), name.to_string())).collect()
}

#[test]
fn resolve_all() {
    let list = displays(&[("LEN-2", "Right"), ("LEN-1", "Left"), ("DEL-9", "Dell")]);
    for wanted in ["all", "ALL", "All", " all "] {
        assert_eq!(resolve_display(wanted, &list).unwrap(), ["LEN-2", "LEN-1", "DEL-9"], "{wanted}");
    }
    assert!(resolve_display("all", &[]).unwrap().is_empty());
}

#[test]
fn resolve_exact_id_wins() {
    // "B" is display 1's name and display 2's id: the id wins.
    let list = displays(&[("A", "B"), ("B", "x")]);
    assert_eq!(resolve_display("B", &list).unwrap(), ["B"]);
    // The id match is exact: "b" is no id, so it matches display 1's name, ignoring case.
    assert_eq!(resolve_display("b", &list).unwrap(), ["A"]);
    assert_eq!(resolve_display(" B ", &list).unwrap(), ["B"]);
}

#[test]
fn resolve_name_case_insensitive() {
    let list = displays(&[("LEN-1", "ThinkVision P32p-20"), ("DEL-9", "DELL U2720Q")]);
    assert_eq!(resolve_display("thinkvision p32p-20", &list).unwrap(), ["LEN-1"]);
    assert_eq!(resolve_display("dell u2720q", &list).unwrap(), ["DEL-9"]);
    assert_eq!(resolve_display("DELL U2720Q", &list).unwrap(), ["DEL-9"]);
}

#[test]
fn resolve_ambiguous() {
    let list = displays(&[("LEN-1", "P32p-20"), ("DEL-9", "Dell"), ("LEN-2", "p32p-20")]);
    let message = resolve_display("P32P-20", &list).unwrap_err().message;
    assert!(message.contains("several"), "{message}");
    assert!(message.contains("P32p-20 (LEN-1)") && message.contains("p32p-20 (LEN-2)"), "{message}");
    assert!(!message.contains("DEL-9"), "{message}");
    // An id still picks one of them.
    assert_eq!(resolve_display("LEN-2", &list).unwrap(), ["LEN-2"]);
}

#[test]
fn resolve_none() {
    let list = displays(&[("LEN-1", "Left"), ("DEL-9", "Dell")]);
    let message = resolve_display("Middle", &list).unwrap_err().message;
    assert!(message.contains("'Middle'"), "{message}");
    assert!(message.contains("Left (LEN-1)") && message.contains("Dell (DEL-9)"), "{message}");
    let message = resolve_display("Middle", &[]).unwrap_err().message;
    assert!(message.contains("'Middle'") && message.contains("none is attached"), "{message}");
}

#[test]
fn combined_exit_code_is_first_failure() {
    assert_eq!(combined_exit_code(&[]), 0);
    assert_eq!(combined_exit_code(&[0, 0]), 0);
    assert_eq!(combined_exit_code(&[0, 5, 1]), 5);
    assert_eq!(combined_exit_code(&[8]), 8);
}

#[test]
fn action_input_choice() {
    assert_eq!(Action::InputHdmi.input_choice(), Some(InputChoice::Kind("hdmi".into())));
    assert_eq!(Action::InputDp.input_choice(), Some(InputChoice::Kind("dp".into())));
    assert_eq!(Action::InputUsbC.input_choice(), Some(InputChoice::Kind("usbc".into())));
    assert_eq!(Action::SwitchToMe.input_choice(), Some(InputChoice::This));
    assert_eq!(Action::BrightnessUp.input_choice(), None);
    assert_eq!(Action::OpenPanel.input_choice(), None);
    let with: Vec<Action> = Action::ALL.into_iter().filter(|a| a.input_choice().is_some()).collect();
    assert_eq!(with, [Action::InputHdmi, Action::InputDp, Action::InputUsbC, Action::SwitchToMe]);
    // Each input shortcut means what `--input` with its kind means.
    for (action, arg) in [(Action::InputHdmi, "hdmi"), (Action::InputDp, "dp"), (Action::InputUsbC, "usbc"),
        (Action::SwitchToMe, "this")] {
        assert_eq!(Some(command(&["--input", arg])), action.input_choice().map(CliCommand::Input), "{arg}");
    }
}

#[test]
fn help_mentions_display() {
    let text = help();
    for needle in ["--display", "name|id|all", "thinkvision://display/", "1|2|3", "several displays"] {
        assert!(text.contains(needle), "{needle}");
    }
}

#[test]
fn usage_error_is_the_message_and_one_hint_line() {
    let e = parse(&["--foo"]).unwrap_err();
    assert_eq!(usage_error(&e), "unknown option '--foo'\nRun with --help for usage.");
    assert!(!usage_error(&e).contains("Exit codes"));
}

#[test]
fn command_line_keeps_the_arguments_as_given() {
    assert_eq!(command_line(&["thinkvision://get/all"]), "thinkvision://get/all");
    assert_eq!(command_line(&["--get", "brightness", "--json"]), "--get brightness --json");
    assert_eq!(command_line(&["--display", "len p32p-20", "--get=contrast"]), "--display \"len p32p-20\" --get=contrast");
    assert_eq!(command_line(&["", "a\"b"]), "\"\" \"a\\\"b\"");
    assert_eq!(command_line::<&str>(&[]), "");
}
