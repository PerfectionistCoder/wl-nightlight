mod color;
mod config;
mod event;
mod wayland;

use std::{sync::mpsc::channel, thread, time::Duration};

use config::RawConfig;
use event::{ColorEvent, ColorMode};
use wayland::{Wayland, WaylandRequest};

fn main() {
    let path = "extra/example.toml";
    let config = RawConfig::new(path).parse();

    let (sender, receiver) = channel();
    let mut wayland = Wayland::new(receiver);
    thread::spawn(move || {
        wayland.poll();
    });

    let mut event = ColorEvent::new(config.light_dark_time, config.location);
    loop {
        match event.mode {
            ColorMode::Light => {
                sender
                    .send(WaylandRequest::ChangeOutputColor(
                        "all".to_string(),
                        config.light,
                    ))
                    .unwrap();
            }
            ColorMode::Dark => {
                sender
                    .send(WaylandRequest::ChangeOutputColor(
                        "all".to_string(),
                        config.dark,
                    ))
                    .unwrap();
            }
        };
        thread::sleep(Duration::from_secs(event.wait_sec as u64));
        event.next();
    }
}
