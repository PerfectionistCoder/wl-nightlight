use std::str::FromStr;

use ini::{Ini, Properties};
use thiserror::Error;

use crate::color::Color;

use super::{Animation, Config, Location};

#[derive(Debug, Clone, Copy, Error)]
#[cfg_attr(test, derive(PartialEq))]
pub enum Error {
    #[error("Section `{0}` is required")]
    MissingSection(&'static str),
    #[error("Key `{0}` is required")]
    MissingKey(&'static str),
    #[error("Value of key `{0}` is invalid")]
    Invalid(&'static str),
    #[error("Value of key `{0}` is out of range")]
    OutOfRange(&'static str),
}

pub type ErrorList = Vec<Error>;

trait Section {
    fn check(&self) -> ErrorList;
}

impl Section for Color {
    fn check(&self) -> ErrorList {
        get_err_vec(&[
            ("temperature", in_range(self.temperature, 1000, 10000)),
            ("brightness", in_range(self.brightness, 0.0, 1.0)),
        ])
    }
}

impl Section for Location {
    fn check(&self) -> ErrorList {
        get_err_vec(&[
            ("lat", in_range(self.lat, -90.0, 90.0)),
            ("lng", in_range(self.lng, -180.0, 180.0)),
        ])
    }
}

impl Section for Animation {
    fn check(&self) -> ErrorList {
        get_err_vec(&[("transition", in_range(self.transition, 0.0, 3600.0))])
    }
}

fn get_err_vec(arr: &[(&'static str, bool)]) -> ErrorList {
    arr.iter()
        .filter(|el| !el.1)
        .map(|el| Error::OutOfRange(el.0))
        .collect()
}

fn in_range<T: PartialOrd>(value: T, min: T, max: T) -> bool {
    min <= value && value <= max
}

pub fn parse_config(file: &Ini) -> Result<Config, ErrorList> {
    let mut errors = vec![];

    let mut location = Location::default();
    parse_section(file, "location", true, &mut errors, |section, errors| {
        let lat = parse_key(section, "lat", true, errors);
        let lng = parse_key(section, "lng", true, errors);
        if let Some(lat) = lat {
            if let Some(lng) = lng {
                location = Location { lat, lng };
                return Some(Box::new(location));
            }
        }
        None
    });

    let mut light = Color::default();
    parse_section(file, "light", false, &mut errors, |section, errors| {
        light_dark_closure(section, errors, &mut light)
    });

    let mut dark = light;
    parse_section(file, "dark", false, &mut errors, |section, errors| {
        light_dark_closure(section, errors, &mut dark)
    });

    let mut animation = Animation::default();
    parse_section(file, "animation", false, &mut errors, |section, errors| {
        let transition = parse_key(section, "transition", false, errors);

        if let Some(v) = transition {
            animation.transition = v;
        }

        Some(Box::new(animation))
    });

    if errors.is_empty() {
        Ok(Config {
            location,
            light,
            dark,
            animation,
        })
    } else {
        Err(errors)
    }
}

fn light_dark_closure(
    section: &Properties,
    errors: &mut Vec<Error>,
    color: &mut Color,
) -> Option<Box<dyn Section>> {
    let temperature = parse_key(section, "temperature", false, errors);
    let brightness = parse_key(section, "brightness", false, errors);

    if let Some(v) = temperature {
        color.temperature = v;
    }
    if let Some(v) = brightness {
        color.brightness = v;
    }
    Some(Box::new(*color))
}

fn parse_section(
    file: &Ini,
    name: &'static str,
    required: bool,
    errors: &mut ErrorList,
    closure: impl FnOnce(&Properties, &mut ErrorList) -> Option<Box<dyn Section>>,
) {
    match file.section(Some(name)) {
        Some(section) => {
            let section = closure(section, errors);
            if let Some(section) = section {
                errors.append(&mut section.check());
            }
        }
        None => {
            if required {
                errors.push(Error::MissingSection(name))
            };
        }
    }
}

fn parse_key<T: FromStr>(
    section: &Properties,
    name: &'static str,
    required: bool,
    errors: &mut ErrorList,
) -> Option<T> {
    match section.get(name) {
        Some(key) => match key.parse() {
            Ok(v) => Some(v),
            Err(_) => {
                errors.push(Error::Invalid(name));
                None
            }
        },
        None => {
            if required {
                errors.push(Error::MissingKey(name));
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{thread_rng, Rng};

    fn setup(
        write: impl FnOnce(&mut Ini),
        assert: impl FnOnce(Config),
        error: impl FnOnce(ErrorList),
    ) {
        let file_name =
            &("test-config/".to_owned() + &thread_rng().gen::<u16>().to_string() + ".ini");
        let mut conf = Ini::new();
        write(&mut conf);
        conf.write_to_file(file_name).unwrap();

        let file = Ini::load_from_file(file_name).unwrap();
        match parse_config(&file) {
            Ok(cfg) => assert(cfg),
            Err(err) => error(err),
        };
    }

    const DISCARD_WRITE: fn(&mut Ini) = |_| {};
    const DISCARD_ASSERT: fn(Config) = |_| {};
    const DISCARD_ERROR: fn(ErrorList) = |_| {};

    #[test]
    fn empty() {
        setup(DISCARD_WRITE, DISCARD_ASSERT, |err| {
            assert_eq!(err, vec![Error::MissingSection("location")])
        });
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
            |conf| {
                write_location(conf, Some(&location));
            },
            |cfg| {
                assert_eq!(cfg.location, location);
                assert_eq!(cfg.light, Color::default());
                assert_eq!(cfg.dark, Color::default());
                assert_eq!(cfg.animation, Animation::default());
            },
            DISCARD_ERROR,
        );
    }

    #[test]
    fn partial_location_1() {
        setup(
            |conf| {
                conf.with_section(Some("location")).set("lat", "0");
            },
            DISCARD_ASSERT,
            |err| {
                assert_eq!(err, vec![Error::MissingKey("lng")]);
            },
        );
    }

    #[test]
    fn partial_location_2() {
        setup(
            |conf| {
                conf.with_section(Some("location")).set("lng", "0");
            },
            DISCARD_ASSERT,
            |err| {
                assert_eq!(err, vec![Error::MissingKey("lat")]);
            },
        );
    }

    #[test]
    fn invalid_location() {
        setup(
            |conf| {
                conf.with_section(Some("location"))
                    .set("lat", "")
                    .set("lng", "");
            },
            DISCARD_ASSERT,
            |err| {
                assert_eq!(err, vec![Error::Invalid("lat"), Error::Invalid("lng")]);
            },
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
            |conf| {
                write_location(conf, None);
                write_mode(conf, "light", &color);
            },
            |cfg| {
                assert_eq!(cfg.light, color);
                assert_eq!(cfg.dark, color);
            },
            DISCARD_ERROR,
        );
    }

    #[test]
    fn partial_light() {
        let brightness = 0.5;

        setup(
            |conf| {
                write_location(conf, None);
                conf.with_section(Some("light"))
                    .set("brightness", brightness.to_string());
            },
            |cfg| {
                assert_eq!(cfg.light.brightness, brightness);
                assert_eq!(cfg.dark.temperature, Color::default().temperature);
            },
            DISCARD_ERROR,
        );
    }

    #[test]
    fn invalid_light() {
        setup(
            |conf| {
                write_location(conf, None);
                conf.with_section(Some("light"))
                    .set("temperature", "")
                    .set("brightness", "");
            },
            DISCARD_ASSERT,
            |err| {
                assert_eq!(
                    err,
                    vec![Error::Invalid("temperature"), Error::Invalid("brightness")]
                );
            },
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
            |conf| {
                write_location(conf, None);
                write_mode(conf, "light", &color1);
                write_mode(conf, "dark", &color2);
            },
            |cfg| {
                assert_eq!(cfg.light, color1);
                assert_eq!(cfg.dark, color2);
            },
            DISCARD_ERROR,
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
            |conf| {
                write_location(conf, None);
                write_mode(conf, "light", &color);
                conf.with_section(Some("dark"))
                    .set("temperature", temperature.to_string());
            },
            |cfg| {
                assert_eq!(cfg.dark.temperature, temperature);
                assert_eq!(cfg.dark.brightness, color.brightness);
            },
            DISCARD_ERROR,
        );
    }

    #[test]
    fn invalid_dark() {
        setup(
            |conf| {
                write_location(conf, None);
                conf.with_section(Some("dark"))
                    .set("temperature", "")
                    .set("brightness", "");
            },
            DISCARD_ASSERT,
            |err| {
                assert_eq!(
                    err,
                    vec![Error::Invalid("temperature"), Error::Invalid("brightness")]
                );
            },
        );
    }

    #[test]
    fn animation() {
        let transition = 10.0;

        setup(
            |conf| {
                write_location(conf, None);
                conf.with_section(Some("animation"))
                    .set("transition", transition.to_string());
            },
            |cfg| {
                assert_eq!(cfg.animation.transition, transition);
            },
            DISCARD_ERROR,
        );
    }

    #[test]
    fn invalid_animation() {
        setup(
            |conf| {
                write_location(conf, None);
                conf.with_section(Some("animation")).set("transition", "");
            },
            DISCARD_ASSERT,
            |err| {
                assert_eq!(err, vec![Error::Invalid("transition")]);
            },
        );
    }
}
