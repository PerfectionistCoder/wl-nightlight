use anyhow::Result;

use wayrs_client::cstr;
use wayrs_client::global::*;
use wayrs_client::protocol::*;
use wayrs_client::{Connection, EventCtx};
use wayrs_protocols::wlr_gamma_control_unstable_v1::*;

use crate::color::{colorramp_fill, Color};

use super::state::WaylandState;

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
    pub fn bind(
        conn: &mut Connection<WaylandState>,
        global: &Global,
        gamma_manager: ZwlrGammaControlManagerV1,
    ) -> Self {
        eprintln!("New output: {}", global.name);
        let output = global
            .bind_with_cb(conn, 4, |ctx: EventCtx<WaylandState, WlOutput>| {
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
            })
            .unwrap();
        Self {
            reg_name: global.name,
            wl: output,
            name: None,
            color: Color::default(),
            gamma_control: gamma_manager.get_gamma_control_with_cb(
                conn,
                output,
                |ctx: EventCtx<WaylandState, ZwlrGammaControlV1>| {
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
                },
            ),
            ramp_size: 0,
            color_changed: true,
        }
    }

    pub fn destroy(self, conn: &mut Connection<WaylandState>) {
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

    pub fn update_displayed_color(&mut self, conn: &mut Connection<WaylandState>) -> Result<()> {
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
