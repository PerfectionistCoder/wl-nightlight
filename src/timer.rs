use std::time::{SystemTime, UNIX_EPOCH};

use getset::CopyGetters;
use sunrise_sunset_calculator::{SunriseSunsetParameters, SunriseSunsetResult};

use crate::config::{Latitude, Longitude};

#[cfg(test)]
mod test_utils;

type Timestamp = i64;

#[derive(Clone, Copy, PartialEq)]
#[cfg_attr(test, derive(Debug))]
pub enum LightMode {
    Light,
    Dark,
}

#[derive(CopyGetters)]
#[getset(get_copy = "pub")]
pub struct ModeTimer {
    next: Timestamp,
    mode: LightMode,
}

impl ModeTimer {
    pub fn new(lat: Latitude, lng: Longitude) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as Timestamp;
        ModeTimer::get_timer(lat, lng, now)
    }

    fn get_timer(lat: Latitude, lng: Longitude, timestamp: Timestamp) -> Self {
        let SunriseSunsetResult {
            set: sunset,
            rise: sunrise,
            ..
        } = SunriseSunsetParameters::new(timestamp, lat, lng)
            .calculate()
            .unwrap();

        if sunrise < timestamp && timestamp < sunset {
            Self {
                next: sunset - timestamp,
                mode: LightMode::Light,
            }
        } else {
            Self {
                next: sunrise - timestamp,
                mode: LightMode::Dark,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_utils::*;
    use super::*;
    const HOUR: i64 = 3600;

    mod narobi {
        use super::*;

        fn get_mode_timer(hour: u32) -> ModeTimer {
            let timestamp = get_timestamp(1, hour, NAIROBI.offset);
            ModeTimer::get_timer(NAIROBI.lat, NAIROBI.lng, timestamp)
        }

        #[test]
        fn before_sunrise() {
            let ModeTimer { next, mode } = get_mode_timer(5);
            assert_eq!(mode, LightMode::Dark);
            assert!(next > HOUR);
            assert!(next < HOUR * 2);
        }

        #[test]
        fn after_sunrise() {
            let ModeTimer { mode, .. } = get_mode_timer(7);
            assert_eq!(mode, LightMode::Light);
        }

        #[test]
        fn noon() {
            let ModeTimer { next, mode } = get_mode_timer(12);
            assert_eq!(mode, LightMode::Light);
            assert!(next > 6 * HOUR);
            assert!(next < 7 * HOUR);
        }

        #[test]
        fn before_sunset() {
            let ModeTimer { next, mode } = get_mode_timer(18);
            assert_eq!(mode, LightMode::Light);
            assert!(next < HOUR);
        }

        #[test]
        fn after_sunset() {
            let ModeTimer { next, mode } = get_mode_timer(19);
            assert_eq!(mode, LightMode::Dark);
            assert!(next > 11 * HOUR);
            assert!(next < 12 * HOUR);
        }
    }

    mod london {
        use super::*;

        fn get_mode_timer(month: u32, hour: u32) -> ModeTimer {
            let timestamp = get_timestamp(month, hour, LONDON.offset);
            ModeTimer::get_timer(LONDON.lat, LONDON.lng, timestamp)
        }

        #[test]
        fn afternoon() {
            let ModeTimer { mode, next } = get_mode_timer(6, 14);
            assert_eq!(mode, LightMode::Light);
            assert!(next > 4 * HOUR);
            assert!(next < 5 * HOUR);
        }

        #[test]
        fn summer_winter() {
            let summer_morning = get_mode_timer(6, 0);
            let winter_morning = get_mode_timer(1, 0);
            assert!(summer_morning.next < winter_morning.next);

            let summer_night = get_mode_timer(6, 14);
            let winter_night = get_mode_timer(1, 14);
            assert!(winter_night.next < summer_night.next);
        }
    }
}
