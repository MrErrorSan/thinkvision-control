//! Append-only `log.txt` in the data directory, rolled to `log.1.txt` past 1 MB. Never panics, never fails.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MAX_BYTES: u64 = 1024 * 1024;

static DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

thread_local! {
    static PREFIX: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// Logs go to `dir` from now on; before this, [`write`] does nothing.
pub fn init(dir: impl Into<PathBuf>) {
    let dir = dir.into();
    let _ = std::fs::create_dir_all(&dir);
    *DIR.lock().unwrap_or_else(|e| e.into_inner()) = Some(dir);
}

/// Every later [`write`] on this thread starts with `prefix` (an engine's `[id] `).
pub fn set_prefix(prefix: impl Into<String>) {
    PREFIX.with(|p| *p.borrow_mut() = prefix.into());
}

/// Appends `yyyy-MM-dd HH:mm:ss.fff message`, after this thread's prefix.
pub fn write(message: &str) {
    let guard = DIR.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(dir) = guard.as_deref() {
        PREFIX.with(|p| match p.borrow().as_str() {
            "" => write_to(dir, message),
            prefix => write_to(dir, &format!("{prefix}{message}")),
        });
    }
}

pub(crate) fn write_to(dir: &Path, message: &str) {
    let path = dir.join("log.txt");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = std::fs::rename(&path, dir.join("log.1.txt"));
    }
    let t = crate::clock::now();
    let line = format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03} {message}\n",
        t.year, t.month, t.day, t.hour, t.minute, t.second, t.millis);
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = file.write_all(line.as_bytes());
    }
}
