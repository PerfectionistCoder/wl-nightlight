use std::time::{SystemTime, UNIX_EPOCH};

use getset::CopyGetters;
use sun_time::{SunTime, Timestamp};

use crate::config::{Latitude, Longitude};

mod sun_time;

#[cfg(test)]
mod test_utils;

#[derive(Debug, PartialEq, Clone, Copy)]
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
        let sun_time = SunTime::new(lat, lng, timestamp);

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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_utils::*;
    use super::*;

    const HOUR: i32 = 3600;

    #[test]
    fn noon() {
        let timestamp = get_timestamp(6, 12, NAIROBI.offset);
        let timer = ModeTimer::get_timer(NAIROBI.lat, NAIROBI.lng, timestamp);
        assert_eq!(timer.mode, LightMode::Light);
        assert!((timer.next - 6 * HOUR).abs() < HOUR);
    }

    #[test]
    fn early_morning() {
        let timestamp = get_timestamp(6, 3, NAIROBI.offset);
        let timer = ModeTimer::get_timer(NAIROBI.lat, NAIROBI.lng, timestamp);
        assert_eq!(timer.mode, LightMode::Dark);
        assert!((timer.next - 3 * HOUR).abs() < HOUR);
    }

    #[test]
    fn late_night() {
        let timestamp = get_timestamp(6, 22, NAIROBI.offset);
        let timer = ModeTimer::get_timer(NAIROBI.lat, NAIROBI.lng, timestamp);
        assert_eq!(timer.mode, LightMode::Dark);
        assert!((timer.next - 8 * HOUR).abs() < HOUR);
    }
}
