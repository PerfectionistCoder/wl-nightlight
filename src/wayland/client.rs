use std::io::ErrorKind;
use std::os::fd::{AsRawFd, RawFd};
use std::sync::Arc;

use anyhow::Result;

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
    pub fn create() -> Result<(Self, WaylandState)> {
        let (mut conn, globals) = Connection::connect_and_collect_globals()?;
        conn.add_registry_cb(wl_register_cb);

        let state = WaylandState::bind(&mut conn, globals)?;

        conn.flush(IoMode::Blocking)?;

        Ok((Self { conn }, state))
    }

    pub fn poll(&mut self, state: &mut WaylandState) -> Result<()> {
        match self.conn.recv_events(IoMode::NonBlocking) {
            Ok(()) => self.conn.dispatch_events(state),
            Err(e) if e.kind() == ErrorKind::WouldBlock => (),
            Err(e) => return Err(e.into()),
        }

        for output in state.outputs_mut() {
            if output.lock().unwrap().color_changed() {
                output
                    .lock()
                    .unwrap()
                    .update_displayed_color(&mut self.conn)?;
            }
        }
        self.conn.flush(IoMode::Blocking)?;
        Ok(())
    }
}

fn wl_register_cb(
    conn: &mut Connection<WaylandState>,
    state: &mut WaylandState,
    event: &wl_registry::Event,
) {
    match event {
        wl_registry::Event::Global(global) if global.is::<WlOutput>() => {
            let output = WaylandOutput::bind(conn, global, state.gamma_manager());
            output.lock().unwrap().set_color(state.color());
            output.lock().unwrap().update_displayed_color(conn).unwrap();
            state.outputs_mut().push(output);
        }
        wl_registry::Event::GlobalRemove(name) => {
            if let Some(output_index) = state
                .outputs_mut()
                .iter()
                .position(|output| output.lock().unwrap().reg_name() == *name)
            {
                let output = state.outputs_mut().swap_remove(output_index);
                Arc::try_unwrap(output)
                    .unwrap()
                    .into_inner()
                    .unwrap()
                    .destroy(conn);
            }
        }
        _ => (),
    }
}
