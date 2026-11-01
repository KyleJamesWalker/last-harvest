//! `log.txt` lines, each after the seconds since the first, so a line can be put against `crash.log`'s Unix time.

use std::fmt::Arguments;

/// Writes one line to stderr, after the seconds since the first line.
#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => { $crate::log::line(format_args!($($arg)*)) };
}

#[cfg(not(target_arch = "wasm32"))]
pub fn line(args: Arguments) {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let secs = START.get_or_init(std::time::Instant::now).elapsed().as_secs_f32();
    eprintln!("{secs:9.3} {args}");
}

#[cfg(target_arch = "wasm32")]
pub fn line(args: Arguments) {
    eprintln!("{args}");
}

/// Seconds since the Unix epoch, as `crash.log` stamps a crash.
#[cfg(not(target_arch = "wasm32"))]
pub fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}
