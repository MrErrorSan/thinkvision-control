//! a marker file around risky monitor operations (capability read, finder snapshot/compare). The engine writes
//! it before the operation and removes it after; a file still there at the next start means the app died inside one,
//! so the app starts in safe mode.
//!
//! There is deliberately no drop guard: a panic unwinding out of the operation is exactly the crash to remember.

use std::fs::{self, File};
use std::io::{ErrorKind, Write};
use std::path::PathBuf;

const FILE: &str = "risky-operation.txt";

/// `risky-operation.txt` in the data directory, holding the name of the operation in progress.
#[derive(Clone, Debug)]
pub struct Marker {
    path: PathBuf,
}

impl Marker {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { path: dir.into().join(FILE) }
    }

    /// The operation the last run never finished ("capability read", "finder snapshot"…), if any. An unreadable
    /// marker counts as left over, with an empty name.
    pub fn left_over(&self) -> Option<String> {
        match fs::read_to_string(&self.path) {
            Ok(text) => Some(text.trim().to_string()),
            Err(e) if e.kind() == ErrorKind::NotFound => None,
            Err(_) => Some(String::new()),
        }
    }

    /// Removes the marker.
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
