//! A marker file around risky monitor operations (capability read, finder snapshot/compare). The engine writes
//! it before the operation and removes it after; a marker still there whose owner is no longer running means that
//! owner died inside one, so the app starts in safe mode.
//!
//! Each owner has its own file, `risky-operation-<key>.txt`, so one process never reads or clears another's.
//! The key is `pid-<pid>` for now. 0.0.8 wrote a single `risky-operation.txt`; it counts as stale.
//!
//! There is deliberately no drop guard: a panic unwinding out of the operation is exactly the crash to remember.

use std::fs::{self, File};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

const PREFIX: &str = "risky-operation";

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

impl Marker {
    pub fn new(dir: impl Into<PathBuf>, key: &str) -> Self {
        Self { path: dir.into().join(format!("{PREFIX}-{key}.txt")) }
    }

    /// Every marker in `dir` whose owner is gone. `is_alive(pid)` answers for `pid-<pid>` keys; other keys and the
    /// 0.0.8 file are always stale. This process's own marker is never returned.
    pub fn left_over(dir: &Path, is_alive: impl Fn(u32) -> bool) -> Vec<LeftOver> {
        let own = process_key();
        let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
        let mut found: Vec<LeftOver> = entries.flatten().filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let key = if name == format!("{PREFIX}.txt") {
                String::new()
            } else {
                name.strip_prefix(&format!("{PREFIX}-"))?.strip_suffix(".txt")?.to_string()
            };
            if key == own {
                return None;
            }
            if let Some(pid) = key.strip_prefix("pid-").and_then(|p| p.parse::<u32>().ok()) {
                if is_alive(pid) {
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
