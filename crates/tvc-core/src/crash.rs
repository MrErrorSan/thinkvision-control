//! A marker file around risky monitor operations (capability read, finder snapshot/compare). The engine writes
//! it before the operation and removes it after; a marker still there whose owner is no longer running means that
//! owner died inside one, so the app starts in safe mode.
//!
//! Each owner has its own file, `risky-operation-<key>.txt`, so one process never reads or clears another's.
//! The key is `pid-<pid>`, or `pid-<pid>-<name>` when one process keeps several markers in one folder (the
//! unstable `-N` monitors share [`SESSION_MARKERS`]). 0.0.8 wrote a single `risky-operation.txt`; it counts as stale.
//!
//! There is deliberately no drop guard: a panic unwinding out of the operation is exactly the crash to remember.

use std::fs::{self, File};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

const PREFIX: &str = "risky-operation";

/// The folder in `monitors/` that holds the crash markers of monitors with unstable (`-N`) ids, whose other state
/// lives in a temporary folder: kept here, the next start finds them like every other monitor's.
pub const SESSION_MARKERS: &str = "_session";

/// This owner's marker file.
#[derive(Clone, Debug)]
pub struct Marker {
    path: PathBuf,
}

/// A marker left by an owner that is no longer running.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeftOver {
    /// "" for the 0.0.8 file.
    pub key: String,
    /// The operation it never finished ("capability read"…); "" when unreadable.
    pub operation: String,
    path: PathBuf,
}

impl LeftOver {
    pub fn clear(&self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// `pid-<pid>` for this process.
pub fn process_key() -> String {
    format!("pid-{}", std::process::id())
}

/// The owner's process id: `pid-<pid>` or `pid-<pid>-<name>`. None for other keys.
fn key_pid(key: &str) -> Option<u32> {
    let rest = key.strip_prefix("pid-")?;
    rest.split_once('-').map_or(rest, |(pid, _)| pid).parse().ok()
}

impl Marker {
    pub fn new(dir: impl Into<PathBuf>, key: &str) -> Self {
        Self { path: dir.into().join(format!("{PREFIX}-{key}.txt")) }
    }

    /// Every marker in `dir` whose owner is gone. `is_alive(pid)` answers for `pid-<pid>` (and `pid-<pid>-<name>`)
    /// keys; other keys and the 0.0.8 file are always stale. This process's own markers are never returned.
    pub fn left_over(dir: &Path, is_alive: impl Fn(u32) -> bool) -> Vec<LeftOver> {
        let own = std::process::id();
        let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
        let mut found: Vec<LeftOver> = entries.flatten().filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let key = if name == format!("{PREFIX}.txt") {
                String::new()
            } else {
                name.strip_prefix(&format!("{PREFIX}-"))?.strip_suffix(".txt")?.to_string()
            };
            if let Some(pid) = key_pid(&key) {
                if pid == own || is_alive(pid) {
                    return None;
                }
            }
            let operation = match fs::read_to_string(entry.path()) {
                Ok(text) => text.trim().to_string(),
                Err(e) if e.kind() == ErrorKind::NotFound => return None,
                Err(_) => String::new(),
            };
            Some(LeftOver { key, operation, path: entry.path() })
        }).collect();
        found.sort_by(|a, b| a.key.cmp(&b.key));
        found
    }

    /// Removes this owner's marker.
    pub fn clear(&self) {
        let _ = fs::remove_file(&self.path);
    }

    /// Removes every marker of this process in `dir`: `pid-<pid>` and each `pid-<pid>-<name>`.
    pub fn clear_process(dir: &Path) {
        let own = std::process::id();
        let Ok(entries) = fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let key = name.strip_prefix(&format!("{PREFIX}-")).and_then(|n| n.strip_suffix(".txt"));
            if key.and_then(key_pid) == Some(own) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    /// Writes the marker before `operation`, flushed to disk so even a system crash leaves it behind.
    pub(crate) fn arm(&self, operation: &str) {
        if let Some(dir) = self.path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(mut file) = File::create(&self.path) {
            let _ = file.write_all(operation.as_bytes()).and_then(|()| file.sync_all());
        }
    }

    /// Removes the marker after the operation finished.
    pub(crate) fn disarm(&self) {
        self.clear();
    }
}
