//! sunrise and sunset from NOAA's published solar-position formulas (General Solar Position Calculations,
//! NOAA Global Monitoring Division), standard zenith 90.833° (refraction plus the sun's radius). Good to about a
//! minute between the polar circles.

/// Sunrise and sunset on a civil date at a place (latitude north positive, longitude east positive), in minutes
/// after 00:00 UTC, wrapped into `0..1440`. Far from Greenwich an event can fall on the neighbouring UTC day
/// (Sydney's sunrise is the evening before in UTC). None for polar day or night, or an invalid date or place.
pub fn sun_times(year: i32, month: u32, day: u32, lat: f64, lon: f64) -> Option<(u32, u32)> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let (rise, set) = events(days_from_civil(i64::from(year), month, day), lat, lon)?;
    Some((rise.rem_euclid(1440) as u32, set.rem_euclid(1440) as u32))
}

/// Sunrise and sunset of the civil date `days` (days since 1970-01-01) at the place, in minutes after that date's
/// 00:00 UTC, not wrapped: negative or past 1440 when the event falls on the neighbouring UTC day.
pub(crate) fn events(days: i64, lat: f64, lon: f64) -> Option<(i64, i64)> {
    let valid = lat.is_finite() && lon.is_finite() && (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon);
    if !valid {
        return None;
    }
    // First pass at local solar noon, then once more at each event's own time.
    let noon = 720.0 - 4.0 * lon;
    let rise = event_minute(days, lat, lon, event_minute(days, lat, lon, noon, -1.0)?, -1.0)?;
    let set = event_minute(days, lat, lon, event_minute(days, lat, lon, noon, 1.0)?, 1.0)?;
    Some((rise.round() as i64, set.round() as i64))
}

/// Sunrise (`sign` -1) or sunset (+1) in minutes after 00:00 UTC, with the sun's position taken at `at` minutes.
fn event_minute(days: i64, lat: f64, lon: f64, at: f64, sign: f64) -> Option<f64> {
    let julian_day = 2_440_587.5 + days as f64 + at / 1440.0;
    let t = (julian_day - 2_451_545.0) / 36_525.0; // Julian centuries since J2000.0

    let mean_long = (280.466_46 + t * (36_000.769_83 + t * 0.000_303_2)).rem_euclid(360.0);
    let mean_anom = 357.529_11 + t * (35_999.050_29 - 0.000_153_7 * t);
    let ecc = 0.016_708_634 - t * (0.000_042_037 + 0.000_000_126_7 * t);
    let m = mean_anom.to_radians();
    let centre = m.sin() * (1.914_602 - t * (0.004_817 + 0.000_014 * t))
        + (2.0 * m).sin() * (0.019_993 - 0.000_101 * t)
        + (3.0 * m).sin() * 0.000_289;
    let omega = (125.04 - 1934.136 * t).to_radians();
    let apparent_long = (mean_long + centre - 0.005_69 - 0.004_78 * omega.sin()).to_radians();
    let mean_obliq = 23.0 + (26.0 + (21.448 - t * (46.815 + t * (0.000_59 - t * 0.001_813))) / 60.0) / 60.0;
    let obliq = (mean_obliq + 0.002_56 * omega.cos()).to_radians();
    let decl = (obliq.sin() * apparent_long.sin()).asin();

    let y = (obliq / 2.0).tan().powi(2);
    let l0 = mean_long.to_radians();
    let eq_of_time = 4.0
        * (y * (2.0 * l0).sin() - 2.0 * ecc * m.sin() + 4.0 * ecc * y * m.sin() * (2.0 * l0).cos()
            - 0.5 * y * y * (4.0 * l0).sin()
            - 1.25 * ecc * ecc * (2.0 * m).sin())
        .to_degrees();

    let phi = lat.to_radians();
    let cos_hour_angle = 90.833_f64.to_radians().cos() / (phi.cos() * decl.cos()) - phi.tan() * decl.tan();
    if !(-1.0..=1.0).contains(&cos_hour_angle) {
        return None; // the sun stays up (< -1) or down (> 1) all day
    }
    let hour_angle = cos_hour_angle.acos().to_degrees();
    Some(720.0 - 4.0 * lon - eq_of_time + sign * 4.0 * hour_angle)
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's days_from_civil).
pub(crate) fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}
