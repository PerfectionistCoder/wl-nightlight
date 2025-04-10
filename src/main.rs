use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle,
    protocol::{
        wl_output::{self, WlOutput},
        wl_registry,
    },
};
use wayland_protocols_wlr::gamma_control::v1::client::zwlr_gamma_control_manager_v1::ZwlrGammaControlManagerV1;

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
}

impl Output {
    pub fn new(global_name: u32, wl_output: WlOutput) -> Self {
        Self {
            global_name,
            wl_output,
            event_name: None,
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
        _qhandle: &QueueHandle<Self>,
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
        _qhandle: &QueueHandle<Self>,
    ) {
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
    println!("{:?}", state);
}
