//! The game's data, sound and music, shared by its shells: `main.rs` in windows on the device and macOS, `web.rs`
//! in a web page, and `android.rs` on a phone.

pub mod audio;
pub mod clock;
pub mod controllers;
pub mod log;
mod loops;
pub mod pak;
pub mod samples;
#[cfg(not(target_arch = "wasm32"))]
pub mod shot;
#[cfg(target_os = "android")]
mod android;
#[cfg(target_arch = "wasm32")]
mod web;

/// The folder the game keeps its data, save and screenshots in: the binary's, or the one holding its app bundle on
/// macOS, as a bundle is replaced whole on an update.
pub fn home() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let bundle = dir.ancestors().nth(2).filter(|app| dir.ends_with("Contents/MacOS") && app.extension().is_some_and(|e| e == "app"));
    Some(bundle.and_then(|app| app.parent()).unwrap_or(dir).to_path_buf())
}

/// The release this binary was built for, or `dev` and its commit.
pub const VERSION: &str = env!("LAST_HARVEST_BUILD");

/// Views for trying split-screen, from `LAST_HARVEST_SPLIT` or the web page's `?split=`: 2 to 4, else none.
pub fn split_views(v: &str) -> Option<usize> {
    v.trim().parse().ok().filter(|n| (2..=controllers::MAX_PLAYERS).contains(n))
}

/// Draws one canvas per local player on a surface `size` pixels across and down, laid out as `split` tiles it, with
/// black where no view goes.
pub fn draw_split(renderer: &mut last_harvest_render::Renderer, gl: &glow::Context, views: &[&last_harvest_game::Canvas], [w, h]: [i32; 2]) {
    let black = last_harvest_game::Canvas { clear: [0.0, 0.0, 0.0, 1.0], ..Default::default() };
    renderer.draw(gl, &black, [0, 0, w, h]);
    let rects = last_harvest_render::split::split(views.len(), [w.max(0) as u32, h.max(0) as u32]);
    for (canvas, [x, y, vw, vh]) in views.iter().zip(rects) {
        renderer.draw_view(gl, canvas, [x as i32, h - (y + vh) as i32, vw as i32, vh as i32]);
    }
}

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

#[cfg(test)]
mod tests {
    #[test]
    fn split_views_take_two_to_four() {
        assert_eq!(["2", " 4 ", "1", "5", "", "two"].map(super::split_views), [Some(2), Some(4), None, None, None, None]);
    }
}
