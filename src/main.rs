mod color;
mod config;
mod switch_mode;
mod wayland;

use std::{fs::read_to_string, sync::mpsc::channel, thread, time::Duration};

use ::log::LevelFilter;
use config::RawConfig;
use simple_logger::SimpleLogger;
use switch_mode::{OutputMode, OutputState};
use wayland::{Wayland, WaylandRequest};

fn main() -> anyhow::Result<()> {
    SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .with_local_timestamps()
        .with_timestamp_format(time::macros::format_description!(
            "[year]-[month]-[day] [hour]:[minute]:[second]"
        ))
        .init()?;

    let path = "extra/example.toml";
    let config = RawConfig::read(&read_to_string(path).unwrap())?
        .check()?
        .parse()?;

    let (request_sender, request_receiver) = channel();
    let (wayland_sender, wayland_receiver) = channel();
    let mut wayland = Wayland::new(wayland_sender, request_receiver)?;

    thread::spawn(move || {
        wayland.process_requests();
    });

    let mut output_state = OutputState::new(config.switch_mode, config.location);
    loop {
        log::info!("enter {} mode", output_state.mode);
        request_sender.send(WaylandRequest::ChangeOutputColor(
            "all".to_string(),
            match output_state.mode {
                OutputMode::Day => config.day,
                OutputMode::Night => config.night,
            },
        ))?;
        wayland_receiver.recv()??;

        log::info!(
            "thread sleep for {} seconds until next mode switch",
            output_state.delay_in_seconds
        );
        thread::sleep(Duration::from_secs(output_state.delay_in_seconds as u64));
        output_state.next();
    }
}
