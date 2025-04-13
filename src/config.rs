use chrono::NaiveTime;
use serde::Deserialize;

use crate::color::Color;

#[derive(Deserialize, Debug)]
struct PartialColor {
    temperature: Option<u16>,
    gamma: Option<f64>,
    brightness: Option<f64>,
    inverted: Option<bool>,
}

#[derive(Deserialize, Debug)]
pub struct Location {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Deserialize, Debug)]
pub struct General {
    pub on: String,
    pub off: String,
}

#[derive(Deserialize, Debug)]
pub struct RawConfig {
    pub light: Option<PartialColor>,
    pub dark: Option<PartialColor>,
    pub location: Option<Location>,
    pub general: General,
}

#[derive(PartialEq, Eq, Debug)]
pub enum ConfigTimeMode {
    Auto,
    Fixed(NaiveTime),
}

#[derive(Debug)]
pub struct LightDarkTime {
    pub light_time: ConfigTimeMode,
    pub dark_time: ConfigTimeMode,
}

pub struct Config {
    pub light: Color,
    pub dark: Color,
    pub location: Option<Location>,
    pub light_dark_time: LightDarkTime,
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
        fn merge_color(color: Option<PartialColor>) -> Color {
            let default = Color::default();
            color.map_or(default, |c| Color {
                temperature: c.temperature.unwrap_or(default.temperature),
                gamma: c.gamma.unwrap_or(default.gamma),
                brightness: c.brightness.unwrap_or(default.brightness),
                inverted: c.inverted.unwrap_or(default.inverted),
            })
        }
        fn parse_time(str: &str) -> ConfigTimeMode {
            match str {
                "auto" => ConfigTimeMode::Auto,
                _ => ConfigTimeMode::Fixed(NaiveTime::parse_from_str(str, "%H:%M").unwrap()),
            }
        }

        let light_time = parse_time(&self.general.off);
        let dark_time = parse_time(&self.general.on);

        if light_time == ConfigTimeMode::Auto || dark_time == ConfigTimeMode::Auto {}

        Config {
            light: merge_color(self.light),
            dark: merge_color(self.dark),
            location: self.location,
            light_dark_time: LightDarkTime {
                light_time,
                dark_time,
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
                 [general]
                 on = \"auto\"
                 off = \"auto\"
            ";
            let config = RawConfig::new(file).parse();
            assert_eq!(config.light, Color::default());
            assert_eq!(config.dark, Color::default());
            assert!(config.location.is_none());
            assert_eq!(config.light_dark_time.light_time, ConfigTimeMode::Auto);
            assert_eq!(config.light_dark_time.dark_time, ConfigTimeMode::Auto);
        }

        mod nix_time_provider {
            use super::*;

            #[test]
            fn fixed_auto() {
                let file = "
                     [general]
                     on = \"19:30\"
                     off = \"auto\"
                ";
                let config = RawConfig::new(file).parse();
                assert_eq!(config.light_dark_time.light_time, ConfigTimeMode::Auto);
                assert_eq!(
                    config.light_dark_time.dark_time,
                    ConfigTimeMode::Fixed(NaiveTime::from_hms_opt(19, 30, 0).unwrap())
                );
            }
        }
    }
}
