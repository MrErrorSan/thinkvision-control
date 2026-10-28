//! automatic Day/Night. The app asks for [`current`] every 60 s and on wake, and applies the preset only when
//! the phase changes, so a manual change in between sticks.

use crate::solar;

/// When to switch between the Day and Night presets.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum AutoMode {
    #[default]
    Off,
    /// Fixed local times, in minutes after local midnight (`0..1440`).
    Times { day: u16, night: u16 },
    /// Sunrise and sunset at a typed place (latitude north positive, longitude east positive).
    Sun { lat: f64, lon: f64 },
}

/// Which preset the schedule wants now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Phase {
    Day,
    Night,
}

/// The phase of the latest scheduled event at or before `now` (looking back into yesterday), so a missed event
/// after sleep still applies. None when off, when the times are invalid or equal, or on a day without sunrise or
/// sunset (polar day or night).
pub fn current(mode: &AutoMode, now_utc_secs: i64, utc_offset_secs: i32) -> Option<Phase> {
    let local = now_utc_secs + i64::from(utc_offset_secs);
    match *mode {
        AutoMode::Off => None,
        AutoMode::Times { day, night } => {
            if day >= 1440 || night >= 1440 || day == night {
                return None;
            }
            let minute = local.rem_euclid(86_400) / 60;
            // An event later today last happened yesterday.
            let last = |at: u16| if i64::from(at) <= minute { i64::from(at) } else { i64::from(at) - 1440 };
            Some(if last(day) > last(night) { Phase::Day } else { Phase::Night })
        }
        AutoMode::Sun { lat, lon } => {
            let today = local.div_euclid(86_400);
            solar::events(today, lat, lon)?;
            // Events carry their real UTC instant; neighbouring dates cover a time zone far from the place.
            (today - 2..=today + 1)
                .filter_map(|date| solar::events(date, lat, lon).map(|(rise, set)| (date * 86_400, rise, set)))
                .flat_map(|(midnight, rise, set)| [(midnight + rise * 60, Phase::Day), (midnight + set * 60, Phase::Night)])
                .filter(|&(at, _)| at <= now_utc_secs)
                .max_by_key(|&(at, _)| at)
                .map(|(_, phase)| phase)
        }
    }
}
