use std::{
    sync::{Arc, Mutex},
    thread::{self, sleep, JoinHandle},
    time::Duration,
};

use anyhow::{bail, Result};
use wayrs_client::{
    global::*,
    protocol::{wl_registry::GlobalArgs, WlOutput},
    Connection,
};
use wayrs_protocols::wlr_gamma_control_unstable_v1::*;

use crate::color::Color;

use super::output::{OutputSetColor, WaylandOutput};

struct Bound<T> {
    min: T,
    max: T,
}

pub struct WaylandState {
    outputs: Vec<Arc<Mutex<WaylandOutput>>>,
    gamma_manager: ZwlrGammaControlManagerV1,
}

impl WaylandState {
    pub fn new(conn: &mut Connection<Self>, globals: Vec<GlobalArgs>) -> Result<Self> {
        let Ok(gamma_manager) = globals.bind(conn, 1) else {
            bail!("Your Wayland compositor is not supported because it does not implement the wlr-gamma-control-unstable-v1 protocol");
        };

        let outputs = globals
            .iter()
            .filter(|g| g.is::<WlOutput>())
            .map(|output| WaylandOutput::bind(conn, output, gamma_manager))
            .collect();

        Ok(Self {
            outputs,
            gamma_manager,
        })
    }

    pub fn outputs(&mut self) -> &mut Vec<Arc<Mutex<WaylandOutput>>> {
        &mut self.outputs
    }

    pub fn gamma_manager(&self) -> ZwlrGammaControlManagerV1 {
        self.gamma_manager
    }

    /// Returns the average color of all outputs, or the default color if there are no outputs
    pub fn color(&self) -> Color {
        if self.outputs.is_empty() {
            Color::default()
        } else {
            let color = self.outputs.iter().fold(
                Color {
                    brightness: 0.0,
                    temperature: 0,
                },
                |color, output| {
                    let output_color = output.lock().unwrap().color();
                    Color {
                        brightness: color.brightness + output_color.brightness,
                        temperature: color.temperature + output_color.temperature,
                    }
                },
            );

            Color {
                temperature: color.temperature / self.outputs.len() as u16,
                brightness: color.brightness / self.outputs.len() as f64,
            }
        }
    }

    pub fn color_changed(&self) -> bool {
        self.outputs
            .iter()
            .any(|output| output.lock().unwrap().color_changed())
    }

    pub fn change_to_color(&self, target: Color, duration: f64) -> Vec<JoinHandle<()>> {
        struct ColorBound {
            temperature: Bound<f64>,
            brightness: Bound<f64>,
        }

        const COLOR_BOUND: ColorBound = ColorBound {
            temperature: Bound {
                min: 50.0,
                max: 100.0,
            },
            brightness: Bound {
                min: 0.005,
                max: 0.01,
            },
        };

        struct Arg {
            property: fn(&Color) -> f64,
            bound: Bound<f64>,
            callback: OutputSetColor,
        }

        const ARGS: [Arg; 2] = [
            Arg {
                property: |c| c.temperature as f64,
                bound: COLOR_BOUND.temperature,
                callback: WaylandOutput::set_temperature,
            },
            Arg {
                property: |c| c.brightness,
                bound: COLOR_BOUND.brightness,
                callback: WaylandOutput::set_brightness,
            },
        ];

        let mut handles = vec![];
        for output in &self.outputs {
            let output = Arc::clone(output);
            handles.push(thread::spawn(move || {
                if duration > 0.0 {
                    let mut handles = vec![];
                    for arg in ARGS {
                        let output = Arc::clone(&output);
                        let color = output.lock().unwrap().color();
                        handles.push(thread::spawn(move || {
                            color_transit(
                                output,
                                (arg.property)(&target),
                                (arg.property)(&color),
                                arg.bound,
                                duration,
                                arg.callback,
                            );
                        }));
                    }
                    handles.into_iter().for_each(|h| h.join().unwrap());
                }
                output.lock().unwrap().set_color(target);
            }));
        }
        handles
    }
}

fn calculate_interval(new: f64, old: f64, bound: Bound<f64>, duration: f64) -> (i32, f64, f64) {
    let diff = new - old;
    let sign = if diff.is_sign_negative() { -1.0 } else { 1.0 };
    let step = (diff / duration).abs().min(bound.max).max(bound.min) * sign;
    let interval = (diff / step).round();
    let step = diff / interval;
    let wait = duration / interval;

    (interval as i32, step, wait)
}

fn color_transit(
    output: Arc<Mutex<WaylandOutput>>,
    new: f64,
    old: f64,
    bound: Bound<f64>,
    duration: f64,
    callback: OutputSetColor,
) {
    let (interval, step, wait) = calculate_interval(new, old, bound, duration);
    for i in 0..interval {
        sleep(Duration::from_secs_f64(wait));
        if i < interval - 1 {
            callback(&mut output.lock().unwrap(), step);
        }
    }
}

