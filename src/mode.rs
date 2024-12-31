use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use sun_time::{SunTime, Timestamp};

use crate::config::Location;

mod sun_time;

#[cfg(test)]
mod test_utils;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum LightMode {
    Light,
    Dark,
}

pub struct ModeTimer {
    next: Timestamp,
    mode: LightMode,
}

impl ModeTimer {
    pub fn new(location: Location) -> Result<Self> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as Timestamp;
        ModeTimer::get_timer(location, now)
    }

    fn get_timer(location: Location, timestamp: Timestamp) -> Result<Self> {
        let sun_time = SunTime::new(location.lat, location.lng, timestamp)?;
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

    pub fn next(&self) -> Timestamp {
        self.next
    }

    pub fn mode(&self) -> LightMode {
        self.mode
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
        let timer = ModeTimer::get_timer(NAIROBI.location(), timestamp).unwrap();
        assert_eq!(timer.mode, LightMode::Light);
        assert!((timer.next - 6 * HOUR).abs() < HOUR);
    }

    #[test]
    fn early_morning() {
        let timestamp = get_timestamp(6, 3, NAIROBI.offset);
        let timer = ModeTimer::get_timer(NAIROBI.location(), timestamp).unwrap();
        assert_eq!(timer.mode, LightMode::Dark);
        assert!((timer.next - 3 * HOUR).abs() < HOUR);
    }

    #[test]
    fn late_night() {
        let timestamp = get_timestamp(6, 22, NAIROBI.offset);
        let timer = ModeTimer::get_timer(NAIROBI.location(), timestamp).unwrap();
        assert_eq!(timer.mode, LightMode::Dark);
        assert!((timer.next - 8 * HOUR).abs() < HOUR);
    }
}
