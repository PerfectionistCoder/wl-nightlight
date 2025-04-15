mod color;
mod config;
mod switch_mode;
mod wayland;

use clap::Parser;
use std::{fs::read_to_string, path::PathBuf, sync::mpsc::channel, thread, time::Duration};

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
        .ok_or_else(|| anyhow::anyhow!("do not know where to look for a config file"))?;
    let content =
        &read_to_string(&path).map_err(|_| anyhow::anyhow!("fail to read file {:?}", &path))?;
    let config = RawConfig::read(content)?.check()?;

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
