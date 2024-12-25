use crate::{
    config::{Latitude, Longitude},
    sun_time::{SunTime, Timestamp},
};
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};

pub struct LatLng {
    pub lat: Latitude,
    pub lng: Longitude,
    pub offset: i32,
}

pub const LONDON: LatLng = LatLng {
    lat: 51.51,
    lng: -0.12,
    offset: 0,
};

pub const NAIROBI: LatLng = LatLng {
    lat: -1.29,
    lng: 36.82,
    offset: 3,
};

fn get_offset_hour(offset: i32) -> FixedOffset {
    FixedOffset::east_opt(offset.clamp(-11, 12) * 3600).unwrap()
}

pub fn get_timestamp(month: u32, hour: u32, offset: i32) -> Timestamp {
    let date = NaiveDate::from_ymd_opt(2024, month, 1).unwrap();
    let time = NaiveTime::from_hms_opt(hour, 0, 0).unwrap();
    let datetime = NaiveDateTime::new(date, time);

    let tz_offset = get_offset_hour(offset);
    tz_offset
        .from_local_datetime(&datetime)
        .unwrap()
        .timestamp()
}

pub fn get_datetime(timestamp: Timestamp, offset: i32) -> DateTime<FixedOffset> {
    let tz = get_offset_hour(offset);
    DateTime::from_timestamp(timestamp, 0)
        .unwrap()
        .with_timezone(&tz)
}

pub struct SunTimeDate {
    pub sunrise: DateTime<FixedOffset>,
    pub sunset: DateTime<FixedOffset>,
}

impl SunTimeDate {
    pub fn new(SunTime { sunrise, sunset }: SunTime, offset: i32) -> Self {
        Self {
            sunrise: get_datetime(sunrise, offset),
            sunset: get_datetime(sunset, offset),
        }
    }
}
