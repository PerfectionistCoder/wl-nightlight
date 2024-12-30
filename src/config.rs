use crate::color::Color;

pub type Transition = f32;
pub type Latitude = f64;
pub type Longitude = Latitude;

pub struct Config {
    light_mode: Color,
    dark_mode: Color,
    lat: Latitude,
    lng: Longitude,
}

impl Config {
    pub fn new() -> Self {
        Self {
            light_mode: Color {
                temperature: 6500,
                brightness: 1.0,
            },
            dark_mode: Color {
                temperature: 5500,
                brightness: 0.8,
            },
            lat: 51.51,
            lng: -0.12,
        }
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
