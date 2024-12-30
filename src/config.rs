use std::{env, str::FromStr};

use anyhow::{bail, Error, Result};
use ini::{Ini, Properties};

use crate::color::Color;

pub type Transition = f32;
pub type Latitude = f64;
pub type Longitude = Latitude;

#[derive(Debug, PartialEq)]
struct Location {
    lat: Latitude,
    lng: Longitude,
}

#[derive(Debug)]
pub struct Config {
    light_mode: Color,
    dark_mode: Color,
    location: Location,
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
            Some(section) => read_light_dark(section, Color::default())?,
            None => Color::default(),
        };
        let dark_mode = match file.section(Some("dark")) {
            Some(section) => read_light_dark(section, light_mode)?,
            None => light_mode,
        };
        let location = match file.section(Some("location")) {
            Some(section) => {
                let lat = section.get("lat").ok_or(Error::msg(""))?.parse()?;
                let lng = section.get("lng").ok_or(Error::msg(""))?.parse()?;
                Location { lat, lng }
            }
            None => bail!(""),
        };
        Ok(Self {
            light_mode,
            dark_mode,
            location,
        })
    }

    pub fn light_mode(&self) -> Color {
        self.light_mode
    }

    pub fn dark_mode(&self) -> Color {
        self.dark_mode
    }
}

fn read_light_dark(section: &Properties, mut default: Color) -> Result<Color> {
    append_color(section, "temperature", |v| default.temperature = v)?;
    append_color(section, "brightness", |v| default.brightness = v)?;
    Ok(default)
}

fn append_color<T, F>(section: &Properties, name: &str, closure: T) -> Result<(), F::Err>
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

    fn get_file_name(name: &str) -> String {
        "test-cfg/test-".to_owned() + name + ".ini"
    }

    #[test]
    #[should_panic]
    fn empty() {
        let file_name = &get_file_name("empty");
        Ini::new().write_to_file(file_name).unwrap();

        let file = Ini::load_from_file(file_name).unwrap();
        Config::parse_ini(file).unwrap();
    }

    fn write_location(conf: &mut Ini, location: Option<&Location>) {
        let location = location.unwrap_or(&Location { lat: 0.0, lng: 0.0 });
        conf.with_section(Some("location"))
            .set("lat", location.lat.to_string())
            .set("lng", location.lng.to_string());
    }

    fn write_mode(conf: &mut Ini, section: &str, color: &Color) {
        conf.with_section(Some(section))
            .set("temperature", color.temperature.to_string())
            .set("brightness", color.brightness.to_string());
    }

    #[test]
    fn minimal() {
        let file_name = &get_file_name("minimal");
        let mut conf = Ini::new();
        write_location(&mut conf, None);
        conf.write_to_file(file_name).unwrap();

        let file = Ini::load_from_file(file_name).unwrap();
        Config::parse_ini(file).unwrap();
    }

    #[test]
    fn location_only() {
        let location = Location {
            lat: 51.51,
            lng: -0.12,
        };

        let file_name = &get_file_name("location");
        let mut conf = Ini::new();
        write_location(&mut conf, Some(&location));
        conf.write_to_file(file_name).unwrap();

        let file = Ini::load_from_file(file_name).unwrap();
        let cfg = Config::parse_ini(file).unwrap();
        assert_eq!(cfg.location, location);
        assert_eq!(cfg.light_mode, Color::default());
        assert_eq!(cfg.dark_mode, Color::default());
    }

    #[test]
    #[should_panic]
    fn partial_location() {
        let file_name = &get_file_name("location_1");
        let mut conf = Ini::new();
        conf.with_section(Some("location")).set("lat", "0");
        conf.write_to_file(file_name).unwrap();

        let file = Ini::load_from_file(file_name).unwrap();
        Config::parse_ini(file).unwrap();
    }

    #[test]
    #[should_panic]
    fn invalid_location() {
        let file_name = &get_file_name("location_2");
        let mut conf = Ini::new();
        conf.with_section(Some("location"))
            .set("lat", "hello")
            .set("lng", "world");
        conf.write_to_file(file_name).unwrap();

        let file = Ini::load_from_file(file_name).unwrap();
        Config::parse_ini(file).unwrap();
    }

    #[test]
    fn location_light_mode() {
        let color = Color {
            temperature: 1234,
            brightness: 0.5,
        };

        let file_name = &get_file_name("light");
        let mut conf = Ini::new();
        write_location(&mut conf, None);
        write_mode(&mut conf, "light", &color);
        conf.write_to_file(file_name).unwrap();

        let file = Ini::load_from_file(file_name).unwrap();
        let cfg = Config::parse_ini(file).unwrap();
        assert_eq!(cfg.light_mode, color);
        assert_eq!(cfg.dark_mode, color);
    }

    #[test]
    fn partial_light() {
        let brightness = 0.8;

        let file_name = &get_file_name("light_1");
        let mut conf = Ini::new();
        write_location(&mut conf, None);
        conf.with_section(Some("light"))
            .set("brightness", brightness.to_string());
        conf.write_to_file(file_name).unwrap();

        let file = Ini::load_from_file(file_name).unwrap();
        let cfg = Config::parse_ini(file).unwrap();
        assert_eq!(cfg.light_mode.brightness, brightness);
        assert_eq!(cfg.dark_mode.temperature, Color::default().temperature);
    }

    #[test]
    #[should_panic]
    fn invalid_light() {
        let file_name = &get_file_name("light_2");
        let mut conf = Ini::new();
        write_location(&mut conf, None);
        conf.with_section(Some("light")).set("temperature", "hi");
        conf.write_to_file(file_name).unwrap();

        let file = Ini::load_from_file(file_name).unwrap();
        Config::parse_ini(file).unwrap();
    }

    #[test]
    fn location_light_dark_mode() {
        let color1 = Color {
            temperature: 1234,
            brightness: 0.5,
        };
        let color2 = Color {
            temperature: 6789,
            brightness: 1.0,
        };

        let file_name = &get_file_name("light_dark");
        let mut conf = Ini::new();
        write_location(&mut conf, None);
        write_mode(&mut conf, "light", &color1);
        write_mode(&mut conf, "dark", &color2);
        conf.write_to_file(file_name).unwrap();

        let file = Ini::load_from_file(file_name).unwrap();
        let cfg = Config::parse_ini(file).unwrap();
        assert_eq!(cfg.light_mode, color1);
        assert_eq!(cfg.dark_mode, color2);
    }

    #[test]
    fn partial_dark() {
        let color = Color {
            temperature: 1234,
            brightness: 0.5,
        };
        let temperature = 4567;

        let file_name = &get_file_name("dark_1");
        let mut conf = Ini::new();
        write_location(&mut conf, None);
        write_mode(&mut conf, "light", &color);
        conf.with_section(Some("dark"))
            .set("temperature", temperature.to_string());
        conf.write_to_file(file_name).unwrap();

        let file = Ini::load_from_file(file_name).unwrap();
        let cfg = Config::parse_ini(file).unwrap();
        assert_eq!(cfg.dark_mode.temperature, temperature);
        assert_eq!(cfg.dark_mode.brightness, color.brightness);
    }

    #[test]
    #[should_panic]
    fn invalid_dark() {
        let file_name = &get_file_name("dark_2");
        let mut conf = Ini::new();
        write_location(&mut conf, None);
        conf.with_section(Some("dark")).set("brightness", "hi");
        conf.write_to_file(file_name).unwrap();

        let file = Ini::load_from_file(file_name).unwrap();
        Config::parse_ini(file).unwrap();
    }
}
