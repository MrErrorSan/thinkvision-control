//! Command-line verbs: `--preset day|night`, `--input hdmi|dp|usbc|this|1|2|3`, `--step brightness=+5`,
//! `--get brightness|contrast|input|colour-mode|all`, `--help`, plus the flags `--startup`, `--json`, `--yes` and
//! `--version`. `--x value` and `--x=value` both work, case-insensitively. Every argument must be valid, or
//! [`parse`] returns a [`CliError`]; of several valid verbs the first wins, and the rest are ignored. `--help` (`-h`,
//! `/?`) wins wherever it appears, even next to a bad argument. A `thinkvision://…` argument (how the OS opens a
//! link) counts as a verb; see [`parse_url`]. No verb gives [`CliCommand::Show`].
//!
//! `--display <name|id|all>` (or a `thinkvision://display/<name|id|all>/…` link) fills [`Parsed::display`], kept as
//! typed; the first one wins. It needs a verb: with none it is a [`CliError`], and `--help` and `--version` ignore
//! it. The app resolves it with [`resolve_display`] against the attached displays, runs the verb on each target and
//! exits with [`combined_exit_code`] of the per-target [`exit_code`]s.

use crate::result::ResultKind;

const MAX_STEP: i32 = 100;

// The first name per choice is the canonical one `format` writes. Kinds match a display's input kinds
// (`InputOption::kind`). The slots `1..=MAX_SLOT` are parsed apart.
const INPUTS: [(&str, &str); 6] =
    [("hdmi", "hdmi"), ("dp", "dp"), ("displayport", "dp"), ("usbc", "usbc"), ("usb-c", "usbc"), ("this", "")];

/// Which input `--input` asks for; each display resolves it against its own inputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputChoice {
    /// This PC's input (`ThisMachineInput`, per display).
    This,
    /// The display's n-th input button, 1-based.
    Slot(u8),
    /// An input kind: "hdmi", "dp" or "usbc".
    Kind(String),
}

const MAX_SLOT: u8 = 3;

fn input_choice(kind: &str) -> InputChoice {
    if kind.is_empty() { InputChoice::This } else { InputChoice::Kind(kind.into()) }
}
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CliCommand {
    /// Open the window.
    #[default]
    Show,
    Preset { night: bool },
    Input(InputChoice),
    Step { code: u8, delta: i32 },
    /// Read and print.
    Get(GetTarget),
    /// Print [`help`].
    Help,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
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
    /// `--display <name|id|all>`: which display a verb acts on; None = the default target. Resolved by the app
    /// against the attached displays (w4-cli parses it).
    pub display: Option<String>,
}

/// A malformed command line; exit code 2, with [`usage_error`] on stderr.
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
                (None, _) if matches!(name.as_str(), "preset" | "input" | "step" | "get" | "display") => {
                    let value = inline.or_else(|| {
                        let next = args.get(i)?.as_ref();
                        (!next.starts_with('-') && next != "/?").then(|| {
                            i += 1;
                            next
                        })
                    });
                    match value.map(str::trim).filter(|v| !v.is_empty()) {
                        Some(v) if name == "display" => {
                            parsed.display.get_or_insert_with(|| v.to_string());
                            Ok(None)
                        }
                        Some(v) => parse_verb(&name, v).map(|c| Some((c, None))),
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
            Ok(Some((verb, display))) if command.is_none() => {
                command = Some(verb);
                if let Some(display) = display {
                    parsed.display.get_or_insert(display);
                }
            }
            Ok(_) => {}
            Err(message) => error = error.or(Some(message)),
        }
    }
    parsed.command = match (help, error) {
        (true, _) => CliCommand::Help,
        (false, Some(message)) => return Err(CliError { message }),
        (false, None) => command.unwrap_or_default(),
    };
    if let Some(display) = parsed.display.as_ref().filter(|_| parsed.command == CliCommand::Show && !parsed.version) {
        return Err(CliError { message: format!("'--display {display}' needs a verb (--preset, --input, --step or --get)") });
    }
    Ok(parsed)
}

