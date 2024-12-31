use std::{env, str::FromStr};

use anyhow::{bail, Error, Result};
use ini::{Ini, Properties};

use crate::color::Color;

pub trait Check {
    fn check(&self) -> Result<()>;

    fn in_range<T: PartialOrd>(value: T, min: T, max: T) -> Result<()> {
        if min <= value && value <= max {
            Ok(())
        } else {
            bail!("");
        }
    }
}

pub type Latitude = f64;
pub type Longitude = Latitude;

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Location {
    pub lat: Latitude,
    pub lng: Longitude,
}

impl Check for Location {
    fn check(&self) -> Result<()> {
        Self::in_range(self.lat, -90.0, 90.0)?;
        Self::in_range(self.lng, -180.0, 180.0)?;
        Ok(())
    }
}

pub type Transition = f32;

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Animation {
    pub transition: Transition,
}

impl Default for Animation {
    fn default() -> Self {
        Self { transition: 3.0 }
    }
}

impl Check for Animation {
    fn check(&self) -> Result<()> {
        Self::in_range(self.transition, 0.0, 3600.0)?;
        Ok(())
    }
}

impl Check for Color {
    fn check(&self) -> Result<()> {
        Self::in_range(self.temperature, 1000, 10000)?;
        Self::in_range(self.brightness, 0.0, 1.0)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct Config {
    light_mode: Color,
    dark_mode: Color,
    location: Location,
    animation: Animation,
}

impl Config {
    pub fn new(path: Option<String>) -> Result<Self> {
        let file_path = path.unwrap_or(
            env::var("XDG_CONFIG_HOME").unwrap_or(env::var("HOME")? + "/.config")
                + "wl-nightlight/config.ini",
        );
        let file = Ini::load_from_file(file_path)?;
        Config::parse_ini(file)
    }

    fn parse_ini(file: Ini) -> Result<Self> {
        let light_mode = match file.section(Some("light")) {
            Some(section) => parse_light_dark(section, Color::default())?,
            None => Color::default(),
        };
        light_mode.check()?;
        let dark_mode = match file.section(Some("dark")) {
            Some(section) => parse_light_dark(section, light_mode)?,
            None => light_mode,
        };
        dark_mode.check()?;

        let location = match file.section(Some("location")) {
            Some(section) => {
                let lat = section.get("lat").ok_or(Error::msg(""))?.parse()?;
                let lng = section.get("lng").ok_or(Error::msg(""))?.parse()?;
                Location { lat, lng }
            }
            None => bail!(""),
        };
        location.check()?;

        let animation = match file.section(Some("animation")) {
            Some(section) => {
                let mut default = Animation::default();
                try_parse(section, "transition", |v| default.transition = v)?;
                default
            }
            None => Animation::default(),
        };
        animation.check()?;

        Ok(Self {
            light_mode,
            dark_mode,
            location,
            animation,
        })
    }

    pub fn light_mode(&self) -> Color {
        self.light_mode
    }

    pub fn dark_mode(&self) -> Color {
        self.dark_mode
    }

    pub fn location(&self) -> Location {
        self.location
    }

    pub fn animation(&self) -> Animation {
        self.animation
    }
}

fn parse_light_dark(section: &Properties, mut default: Color) -> Result<Color> {
    try_parse(section, "temperature", |v| default.temperature = v)?;
    try_parse(section, "brightness", |v| default.brightness = v)?;
    Ok(default)
}

fn try_parse<T, F>(section: &Properties, name: &str, closure: T) -> Result<(), F::Err>
where
    T: FnOnce(F),
    F: FromStr,
{
    if let Some(key) = section.get(name) {
        let key = key.parse()?;
        closure(key);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(name: &str, write: impl FnOnce(&mut Ini), assert: impl FnOnce(Config)) {
        let file_name = &("test-cfg/test-".to_owned() + name + ".ini");
        let mut conf = Ini::new();
        write(&mut conf);
        conf.write_to_file(file_name).unwrap();

        let file = Ini::load_from_file(file_name).unwrap();
        let cfg = Config::parse_ini(file).unwrap();
        assert(cfg);
    }

    const DISCARD_WRITE: fn(&mut Ini) = |_| {};
    const DISCARD_ASSERT: fn(Config) = |_| {};

    #[test]
    #[should_panic]
    fn empty() {
        setup("empty", DISCARD_WRITE, DISCARD_ASSERT);
    }

    fn write_location(conf: &mut Ini, location: Option<&Location>) {
        let location = location.unwrap_or(&Location { lat: 0.0, lng: 0.0 });
        conf.with_section(Some("location"))
            .set("lat", location.lat.to_string())
            .set("lng", location.lng.to_string());
    }

    #[test]
    fn minimal() {
        let location = Location {
            lat: 51.51,
            lng: -0.12,
        };

        setup(
            "minimal",
            |conf| {
                write_location(conf, Some(&location));
            },
            |cfg| {
                assert_eq!(cfg.location, location);
                assert_eq!(cfg.light_mode, Color::default());
                assert_eq!(cfg.dark_mode, Color::default());
                assert_eq!(cfg.animation, Animation::default());
            },
        );
    }

    #[test]
    #[should_panic]
    fn partial_location_1() {
        setup(
            "location_1",
            |conf| {
                conf.with_section(Some("location")).set("lat", "90");
            },
            DISCARD_ASSERT,
        );
    }

    #[test]
    #[should_panic]
    fn partial_location_2() {
        setup(
            "location_2",
            |conf| {
                conf.with_section(Some("location")).set("lng", "-180");
            },
            DISCARD_ASSERT,
        );
    }

    #[test]
    #[should_panic]
    fn invalid_location_1() {
        setup(
            "location_3",
            |conf| {
                conf.with_section(Some("location")).set("lat", "invalid");
            },
            DISCARD_ASSERT,
        );
    }

    #[test]
    #[should_panic]
    fn invalid_location_2() {
        setup(
            "location_4",
            |conf| {
                conf.with_section(Some("location")).set("lng", "invalid");
            },
            DISCARD_ASSERT,
        );
    }

    #[test]
    #[should_panic]
    fn location_out_of_range_1() {
        setup(
            "location_5",
            |conf| {
                write_location(
                    conf,
                    Some(&Location {
                        lat: -91.0,
                        lng: 0.0,
                    }),
                );
            },
            DISCARD_ASSERT,
        );
    }

    #[test]
    #[should_panic]
    fn location_out_of_range_2() {
        setup(
            "location_6",
            |conf| {
                write_location(
                    conf,
                    Some(&Location {
                        lat: 91.0,
                        lng: 0.0,
                    }),
                );
            },
            DISCARD_ASSERT,
        );
    }

    #[test]
    #[should_panic]
    fn location_out_of_range_3() {
        setup(
            "location_7",
            |conf| {
                write_location(
                    conf,
                    Some(&Location {
                        lat: 0.0,
                        lng: -181.0,
                    }),
                );
            },
            DISCARD_ASSERT,
        );
    }

    #[test]
    #[should_panic]
    fn location_out_of_range_4() {
        setup(
            "location_8",
            |conf| {
                write_location(
                    conf,
                    Some(&Location {
                        lat: 0.0,
                        lng: 181.0,
                    }),
                );
            },
            DISCARD_ASSERT,
        );
    }

    fn write_mode(conf: &mut Ini, section: &str, color: &Color) {
        conf.with_section(Some(section))
            .set("temperature", color.temperature.to_string())
            .set("brightness", color.brightness.to_string());
    }

    #[test]
    fn light() {
        let color = Color {
            temperature: 5000,
            brightness: 0.5,
        };

        setup(
            "light_1",
            |conf| {
                write_location(conf, None);
                write_mode(conf, "light", &color);
            },
            |cfg| {
                assert_eq!(cfg.light_mode, color);
                assert_eq!(cfg.dark_mode, color);
            },
        );
    }

    #[test]
    fn partial_light() {
        let brightness = 0.5;

        setup(
            "light_2",
            |conf| {
                write_location(conf, None);
                conf.with_section(Some("light"))
                    .set("brightness", brightness.to_string());
            },
            |cfg| {
                assert_eq!(cfg.light_mode.brightness, brightness);
                assert_eq!(cfg.dark_mode.temperature, Color::default().temperature);
            },
        );
    }

    #[test]
    #[should_panic]
    fn invalid_light() {
        setup(
            "light_3",
            |conf| {
                conf.with_section(Some("light"))
                    .set("temperature", "invalid");
            },
            DISCARD_ASSERT,
        );
    }

    #[test]
    #[should_panic]
    fn light_out_of_range_1() {
        setup(
            "light_4",
            |conf| {
                conf.with_section(Some("light")).set("temperature", "999");
            },
            DISCARD_ASSERT,
        );
    }

    #[test]
    #[should_panic]
    fn light_out_of_range_2() {
        setup(
            "light_5",
            |conf| {
                conf.with_section(Some("light")).set("temperature", "10001");
            },
            DISCARD_ASSERT,
        );
    }

    #[test]
    fn dark() {
        let color1 = Color {
            temperature: 1000,
            brightness: 0.0,
        };
        let color2 = Color {
            temperature: 10000,
            brightness: 1.0,
        };

        setup(
            "dark_1",
            |conf| {
                write_location(conf, None);
                write_mode(conf, "light", &color1);
                write_mode(conf, "dark", &color2);
            },
            |cfg| {
                assert_eq!(cfg.light_mode, color1);
                assert_eq!(cfg.dark_mode, color2);
            },
        );
    }

    #[test]
    fn partial_dark() {
        let color = Color {
            temperature: 5000,
            brightness: 0.5,
        };
        let temperature = 4000;

        setup(
            "dark_2",
            |conf| {
                write_location(conf, None);
                write_mode(conf, "light", &color);
                conf.with_section(Some("dark"))
                    .set("temperature", temperature.to_string());
            },
            |cfg| {
                assert_eq!(cfg.dark_mode.temperature, temperature);
                assert_eq!(cfg.dark_mode.brightness, color.brightness);
            },
        );
    }

    #[test]
    #[should_panic]
    fn invalid_dark() {
        setup(
            "dark_3",
            |conf| {
                write_location(conf, None);
                conf.with_section(Some("dark")).set("brightness", "invalid");
            },
            DISCARD_ASSERT,
        );
    }

    #[test]
    fn animation() {
        let transition = 10.0;

        setup(
            "animation_1",
            |conf| {
                write_location(conf, None);
                conf.with_section(Some("animation"))
                    .set("transition", transition.to_string());
            },
            |cfg| {
                assert_eq!(cfg.animation.transition, transition);
            },
        );
    }

    #[test]
    #[should_panic]
    fn invalid_animation() {
        setup(
            "animation_2",
            |conf| {
                write_location(conf, None);
                conf.with_section(Some("animation"))
                    .set("transition", "invalid");
            },
            DISCARD_ASSERT,
        );
    }

    #[test]
    #[should_panic]
    fn animation_out_of_range() {
        setup(
            "animation_3",
            |conf| {
                write_location(conf, None);
                conf.with_section(Some("animation")).set("transition", "-1");
            },
            DISCARD_ASSERT,
        );
    }
}
