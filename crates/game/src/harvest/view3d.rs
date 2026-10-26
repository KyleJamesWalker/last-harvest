//! The 2.5D view: the same farm, with every sprite's silhouette extruded into a lantern-lit, fogged 3D scene.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use glam::{Mat4, Vec3};

use super::carve;
use super::map::TILE;
use crate::art::Stand;
use super::weapons::{BOLT_LIFE, Hazard};
use super::{DAWN, DropKind, Facing, GRENADE_FLIGHT, GRENADE_RADIUS, Harvest, Phase, TELL_TIME, TURN_RISE, TURNED, Thrown, WALK_FPS, Zombie, len, norm, player_pose, sub};
use crate::art::{self, ATLAS_H, ATLAS_W, Art, PIXELS, RECTS};
use crate::scene3d::{Card, CardScene, Flash};
use crate::{SCREEN_H, SCREEN_W};

/// World units are tiles; sprite pixels are this big.
const PX: f32 = 1.0 / TILE;
const LEADER_TINT: [f32; 3] = [1.6, 1.3, 0.6];
const VIEW_RADIUS: f32 = 16.0;
const CHARACTER_DEPTH: f32 = 3.0 * PX;
const DECAL_LIFT: f32 = 0.03;
const MAX_ZOMBIES_3D: usize = 160;

/// One face of an extruded sprite in sprite space: x right and y down in pixels, z from front (0) to back (1).
/// The normal is in (right, up, back).
#[derive(Clone, Copy)]
struct Face {
    corners: [[f32; 3]; 4],
    uv: [f32; 4],
    normal: [f32; 3],
}

/// Front and back faces, then the side faces along every edge of the silhouette.
fn faces(art: Art) -> Cow<'static, [Face]> {
    static ALL: OnceLock<Vec<Vec<Face>>> = OnceLock::new();
    if art.is_live() {
        return Cow::Owned(art::with_pixels(art, |pixels| extrude(pixels, art.rect())));
    }
    Cow::Borrowed(&ALL.get_or_init(|| (0..RECTS.len()).map(|i| extrude(PIXELS, Art(i as u16).rect())).collect())[art.0 as usize])
}

fn extrude(pixels: &[u8], rect: [u16; 4]) -> Vec<Face> {
    let [rx, ry, w, h] = rect.map(|v| v as i32);
    let opaque = |x: i32, y: i32| x >= 0 && y >= 0 && x < w && y < h && pixels[(((ry + y) * ATLAS_W as i32 + rx + x) * 4 + 3) as usize] > 0;
    let (aw, ah) = (ATLAS_W as f32, ATLAS_H as f32);
    let u = |x: i32| (rx + x) as f32 / aw;
    let v = |y: i32| (ry + y) as f32 / ah;
    let (wf, hf) = (w as f32, h as f32);
    let full = [u(0), v(0), u(w), v(h)];
    let mut out = vec![
        Face { corners: [[0.0, 0.0, 0.0], [wf, 0.0, 0.0], [wf, hf, 0.0], [0.0, hf, 0.0]], uv: full, normal: [0.0, 0.0, -1.0] },
        Face { corners: [[0.0, 0.0, 1.0], [wf, 0.0, 1.0], [wf, hf, 1.0], [0.0, hf, 1.0]], uv: full, normal: [0.0, 0.0, 1.0] },
    ];
    // Runs of edge pixels become one face each, textured with the pixels just inside the outline
    // stretched along the depth, so a wall's side shows its planks rather than its black border.
    let dark = |x: i32, y: i32| {
        let i = (((ry + y) * ATLAS_W as i32 + rx + x) * 4) as usize;
        pixels[i] as u32 + pixels[i + 1] as u32 + pixels[i + 2] as u32 <= 40
    };
    let inside = |x: i32, y: i32, dx: i32, dy: i32| {
        (0..4).map(|k| (x + dx * k, y + dy * k)).find(|&(ix, iy)| opaque(ix, iy) && !dark(ix, iy)).unwrap_or((x, y))
    };
    for x in 0..w {
        for (side, at, nx) in [(-1, x, -1.0), (1, x + 1, 1.0)] {
            let edge = |y: i32| opaque(x, y) && !opaque(x + side, y);
            let mut y = 0;
            while y < h {
                if !edge(y) {
                    y += 1;
                    continue;
                }
                let (y0, col) = (y, inside(x, y, -side, 0).0);
                while y < h && edge(y) && inside(x, y, -side, 0).0 == col {
                    y += 1;
                }
                let (xf, uc) = (at as f32, (rx as f32 + col as f32 + 0.5) / aw);
                out.push(Face {
                    corners: [[xf, y0 as f32, 0.0], [xf, y0 as f32, 1.0], [xf, y as f32, 1.0], [xf, y as f32, 0.0]],
                    uv: [uc, v(y0), uc, v(y)],
                    normal: [nx, 0.0, 0.0],
                });
            }
        }
    }
    for y in 0..h {
        for (side, at, ny) in [(-1, y, 1.0), (1, y + 1, -1.0)] {
            let edge = |x: i32| opaque(x, y) && !opaque(x, y + side);
            let mut x = 0;
            while x < w {
                if !edge(x) {
                    x += 1;
                    continue;
                }
                let (x0, row) = (x, inside(x, y, 0, -side).1);
                while x < w && edge(x) && inside(x, y, 0, -side).1 == row {
                    x += 1;
                }
                let (yf, vc) = (at as f32, (ry as f32 + row as f32 + 0.5) / ah);
                out.push(Face {
                    corners: [[x0 as f32, yf, 1.0], [x as f32, yf, 1.0], [x as f32, yf, 0.0], [x0 as f32, yf, 0.0]],
                    uv: [u(x0), vc, u(x), vc],
                    normal: [0.0, ny, 0.0],
                });
            }
        }
    }
    out
}

