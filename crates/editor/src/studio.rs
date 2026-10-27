//! The editor and its two previews, as both the native window and the web page lay them out: the editing pane
//! on the left and the 2.5D view above the map on the right, each 4:3 like the device's screens.

use last_harvest_game::art::{self, ATLAS_W};
use last_harvest_game::{Align, Canvas, SCREEN_H, SCREEN_W, Showcase};

use crate::editor::Editor;
use crate::store::Store;

const SHOWCASE_SEED: u64 = 7;
/// Radians the 2.5D view turns per pane unit dragged.
const TURN: f32 = 0.005;
/// How near and far the 2.5D view comes, as the player's distance from the sprite; below zero they pass it.
const NEAREST: f32 = -12.0;
const FARTHEST: f32 = 200.0;
/// The 2.5D view's switch for the player, as x, y, w, h in the pane.
const PLAYER_SWITCH: [f32; 4] = [SCREEN_W - 300.0, 8.0, 290.0, 48.0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Edit,
    View,
    Map,
}

pub struct Studio {
    pub editor: Editor,
    showcase: Showcase,
    /// The packed texture with the sprite being edited drawn into the scratch area.
    atlas: Vec<u8>,
    yaw: f32,
    distance: f32,
    zoom: f32,
    /// The map's zoom as a pinch leaves it, before rounding to whole steps.
    map_lens: f32,
    /// The pane a press began in, which keeps the pointer until it lets go.
    held: Option<Pane>,
    /// How the previews' labels say to move them.
    pub hints: [&'static str; 2],
    /// Whether the player stands in the previews.
    player: bool,
}

impl Studio {
    pub fn new(store: Box<dyn Store>, name: &str) -> Self {
        Self {
            editor: Editor::new(store, name),
            showcase: Showcase::new(SHOWCASE_SEED),
            atlas: art::pixels().to_vec(),
            yaw: Showcase::yaw(),
            distance: 48.0,
            zoom: 2.0,
            map_lens: 2.0,
            held: None,
            hints: ["drag to turn, scroll to move closer", "scroll to zoom"],
            player: true,
        }
    }

    /// Which pane a point falls in, for a window `size` wide and tall, and where in that pane's 1024x768 canvas.
    pub fn locate(size: [f32; 2], [x, y]: [f32; 2]) -> (Pane, [f32; 2]) {
        let panes = Self::panes(size);
        let (cx, cy) = (x.clamp(0.0, size[0] - 1.0), y.clamp(0.0, size[1] - 1.0));
        let at = panes.iter().position(|&[px, py, pw, ph]| cx >= px && cy >= py && cx < px + pw && cy < py + ph);
        let pane = [Pane::Edit, Pane::View, Pane::Map][at.unwrap_or(0)];
        let [px, py, pw, ph] = panes[pane as usize];
        (pane, [(x - px) / pw * SCREEN_W, (y - py) / ph * SCREEN_H])
    }

    /// The editing pane, the 2.5D view and the map as x, y, w, h from the top-left of a window `size` big. A wide
    /// window, twice as wide as tall, puts the view and the map down the right; a tall one, 8 wide by 9 tall, puts
    /// them side by side under the editing pane.
    pub fn panes([w, h]: [f32; 2]) -> [[f32; 4]; 3] {
        if h > w {
            let (top, half) = (w * 0.75, w / 2.0);
            return [[0.0, 0.0, w, top], [0.0, top, half, h - top], [half, top, w - half, h - top]];
        }
        let edit = w * 2.0 / 3.0;
        [[0.0, 0.0, edit, h], [edit, 0.0, w - edit, h / 2.0], [edit, h / 2.0, w - edit, h / 2.0]]
    }

    /// Moves time on; returns the art texture when the sprite changed, for the renderer to upload.
    pub fn tick(&mut self, dt: f32) -> Option<&[u8]> {
        self.editor.tick(dt);
        let packed = std::mem::take(&mut self.editor.changed);
        if packed {
            self.pack();
        }
        self.showcase.show(self.editor.shown(), self.editor.stand(), self.editor.fps, self.editor.top_art());
        self.showcase.show_player(self.player);
        self.showcase.update(dt, self.yaw, self.distance);
        packed.then_some(&self.atlas[..])
    }

    /// Draws the sprite's frames into the scratch area and points the live sprites at them.
    fn pack(&mut self) {
        let [sx, sy, sw, sh] = art::SCRATCH.rect().map(|v| v as usize);
        for row in sy..sy + sh {
            let i = (row * ATLAS_W as usize + sx) * 4;
            self.atlas[i..i + sw * 4].fill(0);
        }
        let mut rects = Vec::new();
        let (mut x, mut y, mut shelf) = (0, 0, 0);
        let surfaces = self.editor.surfaces();
        for f in &surfaces {
            if x + f.w > sw {
                (x, y, shelf) = (0, y + shelf + 1, 0);
            }
            if f.w > sw || y + f.h > sh {
                break;
            }
            for row in 0..f.h {
                for col in 0..f.w {
                    let i = ((sy + y + row) * ATLAS_W as usize + sx + x + col) * 4;
                    self.atlas[i..i + 4].copy_from_slice(&f.px[row * f.w + col]);
                }
            }
            rects.push([(sx + x) as u16, (sy + y) as u16, f.w as u16, f.h as u16]);
            x += f.w + 1;
            shelf = shelf.max(f.h);
        }
        if rects.len() < surfaces.len() {
            self.editor.say(format!("Too big to preview whole: the frames and views must fit in {sw}x{sh} pixels"));
        }
        let live = art::set_live(rects, self.atlas.clone());
        self.editor.set_live(live);
    }

