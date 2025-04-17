mod color;
mod config;
mod switch_mode;
mod wayland;

use chrono::{Local, TimeDelta};
use clap::Parser;
use std::{
    fs::read_to_string,
    os::fd::{AsFd, AsRawFd},
    path::PathBuf,
    sync::mpsc::channel,
    thread,
    time::Duration,
};
use timerfd::{SetTimeFlags, TimerFd, TimerState};

use config::RawConfig;
use log::LevelFilter;
use simple_logger::SimpleLogger;
use switch_mode::{OutputMode, OutputState};
use wayland::{Wayland, WaylandRequest};

#[derive(Parser)]
#[command(version,about,long_about = None)]
struct Cli {
    /// Sets config file
    #[arg(short, long, value_name = "path")]
    config: Option<PathBuf>,
    #[arg(short, action = clap::ArgAction::Count, default_value_t = 3,
    help = "Sets the level of verbosity of logs\n(default is -vvv, max level is -vvvv)")]
    verbose: u8,
    /// Turn off all logs
    #[arg(short, long)]
    quite: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let level_filter = if cli.quite {
        LevelFilter::Off
    } else {
        match cli.verbose {
            1 => LevelFilter::Error,
            2 => LevelFilter::Warn,
            3 => LevelFilter::Info,
            _ => LevelFilter::Debug,
        }
    };

    SimpleLogger::new()
        .with_level(level_filter)
        .with_local_timestamps()
        .with_timestamp_format(time::macros::format_description!(
            "[year]-[month]-[day] [hour]:[minute]:[second]"
        ))
        .init()?;

    let path = cli
        .config
        .or(dirs::config_dir().map(|mut p| {
            p.push(env!("CARGO_PKG_NAME"));
            p.push("config.toml");
            p
        }))
        .ok_or_else(|| anyhow::anyhow!("Do not know where to look for a config file"))?;
    let content = &read_to_string(&path).map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => anyhow::anyhow!("File {:?} not found", &path),
        other => anyhow::anyhow!("Fail to read file {:?}, {:?}", &path, other),
    })?;
    let config = RawConfig::read(content)?.check()?;

    let (request_sender, request_receiver) = channel();
    let (wayland_sender, wayland_receiver) = channel();
    let mut wayland = Wayland::new(wayland_sender, request_receiver)?;

    thread::spawn(move || {
        wayland.process_requests();
    });

    let mut output_state = OutputState::new(config.switch_mode, config.location);
    let mut timerfd = TimerFd::new_custom(timerfd::ClockId::Boottime, false, false)?;
    let mut poll_array = [libc::pollfd {
        fd: timerfd.as_fd().as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    }];

    loop {
        log::info!("Enter {} mode", output_state.mode);
        request_sender.send(WaylandRequest::ChangeOutputColor(match output_state.mode {
            OutputMode::Day => config.day,
            OutputMode::Night => config.night,
        }))?;
        wayland_receiver.recv()??;

        let next_mode_switch = Local::now()
            + TimeDelta::new(output_state.delay_in_seconds, 0)
                .expect("Internal: Time delta out of bound");
        log::info!(
            "Thread sleep until {} for next mode switch",
            next_mode_switch.format("%Y-%m-%d %H:%M")
        );
        timerfd.set_state(
            TimerState::Oneshot(Duration::from_secs(output_state.delay_in_seconds as u64)),
            SetTimeFlags::Default,
        );
        loop {
            if unsafe { libc::poll(poll_array.as_mut_ptr(), poll_array.len() as _, -1) } == -1 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                Err(err)?;
            }
            break;
        }
        output_state.next();
    }
}
