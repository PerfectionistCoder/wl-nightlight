use std::{
    sync::{Arc, Mutex},
    thread::{self, sleep},
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

use super::output::{self, WaylandOutput};

const MAX_INTERVAL: u16 = 5;
const MIN_TEMP: u16 = 100;
const MIN_BRIGHTNESS: f64 = 0.05;

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

    pub fn change_to_color(&self, target: Color) {
        for output in &self.outputs {
            let output = Arc::clone(output);
            thread::spawn(move || {
                output.lock().unwrap().set_color(target);
            });
        }
    }
}

// #[cfg(test)]
// mod test {
//     use super::*;
//     use crate::wayland;

//     mod test_color_change {
//         use super::*;

//         fn get_state() -> WaylandState {
//             let (_, state) = wayland::WaylandClient::new().unwrap();
//             state
//         }

//         #[test]
//         fn color_set_to_target() {
//             let mut state = get_state();
//             let target = Color {
//                 temp: 4500,
//                 brightness: 0.5,
//             };
//             state.change_to_color(target);
//             assert_eq!(state.color(), target);
//         }

//         #[test]
//         fn consecutive_call() {
//             let mut state = get_state();
//             let target1 = Color {
//                 temp: 5400,
//                 brightness: 1.0,
//             };
//             let target2 = Color {
//                 temp: 4500,
//                 brightness: 0.5,
//             };
//             state.change_to_color(target1);
//             state.change_to_color(target2);
//             assert_eq!(state.color(), target2);
//         }
//     }
// }
