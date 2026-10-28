//! sunrise/sunset and the automatic Day/Night phase. Reference times: sunrise-sunset.org (UTC).

use crate::schedule::{current, AutoMode, Phase};
use crate::solar::{days_from_civil, sun_times};

const LONDON: (f64, f64) = (51.5074, -0.1278);
const LAHORE: (f64, f64) = (31.5204, 74.3587);
const SYDNEY: (f64, f64) = (-33.8688, 151.2093);
const NEW_YORK: (f64, f64) = (40.7128, -74.0060);
const TROMSO: (f64, f64) = (69.6492, 18.9553);
const HOUR: i32 = 3600;

/// Unix seconds for a UTC date and time.
fn utc(year: i64, month: u32, day: u32, hour: i64, minute: i64) -> i64 {
    days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60
}

/// Within 2 minutes, across the midnight wrap.
fn assert_near(actual: u32, expected_minutes: f64, what: &str) {
    let diff = (f64::from(actual) - expected_minutes).rem_euclid(1440.0);
    assert!(diff.min(1440.0 - diff) <= 2.0, "{what}: {actual} vs {expected_minutes}");
}

#[test]
fn days_from_civil_matches_the_epoch() {
    assert_eq!(days_from_civil(1970, 1, 1), 0);
    assert_eq!(days_from_civil(2000, 3, 1), 11_017);
    assert_eq!(days_from_civil(2026, 10, 28), 20_754);
    assert_eq!(days_from_civil(1969, 12, 31), -1);
}

#[test]
fn sun_times_match_known_cities() {
    let cases = [
        ("London midsummer", 2026, 6, 21, LONDON, 3.0 * 60.0 + 43.07, 20.0 * 60.0 + 21.6),
        ("Lahore midwinter", 2026, 12, 21, LAHORE, 60.0 + 57.67, 12.0 * 60.0 + 3.37),
        // Sunrise is 18:59 UTC the evening before; wrapped into the day.
        ("Sydney summer", 2026, 1, 15, SYDNEY, 18.0 * 60.0 + 59.65, 9.0 * 60.0 + 9.17),
        // Sunset is 00:30 UTC the next day; wrapped.
        ("New York midsummer", 2026, 6, 21, NEW_YORK, 9.0 * 60.0 + 25.02, 30.77),
    ];
    for (what, year, month, day, (lat, lon), rise, set) in cases {
        let (r, s) = sun_times(year, month, day, lat, lon).unwrap_or_else(|| panic!("{what}: no sun times"));
        assert_near(r, rise, what);
        assert_near(s, set, what);
    }
}

#[test]
fn sun_times_none_for_polar_day_night_and_bad_input() {
    assert_eq!(sun_times(2026, 6, 21, TROMSO.0, TROMSO.1), None); // midnight sun
    assert_eq!(sun_times(2026, 12, 21, TROMSO.0, TROMSO.1), None); // polar night
    assert_eq!(sun_times(2026, 6, 21, 90.0, 0.0), None);
    assert_eq!(sun_times(2026, 13, 1, 0.0, 0.0), None);
    assert_eq!(sun_times(2026, 6, 21, 91.0, 0.0), None);
    assert_eq!(sun_times(2026, 6, 21, 0.0, f64::NAN), None);
}

#[test]
fn off_has_no_phase() {
    assert_eq!(current(&AutoMode::Off, utc(2026, 10, 28, 12, 0), 0), None);
}

#[test]
fn times_pick_the_latest_event_looking_back_into_yesterday() {
    let mode = AutoMode::Times { day: 7 * 60, night: 19 * 60 };
    let offset = 5 * HOUR; // local = UTC + 5
    let local = |h: i64, m: i64| utc(2026, 10, 28, h, m) - i64::from(offset);
    let cases = [((0, 0), Phase::Night), ((6, 59), Phase::Night), ((7, 0), Phase::Day), ((12, 0), Phase::Day),
        ((18, 59), Phase::Day), ((19, 0), Phase::Night), ((23, 59), Phase::Night)];
    for ((h, m), phase) in cases {
        assert_eq!(current(&mode, local(h, m), offset), Some(phase), "{h:02}:{m:02}");
    }
    assert_eq!(current(&mode, local(18, 59) + 59, offset), Some(Phase::Day));
}

