//! Command-line verbs: `--preset day|night`, `--input hdmi|dp|usbc|this`, `--step brightness=+5`,
//! `--get brightness|contrast|input|colour-mode|all`, `--help`, plus the flags `--startup`, `--json`, `--yes` and
//! `--version`. `--x value` and `--x=value` both work, case-insensitively. Every argument must be valid, or
//! [`parse`] returns a [`CliError`]; of several valid verbs the first wins, and the rest are ignored. `--help` (`-h`,
//! `/?`) wins wherever it appears, even next to a bad argument. A `thinkvision://…` argument (how the OS opens a
//! link) counts as a verb; see [`parse_url`]. No verb gives [`CliCommand::Show`].

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CliCommand {
    /// Open the window.
    #[default]
    Show,
    Preset { night: bool },
    /// `None` = this PC's input (`ThisMachineInput`).
    Input(Option<u8>),
    Step { code: u8, delta: i32 },
    /// Read and print.
    Get(GetTarget),
    /// Print [`help`].
    Help,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    pub command: CliCommand,
    /// `--startup`: start in the tray without a window.
    pub startup: bool,
    /// `--json`: print results as JSON.
    pub json: bool,
    /// `--yes`: confirms an input switch given on the command line.
    pub yes: bool,
    /// `--version`: print the version and exit.
    pub version: bool,
}

/// A malformed command line; exit code 2, with `message` and the usage on stderr.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CliError {
    pub message: String,
}

/// Exit code for a [`CliError`].
pub const EXIT_USAGE: i32 = 2;
/// Exit code when `--input` needs `--yes` and nobody can confirm on screen.
pub const EXIT_NEEDS_CONFIRMATION: i32 = 3;

pub fn parse<S: AsRef<str>>(args: &[S]) -> Result<Parsed, CliError> {
    let mut parsed = Parsed::default();
    let (mut command, mut help, mut error, mut developer) = (None, false, None, false);
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_ref();
        i += 1;
        let outcome = if arg == "-h" || arg == "/?" {
            help = true;
            Ok(None)
        } else if let Some(rest) = arg.strip_prefix("--") {
            let (typed, inline) = match rest.split_once('=') {
                Some((n, v)) => (n, Some(v)),
                None => (rest, None),
            };
            let name = typed.to_lowercase();
            let flag = match name.as_str() {
                "startup" => Some(&mut parsed.startup),
                "json" => Some(&mut parsed.json),
                "yes" => Some(&mut parsed.yes),
                "version" => Some(&mut parsed.version),
                "help" => Some(&mut help),
                // Developer flags the app reads itself.
                "dark" | "light" | "demo" => Some(&mut developer),
                _ => None,
            };
            match (flag, inline) {
                (Some(_), Some(_)) => Err(format!("option '--{typed}' takes no value")),
                (Some(flag), None) => {
                    *flag = true;
                    Ok(None)
                }
                (None, _) if matches!(name.as_str(), "preset" | "input" | "step" | "get") => {
                    let value = inline.or_else(|| {
                        let next = args.get(i)?.as_ref();
                        (!next.starts_with('-') && next != "/?").then(|| {
                            i += 1;
                            next
                        })
                    });
                    match value.map(str::trim).filter(|v| !v.is_empty()) {
                        Some(v) => parse_verb(&name, v).map(Some),
                        None => Err(format!("'--{typed}' needs a value ({})", choices(&name))),
                    }
                }
                (None, _) => Err(format!("unknown option '--{typed}'")),
            }
        } else if is_url(arg) {
            url_command(arg).map(Some)
        } else if arg.starts_with('-') {
            Err(format!("unknown option '{arg}'"))
        } else {
            Err(format!("unexpected argument '{arg}'"))
        };
        match outcome {
            Ok(verb) => command = command.or(verb),
            Err(message) => error = error.or(Some(message)),
        }
    }
    parsed.command = match (help, error) {
        (true, _) => CliCommand::Help,
        (false, Some(message)) => return Err(CliError { message }),
        (false, None) => command.unwrap_or_default(),
    };
    Ok(parsed)
}

/// `thinkvision-control <version>`, for `--version`.
pub fn version_line() -> String {
    format!("thinkvision-control {}", env!("CARGO_PKG_VERSION"))
}

