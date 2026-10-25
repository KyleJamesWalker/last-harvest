//! The 2.5D view: the same farm, with every sprite's silhouette extruded into a lantern-lit, fogged 3D scene.

use std::sync::OnceLock;

use glam::{Mat4, Vec3};

use super::map::TILE;
use super::{DropKind, Facing, Harvest, PLAYER, Phase, art_for};
use crate::art::{self, ATLAS_H, ATLAS_W, Art, PIXELS, RECTS};
use crate::scene3d::{Card, CardScene};
use crate::{SCREEN_H, SCREEN_W};

/// World units are tiles; sprite pixels are this big.
const PX: f32 = 1.0 / TILE;
const VIEW_RADIUS: f32 = 16.0;
const CHARACTER_DEPTH: f32 = 3.0 * PX;
const DECAL_LIFT: f32 = 0.03;

/// One face of an extruded sprite in sprite space: x right and y down in pixels, z from front (0) to back (1).
/// The normal is in (right, up, back).
#[derive(Clone, Copy)]
struct Face {
    corners: [[f32; 3]; 4],
    uv: [f32; 4],
    normal: [f32; 3],
}

/// Front and back faces, then the side faces along every edge of the silhouette.
fn faces(art: Art) -> &'static [Face] {
    static ALL: OnceLock<Vec<Vec<Face>>> = OnceLock::new();
    &ALL.get_or_init(|| (0..RECTS.len()).map(|i| extrude(Art(i as u16))).collect())[art.0 as usize]
}

