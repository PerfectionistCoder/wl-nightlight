use core::panic;

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime};
#[cfg(not(test))]
use chrono::{Local, Utc};
#[cfg(test)]
use mock_chrono::{Local, Utc};

use sunrise::{
    Coordinates, SolarDay,
    SolarEvent::{Sunrise, Sunset},
};

use crate::config::{ConfigTimeMode, LightDarkTime, Location};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ColorMode {
    Light,
    Dark,
}

#[derive(Default, Debug)]
struct ColorEventState {
    coord: Option<Coordinates>,
    light_time: Option<NaiveTime>,
    dark_time: Option<NaiveTime>,
}

impl ColorEventState {
    fn set_coord(&mut self, lat: f64, lon: f64) {
        self.coord = Some(Coordinates::new(lat, lon).unwrap());
    }
}

trait TimeProvider: std::fmt::Debug {
    fn new(state: &ColorEventState) -> Self
    where
        Self: Sized;
    fn light_time(&self, date: NaiveDate) -> DateTime<chrono::Utc>;
    fn dark_time(&self, date: NaiveDate) -> DateTime<chrono::Utc>;
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
        let ColorEventState { coord, .. } = *state;
        Self {
            coord: coord.unwrap(),
        }
    }
    fn light_time(&self, date: NaiveDate) -> DateTime<chrono::Utc> {
        SolarDay::new(self.coord, date).event_time(Sunrise)
    }
    fn dark_time(&self, date: NaiveDate) -> DateTime<chrono::Utc> {
        SolarDay::new(self.coord, date).event_time(Sunset)
    }
}

#[derive(Debug)]
struct FixedTimeProvider {
    light_time: Option<NaiveTime>,
    dark_time: Option<NaiveTime>,
}

impl TimeProvider for FixedTimeProvider {
    fn new(state: &ColorEventState) -> Self
    where
        Self: Sized,
    {
        let ColorEventState {
            light_time,
            dark_time,
            ..
        } = *state;
        Self {
            light_time,
            dark_time,
        }
    }
    fn light_time(&self, date: NaiveDate) -> DateTime<chrono::Utc> {
        NaiveDateTime::new(date, self.light_time.unwrap())
            .and_local_timezone(Local)
            .unwrap()
            .to_utc()
    }
    fn dark_time(&self, date: NaiveDate) -> DateTime<chrono::Utc> {
        NaiveDateTime::new(date, self.dark_time.unwrap())
            .and_local_timezone(Local)
            .unwrap()
            .to_utc()
    }
}

#[derive(Debug)]
pub struct ColorEvent {
    pub mode: ColorMode,
    pub wait_sec: i64,
    light_time_provider: Box<dyn TimeProvider>,
    dark_time_provider: Box<dyn TimeProvider>,
}

impl ColorEvent {
    pub fn new(light_dark_time: LightDarkTime, location: Option<Location>) -> Self {
        let mut state = ColorEventState::default();

        if let Some(location) = location {
            state.set_coord(location.lat, location.lon);
        }

        state.light_time = match light_dark_time.light_time {
            ConfigTimeMode::Fixed(time) => Some(time),
            _ => None,
        };
        state.dark_time = match light_dark_time.dark_time {
            ConfigTimeMode::Fixed(time) => Some(time),
            _ => None,
        };

        let light_time_provider: Box<dyn TimeProvider> = match light_dark_time.light_time {
            ConfigTimeMode::Auto => Box::new(AutoTimeProvider::new(&state)),
            ConfigTimeMode::Fixed(_) => Box::new(FixedTimeProvider::new(&state)),
        };
        let dark_time_provider: Box<dyn TimeProvider> = match light_dark_time.dark_time {
            ConfigTimeMode::Auto => Box::new(AutoTimeProvider::new(&state)),
            ConfigTimeMode::Fixed(_) => Box::new(FixedTimeProvider::new(&state)),
        };

        let (mode, wait_sec) = calculate_next(&*light_time_provider, &*dark_time_provider);
        Self {
            mode,
            wait_sec,
            light_time_provider,
            dark_time_provider,
        }
    }

    pub fn next(&mut self) {
        let (mode, wait_sec) =
            calculate_next(&*self.light_time_provider, &*self.dark_time_provider);
        self.mode = mode;
        self.wait_sec = wait_sec;
    }
}

