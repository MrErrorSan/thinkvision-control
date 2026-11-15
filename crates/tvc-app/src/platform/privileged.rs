//! Runs a setup card's fix on a worker thread (Linux): the root steps through `pkexec /bin/sh -c`, so the desktop's
//! polkit password dialog appears once, then the user steps through `/bin/sh -c`. Other systems have no fixes.

#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use tvc_core::i18n::{tr, trf};

use super::setup::Fix;

/// How a fix run ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Done,
    /// The last step must wait for the next login (`gnome-extensions enable` just after an install).
    AfterLogin,
    /// The password prompt was closed.
    Cancelled,
    /// No polkit authentication agent runs in this session.
    NoAgent,
    /// pkexec isn't installed.
    NoPkexec,
    /// Wrong password, or polkit doesn't allow it.
    NotAllowed,
    /// A step failed: the last lines of its output.
    Failed(String),
}

impl Outcome {
    /// The card's result line and whether it's an error; None for Done (the app checks again instead). `still` is
    /// the fix's own "log out and back in" line, already translated.
    pub fn text(&self, still: &str) -> Option<(String, bool)> {
        Some(match self {
            Outcome::Done => return None,
            Outcome::AfterLogin => (still.into(), false),
            Outcome::Cancelled => (tr("You closed the password prompt."), false),
            Outcome::NoAgent => (
                tr("No password prompt is available on this desktop (no polkit agent). Run the command above in a \
                    terminal instead."),
                true,
            ),
            Outcome::NoPkexec => (
                tr("No password prompt is available (pkexec isn't installed). Run the command above in a terminal \
                    instead."),
                true,
            ),
            Outcome::NotAllowed => (tr("Authentication failed or isn't allowed."), true),
            Outcome::Failed(tail) if tail.is_empty() => (tr("The command failed."), true),
            Outcome::Failed(tail) => (trf("The command failed:\n{0}", &[tail]), true),
        })
    }
}

/// pkexec's exit: 126 the prompt was dismissed, 127 not authorized (or no agent); anything else is the script's.
/// The script's own 127 (a command not found) is told apart by pkexec's wording.
fn pkexec_outcome(code: Option<i32>, stdout: &str, stderr: &str) -> Outcome {
    match code {
        Some(0) => Outcome::Done,
        Some(126) => Outcome::Cancelled,
        Some(127) if stderr.contains("authentication agent") => Outcome::NoAgent,
        Some(127)
            if stderr.trim().is_empty()
                || stderr.contains("Not authorized")
                || stderr.contains("Error executing command as another user") =>
        {
            Outcome::NotAllowed
        }
        _ => Outcome::Failed(tail(code, stdout, stderr)),
    }
}

/// A user step's exit; with `after_login`, failing means it works after the next login.
fn user_outcome(code: Option<i32>, stdout: &str, stderr: &str, after_login: bool) -> Outcome {
    match code {
        Some(0) => Outcome::Done,
        _ if after_login => Outcome::AfterLogin,
        _ => Outcome::Failed(tail(code, stdout, stderr)),
    }
}

/// The last three non-empty lines of stderr (stdout when stderr is empty), else the exit code.
fn tail(code: Option<i32>, stdout: &str, stderr: &str) -> String {
    let text = if stderr.trim().is_empty() { stdout } else { stderr };
    let lines: Vec<&str> = text.lines().map(str::trim_end).filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return match code {
            Some(c) => format!("exit code {c}"),
            None => "it was stopped".into(),
        };
    }
    lines[lines.len().saturating_sub(3)..].join("\n")
}

