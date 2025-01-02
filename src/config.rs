use std::{
    env,
    fmt::{self, Display, Formatter},
};

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
pub struct ConfigErrorList(ErrorList);

fn vec_write<T>(vec: &[T], op: impl FnMut(&T) -> fmt::Result) -> fmt::Result {
    *vec.iter()
        .map(op)
        .filter(|res| res.is_err())
        .collect::<Vec<_>>()
        .first()
        .unwrap_or(&Ok(()))
}

impl Display for ConfigErrorList {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        vec_write(&self.0, |err| write!(f, "{}", err))
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("cannot construct config file path: $XDG_CONFIG_HOME and $HOME are not set")]
    Path,
    #[error("cannot access '{0}': {1}")]
    File(String, ini::Error),
    #[error("cannot load config file:\n{}", .0)]
    Config(ConfigErrorList),
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
    pub fn load(path: Option<String>) -> Result<Self, Error> {
        let file_path = path.unwrap_or(
            env::var("XDG_CONFIG_HOME")
                .unwrap_or(env::var("HOME").map_err(|_| Error::Path)? + "/.config")
                + "/wl-nightlight/config.ini",
        );
        let file =
            Ini::load_from_file(&file_path).map_err(move |err| Error::File(file_path, err))?;
        parse_config(&file).map_err(|err| Error::Config(ConfigErrorList(err)))
    }
}
