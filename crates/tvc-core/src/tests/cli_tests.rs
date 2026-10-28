//! CommandLineTests.

use crate::cli::{format, parse, CliCommand, Parsed};

fn command(args: &[&str]) -> CliCommand {
    parse(args).command
}

#[test]
fn format_round_trips_through_parse() {
    let commands = [
        CliCommand::Preset { night: false }, CliCommand::Preset { night: true },
        CliCommand::Input(Some(0x11)), CliCommand::Input(Some(0x0F)), CliCommand::Input(Some(0x31)), CliCommand::Input(None),
        CliCommand::Step { code: 0x10, delta: 5 }, CliCommand::Step { code: 0x12, delta: -5 },
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
    assert_eq!(parse(&["--startup", "--input", "usb-c"]), Parsed { command: CliCommand::Input(Some(0x31)), startup: true });
    assert_eq!(parse(&["--startup"]), Parsed { command: CliCommand::Show, startup: true });
}