/// `thinkvision-control <version>`, for `--version`.
pub fn version_line() -> String {
    format!("thinkvision-control {}", env!("CARGO_PKG_VERSION"))
}

/// What a [`CliError`] prints: its message and one line pointing at `--help` (not the whole usage).
pub fn usage_error(e: &CliError) -> String {
    format!("{}\nRun with --help for usage.", e.message)
}

/// The arguments as given, for the log: one that is empty or holds a space or a quote is quoted.
pub fn command_line<S: AsRef<str>>(args: &[S]) -> String {
    let quote = |a: &str| {
        if a.is_empty() || a.contains(|c: char| c.is_whitespace() || c == '"') {
            format!("\"{}\"", a.replace('"', "\\\""))
        } else {
            a.to_string()
        }
    };
    args.iter().map(|a| quote(a.as_ref())).collect::<Vec<_>>().join(" ")
}

/// A `thinkvision:` link, case-insensitive, with or without `//` and a trailing `/`:
/// `open`, `preset/day|night`, `input/hdmi|dp|displayport|usbc|usb-c|this|1|2|3`, `step/brightness|contrast/+N|-N`,
/// `get/brightness|contrast|input|colour-mode|color-mode|all`. Any but `open` may follow `display/<name|id|all>/`
/// (percent-encoded, case kept), which [`parse`] puts in [`Parsed::display`]; this drops it. None when it isn't one
/// of these.
pub fn parse_url(url: &str) -> Option<CliCommand> {
    url_command(url).ok().map(|(command, _)| command)
}

fn url_command(url: &str) -> Result<(CliCommand, Option<String>), String> {
    let unknown = || format!("'{}' is not a known thinkvision: link (see --help)", url.trim());
    if !is_url(url) {
        return Err(format!("'{url}' is not a thinkvision: link"));
    }
    let mut path = url.trim()["thinkvision:".len()..].trim_start_matches('/').trim_end_matches('/');
    let mut display = None;
    if let Some((_, rest)) = path.split_once('/').filter(|(head, _)| head.eq_ignore_ascii_case("display")) {
        let (name, rest) = rest.split_once('/').ok_or_else(unknown)?;
        let name = percent_decode(name).ok_or_else(unknown)?;
        display = Some(name.trim().to_string()).filter(|n| !n.is_empty());
        if display.is_none() {
            return Err(unknown());
        }
        path = rest;
    }
    let lower = path.to_lowercase().replace("%2b", "+");
    let parts: Vec<&str> = lower.split('/').collect();
    let command = match parts[..] {
        ["" | "open"] if display.is_none() => CliCommand::Show,
        ["preset" | "input" | "get", value] => parse_verb(parts[0], value)?,
        ["step", target, delta] => parse_verb("step", &format!("{target}={delta}"))?,
        _ => return Err(unknown()),
    };
    Ok((command, display))
}

