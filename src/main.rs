mod color;
mod event;
mod wayland;

use std::{sync::mpsc::channel, thread};

use wayland::Wayland;

fn main() {
    let (sender, receiver) = channel();
    let mut wayland = Wayland::new(receiver);
    thread::spawn(move || {
        wayland.poll();
    });

    sender
        .send(wayland::WaylandRequest::ChangeOutputColor(
            "HDMI-A-1".to_string(),
            color::Color {
                temp: 5500,
                gamma: 1.0,
                brightness: 1.0,
                inverted: false,
            },
        ))
        .unwrap();
    loop {}
}
