use std::{env, str::FromStr};

use anyhow::{bail, Error, Result};
use ini::{Ini, Properties};

use crate::color::Color;

pub type Latitude = f64;
pub type Longitude = Latitude;

#[derive(Debug)]
pub struct Config {
    light_mode: Color,
    dark_mode: Color,
    lat: Latitude,
    lng: Longitude,
}

impl Config {
    pub fn new(path: Option<String>) -> Result<Self> {
        let file_path = env::var("XDG_CONFIG_HOME").unwrap_or(String::from("~/.config"))
            + "wl-nightlight/config.ini";
        let file = Ini::load_from_file(file_path)?;
        Config::parse_ini(file)
    }

    fn parse_ini(file: Ini) -> Result<Self> {
        let light_mode = match file.section(Some("light")) {
            Some(section) => read_light_dark(section)?,
            None => Color::default(),
        };
        let dark_mode = match file.section(Some("dark")) {
            Some(section) => read_light_dark(section)?,
            None => light_mode,
        };
        let (lat, lng) = match file.section(Some("location")) {
            Some(section) => {
                let lat = section.get("lat").ok_or(Error::msg(""))?.parse()?;
                let lng = section.get("lng").ok_or(Error::msg(""))?.parse()?;
                (lat, lng)
            }
            None => bail!(""),
        };
        Ok(Self {
            light_mode,
            dark_mode,
            lat,
            lng,
        })
    }

    pub fn lat(&self) -> Latitude {
        self.lat
    }

    pub fn lng(&self) -> Longitude {
        self.lng
    }

    pub fn light_mode(&self) -> Color {
        self.light_mode
    }

    pub fn dark_mode(&self) -> Color {
        self.dark_mode
    }
}

fn read_light_dark(section: &Properties) -> Result<Color> {
    let mut color = Color::default();
    append_color(section, "temperature", |v| color.temperature = v)?;
    append_color(section, "brightness", |v| color.brightness = v)?;
    Ok(color)
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

    const FILE_NAME:&str = "test.ini";

    fn write() {
        let mut conf = Ini::new();
        conf.with_section(Some(""));
        conf.write_to_file(FILE_NAME).unwrap();
    }

    #[test]
    fn test_full_config() {
        let file = Ini::load_from_file(FILE_NAME).unwrap();
        let cfg = Config::parse_ini(file).unwrap();
        println!("{cfg:?}");
    }
}
