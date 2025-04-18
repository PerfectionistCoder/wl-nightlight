#[cfg(not(test))]
use chrono::{Local, Utc};
#[cfg(test)]
use mock_chrono::{Local, Utc};

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta};
use sunrise::{
    Coordinates, SolarDay,
    SolarEvent::{self, Sunrise, Sunset},
};

use crate::{
    InternalError,
    config::{Location, Schedule, ScheduleType},
};

#[derive(PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum ColorMode {
    Day,
    Night,
}

#[cfg(not(tarpaulin_include))]
impl std::fmt::Display for ColorMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::Day => write!(f, "[day]"),
            Self::Night => write!(f, "[night]"),
        }
    }
}

trait Scheduler {
    fn get(&self, date: NaiveDate) -> DateTime<chrono::Utc>;
}

struct AutoScheduler {
    coordinates: Coordinates,
    event_type: SolarEvent,
}

impl Scheduler for AutoScheduler {
    fn get(&self, date: NaiveDate) -> DateTime<chrono::Utc> {
        SolarDay::new(self.coordinates, date).event_time(self.event_type)
    }
}

struct FixedScheduler {
    naive_time: NaiveTime,
}

impl Scheduler for FixedScheduler {
    fn get(&self, date: NaiveDate) -> DateTime<chrono::Utc> {
        NaiveDateTime::new(date, self.naive_time)
            .and_local_timezone(Local)
            .unwrap()
            .to_utc()
    }
}

struct RelativeScheduler {
    auto_scheduler: AutoScheduler,
    time_delta: TimeDelta,
}

impl Scheduler for RelativeScheduler {
    fn get(&self, date: NaiveDate) -> DateTime<chrono::Utc> {
        self.auto_scheduler.get(date) + self.time_delta
    }
}

pub struct ModeScheduler {
    pub mode: ColorMode,
    pub delay_ms: i64,
    day_scheduler: Box<dyn Scheduler>,
    night_scheduler: Box<dyn Scheduler>,
}

impl ModeScheduler {
    pub fn new(schedule: Schedule, location: Option<Location>) -> anyhow::Result<Self> {
        let coordinates = match (&schedule.day, &schedule.night) {
            (ScheduleType::Fixed(_), ScheduleType::Fixed(_)) => None,
            _ => {
                let location = location.ok_or(InternalError {
                    message: "Location is required",
                })?;
                Some(
                    Coordinates::new(location.latitude, location.longitude).ok_or(
                        InternalError {
                            message: "Coordinates are out of range",
                        },
                    )?,
                )
            }
        };

        let create_scheduler = |schedule_type: ScheduleType,
                                event_type: SolarEvent|
         -> anyhow::Result<Box<dyn Scheduler>> {
            let error = InternalError {
                message: "Coordinates are not set",
            };
            Ok(match schedule_type {
                ScheduleType::Auto => Box::new(AutoScheduler {
                    coordinates: coordinates.ok_or(error)?,
                    event_type,
                }),
                ScheduleType::Fixed(naive_time) => Box::new(FixedScheduler { naive_time }),
                ScheduleType::Relative(time_delta) => Box::new(RelativeScheduler {
                    auto_scheduler: AutoScheduler {
                        coordinates: coordinates.ok_or(error)?,
                        event_type,
                    },
                    time_delta,
                }),
            })
        };
        let day_scheduler = create_scheduler(schedule.day, Sunrise)?;
        let night_scheduler = create_scheduler(schedule.night, Sunset)?;

        let (mode, delay_ms) = get_next_schedule(&*day_scheduler, &*night_scheduler);

        Ok(Self {
            mode,
            delay_ms,
            day_scheduler,
            night_scheduler,
        })
    }

    pub fn next(&mut self) {
        let (mode, delay_ms) = get_next_schedule(&*self.day_scheduler, &*self.night_scheduler);
        self.mode = mode;
        self.delay_ms = delay_ms;
    }
}

