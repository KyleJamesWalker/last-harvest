#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
//! The game on the device, desktops and macOS, with the public build's platform backend.

fn main() {
    last_harvest::desktop::run(last_harvest::platform::backend());
}
