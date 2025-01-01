use std::{env, fmt};

use getset::CopyGetters;
use ini::Ini;
use parser::{parse_config, ErrorList};
use thiserror::Error;

use crate::color::Color;

mod parser;

pub type Latitude = f32;
pub type Longitude = Latitude;

#[derive(Clone, Copy, Default, CopyGetters)]
#[cfg_attr(test, derive(Debug, PartialEq))]
#[getset(get_copy = "pub")]
pub struct Location {
    lat: Latitude,
    lng: Longitude,
}

pub type Transition = f32;

#[derive(Clone, Copy, CopyGetters)]
#[cfg_attr(test, derive(Debug, PartialEq))]
#[getset(get_copy = "pub")]
pub struct Animation {
    transition: Transition,
}

impl Default for Animation {
    fn default() -> Self {
        Self { transition: 3.0 }
    }
}

#[derive(Debug)]
pub struct ParseErrorList(ErrorList);

impl fmt::Display for ParseErrorList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().for_each(|err| {
            let _ = writeln!(f, "{}", err);
        });
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Fail to locate config file")]
    File,
    #[error("Fail to load config file:\n{}", .0)]
    Config(ParseErrorList),
}

#[derive(CopyGetters)]
#[cfg_attr(test, derive(Debug))]
#[getset(get_copy = "pub")]
pub struct Config {
    location: Location,
    light: Color,
    dark: Color,
    animation: Animation,
}

impl Config {
    pub fn new(path: Option<String>) -> Result<Self, Error> {
        let file_path = path.unwrap_or(
            env::var("XDG_CONFIG_HOME")
                .unwrap_or(env::var("HOME").map_err(|_| Error::File)? + "/.config")
                + "wl-nightlight/config.ini",
        );
        let file = Ini::load_from_file(file_path).map_err(|_| Error::File)?;
        parse_config(&file).map_err(|err| Error::Config(ParseErrorList(err)))
    }
}
