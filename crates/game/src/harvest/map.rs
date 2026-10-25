//! Procedural farm map: a ground tile per cell, solid cells for collision, and props drawn in depth order.

use crate::art::{self, Art};
use crate::rng::Rng;

pub const TILE: f32 = 16.0;
const N: u8 = 1;
const E: u8 = 2;
const S: u8 = 4;
const W: u8 = 8;

#[derive(Clone, Copy, Debug)]
pub struct Prop {
    pub frames: &'static [Art],
    pub fps: f32,
    /// Top-left in world pixels.
    pub x: f32,
    pub y: f32,
    /// World y where the prop meets the ground, for depth sorting.
    pub base: f32,
    /// How far it reaches back from `base`, in world pixels, when extruded for the 3D view.
    pub depth: f32,
}

impl Prop {
    pub fn frame(&self, t: f32) -> Art {
        self.frames[(t * self.fps) as usize % self.frames.len()]
    }
}

/// The pieces of one fence style: top row left, middle, right; side posts left, right; bottom row likewise.
struct Fence {
    top: &'static [Art; 3],
    side: &'static [Art; 2],
    bottom: &'static [Art; 3],
    /// Drawn on each side of the gate, if the style has gate posts.
    gate_post: Option<&'static [Art]>,
}

const RAIL: Fence = Fence { top: &art::RAIL_TOP, side: &art::RAIL_SIDE, bottom: &art::RAIL_BOTTOM, gate_post: None };
const PICKET: Fence = Fence { top: &art::PICKET_TOP, side: &art::PICKET_SIDE, bottom: &art::PICKET_BOTTOM, gate_post: Some(&art::SKULL_FENCE) };

#[derive(Clone, Copy, PartialEq, Eq)]
enum Cell {
    Open,
    Road,
    Path,
    Taken,
}

pub struct Map {
    pub w: usize,
    pub h: usize,
    ground: Vec<Art>,
    overlay: Vec<Option<Art>>,
    solid: Vec<bool>,
    cells: Vec<Cell>,
    pub props: Vec<Prop>,
    /// Flat art drawn on the ground under everything else: corpses and the like.
    pub decals: Vec<(Art, f32, f32)>,
    /// Explosive barrels, by top-left in world pixels; each blocks its cell until it blows.
    pub barrels: Vec<[f32; 2]>,
    /// Where the player starts, in world pixels.
    pub spawn: [f32; 2],
}

/// Buildings and vehicles reach back over their solid rows; small things are slabs.
fn thickness(art: Art, tw: usize, th: usize, open_rows: usize) -> f32 {
    if let Some(art::Stand::Prop { depth }) = art.stand() {
        depth
    } else if tw * th >= 4 {
        ((th - open_rows.min(th - 1)) as f32 * TILE).max(8.0)
    } else if [art::BUSH, art::STUMP, art::HAY, art::TANK, art::MAILBOX].contains(&art) || art::LOG.contains(&art) {
        12.0
    } else {
        3.0
    }
}

fn tiles(px: f32) -> usize {
    (px / TILE).ceil() as usize
}

impl Map {
    pub fn generate(rng: &mut Rng, w: usize, h: usize) -> Self {
        let mut ground = Vec::with_capacity(w * h);
        for _ in 0..w * h {
            ground.push(if rng.below(10) == 0 { art::DIRT_TUFTS[rng.below(2)] } else { art::DIRT[rng.below(2)] });
        }
        let mut map = Map {
            w,
            h,
            ground,
            overlay: vec![None; w * h],
            solid: vec![false; w * h],
            cells: vec![Cell::Open; w * h],
            props: Vec::new(),
            decals: Vec::new(),
            barrels: Vec::new(),
            spawn: [0.0; 2],
        };
        map.border();
        let road = map.road(rng);
        map.gas_station(rng, road);
        let house = map.place_near(rng, &[art::HOUSE], [w / 2, h / 2], 14, 2, 1);
        let barn_art: &'static [Art] = if rng.below(2) == 0 { &[art::BARN_RED] } else { &[art::BARN_GREEN] };
        let barn = map.place_near(rng, barn_art, [w / 2, h / 2], 16, 2, 2);
        for (spot, door) in [(house, 2), (barn, 2)] {
            if let Some((x, y, _, bh)) = spot {
                map.path_to_road(x + door, y + bh, road);
            }
        }
        map.autotile_paths();
        if let Some((x, y, bw, bh)) = barn {
            let at = [x + bw / 2, y + bh / 2];
            map.windmill(rng, at);
            map.place_near(rng, &[art::HAY_STACK], at, 7, 0, 1);
            map.place_near(rng, &[art::HAY], at, 7, 0, 0);
            map.place_near(rng, &[art::TRACTOR], at, 8, 0, 1);
        }
        if let Some((x, y, bw, bh)) = house {
            map.place_near(rng, &[art::MAILBOX], [x + 2, y + bh + 2], 3, 0, 0);
            map.graveyard(rng, [x + bw / 2, y + bh / 2]);
        }
        for _ in 0..4 + rng.below(4) {
            map.field(rng);
        }
        map.roadside(rng, road);
        map.wrecks(rng, road);
        map.scatter(rng);
        map.props.retain(|p| {
            let barrel = p.frames[0] == art::BARREL;
            if barrel {
                map.barrels.push([p.x, p.y]);
            }
            !barrel
        });
        map.spawn = map.find_spawn();
        map
    }

