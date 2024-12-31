use std::{
    sync::{Arc, Mutex},
    thread::{self, sleep},
    time::Duration,
};

use config::Config;
use mode::{LightMode, ModeTimer};

mod color;
mod config;
mod mode;
mod wayland;

pub fn run() {
    let cfg = Config::new(Some(String::from("example-config.ini"))).unwrap();

    let (mut wayland, wayland_state) = wayland::WaylandClient::new().unwrap();
    let state = Arc::new(Mutex::new(wayland_state));

    {
        let state = Arc::clone(&state);
        thread::spawn(move || loop {
            if state.lock().unwrap().color_changed() {
                eprintln!("update color");
                let mut state = state.lock().unwrap();
                wayland.poll(&mut state).unwrap();
            }
        });
    }

    let mut first = true;
    loop {
        let timer = ModeTimer::new(cfg.location()).unwrap();
        let mode = if timer.mode() == LightMode::Light {
            cfg.light_mode()
        } else {
            cfg.dark_mode()
        };

        if first {
            state.lock().unwrap().change_to_color(mode, 0.0);
            first = false;
        } else {
            state.lock().unwrap().change_to_color(mode, cfg.animation().transition);
        }
        eprintln!("wait: {}", timer.next());
        sleep(Duration::from_secs(timer.next() as u64));
    }
}
