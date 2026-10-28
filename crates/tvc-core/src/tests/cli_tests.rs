//! CommandLineTests.

use crate::cli::{exit_code, format, help, parse, parse_url, CliCommand, GetTarget, Parsed};
use crate::result::ResultKind;

fn command(args: &[&str]) -> CliCommand {
    parse(args).command
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
        assert_eq!(parse(&format(&c).unwrap()).command, c, "{c:?}");
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
fn unknown_or_malformed_input_shows_the_window() {
    for args in [&[][..], &["--bogus"], &["--preset", "noon"], &["--step", "brightness=+500"], &["--preset"],
        &["--step", "volume=+5"], &["preset", "day"]] {
        assert_eq!(command(args), CliCommand::Show, "{args:?}");
    }
}

#[test]
fn first_verb_wins_and_a_malformed_first_verb_shows() {
    assert_eq!(command(&["--preset", "day", "--input", "hdmi"]), CliCommand::Preset { night: false });
    assert_eq!(command(&["--preset", "noon", "--input", "hdmi"]), CliCommand::Show);
    assert_eq!(command(&["--bogus", "--input", "hdmi"]), CliCommand::Input(Some(0x11)));
}

#[test]
fn startup_flag_combines_with_a_command() {
    assert_eq!(parse(&["--startup", "--input", "usb-c"]),
        Parsed { command: CliCommand::Input(Some(0x31)), startup: true, json: false });
    assert_eq!(parse(&["--startup"]), Parsed { command: CliCommand::Show, startup: true, json: false });
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
fn malformed_get_shows_and_first_verb_still_wins() {
    assert_eq!(command(&["--get"]), CliCommand::Show);
    assert_eq!(command(&["--get", "volume"]), CliCommand::Show);
    assert_eq!(command(&["--get", "--json"]), CliCommand::Show);
    assert_eq!(command(&["--get", "all", "--preset", "day"]), CliCommand::Get(GetTarget::All));
    assert_eq!(command(&["--preset", "day", "--get", "all"]), CliCommand::Preset { night: false });
}

#[test]
fn json_combines_with_any_verb() {
    assert_eq!(parse(&["--get", "all", "--json"]),
        Parsed { command: CliCommand::Get(GetTarget::All), startup: false, json: true });
    assert_eq!(parse(&["--JSON", "--preset=night"]),
        Parsed { command: CliCommand::Preset { night: true }, startup: false, json: true });
    assert_eq!(parse(&["--json"]), Parsed { command: CliCommand::Show, startup: false, json: true });
    assert!(!parse(&["--input", "dp"]).json);
}

#[test]
fn help_wins_wherever_it_appears() {
    for args in [&["--help"][..], &["-h"], &["/?"], &["--HELP"], &["--preset", "day", "--help"], &["--preset", "noon", "-h"]] {
        assert_eq!(command(args), CliCommand::Help, "{args:?}");
    }
    assert!(parse(&["--help", "--json"]).json);
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
    assert_eq!(command(&["thinkvision://nonsense"]), CliCommand::Show);
    assert_eq!(parse(&["--startup", "thinkvision://get/all", "--json"]),
        Parsed { command: CliCommand::Get(GetTarget::All), startup: true, json: true });
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
