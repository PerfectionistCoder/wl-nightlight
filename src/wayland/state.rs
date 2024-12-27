use std::thread;

use anyhow::{bail, Result};
use wayrs_client::{
    global::*,
    protocol::{wl_registry::GlobalArgs, WlOutput},
    Connection,
};
use wayrs_protocols::wlr_gamma_control_unstable_v1::*;

use crate::color::Color;

use super::output::WaylandOutput;

pub struct WaylandState {
    outputs: Vec<WaylandOutput>,
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

    pub fn output_by_reg_name(&self, reg_name: u32) -> Option<&WaylandOutput> {
        self.outputs
            .iter()
            .find(|output| output.reg_name() == reg_name)
    }

    pub fn mut_output_by_reg_name(&mut self, reg_name: u32) -> Option<&mut WaylandOutput> {
        self.outputs
            .iter_mut()
            .find(|output| output.reg_name() == reg_name)
    }

    pub fn outputs(&mut self) -> &mut Vec<WaylandOutput> {
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
                    let output_color = output.color();
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
        self.outputs.iter().any(|output| output.color_changed())
    }

    pub fn set_brightness(&mut self, brightness: f64) {
        for output in &mut self.outputs {
            let color = output.color();
            output.set_color(Color {
                brightness,
                ..color
            });
        }
    }

    /// Returns `true` if any output was updated
    pub fn update_brightness(&mut self, delta: f64) -> bool {
        let mut updated = false;
        for output in &mut self.outputs {
            let color = output.color();
            let brightness = (color.brightness + delta).clamp(0.0, 1.0);
            if brightness != color.brightness {
                updated = true;
                output.set_color(Color {
                    brightness,
                    ..color
                });
            }
        }

        updated
    }

    pub fn set_temperature(&mut self, temp: u16) {
        for output in &mut self.outputs {
            let color = output.color();
            output.set_color(Color { temp, ..color });
        }
    }

    /// Returns `true` if any output was updated
    pub fn update_temperature(&mut self, delta: i16) -> bool {
        let mut updated = false;
        for output in &mut self.outputs {
            if let Some(new_color) = output.color().with_updated_temp(delta) {
                updated = true;
                output.set_color(new_color);
            }
        }

        updated
    }

    pub fn change_to_color(mut self, color2: Color, duration: u16) -> Self {
        const MAX_INTERVAL: u16 = 5;
        let mut handles = vec![];
        for mut output in self.outputs {
            handles.push(thread::spawn(move || {
                output.set_color(color2);
                output
            }));
        }
        self.outputs = handles.into_iter().map(|h| h.join().unwrap()).collect();
        self
    }
}

#[cfg(test)]
trait TestTraitWaylandState {}

#[cfg(test)]
struct TestWaylandState {
    outputs: Vec<WaylandOutput>,
}

#[cfg(test)]
impl TestTraitWaylandState for TestWaylandState {}

#[cfg(test)]
impl TestTraitWaylandState for WaylandState {}

#[cfg(test)]
mod tests {
    mod test_resulted_gamma {}
}
