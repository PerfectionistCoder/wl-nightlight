use std::io::ErrorKind;
use std::os::fd::{AsRawFd, RawFd};

use anyhow::{bail, Result};

use wayrs_client::global::*;
use wayrs_client::protocol::*;
use wayrs_client::{Connection, IoMode};

use super::{output::WaylandOutput, state::WaylandState};

pub struct WaylandClient {
    conn: Connection<WaylandState>,
}

impl AsRawFd for WaylandClient {
    fn as_raw_fd(&self) -> RawFd {
        self.conn.as_raw_fd()
    }
}

impl WaylandClient {
    pub fn new() -> Result<(Self, WaylandState)> {
        let (mut conn, globals) = Connection::connect_and_collect_globals()?;
        conn.add_registry_cb(
            |conn: &mut Connection<WaylandState>,
             state: &mut WaylandState,
             event: &wl_registry::Event| {
                match event {
                    wl_registry::Event::Global(global) if global.is::<WlOutput>() => {
                        let mut output = WaylandOutput::bind(conn, global, state.gamma_manager);
                        output.set_color(state.color());
                        output.update_displayed_color(conn).unwrap();
                        state.outputs.push(output);
                    }
                    wl_registry::Event::GlobalRemove(name) => {
                        if let Some(output_index) =
                            state.outputs.iter().position(|o| o.reg_name() == *name)
                        {
                            let output = state.outputs.swap_remove(output_index);
                            output.destroy(conn);
                        }
                    }
                    _ => (),
                }
            },
        );

        let Ok(gamma_manager) = globals.bind(&mut conn, 1) else {
            bail!("Your Wayland compositor is not supported because it does not implement the wlr-gamma-control-unstable-v1 protocol");
        };

        let outputs = globals
            .iter()
            .filter(|g| g.is::<WlOutput>())
            .map(|output| WaylandOutput::bind(&mut conn, output, gamma_manager))
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
            if output.color_changed() {
                output.update_displayed_color(&mut self.conn)?;
            }
        }
        self.conn.flush(IoMode::Blocking)?;
        Ok(())
    }
}
