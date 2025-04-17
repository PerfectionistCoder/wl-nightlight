#[cfg(test)]
use serial_test::serial;

use std::{
    os::fd::AsFd,
    sync::mpsc::{Receiver, Sender},
};

use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle,
    protocol::{
        wl_output::{self, WlOutput},
        wl_registry,
    },
};
use wayland_protocols_wlr::gamma_control::v1::client::{
    zwlr_gamma_control_manager_v1::ZwlrGammaControlManagerV1,
    zwlr_gamma_control_v1::{self, ZwlrGammaControlV1},
};

use crate::{
    InternalError,
    color::{Color, fill_color_ramp},
};

pub enum WaylandRequest {
    ChangeOutputColor(Color),
}

pub struct Wayland {
    connection: Connection,
    state: WaylandState,
    sender: Sender<anyhow::Result<()>>,
    receiver: Receiver<WaylandRequest>,
}

impl Wayland {
    pub fn new(
        sender: Sender<anyhow::Result<()>>,
        receiver: Receiver<WaylandRequest>,
    ) -> anyhow::Result<Self> {
        let connection = Connection::connect_to_env()?;

        let display = connection.display();

        let mut event_queue = connection.new_event_queue();
        let qh = event_queue.handle();

        let mut state = WaylandState::new();
        display.get_registry(&qh, ());
        event_queue.roundtrip(&mut state)?;

        if state.gamma_manager.is_none() {
            anyhow::bail!(
                "Your Wayland compositor is not supported because it does not implement the wlr-gamma-control-unstable-v1 protocol"
            )
        }

        if let Some(gamma_manager) = &state.gamma_manager {
            for output in &mut state.outputs {
                let wl_output = &output.wl_output;
                output.gamma_control = Some(gamma_manager.get_gamma_control(wl_output, &qh, ()));
            }
        }
        event_queue.roundtrip(&mut state)?;

        if state.outputs.is_empty() {
            anyhow::bail!("No output found")
        }

        Ok(Self {
            connection,
            state,
            sender,
            receiver,
        })
    }

    pub fn process_requests(&mut self) {
        let result = (|| -> anyhow::Result<()> {
            while let Ok(request) = self.receiver.recv() {
                self.connection
                    .new_event_queue()
                    .roundtrip(&mut self.state)?;

                match request {
                    WaylandRequest::ChangeOutputColor(color) => {
                        for output in self.state.outputs.iter_mut() {
                            output.set_color(color)?;
                        }
                    }
                }

                self.connection.flush()?;
                self.sender
                    .send(Ok(()))
                    .expect("Main thread receiver dropped");
            }

            Ok(())
        })();

        self.sender
            .send(result)
            .expect("Main thread receiver dropped");
    }
}

#[cfg_attr(test, derive(Debug))]
struct WaylandState {
    outputs: Vec<DisplayOutput>,
    gamma_manager: Option<ZwlrGammaControlManagerV1>,
}

impl WaylandState {
    fn new() -> Self {
        Self {
            gamma_manager: None,
            outputs: Vec::new(),
        }
    }
}

#[cfg_attr(test, derive(Debug))]
struct DisplayOutput {
    registry_name: u32,
    wl_output: WlOutput,
    output_name: Option<String>,
    gamma_control: Option<ZwlrGammaControlV1>,
    gamma_size: usize,
    color: Color,
}

impl DisplayOutput {
    fn new(registry_name: u32, wl_output: WlOutput) -> Self {
        Self {
            registry_name,
            wl_output,
            output_name: None,
            gamma_control: None,
            gamma_size: 0,
            color: Color::default(),
        }
    }

    fn destroy(&self) {
        log::debug!("Destroy output `{}`", self.registry_name);
        if let Some(gamma_control) = &self.gamma_control {
            gamma_control.destroy();
        };
        self.wl_output.release();
    }

