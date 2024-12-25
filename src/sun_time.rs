use anyhow::Error;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::{Latitude, Longitude};

#[derive(Debug)]
pub struct SunTime {
    pub sunrise: Timestamp,
    pub sunset: Timestamp,
}

pub type Timestamp = i64;
pub fn get_current_timestamp() -> Result<Timestamp, Error> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_secs()
        .try_into()
        .map_err(Error::new)
}

impl SunTime {
    pub fn calc_time(lat: Latitude, lng: Longitude, timestamp: Timestamp) -> Result<Self, Error> {
        const FULL_CIRCLE: f64 = 360_f64;

        let j_date = timestamp as f64 / 86400.0 + 2440587.5;

        let n = (j_date - (2451545.0 + 0.0009) + 69.184 / 86400.0).ceil();

        let j_ = n + 0.0009 - lng / FULL_CIRCLE;

        let m_degrees = (357.5291 + 0.98560028 * j_) % FULL_CIRCLE;
        let m_radians = m_degrees.to_radians();
        let c_degrees = 1.9148 * m_radians.sin()
            + 0.02 * (2_f64 * m_radians).sin()
            + 0.0003 * (3_f64 * m_radians).sin();

        let l_degrees = (m_degrees + c_degrees + 180.0 + 102.9372) % FULL_CIRCLE;
        let lambda_radians = l_degrees.to_radians();

        let j_transit =
            2451545.0 + j_ + 0.0053 * m_radians.sin() - 0.0069 * (2_f64 * lambda_radians).sin();

        let sin_d = lambda_radians.sin() * 23.4397_f64.to_radians().sin();
        let cos_d = sin_d.asin().cos();
        let some_cos = (-0.833_f64.to_radians().sin() - lat.to_radians().sin() * sin_d)
            / (lat.to_radians().cos() * cos_d);

        let w0_radians = some_cos.acos();
        if w0_radians.is_nan() {
            return Err(Error::msg(""));
        }
        let w0_degrees = w0_radians.to_degrees();

        fn j_day_to_timestamp(j: f64) -> Timestamp {
            ((j - 2440587.5) * 86400_f64).round() as Timestamp
        }
        let j_rise = j_transit - w0_degrees / FULL_CIRCLE;
        let j_set = j_transit + w0_degrees / FULL_CIRCLE;

        Ok(SunTime {
            sunrise: j_day_to_timestamp(j_rise),
            sunset: j_day_to_timestamp(j_set),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{get_timestamp, LONDON};
    use chrono::Datelike;

    mod sun_time_date {
        use chrono::Local;

        use crate::test_utils::get_datetime;

        use super::*;

        #[test]
        fn between_sunrise_sunset() {
            let SunTime { sunrise, sunset } =
                SunTime::calc_time(LONDON.lat, LONDON.lng, Local::now().timestamp()).unwrap();
            let sunrise_date = get_datetime(sunrise, LONDON.offset);
            let sunset_date = get_datetime(sunset, LONDON.offset);
            assert_eq!(sunrise_date.day(), sunset_date.day())
        }

        #[test]
        fn before_sunrise() {
            let timestamp = get_timestamp(6, 1, LONDON.offset);
            let SunTime { sunrise, .. } =
                SunTime::calc_time(LONDON.lat, LONDON.lng, timestamp).unwrap();
            let date = get_datetime(timestamp, LONDON.offset);
            let sunrise_date = get_datetime(sunrise, LONDON.offset);
            assert_eq!(date.day(), sunrise_date.day());
        }

        #[test]
        fn after_sunset() {
            let timestamp = get_timestamp(6, 23, LONDON.offset);
            let SunTime { sunset, .. } =
                SunTime::calc_time(LONDON.lat, LONDON.lng, timestamp).unwrap();
            let date = get_datetime(timestamp, LONDON.offset);
            let sunset_date = get_datetime(sunset, LONDON.offset);
            assert_eq!(date.day() + 1, sunset_date.day());
        }
    }

    mod sun_time {
        use chrono::Timelike;

        use crate::test_utils::{get_datetime, SunTimeDate, NAIROBI};

        use super::*;

        #[test]
        fn utc() {
            let timestamp = get_timestamp(6, 12, LONDON.offset);
            let SunTime { sunrise, sunset } =
                SunTime::calc_time(LONDON.lat, LONDON.lng, timestamp).unwrap();
            assert!(sunrise < sunset);
            assert!(sunrise < timestamp);
            assert!(sunset > timestamp);
        }

        #[test]
        fn eat() {
            let timestamp = get_timestamp(6, 0, NAIROBI.offset);
            let SunTime { sunrise, sunset } =
                SunTime::calc_time(NAIROBI.lat, NAIROBI.lng, timestamp).unwrap();
            assert_eq!(get_datetime(sunrise, NAIROBI.offset).hour(), 6);
            assert_eq!(get_datetime(sunset, NAIROBI.offset).hour(), 18);
        }

        #[test]
        fn summer_winter() {
            let summer = get_timestamp(8, 0, LONDON.offset);
            let summer_sun_time_date = SunTimeDate::new(
                SunTime::calc_time(LONDON.lat, LONDON.lng, summer).unwrap(),
                LONDON.offset,
            );
            let winter = get_timestamp(12, 0, LONDON.offset);
            let winter_sun_time_date = SunTimeDate::new(
                SunTime::calc_time(LONDON.lat, LONDON.lng, winter).unwrap(),
                LONDON.offset,
            );

            assert!(summer_sun_time_date.sunrise.hour() < winter_sun_time_date.sunrise.hour());
            assert!(summer_sun_time_date.sunset.hour() > winter_sun_time_date.sunset.hour());
        }
    }
}
