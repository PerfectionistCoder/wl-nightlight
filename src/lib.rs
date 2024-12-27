use std::{thread::sleep, time::Duration};

use config::Config;
use mode::LightMode::{self, Dark, Light};

mod color;
mod config;
mod mode;
mod wayland;

pub fn run() {
    let cfg = Config::new();
    let mut time;
    let (mut wayland, mut wayland_state) = wayland::WaylandClient::new().unwrap();

    loop {
        println!("{}", wayland_state.color_changed());

        let mode = LightMode::get_mode(cfg.lat(), cfg.lng()).unwrap();
        match mode {
            Light(t) => {
                wayland_state.change_to_color(cfg.light_mode());
                time = t;
            }
            Dark(t) => {
                wayland_state.change_to_color(cfg.dark_mode());
                time = t;
            }
        };
        sleep(Duration::from_secs_f64(0.1));
        
        loop {
            if wayland.poll(&mut wayland_state).is_ok() {
                println!("change");
                break;
            }
            sleep(Duration::from_millis(50));
        }

        println!("wait {}", time);
        println!("{:?}", wayland_state.outputs());
        sleep(Duration::from_secs(time as u64));
    }
}

#[cfg(test)]
mod test_utils;
