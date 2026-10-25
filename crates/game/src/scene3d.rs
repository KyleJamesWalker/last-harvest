//! The per-frame 3D scene a backend draws under the 2D shapes: textured quads from the art texture.

use glam::{Mat4, Vec3};

/// Short-lived lights on top of the lantern, such as muzzle flashes and explosions.
pub const MAX_FLASHES: usize = 4;

/// Textured quads from the art texture, lit by a lantern and a few flashes, and faded into fog.
#[derive(Clone, Debug)]
pub struct CardScene {
    pub view_proj: Mat4,
    pub eye: Vec3,
    pub light: Vec3,
    pub light_color: [f32; 3],
    /// How far the light reaches, in world units.
    pub light_range: f32,
    pub flashes: Vec<Flash>,
    pub ambient: [f32; 3],
    pub fog: [f32; 3],
    /// Distances from the eye where fog starts and where it is total.
    pub fog_range: [f32; 2],
    pub cards: Vec<Card>,
}

#[derive(Clone, Copy, Debug)]
pub struct Flash {
    pub pos: Vec3,
    pub color: [f32; 3],
    pub range: f32,
}

/// One quad. Corners run top-left, top-right, bottom-right, bottom-left, matching `uv` (u0, v0, u1, v1).
#[derive(Clone, Copy, Debug)]
pub struct Card {
    pub corners: [Vec3; 4],
    pub uv: [f32; 4],
    pub normal: Vec3,
    /// Multiplies the texture color.
    pub tint: [f32; 3],
    /// 0 is lit and fogged like everything else; 1 shines at full color through the dark.
    pub glow: f32,
}

impl Card {
    pub fn new(corners: [Vec3; 4], uv: [f32; 4], normal: Vec3) -> Self {
        Self { corners, uv, normal, tint: [1.0; 3], glow: 0.0 }
    }
}
