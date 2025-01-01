use std::env;

use ini::Ini;
use parser::parse_config;

use crate::color::Color;

mod parser;

pub type Latitude = f32;
pub type Longitude = Latitude;

#[derive(Debug, PartialEq, Clone, Copy, Default)]
pub struct Location {
    pub lat: Latitude,
    pub lng: Longitude,
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

#[derive(Debug)]
pub enum Error {
    File,
    Parse(parser::ErrorList),
}

#[derive(Debug)]
pub struct Config {
    location: Location,
    light: Color,
    dark: Color,
    animation: Animation,
}

impl Config {
    pub fn location(&self) -> Location {
        self.location
    }

    pub fn animation(&self) -> Animation {
        self.animation
    }

    pub fn new(path: Option<String>) -> Result<Self, Error> {
        let file_path = path.unwrap_or(
            env::var("XDG_CONFIG_HOME")
                .unwrap_or(env::var("HOME").map_err(|_| Error::File)? + "/.config")
                + "wl-nightlight/config.ini",
        );
        let file = Ini::load_from_file(file_path).map_err(|_| Error::File)?;
        parse_config(&file).map_err(|err| Error::Parse(err))
    }
}
