use core::panic;
use std::fmt::Display;

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime};
#[cfg(not(test))]
use chrono::{Local, Utc};
#[cfg(test)]
use mock_chrono::{Local, Utc};

use sunrise::{
    Coordinates, SolarDay,
    SolarEvent::{Sunrise, Sunset},
};

use crate::config::{Location, SwitchMode, TimeProviderMode};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum OutputMode {
    Day,
    Night,
}

impl Display for OutputMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::Day => write!(f, "[day]"),
            Self::Night => write!(f, "[night]"),
        }
    }
}

#[derive(Default, Debug)]
struct TimeProviderState {
    coord: Option<Coordinates>,
    fixed_day_time: Option<NaiveTime>,
    fixed_night_time: Option<NaiveTime>,
}

impl TimeProviderState {
    fn set_coord(&mut self, latitude: f64, longitude: f64) {
        self.coord = Some(Coordinates::new(latitude, longitude).expect("coordinates out of range"));
    }
}

trait TimeProvider: std::fmt::Debug {
    fn new(state: &TimeProviderState) -> Self
    where
        Self: Sized;
    fn get_day_time(&self, date: NaiveDate) -> DateTime<chrono::Utc>;
    fn get_night_time(&self, date: NaiveDate) -> DateTime<chrono::Utc>;
}

#[derive(Debug)]
struct AutoTimeProvider {
    coord: Coordinates,
}

impl TimeProvider for AutoTimeProvider {
    fn new(state: &TimeProviderState) -> Self
    where
        Self: Sized,
    {
        let TimeProviderState { coord, .. } = *state;
        Self {
            coord: coord.expect("coordinates not set"),
        }
    }
    fn get_day_time(&self, date: NaiveDate) -> DateTime<chrono::Utc> {
        SolarDay::new(self.coord, date).event_time(Sunrise)
    }
    fn get_night_time(&self, date: NaiveDate) -> DateTime<chrono::Utc> {
        SolarDay::new(self.coord, date).event_time(Sunset)
    }
}

#[derive(Debug)]
struct FixedTimeProvider {
    day_time: NaiveTime,
    night_time: NaiveTime,
}

impl TimeProvider for FixedTimeProvider {
    fn new(state: &TimeProviderState) -> Self
    where
        Self: Sized,
    {
        let TimeProviderState {
            fixed_day_time,
            fixed_night_time,
            ..
        } = *state;
        Self {
            day_time: fixed_day_time.expect("fixed day time not set"),
            night_time: fixed_night_time.expect("fixed night time not set"),
        }
    }
    fn get_day_time(&self, date: NaiveDate) -> DateTime<chrono::Utc> {
        NaiveDateTime::new(date, self.day_time)
            .and_local_timezone(Local)
            .unwrap()
            .to_utc()
    }
    fn get_night_time(&self, date: NaiveDate) -> DateTime<chrono::Utc> {
        NaiveDateTime::new(date, self.night_time)
            .and_local_timezone(Local)
            .unwrap()
            .to_utc()
    }
}

#[derive(Debug)]
pub struct OutputState {
    pub mode: OutputMode,
    pub delay_in_seconds: i64,
    day_time_provider: Box<dyn TimeProvider>,
    night_time_provider: Box<dyn TimeProvider>,
}

impl OutputState {
    pub fn new(switch_mode: SwitchMode, location: Option<Location>) -> Self {
        let mut state = TimeProviderState::default();

        if let Some(location) = location {
            state.set_coord(location.latitude, location.longitude);
        }

        state.fixed_day_time = match switch_mode.day {
            TimeProviderMode::Fixed(time) => Some(time),
            _ => None,
        };
        state.fixed_night_time = match switch_mode.night {
            TimeProviderMode::Fixed(time) => Some(time),
            _ => None,
        };

        let day_time_provider: Box<dyn TimeProvider> = match switch_mode.day {
            TimeProviderMode::Auto => Box::new(AutoTimeProvider::new(&state)),
            TimeProviderMode::Fixed(_) => Box::new(FixedTimeProvider::new(&state)),
        };
        let night_time_provider: Box<dyn TimeProvider> = match switch_mode.night {
            TimeProviderMode::Auto => Box::new(AutoTimeProvider::new(&state)),
            TimeProviderMode::Fixed(_) => Box::new(FixedTimeProvider::new(&state)),
        };

        let (mode, delay_in_seconds) =
            get_next_mode_switch(&*day_time_provider, &*night_time_provider);
        Self {
            mode,
            delay_in_seconds,
            day_time_provider,
            night_time_provider,
        }
    }

    pub fn next(&mut self) {
        let (mode, delay_in_seconds) =
            get_next_mode_switch(&*self.day_time_provider, &*self.night_time_provider);
        self.mode = mode;
        self.delay_in_seconds = delay_in_seconds;
    }
}

