use chrono::DateTime;
#[cfg(not(test))]
use chrono::{Local, Utc};
#[cfg(test)]
use mock_chrono::{Local, Utc};

use sunrise::{
    Coordinates, SolarDay,
    SolarEvent::{Sunrise, Sunset},
};

#[derive(Debug, PartialEq, Eq)]
pub enum Mode {
    Light,
    Dark,
}

#[derive(Debug)]
pub struct Event {
    mode: Mode,
    wait: i64,
}

impl Event {
    pub fn new_sunrise(lat: f64, lon: f64) -> Self {
        let date = Local::now().date_naive();

        let coord = Coordinates::new(lat, lon).unwrap();
        let solar_day = SolarDay::new(coord, date);
        let sunrise = solar_day.event_time(Sunrise);
        let sunset = solar_day.event_time(Sunset);

        Self::new(sunrise, sunset, || {
            SolarDay::new(coord, date.succ_opt().unwrap()).event_time(Sunrise)
        })
    }

    fn new<T: Fn() -> DateTime<chrono::Utc>>(
        sunrise: DateTime<chrono::Utc>,
        sunset: DateTime<chrono::Utc>,
        next_sunrise: T,
    ) -> Self {
        let now = Utc::now();

        let (mode, wait) = Self::next_event(now, sunrise, sunset);
        Self {
            mode,
            wait: (wait.unwrap_or_else(next_sunrise) - now).num_seconds(),
        }
    }

    fn next_event(
        now: DateTime<chrono::Utc>,
        sunrise: DateTime<chrono::Utc>,
        sunset: DateTime<chrono::Utc>,
    ) -> (Mode, Option<DateTime<chrono::Utc>>) {
        if now < sunrise {
            (Mode::Light, Some(sunrise))
        } else if now < sunset {
            (Mode::Dark, Some(sunset))
        } else {
            (Mode::Light, None)
        }
    }
}

#[cfg(test)]
mod test {
    use chrono::{FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, Offset, TimeZone, Timelike};

    use super::*;

    const HOUR: i32 = 3600;
    const NAIVEDATE: NaiveDate = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();

    fn set_time(hour: u32, min: u32, offset: i32) {
        let offset = FixedOffset::east_opt(offset * HOUR).unwrap();
        mock_chrono::set(
            offset
                .from_local_datetime(&NaiveDateTime::new(
                    NAIVEDATE,
                    NaiveTime::from_hms_opt(hour, min, 0).unwrap(),
                ))
                .unwrap(),
        );
    }

    #[test]
    fn mock() {
        let hour = 12;
        let min = 0;
        let offset = 8;
        set_time(hour, min, offset);
        assert_ne!(mock_chrono::Local::now(), chrono::Local::now());
        assert_ne!(mock_chrono::Utc::now(), chrono::Utc::now());
        assert_eq!(mock_chrono::Local::now().hour(), hour);
        assert_eq!(mock_chrono::Local::now().minute(), min);
        assert_eq!(
            mock_chrono::Local::now().offset().fix(),
            FixedOffset::east_opt(offset * HOUR).unwrap()
        );
    }

    mod event {
        use super::*;

        const OFFSET: i32 = 3;
        const LAT: f64 = -1.2;
        const LON: f64 = 36.8;

        #[test]
        fn morning() {
            set_time(0, 0, OFFSET);
            let Event { mode, wait } = Event::new_sunrise(LAT, LON);
            assert!(wait < 7 * HOUR as i64);
            assert!(wait > 6 * HOUR as i64);
            assert_eq!(mode, Mode::Light);
        }

        #[test]
        fn noon() {
            set_time(13, 0, OFFSET);
            let Event { mode, wait } = Event::new_sunrise(LAT, LON);
            assert!(wait < 6 * HOUR as i64);
            assert!(wait > 5 * HOUR as i64);
            assert_eq!(mode, Mode::Dark);
        }

        #[test]
        fn night() {
            set_time(23, 0, OFFSET);
            let Event { mode, wait } = Event::new_sunrise(LAT, LON);
            assert!(wait < 8 * HOUR as i64);
            assert!(wait > 7 * HOUR as i64);
            assert_eq!(mode, Mode::Light);
        }
    }
}

#[cfg(test)]
mod mock_chrono {
    use std::cell::Cell;

    use chrono::{DateTime, Offset};

    thread_local! {
        static DATE: Cell<Option<DateTime<chrono::FixedOffset>>> = const { Cell::new(None) };
    }

    pub struct Utc;

    impl Utc {
        pub fn now() -> DateTime<chrono::Utc> {
            DATE.with(|date| date.get().unwrap().with_timezone(&chrono::Utc))
        }
    }

    pub struct Local;

    impl Local {
        pub fn now() -> DateTime<chrono::Local> {
            DATE.with(|date| {
                let localdate: DateTime<chrono::Local> = date.get().unwrap().into();
                let offset = date.get().unwrap().offset().fix();
                DateTime::from_naive_utc_and_offset(localdate.naive_utc(), offset)
            })
        }
    }

    pub fn set(val: DateTime<chrono::FixedOffset>) {
        DATE.with(|date| date.set(Some(val)));
    }
}