/// Where a standing sprite goes: `origin` is its front bottom-left corner, `right` runs along its width,
/// `up` along its height and `back` into its depth, `depth` world units deep. Scaling `right` or `up` stretches it.
struct Pose {
    origin: Vec3,
    right: Vec3,
    up: Vec3,
    back: Vec3,
    depth: f32,
    tint: [f32; 3],
}

impl Pose {
    fn upright(origin: Vec3, right: Vec3, back: Vec3, depth: f32) -> Self {
        Self { origin, right, up: Vec3::Y, back, depth, tint: [1.0; 3] }
    }
}

/// Places a sprite standing on the ground; only its front and back faces when `solid` is false.
/// Faces turned away from `eye` are left out: every extrusion is closed, so they would be hidden anyway,
/// and alpha-tested fragments cost full price on the device's GPU.
fn stand(out: &mut Vec<Card>, eye: Vec3, art: Art, flip: bool, pose: Pose, solid: bool) {
    let Pose { origin, right, up, back, depth, tint } = pose;
    let (w, h) = (art.w(), art.h());
    let all = faces(art);
    let used = if solid { &all[..] } else { &all[..2] };
    let (rn, un) = (right.normalize_or_zero(), up.normalize_or_zero());
    for f in used {
        let place = |[x, y, z]: [f32; 3]| {
            let x = if flip { w - x } else { x };
            origin + right * (x * PX) + up * ((h - y) * PX) + back * (z * depth)
        };
        let n = f.normal;
        let normal = rn * if flip { -n[0] } else { n[0] } + un * n[1] + back * n[2];
        let corners = f.corners.map(place);
        if normal.dot(eye - corners[0]) > 0.0 {
            out.push(Card { tint, ..Card::new(corners, f.uv, normal) });
        }
    }
}

/// Puts a sprite fixed to the map, with its left edge at `left` and its bottom row meeting the ground at `base`,
/// both in map pixels. Figures turn to face the camera, so the caller draws those.
fn fixed(out: &mut Vec<Card>, eye: Vec3, a: Art, how: Stand, left: f32, base: f32) {
    match how {
        Stand::Prop { depth } => stand(out, eye, a, false, Pose::upright(Vec3::new(left * PX, 0.0, base * PX), Vec3::X, -Vec3::Z, depth * PX), true),
        // The sprite is the roof, its top row to the north, and its edges run down to the ground.
        Stand::Slab { height } => {
            let pose = Pose { origin: Vec3::new(left * PX, height * PX, base * PX), right: Vec3::X, up: -Vec3::Z, back: -Vec3::Y, depth: height * PX, tint: [1.0; 3] };
            stand(out, eye, a, false, pose, true);
        }
        Stand::Ground => flat(out, a, left * PX, (base - a.h()) * PX, DECAL_LIFT + 0.006, [1.0; 3]),
        Stand::Carve { footprint, south, east } => carved(out, eye, [a, footprint.unwrap_or(a), south, east], footprint.is_some(), Vec3::new(left * PX, 0.0, (base - a.h()) * PX)),
        Stand::Figure => {}
    }
}

