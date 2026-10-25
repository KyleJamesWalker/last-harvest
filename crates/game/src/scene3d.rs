//! The per-frame 3D scene a backend draws under the 2D shapes: textured quads from the art texture.

use glam::{Mat4, Vec3};

/// Textured quads from the art texture, lit by one point light and faded into fog.
#[derive(Clone, Debug)]
pub struct CardScene {
    pub view_proj: Mat4,
    pub eye: Vec3,
    pub light: Vec3,
    /// How far the light reaches, in world units.
    pub light_range: f32,
    pub ambient: [f32; 3],
    pub fog: [f32; 3],
    /// Distances from the eye where fog starts and where it is total.
    pub fog_range: [f32; 2],
    pub cards: Vec<Card>,
}

/// One quad. Corners run top-left, top-right, bottom-right, bottom-left, matching `uv` (u0, v0, u1, v1).
#[derive(Clone, Copy, Debug)]
pub struct Card {
    pub corners: [Vec3; 4],
    pub uv: [f32; 4],
    pub normal: Vec3,
}