/// A `thinkvision:` link, case-insensitive, with or without `//` and a trailing `/`:
/// `open`, `preset/day|night`, `input/hdmi|dp|displayport|usbc|usb-c|this`, `step/brightness|contrast/+N|-N`,
/// `get/brightness|contrast|input|colour-mode|color-mode|all`. None when it isn't one of these.
pub fn parse_url(url: &str) -> Option<CliCommand> {
    url_command(url).ok()
}

fn url_command(url: &str) -> Result<CliCommand, String> {
    if !is_url(url) {
        return Err(format!("'{url}' is not a thinkvision: link"));
    }
    let lower = url.trim().to_lowercase().replace("%2b", "+");
    let path = lower["thinkvision:".len()..].trim_start_matches('/').trim_end_matches('/');
    let parts: Vec<&str> = path.split('/').collect();
    match parts[..] {
        ["" | "open"] => Ok(CliCommand::Show),
        ["preset" | "input" | "get", value] => parse_verb(parts[0], value),
        ["step", target, delta] => parse_verb("step", &format!("{target}={delta}")),
        _ => Err(format!("'{}' is not a known thinkvision: link (see --help)", url.trim())),
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
    "ThinkVision Control

Usage: thinkvision-control [verb] [--json] [--yes] [--startup]
On Windows, scripts should run tvc (tvc.exe), the console launcher, with the same arguments.

Verbs (the first one counts):
  --preset day|night                 Apply the Day or Night preset
  --input hdmi|dp|usbc|this          Switch the input (this = this PC's input)
  --step brightness|contrast=+N|-N   Change brightness or contrast by N (up to 100)
  --get brightness|contrast|input|colour-mode|all
                                     Print current values
  --help, -h, /?                     Show this text
  --version                          Print the version
  (none)                             Open the window

Flags:
  --json      Print the result as JSON
  --yes       Confirm an --input switch without asking on screen
  --startup   Start in the tray without a window

Exit codes: 0 OK, 1 monitor not found, 2 bad arguments or value out of range,
            3 input switch needs --yes, 4 not supported,
            5 monitor not answering (or asleep, or safe mode), 8 timed out.

Links (thinkvision://):
  thinkvision://open
  thinkvision://preset/day             thinkvision://preset/night
  thinkvision://input/hdmi             (also dp, usbc, this)
  thinkvision://step/brightness/+10    thinkvision://step/contrast/-5
  thinkvision://get/all
"
}

/// The accepted values of a verb, for error messages.
fn choices(verb: &str) -> &'static str {
    match verb {
        "preset" => "day, night",
        "input" => "hdmi, dp, usbc, this",
        "get" => "brightness, contrast, input, colour-mode, all",
        _ => "brightness=+N or contrast=-N",
    }
}

/// `value` is trimmed but may be in any case.
fn parse_verb(name: &str, value: &str) -> Result<CliCommand, String> {
    let lower = value.to_lowercase();
    let command = match name {
        "preset" => match lower.as_str() {
            "day" => Some(CliCommand::Preset { night: false }),
            "night" => Some(CliCommand::Preset { night: true }),
            _ => None,
        },
        "input" => INPUTS.iter().find(|(n, _)| *n == lower).map(|(_, c)| CliCommand::Input(*c)),
        "get" => TARGETS.iter().find(|(n, _)| *n == lower).map(|(_, t)| CliCommand::Get(*t)),
        _ => return parse_step(&lower),
    };
    let what = match name {
        "preset" => "a preset",
        "input" => "an input",
        _ => "something --get can read",
    };
    command.ok_or_else(|| format!("'{value}' is not {what} ({})", choices(name)))
}

fn parse_step(value: &str) -> Result<CliCommand, String> {
    let Some((target, delta)) = value.split_once('=') else {
        return Err(format!("'{value}' is not a step ({})", choices("step")));
    };
    let Some(code) = STEPS.iter().find(|(n, _)| *n == target).map(|(_, c)| *c) else {
        return Err(format!("'{target}' can't be stepped (brightness, contrast)"));
    };
    let delta: i32 = delta.parse().map_err(|_| format!("'{delta}' is not a number like +5 or -5"))?;
    if !(-MAX_STEP..=MAX_STEP).contains(&delta) {
        return Err(format!("step {delta:+} is out of range (-{MAX_STEP} to +{MAX_STEP})"));
    }
    Ok(CliCommand::Step { code, delta })
}
