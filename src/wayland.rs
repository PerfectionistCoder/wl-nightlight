mod client;
mod output;
mod state;

pub use client::WaylandClient;

use std::sync::{Arc, Mutex, MutexGuard};

fn unwrap_output<T>(output: &Arc<Mutex<T>>) -> MutexGuard<'_, T> {
    output.lock().unwrap()
}
