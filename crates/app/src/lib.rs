//! The game's data, sound and music, shared by its two shells: `main.rs` in windows on the device and macOS, and
//! `web.rs` in a web page.

pub mod audio;
mod loops;
pub mod pak;
pub mod samples;
#[cfg(target_arch = "wasm32")]
mod web;

/// The release this binary was built for, or `dev` and its commit.
pub const VERSION: &str = env!("LAST_HARVEST_BUILD");
