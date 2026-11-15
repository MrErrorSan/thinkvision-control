//! `tvc`: runs `thinkvision-control` from the same folder with the same arguments, waits for it and exits with
//! its exit code. The app is a GUI-subsystem exe on Windows, so cmd and PowerShell don't wait for it and lose its
//! output and exit code; this console-subsystem exe does wait. The child inherits stdin, stdout and stderr.

use std::process::{exit, Command};

const APP: &str = if cfg!(windows) { "thinkvision-control.exe" } else { "thinkvision-control" };

/// Exit code when the app can't be started (the shells' "command not found").
const EXIT_NO_APP: i32 = 127;

fn main() {
    let app = match std::env::current_exe() {
        Ok(exe) => exe.with_file_name(APP),
        Err(e) => {
            eprintln!("tvc: can't find its own location: {e}");
            exit(EXIT_NO_APP);
        }
    };
    match Command::new(&app).args(std::env::args_os().skip(1)).status() {
        // None: ended by a signal (Unix).
        Ok(status) => exit(status.code().unwrap_or(1)),
        Err(e) => {
            eprintln!("tvc: can't start {}: {e}", app.display());
            exit(EXIT_NO_APP);
        }
    }
}