#[cfg(test)]
mod test {
    use super::super::WaylandClient;
    use super::*;
    use std::cmp::Ordering;

    impl PartialOrd for Color {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            if self.temperature < other.temperature && self.brightness < other.brightness {
                Some(Ordering::Less)
            } else if self.temperature > other.temperature && self.brightness > other.brightness {
                Some(Ordering::Greater)
            } else if self.temperature == other.temperature && self.brightness == other.brightness {
                Some(Ordering::Equal)
            } else {
                None
            }
        }
    }

    fn get_state() -> WaylandState {
        let (_, state) = WaylandClient::new().unwrap();
        state
    }

    const TARGET: Color = Color {
        temperature: 4500,
        brightness: 0.5,
    };

    const ORIGINAL: Color = Color {
        temperature: 6500,
        brightness: 1.0,
    };

    mod test_instant_color_change {
        use super::*;

        fn state_helper(state: &WaylandState, target: Color) {
            state
                .change_to_color(target, 0_f64)
                .into_iter()
                .for_each(|h| h.join().unwrap());
        }

        #[test]
        fn color_decrement() {
            let state = get_state();
            state_helper(&state, ORIGINAL);
            state_helper(&state, TARGET);
            assert_eq!(state.color(), TARGET);
        }

        #[test]
        fn color_increment() {
            let state = get_state();
            state_helper(&state, TARGET);
            state_helper(&state, ORIGINAL);
            assert_eq!(state.color(), ORIGINAL);
        }

        #[test]
        fn consecutive_call() {
            let state = get_state();
            let target1 = Color {
                temperature: 7500,
                brightness: 0.5,
            };
            let target2 = Color {
                temperature: 5500,
                brightness: 0.9,
            };
            let target3 = Color {
                temperature: 8500,
                brightness: 0.7,
            };

            state_helper(&state, target1);
            state_helper(&state, target2);
            state_helper(&state, target3);

            assert_eq!(state.color(), target3);
        }
    }

    mod test_calculate_interval {
        use super::*;

        #[test]
        fn normal() {
            assert_eq!(
                (10, 100.0, 1.0),
                calculate_interval(
                    1000.0,
                    0.0,
                    Bound {
                        min: 0.0,
                        max: 1000.0
                    },
                    10.0
                )
            )
        }

        #[test]
        fn max_cap() {
            assert_eq!(
                (10, 100.0, 0.1),
                calculate_interval(
                    1000.0,
                    0.0,
                    Bound {
                        min: 0.0,
                        max: 100.0
                    },
                    1.0
                )
            )
        }

        #[test]
        fn min_cap() {
            assert_eq!(
                (2, 5.0, 5.0),
                calculate_interval(
                    10.0,
                    0.0,
                    Bound {
                        min: 5.0,
                        max: 100.0
                    },
                    10.0
                )
            )
        }

        #[test]
        fn negative_cap() {
            assert_eq!(
                (10, -100.0, 0.1),
                calculate_interval(
                    0.0,
                    1000.0,
                    Bound {
                        min: 0.0,
                        max: 100.0
                    },
                    1.0
                )
            )
        }
    }

    mod test_with_duration {
        use super::*;

        fn timeline(list: &[Option<Bound<Color>>]) {
            let state = get_state();
            let time = 1_f64;
            let handles = state.change_to_color(TARGET, time);
            let len = list.len() + 1;
            for b in list.iter() {
                sleep(Duration::from_secs_f64(time / len as f64));
                if let Some(b) = b {
                    assert!(state.color() < b.max);
                    assert!(state.color() > b.min);
                }
            }
            handles.into_iter().for_each(|h| h.join().unwrap());
            assert_eq!(state.color(), TARGET);
        }

        #[test]
        fn check_mid() {
            let temperature_diff = ORIGINAL.temperature - TARGET.temperature;
            let brightness_diff = ORIGINAL.brightness - TARGET.brightness;
            let min = Color {
                temperature: TARGET.temperature + temperature_diff / 4,
                brightness: TARGET.brightness + brightness_diff / 4_f64,
            };
            let max = Color {
                temperature: TARGET.temperature + temperature_diff / 4 * 3,
                brightness: TARGET.brightness + brightness_diff / 4_f64 * 3_f64,
            };
            timeline(&[None, Some(Bound { min, max }), None]);
        }

        #[test]
        fn check_quoter() {
            let mid = Color {
                temperature: (TARGET.temperature + ORIGINAL.temperature) / 2,
                brightness: (TARGET.brightness + ORIGINAL.brightness) / 2_f64,
            };
            timeline(&[
                Some(Bound {
                    min: mid,
                    max: ORIGINAL,
                }),
                None,
                Some(Bound {
                    min: TARGET,
                    max: mid,
                }),
            ]);
        }
    }
}
