use std::{
    fmt::{self, Display, Formatter},
    str::FromStr,
    vec,
};

use ini::{Ini, Properties};
use thiserror::Error;

use crate::color::Color;

use super::{first_err, Animation, Config, Location};

#[derive(Debug, Clone, PartialEq, Error)]
pub enum ErrorDetail {
    #[error("")]
    MissingSection,
    #[error("Key '{0}' is required")]
    MissingKey(&'static str),
    #[error("Value of key '{0}' is invalid")]
    Invalid(&'static str),
    #[error("Value of key '{0}' is out of range")]
    OutOfRange(&'static str),
}

#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Error {
    name: &'static str,
    detail: Vec<ErrorDetail>,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.detail.len() == 1 && self.detail[0] == ErrorDetail::MissingSection {
            writeln!(f, "Section '{}' is required", self.name)
        } else {
            writeln!(f, "In section [{}]:", self.name)?;
            first_err(&self.detail, |err| writeln!(f, " - {}", err))
        }
    }
}

pub type ErrorList = Vec<Error>;
type ErrorDetailList = Vec<ErrorDetail>;

trait Section {
    fn check(&self) -> ErrorDetailList;
}

impl Section for Color {
    fn check(&self) -> ErrorDetailList {
        to_error_list(&[
            ("temperature", in_range(self.temperature, 1000, 10000)),
            ("brightness", in_range(self.brightness, 0.0, 1.0)),
        ])
    }
}

impl Section for Location {
    fn check(&self) -> ErrorDetailList {
        to_error_list(&[
            ("lat", in_range(self.lat, -90.0, 90.0)),
            ("lng", in_range(self.lng, -180.0, 180.0)),
        ])
    }
}

impl Section for Animation {
    fn check(&self) -> ErrorDetailList {
        to_error_list(&[("transition", in_range(self.transition, 0.0, 3600.0))])
    }
}

fn to_error_list(array: &[(&'static str, bool)]) -> ErrorDetailList {
    array
        .iter()
        .filter(|elem| !elem.1)
        .map(|elem| ErrorDetail::OutOfRange(elem.0))
        .collect()
}

fn in_range<T: PartialOrd>(value: T, min: T, max: T) -> bool {
    min <= value && value <= max
}

pub fn parse_config(file: &Ini) -> Result<Config, ErrorList> {
    let mut errors = vec![];

    let mut location = Location::default();
    parse_section(file, "location", true, &mut errors, |section, detail| {
        let lat = parse_key(section, "lat", true, detail);
        let lng = parse_key(section, "lng", true, detail);
        if let Some(lat) = lat {
            if let Some(lng) = lng {
                location = Location { lat, lng };
                return Some(Box::new(location));
            }
        }
        None
    });

    let mut light = Color::default();
    parse_section(file, "light", false, &mut errors, |section, detail| {
        light_dark(section, detail, &mut light)
    });

    let mut dark = light;
    parse_section(file, "dark", false, &mut errors, |section, detail| {
        light_dark(section, detail, &mut dark)
    });

    let mut animation = Animation::default();
    parse_section(file, "animation", false, &mut errors, |section, detail| {
        let transition = parse_key(section, "transition", false, detail);
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

fn light_dark(
    section: &Properties,
    detail: &mut ErrorDetailList,
    color: &mut Color,
) -> Option<Box<dyn Section>> {
    let temperature = parse_key(section, "temperature", false, detail);
    let brightness = parse_key(section, "brightness", false, detail);
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
    op: impl FnOnce(&Properties, &mut ErrorDetailList) -> Option<Box<dyn Section>>,
) {
    match file.section(Some(name)) {
        Some(section) => {
            let mut error = Error {
                name,
                detail: vec![],
            };
            let section = op(section, &mut error.detail);
            if let Some(section) = section {
                error.detail.append(&mut section.check());
            }
            if !error.detail.is_empty() {
                errors.push(error);
            }
        }
        None => {
            if required {
                errors.push(Error {
                    name,
                    detail: vec![ErrorDetail::MissingSection],
                })
            };
        }
    }
}

fn parse_key<T: FromStr>(
    section: &Properties,
    name: &'static str,
    required: bool,
    detail: &mut ErrorDetailList,
) -> Option<T> {
    match section.get(name) {
        Some(key) => match key.parse() {
            Ok(v) => Some(v),
            Err(_) => {
                detail.push(ErrorDetail::Invalid(name));
                None
            }
        },
        None => {
            if required {
                detail.push(ErrorDetail::MissingKey(name));
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

    mod section {
        use super::*;

        #[test]
        fn empty() {
            setup(DISCARD_WRITE, DISCARD_ASSERT, |err| {
                assert_eq!(
                    err,
                    vec![Error {
                        name: "location",
                        detail: vec![ErrorDetail::MissingSection]
                    }]
                )
            });
        }

        #[test]
        fn location() {
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
    }

    mod partial {
        use super::*;

        #[test]
        fn location_1() {
            setup(
                |conf| {
                    conf.with_section(Some("location")).set("lat", "0");
                },
                DISCARD_ASSERT,
                |err| {
                    assert_eq!(
                        err,
                        vec![Error {
                            name: "location",
                            detail: vec![ErrorDetail::MissingKey("lng")]
                        }]
                    );
                },
            );
        }

        #[test]
        fn location_2() {
            setup(
                |conf| {
                    conf.with_section(Some("location")).set("lng", "0");
                },
                DISCARD_ASSERT,
                |err| {
                    assert_eq!(
                        err,
                        vec![Error {
                            name: "location",
                            detail: vec![ErrorDetail::MissingKey("lat")]
                        }]
                    );
                },
            );
        }

        #[test]
        fn light() {
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
        fn dark() {
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
    }

    mod invalid {
        use super::*;

        #[test]
        fn location() {
            setup(
                |conf| {
                    conf.with_section(Some("location"))
                        .set("lat", "")
                        .set("lng", "");
                },
                DISCARD_ASSERT,
                |err| {
                    assert_eq!(
                        err,
                        vec![Error {
                            name: "location",
                            detail: vec![ErrorDetail::Invalid("lat"), ErrorDetail::Invalid("lng")]
                        },]
                    );
                },
            );
        }

        #[test]
        fn light() {
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
                        vec![Error {
                            name: "light",
                            detail: vec![
                                ErrorDetail::Invalid("temperature"),
                                ErrorDetail::Invalid("brightness")
                            ]
                        },]
                    );
                },
            );
        }

        #[test]
        fn dark() {
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
                        vec![Error {
                            name: "dark",
                            detail: vec![
                                ErrorDetail::Invalid("temperature"),
                                ErrorDetail::Invalid("brightness")
                            ]
                        },]
                    );
                },
            );
        }

        #[test]
        fn animation() {
            setup(
                |conf| {
                    write_location(conf, None);
                    conf.with_section(Some("animation")).set("transition", "");
                },
                DISCARD_ASSERT,
                |err| {
                    assert_eq!(
                        err,
                        vec![Error {
                            name: "animation",
                            detail: vec![ErrorDetail::Invalid("transition")]
                        }]
                    );
                },
            );
        }
    }
}
