mod color;
mod config;
mod switch_mode;
mod wayland;

use std::{sync::mpsc::channel, thread, time::Duration};

use config::RawConfig;
use switch_mode::{DisplayMode, DisplayModeState};
use wayland::{Wayland, WaylandRequest};

fn main() {
    let path = "extra/example.toml";
    let config = RawConfig::new(path).parse();

    let (sender, receiver) = channel();
    let mut wayland = Wayland::new(receiver);
    thread::spawn(move || {
        wayland.process_requests();
    });

    let mut event = DisplayModeState::new(config.switch_mode, config.location);
    loop {
        match event.mode {
            DisplayMode::Day => {
                sender
                    .send(WaylandRequest::ChangeOutputColor(
                        "all".to_string(),
                        config.day,
                    ))
                    .unwrap();
            }
            DisplayMode::Night => {
                sender
                    .send(WaylandRequest::ChangeOutputColor(
                        "all".to_string(),
                        config.night,
                    ))
                    .unwrap();
            }
        };
        thread::sleep(Duration::from_secs(event.delay_in_seconds as u64));
        event.next();
    }
}