fn calculate_next(
    light_time_provider: &dyn TimeProvider,
    dark_time_provider: &dyn TimeProvider,
) -> (ColorMode, i64) {
    let date = Local::now().date_naive();
    let now = Utc::now();

    let light_time = light_time_provider.light_time(date);
    let dark_time = dark_time_provider.dark_time(date);

    if light_time > dark_time {
        panic!();
    }

    let mode: ColorMode;
    let until: DateTime<chrono::Utc>;
    if now < light_time {
        mode = ColorMode::Dark;
        until = light_time;
    } else if now < dark_time {
        mode = ColorMode::Light;
        until = dark_time;
    } else {
        mode = ColorMode::Dark;
        until = light_time_provider.light_time(date.succ_opt().unwrap())
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

        mod auto {
            use super::*;

            // Nairobi
            const OFFSET: FixedOffset = FixedOffset::east_opt(3 * HOUR).unwrap();
            const LOCATION: Option<Location> = Some(Location {
                lat: -1.2,
                lon: 36.8,
            });

            const LIGHT_DARK_TIME: LightDarkTime = LightDarkTime {
                light_time: ConfigTimeMode::Auto,
                dark_time: ConfigTimeMode::Auto,
            };

            #[test]
            fn morning() {
                set_time(0, 0, OFFSET);
                let mut event = ColorEvent::new(LIGHT_DARK_TIME, LOCATION);

                let sunrise = forward_time(event.wait_sec, &OFFSET);
                assert_eq!(event.mode, ColorMode::Dark);
                assert_eq!(sunrise.hour(), 6);
                assert!(sunrise.minute() > 15 && sunrise.minute() < 45);

                mock_chrono::set(sunrise);
                event.next();
                let sunset = forward_time(event.wait_sec, &OFFSET);
                assert_eq!(event.mode, ColorMode::Light);
                assert_eq!(sunset.hour(), 18);
                assert!(sunset.minute() > 15 && sunset.minute() < 45);
            }

            #[test]
            fn noon() {
                set_time(13, 0, OFFSET);
                let mut event = ColorEvent::new(LIGHT_DARK_TIME, LOCATION);

                let sunset = forward_time(event.wait_sec, &OFFSET);
                assert_eq!(event.mode, ColorMode::Light);
                assert_eq!(sunset.hour(), 18);
                assert!(sunset.minute() > 15 && sunset.minute() < 45);

                mock_chrono::set(sunset);
                event.next();
                let sunrise = forward_time(event.wait_sec, &OFFSET);
                assert_eq!(event.mode, ColorMode::Dark);
                assert_eq!(sunrise.hour(), 6);
                assert!(sunrise.minute() > 15 && sunrise.minute() < 45);
            }

            #[test]
            fn midnight() {
                set_time(23, 0, OFFSET);
                let mut event = ColorEvent::new(LIGHT_DARK_TIME, LOCATION);

                let sunrise = forward_time(event.wait_sec, &OFFSET);
                assert_eq!(event.mode, ColorMode::Dark);
                assert_eq!(sunrise.hour(), 6);
                assert!(sunrise.minute() > 15 && sunrise.minute() < 45);

                mock_chrono::set(sunrise);
                event.next();
                let sunset = forward_time(event.wait_sec, &OFFSET);
                assert_eq!(event.mode, ColorMode::Light);
                assert_eq!(sunset.hour(), 18);
                assert!(sunset.minute() > 15 && sunset.minute() < 45);
            }
        }

        mod fixed {
            use super::*;

            const OFFSET: FixedOffset = FixedOffset::east_opt(0).unwrap();
            const LOCATION: Option<Location> = None;

            const LIGHT_DARK_TIME: LightDarkTime = LightDarkTime {
                light_time: ConfigTimeMode::Fixed(NaiveTime::from_hms_opt(8, 0, 0).unwrap()),
                dark_time: ConfigTimeMode::Fixed(NaiveTime::from_hms_opt(19, 0, 0).unwrap()),
            };

            #[test]
            fn morning() {
                set_time(0, 0, OFFSET);
                let mut event = ColorEvent::new(LIGHT_DARK_TIME, LOCATION);

                let sunrise = forward_time(event.wait_sec, &OFFSET);
                assert_eq!(event.mode, ColorMode::Dark);
                assert_eq!(sunrise.hour(), 8);
                assert_eq!(sunrise.minute(), 0);

                mock_chrono::set(sunrise);
                event.next();
                let sunset = forward_time(event.wait_sec, &OFFSET);
                assert_eq!(event.mode, ColorMode::Light);
                assert_eq!(sunset.hour(), 19);
                assert_eq!(sunset.minute(), 0);
            }

            #[test]
            fn noon() {
                set_time(13, 0, OFFSET);
                let mut event = ColorEvent::new(LIGHT_DARK_TIME, LOCATION);

                let sunset = forward_time(event.wait_sec, &OFFSET);
                assert_eq!(event.mode, ColorMode::Light);
                assert_eq!(sunset.hour(), 19);
                assert_eq!(sunset.minute(), 0);

                mock_chrono::set(sunset);
                event.next();
                let sunrise = forward_time(event.wait_sec, &OFFSET);
                assert_eq!(event.mode, ColorMode::Dark);
                assert_eq!(sunrise.hour(), 8);
                assert_eq!(sunrise.minute(), 0);
            }

            #[test]
            fn midnight() {
                set_time(23, 0, OFFSET);
                let mut event = ColorEvent::new(LIGHT_DARK_TIME, LOCATION);

                let sunrise = forward_time(event.wait_sec, &OFFSET);
                assert_eq!(event.mode, ColorMode::Dark);
                assert_eq!(sunrise.hour(), 8);
                assert_eq!(sunrise.minute(), 0);

                mock_chrono::set(sunrise);
                event.next();
                let sunset = forward_time(event.wait_sec, &OFFSET);
                assert_eq!(event.mode, ColorMode::Light);
                assert_eq!(sunset.hour(), 19);
                assert_eq!(sunset.minute(), 0);
            }
        }
    }
}

