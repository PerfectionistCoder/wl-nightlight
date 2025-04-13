use chrono::NaiveTime;
use serde::Deserialize;

use crate::color::DisplayColor;

#[derive(Deserialize, Debug)]
struct DisplayColorConfig {
    temperature: Option<u16>,
    gamma: Option<f64>,
    brightness: Option<f64>,
    inverted: Option<bool>,
}

#[derive(Deserialize, Debug)]
pub struct LocationConfig {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Deserialize, Debug)]
pub struct SwitchModeConfig {
    pub day: String,
    pub night: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct RawConfig {
    day: Option<DisplayColorConfig>,
    night: Option<DisplayColorConfig>,
    pub location: Option<LocationConfig>,
    pub switch_mode: SwitchModeConfig,
}

#[derive(PartialEq, Eq, Debug)]
pub enum TimeProviderMode {
    Auto,
    Fixed(NaiveTime),
}

#[derive(Debug)]
pub struct SwitchMode {
    pub day: TimeProviderMode,
    pub night: TimeProviderMode,
}

pub struct Config {
    pub day: DisplayColor,
    pub night: DisplayColor,
    pub location: Option<LocationConfig>,
    pub switch_mode: SwitchMode,
}

impl RawConfig {
    #[cfg(not(test))]
    pub fn new(path: &str) -> Self {
        use std::fs::read_to_string;

        let file = read_to_string(path).unwrap();
        toml::from_str(&file).unwrap()
    }

    #[cfg(test)]
    pub fn new(file: &str) -> Self {
        toml::from_str(file).unwrap()
    }

    pub fn parse(self) -> Config {
        fn merge_color(color: Option<DisplayColorConfig>) -> DisplayColor {
            let default = DisplayColor::default();
            color.map_or(default, |c| DisplayColor {
                temperature: c.temperature.unwrap_or(default.temperature),
                gamma: c.gamma.unwrap_or(default.gamma),
                brightness: c.brightness.unwrap_or(default.brightness),
                inverted: c.inverted.unwrap_or(default.inverted),
            })
        }
        fn parse_time(str: &str) -> TimeProviderMode {
            match str {
                "auto" => TimeProviderMode::Auto,
                _ => TimeProviderMode::Fixed(NaiveTime::parse_from_str(str, "%H:%M").unwrap()),
            }
        }

        let light_time = parse_time(&self.switch_mode.night);
        let dark_time = parse_time(&self.switch_mode.day);

        if light_time == TimeProviderMode::Auto || dark_time == TimeProviderMode::Auto {}

        Config {
            day: merge_color(self.day),
            night: merge_color(self.night),
            location: self.location,
            switch_mode: SwitchMode {
                day: light_time,
                night: dark_time,
            },
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    mod parse {
        use super::*;

        #[test]
        fn minimal() {
            let file = "
                 [switch-mode]
                 day = \"auto\"
                 night = \"auto\"
            ";
            let config = RawConfig::new(file).parse();
            assert_eq!(config.day, DisplayColor::default());
            assert_eq!(config.night, DisplayColor::default());
            assert!(config.location.is_none());
            assert_eq!(config.switch_mode.day, TimeProviderMode::Auto);
            assert_eq!(config.switch_mode.night, TimeProviderMode::Auto);
        }

        mod mix_time_provider {
            use super::*;

            #[test]
            fn fixed_auto() {
                let file = "
                     [switch-mode]
                     day = \"19:30\"
                     night = \"auto\"
                ";
                let config = RawConfig::new(file).parse();
                assert_eq!(config.switch_mode.day, TimeProviderMode::Auto);
                assert_eq!(
                    config.switch_mode.night,
                    TimeProviderMode::Fixed(NaiveTime::from_hms_opt(19, 30, 0).unwrap())
                );
            }
        }
    }
}