fn extrude(art: Art) -> Vec<Face> {
    let [rx, ry, w, h] = art.rect().map(|v| v as i32);
    let opaque = |x: i32, y: i32| x >= 0 && y >= 0 && x < w && y < h && PIXELS[(((ry + y) * ATLAS_W as i32 + rx + x) * 4 + 3) as usize] > 0;
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
        PIXELS[i] as u32 + PIXELS[i + 1] as u32 + PIXELS[i + 2] as u32 <= 40
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
            let edge = |x: i32| opaque(x, y) && !opaque(x, y + side) && (side < 0 || y + 1 < h);
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

/// Where a standing sprite goes: `origin` is its front bottom-left corner, `right` runs along its width
/// and `back` into its depth, `depth` world units deep.
struct Pose {
    origin: Vec3,
    right: Vec3,
    back: Vec3,
    depth: f32,
}

/// Places a sprite standing on the ground; only its front and back faces when `solid` is false.
fn stand(out: &mut Vec<Card>, art: Art, flip: bool, pose: Pose, solid: bool) {
    let Pose { origin, right, back, depth } = pose;
    let (w, h) = (art.w(), art.h());
    let all = faces(art);
    let used = if solid { all } else { &all[..2] };
    for f in used {
        let place = |[x, y, z]: [f32; 3]| {
            let x = if flip { w - x } else { x };
            origin + right * (x * PX) + Vec3::Y * ((h - y) * PX) + back * (z * depth)
        };
        let n = f.normal;
        let normal = right * if flip { -n[0] } else { n[0] } + Vec3::Y * n[1] + back * n[2];
        out.push(Card { corners: f.corners.map(place), uv: f.uv, normal });
    }
}

/// Lays a sprite flat on the ground with its top-left at (x, z), lifted by `lift`.
fn flat(out: &mut Vec<Card>, art: Art, x: f32, z: f32, lift: f32) {
    let [rx, ry, w, h] = art.rect().map(f32::from);
    let (aw, ah) = (ATLAS_W as f32, ATLAS_H as f32);
    let (x1, z1) = (x + w * PX, z + h * PX);
    let corners = [Vec3::new(x, lift, z), Vec3::new(x1, lift, z), Vec3::new(x1, lift, z1), Vec3::new(x, lift, z1)];
    out.push(Card { corners, uv: [rx / aw, ry / ah, (rx + w) / aw, (ry + h) / ah], normal: Vec3::Y });
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

pub fn scene(h: &Harvest) -> CardScene {
    let forward = Vec3::new(h.view_yaw.cos(), 0.0, h.view_yaw.sin());
    let right = forward.cross(Vec3::Y);
    let p = world(h.pos);
    let eye = p - forward * 2.4 + Vec3::Y * 1.5;
    let target = p + forward * 4.0 + Vec3::Y * 0.7;
    let view = Mat4::look_at_rh(eye, target, Vec3::Y);
    let proj = Mat4::perspective_rh_gl(66f32.to_radians(), SCREEN_W / SCREEN_H, 0.3, VIEW_RADIUS + 6.0);
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
            flat(&mut cards, ground, corner.x, corner.z, 0.0);
            if let Some(crop) = overlay {
                stand(&mut cards, crop, false, Pose { origin: corner + Vec3::Z, right: Vec3::X, back: -Vec3::Z, depth: 2.0 * PX }, false);
            }
        }
    }
    for &(a, x, y) in h.map.decals.iter().chain(&h.stains) {
        let q = world([x, y]);
        if near(q) {
            flat(&mut cards, a, q.x, q.z, DECAL_LIFT);
        }
    }
    // Anything standing behind the player would fill the screen, so only what is ahead gets drawn.
    let ahead = |q: Vec3| near(q) && (q - p).dot(forward) > 0.3;
    for prop in &h.map.props {
        let base = Vec3::new(prop.x * PX, 0.0, prop.base * PX);
        let middle = base + Vec3::new(prop.frames[0].w() * PX * 0.5, 0.0, -prop.depth * PX * 0.5);
        if ahead(middle) {
            stand(&mut cards, prop.frame(h.t), false, Pose { origin: base, right: Vec3::X, back: -Vec3::Z, depth: prop.depth * PX }, true);
        }
    }
    for b in &h.barrels {
        let a = if b.flash > 0.0 { art::BARREL_FLASH } else { art::BARREL };
        let base = Vec3::new(b.pos[0] * PX, 0.0, (b.pos[1] + a.h()) * PX);
        if ahead(base) {
            stand(&mut cards, a, false, Pose { origin: base, right: Vec3::X, back: -Vec3::Z, depth: 12.0 * PX }, true);
        }
    }

    // Characters and pickups turn to face the camera.
    let billboard = |cards: &mut Vec<Card>, a: Art, flip: bool, at: [f32; 2], lift: f32, depth: f32| {
        let q = world(at);
        if near(q) && (q - p).dot(forward) > -0.8 {
            let origin = q - right * (a.w() * PX / 2.0) - forward * (depth / 2.0) + Vec3::Y * lift;
            stand(cards, a, flip, Pose { origin, right, back: forward, depth }, depth > 0.0);
        }
    };
    for z in &h.zombies {
        let look = &z.kind.stats().look;
        let (facing, flip) = facing_from(z.dir, forward, right);
        let a = if z.flash > 0.0 { look.flash[facing.index()] } else { look.walk[facing.index()][z.frame()] };
        billboard(&mut cards, a, flip, z.pos, 0.0, CHARACTER_DEPTH);
    }
    let (facing, flip) = facing_from(h.aim, forward, right);
    billboard(&mut cards, art_for(h, &PLAYER, facing), flip, h.pos, 0.0, CHARACTER_DEPTH);
    for d in &h.drops {
        let a = match d.kind {
            DropKind::Money(_) => art::MONEY[3],
            DropKind::Health => art::HEALTH,
        };
        billboard(&mut cards, a, false, d.pos, 0.1 + (h.t * 4.0 + d.pos[0]).sin().abs() * 0.1, 0.0);
    }
    for f in &h.fx {
        let a = f.frames[((f.t * f.fps) as usize).min(f.frames.len() - 1)];
        billboard(&mut cards, a, false, [f.pos[0], f.pos[1] + 6.0], 0.25, 0.0);
    }
    for g in &h.thrown {
        let (at, lift) = g.at();
        billboard(&mut cards, art::GRENADE, false, at, lift * PX, 0.0);
    }
    for b in &h.bullets {
        billboard(&mut cards, art::MUZZLE, false, [b.pos[0], b.pos[1] + 6.0], 0.35, 0.0);
    }

    let dying = matches!(h.phase, Phase::Over { won: false, .. });
    let lantern = if dying { 3.0 } else { 9.0 } * (1.0 - 0.04 * (h.t * 13.0).sin().abs());
    CardScene {
        view_proj: proj * view,
        eye,
        light: p + Vec3::Y * 1.4 + forward * 0.3,
        light_range: lantern,
        ambient: [0.2, 0.2, 0.25],
        fog: [0.11, 0.11, 0.13],
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
}