/// The carved solid for a sprite and its views, worked out once for packed sprites and every time for live ones.
fn carving([top, footprint, south, east]: [Art; 4], own_footprint: bool) -> Cow<'static, [carve::Quad]> {
    type Cache = Mutex<HashMap<[Art; 4], &'static [carve::Quad]>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let views = carve::Views { top: top.rect(), footprint: own_footprint.then(|| footprint.rect()), south: south.rect(), east: east.rect() };
    if top.is_live() {
        return Cow::Owned(art::with_pixels(top, |px| carve::carve(px, ATLAS_W as usize, &views)));
    }
    let mut cache = CACHE.get_or_init(Default::default).lock().unwrap();
    Cow::Borrowed(*cache.entry([top, footprint, south, east]).or_insert_with(|| carve::carve(PIXELS, ATLAS_W as usize, &views).leak()))
}

/// Places a carved solid with its north-west ground corner at `origin`: `arts` are the sprite, the footprint
/// (the sprite again unless `own_footprint`), and the south and east views.
fn carved(out: &mut Vec<Card>, eye: Vec3, arts: [Art; 4], own_footprint: bool, origin: Vec3) {
    let (aw, ah) = (ATLAS_W as f32, ATLAS_H as f32);
    for q in carving(arts, own_footprint).iter() {
        let corners = q.corners.map(|[x, y, z]| origin + Vec3::new(x, y, z) * PX);
        let normal = Vec3::from(q.normal);
        if normal.dot(eye - corners[0]) > 0.0 {
            let [u0, v0, u1, v1] = q.uv;
            out.push(Card::new(corners, [u0 / aw, v0 / ah, u1 / aw, v1 / ah], normal));
        }
    }
}

/// Lays a sprite flat on the ground with its top-left at (x, z), lifted by `lift`.
fn flat(out: &mut Vec<Card>, art: Art, x: f32, z: f32, lift: f32, tint: [f32; 3]) {
    let [rx, ry, w, h] = art.rect().map(f32::from);
    let (aw, ah) = (ATLAS_W as f32, ATLAS_H as f32);
    let (x1, z1) = (x + w * PX, z + h * PX);
    let corners = [Vec3::new(x, lift, z), Vec3::new(x1, lift, z), Vec3::new(x1, lift, z1), Vec3::new(x, lift, z1)];
    out.push(Card { tint, ..Card::new(corners, [rx / aw, ry / ah, (rx + w) / aw, (ry + h) / ah], Vec3::Y) });
}

/// A solid square of color facing the camera, `size` world units across, centered on `at`.
fn speck(out: &mut Vec<Card>, at: Vec3, size: f32, right: Vec3, color: [f32; 3], glow: f32) {
    let (r, u) = (right * (size / 2.0), Vec3::Y * (size / 2.0));
    let [rx, ry, w, h] = art::WHITE.rect().map(f32::from);
    let (aw, ah) = (ATLAS_W as f32, ATLAS_H as f32);
    // The middle of the white square, so filtering never reaches its edge.
    let uv = [(rx + w * 0.25) / aw, (ry + h * 0.25) / ah, (rx + w * 0.75) / aw, (ry + h * 0.75) / ah];
    let corners = [at - r + u, at + r + u, at + r - u, at - r - u];
    out.push(Card { tint: color, glow, ..Card::new(corners, uv, right.cross(Vec3::Y)) });
}

/// A flat square of color on the ground.
fn patch(out: &mut Vec<Card>, at: Vec3, size: f32, color: [f32; 3]) {
    let [rx, ry, w, h] = art::WHITE.rect().map(f32::from);
    let (aw, ah) = (ATLAS_W as f32, ATLAS_H as f32);
    let uv = [(rx + w * 0.25) / aw, (ry + h * 0.25) / ah, (rx + w * 0.75) / aw, (ry + h * 0.75) / ah];
    let s = size / 2.0;
    let corners = [at + Vec3::new(-s, 0.0, -s), at + Vec3::new(s, 0.0, -s), at + Vec3::new(s, 0.0, s), at + Vec3::new(-s, 0.0, s)];
    out.push(Card { tint: color, ..Card::new(corners, uv, Vec3::Y) });
}

fn world(p: [f32; 2]) -> Vec3 {
    Vec3::new(p[0] * PX, 0.0, p[1] * PX)
}

/// Which frames show a character moving along `dir` (map space), seen by a camera looking along `forward`.
fn facing_from(dir: [f32; 2], forward: Vec3, right: Vec3) -> (Facing, bool) {
    let (a, b) = (dir[0] * forward.x + dir[1] * forward.z, dir[0] * right.x + dir[1] * right.z);
    if a.abs() >= b.abs() {
        (if a > 0.0 { Facing::Back } else { Facing::Front }, false)
    } else {
        (Facing::Side, b < 0.0)
    }
}

