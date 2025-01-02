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
    let print_error = |op: Box<dyn FnOnce()>| {
        eprint!("{program}: ");
        op();
        exit(1);
    };

    let cfg = Config::new(Some(String::from("example-config.ini"))).unwrap_or_else(|err| {
        print_error(Box::new(move || {
            eprintln!("{err}");
        }))
    });

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
