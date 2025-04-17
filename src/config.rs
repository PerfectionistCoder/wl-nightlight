use std::{
    fmt::{self, Display},
    ops::Deref,
};

use chrono::NaiveTime;
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

fn validate_switch_mode(value: &str) -> Result<(), ValidationError> {
    NaiveTime::parse_from_str(value, "%H:%M")
        .map(|_| ())
        .map_err(|_| ValidationError::new("time"))
}

#[derive(Deserialize, Debug, Validate)]
pub struct SwitchModeConfig {
    #[validate(custom(function = "validate_switch_mode"))]
    day: Option<String>,
    #[validate(custom(function = "validate_switch_mode"))]
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
    switch_mode: Option<SwitchModeConfig>,
}

#[derive(Error, Debug)]
#[cfg_attr(test, derive(PartialEq))]
enum ConfigError {
    Invalid(ValidationErrors),
    MissingLocation,
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
                    String::from("[") + field
                } else {
                    format!("{}.{}", path_prefix, field)
                };

                match error_kind {
                    ValidationErrorsKind::Field(errors) => {
                        for error in errors {
                            let detail = match error.code.deref() {
                                "range" => format!(
                                    "is not in range [{}, {}]",
                                    error.params["min"], error.params["max"]
                                ),
                                "time" => "is not in format `HH:MM`".to_string(),
                                _ => panic!(),
                            };
                            writeln!(f, "{}] {}", field_path, detail)?;
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
            Self::Invalid(v_e) => print_errors(v_e, f, String::new()),
            Self::MissingLocation => writeln!(
                f,
                "[location] is required when [switch-mode.day] or [switch-mode.night] is unset"
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
            .map_err(ConfigError::Invalid)
            .map(|_| self)?
            .parse()
    }

    fn parse(self) -> anyhow::Result<Config> {
        fn fill_color(color: Option<ColorConfig>) -> Color {
            let default = Color::default();
            color.map_or(default, |c| Color {
                temperature: c.temperature.unwrap_or(default.temperature),
                gamma: c.gamma.unwrap_or(default.gamma),
                brightness: c.brightness.unwrap_or(default.brightness),
                inverted: c.inverted.unwrap_or(default.inverted),
            })
        }

        fn parse_time_mode(time: &str) -> anyhow::Result<TimeProviderMode> {
            Ok(TimeProviderMode::Fixed(NaiveTime::parse_from_str(
                time, "%H:%M",
            )?))
        }

        let day_color = fill_color(self.day);
        let night_color = fill_color(self.night);

        let day_mode: TimeProviderMode;
        let night_mode: TimeProviderMode;
        match self.switch_mode {
            None => {
                day_mode = TimeProviderMode::Auto;
                night_mode = TimeProviderMode::Auto;
            }
            Some(switch_mode) => {
                fn fill(mode_time: Option<String>) -> anyhow::Result<TimeProviderMode> {
                    mode_time.map_or(Ok(TimeProviderMode::Auto), |time| parse_time_mode(&time))
                }
                day_mode = fill(switch_mode.day)?;
                night_mode = fill(switch_mode.night)?;
            }
        }

        if !(day_mode.is_fixed() && night_mode.is_fixed()) && self.location.is_none() {
            Err(ConfigError::MissingLocation)?
        }

        Ok(Config {
            day: day_color,
            night: night_color,
            location: self.location,
            switch_mode: SwitchMode {
                day: day_mode,
                night: night_mode,
            },
        })
    }
}

#[derive(PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum TimeProviderMode {
    Auto,
    Fixed(NaiveTime),
}

impl TimeProviderMode {
    fn is_fixed(&self) -> bool {
        if let Self::Fixed(_) = self {
            return true;
        }
        false
    }
}

#[cfg_attr(test, derive(Debug))]
pub struct SwitchMode {
    pub day: TimeProviderMode,
    pub night: TimeProviderMode,
}

#[cfg_attr(test, derive(Debug))]
pub struct Config {
    pub day: Color,
    pub night: Color,
    pub location: Option<Location>,
    pub switch_mode: SwitchMode,
}

#[cfg(test)]
mod test {
    use core::panic;

    use validator::ValidationErrorsKind;

    use super::*;

    fn cmp_err<T>(result: anyhow::Result<T>, error: ConfigError) {
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
        assert_eq!(config.switch_mode.day, TimeProviderMode::Auto);
        assert_eq!(config.switch_mode.night, TimeProviderMode::Auto);
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
            cmp_err(
                RawConfig::read(file).unwrap().check(),
                ConfigError::MissingLocation,
            );
        }

        #[test]
        fn day_auto_night_fixed() {
            let file = "
                [switch-mode]
                night = \"00:00\"
            ";
            cmp_err(
                RawConfig::read(file).unwrap().check(),
                ConfigError::MissingLocation,
            );
        }

        #[test]
        fn day_fixed_night_auto() {
            let file = "
                [switch-mode]
                day = \"00:00\"
            ";
            cmp_err(
                RawConfig::read(file).unwrap().check(),
                ConfigError::MissingLocation,
            );
        }

        #[test]
        fn day_night_fixed() {
            let file = "
                [switch-mode]
                day = \"00:00\"
                night = \"00:00\"
            ";
            RawConfig::read(file).unwrap().check().unwrap();
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
                Some(ConfigError::Invalid(ValidationErrors(map)))
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
                Some(ConfigError::Invalid(ValidationErrors(map)))
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

                    [switch-mode]
                    night = \"19:30\"
                ";
            let config = RawConfig::read(file).unwrap().check().unwrap();
            assert_eq!(config.switch_mode.day, TimeProviderMode::Auto);
            assert_eq!(
                config.switch_mode.night,
                TimeProviderMode::Fixed(NaiveTime::from_hms_opt(19, 30, 0).unwrap())
            );
        }

        #[test]
        fn day_fixed_night_auto() {
            let file = "
                    [location]
                    latitude = 0
                    longitude = 0

                    [switch-mode]
                    day = \"08:30\"
                ";
            let config = RawConfig::read(file).unwrap().check().unwrap();
            assert_eq!(
                config.switch_mode.day,
                TimeProviderMode::Fixed(NaiveTime::from_hms_opt(8, 30, 0).unwrap())
            );
            assert_eq!(config.switch_mode.night, TimeProviderMode::Auto);
        }
    }

    mod parse_time {
        use super::*;

        #[test]
        fn random_string() {
            let file = "
                [switch-mode]
                day = \"foo\"                
                night = \"bar\"
            ";

            assert!(matches!(
                RawConfig::read(file).unwrap().check(),
                Err(err) if matches!(
                    err.downcast_ref::<ConfigError>(),
                    Some(ConfigError::Invalid(ValidationErrors(map)))
                        if matches!(
                         map.get("switch_mode"),
                         Some(ValidationErrorsKind::Struct(errs))
                          if errs.errors().contains_key("day") && errs.errors().contains_key("night")
                        )
                )
            ));
        }

        #[test]
        fn invalid_time() {
            let file = "
                [switch-mode]
                day = \"25:00\"                
                night = \"00:61\"
            ";

            assert!(matches!(
                RawConfig::read(file).unwrap().check(),
                Err(err) if matches!(
                    err.downcast_ref::<ConfigError>(),
                    Some(ConfigError::Invalid(ValidationErrors(map)))
                        if matches!(
                         map.get("switch_mode"),
                         Some(ValidationErrorsKind::Struct(errs))
                          if errs.errors().contains_key("day") && errs.errors().contains_key("night")
                        )
                )
            ));
        }
    }
}
