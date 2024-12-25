use std::io::ErrorKind;
use std::os::fd::{AsRawFd, RawFd};

use anyhow::{bail, Result};

use wayrs_client::cstr;
use wayrs_client::global::*;
use wayrs_client::protocol::*;
use wayrs_client::{Connection, EventCtx, IoMode};
use wayrs_protocols::wlr_gamma_control_unstable_v1::*;

use crate::color::{colorramp_fill, Color};

pub struct Wayland {
    conn: Connection<WaylandState>,
}

impl AsRawFd for Wayland {
    fn as_raw_fd(&self) -> RawFd {
        self.conn.as_raw_fd()
    }
}

impl Wayland {
    pub fn new() -> Result<(Self, WaylandState)> {
        let (mut conn, globals) = Connection::connect_and_collect_globals()?;
        conn.add_registry_cb(wl_registry_cb);

        let Ok(gamma_manager) = globals.bind(&mut conn, 1) else {
            bail!("Your Wayland compositor is not supported because it does not implement the wlr-gamma-control-unstable-v1 protocol");
        };

        let outputs = globals
            .iter()
            .filter(|g| g.is::<WlOutput>())
            .map(|output| Output::bind(&mut conn, output, gamma_manager))
            .collect();

        let state = WaylandState {
            outputs,
            gamma_manager,
        };

        conn.flush(IoMode::Blocking)?;

        Ok((Self { conn }, state))
    }

    pub fn poll(&mut self, state: &mut WaylandState) -> Result<()> {
        match self.conn.recv_events(IoMode::NonBlocking) {
            Ok(()) => self.conn.dispatch_events(state),
            Err(e) if e.kind() == ErrorKind::WouldBlock => (),
            Err(e) => return Err(e.into()),
        }

        for output in &mut state.outputs {
            if output.color_changed {
                output.update_displayed_color(&mut self.conn)?;
            }
        }
        self.conn.flush(IoMode::Blocking)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct Output {
    reg_name: u32,
    wl: WlOutput,
    name: Option<String>,
    color: Color,
    gamma_control: ZwlrGammaControlV1,
    ramp_size: usize,
    color_changed: bool,
}

impl Output {
    fn bind(
        conn: &mut Connection<WaylandState>,
        global: &Global,
        gamma_manager: ZwlrGammaControlManagerV1,
    ) -> Self {
        eprintln!("New output: {}", global.name);
        let output = global.bind_with_cb(conn, 4, wl_output_cb).unwrap();
        Self {
            reg_name: global.name,
            wl: output,
            name: None,
            color: Color::default(),
            gamma_control: gamma_manager.get_gamma_control_with_cb(conn, output, gamma_control_cb),
            ramp_size: 0,
            color_changed: true,
        }
    }

    fn destroy(self, conn: &mut Connection<WaylandState>) {
        eprintln!("Output {} removed", self.reg_name);
        self.gamma_control.destroy(conn);
        self.wl.release(conn);
    }

    pub fn reg_name(&self) -> u32 {
        self.reg_name
    }

    pub fn color(&self) -> Color {
        self.color
    }

    pub fn color_changed(&self) -> bool {
        self.color_changed
    }

    pub fn set_color(&mut self, color: Color) {
        if color != self.color {
            self.color = color;
            self.color_changed = true;
        }
    }

    pub fn object_path(&self) -> Option<String> {
        self.name
            .as_deref()
            .map(|name| format!("/outputs/{}", name.replace('-', "_")))
    }

    fn update_displayed_color(&mut self, conn: &mut Connection<WaylandState>) -> Result<()> {
        if self.ramp_size == 0 {
            return Ok(());
        }

        let file = shmemfdrs2::create_shmem(cstr!("/ramp-buffer"))?;
        file.set_len(self.ramp_size as u64 * 6)?;
        let mut mmap = unsafe { memmap2::MmapMut::map_mut(&file)? };
        let buf = bytemuck::cast_slice_mut::<u8, u16>(&mut mmap);
        let (r, rest) = buf.split_at_mut(self.ramp_size);
        let (g, b) = rest.split_at_mut(self.ramp_size);
        colorramp_fill(r, g, b, self.ramp_size, self.color);
        self.gamma_control.set_gamma(conn, file.into());

        self.color_changed = false;
        Ok(())
    }
}

fn wl_registry_cb(
    conn: &mut Connection<WaylandState>,
    state: &mut WaylandState,
    event: &wl_registry::Event,
) {
    match event {
        wl_registry::Event::Global(global) if global.is::<WlOutput>() => {
            let mut output = Output::bind(conn, global, state.gamma_manager);
            output.set_color(state.color());
            output.update_displayed_color(conn).unwrap();
            state.outputs.push(output);
        }
        wl_registry::Event::GlobalRemove(name) => {
            if let Some(output_index) = state.outputs.iter().position(|o| o.reg_name == *name) {
                let output = state.outputs.swap_remove(output_index);
                output.destroy(conn);
            }
        }
        _ => (),
    }
}

fn gamma_control_cb(ctx: EventCtx<WaylandState, ZwlrGammaControlV1>) {
    let output_index = ctx
        .state
        .outputs
        .iter()
        .position(|o| o.gamma_control == ctx.proxy)
        .expect("Received event for unknown output");
    match ctx.event {
        zwlr_gamma_control_v1::Event::GammaSize(size) => {
            let output = &mut ctx.state.outputs[output_index];
            eprintln!("Output {}: ramp_size = {}", output.reg_name, size);
            output.ramp_size = size as usize;
            output.update_displayed_color(ctx.conn).unwrap();
        }
        zwlr_gamma_control_v1::Event::Failed => {
            let output = ctx.state.outputs.swap_remove(output_index);
            eprintln!("Output {}: gamma_control::Event::Failed", output.reg_name);
            output.destroy(ctx.conn);
        }
        _ => (),
    }
}

fn wl_output_cb(ctx: EventCtx<WaylandState, WlOutput>) {
    if let wl_output::Event::Name(name) = ctx.event {
        let output = ctx
            .state
            .outputs
            .iter_mut()
            .find(|o| o.wl == ctx.proxy)
            .unwrap();
        let name = String::from_utf8(name.into_bytes()).expect("invalid output name");
        eprintln!("Output {}: name = {name:?}", output.reg_name);
        output.name = Some(name);
    }
}

pub struct WaylandState {
    outputs: Vec<Output>,
    gamma_manager: ZwlrGammaControlManagerV1,
}

impl WaylandState {
    pub fn output_by_reg_name(&self, reg_name: u32) -> Option<&Output> {
        self.outputs
            .iter()
            .find(|output| output.reg_name() == reg_name)
    }

    pub fn mut_output_by_reg_name(&mut self, reg_name: u32) -> Option<&mut Output> {
        self.outputs
            .iter_mut()
            .find(|output| output.reg_name() == reg_name)
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
}
