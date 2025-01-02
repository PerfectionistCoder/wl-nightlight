use std::{
    sync::{Arc, Mutex},
    thread::{self, sleep, JoinHandle},
    time::Duration,
};

use anyhow::{bail, Result};
use getset::{CopyGetters, MutGetters};
use wayrs_client::{
    global::*,
    protocol::{wl_registry::GlobalArgs, WlOutput},
    Connection,
};
use wayrs_protocols::wlr_gamma_control_unstable_v1::*;

use crate::{
    color::{Brightness, Color, Temperature},
    config::Transition,
};

use super::output::WaylandOutput;

struct Bound<T> {
    min: T,
    max: T,
}

#[derive(MutGetters, CopyGetters)]
pub struct WaylandState {
    #[getset(get_mut = "pub")]
    outputs: Vec<Arc<Mutex<WaylandOutput>>>,
    #[getset(get_copy = "pub")]
    gamma_manager: ZwlrGammaControlManagerV1,
}

impl WaylandState {
    pub fn bind(conn: &mut Connection<Self>, globals: Vec<GlobalArgs>) -> Result<Self> {
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
                temperature: color.temperature / self.outputs.len() as Temperature,
                brightness: color.brightness / self.outputs.len() as Brightness,
            }
        }
    }

    pub fn color_changed(&self) -> bool {
        self.outputs
            .iter()
            .any(|output| output.lock().unwrap().color_changed())
    }

    pub fn change_to_color(&self, target: Color, transition: Transition) -> Vec<JoinHandle<()>> {
        struct ColorBound {
            temperature: Bound<Precision>,
            brightness: Bound<Precision>,
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
            property: fn(&Color) -> Precision,
            bound: Bound<Precision>,
            op: OutputSetColor,
        }

        const ARGS: [Arg; 2] = [
            Arg {
                property: |c| c.temperature as Precision,
                bound: COLOR_BOUND.temperature,
                op: |output, step| {
                    let color = output.color();
                    output.set_color(Color {
                        temperature: color.temperature + step as i16,
                        ..color
                    });
                },
            },
            Arg {
                property: |c| c.brightness as Precision,
                bound: COLOR_BOUND.brightness,
                op: |output, step| {
                    let color = output.color();
                    output.set_color(Color {
                        brightness: color.brightness + step,
                        ..color
                    });
                },
            },
        ];

        let mut handles = vec![];
        for output in &self.outputs {
            if output.lock().unwrap().color() == target {
                continue;
            }
            let output = Arc::clone(output);
            handles.push(thread::spawn(move || {
                if transition > 0.0 {
                    let mut handles = vec![];
                    for arg in ARGS {
                        let output = Arc::clone(&output);
                        let color = output.lock().unwrap().color();
                        handles.push(thread::spawn(move || {
                            color_change_animation(
                                output,
                                (arg.property)(&target),
                                (arg.property)(&color),
                                arg.bound,
                                transition,
                                arg.op,
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

type Precision = f32;

fn calculate_interval(
    new: Precision,
    old: Precision,
    bound: Bound<Precision>,
    transition: Transition,
) -> (i32, Precision, Precision) {
    let diff = new - old;
    let sign = if diff.is_sign_negative() { -1.0 } else { 1.0 };
    let step = (diff / transition).abs().min(bound.max).max(bound.min) * sign;
    let interval = (diff / step).round();
    let step = diff / interval;
    let wait = transition / interval;

    (interval as i32, step, wait)
}

pub type OutputSetColor = fn(&mut WaylandOutput, Precision);

fn color_change_animation(
    output: Arc<Mutex<WaylandOutput>>,
    new: Precision,
    old: Precision,
    bound: Bound<Precision>,
    transition: Transition,
    op: OutputSetColor,
) {
    let (interval, step, wait) = calculate_interval(new, old, bound, transition);
    for i in 0..interval {
        sleep(Duration::from_secs_f32(wait));
        if i < interval - 1 {
            op(&mut output.lock().unwrap(), step);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::WaylandClient;
    use super::*;

    const TARGET: Color = Color {
        temperature: 4500,
        brightness: 0.5,
    };

    const ORIGINAL: Color = Color {
        temperature: 6500,
        brightness: 1.0,
    };

    fn state_helper(state: &WaylandState, target: Color) {
        state
            .change_to_color(target, 0.0)
            .into_iter()
            .for_each(|h| h.join().unwrap());
    }

    fn get_state() -> WaylandState {
        let (_, state) = WaylandClient::create().unwrap();
        state_helper(&state, ORIGINAL);
        state
    }

    mod instant {
        use super::*;

        #[test]
        fn color_decrement() {
            let state = get_state();
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

    mod calculate_interval {
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

    mod transition {
        use super::*;
        use std::cmp::Ordering;

        impl PartialOrd for Color {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                if self.temperature < other.temperature && self.brightness < other.brightness {
                    Some(Ordering::Less)
                } else if self.temperature > other.temperature && self.brightness > other.brightness
                {
                    Some(Ordering::Greater)
                } else if self.temperature == other.temperature
                    && self.brightness == other.brightness
                {
                    Some(Ordering::Equal)
                } else {
                    None
                }
            }
        }

        fn get_fraction_of(color1: Color, color2: Color, fraction: f64) -> Color {
            let bigger: &Color;
            let smaller: &Color;

            if color1 > color2 {
                bigger = &color1;
                smaller = &color2;
            } else {
                bigger = &color2;
                smaller = &color1
            }

            let t_diff = bigger.temperature - smaller.temperature;
            let b_diff = bigger.brightness - smaller.brightness;

            Color {
                temperature: smaller.temperature + (t_diff as f64 * fraction) as i16,
                brightness: smaller.brightness + (b_diff as f64 * fraction) as f32,
            }
        }

        fn timeline(list: &[Option<Bound<Color>>]) {
            let state = get_state();
            let time = 1.0;
            let handles = state.change_to_color(TARGET, time);
            let len = list.len() + 1;
            for b in list.iter() {
                sleep(Duration::from_secs_f32(time / len as f32));
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
            let min = get_fraction_of(TARGET, ORIGINAL, 1.0 / 4.0);
            let max = get_fraction_of(TARGET, ORIGINAL, 3.0 / 4.0);
            timeline(&[None, None, Some(Bound { min, max }), None, None]);
        }

        #[test]
        fn check_quoter() {
            let mid = get_fraction_of(TARGET, ORIGINAL, 1.0 / 2.0);
            timeline(&[
                None,
                None,
                Some(Bound {
                    min: mid,
                    max: ORIGINAL,
                }),
                None,
                None,
                None,
                Some(Bound {
                    min: TARGET,
                    max: mid,
                }),
                None,
                None,
            ]);
        }
    }
}
