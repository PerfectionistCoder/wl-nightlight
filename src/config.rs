use std::fmt::{self, Display};

use chrono::{NaiveTime, TimeDelta, Timelike};
use serde::Deserialize;
use thiserror::Error;
use validator::{Validate, ValidationError, ValidationErrors, ValidationErrorsKind};

use crate::color::Color;

#[derive(Deserialize, Debug, Validate)]
struct ColorConfig {
    #[validate(range(min = 1000, max = 10000))]
    temperature: Option<u16>,
    #[validate(range(min = 0.0))]
    gamma: Option<f64>,
    #[validate(range(min = 0.0))]
    brightness: Option<f64>,
    inverted: Option<bool>,
}

#[derive(Deserialize, Debug, Validate)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Location {
    #[validate(range(min = -90.0, max = 90.0))]
    pub latitude: f64,
    #[validate(range(min = -180.0, max = 180.0))]
    pub longitude: f64,
}

fn parse_schedule(time_str: &str) -> Result<ScheduleType, ValidationError> {
    let first_char = time_str.chars().next();
    Ok(match first_char {
        Some(c) if c == '+' || c == '-' => {
            let sign = if first_char == Some('+') { 1 } else { -1 };
            let naive_time = NaiveTime::parse_from_str(&time_str[1..], "%H:%M")
                .map_err(|_| ValidationError::new("relative_time"))?;
            let time_delta = (TimeDelta::hours(naive_time.hour() as i64)
                + TimeDelta::minutes(naive_time.minute() as i64))
                * sign;
            ScheduleType::Relative(time_delta)
        }
        _ => ScheduleType::Fixed(
            NaiveTime::parse_from_str(time_str, "%H:%M")
                .map_err(|_| ValidationError::new("fixed_time"))?,
        ),
    })
}
fn validate_schedule(time_str: &str) -> Result<(), ValidationError> {
    parse_schedule(time_str).map(|_| ())
}

#[derive(Deserialize, Debug, Validate)]
pub struct ScheduleConfig {
    #[validate(custom(function = "validate_schedule"))]
    day: Option<String>,
    #[validate(custom(function = "validate_schedule"))]
    night: Option<String>,
}

#[derive(Deserialize, Debug, Validate)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct RawConfig {
    #[validate(nested)]
    day: Option<ColorConfig>,
    #[validate(nested)]
    night: Option<ColorConfig>,
    #[validate(nested)]
    location: Option<Location>,
    #[validate(nested)]
    schedule: Option<ScheduleConfig>,
}

#[derive(Error, Debug)]
#[cfg_attr(test, derive(PartialEq))]
enum ConfigError {
    ValidationError(ValidationErrors),
    LocationError,
}

#[cfg(not(tarpaulin_include))]
impl Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        fn print_errors(
            errors: &ValidationErrors,
            f: &mut fmt::Formatter<'_>,
            path_prefix: String,
        ) -> fmt::Result {
            for (field, error_kind) in &errors.0 {
                let field_path = if path_prefix.is_empty() {
                    field.to_string()
                } else {
                    format!("{}.{}", path_prefix, field)
                };

                match error_kind {
                    ValidationErrorsKind::Field(errors) => {
                        for error in errors {
                            let message = match &*error.code {
                                "range"
                                    if error.params.contains_key("max")
                                        && error.params.contains_key("min") =>
                                {
                                    format!(
                                        "in range {}-{}",
                                        error.params["min"], error.params["max"]
                                    )
                                }
                                "range" if error.params.contains_key("min") => {
                                    format!("greater than {}", error.params["min"])
                                }
                                "fixed_time" => "in format 'HH:MM'".to_string(),
                                "relative_time" => "in format '+HH:MM' or '-HH:MM'".to_string(),
                                _ => return Err(std::fmt::Error),
                            };
                            writeln!(
                                f,
                                "value {} in field `{}` is not {}",
                                error.params["value"], field_path, message
                            )?;
                        }
                    }
                    ValidationErrorsKind::Struct(nested_errors) => {
                        print_errors(nested_errors, f, field_path)?;
                    }
                    ValidationErrorsKind::List(items) => {
                        for (index, nested_errors) in items {
                            let indexed_path = format!("{}[{}]", field_path, index);
                            print_errors(nested_errors, f, indexed_path)?;
                        }
                    }
                }
            }
            Ok(())
        }

        match self {
            Self::ValidationError(v_e) => print_errors(v_e, f, String::new()),
            Self::LocationError => writeln!(
                f,
                "[location] is required when [schedule.day] or [schedule.night] is unset"
            ),
        }
    }
}

impl RawConfig {
    pub fn read(file: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(file)
    }

    pub fn check(self) -> anyhow::Result<Config> {
        self.validate()
            .map_err(ConfigError::ValidationError)
            .map(|_| self)?
            .parse()
    }

