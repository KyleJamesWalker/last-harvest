//! The game's data, sound and music, shared by its shells: `main.rs` in windows on the device and macOS, `web.rs`
//! in a web page, and `android.rs` on a phone.

#[cfg(all(feature = "store", target_arch = "wasm32"))]
compile_error!(
    "the store feature builds in game_data.pak, which holds the licensed tileset: never for the web"
);

#[cfg(target_os = "android")]
mod android;
pub mod audio;
pub mod clock;
pub mod controllers;
pub mod log;
mod loops;
pub mod online;
pub mod pak;
pub mod platform;
pub mod replays;
pub mod samples;
#[cfg(not(target_arch = "wasm32"))]
pub mod shot;
#[cfg(target_arch = "wasm32")]
mod web;

/// The folder the game keeps its data, save and screenshots in: the binary's, or the one holding its app bundle on
/// macOS, as a bundle is replaced whole on an update.
pub fn home() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let bundle = dir.ancestors().nth(2).filter(|app| {
        dir.ends_with("Contents/MacOS") && app.extension().is_some_and(|e| e == "app")
    });
    Some(
        bundle
            .and_then(|app| app.parent())
            .unwrap_or(dir)
            .to_path_buf(),
    )
}

/// The release this binary was built for, or `dev` and its commit.
pub const VERSION: &str = env!("LAST_HARVEST_BUILD");

/// Views for trying split-screen, from `LAST_HARVEST_SPLIT` or the web page's `?split=`: 2 to 4, else none.
pub fn split_views(v: &str) -> Option<usize> {
    v.trim()
        .parse()
        .ok()
        .filter(|n| (2..=controllers::MAX_PLAYERS).contains(n))
}

/// Players 2 onward's input this frame from their seats' pads, each with presses from its own tracker.
pub fn seated_others(
    seated: &[(last_harvest_game::Buttons, [f32; 2], f32)],
    trackers: &mut Vec<last_harvest_game::InputTracker>,
) -> Vec<last_harvest_game::Input> {
    trackers.resize_with(seated.len().saturating_sub(1), Default::default);
    seated
        .iter()
        .skip(1)
        .zip(trackers)
        .map(|(&(held, stick, look), t)| {
            t.frame(last_harvest_game::RawInput {
                held,
                stick,
                look,
                ..Default::default()
            })
        })
        .collect()
}

/// How many views to draw on a surface `size` pixels across and down, one per local player during a split run, after
/// telling the game each one's size.
pub fn lay_out_views(game: &mut last_harvest_game::Game, splits: bool, [w, h]: [u32; 2]) -> usize {
    let views = if splits && game.in_run() {
        game.views()
    } else {
        1
    };
    game.set_view_sizes(
        last_harvest_render::split::split(views, [w, h])
            .iter()
            .map(|r| [r[2] as f32, r[3] as f32])
            .collect(),
    );
    views
}

/// Player 1's canvas then each other player's, for `draw_split`.
pub fn frame_views(frame: &last_harvest_game::Frame, n: usize) -> Vec<&last_harvest_game::Canvas> {
    std::iter::once(&frame.top)
        .chain(&frame.views)
        .take(n)
        .collect()
}

/// Draws one canvas per local player on a surface `size` pixels across and down, laid out as `split` tiles it, with
/// black where no view goes.
pub fn draw_split(
    renderer: &mut last_harvest_render::Renderer,
    gl: &glow::Context,
    views: &[&last_harvest_game::Canvas],
    [w, h]: [i32; 2],
) {
    let black = last_harvest_game::Canvas {
        clear: [0.0, 0.0, 0.0, 1.0],
        ..Default::default()
    };
    renderer.draw(gl, &black, [0, 0, w, h]);
    let rects = last_harvest_render::split::split(views.len(), [w.max(0) as u32, h.max(0) as u32]);
    for (canvas, [x, y, vw, vh]) in views.iter().zip(rects) {
        renderer.draw_view(
            gl,
            canvas,
            [x as i32, h - (y + vh) as i32, vw as i32, vh as i32],
        );
    }
}

/// Draws a phone's frame on a surface `size` pixels across and down: black, then each screen in its rect, then the
/// on-screen controls over everything.
pub fn draw_phone(
    renderer: &mut last_harvest_render::Renderer,
    gl: &glow::Context,
    frame: &last_harvest_game::Frame,
    phone: &last_harvest_game::touch::Phone,
    [w, h]: [i32; 2],
) {
    let black = last_harvest_game::Canvas {
        clear: [0.0, 0.0, 0.0, 1.0],
        ..Default::default()
    };
    renderer.draw(gl, &black, [0, 0, w, h]);
    for (rect, canvas) in [(phone.top, &frame.top), (phone.bottom, &frame.bottom)] {
        if let Some([x, y, rw, rh]) = rect {
            let [x, y, rw, rh] = [
                x.round() as i32,
                y.round() as i32,
                rw.round() as i32,
                rh.round() as i32,
            ];
            renderer.draw(gl, canvas, [x, h - y - rh, rw, rh]);
        }
    }
    renderer.draw(gl, &phone.pad, [0, 0, w, h]);
}

#[cfg(test)]
mod tests {
    #[test]
    fn split_views_take_two_to_four() {
        assert_eq!(
            ["2", " 4 ", "1", "5", "", "two"].map(super::split_views),
            [Some(2), Some(4), None, None, None, None]
        );
    }
}
