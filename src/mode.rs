use crate::config::{Latitude, Longitude};

use sun_time::{calculate_sun_time, get_current_timestamp, Timestamp};

use anyhow::Result;

#[cfg(test)]
pub(crate) mod sun_time;
#[cfg(not(test))]
mod sun_time;

#[derive(PartialEq, Eq)]
pub enum LightMode {
    Light(Timestamp),
    Dark(Timestamp),
}

fn decide_mode(lat: Latitude, lng: Longitude, timestamp: Timestamp) -> Result<LightMode> {
    let sun_time = calculate_sun_time(lat, lng, timestamp)?;
    Ok(
        if sun_time.sunrise() < timestamp && timestamp < sun_time.sunset() {
            LightMode::Light(sun_time.sunset() - timestamp)
        } else {
            LightMode::Dark(sun_time.sunrise() - timestamp)
        },
    )
}
pub fn get_mode(lat: Latitude, lng: Longitude) -> Result<LightMode> {
    let now = get_current_timestamp()?;
    decide_mode(lat, lng, now)
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
            if let LightMode::Light(time_left) =
                decide_mode(NAIROBI.lat, NAIROBI.lng, timestamp).unwrap()
            {
                assert!((time_left - 6 * HOUR).abs() < HOUR)
            } else {
                panic!()
            }
        }

        #[test]
        fn early_morning() {
            let timestamp = get_timestamp(6, 3, NAIROBI.offset);
            if let LightMode::Dark(time_left) =
                decide_mode(NAIROBI.lat, NAIROBI.lng, timestamp).unwrap()
            {
                assert!((time_left - 3 * HOUR).abs() < HOUR)
            } else {
                panic!()
            }
        }

        #[test]
        fn late_night() {
            let timestamp = get_timestamp(6, 22, NAIROBI.offset);
            if let LightMode::Dark(time_left) =
                decide_mode(NAIROBI.lat, NAIROBI.lng, timestamp).unwrap()
            {
                assert!((time_left - 8 * HOUR).abs() < HOUR)
            } else {
                panic!()
            }
        }
    }
}