    fn update_gamma(&mut self) -> anyhow::Result<()> {
        if self.gamma_size == 0 {
            log::warn!(
                "Skip updating gamma of output `{}` as the gamma size is 0",
                self.registry_name
            );
            return Ok(());
        }

        let file = shmemfdrs2::create_shmem(c"/ramp-buffer")?;
        file.set_len(self.gamma_size as u64 * 6)?;
        let mut mmap = unsafe { memmap2::MmapMut::map_mut(&file)? };
        let buf = bytemuck::cast_slice_mut::<u8, u16>(&mut mmap);
        let (r, rest) = buf.split_at_mut(self.gamma_size);
        let (g, b) = rest.split_at_mut(self.gamma_size);
        fill_color_ramp(r, g, b, self.gamma_size, self.color);
        self.gamma_control
            .as_ref()
            .ok_or(InternalError {
                message: "No gamma control for output",
            })?
            .set_gamma(file.as_fd());

        Ok(())
    }

    fn set_color(&mut self, color: Color) -> anyhow::Result<()> {
        if self.color != color {
            self.color = color;
            self.update_gamma()?;
        }

        Ok(())
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for WaylandState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _conn: &Connection,
        qh: &QueueHandle<WaylandState>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => {
                if interface == WlOutput::interface().name {
                    let wl_output = registry.bind::<WlOutput, _, _>(name, version, qh, ());
                    state.outputs.push(DisplayOutput::new(name, wl_output));
                    log::debug!("Bind output `{}`", name);
                } else if interface == ZwlrGammaControlManagerV1::interface().name {
                    state.gamma_manager = Some(registry.bind::<ZwlrGammaControlManagerV1, _, _>(
                        name,
                        version,
                        qh,
                        (),
                    ));
                    log::debug!("Bind gamma control manager");
                }
            }
            wl_registry::Event::GlobalRemove { name } => {
                if let Some(index) = state.outputs.iter().position(|o| o.registry_name == name) {
                    let output = state.outputs.swap_remove(index);
                    output.destroy();
                }
            }
            _ => (),
        }
    }
}

impl Dispatch<WlOutput, ()> for WaylandState {
    fn event(
        state: &mut Self,
        proxy: &WlOutput,
        event: <WlOutput as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Name { name } = event {
            let output = state
                .outputs
                .iter_mut()
                .find(|o| o.wl_output == *proxy)
                .expect("Received event for unknown output");
            log::debug!("New output `{}`, named `{}`", output.registry_name, name);
            output.output_name = Some(name);
        }
    }
}

impl Dispatch<ZwlrGammaControlManagerV1, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwlrGammaControlManagerV1,
        _event: <ZwlrGammaControlManagerV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrGammaControlV1, ()> for WaylandState {
    fn event(
        state: &mut Self,
        proxy: &ZwlrGammaControlV1,
        event: <ZwlrGammaControlV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let index = state
            .outputs
            .iter()
            .position(|o| o.gamma_control.as_ref().is_some_and(|g| g == proxy))
            .expect("Received event for unknown input");
        match event {
            zwlr_gamma_control_v1::Event::GammaSize { size } => {
                let output = &mut state.outputs[index];
                output.gamma_size = size as usize;
                log::debug!(
                    "New gamma control for output `{}`, gamma size is `{}`",
                    output.registry_name,
                    size
                );
            }
            zwlr_gamma_control_v1::Event::Failed => {
                let output = state.outputs.swap_remove(index);
                output.destroy();
                log::error!(
                    "Gamma control failed to assign gamma size to output `{}`",
                    output.registry_name
                );
            }
            _ => (),
        }
    }
}

#[cfg(test)]
#[serial]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn get_wayland() -> anyhow::Result<(
        Wayland,
        Receiver<anyhow::Result<()>>,
        Sender<WaylandRequest>,
    )> {
        let (req_sender, req_receiver) = mpsc::channel();
        let (res_sender, res_receiver) = mpsc::channel();
        let wayland = Wayland::new(res_sender, req_receiver)?;
        Ok((wayland, res_receiver, req_sender))
    }

    #[test]
    fn no_multiple_instance() {
        let (wayland, ..) = get_wayland().unwrap();
        assert!(get_wayland().is_err());
        let _ = wayland;
    }

    #[test]
    fn process_requests() {
        let (mut wayland, receiver, sender) = get_wayland().unwrap();

        sender
            .send(WaylandRequest::ChangeOutputColor(Color {
                temperature: 1000,
                gamma: 0.1,
                brightness: 0.1,
                inverted: true,
            }))
            .unwrap();

        std::thread::spawn(move || {
            wayland.process_requests();
        });

        assert!(receiver.recv().unwrap().is_ok());
    }
}
