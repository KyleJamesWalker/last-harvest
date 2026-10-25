//! One sprite placed on a quiet farm and drawn the way the game draws both screens, for the sprite editor.

use std::f32::consts::FRAC_PI_2;

use super::map::TILE;
use super::{Harvest, SCALE, VIEW_H, VIEW_W, view3d};
use crate::art::Art;
use crate::{Canvas, Color};

/// How a sprite stands in the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stand {
    /// Fixed to the map like a building or fence, extruded `depth` pixels deep.
    Prop { depth: f32 },
    /// Turning to face the camera like a character.
    Figure,
    /// Lying flat on the ground like a tile or a stain.
    Ground,
}

/// The sprite on show, centered at `pos` with its feet on the ground.
pub(super) struct Shown {
    pub frames: Vec<Art>,
    pub stand: Stand,
    pub fps: f32,
    pub pos: [f32; 2],
}

impl Shown {
    pub fn frame(&self, t: f32) -> Art {
        self.frames[(t * self.fps) as usize % self.frames.len()]
    }
}

pub struct Showcase {
    harvest: Harvest,
}

impl Showcase {
    pub fn new(seed: u64) -> Self {
        let mut harvest = Harvest::calm(seed);
        harvest.birds.clear();
        let pos = [harvest.map.spawn[0], harvest.map.spawn[1] - 3.0 * TILE];
        harvest.shown = Some(Shown { frames: Vec::new(), stand: Stand::Figure, fps: 6.0, pos });
        Self { harvest }
    }

    /// Shows `frames`, animated at `fps`, standing as `stand`.
    pub fn show(&mut self, frames: Vec<Art>, stand: Stand, fps: f32) {
        if let Some(s) = &mut self.harvest.shown {
            s.frames = frames;
            s.stand = stand;
            s.fps = fps;
        }
    }

    /// Moves the clock on and walks the player around the sprite to look at it from `yaw`, `distance` map pixels away.
    pub fn update(&mut self, dt: f32, yaw: f32, distance: f32) {
        let h = &mut self.harvest;
        h.t += dt;
        let Some(s) = &h.shown else { return };
        let (forward, right) = ([yaw.cos(), yaw.sin()], [-yaw.sin(), yaw.cos()]);
        // A step to the side keeps the player from standing between the camera and the sprite.
        h.pos = [s.pos[0] - forward[0] * distance - right[0] * 14.0, s.pos[1] - forward[1] * distance - right[1] * 14.0];
        h.view_yaw = yaw;
        h.aim = forward;
    }

    /// The 2.5D view, as on the top screen.
    pub fn top(&self) -> Canvas {
        let scene = view3d::scene(&self.harvest);
        Canvas { clear: [scene.fog[0], scene.fog[1], scene.fog[2], 1.0], cards: Some(scene), shapes: Vec::new() }
    }

    /// The map around the sprite, as on the bottom screen, magnified `zoom` times past the game's own scale.
    pub fn map(&self, zoom: f32) -> Canvas {
        let mut c = Canvas { clear: [0.0, 0.0, 0.0, 1.0], ..Default::default() };
        let h = &self.harvest;
        let Some(s) = &h.shown else { return c };
        let scale = SCALE * zoom;
        let cam = [s.pos[0] - VIEW_W / zoom / 2.0, s.pos[1] - 8.0 - VIEW_H / zoom / 2.0];
        let wash = view3d::sky(h).wash;
        let tint: Color = [0, 1, 2, 3].map(|i| if i == 3 { 1.0 } else { 1.0 - wash[3] + wash[3] * wash[i] });
        h.world(&mut c, cam, scale, tint);
        c
    }

    /// Default viewing angle: looking north, as a new run starts.
    pub fn yaw() -> f32 {
        -FRAC_PI_2
    }
}
