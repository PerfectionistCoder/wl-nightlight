use crate::config::{Latitude, Longitude};

use crate::sun_time::{get_current_timestamp, SunTime, Timestamp};
use anyhow::Error;

#[derive(PartialEq, Eq)]
pub enum LightMode {
    Light,
    Dark,
}

impl LightMode {
    fn decide_mode(
        lat: Latitude,
        lng: Longitude,
        timestamp: Timestamp,
    ) -> Result<Self, Error> {
        let SunTime { sunrise, sunset } = SunTime::calc_time(lat, lng, timestamp)?;
        Ok(if sunrise < timestamp && timestamp < sunset {
            LightMode::Light
        } else {
            LightMode::Dark
        })
    }
    pub fn get_mode(lat: Latitude, lng: Longitude) -> Result<Self, Error> {
        let now = get_current_timestamp()?;
        LightMode::decide_mode(lat, lng, now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{get_timestamp, LONDON};

    #[test]
    fn noon() {
        let timestamp = get_timestamp(6, 12, LONDON.offset);
        assert!(
            LightMode::decide_mode(LONDON.lat, LONDON.lng, timestamp).unwrap() == LightMode::Light
        );
    }

    #[test]
    fn early_morning() {
        let timestamp = get_timestamp(6, 3, LONDON.offset);
        assert!(
            LightMode::decide_mode(LONDON.lat, LONDON.lng, timestamp).unwrap() == LightMode::Dark
        );
    }

    #[test]
    fn late_night() {
        let timestamp = get_timestamp(6, 22, LONDON.offset);
        assert!(
            LightMode::decide_mode(LONDON.lat, LONDON.lng, timestamp).unwrap() == LightMode::Dark
        );
    }
}
