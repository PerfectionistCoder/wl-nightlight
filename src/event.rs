use chrono::{DateTime, NaiveDate};
#[cfg(not(test))]
use chrono::{Local, Utc};
#[cfg(test)]
use mock_chrono::{Local, Utc};

use sunrise::{
    Coordinates, SolarDay,
    SolarEvent::{Sunrise, Sunset},
};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ColorMode {
    Light,
    Dark,
}

struct ColorEventState {
    coord: Coordinates,
}

impl ColorEventState {
    fn new(lat: f64, lon: f64) -> Self {
        Self {
            coord: Coordinates::new(lat, lon).unwrap(),
        }
    }
}

trait TimeProvider: std::fmt::Debug {
    fn new(state: &ColorEventState) -> Self
    where
        Self: Sized;
    fn day(&self, date: NaiveDate) -> DateTime<chrono::Utc>;
    fn night(&self, date: NaiveDate) -> DateTime<chrono::Utc>;
}

#[derive(Debug)]
struct AutoTimeProvider {
    coord: Coordinates,
}

impl TimeProvider for AutoTimeProvider {
    fn new(state: &ColorEventState) -> Self
    where
        Self: Sized,
    {
        let ColorEventState { coord } = *state;
        Self { coord }
    }
    fn day(&self, date: NaiveDate) -> DateTime<chrono::Utc> {
        SolarDay::new(self.coord, date).event_time(Sunrise)
    }
    fn night(&self, date: NaiveDate) -> DateTime<chrono::Utc> {
        SolarDay::new(self.coord, date).event_time(Sunset)
    }
}

#[derive(Debug)]
pub struct ColorEvent {
    pub mode: ColorMode,
    pub wait_sec: i64,
    day_time_provider: Box<dyn TimeProvider>,
    night_time_provider: Box<dyn TimeProvider>,
}

impl ColorEvent {
    pub fn new_auto(lat: f64, lon: f64) -> Self {
        let state = ColorEventState::new(lat, lon);
        let day_time_provider: Box<dyn TimeProvider> = Box::new(AutoTimeProvider::new(&state));
        let night_time_provider: Box<dyn TimeProvider> = Box::new(AutoTimeProvider::new(&state));

        let (mode, wait_sec) = calculate_next(&*day_time_provider, &*night_time_provider);
        Self {
            mode,
            wait_sec,
            day_time_provider,
            night_time_provider,
        }
    }

    pub fn next(&mut self) {
        let (mode, wait_sec) = calculate_next(&*self.day_time_provider, &*self.night_time_provider);
        self.mode = mode;
        self.wait_sec = wait_sec;
    }
}

fn calculate_next(
    day_time_provider: &dyn TimeProvider,
    night_time_provider: &dyn TimeProvider,
) -> (ColorMode, i64) {
    let date = Local::now().date_naive();
    let now = Utc::now();

    let day_time = day_time_provider.day(date);
    let night_time = night_time_provider.night(date);

    let mode: ColorMode;
    let until: DateTime<chrono::Utc>;
    if now < day_time {
        mode = ColorMode::Dark;
        until = day_time;
    } else if now < night_time {
        mode = ColorMode::Light;
        until = night_time;
    } else {
        mode = ColorMode::Dark;
        until = day_time_provider.day(date.succ_opt().unwrap())
    }
    (mode, (until - now).num_seconds())
}

#[cfg(test)]
mod test {
    use chrono::{
        FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, Offset, TimeDelta, TimeZone, Timelike,
    };

    use super::*;

    const HOUR: i32 = 3600;
    const NAIVEDATE: NaiveDate = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();

    fn set_time(hour: u32, min: u32, offset: FixedOffset) {
        mock_chrono::set(
            offset
                .from_local_datetime(&NaiveDateTime::new(
                    NAIVEDATE,
                    NaiveTime::from_hms_opt(hour, min, 0).unwrap(),
                ))
                .unwrap(),
        );
    }

    fn forward_time(sec: i64, offset: &FixedOffset) -> DateTime<FixedOffset> {
        (mock_chrono::Local::now() + TimeDelta::new(sec, 0).unwrap()).with_timezone(offset)
    }

    #[test]
    fn chrono_mock() {
        let hour = 12;
        let min = 0;
        let offset = 8;
        set_time(hour, min, FixedOffset::east_opt(offset * HOUR).unwrap());
        assert_ne!(mock_chrono::Local::now(), chrono::Local::now());
        assert_ne!(mock_chrono::Utc::now(), chrono::Utc::now());
        assert_eq!(mock_chrono::Local::now().hour(), hour);
        assert_eq!(mock_chrono::Local::now().minute(), min);
        assert_eq!(
            mock_chrono::Local::now().offset().fix(),
            FixedOffset::east_opt(offset * HOUR).unwrap()
        );
        assert_eq!(mock_chrono::Local::now().to_utc(), mock_chrono::Utc::now());
    }

    mod event_loop {
        use super::*;

        // Nairobi
        const OFFSET: FixedOffset = FixedOffset::east_opt(3 * HOUR).unwrap();
        const LAT: f64 = -1.2;
        const LON: f64 = 36.8;

        #[test]
        fn morning() {
            set_time(0, 0, OFFSET);
            let mut event = ColorEvent::new_auto(LAT, LON);
            let ColorEvent { mode, wait_sec, .. } = event;
            assert!(wait_sec < 7 * HOUR as i64);
            assert!(wait_sec > 6 * HOUR as i64);
            assert_eq!(mode, ColorMode::Dark);

            mock_chrono::set(forward_time(event.wait_sec, &OFFSET));
            event.next();
            assert_eq!(event.mode, ColorMode::Light);
            assert_eq!(forward_time(event.wait_sec, &OFFSET).hour(), 18);
        }

        #[test]
        fn noon() {
            set_time(13, 0, OFFSET);
            let mut event = ColorEvent::new_auto(LAT, LON);
            let ColorEvent { mode, wait_sec, .. } = event;
            assert!(wait_sec < 6 * HOUR as i64);
            assert!(wait_sec > 5 * HOUR as i64);
            assert_eq!(mode, ColorMode::Light);

            mock_chrono::set(forward_time(event.wait_sec, &OFFSET));
            event.next();
            assert_eq!(event.mode, ColorMode::Dark);
            assert_eq!(forward_time(event.wait_sec, &OFFSET).hour(), 6);
        }

        #[test]
        fn night() {
            set_time(23, 0, OFFSET);
            let mut event = ColorEvent::new_auto(LAT, LON);
            let ColorEvent { mode, wait_sec, .. } = event;
            assert!(wait_sec < 8 * HOUR as i64);
            assert!(wait_sec > 7 * HOUR as i64);
            assert_eq!(mode, ColorMode::Dark);

            mock_chrono::set(forward_time(event.wait_sec, &OFFSET));
            event.next();
            assert_eq!(event.mode, ColorMode::Light);
            assert_eq!(forward_time(event.wait_sec, &OFFSET).hour(), 18);
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
