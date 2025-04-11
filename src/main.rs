mod color;
mod wayland;

use wayland::Wayland;

fn main() {
    let mut wayland = Wayland::new();
    wayland.state.outputs[0].set_gamma();
    wayland.conn.flush().unwrap();
    loop {}
}
