use anyhow::{Error, Ok};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub struct SunTime {
    sunrise: f64,
    sunset: f64,
}

impl SunTime {
    pub fn calc(
        lat: f64,
        lng: f64,
        time: Option<f64>,
    ) -> Result<SunTime, anyhow::Error> {
        const FULL_CIRCLE: f64 = 360_f64;

        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
        let j_date = time.unwrap_or(now) / 86400.0 + 2440587.5;

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

        fn j_day_to_timestamp(j: f64) -> f64 {
            ((j - 2440587.5) * 86400_f64).trunc()
        }
        let j_rise = j_transit - w0_degrees / FULL_CIRCLE;
        let j_set = j_transit + w0_degrees / FULL_CIRCLE;

        Ok(SunTime {
            sunrise: j_day_to_timestamp(j_rise),
            sunset: j_day_to_timestamp(j_set),
        })
    }
}

mod tests {
    use super::*;

    use chrono::{DateTime, Datelike, Local, NaiveTime};

    use std::cmp::{Eq, Ord, Ordering, PartialEq};
    impl Eq for SunTime {}
    impl PartialEq for SunTime {
        fn eq(&self, other: &SunTime) -> bool {
            self.sunrise == other.sunrise && self.sunset == other.sunset
        }
    }
    impl Ord for SunTime {
        fn cmp(&self, other: &Self) -> Ordering {
            let self_diff = self.sunset - self.sunrise;
            let other_diff = other.sunset - self.sunrise;
            if self_diff > other_diff {
                Ordering::Greater
            } else if self_diff < other_diff {
                Ordering::Less
            } else {
                Ordering::Equal
            }
        }
    }
    impl PartialOrd for SunTime {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            Some(self.cmp(other))
        }
    }

    static LAT: f64 = 51.51;
    static LNG: f64 = -0.12;

    #[test]
    fn optional_time_argument() {
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        assert_eq!(
            SunTime::calc(LAT, LNG, Some(time)).unwrap(),
            SunTime::calc(LAT, LNG, None).unwrap()
        );
    }

    #[test]
    fn within_same_day() {
        let SunTime { sunrise, sunset } = SunTime::calc(LAT, LNG, None).unwrap();
        let sunrise_date = DateTime::from_timestamp(sunrise as i64, 0).unwrap();
        let sunset_date = DateTime::from_timestamp(sunset as i64, 0).unwrap();
        assert!(sunrise < sunset);
        assert_eq!(sunrise_date.day(), sunset_date.day())
    }

    #[test]
    fn before_and_after_noon() {
        let today_noon = Local::now()
            .with_time(NaiveTime::from_hms_opt(12, 0, 0).unwrap())
            .unwrap()
            .timestamp() as f64;
        let SunTime { sunrise, sunset } = SunTime::calc(LAT, LNG, Some(today_noon)).unwrap();
        assert!(sunrise < today_noon);
        assert!(sunset > today_noon);
    }
}
