//! The game's data, sound and music, shared by its shells: `main.rs` in windows on the device and macOS, `web.rs`
//! in a web page, and `android.rs` on a phone.

pub mod audio;
pub mod log;
mod loops;
pub mod pak;
pub mod samples;
#[cfg(target_os = "android")]
mod android;
#[cfg(target_arch = "wasm32")]
mod web;

/// The release this binary was built for, or `dev` and its commit.
pub const VERSION: &str = env!("LAST_HARVEST_BUILD");

/// Draws a phone's frame on a surface `size` pixels across and down: black, then each screen in its rect, then the
/// on-screen controls over everything.
pub fn draw_phone(renderer: &mut last_harvest_render::Renderer, gl: &glow::Context, frame: &last_harvest_game::Frame, phone: &last_harvest_game::touch::Phone, [w, h]: [i32; 2]) {
    let black = last_harvest_game::Canvas { clear: [0.0, 0.0, 0.0, 1.0], ..Default::default() };
    renderer.draw(gl, &black, [0, 0, w, h]);
    for (rect, canvas) in [(phone.top, &frame.top), (phone.bottom, &frame.bottom)] {
        if let Some([x, y, rw, rh]) = rect {
            let [x, y, rw, rh] = [x.round() as i32, y.round() as i32, rw.round() as i32, rh.round() as i32];
            renderer.draw(gl, canvas, [x, h - y - rh, rw, rh]);
        }
    }
    renderer.draw(gl, &phone.pad, [0, 0, w, h]);
}
