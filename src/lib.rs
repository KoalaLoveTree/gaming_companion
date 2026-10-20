#![cfg_attr(not(feature = "window"), allow(dead_code))]

mod chrome;
mod controller;
mod error;
mod model;
mod settings;

#[cfg(all(feature = "window", target_os = "linux"))]
mod frame;
#[cfg(all(feature = "window", target_os = "linux"))]
mod gnome_pin;

#[cfg(all(feature = "window", target_os = "linux"))]
mod portal;

#[cfg(feature = "window")]
mod tray;
#[cfg(feature = "window")]
mod window;

pub use error::{Error, Result};

/// Load plugins, open the active resource, and run until the window closes.
#[cfg(feature = "window")]
pub fn launch() -> Result<()> {
    window::launch()
}