fn get_next_schedule(
    day_scheduler: &dyn Scheduler,
    night_scheduler: &dyn Scheduler,
) -> (ColorMode, i64) {
    let date = Local::now().date_naive();
    let now = Utc::now();

    let day_date_time = day_scheduler.get(date);
    let night_date_time = night_scheduler.get(date);

    if day_date_time > night_date_time {
        log::error!(
            "`schedule.day` ({}) occurs after `schedule.night` ({})",
            day_date_time.with_timezone(&Local).format("%H:%M"),
            night_date_time.with_timezone(&Local).format("%H:%M"),
        );
    }

    let mode: ColorMode;
    let until: DateTime<chrono::Utc>;
    if now < day_date_time {
        mode = ColorMode::Night;
        until = day_date_time;
    } else if now < night_date_time {
        mode = ColorMode::Day;
        until = night_date_time;
    } else {
        mode = ColorMode::Night;
        until = day_scheduler.get(date.succ_opt().unwrap());
    }
    (mode, (until - now).num_milliseconds() + 1)
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

    fn forward_time(millis: i64, offset: &FixedOffset) -> DateTime<FixedOffset> {
        (mock_chrono::Local::now() + TimeDelta::milliseconds(millis)).with_timezone(offset)
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
        use std::ops::Range;

        use super::*;

        const NAIROBI_OFFSET: FixedOffset = FixedOffset::east_opt(3 * HOUR).unwrap();
        const NAIROBI_LOCATION: Option<Location> = Some(Location {
            latitude: -1.2,
            longitude: 36.8,
        });

        fn assert_next_event(
            event: &mut ModeScheduler,
            expected_mode: ColorMode,
            expected_hour: u32,
            minute_range: Range<u32>,
            offset: &FixedOffset,
        ) {
            let dt = forward_time(event.delay_ms, offset);
            assert_eq!(event.mode, expected_mode);
            assert_eq!(dt.hour(), expected_hour);
            assert!(minute_range.contains(&dt.minute()),);

            mock_chrono::set(dt);
            event.next();
        }

        mod auto {
            use super::*;

            const DAY_NIGHT_TIME: Schedule = Schedule {
                day: ScheduleType::Auto,
                night: ScheduleType::Auto,
            };
            const SUNRISE: u32 = 6;
            const SUNSET: u32 = 18;
            const RANGE: Range<u32> = 15..45;
            const OFFSET: &FixedOffset = &NAIROBI_OFFSET;

            #[test]
            fn morning() {
                set_time(0, 0, NAIROBI_OFFSET);
                let mut event = ModeScheduler::new(DAY_NIGHT_TIME, NAIROBI_LOCATION).unwrap();

                assert_next_event(&mut event, ColorMode::Night, SUNRISE, RANGE, OFFSET);
                assert_next_event(&mut event, ColorMode::Day, SUNSET, RANGE, OFFSET);
            }

            #[test]
            fn noon() {
                set_time(13, 0, NAIROBI_OFFSET);
                let mut event = ModeScheduler::new(DAY_NIGHT_TIME, NAIROBI_LOCATION).unwrap();

                assert_next_event(&mut event, ColorMode::Day, SUNSET, RANGE, OFFSET);
                assert_next_event(&mut event, ColorMode::Night, SUNRISE, RANGE, OFFSET);
            }

            #[test]
            fn midnight() {
                set_time(23, 0, NAIROBI_OFFSET);
                let mut event = ModeScheduler::new(DAY_NIGHT_TIME, NAIROBI_LOCATION).unwrap();

                assert_next_event(&mut event, ColorMode::Night, SUNRISE, RANGE, OFFSET);
                assert_next_event(&mut event, ColorMode::Day, SUNSET, RANGE, OFFSET);
            }
        }

        mod fixed {
            use super::*;

            const OFFSET: &FixedOffset = &FixedOffset::east_opt(0).unwrap();
            const LOCATION: Option<Location> = None;

            const DAY_NIGHT_TIME: Schedule = Schedule {
                day: ScheduleType::Fixed(NaiveTime::from_hms_opt(8, 0, 0).unwrap()),
                night: ScheduleType::Fixed(NaiveTime::from_hms_opt(19, 0, 0).unwrap()),
            };
            const SUNRISE: u32 = 8;
            const SUNSET: u32 = 19;
            const RANGE: Range<u32> = 0..1;

            #[test]
            fn morning() {
                set_time(0, 0, *OFFSET);
                let mut event = ModeScheduler::new(DAY_NIGHT_TIME, LOCATION).unwrap();

                assert_next_event(&mut event, ColorMode::Night, SUNRISE, RANGE, OFFSET);
                assert_next_event(&mut event, ColorMode::Day, SUNSET, RANGE, OFFSET);
            }

            #[test]
            fn noon() {
                set_time(13, 0, *OFFSET);
                let mut event = ModeScheduler::new(DAY_NIGHT_TIME, LOCATION).unwrap();

                assert_next_event(&mut event, ColorMode::Day, SUNSET, RANGE, OFFSET);
                assert_next_event(&mut event, ColorMode::Night, SUNRISE, RANGE, OFFSET);
            }

            #[test]
            fn midnight() {
                set_time(23, 0, *OFFSET);
                let mut event = ModeScheduler::new(DAY_NIGHT_TIME, LOCATION).unwrap();

                assert_next_event(&mut event, ColorMode::Night, SUNRISE, RANGE, OFFSET);
                assert_next_event(&mut event, ColorMode::Day, SUNSET, RANGE, OFFSET);
            }
        }

        mod relative {
            use super::*;

            const DAY_NIGHT_TIME: Schedule = Schedule {
                day: ScheduleType::Relative(TimeDelta::hours(1)),
                night: ScheduleType::Relative(TimeDelta::hours(-2)),
            };
            const SUNRISE: u32 = 7;
            const SUNSET: u32 = 16;
            const RANGE: Range<u32> = 15..45;
            const OFFSET: &FixedOffset = &NAIROBI_OFFSET;

            #[test]
            fn morning() {
                set_time(0, 0, NAIROBI_OFFSET);
                let mut event = ModeScheduler::new(DAY_NIGHT_TIME, NAIROBI_LOCATION).unwrap();

                assert_next_event(&mut event, ColorMode::Night, SUNRISE, RANGE, OFFSET);
                assert_next_event(&mut event, ColorMode::Day, SUNSET, RANGE, OFFSET);
            }

            #[test]
            fn noon() {
                set_time(13, 0, NAIROBI_OFFSET);
                let mut event = ModeScheduler::new(DAY_NIGHT_TIME, NAIROBI_LOCATION).unwrap();

                assert_next_event(&mut event, ColorMode::Day, SUNSET, RANGE, OFFSET);
                assert_next_event(&mut event, ColorMode::Night, SUNRISE, RANGE, OFFSET);
            }

            #[test]
            fn midnight() {
                set_time(23, 0, NAIROBI_OFFSET);
                let mut event = ModeScheduler::new(DAY_NIGHT_TIME, NAIROBI_LOCATION).unwrap();

                assert_next_event(&mut event, ColorMode::Night, SUNRISE, RANGE, OFFSET);
                assert_next_event(&mut event, ColorMode::Day, SUNSET, RANGE, OFFSET);
            }
        }

        mod auto_fixed {
            use super::*;

            const OFFSET: FixedOffset = NAIROBI_OFFSET;

            #[test]
            fn day_auto_night_fixed() {
                set_time(0, 0, OFFSET);
                let mut event = ModeScheduler::new(
                    Schedule {
                        day: ScheduleType::Auto,
                        night: ScheduleType::Fixed(NaiveTime::from_hms_opt(19, 0, 0).unwrap()),
                    },
                    NAIROBI_LOCATION,
                )
                .unwrap();

                assert_next_event(&mut event, ColorMode::Night, 6, 15..45, &OFFSET);
                assert_next_event(&mut event, ColorMode::Day, 19, 0..1, &OFFSET);
            }

            #[test]
            fn day_fixed_night_auto() {
                set_time(0, 0, OFFSET);
                let mut event = ModeScheduler::new(
                    Schedule {
                        day: ScheduleType::Fixed(NaiveTime::from_hms_opt(7, 0, 0).unwrap()),
                        night: ScheduleType::Auto,
                    },
                    NAIROBI_LOCATION,
                )
                .unwrap();

                assert_next_event(&mut event, ColorMode::Night, 7, 0..1, &OFFSET);
                assert_next_event(&mut event, ColorMode::Day, 18, 15..45, &OFFSET);
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

    #[derive(Clone)]
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
