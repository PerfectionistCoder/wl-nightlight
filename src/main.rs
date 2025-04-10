use std::os::fd::{BorrowedFd, IntoRawFd};

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

mod color;

use color::{Color, colorramp_fill};

#[derive(Debug)]
struct WaylandState {
    outputs: Vec<Output>,
    gamma_manager: Option<ZwlrGammaControlManagerV1>,
}
impl WaylandState {
    pub fn new() -> Self {
        Self {
            gamma_manager: None,
            outputs: Vec::new(),
        }
    }
}

#[derive(Debug)]
struct Output {
    global_name: u32,
    wl_output: WlOutput,
    event_name: Option<String>,
    gamma_control: Option<ZwlrGammaControlV1>,
    gamma_size: Option<usize>,
    color: Color,
    color_changed: bool,
}

impl Output {
    fn new(global_name: u32, wl_output: WlOutput) -> Self {
        Self {
            global_name,
            wl_output,
            event_name: None,
            gamma_control: None,
            gamma_size: None,
            color: Color::default(),
            color_changed: false,
        }
    }

    fn set_gamma(&mut self) {
        let file = shmemfdrs2::create_shmem(c"/ramp-buffer").unwrap();
        file.set_len(self.gamma_size.unwrap() as u64 * 6).unwrap();
        let mut mmap = unsafe { memmap2::MmapMut::map_mut(&file).unwrap() };
        let buf = bytemuck::cast_slice_mut::<u8, u16>(&mut mmap);
        let (r, rest) = buf.split_at_mut(self.gamma_size.unwrap());
        let (g, b) = rest.split_at_mut(self.gamma_size.unwrap());
        colorramp_fill(r, g, b, self.gamma_size.unwrap(), self.color);
        let fd = unsafe { BorrowedFd::borrow_raw(file.into_raw_fd()) };
        self.gamma_control.as_ref().unwrap().set_gamma(fd);
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
                state.outputs.push(Output::new(name, wl_output));
                let mut event_queue = conn.new_event_queue();
                event_queue.roundtrip(state).unwrap();
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
            output.event_name = Some(name);
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
        conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let zwlr_gamma_control_v1::Event::GammaSize { size } = event {
            let output = state
                .outputs
                .iter_mut()
                .find(|o| o.gamma_control.as_ref().unwrap() == proxy)
                .unwrap();
            output.gamma_size = Some(size as usize);
            output.set_gamma();
            let mut event_queue = conn.new_event_queue();
            event_queue.roundtrip(state).unwrap();
        }
    }
}

fn main() {
    let conn = Connection::connect_to_env().unwrap();

    let display = conn.display();

    let mut event_queue = conn.new_event_queue();
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
    println!("{:?}", state);
    loop {}
}
