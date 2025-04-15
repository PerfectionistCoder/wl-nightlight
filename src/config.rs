use std::fmt::Display;

use chrono::NaiveTime;
use serde::Deserialize;
use thiserror::Error;
use validator::{Validate, ValidationErrors};

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

#[derive(Deserialize, Debug, Validate)]
pub struct SwitchModeConfig {
    pub day: Option<String>,
    pub night: Option<String>,
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
    pub location: Option<Location>,
    #[validate(nested)]
    pub switch_mode: Option<SwitchModeConfig>,
}

#[derive(Error, Debug)]
#[cfg_attr(test, derive(PartialEq))]
enum ConfigError {
    Invalid(ValidationErrors),
    MissingLocation,
    InvalidTime(String),
}

impl Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{:?}", self)
    }
}

impl RawConfig {
    pub fn read(file: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(file)
    }

    pub fn check(self) -> anyhow::Result<Self> {
        Ok(self
            .validate()
            .map_err(ConfigError::Invalid)
            .map(|_| self)?)
    }

    pub fn parse(self) -> anyhow::Result<Config> {
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
            fn parse_time(time: &str) -> anyhow::Result<NaiveTime> {
                Ok(NaiveTime::parse_from_str(time, "%H:%M")
                    .map_err(|_| ConfigError::InvalidTime(time.to_string()))?)
            }
            Ok(TimeProviderMode::Fixed(parse_time(time)?))
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
                day_mode = switch_mode
                    .day
                    .map_or(Ok(TimeProviderMode::Auto), |time| parse_time_mode(&time))?;
                night_mode = switch_mode
                    .night
                    .map_or(Ok(TimeProviderMode::Auto), |time| parse_time_mode(&time))?;
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

#[derive(PartialEq, Eq, Debug)]
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

#[derive(Debug)]
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
    use std::collections::HashMap;

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
        let config = RawConfig::read(file)
            .unwrap()
            .check()
            .unwrap()
            .parse()
            .unwrap();
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
        let config = RawConfig::read(file)
            .unwrap()
            .check()
            .unwrap()
            .parse()
            .unwrap();
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
                RawConfig::read(file).unwrap().check().unwrap().parse(),
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
                RawConfig::read(file).unwrap().check().unwrap().parse(),
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
                RawConfig::read(file).unwrap().check().unwrap().parse(),
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
            RawConfig::read(file)
                .unwrap()
                .check()
                .unwrap()
                .parse()
                .unwrap();
        }
    }

    #[test]
    fn latitude() {
        let file = "
                [location]
                latitude = 90.1
                longitude = 0
            ";

        assert!(RawConfig::read(file).unwrap().check().is_err_and(|e| {
            matches!(
                e.downcast::<ConfigError>().unwrap(),
                ConfigError::Invalid(_)
            )
        }),);
    }

    #[test]
    fn longitude() {
        let file = "
                [location]
                latitude = 0
                longitude = -180.1
            ";

        assert!(RawConfig::read(file).unwrap().check().is_err_and(|e| {
            matches!(
                e.downcast::<ConfigError>().unwrap(),
                ConfigError::Invalid(_)
            )
        }),);
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

    #[test]
    fn fixed_auto() {
        let file = "
                    [location]
                    latitude = 0
                    longitude = 0

                    [switch-mode]
                    night = \"19:30\"
                ";
        let config = RawConfig::read(file)
            .unwrap()
            .check()
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(config.switch_mode.day, TimeProviderMode::Auto);
        assert_eq!(
            config.switch_mode.night,
            TimeProviderMode::Fixed(NaiveTime::from_hms_opt(19, 30, 0).unwrap())
        );
    }
}