    fn parse(self) -> anyhow::Result<Config> {
        fn apply_default_color(color: Option<ColorConfig>) -> Color {
            let default = Color::default();
            color.map_or(default, |c| Color {
                temperature: c.temperature.unwrap_or(default.temperature),
                gamma: c.gamma.unwrap_or(default.gamma),
                brightness: c.brightness.unwrap_or(default.brightness),
                inverted: c.inverted.unwrap_or(default.inverted),
            })
        }

        let day_color = apply_default_color(self.day);
        let night_color = apply_default_color(self.night);

        let day_type: ScheduleType;
        let night_type: ScheduleType;
        match self.schedule {
            None => {
                day_type = ScheduleType::Auto;
                night_type = ScheduleType::Auto;
            }
            Some(schedule) => {
                fn resolve_schedule_str(
                    schedule_str: Option<String>,
                ) -> anyhow::Result<ScheduleType> {
                    schedule_str.map_or(Ok(ScheduleType::Auto), |time_str| {
                        Ok(parse_schedule(&time_str)?)
                    })
                }
                day_type = resolve_schedule_str(schedule.day)?;
                night_type = resolve_schedule_str(schedule.night)?;
            }
        }

        if !(day_type.is_fixed() && night_type.is_fixed()) && self.location.is_none() {
            Err(ConfigError::LocationError)?
        }

        Ok(Config {
            day: day_color,
            night: night_color,
            location: self.location,
            schedule: Schedule {
                day: day_type,
                night: night_type,
            },
        })
    }
}

#[derive(PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum ScheduleType {
    Auto,
    Fixed(NaiveTime),
    Relative(TimeDelta),
}

impl ScheduleType {
    fn is_fixed(&self) -> bool {
        if let Self::Fixed(_) = self {
            return true;
        }
        false
    }
}

#[cfg_attr(test, derive(Debug))]
pub struct Schedule {
    pub day: ScheduleType,
    pub night: ScheduleType,
}

#[cfg_attr(test, derive(Debug))]
pub struct Config {
    pub day: Color,
    pub night: Color,
    pub location: Option<Location>,
    pub schedule: Schedule,
}

#[cfg(test)]
mod test {
    use core::panic;

    use validator::ValidationErrorsKind;

    use super::*;

    fn assert_same_error<T>(result: anyhow::Result<T>, error: ConfigError) {
        if let Err(err) = result {
            assert_eq!(err.downcast::<ConfigError>().unwrap(), error)
        } else {
            panic!()
        }
    }

    #[test]
    fn minimal() {
        let file = "
                [location]
                latitude = 0
                longitude = 0
            ";
        let config = RawConfig::read(file).unwrap().check().unwrap();
        assert_eq!(config.day, Color::default());
        assert_eq!(config.night, Color::default());
        assert_eq!(
            config.location,
            Some(Location {
                latitude: 0.0,
                longitude: 0.0
            })
        );
        assert_eq!(config.schedule.day, ScheduleType::Auto);
        assert_eq!(config.schedule.night, ScheduleType::Auto);
    }

    #[test]
    fn color_default() {
        let file = "
                [day]
                temperature = 1000
                inverted = true

                [night]
                brightness = 0.5
                gamma = 0.4

                [location]
                latitude = 0
                longitude = 0
            ";
        let config = RawConfig::read(file).unwrap().check().unwrap();
        assert_eq!(
            config.day,
            Color {
                temperature: 1000,
                inverted: true,
                ..Color::default()
            }
        );
        assert_eq!(
            config.night,
            Color {
                brightness: 0.5,
                gamma: 0.4,
                ..Color::default()
            }
        );
    }

    mod location {
        use super::*;

        #[test]
        fn day_night_auto() {
            let file = "";
            assert_same_error(
                RawConfig::read(file).unwrap().check(),
                ConfigError::LocationError,
            );
        }

        #[test]
        fn day_auto_night_fixed() {
            let file = "
                [schedule]
                night = \"00:00\"
            ";
            assert_same_error(
                RawConfig::read(file).unwrap().check(),
                ConfigError::LocationError,
            );
        }

        #[test]
        fn day_fixed_night_auto() {
            let file = "
                [schedule]
                day = \"00:00\"
            ";
            assert_same_error(
                RawConfig::read(file).unwrap().check(),
                ConfigError::LocationError,
            );
        }

        #[test]
        fn day_night_fixed() {
            let file = "
                [schedule]
                day = \"00:00\"
                night = \"00:00\"
            ";
            RawConfig::read(file).unwrap().check().unwrap();
        }

        #[test]
        fn day_auto_night_relative() {
            let file = "
                [schedule]
                night = \"+01:00\"
            ";
            assert_same_error(
                RawConfig::read(file).unwrap().check(),
                ConfigError::LocationError,
            );
        }

        #[test]
        fn day_relative_night_fixed() {
            let file = "
                [schedule]
                day = \"-01:00\"
                night = \"19:00\"
            ";
            assert_same_error(
                RawConfig::read(file).unwrap().check(),
                ConfigError::LocationError,
            );
        }

        #[test]
        fn day_night_relative() {
            let file = "
                [schedule]
                day = \"+02:00\"
                night = \"+02:00\"
            ";
            assert_same_error(
                RawConfig::read(file).unwrap().check(),
                ConfigError::LocationError,
            );
        }
    }