#[cfg(test)]
mod mock_chrono {
    use std::cell::Cell;

    use chrono::{
        DateTime, FixedOffset, MappedLocalTime, NaiveDate, NaiveDateTime, NaiveTime, Offset,
        TimeZone,
    };

    thread_local! {
        static DATE: Cell<Option<DateTime<chrono::FixedOffset>>> = const { Cell::new(None) };
    }

    pub struct Utc;

    impl Utc {
        pub fn now() -> DateTime<chrono::Utc> {
            DATE.with(|date| date.get().unwrap().with_timezone(&chrono::Utc))
        }
    }

    mod inner {
        use super::*;

        pub(super) fn offset_from_utc_datetime(
            _utc_time: &NaiveDateTime,
        ) -> MappedLocalTime<FixedOffset> {
            DATE.with(|date| {
                let offset = date.get().unwrap().offset().fix().local_minus_utc();
                MappedLocalTime::Single(FixedOffset::east_opt(offset).unwrap())
            })
        }

        pub(super) fn offset_from_local_datetime(
            _local_time: &NaiveDateTime,
        ) -> MappedLocalTime<FixedOffset> {
            DATE.with(|date| {
                let offset = date.get().unwrap().offset().fix().local_minus_utc();
                MappedLocalTime::Single(FixedOffset::east_opt(offset).unwrap())
            })
        }
    }

    #[derive(Clone)]
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

    impl TimeZone for Local {
        type Offset = FixedOffset;

        fn from_offset(_offset: &FixedOffset) -> Local {
            Local
        }

        #[allow(deprecated)]
        fn offset_from_local_date(&self, local: &NaiveDate) -> MappedLocalTime<FixedOffset> {
            // Get the offset at local midnight.
            self.offset_from_local_datetime(&local.and_time(NaiveTime::MIN))
        }

        fn offset_from_local_datetime(
            &self,
            local: &NaiveDateTime,
        ) -> MappedLocalTime<FixedOffset> {
            inner::offset_from_local_datetime(local)
        }

        #[allow(deprecated)]
        fn offset_from_utc_date(&self, utc: &NaiveDate) -> FixedOffset {
            // Get the offset at midnight.
            self.offset_from_utc_datetime(&utc.and_time(NaiveTime::MIN))
        }

        fn offset_from_utc_datetime(&self, utc: &NaiveDateTime) -> FixedOffset {
            inner::offset_from_utc_datetime(utc).unwrap()
        }
    }

    pub fn set(val: DateTime<chrono::FixedOffset>) {
        DATE.with(|date| date.set(Some(val)));
    }
}
