//! Command-line verbs: `--preset day|night`, `--input hdmi|dp|usbc|this`, `--step brightness=+5`,
//! `--get brightness|contrast|input|colour-mode|all`, `--help`, plus the flags `--startup` and `--json`.
//! `--x value` and `--x=value` both work, case-insensitively. The first verb wins; a malformed first verb (or none)
//! gives [`CliCommand::Show`]. `--help` (`-h`, `/?`) wins wherever it appears. A `thinkvision://…` argument (how
//! the OS opens a link) counts as a verb; see [`parse_url`].

use crate::result::ResultKind;

const MAX_STEP: i32 = 100;

// The first name per code is the canonical one `format` writes.
const INPUTS: [(&str, Option<u8>); 6] =
    [("hdmi", Some(0x11)), ("dp", Some(0x0F)), ("displayport", Some(0x0F)), ("usbc", Some(0x31)), ("usb-c", Some(0x31)), ("this", None)];
const STEPS: [(&str, u8); 2] = [("brightness", 0x10), ("contrast", 0x12)];
const TARGETS: [(&str, GetTarget); 6] = [("brightness", GetTarget::Brightness), ("contrast", GetTarget::Contrast),
    ("input", GetTarget::Input), ("colour-mode", GetTarget::ColourMode), ("color-mode", GetTarget::ColourMode),
    ("all", GetTarget::All)];

/// What `--get` reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GetTarget {
    Brightness,
    Contrast,
    Input,
    ColourMode,
    All,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CliCommand {
    /// Open the window.
    Show,
    Preset { night: bool },
    /// `None` = this PC's input (`ThisMachineInput`).
    Input(Option<u8>),
    Step { code: u8, delta: i32 },
    /// Read and print; always runs its own read-only connection (a forwarded command can't return output).
    Get(GetTarget),
    /// Print [`help`].
    Help,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parsed {
    pub command: CliCommand,
    /// `--startup`: start in the tray without a window.
    pub startup: bool,
    /// `--json`: print results as JSON.
    pub json: bool,
}

pub fn parse<S: AsRef<str>>(args: &[S]) -> Parsed {
    let mut command = None;
    let (mut startup, mut json, mut help) = (false, false, false);
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_ref();
        i += 1;
        if arg == "-h" || arg == "/?" {
            help = true;
            continue;
        }
        let Some(rest) = arg.strip_prefix("--") else {
            if is_url(arg) && command.is_none() {
                command = Some(parse_url(arg).unwrap_or(CliCommand::Show));
            }
            continue;
        };
        let (name, inline) = match rest.split_once('=') {
            Some((n, v)) => (n.to_lowercase(), Some(v)),
            None => (rest.to_lowercase(), None),
        };
        match name.as_str() {
            "startup" => startup = true,
            "json" => json = true,
            "help" => help = true,
            "preset" | "input" | "step" | "get" => {
                let value = match inline {
                    Some(v) => Some(v),
                    None if i < args.len() && !args[i].as_ref().starts_with("--") => {
                        i += 1;
                        Some(args[i - 1].as_ref())
                    }
                    None => None,
                };
                if command.is_none() {
                    command =
                        Some(value.and_then(|v| parse_verb(&name, &v.trim().to_lowercase())).unwrap_or(CliCommand::Show));
                }
            }
            _ => {}
        }
    }
    let command = if help { CliCommand::Help } else { command.unwrap_or(CliCommand::Show) };
    Parsed { command, startup, json }
}