/// Decodes `%XX` escapes as UTF-8; None when malformed.
fn percent_decode(text: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(text.len());
    let mut rest = text.as_bytes();
    while let [first, tail @ ..] = rest {
        if *first == b'%' {
            let hex = std::str::from_utf8(tail.get(..2)?).ok()?;
            bytes.push(u8::from_str_radix(hex, 16).ok()?);
            rest = &tail[2..];
        } else {
            bytes.push(*first);
            rest = tail;
        }
    }
    String::from_utf8(bytes).ok()
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
        CliCommand::Input(ref choice) => match choice {
            InputChoice::Slot(n) if (1..=MAX_SLOT).contains(n) => args("--input", n.to_string()),
            InputChoice::Slot(_) => None,
            _ => INPUTS.iter().find(|(_, k)| input_choice(k) == *choice).and_then(|(n, _)| args("--input", n.to_string())),
        },
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

/// The exit code of a verb run on several displays: 0 only when every target succeeded, else the first non-zero
/// code in target order. `codes` holds each target's [`exit_code`].
pub fn combined_exit_code(codes: &[i32]) -> i32 {
    codes.iter().copied().find(|&code| code != 0).unwrap_or(0)
}

/// The ids `--display <wanted>` targets, from `displays` as `(id, name)` pairs. `all` (any case) gives every id; an
/// exact id match wins; otherwise `wanted` must equal exactly one name, ignoring case. An ambiguous name, or none,
/// is a [`CliError`] listing the candidates, or the attached displays, as "name (id)".
pub fn resolve_display<'a>(wanted: &str, displays: &'a [(String, String)]) -> Result<Vec<&'a str>, CliError> {
    let wanted = wanted.trim();
    if wanted.eq_ignore_ascii_case("all") {
        return Ok(displays.iter().map(|(id, _)| id.as_str()).collect());
    }
    if let Some((id, _)) = displays.iter().find(|(id, _)| id == wanted) {
        return Ok(vec![id.as_str()]);
    }
    let lower = wanted.to_lowercase();
    let matches: Vec<_> = displays.iter().filter(|(_, name)| name.to_lowercase() == lower).collect();
    fn list<'b>(displays: impl Iterator<Item = &'b (String, String)>) -> String {
        displays.map(|(id, name)| format!("{name} ({id})")).collect::<Vec<_>>().join(", ")
    }
    let message = match matches[..] {
        [(id, _)] => return Ok(vec![id.as_str()]),
        [] if displays.is_empty() => format!("no display is called '{wanted}': none is attached"),
        [] => format!("no display is called '{wanted}' (attached: {})", list(displays.iter())),
        _ => format!("'{wanted}' matches several displays, give an id: {}", list(matches.into_iter())),
    };
    Err(CliError { message })
}

/// Usage text for `--help`.
pub fn help() -> &'static str {
    "ThinkVision Control

Usage: thinkvision-control [verb] [--display name|id|all] [--json] [--yes] [--startup]
On Windows, scripts should run tvc (tvc.exe), the console launcher, with the same arguments.

Verbs (the first one counts):
  --preset day|night                 Apply the Day or Night preset
  --input hdmi|dp|usbc|this|1|2|3    Switch the input (this = this PC's input,
                                     1-3 = the display's first to third input)
  --step brightness|contrast=+N|-N   Change brightness or contrast by N (up to 100)
  --get brightness|contrast|input|colour-mode|all
                                     Print current values
  --help, -h, /?                     Show this text
  --version                          Print the version
  (none)                             Open the window

Flags:
  --display name|id|all   Act on one display (an id, or a name in any case that
                          matches exactly one display) or on all of them.
                          Without it, the verb acts on the default display.
  --json      Print the result as JSON
  --yes       Confirm an --input switch without asking on screen
  --startup   Start in the tray without a window

--get on several displays prints a JSON array with one object per display;
on one display it prints a single object, as before.

Exit codes: 0 OK, 1 monitor not found, 2 bad arguments or value out of range,
            3 input switch needs --yes, 4 not supported,
            5 monitor not answering (or asleep, or safe mode), 8 timed out.
            With several displays: 0 only if every display succeeded, else
            the code of the first one that failed.

Links (thinkvision://):
  thinkvision://open
  thinkvision://preset/day             thinkvision://preset/night
  thinkvision://input/hdmi             (also dp, usbc, this)
  thinkvision://step/brightness/+10    thinkvision://step/contrast/-5
  thinkvision://get/all
  thinkvision://display/<name|id|all>/step/brightness/+5   (any link but open)
"
}

/// The accepted values of a verb, for error messages.
fn choices(verb: &str) -> &'static str {
    match verb {
        "preset" => "day, night",
        "input" => "hdmi, dp, usbc, this, 1, 2, 3",
        "display" => "a display name, id or all",
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
        "input" => match lower.parse::<u8>() {
            Ok(n) if (1..=MAX_SLOT).contains(&n) => Some(CliCommand::Input(InputChoice::Slot(n))),
            _ => INPUTS.iter().find(|(n, _)| *n == lower).map(|(_, k)| CliCommand::Input(input_choice(k))),
        },
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