/// Runs `fix`, blocking: call it on a worker thread.
#[cfg(target_os = "linux")]
pub fn run(fix: &Fix) -> Outcome {
    use std::io::ErrorKind;
    use std::process::{Command, Stdio};

    let output = |cmd: &mut Command| cmd.stdin(Stdio::null()).output();
    if let Some(script) = fix.root_script() {
        // The absolute path where it exists, so a PATH entry can't stand in for it.
        let pkexec = if std::path::Path::new("/usr/bin/pkexec").exists() { "/usr/bin/pkexec" } else { "pkexec" };
        let out = match output(Command::new(pkexec).args(["/bin/sh", "-c", &script])) {
            Ok(out) => out,
            Err(e) if e.kind() == ErrorKind::NotFound => return Outcome::NoPkexec,
            Err(e) => return Outcome::Failed(e.to_string()),
        };
        let outcome = pkexec_outcome(
            out.status.code(),
            &String::from_utf8_lossy(&out.stdout),
            &String::from_utf8_lossy(&out.stderr),
        );
        if outcome != Outcome::Done {
            return outcome;
        }
    }
    for (script, after_login) in fix.user_scripts() {
        let outcome = match output(Command::new("/bin/sh").args(["-c", &script])) {
            Ok(out) => user_outcome(
                out.status.code(),
                &String::from_utf8_lossy(&out.stdout),
                &String::from_utf8_lossy(&out.stderr),
                after_login,
            ),
            Err(e) => Outcome::Failed(e.to_string()),
        };
        if outcome != Outcome::Done {
            return outcome;
        }
    }
    Outcome::Done
}

/// Other systems have no fixes.
#[cfg(not(target_os = "linux"))]
pub fn run(_fix: &Fix) -> Outcome {
    Outcome::NoPkexec
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkexec_exit_codes() {
        assert_eq!(pkexec_outcome(Some(0), "", ""), Outcome::Done);
        assert_eq!(pkexec_outcome(Some(126), "", "Error executing command as another user: Request dismissed"),
            Outcome::Cancelled);
        assert_eq!(
            pkexec_outcome(Some(127), "", "Error executing command as another user: No authentication agent found."),
            Outcome::NoAgent
        );
        assert_eq!(pkexec_outcome(Some(127), "", "Error executing command as another user: Not authorized"),
            Outcome::NotAllowed);
        assert_eq!(pkexec_outcome(Some(127), "", ""), Outcome::NotAllowed);
    }

    #[test]
    fn script_failures_keep_the_last_lines() {
        assert_eq!(pkexec_outcome(Some(127), "", "sh: 1: udevadm: not found\n"),
            Outcome::Failed("sh: 1: udevadm: not found".into()));
        assert_eq!(pkexec_outcome(Some(1), "", "a\nb\n\nc\nd\n"), Outcome::Failed("b\nc\nd".into()));
        assert_eq!(pkexec_outcome(Some(100), "E: Unable to locate package\n", ""),
            Outcome::Failed("E: Unable to locate package".into()));
        assert_eq!(pkexec_outcome(Some(2), "", ""), Outcome::Failed("exit code 2".into()));
        assert_eq!(pkexec_outcome(None, "", ""), Outcome::Failed("it was stopped".into()));
    }

    #[test]
    fn user_steps() {
        assert_eq!(user_outcome(Some(0), "", "", true), Outcome::Done);
        assert_eq!(user_outcome(Some(2), "", "Extension does not exist", true), Outcome::AfterLogin);
        assert_eq!(
            user_outcome(Some(1), "", "Failed to restart\n", false),
            Outcome::Failed("Failed to restart".into())
        );
    }

    #[test]
    fn result_lines() {
        assert_eq!(Outcome::Done.text("x"), None);
        assert_eq!(Outcome::AfterLogin.text("Log out"), Some(("Log out".into(), false)));
        assert_eq!(Outcome::Cancelled.text(""), Some(("You closed the password prompt.".into(), false)));
        assert!(Outcome::NoAgent.text("").is_some_and(|(t, e)| e && t.contains("no polkit agent")));
        assert!(Outcome::NoPkexec.text("").is_some_and(|(t, e)| e && t.contains("in a terminal")));
        assert_eq!(Outcome::NotAllowed.text(""), Some(("Authentication failed or isn't allowed.".into(), true)));
        assert_eq!(Outcome::Failed("boom".into()).text(""), Some(("The command failed:\nboom".into(), true)));
    }
}