/// A `thinkvision:` link, case-insensitive, with or without `//` and a trailing `/`:
/// `open`, `preset/day|night`, `input/hdmi|dp|displayport|usbc|usb-c|this`, `step/brightness|contrast/+N|-N`,
/// `get/brightness|contrast|input|colour-mode|color-mode|all`. None when it isn't one of these.
pub fn parse_url(url: &str) -> Option<CliCommand> {
    if !is_url(url) {
        return None;
    }
    let lower = url.trim().to_lowercase().replace("%2b", "+");
    let path = lower["thinkvision:".len()..].trim_start_matches('/').trim_end_matches('/');
    let parts: Vec<&str> = path.split('/').collect();
    match parts[..] {
        ["" | "open"] => Some(CliCommand::Show),
        ["preset" | "input" | "get", value] => parse_verb(parts[0], value),
        ["step", target, delta] => parse_verb("step", &format!("{target}={delta}")),
        _ => None,
    }
}

fn is_url(arg: &str) -> bool {
    arg.trim().get(..12).is_some_and(|scheme| scheme.eq_ignore_ascii_case("thinkvision:"))
}

/// Canonical arguments (`--preset day`, `--input dp`, `--step brightness=+5`, `--get all`, `--help`); None when not
/// expressible.
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
        CliCommand::Get(target) => TARGETS.iter().find(|(_, t)| *t == target).and_then(|(n, _)| args("--get", n.to_string())),
        CliCommand::Help => Some(vec!["--help".into()]),
    }
}

/// The process exit code (PowerToys' CliExitCodes): 0 OK, 1 not found, 2 out of range, 4 unsupported, 5 hardware
/// (not answering, asleep, safe mode), 8 timed out. `kind` is the result, None when none arrived in time;
/// `status_found` is false when the last status said no monitor is attached, which turns a missing result into 1.
pub fn exit_code(kind: Option<ResultKind>, status_found: bool) -> i32 {
    match kind {
        Some(ResultKind::Ok) => 0,
        Some(ResultKind::NotFound) => 1,
        Some(ResultKind::OutOfRange) => 2,
        Some(ResultKind::Unsupported | ResultKind::Refused | ResultKind::Kept) => 4,
        Some(ResultKind::NotResponding | ResultKind::Asleep | ResultKind::SafeMode) => 5,
        None if !status_found => 1,
        None => 8,
    }
}

/// Usage text for `--help`.
pub fn help() -> &'static str {
    "\
ThinkVision Control

Usage: thinkvision-control [verb] [--json] [--startup]

Verbs (the first one counts):
  --preset day|night                 Apply the Day or Night preset
  --input hdmi|dp|usbc|this          Switch the input (this = this PC's input)
  --step brightness|contrast=+N|-N   Change brightness or contrast by N (up to 100)
  --get brightness|contrast|input|colour-mode|all
                                     Print current values (always reads the monitor itself)
  --help, -h, /?                     Show this text
  (none)                             Open the window

Flags:
  --json      Print the result as JSON
  --startup   Start in the tray without a window

Exit codes: 0 OK, 1 monitor not found, 2 value out of range, 4 not supported,
            5 monitor not answering (or asleep, or safe mode), 8 timed out.
A verb handed to a running copy exits 0; the result shows on screen there.

Links (thinkvision://):
  thinkvision://open
  thinkvision://preset/day             thinkvision://preset/night
  thinkvision://input/hdmi             (also dp, usbc, this)
  thinkvision://step/brightness/+10    thinkvision://step/contrast/-5
  thinkvision://get/all
"
}

fn parse_verb(name: &str, value: &str) -> Option<CliCommand> {
    match name {
        "preset" => match value {
            "day" => Some(CliCommand::Preset { night: false }),
            "night" => Some(CliCommand::Preset { night: true }),
            _ => None,
        },
        "input" => INPUTS.iter().find(|(n, _)| *n == value).map(|(_, c)| CliCommand::Input(*c)),
        "get" => TARGETS.iter().find(|(n, _)| *n == value).map(|(_, t)| CliCommand::Get(*t)),
        _ => {
            let (target, delta) = value.split_once('=')?;
            let code = STEPS.iter().find(|(n, _)| *n == target)?.1;
            let delta: i32 = delta.parse().ok()?;
            (-MAX_STEP..=MAX_STEP).contains(&delta).then_some(CliCommand::Step { code, delta })
        }
    }
}
