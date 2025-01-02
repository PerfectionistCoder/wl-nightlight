use std::{
    process::exit,
    sync::{Arc, Mutex},
    thread::{self, sleep},
    time::Duration,
};

use config::Config;
use timer::{LightMode, ModeTimer};

mod color;
mod config;
mod timer;
mod wayland;

pub fn run(mut args: impl Iterator<Item = String>) {
    let program = args.next().unwrap();

    let cfg = Config::load(None).unwrap_or_else(|err| {
        eprintln!("{program}: {err}");
        exit(1);
    });

    let (mut wayland, wayland_state) = wayland::WaylandClient::create().unwrap();
    let state = Arc::new(Mutex::new(wayland_state));

    {
        let state = Arc::clone(&state);
        thread::spawn(move || loop {
            if state.lock().unwrap().color_changed() {
                wayland.poll(&mut state.lock().unwrap()).unwrap();
            }
            sleep(Duration::from_millis(1));
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

        let handles = state.lock().unwrap().change_to_color(mode, {
            if first {
                first = false;
                0.0
            } else {
                cfg.animation().transition()
            }
        });

        sleep(Duration::from_secs(timer.next() as u64));
        handles.into_iter().for_each(|h| h.join().unwrap());
    }
}