/// The colors of the night, warming toward dawn over the last three minutes.
pub struct Sky {
    pub fog: [f32; 3],
    pub ambient: [f32; 3],
    pub lantern: [f32; 3],
    /// Laid over the bottom screen's map.
    pub wash: [f32; 4],
}

pub fn sky(h: &Harvest) -> Sky {
    let won = matches!(h.phase, Phase::Over { won: true, .. });
    let k = if won { 1.0 } else { ((h.t - (DAWN - 180.0)) / 180.0).clamp(0.0, 1.0) };
    let k = k * k * (3.0 - 2.0 * k);
    let mix = |a: [f32; 3], b: [f32; 3]| [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * k);
    let night = 1.0 - k;
    Sky {
        fog: mix([0.07, 0.08, 0.12], [0.45, 0.32, 0.3]),
        ambient: mix([0.18, 0.19, 0.27], [0.62, 0.52, 0.48]),
        lantern: mix([1.0, 0.86, 0.66], [0.6, 0.5, 0.4]),
        wash: [0.05 + 0.9 * k, 0.07 + 0.5 * k, 0.2 + 0.1 * k, 0.22 * night + 0.1 * k],
    }
}

/// A ring of glowing specks on the ground.
fn patch_ring(out: &mut Vec<Card>, center: Vec3, radius: f32, color: [f32; 3], right: Vec3, near: impl Fn(Vec3) -> bool) {
    for i in 0..16 {
        let a = i as f32 / 16.0 * std::f32::consts::TAU;
        let q = center + Vec3::new(a.cos() * radius, 0.05, a.sin() * radius);
        if near(q) {
            speck(out, q, 0.06, right, color, 1.0);
        }
    }
}

/// Where a point in the world lands on the top screen, in screen pixels, and how far away it is; None behind the camera.
pub fn to_screen(view_proj: Mat4, at: [f32; 2], height: f32) -> Option<([f32; 2], f32)> {
    let q = world(at) + Vec3::Y * (height * PX);
    let c = view_proj * q.extend(1.0);
    if c.w < 0.3 {
        return None;
    }
    let (x, y) = (c.x / c.w, c.y / c.w);
    Some(([(x + 1.0) * 0.5 * SCREEN_W, (1.0 - y) * 0.5 * SCREEN_H], c.w))
}

/// A camera-facing sprite: a character, pickup or effect.
struct Board {
    art: Art,
    flip: bool,
    at: [f32; 2],
    lift: f32,
    depth: f32,
    /// Width and height multipliers.
    scale: [f32; 2],
    /// Tips the top of the sprite this way, in world units per unit of height.
    lean: Vec3,
    tint: [f32; 3],
}

impl Board {
    fn new(art: Art, flip: bool, at: [f32; 2], lift: f32, depth: f32) -> Self {
        Self { art, flip, at, lift, depth, scale: [1.0; 2], lean: Vec3::ZERO, tint: [1.0; 3] }
    }
}