    #[test]
    fn latitude() {
        let file = "
                [location]
                latitude = 91
                longitude = 0
            ";

        assert!(matches!(
            RawConfig::read(file).unwrap().check(),
            Err(err) if matches!(
                err.downcast_ref::<ConfigError>(),
                Some(ConfigError::ValidationError(ValidationErrors(map)))
                    if matches!(
                        map.get("location"),
                        Some(ValidationErrorsKind::Struct(errs))
                            if errs.errors().contains_key("latitude")
                    )
            )
        ));
    }

    #[test]
    fn longitude() {
        let file = "
                [location]
                latitude = 0
                longitude = -180.1
            ";

        assert!(matches!(
            RawConfig::read(file).unwrap().check(),
            Err(err) if matches!(
                err.downcast_ref::<ConfigError>(),
                Some(ConfigError::ValidationError(ValidationErrors(map)))
                    if matches!(
                        map.get("location"),
                        Some(ValidationErrorsKind::Struct(errs))
                            if errs.errors().contains_key("longitude")
                    )
            )
        ));
    }

    #[test]
    fn unknown_field() {
        let file = "
                [location]
                latitude = 0
                longitude = 0

                [unknown]
            ";
        assert!(RawConfig::read(file).is_err());
    }

    mod time_provider {
        use super::*;

        #[test]
        fn day_auto_night_fixed() {
            let file = "
                    [location]
                    latitude = 0
                    longitude = 0

                    [schedule]
                    night = \"19:30\"
                ";
            let config = RawConfig::read(file).unwrap().check().unwrap();
            assert_eq!(config.schedule.day, ScheduleType::Auto);
            assert_eq!(
                config.schedule.night,
                ScheduleType::Fixed(NaiveTime::from_hms_opt(19, 30, 0).unwrap())
            );
        }

        #[test]
        fn day_fixed_night_auto() {
            let file = "
                    [location]
                    latitude = 0
                    longitude = 0

                    [schedule]
                    day = \"08:30\"
                ";
            let config = RawConfig::read(file).unwrap().check().unwrap();
            assert_eq!(
                config.schedule.day,
                ScheduleType::Fixed(NaiveTime::from_hms_opt(8, 30, 0).unwrap())
            );
            assert_eq!(config.schedule.night, ScheduleType::Auto);
        }

        #[test]
        fn day_auto_night_relative() {
            let file = "
                    [location]
                    latitude = 0
                    longitude = 0

                    [schedule]
                    night = \"-00:30\"
                ";
            let config = RawConfig::read(file).unwrap().check().unwrap();
            assert_eq!(config.schedule.day, ScheduleType::Auto);
            assert_eq!(
                config.schedule.night,
                ScheduleType::Relative(-TimeDelta::minutes(30))
            );
        }

        #[test]
        fn day_fixed_night_relative() {
            let file = "
                    [location]
                    latitude = 0
                    longitude = 0

                    [schedule]
                    day = \"09:00\"
                    night = \"+00:00\"
                ";
            let config = RawConfig::read(file).unwrap().check().unwrap();
            assert_eq!(
                config.schedule.day,
                ScheduleType::Fixed(NaiveTime::from_hms_opt(9, 0, 0).unwrap())
            );
            assert_eq!(
                config.schedule.night,
                ScheduleType::Relative(TimeDelta::seconds(0))
            );
        }
    }

    mod parse_time {
        use super::*;

        #[test]
        fn random_string() {
            let file = "
                [schedule]
                day = \"foo\"                
                night = \"bar\"
            ";

            assert!(matches!(
                RawConfig::read(file).unwrap().check(),
                Err(err) if matches!(
                    err.downcast_ref::<ConfigError>(),
                    Some(ConfigError::ValidationError(ValidationErrors(map)))
                        if matches!(
                         map.get("schedule"),
                         Some(ValidationErrorsKind::Struct(errs))
                          if errs.errors().contains_key("day") && errs.errors().contains_key("night")
                        )
                )
            ));
        }

        mod invalid_time {
            use super::*;

            #[test]
            fn fixed_time() {
                let file = "
                [schedule]
                day = \"25:00\"                
                night = \"00:61\"
            ";

                assert!(matches!(
                    RawConfig::read(file).unwrap().check(),
                    Err(err) if matches!(
                        err.downcast_ref::<ConfigError>(),
                        Some(ConfigError::ValidationError(ValidationErrors(map)))
                            if matches!(
                             map.get("schedule"),
                             Some(ValidationErrorsKind::Struct(errs))
                              if errs.errors().contains_key("day") && errs.errors().contains_key("night")
                            )
                    )
                ));
            }

            #[test]
            fn relative_time() {
                let file = "
                [schedule]
                day = \"+25:00\"                
                night = \"-00:61\"
            ";

                assert!(matches!(
                    RawConfig::read(file).unwrap().check(),
                    Err(err) if matches!(
                        err.downcast_ref::<ConfigError>(),
                        Some(ConfigError::ValidationError(ValidationErrors(map)))
                            if matches!(
                             map.get("schedule"),
                             Some(ValidationErrorsKind::Struct(errs))
                              if errs.errors().contains_key("day") && errs.errors().contains_key("night")
                            )
                    )
                ));
            }
        }
    }
}
