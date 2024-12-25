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
    pub fn calc(lat: Latitude, lng: Longitude, timestamp: Option<Timestamp>) -> Result<Self, Error> {
        const FULL_CIRCLE: f64 = 360_f64;

        let now = get_current_timestamp()?;
        let j_date = timestamp.unwrap_or(now) as f64 / 86400.0 + 2440587.5;

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
    use crate::test_utils::LONDON;
    use chrono::{DateTime, Datelike, NaiveDate, Utc};

    use std::cmp::{Eq, PartialEq};
    impl Eq for SunTime {}
    impl PartialEq for SunTime {
        fn eq(&self, other: &SunTime) -> bool {
            self.sunrise == other.sunrise && self.sunset == other.sunset
        }
    }

    #[test]
    fn within_same_day() {
        let SunTime { sunrise, sunset } = SunTime::calc(LONDON.lat, LONDON.lng, None).unwrap();
        let sunrise_date = DateTime::from_timestamp(sunrise, 0).unwrap();
        let sunset_date = DateTime::from_timestamp(sunset, 0).unwrap();
        assert_eq!(sunrise_date.day(), sunset_date.day())
    }

    #[test]
    fn before_and_after_noon() {
        let time = DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDate::from_ymd_opt(2024, 5, 1)
                .unwrap()
                .and_hms_opt(12, 0, 0)
                .unwrap(),
            Utc,
        )
        .timestamp();
        let SunTime { sunrise, sunset } =
            SunTime::calc(LONDON.lat, LONDON.lng, Some(time)).unwrap();
        assert!(sunrise < sunset);
        assert!(sunrise < time);
        assert!(sunset > time);
    }
}
