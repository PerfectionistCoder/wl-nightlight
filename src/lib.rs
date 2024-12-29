use std::{thread::sleep, time::Duration};

use config::Config;
use mode::{
    get_mode,
    LightMode::{Dark, Light},
};

mod color;
mod config;
mod mode;
mod wayland;

pub fn run() {
    let cfg = Config::new();
    let mut time;
    let (mut wayland, mut wayland_state) = wayland::Wayland::new().unwrap();

    loop {
        println!("{}", wayland_state.color_changed());

        let mode = get_mode(cfg.lat(), cfg.lng()).unwrap();
        wayland_state = match mode {
            Light(t) => {
                let s = wayland_state.change_to_color(cfg.light_mode(), 0);
                time = t;
                s
            }
            Dark(t) => {
                let s = wayland_state.change_to_color(cfg.dark_mode(), 0);
                time = t;
                s
            }
        };

        loop {
            if wayland.poll(&mut wayland_state).is_ok() {
                println!("change");
                break;
            }
            sleep(Duration::from_millis(50));
        }

        println!("wait {}", time);
        sleep(Duration::from_secs(time as u64));
    }
}

#[cfg(test)]
mod test_utils;