    fn i(&self, x: usize, y: usize) -> usize {
        y * self.w + x
    }

    pub fn width_px(&self) -> f32 {
        self.w as f32 * TILE
    }

    pub fn height_px(&self) -> f32 {
        self.h as f32 * TILE
    }

    pub fn solid_at(&self, x: f32, y: f32) -> bool {
        if x < 0.0 || y < 0.0 {
            return true;
        }
        let (tx, ty) = ((x / TILE) as usize, (y / TILE) as usize);
        tx >= self.w || ty >= self.h || self.solid[self.i(tx, ty)]
    }

    /// Whether a box centered on (x, y) with half extents `half` overlaps a solid cell.
    pub fn hits(&self, x: f32, y: f32, half: [f32; 2]) -> bool {
        let (x0, y0, x1, y1) = (x - half[0], y - half[1], x + half[0], y + half[1]);
        let (tx0, ty0) = ((x0 / TILE).floor(), (y0 / TILE).floor());
        let (tx1, ty1) = ((x1 / TILE).floor(), (y1 / TILE).floor());
        let mut ty = ty0;
        while ty <= ty1 {
            let mut tx = tx0;
            while tx <= tx1 {
                if self.solid_at(tx * TILE + 1.0, ty * TILE + 1.0) {
                    return true;
                }
                tx += 1.0;
            }
            ty += 1.0;
        }
        false
    }

    /// Moves a box by `delta`, one axis at a time, so it slides along walls.
    pub fn slide(&self, pos: [f32; 2], delta: [f32; 2], half: [f32; 2]) -> [f32; 2] {
        let mut p = pos;
        if !self.hits(p[0] + delta[0], p[1], half) {
            p[0] += delta[0];
        }
        if !self.hits(p[0], p[1] + delta[1], half) {
            p[1] += delta[1];
        }
        p
    }

