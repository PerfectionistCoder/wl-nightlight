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
    let cfg = Config::new(None).unwrap();

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
        let timer = ModeTimer::new(cfg.location().lat(), cfg.location().lng());
        let mode = if timer.mode() == LightMode::Light {
            cfg.light()
        } else {
            cfg.dark()
        };

        state.lock().unwrap().change_to_color(mode, {
            if first {
                first = false;
                0.0
            } else {
                cfg.animation().transition()
            }
        });

        eprintln!("wait: {}", timer.next());
        sleep(Duration::from_secs(timer.next() as u64));
    }
}
