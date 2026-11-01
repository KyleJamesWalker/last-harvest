//! `log.txt` lines, each after the seconds since the first, so a line can be put against `crash.log`'s Unix time.

use std::fmt::Arguments;

/// Writes one line to stderr, after the seconds since the first line.
#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => { $crate::log::line(format_args!($($arg)*)) };
}

#[cfg(target_os = "android")]
pub fn line(args: Arguments) {
    unsafe extern "C" {
        fn __android_log_write(prio: i32, tag: *const std::ffi::c_char, text: *const std::ffi::c_char) -> i32;
    }
    let text = std::ffi::CString::new(args.to_string().replace('\0', " ")).unwrap_or_default();
    // 4 is ANDROID_LOG_INFO.
    unsafe { __android_log_write(4, c"last-harvest".as_ptr(), text.as_ptr()) };
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
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