    /// The editing pane, the 2.5D view and the map, in the order `panes` places them.
    pub fn canvases(&self) -> [Canvas; 3] {
        let mut edit = Canvas::default();
        self.editor.draw(&mut edit);
        let mut view = self.showcase.top();
        view.label(format!("2.5D view: {}", self.hints[0]), 16.0, 12.0, 30.0, [1.0, 1.0, 1.0, 0.85], Align::Left);
        let [x, y, w, h] = PLAYER_SWITCH;
        view.rect(x, y, w, h, [0.0, 0.0, 0.0, 0.5]);
        let label = if self.player { "Player: shown" } else { "Player: hidden" };
        view.label(label, x + w / 2.0, y + 8.0, 28.0, [1.0, 1.0, 1.0, 0.9], Align::Center);
        let mut map = self.showcase.map(self.zoom);
        map.label(format!("Map at {}x the game's scale: {}", self.zoom, self.hints[1]), 16.0, 12.0, 30.0, [1.0, 1.0, 1.0, 0.85], Align::Left);
        [edit, view, map]
    }

    pub fn press(&mut self, pane: Pane, p: [f32; 2], right: bool, shift: bool) {
        let [x, y, w, h] = PLAYER_SWITCH;
        if pane == Pane::View && (x..x + w).contains(&p[0]) && (y..y + h).contains(&p[1]) {
            self.player = !self.player;
            return;
        }
        self.held = Some(pane);
        if pane == Pane::Edit {
            self.editor.mouse_down(p, right, shift);
        }
    }

    /// The pointer moved to `p` in `pane`, by `dx` pane units across.
    pub fn motion(&mut self, pane: Pane, p: [f32; 2], dx: f32) {
        match self.held {
            Some(Pane::View) => self.yaw += dx * TURN,
            Some(Pane::Map) => {}
            _ if pane == Pane::Edit => self.editor.drag(p),
            _ => {}
        }
    }

    pub fn release(&mut self) {
        self.held = None;
        self.editor.mouse_up();
    }

    /// Two fingers spreading `factor` times further apart about `center` in `pane`, and moving by `pan`.
    pub fn pinch(&mut self, pane: Pane, center: [f32; 2], factor: f32, pan: [f32; 2]) {
        match pane {
            Pane::Edit => self.editor.zoom_canvas(center, factor, pan),
            Pane::View => self.distance = (self.distance - 40.0 * (factor - 1.0)).clamp(NEAREST, FARTHEST),
            Pane::Map => {
                self.map_lens = (self.map_lens * factor).clamp(1.0, 6.0);
                self.zoom = self.map_lens.round();
            }
        }
    }

    /// A scroll of `dy` lines over `p` in `pane`.
    pub fn wheel(&mut self, pane: Pane, p: [f32; 2], dy: f32) {
        match pane {
            Pane::Edit => self.editor.wheel(p, dy),
            Pane::View => self.distance = (self.distance - dy * 6.0).clamp(NEAREST, FARTHEST),
            Pane::Map => {
                self.zoom = (self.zoom + dy.signum()).clamp(1.0, 6.0);
                self.map_lens = self.zoom;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::FsStore;

    #[test]
    fn the_view_hides_the_player_on_a_tap_and_comes_closer_without_them() {
        let dir = std::env::temp_dir().join(format!("studio-test-{}", std::process::id()));
        let mut s = Studio::new(Box::new(FsStore { dir: dir.clone(), catalog: dir.join("art.txt"), cards: dir.join("enemies") }), "BARREL");
        let [x, y, w, h] = PLAYER_SWITCH;
        s.press(Pane::View, [x + w / 2.0, y + h / 2.0], false, false);
        assert!(!s.player && s.held.is_none(), "a tap on the switch, not a drag");
        for _ in 0..40 {
            s.wheel(Pane::View, [500.0, 400.0], 1.0);
        }
        assert_eq!(s.distance, NEAREST);
        assert!(s.distance < 0.0, "past where the player would stand");
    }

    #[test]
    fn points_land_in_the_pane_under_them_wide_or_tall() {
        assert_eq!(Studio::locate([1536.0, 768.0], [512.0, 384.0]), (Pane::Edit, [512.0, 384.0]));
        assert_eq!(Studio::locate([1536.0, 768.0], [1280.0, 576.0]), (Pane::Map, [512.0, 384.0]));
        assert_eq!(Studio::locate([800.0, 900.0], [400.0, 300.0]), (Pane::Edit, [512.0, 384.0]));
        assert_eq!(Studio::locate([800.0, 900.0], [200.0, 750.0]), (Pane::View, [512.0, 384.0]));
        assert_eq!(Studio::locate([800.0, 900.0], [600.0, 750.0]), (Pane::Map, [512.0, 384.0]));
        assert_eq!(Studio::locate([1536.0, 768.0], [1600.0, 100.0]).0, Pane::View, "past the window's edge, the pane nearest");
    }
}
