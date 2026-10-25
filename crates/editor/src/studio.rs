//! The editor and its two previews, as both the native window and the web page lay them out: the editing pane
//! on the left and the 2.5D view above the map on the right, each 4:3 like the device's screens.

use last_harvest_game::art::{self, ATLAS_W, PIXELS};
use last_harvest_game::{Align, Canvas, SCREEN_H, SCREEN_W, Showcase};

use crate::editor::Editor;
use crate::store::Store;

const SHOWCASE_SEED: u64 = 7;
/// Radians the 2.5D view turns per pane unit dragged.
const TURN: f32 = 0.005;

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
    /// The pane a press began in, which keeps the pointer until it lets go.
    held: Option<Pane>,
    /// How the previews' labels say to move them.
    pub hints: [&'static str; 2],
}

impl Studio {
    pub fn new(store: Box<dyn Store>, name: &str) -> Self {
        Self {
            editor: Editor::new(store, name),
            showcase: Showcase::new(SHOWCASE_SEED),
            atlas: PIXELS.to_vec(),
            yaw: Showcase::yaw(),
            distance: 48.0,
            zoom: 2.0,
            held: None,
            hints: ["drag to turn, scroll to move closer", "scroll to zoom"],
        }
    }

    /// Which pane a point falls in, for a window `size` wide and tall, and where in that pane's 1024x768 canvas.
    pub fn locate(size: [f32; 2], [x, y]: [f32; 2]) -> (Pane, [f32; 2]) {
        let [edit, _, _] = Self::panes(size);
        if x < edit[2] {
            (Pane::Edit, [x / edit[2] * SCREEN_W, y / edit[3] * SCREEN_H])
        } else {
            let (w, half) = (size[0] - edit[2], size[1] / 2.0);
            let pane = if y < half { Pane::View } else { Pane::Map };
            (pane, [(x - edit[2]) / w * SCREEN_W, (y % half) / half * SCREEN_H])
        }
    }

    /// The editing pane, the 2.5D view and the map as x, y, w, h from the top-left of a window `size` big,
    /// which should be twice as wide as it is tall.
    pub fn panes([w, h]: [f32; 2]) -> [[f32; 4]; 3] {
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
        self.showcase.show(self.editor.shown(), self.editor.stand(), self.editor.fps);
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
        for f in self.editor.surfaces() {
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
        let live = art::set_live(rects, self.atlas.clone());
        self.editor.set_live(live);
    }

    /// The editing pane, the 2.5D view and the map, in the order `panes` places them.
    pub fn canvases(&self) -> [Canvas; 3] {
        let mut edit = Canvas::default();
        self.editor.draw(&mut edit);
        let mut view = self.showcase.top();
        view.label(format!("2.5D view: {}", self.hints[0]), 16.0, 12.0, 30.0, [1.0, 1.0, 1.0, 0.85], Align::Left);
        let mut map = self.showcase.map(self.zoom);
        map.label(format!("Map at {}x the game's scale: {}", self.zoom, self.hints[1]), 16.0, 12.0, 30.0, [1.0, 1.0, 1.0, 0.85], Align::Left);
        [edit, view, map]
    }

    pub fn press(&mut self, pane: Pane, p: [f32; 2], right: bool, shift: bool) {
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

    /// A scroll of `dy` lines over `p` in `pane`.
    pub fn wheel(&mut self, pane: Pane, p: [f32; 2], dy: f32) {
        match pane {
            Pane::Edit => self.editor.wheel(p, dy),
            Pane::View => self.distance = (self.distance - dy * 6.0).clamp(24.0, 200.0),
            Pane::Map => self.zoom = (self.zoom + dy.signum()).clamp(1.0, 6.0),
        }
    }
}
