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

use super::output::WaylandOutput;

const MIN_TEMP: u16 = 50;
const MAX_TEMP: u16 = 100;
const MIN_BRIGHTNESS: f64 = 0.005;
const MAX_BRIGHTNESS: f64 = 0.01;

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
                    temp: 0,
                },
                |color, output| {
                    let output_color = output.lock().unwrap().color();
                    Color {
                        brightness: color.brightness + output_color.brightness,
                        temp: color.temp + output_color.temp,
                    }
                },
            );

            Color {
                temp: color.temp / self.outputs.len() as u16,
                brightness: color.brightness / self.outputs.len() as f64,
            }
        }
    }

    pub fn color_changed(&self) -> bool {
        self.outputs
            .iter()
            .any(|output| output.lock().unwrap().color_changed())
    }

    fn calculate_interval(
        new: f32,
        old: f32,
        min: f32,
        max: f32,
        duration: f32,
    ) -> (i32, f32, f32) {
        const MAX_INTERVAL_PER_SEC: f32 = 60_f32;

        let diff = new - old;
        let sign = if diff.is_sign_negative() { -1.0 } else { 1.0 };
        let step = (diff / duration).abs().min(max).max(min) * sign;
        let interval = (diff / step).min(duration * MAX_INTERVAL_PER_SEC).round();
        let step = diff / interval;
        let wait = duration / interval;

        (interval as i32, step, wait)
    }
    pub fn change_to_color(&self, target: Color, duration: f32) -> Vec<JoinHandle<()>> {
        let mut handles = vec![];
        for output in &self.outputs {
            let output = Arc::clone(output);
            handles.push(thread::spawn(move || {
                if duration > 0.0 {
                    let output1 = Arc::clone(&output);
                    let output2 = Arc::clone(&output);
                    let t1 = thread::spawn(move || {
                        let (interval, step, wait) = WaylandState::calculate_interval(
                            target.temp as f32,
                            output1.lock().unwrap().color().temp as f32,
                            MIN_TEMP as f32,
                            MAX_TEMP as f32,
                            duration,
                        );
                        for i in 0..interval {
                            sleep(Duration::from_secs_f32(wait));
                            if i < interval - 1 {
                                let c = output1.lock().unwrap().color();
                                output1.lock().unwrap().set_color(Color {
                                    temp: (c.temp as i16 + step as i16) as u16,
                                    ..c
                                });
                            }
                        }
                    });
                    let t2 = thread::spawn(move || {
                        let (interval, step, wait) = WaylandState::calculate_interval(
                            target.brightness as f32,
                            output2.lock().unwrap().color().brightness as f32,
                            MIN_BRIGHTNESS as f32,
                            MAX_BRIGHTNESS as f32,
                            duration,
                        );
                        for i in 0..interval {
                            sleep(Duration::from_secs_f32(wait));
                            if i < interval - 1 {
                                let c = output2.lock().unwrap().color();
                                output2.lock().unwrap().set_color(Color {
                                    brightness: (c.brightness + step as f64),
                                    ..c
                                });
                            }
                        }
                    });
                    t1.join().unwrap();
                    t2.join().unwrap();
                }
                output.lock().unwrap().set_color(target);
            }));
        }
        handles
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::wayland;
    use std::cmp::Ordering;

    impl PartialOrd for Color {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            if self.temp < other.temp && self.brightness < other.brightness {
                Some(Ordering::Less)
            } else if self.temp > other.temp && self.brightness > other.brightness {
                Some(Ordering::Greater)
            } else if self.temp == other.temp && self.brightness == other.brightness {
                Some(Ordering::Equal)
            } else {
                None
            }
        }
    }

    fn get_state() -> WaylandState {
        let (_, state) = wayland::WaylandClient::new().unwrap();
        state
    }

    mod test_default_gamma {
        use super::*;

        #[test]
        fn default_color() {
            assert!(get_state()
                .outputs()
                .iter()
                .all(|o| o.lock().unwrap().color() == Color::default()))
        }
    }

    mod test_instant_color_change {
        use super::*;

        fn state_helper(state: &WaylandState, target: Color) {
            state
                .change_to_color(target, 0_f32)
                .into_iter()
                .for_each(|h| h.join().unwrap());
        }

        #[test]
        fn color_decrement() {
            let state = get_state();
            let target = Color {
                temp: 4500,
                brightness: 0.5,
            };
            state_helper(&state, target);
            assert_eq!(state.color(), target);
        }

        #[test]
        fn color_increment() {
            let state = get_state();
            let target = Color {
                temp: 4500,
                brightness: 0.5,
            };
            state_helper(&state, target);
            state_helper(&state, Color::default());
            assert_eq!(state.color(), Color::default());
        }

        #[test]
        fn consecutive_call() {
            let state = get_state();
            let target1 = Color {
                temp: 7500,
                brightness: 0.5,
            };
            let target2 = Color {
                temp: 4500,
                brightness: 0.9,
            };
            let target3 = Color {
                temp: 5500,
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
                WaylandState::calculate_interval(1000.0, 0.0, 0.0, 1000.0, 10.0)
            )
        }

        #[test]
        fn max_cap() {
            assert_eq!(
                (10, 100.0, 0.1),
                WaylandState::calculate_interval(1000.0, 0.0, 0.0, 100.0, 1.0)
            )
        }

        #[test]
        fn min_cap() {
            assert_eq!(
                (2, 5.0, 5.0),
                WaylandState::calculate_interval(10.0, 0.0, 5.0, 100.0, 10.0)
            )
        }

        #[test]
        fn negative_cap() {
            assert_eq!(
                (10, -100.0, 0.1),
                WaylandState::calculate_interval(0.0, 1000.0, 0.0, 100.0, 1.0)
            )
        }
    }

    mod test_with_duration {
        use super::*;

        const TARGET: Color = Color {
            temp: 4500,
            brightness: 0.5,
        };

        struct Bound {
            min: Color,
            max: Color,
        }

        fn timeline(list: &[Option<Bound>]) {
            let state = get_state();
            let time = 1_f32;
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
            let min = Color {
                temp: (TARGET.temp + Color::default().temp) / 4,
                brightness: (TARGET.brightness + Color::default().brightness) / 4_f64,
            };
            let max = Color {
                temp: (TARGET.temp + Color::default().temp) / 4 * 3,
                brightness: (TARGET.brightness + Color::default().brightness) / 4_f64 * 3_f64,
            };
            timeline(&[None, Some(Bound { min, max }), None]);
        }

        #[test]
        fn check_quoter() {
            let mid = Color {
                temp: (TARGET.temp + Color::default().temp) / 2,
                brightness: (TARGET.brightness + Color::default().brightness) / 2_f64,
            };
            timeline(&[
                Some(Bound {
                    min: mid,
                    max: Color::default(),
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