#[test]
fn times_with_night_before_day() {
    let mode = AutoMode::Times { day: 7 * 60, night: 60 }; // night at 01:00
    let offset = -4 * HOUR;
    let local = |h: i64, m: i64| utc(2026, 3, 1, h, m) - i64::from(offset);
    let cases = [((0, 30), Phase::Day), ((1, 0), Phase::Night), ((6, 59), Phase::Night), ((7, 0), Phase::Day),
        ((23, 59), Phase::Day)];
    for ((h, m), phase) in cases {
        assert_eq!(current(&mode, local(h, m), offset), Some(phase), "{h:02}:{m:02}");
    }
}

#[test]
fn invalid_times_have_no_phase() {
    let now = utc(2026, 10, 28, 12, 0);
    assert_eq!(current(&AutoMode::Times { day: 420, night: 420 }, now, 0), None);
    assert_eq!(current(&AutoMode::Times { day: 1440, night: 60 }, now, 0), None);
}

#[test]
fn sun_follows_sunrise_and_sunset() {
    // Lahore 2026-12-21: sunrise 01:58 UTC, sunset 12:03 UTC; local UTC+5.
    let mode = AutoMode::Sun { lat: LAHORE.0, lon: LAHORE.1 };
    let offset = 5 * HOUR;
    let cases = [((1, 50), Phase::Night), ((2, 5), Phase::Day), ((11, 55), Phase::Day), ((12, 10), Phase::Night),
        ((20, 0), Phase::Night)];
    for ((h, m), phase) in cases {
        assert_eq!(current(&mode, utc(2026, 12, 21, h, m), offset), Some(phase), "{h:02}:{m:02} UTC");
    }
}

#[test]
fn sun_uses_real_instants_across_utc_days() {
    // Sydney 2026-01-15 (AEDT, UTC+11): sunrise 2026-01-14 18:59 UTC, sunset 2026-01-15 09:09 UTC.
    let sydney = AutoMode::Sun { lat: SYDNEY.0, lon: SYDNEY.1 };
    for offset in [11 * HOUR, 0] {
        assert_eq!(current(&sydney, utc(2026, 1, 14, 18, 50), offset), Some(Phase::Night), "{offset}");
        assert_eq!(current(&sydney, utc(2026, 1, 14, 19, 10), offset), Some(Phase::Day), "{offset}");
        assert_eq!(current(&sydney, utc(2026, 1, 15, 9, 0), offset), Some(Phase::Day), "{offset}");
        assert_eq!(current(&sydney, utc(2026, 1, 15, 9, 20), offset), Some(Phase::Night), "{offset}");
    }
    // New York 2026-06-21 (EDT, UTC-4): sunset 2026-06-22 00:31 UTC.
    let new_york = AutoMode::Sun { lat: NEW_YORK.0, lon: NEW_YORK.1 };
    assert_eq!(current(&new_york, utc(2026, 6, 22, 0, 20), -4 * HOUR), Some(Phase::Day));
    assert_eq!(current(&new_york, utc(2026, 6, 22, 0, 40), -4 * HOUR), Some(Phase::Night));
}

#[test]
fn sun_has_no_phase_on_a_polar_day() {
    let mode = AutoMode::Sun { lat: TROMSO.0, lon: TROMSO.1 };
    assert_eq!(current(&mode, utc(2026, 6, 21, 12, 0), 2 * HOUR), None);
    assert_eq!(current(&mode, utc(2026, 12, 21, 12, 0), HOUR), None);
}

#[test]
fn london_catches_up_after_sleeping_through_sunset() {
    // Asleep from 19:00 to 23:00 UTC on midsummer: the missed sunset (20:22) applies on wake.
    let mode = AutoMode::Sun { lat: LONDON.0, lon: LONDON.1 };
    assert_eq!(current(&mode, utc(2026, 6, 21, 19, 0), HOUR), Some(Phase::Day));
    assert_eq!(current(&mode, utc(2026, 6, 21, 23, 0), HOUR), Some(Phase::Night));
}
