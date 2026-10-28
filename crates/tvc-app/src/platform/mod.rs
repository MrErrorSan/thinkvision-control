//! OS integration: data folder, single instance, autostart, system events (display, lock, power), efficiency mode,
//! taskbar theme, setup hints, global hotkeys, monitor wake, console attach and the `thinkvision://` scheme. One
//! module per concern, `#[cfg(target_os)]` inside each.

pub mod autostart;
pub mod console;
pub mod efficiency;
pub mod hotkeys;
pub mod instance;
pub mod monitor_events;
pub mod setup;
pub mod taskbar;
pub mod url_scheme;
pub mod wake;

use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

/// The 0.0.4 data folder, so settings carry over: `%APPDATA%\ThinkVisionControl` on Windows,
/// `$XDG_CONFIG_HOME/ThinkVisionControl` or `~/.config/ThinkVisionControl` on Linux and macOS.
pub fn data_dir() -> PathBuf {
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join("AppData").join("Roaming"));
    #[cfg(not(windows))]
    let base = config_home();
    base.join("ThinkVisionControl")
}

fn home() -> PathBuf {
    std::env::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// `$XDG_CONFIG_HOME` when set to an absolute path, else `~/.config` (.NET's ApplicationData on Unix).
#[cfg(not(windows))]
fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(".config"))
}

/// Calls `f` once, `delay` after the last of a burst of `trigger`s, on its own thread. Dropping it stops the thread.
pub(crate) struct Debouncer {
    tx: mpsc::Sender<()>,
}

impl Debouncer {
    pub fn new(delay: Duration, f: impl Fn() + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel::<()>();
        let _ = thread::Builder::new().name("tvc-debounce".into()).spawn(move || {
            while rx.recv().is_ok() {
                loop {
                    match rx.recv_timeout(delay) {
                        Ok(()) => {}
                        Err(RecvTimeoutError::Timeout) => {
                            f();
                            break;
                        }
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
            }
        });
        Self { tx }
    }

    pub fn trigger(&self) {
        let _ = self.tx.send(());
    }
}

/// NUL-terminated UTF-16 for Win32 `PCWSTR` parameters.
#[cfg(windows)]
pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn debouncer_coalesces_a_burst() {
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        // Wide margins: Windows sleeps overshoot by tens of ms when the machine is busy.
        let d = Debouncer::new(Duration::from_millis(400), move || {
            c.fetch_add(1, Ordering::SeqCst);
        });
        for _ in 0..5 {
            d.trigger();
            thread::sleep(Duration::from_millis(10));
        }
        thread::sleep(Duration::from_millis(1000));
        assert_eq!(count.load(Ordering::SeqCst), 1);
        d.trigger();
        thread::sleep(Duration::from_millis(1000));
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn data_dir_ends_with_app_folder() {
        assert!(data_dir().ends_with("ThinkVisionControl"));
    }
}
