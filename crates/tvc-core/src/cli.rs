//! Command-line verbs: `--preset day|night`, `--input hdmi|dp|usbc|this`, `--step brightness=+5`, plus `--startup`.
//! `--x value` and `--x=value` both work, case-insensitively. The first verb wins; a malformed first verb (or none)
//! gives [`CliCommand::Show`].

const MAX_STEP: i32 = 100;

// The first name per code is the canonical one `format` writes.
const INPUTS: [(&str, Option<u8>); 6] =
    [("hdmi", Some(0x11)), ("dp", Some(0x0F)), ("displayport", Some(0x0F)), ("usbc", Some(0x31)), ("usb-c", Some(0x31)), ("this", None)];
const STEPS: [(&str, u8); 2] = [("brightness", 0x10), ("contrast", 0x12)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CliCommand {
    /// Open the window.
    Show,
    Preset { night: bool },
    /// `None` = this PC's input (`ThisMachineInput`).
    Input(Option<u8>),
    Step { code: u8, delta: i32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parsed {
    pub command: CliCommand,
    /// `--startup`: start in the tray without a window.
    pub startup: bool,
}

pub fn parse<S: AsRef<str>>(args: &[S]) -> Parsed {
    let mut command = None;
    let mut startup = false;
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_ref();
        i += 1;
        let Some(rest) = arg.strip_prefix("--") else { continue };
        let (name, inline) = match rest.split_once('=') {
            Some((n, v)) => (n.to_lowercase(), Some(v)),
            None => (rest.to_lowercase(), None),
        };
        if name == "startup" {
            startup = true;
            continue;
        }
        if !matches!(name.as_str(), "preset" | "input" | "step") {
            continue;
        }
        let value = match inline {
            Some(v) => Some(v),
            None if i < args.len() && !args[i].as_ref().starts_with("--") => {
                i += 1;
                Some(args[i - 1].as_ref())
            }
            None => None,
        };
        if command.is_none() {
            command = Some(value.and_then(|v| parse_verb(&name, &v.trim().to_lowercase())).unwrap_or(CliCommand::Show));
        }
    }
    Parsed { command: command.unwrap_or(CliCommand::Show), startup }
}

/// Canonical arguments (`--preset day`, `--input dp`, `--step brightness=+5`); None when not expressible.
pub fn format(command: &CliCommand) -> Option<Vec<String>> {
    let args = |verb: &str, value: String| Some(vec![verb.to_string(), value]);
    match *command {
        CliCommand::Show => Some(Vec::new()),
        CliCommand::Preset { night } => args("--preset", if night { "night" } else { "day" }.into()),
        CliCommand::Input(code) => INPUTS.iter().find(|(_, c)| *c == code).and_then(|(n, _)| args("--input", n.to_string())),
        CliCommand::Step { code, delta } if (-MAX_STEP..=MAX_STEP).contains(&delta) => {
            let name = STEPS.iter().find(|(_, c)| *c == code)?.0;
            args("--step", format!("{name}={}{}", if delta < 0 { "-" } else { "+" }, delta.abs()))
        }
        CliCommand::Step { .. } => None,
    }
}

fn parse_verb(name: &str, value: &str) -> Option<CliCommand> {
    match name {
        "preset" => match value {
            "day" => Some(CliCommand::Preset { night: false }),
            "night" => Some(CliCommand::Preset { night: true }),
            _ => None,
        },
        "input" => INPUTS.iter().find(|(n, _)| *n == value).map(|(_, c)| CliCommand::Input(*c)),
        _ => {
            let (target, delta) = value.split_once('=')?;
            let code = STEPS.iter().find(|(n, _)| *n == target)?.1;
            let delta: i32 = delta.parse().ok()?;
            (-MAX_STEP..=MAX_STEP).contains(&delta).then_some(CliCommand::Step { code, delta })
        }
    }
}