fn get_next_mode_switch(
    day_time_provider: &dyn TimeProvider,
    night_time_provider: &dyn TimeProvider,
) -> (OutputMode, i64) {
    let date = Local::now().date_naive();
    let now = Utc::now();

    let day_time = day_time_provider.get_day_time(date);
    let night_time = night_time_provider.get_night_time(date);

    if day_time > night_time {
        panic!();
    }

    let mode: OutputMode;
    let until: DateTime<chrono::Utc>;
    if now < day_time {
        mode = OutputMode::Night;
        until = day_time;
    } else if now < night_time {
        mode = OutputMode::Day;
        until = night_time;
    } else {
        mode = OutputMode::Night;
        until = day_time_provider.get_day_time(date.succ_opt().unwrap())
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
                latitude: -1.2,
                longitude: 36.8,
            });

            const DAY_NIGHT_TIME: SwitchMode = SwitchMode {
                day: TimeProviderMode::Auto,
                night: TimeProviderMode::Auto,
            };

            #[test]
            fn morning() {
                set_time(0, 0, OFFSET);
                let mut event = OutputState::new(DAY_NIGHT_TIME, LOCATION);

                let sunrise = forward_time(event.delay_in_seconds, &OFFSET);
                assert_eq!(event.mode, OutputMode::Night);
                assert_eq!(sunrise.hour(), 6);
                assert!(sunrise.minute() > 15 && sunrise.minute() < 45);

                mock_chrono::set(sunrise);
                event.next();
                let sunset = forward_time(event.delay_in_seconds, &OFFSET);
                assert_eq!(event.mode, OutputMode::Day);
                assert_eq!(sunset.hour(), 18);
                assert!(sunset.minute() > 15 && sunset.minute() < 45);
            }

            #[test]
            fn noon() {
                set_time(13, 0, OFFSET);
                let mut event = OutputState::new(DAY_NIGHT_TIME, LOCATION);

                let sunset = forward_time(event.delay_in_seconds, &OFFSET);
                assert_eq!(event.mode, OutputMode::Day);
                assert_eq!(sunset.hour(), 18);
                assert!(sunset.minute() > 15 && sunset.minute() < 45);

                mock_chrono::set(sunset);
                event.next();
                let sunrise = forward_time(event.delay_in_seconds, &OFFSET);
                assert_eq!(event.mode, OutputMode::Night);
                assert_eq!(sunrise.hour(), 6);
                assert!(sunrise.minute() > 15 && sunrise.minute() < 45);
            }

            #[test]
            fn midnight() {
                set_time(23, 0, OFFSET);
                let mut event = OutputState::new(DAY_NIGHT_TIME, LOCATION);

                let sunrise = forward_time(event.delay_in_seconds, &OFFSET);
                assert_eq!(event.mode, OutputMode::Night);
                assert_eq!(sunrise.hour(), 6);
                assert!(sunrise.minute() > 15 && sunrise.minute() < 45);

                mock_chrono::set(sunrise);
                event.next();
                let sunset = forward_time(event.delay_in_seconds, &OFFSET);
                assert_eq!(event.mode, OutputMode::Day);
                assert_eq!(sunset.hour(), 18);
                assert!(sunset.minute() > 15 && sunset.minute() < 45);
            }
        }

        mod fixed {
            use super::*;

            const OFFSET: FixedOffset = FixedOffset::east_opt(0).unwrap();
            const LOCATION: Option<Location> = None;

            const DAY_NIGHT_TIME: SwitchMode = SwitchMode {
                day: TimeProviderMode::Fixed(NaiveTime::from_hms_opt(8, 0, 0).unwrap()),
                night: TimeProviderMode::Fixed(NaiveTime::from_hms_opt(19, 0, 0).unwrap()),
            };

            #[test]
            fn morning() {
                set_time(0, 0, OFFSET);
                let mut event = OutputState::new(DAY_NIGHT_TIME, LOCATION);

                let sunrise = forward_time(event.delay_in_seconds, &OFFSET);
                assert_eq!(event.mode, OutputMode::Night);
                assert_eq!(sunrise.hour(), 8);
                assert_eq!(sunrise.minute(), 0);

                mock_chrono::set(sunrise);
                event.next();
                let sunset = forward_time(event.delay_in_seconds, &OFFSET);
                assert_eq!(event.mode, OutputMode::Day);
                assert_eq!(sunset.hour(), 19);
                assert_eq!(sunset.minute(), 0);
            }

            #[test]
            fn noon() {
                set_time(13, 0, OFFSET);
                let mut event = OutputState::new(DAY_NIGHT_TIME, LOCATION);

                let sunset = forward_time(event.delay_in_seconds, &OFFSET);
                assert_eq!(event.mode, OutputMode::Day);
                assert_eq!(sunset.hour(), 19);
                assert_eq!(sunset.minute(), 0);

                mock_chrono::set(sunset);
                event.next();
                let sunrise = forward_time(event.delay_in_seconds, &OFFSET);
                assert_eq!(event.mode, OutputMode::Night);
                assert_eq!(sunrise.hour(), 8);
                assert_eq!(sunrise.minute(), 0);
            }

            #[test]
            fn midnight() {
                set_time(23, 0, OFFSET);
                let mut event = OutputState::new(DAY_NIGHT_TIME, LOCATION);

                let sunrise = forward_time(event.delay_in_seconds, &OFFSET);
                assert_eq!(event.mode, OutputMode::Night);
                assert_eq!(sunrise.hour(), 8);
                assert_eq!(sunrise.minute(), 0);

                mock_chrono::set(sunrise);
                event.next();
                let sunset = forward_time(event.delay_in_seconds, &OFFSET);
                assert_eq!(event.mode, OutputMode::Day);
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
