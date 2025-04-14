use std::{os::fd::AsFd, sync::mpsc::Receiver};

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

use crate::color::{DisplayColor, fill_color_ramp};

pub enum WaylandRequest {
    ChangeOutputColor(String, DisplayColor),
}

pub struct Wayland {
    pub connection: Connection,
    pub state: WaylandState,
    receiver: Receiver<WaylandRequest>,
}

impl Wayland {
    pub fn new(receiver: Receiver<WaylandRequest>) -> Self {
        let connection = Connection::connect_to_env().unwrap();

        let display = connection.display();

        let mut event_queue = connection.new_event_queue();
        let qh = event_queue.handle();

        let mut state = WaylandState::new();
        display.get_registry(&qh, ());
        event_queue.roundtrip(&mut state).unwrap();

        if let Some(gamma_manager) = &state.gamma_manager {
            for output in &mut state.outputs {
                let wl_output = &output.wl_output;
                output.gamma_control = Some(gamma_manager.get_gamma_control(wl_output, &qh, ()));
            }
        }
        event_queue.roundtrip(&mut state).unwrap();

        Self {
            connection,
            state,
            receiver,
        }
    }

    pub fn process_requests(&mut self) {
        while let Ok(request) = self.receiver.recv() {
            self.connection
                .new_event_queue()
                .roundtrip(&mut self.state)
                .unwrap();

            match request {
                WaylandRequest::ChangeOutputColor(output_name, color) => {
                    if &output_name == "all" {
                        for output in self.state.outputs.iter_mut() {
                            output.set_color(color);
                        }
                    } else {
                        let output = self
                            .state
                            .outputs
                            .iter_mut()
                            .find(|o| o.output_name == output_name)
                            .unwrap();
                        output.set_color(color);
                    }
                }
            }
            self.connection.flush().unwrap();
        }
    }
}

#[derive(Debug)]
pub struct WaylandState {
    pub outputs: Vec<DisplayOutput>,
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
pub struct DisplayOutput {
    global_name: u32,
    wl_output: WlOutput,
    output_name: String,
    gamma_control: Option<ZwlrGammaControlV1>,
    gamma_size: usize,
    color: DisplayColor,
}

impl DisplayOutput {
    fn new(global_name: u32, wl_output: WlOutput) -> Self {
        Self {
            global_name,
            wl_output,
            output_name: "".to_string(),
            gamma_control: None,
            gamma_size: 0,
            color: DisplayColor::default(),
        }
    }

    fn update_gamma(&mut self) {
        let file = shmemfdrs2::create_shmem(c"/ramp-buffer").unwrap();
        file.set_len(self.gamma_size as u64 * 6).unwrap();
        let mut mmap = unsafe { memmap2::MmapMut::map_mut(&file).unwrap() };
        let buf = bytemuck::cast_slice_mut::<u8, u16>(&mut mmap);
        let (r, rest) = buf.split_at_mut(self.gamma_size);
        let (g, b) = rest.split_at_mut(self.gamma_size);
        fill_color_ramp(r, g, b, self.gamma_size, self.color);
        self.gamma_control.as_ref().unwrap().set_gamma(file.as_fd());
    }

    fn set_color(&mut self, color: DisplayColor) {
        if self.color != color {
            self.color = color;
            self.update_gamma();
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for WaylandState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        conn: &Connection,
        qh: &QueueHandle<WaylandState>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            if interface == WlOutput::interface().name {
                let wl_output = registry.bind::<WlOutput, _, _>(name, version, qh, ());
                state.outputs.push(DisplayOutput::new(name, wl_output));
                conn.new_event_queue().roundtrip(state).unwrap();
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
                .unwrap();
            output.output_name = name;
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
                .find(|o| o.gamma_control.as_ref().unwrap() == proxy)
                .unwrap();
            output.gamma_size = size as usize;
        }
    }
}
