use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;

use crate::config::{Latitude, Longitude};
use sun_time::{SunTime, Timestamp};

#[cfg(test)]
pub(crate) mod sun_time;

#[cfg(not(test))]
mod sun_time;

#[derive(Debug, PartialEq, Eq)]
enum LightMode {
    Light,
    Dark,
}

pub struct ModeTimer {
    next: Timestamp,
    mode: LightMode,
}

impl ModeTimer {
    pub fn new(lat: Latitude, lng: Longitude) -> Result<Self> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as Timestamp;
        ModeTimer::get_timer(lat, lng, now)
    }

    fn get_timer(lat: Latitude, lng: Longitude, timestamp: Timestamp) -> Result<Self> {
        let sun_time = SunTime::new(lat, lng, timestamp)?;
        Ok(
            if sun_time.sunrise() < timestamp && timestamp < sun_time.sunset() {
                Self {
                    next: sun_time.sunset() - timestamp,
                    mode: LightMode::Light,
                }
            } else {
                Self {
                    next: sun_time.sunrise() - timestamp,
                    mode: LightMode::Dark,
                }
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{get_timestamp, NAIROBI};

    const HOUR: i64 = 3600;

    mod test_light_mode_and_time_left {
        use super::*;

        #[test]
        fn noon() {
            let timestamp = get_timestamp(6, 12, NAIROBI.offset);
            let timer = ModeTimer::get_timer(NAIROBI.lat, NAIROBI.lng, timestamp).unwrap();
            assert_eq!(timer.mode, LightMode::Light);
            assert!((timer.next - 6 * HOUR).abs() < HOUR);
        }

        #[test]
        fn early_morning() {
            let timestamp = get_timestamp(6, 3, NAIROBI.offset);
            let timer = ModeTimer::get_timer(NAIROBI.lat, NAIROBI.lng, timestamp).unwrap();
            assert_eq!(timer.mode, LightMode::Dark);
            assert!((timer.next - 3 * HOUR).abs() < HOUR);
        }

        #[test]
        fn late_night() {
            let timestamp = get_timestamp(6, 22, NAIROBI.offset);
            let timer = ModeTimer::get_timer(NAIROBI.lat, NAIROBI.lng, timestamp).unwrap();
            assert_eq!(timer.mode, LightMode::Dark);
            assert!((timer.next - 8 * HOUR).abs() < HOUR);
        }
    }
}
