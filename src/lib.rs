use std::{
    sync::{Arc, Mutex},
    thread::{self, sleep}, time::Duration,
};

use config::Config;

mod color;
mod config;
mod mode;
mod wayland;

pub fn run() {
    let cfg = Config::new(None).unwrap();
    let (mut wayland, wayland_state) = wayland::WaylandClient::new().unwrap();
    let state = Arc::new(Mutex::new(wayland_state));

    let state1 = Arc::clone(&state);
    thread::spawn(move || {
        state1.lock().unwrap().change_to_color(cfg.dark_mode(), 3.0);
    });
    
    loop {
        if state.lock().unwrap().color_changed() {
            let mut state = state.lock().unwrap();
            let _ = wayland.poll(&mut state);
        }
        sleep(Duration::from_millis(10));
    }
}

#[cfg(test)]
mod test_utils;
