use wl_nightlight::wayland;

fn main() {
    let (mut wayland, mut wayland_state) = wayland::Wayland::new().unwrap();

    wayland_state.set_brightness(0.5);
    loop {
        wayland.poll(&mut wayland_state);
    }
}
