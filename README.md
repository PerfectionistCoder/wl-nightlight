# wl-nightlight

## Prerequisites

- A Wayland compositor supporting the `wlr-gamma-control-unstable-v1` protocol

## Features

- Adjusts screen color temperature, gamma, and brightness
- Supports both automatic (sunrise/sunset) and fixed time scheduling

## Installation

### From Source

1. Clone this repository
2. Build and install:
   ```sh
   cargo install --path .
   ```

## Configuration

wl-nightlight uses a TOML configuration file. By default, it looks for the config at:

`${XDG_CONFIG_HOME}/wl-nightlight/config.toml`

If `XDG_CONFIG_HOME` is unset, the default of `~/.config` is used.

Configuration documentation is in the [example config file](extra/example.toml)

## Usage

Run `wl-nightlight -h` for help on command line options.
