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

use crate::color::{Color, fill_color_ramp};

pub enum WaylandRequest {
    ChangeOutputColor(String, Color),
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
                    WaylandRequest::ChangeOutputColor(output_name, color) => match &output_name[..]
                    {
                        "all" => {
                            for output in self.state.outputs.iter_mut() {
                                output.set_color(color)?;
                            }
                        }
                        _ => {
                            let output = self
                                .state
                                .outputs
                                .iter_mut()
                                .find(|o| o.output_name == output_name);
                            match output {
                                Some(output) => output.set_color(color)?,
                                None => {
                                    log::warn!("no output `{output_name}` found");
                                }
                            }
                        }
                    },
                }

                self.sender.send(Ok(()))?;
                self.connection.flush()?;
            }

            Ok(())
        })();

        if let Err(error) = result {
            if self.sender.send(Err(error)).is_err() {};
        }
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
struct DisplayOutput {
    registry_name: u32,
    wl_output: WlOutput,
    output_name: String,
    gamma_control: Option<ZwlrGammaControlV1>,
    gamma_size: usize,
    color: Color,
}

impl DisplayOutput {
    fn new(registry_name: u32, wl_output: WlOutput) -> Self {
        Self {
            registry_name,
            wl_output,
            output_name: "".to_string(),
            gamma_control: None,
            gamma_size: 0,
            color: Color::default(),
        }
    }

    fn update_gamma(&mut self) -> anyhow::Result<()> {
        if self.gamma_size == 0 {
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
            // all output should be assigned a gamma_control
            .unwrap()
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
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            log::debug!("global: [{name}] {interface} v{version}");
            if interface == WlOutput::interface().name {
                let wl_output = registry.bind::<WlOutput, _, _>(name, version, qh, ());
                state.outputs.push(DisplayOutput::new(name, wl_output));
            } else if interface == ZwlrGammaControlManagerV1::interface().name {
                state.gamma_manager =
                    Some(registry.bind::<ZwlrGammaControlManagerV1, _, _>(name, version, qh, ()));
            }
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
                // should be able to find the same wl_output right after binding
                .unwrap();
            output.output_name = name;
            log::info!("new output: {}", &output.output_name);
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
        if let zwlr_gamma_control_v1::Event::GammaSize { size } = event {
            let output = state
                .outputs
                .iter_mut()
                // all output should be assigned a gamma_control
                .find(|o| o.gamma_control.as_ref().unwrap() == proxy)
                // should be able to find the same gamma_control right after binding
                .unwrap();
            output.gamma_size = size as usize;
            log::info!("output {0} gamma size: {size}", &output.output_name);
        }
    }
}