    /// Steps from every open cell to the cell under `target`, through open cells; `u16::MAX` where unreachable.
    pub fn flow(&self, target: [f32; 2]) -> Vec<u16> {
        let mut dist = vec![u16::MAX; self.w * self.h];
        let (tx, ty) = ((target[0] / TILE) as usize, (target[1] / TILE) as usize);
        if tx >= self.w || ty >= self.h {
            return dist;
        }
        let mut queue = std::collections::VecDeque::from([(tx, ty)]);
        dist[self.i(tx, ty)] = 0;
        while let Some((x, y)) = queue.pop_front() {
            let d = dist[self.i(x, y)] + 1;
            for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
                if nx < self.w && ny < self.h {
                    let i = self.i(nx, ny);
                    if !self.solid[i] && dist[i] == u16::MAX {
                        dist[i] = d;
                        queue.push_back((nx, ny));
                    }
                }
            }
        }
        dist
    }

    /// Direction from `pos` toward the neighboring cell nearest the flow target, or None off the field.
    pub fn downhill(&self, flow: &[u16], pos: [f32; 2]) -> Option<[f32; 2]> {
        let (x, y) = ((pos[0] / TILE) as i32, (pos[1] / TILE) as i32);
        let at = |x: i32, y: i32| {
            if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h { u16::MAX } else { flow[self.i(x as usize, y as usize)] }
        };
        let here = at(x, y);
        let mut best = (here, None);
        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
            // Diagonals only when both sides are open, so nobody cuts a corner into a wall.
            if dx != 0 && dy != 0 && (at(x + dx, y) == u16::MAX || at(x, y + dy) == u16::MAX) {
                continue;
            }
            let d = at(x + dx, y + dy);
            if d < best.0 {
                best = (d, Some((x + dx, y + dy)));
            }
        }
        let (cx, cy) = best.1?;
        let to = [(cx as f32 + 0.5) * TILE - pos[0], (cy as f32 + 0.5) * TILE - pos[1]];
        let l = (to[0] * to[0] + to[1] * to[1]).sqrt().max(1e-4);
        Some([to[0] / l, to[1] / l])
    }

    /// Clears the cell under a world point, as when a barrel blows.
    pub fn open(&mut self, x: f32, y: f32) {
        let (tx, ty) = ((x / TILE) as usize, (y / TILE) as usize);
        if tx < self.w && ty < self.h {
            let i = self.i(tx, ty);
            self.solid[i] = false;
        }
    }

    /// Ground and overlay art for one cell.
    pub fn tile(&self, x: usize, y: usize) -> (Art, Option<Art>) {
        let i = self.i(x, y);
        (self.ground[i], self.overlay[i])
    }

    fn free(&self, x: usize, y: usize, w: usize, h: usize, margin: usize) -> bool {
        if x < margin + 1 || y < margin + 1 || x + w + margin + 1 > self.w || y + h + margin + 1 > self.h {
            return false;
        }
        (y - margin..y + h + margin).all(|yy| (x - margin..x + w + margin).all(|xx| self.cells[self.i(xx, yy)] == Cell::Open))
    }

    fn claim(&mut self, x: usize, y: usize, w: usize, h: usize, as_cell: Cell) {
        for yy in y..(y + h).min(self.h) {
            for xx in x..(x + w).min(self.w) {
                let i = self.i(xx, yy);
                self.cells[i] = as_cell;
            }
        }
    }

    /// Adds a prop with its top-left on tile (x, y); every row but the top `open_rows` blocks movement.
    fn prop(&mut self, frames: &'static [Art], fps: f32, x: usize, y: usize, open_rows: usize) -> (usize, usize) {
        let (tw, th) = (tiles(frames[0].w()), tiles(frames[0].h()));
        let (px, py) = (x as f32 * TILE, y as f32 * TILE);
        let depth = thickness(frames[0], tw, th, open_rows);
        self.props.push(Prop { frames, fps, x: px, y: py, base: py + frames[0].h(), depth });
        for yy in y + open_rows.min(th - 1)..(y + th).min(self.h) {
            for xx in x..(x + tw).min(self.w) {
                let i = self.i(xx, yy);
                self.solid[i] = true;
            }
        }
        self.claim(x, y, tw, th, Cell::Taken);
        (tw, th)
    }

    /// Places a prop at a free spot within `radius` tiles of `at`; returns its tile rect.
    fn place_near(&mut self, rng: &mut Rng, frames: &'static [Art], at: [usize; 2], radius: usize, margin: usize, open_rows: usize) -> Option<(usize, usize, usize, usize)> {
        let (tw, th) = (tiles(frames[0].w()), tiles(frames[0].h()));
        for _ in 0..300 {
            let x = (at[0] + rng.below(2 * radius + 1)).saturating_sub(radius);
            let y = (at[1] + rng.below(2 * radius + 1)).saturating_sub(radius);
            if self.free(x, y, tw, th, margin) {
                self.prop(frames, 0.0, x, y, open_rows);
                return Some((x, y, tw, th));
            }
        }
        None
    }

    fn place_anywhere(&mut self, rng: &mut Rng, frames: &'static [Art], open_rows: usize) {
        let at = [self.w / 2, self.h / 2];
        self.place_near(rng, frames, at, self.w.max(self.h) / 2, 0, open_rows);
    }

    /// Dead trees and bushes around the edge keep everyone on the map.
    fn border(&mut self) {
        let (w, h) = (self.w, self.h);
        let ring = (0..w).flat_map(|x| [(x, 0), (x, h - 1)]).chain((1..h - 1).flat_map(|y| [(0, y), (w - 1, y)]));
        for (n, (x, y)) in ring.enumerate() {
            let art = if n % 3 == 0 { &art::DEAD_TREE[..1] } else { std::slice::from_ref(&art::BUSH) };
            self.prop(art, 0.0, x, y, 0);
        }
    }

    /// A vertical road, three tiles wide; returns its left column.
    fn road(&mut self, rng: &mut Rng) -> usize {
        let x = self.w / 4 + rng.below(self.w / 2);
        for y in 1..self.h - 1 {
            for k in 0..3 {
                let i = self.i(x + k, y);
                self.ground[i] = art::ROAD_V[k];
                self.cells[i] = Cell::Road;
            }
        }
        x
    }

    fn gas_station(&mut self, rng: &mut Rng, road: usize) {
        let (tw, th) = (tiles(art::GAS_STATION.w()), tiles(art::GAS_STATION.h()));
        let left = rng.below(2) == 0 && road > tw + 3;
        let x = if left { road - tw - 1 } else { road + 4 };
        for _ in 0..50 {
            let y = 3 + rng.below(self.h - th - 10);
            if self.free(x, y, tw, th + 3, 0) {
                self.prop(&[art::GAS_STATION], 0.0, x, y, 1);
                self.prop(&[art::GAS_PUMP], 0.0, x + 1, y + th, 2);
                self.prop(&[art::GAS_PUMP], 0.0, x + tw - 3, y + th, 2);
                if self.place_near(rng, &art::TRUCK, [x + tw / 2, y + th + 3], 4, 0, 1).is_some() {
                    let last = self.props.len() - 1;
                    self.props[last].fps = 3.0;
                }
                self.place_near(rng, &[art::CAR_V], [x + tw / 2, y + th + 2], 4, 0, 1);
                return;
            }
        }
    }

    fn windmill(&mut self, rng: &mut Rng, at: [usize; 2]) {
        if let Some((x, y, _, _)) = self.place_near(rng, &[art::WINDMILL], at, 8, 1, 2) {
            let (px, py) = (x as f32 * TILE, y as f32 * TILE - 6.0);
            let base = py + 6.0 + art::WINDMILL.h() + 0.1;
            self.props.push(Prop { frames: &art::WINDMILL_FAN, fps: 8.0, x: px, y: py, base, depth: 2.0 });
        }
    }

    /// Tombstones in a picket fence, with skull posts at the gate.
    fn graveyard(&mut self, rng: &mut Rng, at: [usize; 2]) {
        for _ in 0..200 {
            let x = (at[0] + rng.below(19)).saturating_sub(9);
            let y = (at[1] + rng.below(19)).saturating_sub(9);
            if self.free(x, y, 7, 5, 1) {
                for (dx, dy) in [(1, 1), (3, 1), (5, 1), (2, 3), (4, 3)] {
                    if rng.below(5) > 0 {
                        self.prop(&[art::TOMBSTONE], 0.0, x + dx, y + dy, 0);
                    }
                }
                self.fence(rng, &PICKET, x, y, 7, 5);
                self.claim(x, y, 7, 5, Cell::Taken);
                return;
            }
        }
    }

    /// Lays a dirt path from (x, y) down one tile, then across to the road.
    fn path_to_road(&mut self, x: usize, y: usize, road: usize) {
        let y = (y + 1).min(self.h - 2);
        let mut steps = vec![(x, y - 1), (x, y)];
        let (from, to) = if x < road { (x, road - 1) } else { (road + 3, x) };
        steps.extend((from..=to).map(|xx| (xx, y)));
        for (xx, yy) in steps {
            let i = self.i(xx, yy);
            if self.cells[i] == Cell::Open {
                self.cells[i] = Cell::Path;
            }
        }
    }

    fn autotile_paths(&mut self) {
        for y in 1..self.h - 1 {
            for x in 1..self.w - 1 {
                if self.cells[self.i(x, y)] != Cell::Path {
                    continue;
                }
                let joins = |xx: usize, yy: usize| matches!(self.cells[self.i(xx, yy)], Cell::Path | Cell::Road);
                let mask = N * joins(x, y - 1) as u8 + E * joins(x + 1, y) as u8 + S * joins(x, y + 1) as u8 + W * joins(x - 1, y) as u8;
                let i = self.i(x, y);
                self.ground[i] = art::PATH[mask as usize];
            }
        }
    }

    fn field(&mut self, rng: &mut Rng) {
        let (fw, fh) = (5 + rng.below(7), 4 + rng.below(5));
        let fenced = rng.below(2) == 0;
        for _ in 0..200 {
            let x = 2 + rng.below(self.w - fw - 4);
            let y = 2 + rng.below(self.h - fh - 4);
            if !self.free(x, y, fw, fh, 2) {
                continue;
            }
            let kind = rng.below(4);
            for yy in y..y + fh {
                for xx in x..x + fw {
                    let i = self.i(xx, yy);
                    let soil = art::SOIL[rng.below(2)];
                    self.ground[i] = if kind == 1 { art::SPROUTS[rng.below(2)] } else { soil };
                    if kind >= 2 {
                        self.overlay[i] = Some(match yy {
                            _ if yy == y => art::CORN_TOP[rng.below(2)],
                            _ if yy == y + fh - 1 => art::CORN_BOTTOM,
                            _ => art::CORN,
                        });
                    }
                }
            }
            self.claim(x, y, fw, fh, Cell::Taken);
            if kind < 2 && rng.below(2) == 0 {
                self.prop(&[art::SCARECROW], 0.0, x + fw / 2, y + fh / 2 - 1, 1);
            }
            if fenced {
                self.fence(rng, &RAIL, x - 1, y - 1, fw + 2, fh + 2);
            }
            return;
        }
    }

    /// A fence around the rect, with a two-tile gate in the bottom.
    fn fence(&mut self, rng: &mut Rng, style: &Fence, x: usize, y: usize, w: usize, h: usize) {
        let gate = x + 1 + rng.below(w - 3);
        for xx in x..x + w {
            let k = if xx == x { 0 } else if xx == x + w - 1 { 2 } else { 1 };
            self.prop(&style.top[k..k + 1], 0.0, xx, y, 0);
            let post = (xx + 1 == gate || xx == gate + 2) && k == 1;
            match style.gate_post {
                Some(posts) if post => {
                    let p = rng.below(posts.len());
                    self.prop(&posts[p..p + 1], 0.0, xx, y + h - 1, 0);
                }
                _ if xx != gate && xx != gate + 1 => {
                    self.prop(&style.bottom[k..k + 1], 0.0, xx, y + h - 1, 0);
                }
                _ => {}
            }
        }
        for yy in y + 1..y + h - 1 {
            self.prop(&style.side[..1], 0.0, x, yy, 0);
            self.prop(&style.side[1..], 0.0, x + w - 1, yy, 0);
        }
    }

    /// Signs on the shoulders and barriers half-closing the road near each end.
    fn roadside(&mut self, rng: &mut Rng, road: usize) {
        for _ in 0..4 {
            let x = if rng.below(2) == 0 { road - 1 } else { road + 3 };
            let y = 2 + rng.below(self.h - 4);
            if self.cells[self.i(x, y)] == Cell::Open {
                let set: &'static [Art] = if rng.below(2) == 0 { &art::SIGN } else { &art::SIGN_LOW };
                let k = rng.below(set.len());
                self.prop(&set[k..k + 1], 0.0, x, y, 0);
            }
        }
        for (lanes, y) in [([0, 2], 3), ([0, 1], self.h - 4)] {
            for lane in lanes {
                self.prop(&[art::BARRIER], 0.0, road + lane, y, 0);
            }
        }
    }

    /// Wrecked cars on the road, each blocking one lane.
    fn wrecks(&mut self, rng: &mut Rng, road: usize) {
        for _ in 0..3 + rng.below(3) {
            let (x, y) = (road + rng.below(3), 2 + rng.below(self.h - 6));
            let k = rng.below(art::WRECK_V.len());
            if (y..y + 3).all(|yy| !self.solid[self.i(x, yy)]) {
                self.prop(&art::WRECK_V[k..k + 1], 0.0, x, y, 1);
            }
        }
    }

    fn scatter(&mut self, rng: &mut Rng) {
        let area = self.w * self.h;
        let counts: [(&'static [Art], usize, usize); 10] = [
            (&art::DEAD_TREE, area / 120, 0),
            (std::slice::from_ref(&art::BUSH), area / 140, 0),
            (std::slice::from_ref(&art::BUSHES), area / 900, 1),
            (std::slice::from_ref(&art::TALL_TREE), area / 400, 1),
            (&art::LOG, area / 400, 0),
            (std::slice::from_ref(&art::STUMP), area / 600, 0),
            (std::slice::from_ref(&art::BARREL), area / 500, 0),
            (std::slice::from_ref(&art::TANK), area / 1200, 0),
            (&art::WRECK_H, area / 1200, 0),
            (&art::SIGN, area / 1200, 0),
        ];
        for (set, n, open_rows) in counts {
            for _ in 0..n {
                let k = rng.below(set.len());
                self.place_anywhere(rng, &set[k..k + 1], open_rows);
            }
        }
        for _ in 0..area / 300 {
            let (x, y) = (1 + rng.below(self.w - 2), 1 + rng.below(self.h - 2));
            if self.cells[self.i(x, y)] == Cell::Open {
                let a = art::CORPSE[rng.below(art::CORPSE.len())];
                self.decals.push((a, x as f32 * TILE, y as f32 * TILE));
            }
        }
        for _ in 0..area / 600 {
            let (cx, cy, r) = (rng.below(self.w) as i32, rng.below(self.h) as i32, rng.below(2) as i32);
            for y in (cy - r).max(1)..(cy + r + 1).min(self.h as i32 - 1) {
                for x in (cx - r).max(1)..(cx + r + 1).min(self.w as i32 - 1) {
                    let i = self.i(x as usize, y as usize);
                    if (x - cx).pow(2) + (y - cy).pow(2) <= r * r && self.cells[i] == Cell::Open {
                        self.ground[i] = art::GRASS[rng.below(2)];
                    }
                }
            }
        }
    }

    /// The open tile nearest the middle with a clear tile all around it.
    fn find_spawn(&self) -> [f32; 2] {
        let (cx, cy) = (self.w as i32 / 2, self.h as i32 / 2);
        let clear = |x: i32, y: i32| (-1..=1).all(|dy| (-1..=1).all(|dx| !self.solid[self.i((x + dx) as usize, (y + dy) as usize)]));
        for r in 0..self.w.max(self.h) as i32 {
            for y in (cy - r).max(2)..=(cy + r).min(self.h as i32 - 3) {
                for x in (cx - r).max(2)..=(cx + r).min(self.w as i32 - 3) {
                    if clear(x, y) {
                        return [(x as f32 + 0.5) * TILE, (y as f32 + 0.5) * TILE];
                    }
                }
            }
        }
        [cx as f32 * TILE, cy as f32 * TILE]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_seed_builds_a_farm_with_a_clear_spawn() {
        for seed in 1..40 {
            let map = Map::generate(&mut Rng::new(seed), 72, 54);
            let [x, y] = map.spawn;
            assert!(!map.hits(x, y, [6.0, 6.0]), "seed {seed}: spawn blocked");
            let has = |a: Art| map.props.iter().any(|p| p.frames[0] == a);
            assert!(has(art::HOUSE) || has(art::BARN_RED) || has(art::BARN_GREEN), "seed {seed}: no farmstead");
        }
    }

    #[test]
    fn the_edge_is_solid() {
        let map = Map::generate(&mut Rng::new(7), 40, 30);
        assert!(map.solid_at(5.0, 5.0));
        assert!(map.solid_at(-1.0, 100.0));
        assert!(map.hits(map.width_px() - 4.0, 100.0, [2.0, 2.0]));
    }

    #[test]
    fn the_flow_field_leads_around_walls() {
        let map = Map::generate(&mut Rng::new(11), 72, 54);
        let flow = map.flow(map.spawn);
        let start = [map.width_px() - 2.5 * TILE, map.height_px() - 2.5 * TILE];
        let mut pos = start;
        for _ in 0..2000 {
            match map.downhill(&flow, pos) {
                Some(d) => pos = map.slide(pos, [d[0] * 2.0, d[1] * 2.0], [3.0, 2.0]),
                None => break,
            }
        }
        let goal = ((map.spawn[0] - pos[0]).powi(2) + (map.spawn[1] - pos[1]).powi(2)).sqrt();
        assert!(goal < TILE * 1.5, "stuck at {pos:?}, spawn {:?}", map.spawn);
    }

    #[test]
    fn slide_keeps_the_free_axis() {
        let map = Map::generate(&mut Rng::new(3), 40, 30);
        let [x, y] = map.spawn;
        let moved = map.slide([x, y], [-1000.0, 0.0], [4.0, 4.0]);
        assert_eq!(moved, [x, y]);
        let moved = map.slide([x, y], [1.0, 1.0], [4.0, 4.0]);
        assert_eq!(moved, [x + 1.0, y + 1.0]);
    }
}
