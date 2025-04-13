use std::fs::read_to_string;

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
    pub on: Option<String>,
    pub off: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct ConfigRaw {
    pub light: Option<PartialColor>,
    pub dark: Option<PartialColor>,
    pub location: Option<Location>,
    pub general: Option<General>,
}

impl ConfigRaw {
    pub fn new() -> Self {
        let file = read_to_string("extra/example.toml").unwrap();
        toml::from_str(&file).unwrap()
    }
}

#[derive(Debug)]
pub struct LightDarkTime {
    pub light_time: Option<NaiveTime>,
    pub dark_time: Option<NaiveTime>,
}

pub struct Config {
    pub light: Color,
    pub dark: Color,
    pub location: Option<Location>,
    pub light_dark_time: LightDarkTime,
}
