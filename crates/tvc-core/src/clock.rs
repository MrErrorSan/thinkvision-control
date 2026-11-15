//! Local wall-clock time for file stamps and log lines. The core makes no OS calls, so the app hands it a function
//! that reads the current UTC offset ([`set_utc_offset_source`]), asked on every call so a DST change or a new time
//! zone shows up without a restart. Without one, stamps are UTC.

use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

static OFFSET_SOURCE: OnceLock<fn() -> i32> = OnceLock::new();

/// Reads the offset from UTC in seconds (east positive, e.g. 18000 for UTC+5) from `source` on every call from now on. The first source set wins.
pub fn set_utc_offset_source(source: fn() -> i32) {
    let _ = OFFSET_SOURCE.set(source);
}

fn utc_offset() -> i32 {
    OFFSET_SOURCE.get().map_or(0, |source| source())
}

/// Local civil time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DateTime {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub millis: u32,
}

pub(crate) fn now() -> DateTime {
    let since = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    from_unix_millis(since.as_millis() as i64 + i64::from(utc_offset()) * 1000)
}

pub(crate) fn from_unix_millis(ms: i64) -> DateTime {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400) as u32;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    DateTime { year, month, day, hour: tod / 3600, minute: tod / 60 % 60, second: tod % 60, millis: ms.rem_euclid(1000) as u32 }
}