pub fn scene(h: &Harvest) -> CardScene {
    let j = &h.juice;
    // After a death the camera swings around in front of the player and pulls back, to watch them turn.
    let swing = match h.phase {
        Phase::Over { t, won: false } => {
            let k = (t / 1.2).clamp(0.0, 1.0);
            k * k * (3.0 - 2.0 * k)
        }
        _ => 0.0,
    };
    let yaw = h.view_yaw + std::f32::consts::PI * swing;
    let forward = Vec3::new(yaw.cos(), 0.0, yaw.sin());
    let right = forward.cross(Vec3::Y);
    let p = world(h.pos);
    let [sx, sy, roll] = j.shake_offset();
    let roll = roll + h.lean * 0.035;
    let bob = if h.walk > 0.0 { (h.walk * WALK_FPS * std::f32::consts::PI / 2.0).sin().abs() * 0.04 } else { 0.0 };
    let shake = right * sx + Vec3::Y * (sy + bob);
    let eye = p - forward * (2.4 + 0.2 * swing) + Vec3::Y * (1.5 + 0.4 * swing) + shake;
    let looking = p + forward * 4.0 + Vec3::Y * (0.7 + j.kick * 6.0);
    let target = looking.lerp(p + Vec3::Y * 0.5, swing) + shake;
    let view = Mat4::look_at_rh(eye, target, Vec3::Y * roll.cos() + right * roll.sin());
    let proj = Mat4::perspective_rh_gl((66.0 + j.punch).to_radians(), SCREEN_W / SCREEN_H, 0.3, VIEW_RADIUS + 6.0);
    let near = |q: Vec3| {
        let d = q - p;
        d.length() < VIEW_RADIUS && d.dot(forward) > -4.0
    };
    let mut cards = Vec::new();

    let (cx, cz) = (p.x as i32, p.z as i32);
    let r = VIEW_RADIUS as i32;
    for ty in (cz - r).max(0)..(cz + r).min(h.map.h as i32) {
        for tx in (cx - r).max(0)..(cx + r).min(h.map.w as i32) {
            let corner = Vec3::new(tx as f32, 0.0, ty as f32);
            if !near(corner + Vec3::new(0.5, 0.0, 0.5)) {
                continue;
            }
            let (ground, overlay) = h.map.tile(tx as usize, ty as usize);
            flat(&mut cards, ground, corner.x, corner.z, 0.0, [1.0; 3]);
            if let Some(crop) = overlay {
                stand(&mut cards, eye, crop, false, Pose::upright(corner + Vec3::Z, Vec3::X, -Vec3::Z, 2.0 * PX), false);
            }
        }
    }
    for &(a, x, y) in &h.map.decals {
        let q = world([x, y]);
        if near(q) {
            flat(&mut cards, a, q.x, q.z, DECAL_LIFT, [1.0; 3]);
        }
    }
    for (i, &(a, x, y, tint)) in h.stains.iter().enumerate() {
        let q = world([x, y]);
        if near(q) {
            // Newer stains sit a hair higher, so overlapping ones don't flicker.
            flat(&mut cards, a, q.x, q.z, DECAL_LIFT + 0.002 + i as f32 * 0.00005, tint);
        }
    }
    // Anything standing behind the player would fill the screen, so only what is ahead gets drawn.
    let ahead = |q: Vec3| near(q) && (q - p).dot(forward) > 0.3;
    let mut figures = Vec::new();
    for prop in &h.map.props {
        let a = prop.frame(h.t);
        let how = a.stand().unwrap_or(Stand::Prop { depth: prop.depth });
        let bottom = prop.y + a.h();
        let middle = match how {
            Stand::Prop { depth } => Vec3::new((prop.x + a.w() * 0.5) * PX, 0.0, (prop.base - depth * 0.5) * PX),
            _ => Vec3::new((prop.x + a.w() * 0.5) * PX, 0.0, (bottom - a.h() * 0.5) * PX),
        };
        if !ahead(middle) {
            continue;
        }
        if how == Stand::Figure {
            figures.push((a, [prop.x + a.w() / 2.0, bottom]));
        } else {
            fixed(&mut cards, eye, a, how, prop.x, if matches!(how, Stand::Prop { .. }) { prop.base } else { bottom });
        }
    }
    if let Some(s) = h.shown.as_ref().filter(|s| !s.frames.is_empty() && s.stand != Stand::Figure) {
        let a = s.frame(h.t);
        fixed(&mut cards, eye, a, s.stand, s.pos[0] - a.w() / 2.0, s.pos[1]);
    }
    for b in &h.barrels {
        let a = if b.flash > 0.0 { art::BARREL_FLASH } else { art::BARREL };
        let base = Vec3::new(b.pos[0] * PX, 0.0, (b.pos[1] + a.h()) * PX);
        if ahead(base) {
            stand(&mut cards, eye, a, false, Pose::upright(base, Vec3::X, -Vec3::Z, 12.0 * PX), true);
        }
    }

    let board = |cards: &mut Vec<Card>, b: Board| {
        let q = world(b.at);
        // Close to the lens a character fills the screen without showing anything useful.
        if near(q) && (q - p).dot(forward) > -0.8 && (q - eye).length() > 1.4 {
            let r = right * b.scale[0];
            let origin = q - r * (b.art.w() * PX / 2.0) - forward * (b.depth / 2.0) + Vec3::Y * b.lift;
            let up = (Vec3::Y + b.lean) * b.scale[1];
            stand(cards, eye, b.art, b.flip, Pose { origin, right: r, up, back: forward, depth: b.depth, tint: b.tint }, b.depth > 0.0);
        }
    };
    // A full horde swarming the player would bury the GPU; the farthest are lost in the fog anyway.
    let mut nearest: Vec<&Zombie> = h.zombies.iter().collect();
    if nearest.len() > MAX_ZOMBIES_3D {
        nearest.select_nth_unstable_by(MAX_ZOMBIES_3D, |a, b| len(sub(a.pos, h.pos)).total_cmp(&len(sub(b.pos, h.pos))));
        nearest.truncate(MAX_ZOMBIES_3D);
    }
    for z in nearest {
        let (facing, flip) = facing_from(z.dir, forward, right);
        let a = z.art(facing);
        let s = z.squash;
        // A rising zombie starts below the ground plane, which hides the part still buried.
        let mut b = Board::new(a, flip, z.pos, -(1.0 - z.rise) * a.h() * PX, CHARACTER_DEPTH);
        if z.leader {
            b.tint = LEADER_TINT;
        }
        b.scale = [1.0 + 0.3 * s, 1.0 - 0.2 * s];
        b.lean = Vec3::new(z.knock[0], 0.0, z.knock[1]) * (0.35 * s);
        board(&mut cards, b);
    }
    let (facing, flip) = facing_from(h.aim, forward, right);
    let (a, rise) = player_pose(h, facing);
    board(&mut cards, Board::new(a, flip, h.pos, -(1.0 - rise) * a.h() * PX, CHARACTER_DEPTH));
    if let Some(s) = h.shown.as_ref().filter(|s| s.stand == Stand::Figure && !s.frames.is_empty()) {
        board(&mut cards, Board::new(s.frame(h.t), false, s.pos, 0.0, CHARACTER_DEPTH));
    }
    for (a, at) in figures {
        board(&mut cards, Board::new(a, false, at, 0.0, CHARACTER_DEPTH));
    }
    for d in &h.decoys {
        // It shudders harder as it gets ready to burst.
        let mut b = Board::new(art::SCARECROW, false, d.pos, 0.0, CHARACTER_DEPTH);
        let k = d.age / d.life;
        b.lean = right * ((d.age * (10.0 + 30.0 * k)).sin() * 0.08 * k);
        board(&mut cards, b);
    }
    for d in h.dogs.iter().filter(|d| (world(d.pos) - p).dot(forward) >= 0.0) {
        let flip = d.heading[0] * right.x + d.heading[1] * right.z < 0.0;
        board(&mut cards, Board::new(d.art(), flip, d.pos, 0.0, CHARACTER_DEPTH));
    }
    for (i, (p, flip)) in h.crows().into_iter().enumerate() {
        let a = art::CROW_FLY[(h.t * 12.0 + i as f32) as usize % art::CROW_FLY.len()];
        board(&mut cards, Board::new(a, flip, p, 12.0 * PX, 0.0));
    }
    for s in &h.swings {
        let (angle, r) = s.now();
        let at = [h.pos[0] + angle.cos() * r, h.pos[1] + angle.sin() * r];
        board(&mut cards, Board::new(s.art, angle.cos() * right.x + angle.sin() * right.z < 0.0, at, 4.0 * PX, 0.0));
    }
    for patch in &h.patches {
        let q = world(patch.pos);
        if !near(q) {
            continue;
        }
        match patch.hazard {
            // Underfoot behind the player they'd fill the bottom of the screen.
            Hazard::Tacks if (q - p).dot(forward) < 0.0 => {}
            Hazard::Tacks => {
                let a = art::TACKS;
                flat(&mut cards, a, q.x - a.w() * PX / 2.0, q.z - a.h() * PX / 2.0, DECAL_LIFT + 0.008, [1.0; 3]);
            }
            Hazard::Fire => {
                let a = art::FIRE[((h.t * 10.0 + patch.pos[0]) as usize) % art::FIRE.len()];
                let mut b = Board::new(a, false, patch.pos, 0.0, 0.0);
                b.tint = [1.6, 1.3, 1.0];
                board(&mut cards, b);
            }
            Hazard::Flare => {
                board(&mut cards, Board::new(art::FLARE, false, patch.pos, 0.0, 0.0));
                patch_ring(&mut cards, q, patch.radius * PX, [1.0, 0.3, 0.2], right, near);
            }
        }
    }
    for b in &h.bolts {
        let fade = 1.0 - b.age / BOLT_LIFE;
        for w in b.points.windows(2) {
            for k in 0..10 {
                let t = k as f32 / 10.0;
                let jitter = (k as f32 * 7.3 + b.age * 90.0).sin() * 2.0;
                let at = [w[0][0] + (w[1][0] - w[0][0]) * t + jitter, w[0][1] + (w[1][1] - w[0][1]) * t + 6.0];
                let q = world(at) + Vec3::Y * (0.5 + jitter * PX);
                if near(q) && (q - p).dot(forward) > 0.0 {
                    speck(&mut cards, q, 0.07 * fade, right, [0.7, 0.85, 1.0], 1.0);
                }
            }
        }
    }
    for d in &h.drops {
        let a = match d.kind {
            DropKind::Money(_) => art::MONEY[3],
            DropKind::Health => art::HEALTH,
            DropKind::Chest => art::CHEST,
        };
        let mut b = Board::new(a, false, d.pos, 0.1 + d.z * PX + (h.t * 4.0 + d.pos[0]).sin().abs() * 0.1, 0.0);
        // Coins spin; never quite edge-on, or they vanish.
        if d.kind != DropKind::Chest {
            b.scale[0] = (h.t * 5.0 + d.pos[1]).cos().abs().max(0.2);
        }
        board(&mut cards, b);
    }
    for c in &h.juice.corpses {
        let q = world(c.pos);
        if !near(q) || (q - p).dot(forward) < 0.0 {
            continue;
        }
        let fall = Vec3::new(c.fall[0], 0.0, c.fall[1]).normalize_or(forward);
        let side = fall.cross(Vec3::Y);
        // Slumped short of flat, or the low camera would see the body edge-on.
        let tip = if c.z > 0.0 { c.spin } else { c.tipped() * 1.2 };
        let up = Vec3::Y * tip.cos() + fall * tip.sin();
        let back = up.cross(side);
        let lift = c.z * PX + 0.04 - c.sunk() * PX;
        let origin = q - side * (c.art.w() * PX / 2.0) + Vec3::Y * lift;
        stand(&mut cards, eye, c.art, c.flip, Pose { origin, right: side, up, back, depth: PX, tint: [0.8; 3] }, false);
    }
    for b in &h.birds {
        let mut board_b = Board::new(b.art(), b.flip, b.pos, b.z * PX, 0.0);
        board_b.tint = [0.7; 3];
        board(&mut cards, board_b);
    }
    for f in &h.fx {
        let a = f.frames[((f.t * f.fps) as usize).min(f.frames.len() - 1)];
        board(&mut cards, Board::new(a, false, [f.pos[0], f.pos[1] + 6.0], 0.25, 0.0));
    }
    for g in &h.thrown {
        let (at, lift) = g.at();
        board(&mut cards, Board::new(g.art(), false, at, lift * PX, 0.0));
        if g.kind != Thrown::Grenade {
            continue;
        }
        // A shadow that grows as the grenade comes down, and a ring of red marks where it will land.
        let k = (g.t / GRENADE_FLIGHT).min(1.0);
        patch(&mut cards, world(at) + Vec3::Y * (DECAL_LIFT + 0.01), 0.15 + 0.15 * k, [0.02; 3]);
        let spin = g.t * 3.0;
        for i in 0..10 {
            let a = spin + i as f32 / 10.0 * std::f32::consts::TAU;
            let rr = GRENADE_RADIUS * PX * (1.15 - 0.15 * k);
            let q = world(g.to) + Vec3::new(a.cos() * rr, 0.06, a.sin() * rr);
            if near(q) {
                speck(&mut cards, q, 0.08, right, [1.0, 0.25, 0.15], 1.0);
            }
        }
    }
    for b in &h.bullets {
        board(&mut cards, Board::new(art::MUZZLE, false, [b.pos[0], b.pos[1] + 6.0], 0.35, 0.0));
        // A short glowing tracer behind each bullet.
        let n = norm(b.vel);
        for k in 1..4 {
            let at = [b.pos[0] - n[0] * k as f32 * 3.0, b.pos[1] + 6.0 - n[1] * k as f32 * 3.0];
            let q = world(at) + Vec3::Y * 0.42;
            if near(q) && (q - p).dot(forward) > 0.0 {
                speck(&mut cards, q, 0.07 - 0.015 * k as f32, right, [1.0, 0.8, 0.4], 1.0);
            }
        }
    }
    for t in &h.tells {
        // Churned earth, darkening as the zombie nears the surface.
        let k = (t.t / TELL_TIME).min(1.0);
        let mark = art::BLOOD_STAIN[(t.pos[0] as usize) % art::BLOOD_STAIN.len()];
        let q = world([t.pos[0] - mark.w() / 2.0, t.pos[1] - mark.h() / 2.0]);
        if near(q) {
            flat(&mut cards, mark, q.x, q.z, DECAL_LIFT + 0.004, [0.45 - 0.2 * k, 0.32 - 0.15 * k, 0.2 - 0.1 * k]);
        }
    }
    for pt in &h.juice.particles {
        let q = world(pt.pos) + Vec3::Y * (pt.z * PX);
        if near(q) && (q - p).dot(forward) > 0.0 {
            speck(&mut cards, q, pt.size_now() * PX, right, pt.color, pt.glow);
        }
    }
    for ring in &h.juice.rings {
        let rr = ring.now() * PX;
        let size = 0.18 * (1.0 - ring.age / ring.life);
        let n = 36;
        for i in 0..n {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            let q = world(ring.center) + Vec3::new(a.cos() * rr, 0.08, a.sin() * rr);
            if near(q) {
                speck(&mut cards, q, size, right, ring.color, 1.0);
            }
        }
    }

    let flashes = h
        .juice
        .flashes
        .iter()
        .map(|f| Flash { pos: world(f.pos) + Vec3::Y * (f.z * PX), color: f.color.map(|c| c * f.strength()), range: f.range * (0.5 + 0.5 * f.strength()) })
        .collect();
    let health = h.hp / h.kit.max_hp();
    let dying = matches!(h.phase, Phase::Over { won: false, .. });
    // Once the player has climbed back out, a sickly light comes up on what they've become.
    let turned = match h.phase {
        Phase::Over { t, won: false } => ((t - TURN_RISE) / (TURNED - TURN_RISE)).clamp(0.0, 1.0),
        _ => 0.0,
    };
    // The lantern gutters when you're nearly done for.
    let gutter = if health < 0.3 { 0.15 * (h.juice.clock * 23.0).sin().abs() * (h.juice.clock * 7.0).sin().abs() } else { 0.0 };
    let lantern = if dying { 3.0 + 4.0 * turned } else { 9.0 * h.kit.mods.view + 6.0 * h.beam } * (1.0 - 0.04 * (h.t * 13.0).sin().abs() - gutter);
    let sky = sky(h);
    let hurt = h.juice.hurt / 0.35;
    let light_color = [sky.lantern[0], sky.lantern[1] * (1.0 - 0.4 * hurt), sky.lantern[2] * (1.0 - 0.5 * hurt)];
    let sickly = [0.55, 1.0, 0.45];
    let light_color = [0, 1, 2].map(|i| light_color[i] + (sickly[i] - light_color[i]) * turned);
    CardScene {
        view_proj: proj * view,
        eye,
        light: p + Vec3::Y * 1.4 + forward * 0.3,
        light_color,
        light_range: lantern,
        flashes,
        ambient: sky.ambient,
        // Color drains away as the player nears death, and is gone once they fall.
        saturation: if dying { 0.15 + 0.45 * turned } else { 0.35 + 0.65 * (health / 0.3).min(1.0) },
        fog: sky.fog,
        fog_range: [6.0, VIEW_RADIUS],
        cards,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sprite_extrudes_to_a_closed_front_and_back() {
        for (name, frames) in art::ALL {
            for &a in *frames {
                let f = faces(a);
                assert!(f.len() >= 2, "{name}");
                assert!(f.iter().all(|f| f.uv.iter().all(|v| (0.0..=1.0).contains(v))), "{name}: uv out of range");
            }
        }
    }

    #[test]
    fn a_view_stays_within_a_device_budget() {
        let mut worst = 0;
        for seed in 1..20 {
            let mut h = Harvest::new(seed);
            for yaw in 0..8 {
                h.view_yaw = yaw as f32 * std::f32::consts::FRAC_PI_4;
                worst = worst.max(scene(&h).cards.len());
            }
        }
        eprintln!("most cards in one view: {worst}");
        assert!(worst < 12_000, "{worst} cards");
    }

    #[test]
    fn a_swarm_with_every_effect_going_stays_within_budget() {
        use crate::harvest::juice::{BLOOD, Corpse};
        use crate::harvest::{Kind, Zombie};
        let mut h = Harvest::calm(7);
        for i in 0..300 {
            let a = i as f32 * 0.37;
            let r = 12.0 + (i % 40) as f32 * 3.0;
            h.zombies.push(Zombie::new(Kind::Skinny, [h.pos[0] + a.cos() * r, h.pos[1] + a.sin() * r], 10.0, 1.0, 0.0));
        }
        for i in 0..60 {
            let a = i as f32 * 0.5;
            let at = [h.pos[0] + a.cos() * 40.0, h.pos[1] + a.sin() * 40.0];
            h.juice.corpse(Corpse { art: art::SKINNY_HURT[0], flip: false, pos: at, z: 0.0, vel: [0.0; 2], vz: 0.0, fall: [1.0, 0.0], age: 1.0, spin: 0.0 });
            h.juice.burst(at, 8.0, 12, [0.0; 2], 0.0, [10.0, 40.0], BLOOD, 1.5, 1.0, 0.0);
        }
        h.juice.ring(h.pos, 80.0, 1.0, BLOOD);
        h.juice.ring(h.pos, 40.0, 1.0, BLOOD);
        let mut worst = 0;
        for yaw in 0..8 {
            h.view_yaw = yaw as f32 * std::f32::consts::FRAC_PI_4;
            worst = worst.max(scene(&h).cards.len());
        }
        eprintln!("most cards with everything going: {worst}");
        assert!(worst < 12_000, "{worst} cards");
    }
}
