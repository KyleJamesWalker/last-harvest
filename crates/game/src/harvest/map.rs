//! Procedural farm map: a ground tile per cell, solid cells for collision, and props drawn in depth order.

use crate::art::{self, Art};
use crate::rng::Rng;

mod farm;
mod farms;
mod ground;
mod text;
pub use farm::parse as parse_farm;
pub use farm::set as set_farm;
pub use farms::{DIR as FARM_DIR, GROUND_END as FARM_GROUND_END, ID_MAX as FARM_ID_MAX, file as farm_file, ids as farm_ids, install as install_farms, of_file as farm_of_file, use_farm, use_farm_text, valid_id as valid_farm_id};
use farms::with_every_ground;

/// Checks a hand-made map as the game loads it: against the ground of every farm installed.
pub fn check_map_on_any_farm(file: &str, text: String) -> Result<crate::data::LoadedMap, String> {
    with_every_ground(|| text::check(file, text))
}

/// Checks the ground codes a farm's text names against `ground.txt` as the game reads it.
pub fn check_farm_codes(f: &farm::Farm) -> Result<(), String> {
    farm::check_codes(f, ground::ground())
}
pub(super) use ground::crop_piece;
pub use ground::{parse as parse_ground, set as set_ground, Stalk, Sway};
pub use text::{EXTENSION as MAP_EXTENSION, ground_codes, MAX_SIZE as MAP_MAX, MIN_SIZE as MAP_MIN, MapSource, PropKind, parse_props, props as map_props, PropLine, Tough, check as check_map, valid_id as valid_map_id};

pub const TILE: f32 = 16.0;

/// A cell's crop: its piece on the map, and its piece and sway in 3D.
pub type CellCrop = (Option<Art>, Option<Stalk>);
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
    /// Hit points left, and whole; 0 whole for one that doesn't break.
    pub hp: f32,
    full: f32,
    /// Seconds left of the flash from a hit.
    pub flash: f32,
    /// What shows once it breaks; without one it goes.
    broken: Option<&'static [Art]>,
    /// The cells it blocks, as x, y, width and height in tiles, and whether they stop shots.
    cells: [usize; 4],
    /// The part of it that blocks, in world pixels as x0, y0, x1, y1: its picture below any walk-through rows.
    area: [f32; 4],
    /// How far past its picture a fence side runs south to meet its corner's board, in pixels.
    pub reach: f32,
    cover: bool,
    /// A piece of a fence the map put up, of whatever style.
    fence: bool,
}

/// A catalog sprite's frames by name, however many it has.
fn named(name: &str) -> Option<&'static [Art]> {
    art::ALL.iter().find(|(n, _)| *n == name).map(|(_, f)| *f).filter(|f| !f.is_empty())
}

impl Prop {
    pub fn cells(&self) -> [usize; 4] {
        self.cells
    }

    /// A prop that blocks nothing and never breaks.
    fn new(frames: &'static [Art], fps: f32, x: f32, y: f32, base: f32, depth: f32) -> Self {
        Prop { frames, fps, x, y, base, depth, hp: 0.0, full: 0.0, flash: 0.0, broken: None, cells: [0; 4], area: [0.0; 4], reach: 0.0, cover: false, fence: false }
    }

    pub fn frame(&self, t: f32) -> Art {
        self.frames[(t * self.fps) as usize % self.frames.len()]
    }

    /// The box it fills in cell (x, y), if any: what its outline covers there when drawn from above, else its
    /// `area`.
    fn fill(&self, x: usize, y: usize) -> Option<[u8; 4]> {
        if !self.blocks(x, y) {
            return None;
        }
        let outline = match self.frames[0].stand() {
            Some(art::Stand::Slab { .. } | art::Stand::Turned { .. }) => self.frames[0],
            Some(art::Stand::Carve { footprint, .. }) => footprint.unwrap_or(self.frames[0]),
            _ => return Some(clip(self.area, x, y)).filter(|b| b[2] > b[0] && b[3] > b[1]),
        };
        let (cx, cy) = (x as f32 * TILE - self.x, y as f32 * TILE - self.y);
        let span = |c: f32, n: usize| (c.max(0.0) as usize).min(n)..((c + TILE).max(0.0) as usize).min(n);
        art::with_outline(outline, |solid, [w, h]| {
            let mut b: Option<[f32; 4]> = None;
            for j in span(cy, h) {
                for i in span(cx, w).filter(|&i| solid(i, j)) {
                    let (lx, ly) = (i as f32 - cx, j as f32 - cy);
                    b = Some(b.map_or([lx, ly, lx + 1.0, ly + 1.0], |b| [b[0].min(lx), b[1].min(ly), b[2].max(lx + 1.0), b[3].max(ly + 1.0)]));
                }
            }
            b.map(|b| b.map(|v| v as u8))
        })
    }

    fn blocks(&self, x: usize, y: usize) -> bool {
        let [cx, cy, w, h] = self.cells;
        (cx..cx + w).contains(&x) && (cy..cy + h).contains(&y)
    }

    /// Steps of open ground a zombie would rather walk than chew through it; 0 for one that doesn't break.
    fn chew_steps(&self) -> u16 {
        if self.full > 0.0 { (1.0 + self.full / CHEW_HP_PER_STEP).min(MAX_STEP as f32) as u16 } else { 0 }
    }
}

/// Hit points a breakable prop is worth a step of detour for, in the flow field.
const CHEW_HP_PER_STEP: f32 = 8.0;
/// The most one cell of the flow field costs; the bucket queue keeps one bucket more than this.
const MAX_STEP: usize = 15;

/// Room a flow field rebuild reuses: its ring of buckets and the two queues of cells at the current distance.
#[derive(Default)]
pub struct Steps {
    ring: [Vec<u32>; MAX_STEP + 1],
    now: Vec<u32>,
    next: Vec<u32>,
}

/// No prop breaks with more hit points than this.
pub const MAX_HP: f32 = 1000.0;
const NO_OWNER: u32 = u32::MAX;

/// The pieces of one fence style: top row left, middle, right; side posts left, right; bottom row likewise.
#[derive(Clone, Copy, Debug)]
struct Fence {
    top: &'static [Art],
    side: &'static [Art],
    bottom: &'static [Art],
    /// Drawn on each side of the gate, if the style has gate posts.
    gate_post: Option<&'static [Art]>,
}

const RAIL: Fence = Fence { top: &art::RAIL_TOP, side: &art::RAIL_SIDE, bottom: &art::RAIL_BOTTOM, gate_post: None };
const PICKET: Fence = Fence { top: &art::PICKET_TOP, side: &art::PICKET_SIDE, bottom: &art::PICKET_BOTTOM, gate_post: Some(&art::SKULL_FENCE) };

/// How a placed prop stands in the way: whether it stops shots, its hit points, 0 if it doesn't break, and what
/// shows once it does.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Toughness {
    cover: bool,
    hp: f32,
    broken: Option<&'static [Art]>,
}

/// How a sprite stands in the way when no prop line says otherwise: a player's prop as its text says, a fence
/// piece as `props.txt` says fences do, and the game's own as it lists them.
/// Whether a sprite is a piece of either fence style.
fn is_fence(a: Art) -> bool {
    [&RAIL, &PICKET].iter().any(|f| f.top.contains(&a) || f.side.contains(&a) || f.bottom.contains(&a) || f.gate_post.is_some_and(|g| g.contains(&a)))
}

fn toughness(a: Art) -> Toughness {
    if let Some(p) = super::props::named_of(a) {
        return Toughness { cover: p.cover, hp: p.hp, broken: p.broken_for(a) };
    }
    if is_fence(a) {
        return Toughness::fence();
    }
    let kind = art::ALL.iter().find(|(_, f)| f.contains(&a)).and_then(|(n, _)| text::prop_kind(n));
    kind.map_or(Toughness::other(), Toughness::of)
}

/// The tiles of a ground code from `ground.txt`, else of its first.
fn ground_art(code: u8) -> &'static [Art] {
    let all = ground::ground();
    all.iter().find(|g| g.code == code).unwrap_or(&all[0]).art
}

/// Top rows a figure walks through for a sprite the game places: a player's prop's own, else as `props.txt` lists
/// it.
fn open_of(a: Art) -> usize {
    if let Some(p) = super::props::named_of(a) {
        return p.open;
    }
    art::ALL.iter().find(|(_, f)| f.contains(&a)).and_then(|(n, _)| text::prop_kind(n)).map_or(0, |k| k.open)
}

impl Toughness {
    fn of(k: &PropKind) -> Self {
        Toughness { cover: k.cover, hp: k.hp, broken: None }
    }

    /// How fence pieces stand in the way.
    fn fence() -> Self {
        Self::of(&text::book().fence)
    }

    /// How a sprite `props.txt` doesn't list stands in the way.
    fn other() -> Self {
        Self::of(&text::book().other)
    }

    /// These, with what a prop line says over them.
    fn with(self, t: Tough) -> Self {
        Toughness { cover: t.cover.unwrap_or(self.cover), hp: t.hp.unwrap_or(self.hp), ..self }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cell {
    Open,
    Road,
    Path,
    Taken,
}

#[derive(Clone)]
pub struct Map {
    pub w: usize,
    pub h: usize,
    ground: Vec<Art>,
    overlay: Vec<Option<Art>>,
    /// The crop standing on each cell in 3D.
    standing: Vec<Option<Stalk>>,
    /// Cells nobody walks through.
    solid: Vec<bool>,
    /// Within each solid cell, the box its props fill, which is all the player bumps into there, and a second box
    /// where a fence's side joins it, or `NO_BOX`.
    shape: Vec<[[u8; 4]; 2]>,
    /// Solid cells that stop shots too.
    cover: Vec<bool>,
    /// The breakable prop blocking each cell, by its place in `props`.
    owner: Vec<u32>,
    /// What crossing each cell costs a zombie, as `Prop::chew_steps` says; 0 for a wall.
    cost: Vec<u8>,
    cells: Vec<Cell>,
    pub props: Vec<Prop>,
    /// Flat art drawn on the ground under everything else: corpses and the like.
    pub decals: Vec<(Art, f32, f32)>,
    /// Explosive barrels, by top-left in world pixels; each blocks its cell until it blows.
    pub barrels: Vec<[f32; 2]>,
    /// Each barrel's hit points as its prop line set them; 0 for tuning.txt's.
    pub barrel_hp: Vec<f32>,
    /// Where the player starts, in world pixels.
    pub spawn: [f32; 2],
    /// The farmhouse's front step, which the player runs back to once dawn breaks.
    pub home: [f32; 2],
}

/// Buildings and vehicles reach back over their solid rows; small things are slabs.
fn thickness(art: Art, tw: usize, th: usize, open_rows: usize) -> f32 {
    if let Some(art::Stand::Prop { depth }) = art.stand() {
        depth
    } else if let Some(art::Stand::Fold { rows }) = art.stand() {
        rows.clamp(1.0, (art.h() - 1.0).max(1.0)).floor()
    } else if tw * th >= 4 {
        ((th - open_rows.min(th - 1)) as f32 * TILE).max(8.0)
    } else {
        3.0
    }
}

/// How far a prop `w` by `h` pixels reaches back in 3D when it says no depth, placed with `open_rows` walk-through
/// rows; `art` is the packed sprite, if it is one.
pub fn default_depth(w: f32, h: f32, art: Option<Art>, open_rows: usize) -> f32 {
    thickness(art.unwrap_or(Art(u16::MAX)), tiles(w), tiles(h), open_rows)
}

/// How far a sprite the game places reaches back in 3D, as the map stands it.
pub(super) fn depth_of(a: Art) -> f32 {
    thickness(a, tiles(a.w()), tiles(a.h()), open_of(a))
}

fn tiles(px: f32) -> usize {
    (px / TILE).ceil() as usize
}

/// One frame of a sprite the farm puts down, at random when it has several.
fn one_of(rng: &mut Rng, set: &'static [Art]) -> &'static [Art] {
    let k = if set.len() > 1 { rng.below(set.len()) } else { 0 };
    &set[k..k + 1]
}

/// A whole cell's box, as x0, y0, x1, y1 pixels from its corner.
const WHOLE: [u8; 4] = [0, 0, TILE as u8, TILE as u8];
const NO_BOX: [u8; 4] = [0; 4];

/// The part of world rect `area` inside cell (x, y), as a box from its corner.
fn clip(area: [f32; 4], x: usize, y: usize) -> [u8; 4] {
    let (cx, cy) = (x as f32 * TILE, y as f32 * TILE);
    let at = |v: f32, c: f32| (v - c).clamp(0.0, TILE) as u8;
    [at(area[0], cx), at(area[1], cy), at(area[2], cx), at(area[3], cy)]
}

fn union(a: [u8; 4], b: [u8; 4]) -> [u8; 4] {
    [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]
}

/// Where a picture's opaque pixels are, as x0, y0, x1, y1 from its top-left; the whole picture when none are.
fn opaque_bounds(a: Art) -> [f32; 4] {
    art::with_outline(a, |opaque, [w, h]| {
        let cols: Vec<usize> = (0..w).filter(|&x| (0..h).any(|y| opaque(x, y))).collect();
        let rows: Vec<usize> = (0..h).filter(|&y| (0..w).any(|x| opaque(x, y))).collect();
        let found = match (cols.first(), rows.first(), cols.last(), rows.last()) {
            (Some(&x0), Some(&y0), Some(&x1), Some(&y1)) => [x0, y0, x1 + 1, y1 + 1],
            _ => [0, 0, w, h],
        };
        found.map(|v| v as f32)
    })
}

impl Map {
    /// The farm's ground inside its fence, with nothing standing on it.
    fn bare(rng: &mut Rng, w: usize, h: usize) -> Self {
        let f = farm::farm();
        let (dirt, tufted) = (ground_art(f.ground), ground_art(f.tufted));
        let mut ground = Vec::with_capacity(w * h);
        for _ in 0..w * h {
            ground.push(if f.tufts > 0 && rng.below(f.tufts) == 0 { tufted[rng.below(tufted.len())] } else { dirt[rng.below(dirt.len())] });
        }
        let mut map = Map {
            w,
            h,
            ground,
            overlay: vec![None; w * h],
            standing: vec![None; w * h],
            solid: vec![false; w * h],
            shape: vec![[WHOLE, NO_BOX]; w * h],
            cover: vec![false; w * h],
            owner: vec![NO_OWNER; w * h],
            cost: vec![1; w * h],
            cells: vec![Cell::Open; w * h],
            props: Vec::new(),
            decals: Vec::new(),
            barrels: Vec::new(),
            barrel_hp: Vec::new(),
            spawn: [0.0; 2],
            home: [0.0; 2],
        };
        map.border();
        map
    }

    /// An open field `w` by `h` tiles, with the player starting in its middle.
    pub fn open_field(rng: &mut Rng, w: usize, h: usize) -> Self {
        let mut map = Self::bare(rng, w, h);
        map.spawn = [w as f32 * TILE / 2.0, h as f32 * TILE / 2.0];
        map.home = map.spawn;
        map
    }

    pub fn generate(rng: &mut Rng, w: usize, h: usize) -> Self {
        let f = farm::farm();
        let mut map = Self::bare(rng, w, h);
        let road = map.road(rng);
        map.gas_station(rng, road);
        let mut stood = Vec::new();
        for b in f.buildings.iter().filter(|b| !b.sprites.is_empty()) {
            let k = if b.sprites.len() > 1 { rng.below(b.sprites.len()) } else { 0 };
            stood.extend(map.place_near(rng, b.sprites[k], [w / 2, h / 2], b.within, b.margin).map(|spot| (b, spot)));
        }
        for &(b, (x, y, bw, bh)) in &stood {
            map.path_to_road(x + b.door.min(bw - 1), y + bh, road);
        }
        map.autotile_paths();
        // The last building's props go down first.
        for &(b, (x, y, bw, bh)) in stood.iter().rev() {
            let (middle, step) = ([x + bw / 2, y + bh / 2], [x + b.door.min(bw - 1), y + bh + 2]);
            for near in &b.near {
                match *near {
                    farm::Near::Windmill => map.windmill(rng, middle),
                    farm::Near::Graveyard => map.graveyard(rng, middle),
                    farm::Near::Prop { frames, within, door } => _ = map.place_near(rng, frames, if door { step } else { middle }, within, 0),
                }
            }
        }
        for _ in 0..f.fields + if f.more_fields > 0 { rng.below(f.more_fields) } else { 0 } {
            map.field(rng);
        }
        map.roadside(rng, road);
        map.wrecks(rng, road);
        map.scatter(rng);
        map.keep_barrels_apart();
        map.spawn = map.find_spawn();
        map.home = match stood.first() {
            Some(&(b, (x, y, bw, bh))) => [(x + b.door.min(bw - 1)) as f32 * TILE + TILE / 2.0, (y + bh) as f32 * TILE + TILE / 2.0],
            None => map.spawn,
        };
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

    /// Whether a shot at a world point stops there: on cover, or off the map.
    pub fn cover_at(&self, x: f32, y: f32) -> bool {
        if x < 0.0 || y < 0.0 {
            return true;
        }
        let (tx, ty) = ((x / TILE) as usize, (y / TILE) as usize);
        tx >= self.w || ty >= self.h || self.cover[self.i(tx, ty)]
    }

    /// Whether a box centered on (x, y) with half extents `half` overlaps what a solid cell's props fill, as the player
    /// bumps into them.
    pub fn hits(&self, x: f32, y: f32, half: [f32; 2]) -> bool {
        self.hits_as::<true>(x, y, half)
    }

    /// Whether a box overlaps a solid cell at all, as zombies bump into them: they find their way cell by cell, so a
    /// zombie in part of a solid cell would be lost.
    pub fn hits_cells(&self, x: f32, y: f32, half: [f32; 2]) -> bool {
        self.hits_as::<false>(x, y, half)
    }

    fn hits_as<const FILLED: bool>(&self, x: f32, y: f32, half: [f32; 2]) -> bool {
        let (x0, y0, x1, y1) = (x - half[0], y - half[1], x + half[0], y + half[1]);
        let (tx0, ty0) = ((x0 / TILE).floor(), (y0 / TILE).floor());
        let (tx1, ty1) = ((x1 / TILE).floor(), (y1 / TILE).floor());
        let mut ty = ty0;
        while ty <= ty1 {
            let mut tx = tx0;
            while tx <= tx1 {
                if self.solid_at(tx * TILE + 1.0, ty * TILE + 1.0) {
                    let i = if tx < 0.0 || ty < 0.0 { usize::MAX } else { (ty as usize) * self.w + tx as usize };
                    let filled = if FILLED { self.shape.get(i).copied().unwrap_or([WHOLE, NO_BOX]) } else { [WHOLE, NO_BOX] };
                    let (cx, cy) = (tx * TILE, ty * TILE);
                    let overlaps = |b: [u8; 4]| {
                        let [bx0, by0, bx1, by1] = b.map(f32::from);
                        bx1 > bx0 && x0 < cx + bx1 && x1 > cx + bx0 && y0 < cy + by1 && y1 > cy + by0
                    };
                    if filled.into_iter().any(overlaps) {
                        return true;
                    }
                }
                tx += 1.0;
            }
            ty += 1.0;
        }
        false
    }

    /// Moves a box by `delta`, one axis at a time, so it slides along walls, as `hits` bumps into them.
    pub fn slide(&self, pos: [f32; 2], delta: [f32; 2], half: [f32; 2]) -> [f32; 2] {
        self.slide_as::<true>(pos, delta, half)
    }

    /// Moves a box as `slide` does, bumping into whole cells as `hits_cells` does.
    pub fn slide_cells(&self, pos: [f32; 2], delta: [f32; 2], half: [f32; 2]) -> [f32; 2] {
        self.slide_as::<false>(pos, delta, half)
    }

    fn slide_as<const FILLED: bool>(&self, pos: [f32; 2], delta: [f32; 2], half: [f32; 2]) -> [f32; 2] {
        let mut p = pos;
        if !self.hits_as::<FILLED>(p[0] + delta[0], p[1], half) {
            p[0] += delta[0];
        }
        if !self.hits_as::<FILLED>(p[0], p[1] + delta[1], half) {
            p[1] += delta[1];
        }
        p
    }

    #[cfg(test)]
    pub fn flow(&self, target: [f32; 2]) -> Vec<u16> {
        let mut dist = Vec::new();
        self.steps::<true>(target, &mut dist, &mut Steps::default());
        dist
    }

    /// What it costs a zombie to get to the cell under `target` from every cell, into `dist`: a step for each open
    /// cell, and more through a prop it can chew through; `u16::MAX` where it can't.
    pub fn flow_into(&self, target: [f32; 2], dist: &mut Vec<u16>, steps: &mut Steps) {
        self.steps::<true>(target, dist, steps);
    }

    /// Steps from every open cell to the cell under `target` over open ground, as the player walks; `u16::MAX`
    /// where unreachable.
    pub fn reach(&self, target: [f32; 2]) -> Vec<u16> {
        let mut dist = Vec::new();
        self.steps::<false>(target, &mut dist, &mut Steps::default());
        dist
    }

    /// Dial's algorithm: cells a step away queue for the next distance, and those further through something to
    /// chew in a ring of buckets as long as the dearest step. Queued cells carry x in the low half and y in the
    /// high, which saves a division a cell on the device.
    fn steps<const CHEW: bool>(&self, target: [f32; 2], dist: &mut Vec<u16>, steps: &mut Steps) {
        let (w, h) = (self.w, self.h);
        dist.clear();
        dist.resize(w * h, u16::MAX);
        let (tx, ty) = ((target[0] / TILE) as usize, (target[1] / TILE) as usize);
        if tx >= w || ty >= h {
            return;
        }
        let Steps { ring, now, next } = steps;
        ring.iter_mut().for_each(Vec::clear);
        next.clear();
        now.clear();
        now.push((ty << 16 | tx) as u32);
        dist[self.i(tx, ty)] = 0;
        let (mut d, mut idle) = (0u16, 0);
        while idle <= MAX_STEP && d < u16::MAX - MAX_STEP as u16 {
            now.append(&mut ring[d as usize % ring.len()]);
            idle = if now.is_empty() { idle + 1 } else { 0 };
            for &c in now.iter() {
                let (x, y) = ((c & 0xffff) as usize, (c >> 16) as usize);
                if dist[y * w + x] != d {
                    continue;
                }
                for (ok, nx, ny) in [(x > 0, x.wrapping_sub(1), y), (x + 1 < w, x + 1, y), (y > 0, x, y.wrapping_sub(1)), (y + 1 < h, x, y + 1)] {
                    let j = ny.wrapping_mul(w).wrapping_add(nx);
                    let step = match ok {
                        false => continue,
                        true if CHEW => self.cost[j] as u16,
                        true => u16::from(!self.solid[j]),
                    };
                    if step > 0 && d + step < dist[j] {
                        dist[j] = d + step;
                        let c = (ny << 16 | nx) as u32;
                        if step == 1 { next.push(c) } else { ring[dist[j] as usize % ring.len()].push(c) }
                    }
                }
            }
            now.clear();
            std::mem::swap(now, next);
            d += 1;
        }
    }

    /// Direction from `pos` toward the neighboring cell nearest the flow target, or None off the field.
    pub fn downhill(&self, flow: &[u16], pos: [f32; 2]) -> Option<[f32; 2]> {
        let (x, y) = ((pos[0] / TILE) as i32, (pos[1] / TILE) as i32);
        let inside = |x: i32, y: i32| x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h;
        let at = |x: i32, y: i32| if inside(x, y) { flow[self.i(x as usize, y as usize)] } else { u16::MAX };
        let open = |x: i32, y: i32| inside(x, y) && !self.solid[self.i(x as usize, y as usize)];
        let here = at(x, y);
        let mut best = (here, None);
        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
            // Diagonals only when both sides are open, so nobody cuts a corner into a wall.
            if dx != 0 && dy != 0 && !(open(x + dx, y) && open(x, y + dy)) {
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

    /// The breakable prop blocking the cell under a world point, by its place in `props`.
    pub fn breakable_at(&self, x: f32, y: f32) -> Option<usize> {
        if x < 0.0 || y < 0.0 {
            return None;
        }
        let (tx, ty) = ((x / TILE) as usize, (y / TILE) as usize);
        (tx < self.w && ty < self.h).then(|| self.owner[self.i(tx, ty)]).filter(|&o| o != NO_OWNER).map(|o| o as usize)
    }

    /// Breakable props with blocked cells within `radius` of `c`, by their place in `props`.
    pub fn breakables_near(&self, c: [f32; 2], radius: f32) -> Vec<usize> {
        let near = |p: &Prop| {
            let [x, y, w, h] = p.cells.map(|v| v as f32 * TILE);
            let d = [(c[0] - c[0].clamp(x, x + w)), (c[1] - c[1].clamp(y, y + h))];
            d[0] * d[0] + d[1] * d[1] < radius * radius
        };
        self.props.iter().enumerate().filter(|(_, p)| p.full > 0.0 && near(p)).map(|(i, _)| i).collect()
    }

    /// Takes `damage` off a breakable prop and flashes it; it breaks in `crumble`.
    pub fn hurt_prop(&mut self, i: usize, damage: f32, flash: f32) {
        if let Some(p) = self.props.get_mut(i).filter(|p| p.full > 0.0) {
            p.hp -= damage;
            p.flash = p.flash.max(flash);
        }
    }

    /// Breaks one prop out of hit points, if any: its cells open where nothing else blocks them, and it shows
    /// its broken frame or goes. Returns its middle, in world pixels.
    pub fn crumble(&mut self, dt: f32) -> Option<[f32; 2]> {
        for p in &mut self.props {
            p.flash = (p.flash - dt).max(0.0);
        }
        let i = self.props.iter().position(|p| p.full > 0.0 && p.hp <= 0.0)?;
        let p = self.props[i];
        match p.broken {
            Some(frames) => self.props[i] = Prop::new(frames, 0.0, p.x, p.y, p.base, p.depth),
            None => {
                self.props.remove(i);
            }
        }
        let [x, y, w, h] = p.cells;
        for yy in y..y + h {
            for xx in x..x + w {
                let k = self.i(xx, yy);
                let filled = self.props.iter().filter_map(|q| Some((q.cover, q.fill(xx, yy)?)));
                let (cover, shape) = filled.fold((false, None), |(c, s), (qc, b)| (c || qc, Some(s.map_or(b, |s| union(s, b)))));
                (self.solid[k], self.cover[k], self.shape[k]) = (shape.is_some(), cover, [shape.unwrap_or(WHOLE), NO_BOX]);
            }
        }
        self.join_fences();
        self.index_breakables();
        Some([p.x + p.frames[0].w() / 2.0, p.base - (h as f32 * TILE) / 2.0])
    }

    /// Notes which breakable prop blocks each cell, the last one placed where they overlap, and what each cell
    /// costs a zombie to cross.
    fn index_breakables(&mut self) {
        self.owner.fill(NO_OWNER);
        for (i, p) in self.props.iter().enumerate().filter(|(_, p)| p.full > 0.0) {
            let [x, y, w, h] = p.cells;
            for yy in y..y + h {
                for xx in x..x + w {
                    self.owner[yy * self.w + xx] = i as u32;
                }
            }
        }
        for k in 0..self.cost.len() {
            self.cost[k] = match (self.solid[k], self.owner[k]) {
                (false, _) => 1,
                (true, NO_OWNER) => 0,
                (true, o) => self.props[o as usize].chew_steps() as u8,
            };
        }
    }

    /// Moves barrels from the props to `barrels`, since they blow up rather than break, and notes the rest's
    /// breakable cells.
    fn keep_barrels_apart(&mut self) {
        let (barrels, hp) = (&mut self.barrels, &mut self.barrel_hp);
        self.props.retain(|p| {
            let barrel = p.frames[0] == art::BARREL;
            if barrel {
                barrels.push([p.x, p.y]);
                hp.push(p.hp);
            }
            !barrel
        });
        self.join_fences();
        self.index_breakables();
    }

    /// Runs each fence side that ends on another fence piece on down through that cell to what blocks there, so a
    /// corner whose bottom piece blocks only its thin board has no gap between the two, while a side ending anywhere
    /// else stays as drawn.
    fn join_fences(&mut self) {
        let mut joins = Vec::new();
        for (k, p) in self.props.iter().enumerate().filter(|(_, p)| matches!(p.frames[0].stand(), Some(art::Stand::Turned { .. }))) {
            let [x0, _, x1, y1] = p.area;
            let (tx, ty) = ((x0 / TILE) as usize, (y1 / TILE) as usize);
            let fence_below = || self.props.iter().any(|q| q.blocks(tx, ty) && (q.fence || is_fence(q.frames[0])));
            if y1 % TILE != 0.0 || tx >= self.w || ty >= self.h || !self.solid[self.i(tx, ty)] || !fence_below() {
                continue;
            }
            let i = self.i(tx, ty);
            let [cx0, _, cx1, _] = clip([x0, ty as f32 * TILE, x1, (ty + 1) as f32 * TILE], tx, ty);
            joins.push((k, i, [cx0, 0, cx1, self.shape[i][0][1]]));
        }
        for (k, i, b) in joins {
            self.props[k].reach = b[3] as f32;
            self.shape[i][1] = if self.shape[i][1][2] > self.shape[i][1][0] { union(self.shape[i][1], b) } else { b };
        }
    }

    /// Clears the cell under a world point, as when a barrel blows.
    pub fn open(&mut self, x: f32, y: f32) {
        let (tx, ty) = ((x / TILE) as usize, (y / TILE) as usize);
        if tx < self.w && ty < self.h {
            let i = self.i(tx, ty);
            (self.solid[i], self.cover[i], self.cost[i]) = (false, false, 1);
        }
    }

    /// Blocks the cell under a world point again, as when a barrel comes back.
    pub fn close(&mut self, x: f32, y: f32) {
        let (tx, ty) = ((x / TILE) as usize, (y / TILE) as usize);
        if tx < self.w && ty < self.h {
            let i = self.i(tx, ty);
            (self.solid[i], self.cover[i], self.cost[i], self.shape[i]) = (true, true, 0, [WHOLE, NO_BOX]);
        }
    }

    /// Ground and overlay art for one cell.
    pub fn tile(&self, x: usize, y: usize) -> (Art, Option<Art>) {
        let i = self.i(x, y);
        (self.ground[i], self.overlay[i])
    }

    /// Plants the ground `code`'s crop on every cell within `reach` tiles of `at`, as a field of north-south rows
    /// with a lane through `at` and every seventh column and ninth row left bare, so it reads as planted.
    pub(super) fn plant(&mut self, code: u8, at: [usize; 2], reach: usize) {
        let Some(g) = ground::ground().iter().find(|g| g.code == code) else { return };
        let Some((top, middle, bottom)) = g.crop else { return };
        let ([ax, ay], [w, h]) = (at, [self.w, self.h]);
        let sown = move |x: usize, y: usize| {
            let (dx, dy) = (x.abs_diff(ax), y.abs_diff(ay));
            x < w && y < h && dx * dx + dy * dy <= reach * reach && dx != 0 && !x.is_multiple_of(7) && !y.is_multiple_of(9)
        };
        let cells: Vec<[usize; 2]> = (0..self.h).flat_map(|y| (0..self.w).map(move |x| [x, y])).filter(|&[x, y]| sown(x, y)).collect();
        for [x, y] in cells {
            let (up, down) = (y > 0 && sown(x, y - 1), sown(x, y + 1));
            let i = self.i(x, y);
            self.ground[i] = g.tile(x, y, up, down);
            self.overlay[i] = Some(crop_piece(top, middle, bottom, x, y, up, down));
            self.standing[i] = g.stalk(x, y, up, down);
        }
    }

    /// The crop standing on one cell in 3D.
    pub fn standing(&self, x: usize, y: usize) -> Option<Stalk> {
        self.standing[self.i(x, y)]
    }

    /// Lays a crop on one cell: its piece on the map, and its piece and sway in 3D; returns what was there.
    pub fn set_crop(&mut self, x: usize, y: usize, crop: CellCrop) -> CellCrop {
        let i = self.i(x, y);
        (std::mem::replace(&mut self.overlay[i], crop.0), std::mem::replace(&mut self.standing[i], crop.1))
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

    /// Adds a prop with its top-left on tile (x, y); every row but the top ones `props.txt` opens blocks movement.
    fn prop(&mut self, frames: &'static [Art], fps: f32, x: usize, y: usize) -> (usize, usize) {
        self.prop_as(frames, fps, x, y, open_of(frames[0]), toughness(frames[0]))
    }

    /// Adds a prop as `prop` does, standing in the way of shots as `tough` says.
    fn prop_as(&mut self, frames: &'static [Art], fps: f32, x: usize, y: usize, open_rows: usize, tough: Toughness) -> (usize, usize) {
        let (tw, th) = (tiles(frames[0].w()), tiles(frames[0].h()));
        let (px, py) = (x as f32 * TILE, y as f32 * TILE);
        let stand = frames[0].stand();
        // Drawn from above, the whole picture stands in 3D, so it all blocks. Upright, walk-through rows open from the
        // top, never all of it, and the 3D view extrudes it as deep as what is left unless its stand says.
        let from_above = matches!(stand, Some(art::Stand::Slab { .. } | art::Stand::Carve { .. } | art::Stand::Turned { .. }));
        let [ox0, oy0, ox1, oy1] = opaque_bounds(frames[0]);
        let open_px = if from_above { 0.0 } else { open_rows as f32 * TILE };
        let mut area = [px + ox0, (py + oy0).max(py + open_px).min(py + oy1 - 1.0), px + ox1, py + oy1];
        let depth = match stand {
            None if tw * th >= 4 => (area[3] - area[1]).max(8.0),
            _ => thickness(frames[0], tw, th, open_rows),
        };
        // Upright, it blocks just as deep as the 3D view stands it back from its bottom edge: never inside it, never
        // short of it.
        if !from_above {
            area[3] = py + frames[0].h();
            area[1] = (area[3] - depth.max(1.0)).max(py + oy0);
        }
        let (left, top) = ((area[0] / TILE) as usize, (area[1] / TILE) as usize);
        let (right, bottom) = (tiles(area[2]).min(self.w), tiles(area[3]).min(self.h));
        let cells = [left, top, right.saturating_sub(left), bottom.saturating_sub(top)];
        let hp = tough.hp.clamp(0.0, MAX_HP);
        let prop = Prop { hp, full: hp, broken: tough.broken, cells, area, cover: tough.cover, ..Prop::new(frames, fps, px, py, py + frames[0].h(), depth) };
        for yy in top..bottom {
            for xx in left..right {
                let Some(b) = prop.fill(xx, yy) else { continue };
                let i = self.i(xx, yy);
                self.shape[i][0] = if self.solid[i] { union(self.shape[i][0], b) } else { b };
                self.solid[i] = true;
                self.cover[i] |= tough.cover;
            }
        }
        self.props.push(prop);
        self.claim(x, y, tw, th, Cell::Taken);
        (tw, th)
    }

    /// Places a prop at a free spot within `radius` tiles of `at`; returns its tile rect.
    fn place_near(&mut self, rng: &mut Rng, frames: &'static [Art], at: [usize; 2], radius: usize, margin: usize) -> Option<(usize, usize, usize, usize)> {
        let (tw, th) = (tiles(frames[0].w()), tiles(frames[0].h()));
        for _ in 0..300 {
            let x = (at[0] + rng.below(2 * radius + 1)).saturating_sub(radius);
            let y = (at[1] + rng.below(2 * radius + 1)).saturating_sub(radius);
            if self.free(x, y, tw, th, margin) {
                self.prop(frames, 0.0, x, y);
                return Some((x, y, tw, th));
            }
        }
        None
    }

    fn place_anywhere(&mut self, rng: &mut Rng, frames: &'static [Art]) {
        let at = [self.w / 2, self.h / 2];
        self.place_near(rng, frames, at, self.w.max(self.h) / 2, 0);
    }

    /// Props around the edge, as `farm.txt` rings it, keep everyone on the map.
    fn border(&mut self) {
        let (w, h) = (self.w, self.h);
        let border = &farm::farm().border;
        let ring = (0..w).flat_map(|x| [(x, 0), (x, h - 1)]).chain((1..h - 1).flat_map(|y| [(0, y), (w - 1, y)]));
        for (n, (x, y)) in ring.enumerate() {
            self.prop(&border[n % border.len()][..1], 0.0, x, y);
        }
    }

    /// A vertical road, three tiles wide; returns its left column.
    fn road(&mut self, rng: &mut Rng) -> usize {
        let x = self.w / 4 + rng.below(self.w / 2);
        let g = ground::of(farm::farm().road.ground);
        for y in 1..self.h - 1 {
            for k in 0..3 {
                let i = self.i(x + k, y);
                self.ground[i] = g.tile(x + k, y, k > 0, k < 2);
                self.cells[i] = Cell::Road;
            }
        }
        x
    }

    fn gas_station(&mut self, rng: &mut Rng, road: usize) {
        let s = &farm::farm().station;
        let station = one_of(rng, s.station);
        let (tw, th) = (tiles(station[0].w()), tiles(station[0].h()));
        let left = rng.below(2) == 0 && road > tw + 3;
        let x = if left { road - tw - 1 } else { road + 4 };
        if self.h < th + 11 {
            return;
        }
        for _ in 0..50 {
            let y = 3 + rng.below(self.h - th - 10);
            if self.free(x, y, tw, th + 3, 0) {
                self.prop(station, 0.0, x, y);
                for px in [x + 1, (x + tw).saturating_sub(3)] {
                    let pump = one_of(rng, s.pump);
                    self.prop(pump, 0.0, px, y + th);
                }
                if let Some((tx, ty, _, _)) = self.place_near(rng, s.truck, [x + tw / 2, y + th + 3], 4, 0) {
                    let last = self.props.len() - 1;
                    self.props[last].fps = 3.0;
                    self.drive(s.truck[0], s.driver, tx, ty);
                }
                let car = one_of(rng, s.car);
                self.place_near(rng, car, [x + tw / 2, y + th + 2], 4, 0);
                return;
            }
        }
    }

    /// Leans the truck's driver on the back of `truck`, placed at tile (x, y), smoking, his feet on its ground line.
    fn drive(&mut self, truck: Art, driver: &'static [Art], x: usize, y: usize) {
        let h = driver[0].h();
        let (px, py) = (x as f32 * TILE + truck.w() - 2.0, y as f32 * TILE + truck.h() - h);
        self.props.push(Prop::new(driver, 3.0, px, py, py + h, 2.0));
    }

    fn windmill(&mut self, rng: &mut Rng, at: [usize; 2]) {
        let w = &farm::farm().windmill;
        let mill = one_of(rng, w.mill);
        if let Some((x, y, _, _)) = self.place_near(rng, mill, at, 8, 1) {
            let (px, py) = (x as f32 * TILE, y as f32 * TILE - 6.0);
            let base = py + 6.0 + mill[0].h() + 0.1;
            self.props.push(Prop::new(w.fan, 8.0, px, py, base, 2.0));
        }
    }

    /// Tombstones in a picket fence, with skull posts at the gate.
    fn graveyard(&mut self, rng: &mut Rng, at: [usize; 2]) {
        for _ in 0..200 {
            let x = (at[0] + rng.below(19)).saturating_sub(9);
            let y = (at[1] + rng.below(19)).saturating_sub(9);
            if self.free(x, y, 7, 5, 1) {
                let g = &farm::farm().graveyard;
                for (dx, dy) in [(1, 1), (3, 1), (5, 1), (2, 3), (4, 3)] {
                    if rng.below(5) > 0 {
                        let stone = one_of(rng, g.tombstone);
                        self.prop(stone, 0.0, x + dx, y + dy);
                    }
                }
                self.fence(rng, &g.fence, x, y, 7, 5);
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
        let path_art = ground::ground().iter().find(|g| g.cell == Cell::Path).map_or(&art::PATH[..], |g| g.art);
        for y in 1..self.h - 1 {
            for x in 1..self.w - 1 {
                if self.cells[self.i(x, y)] != Cell::Path {
                    continue;
                }
                let joins = |xx: usize, yy: usize| matches!(self.cells[self.i(xx, yy)], Cell::Path | Cell::Road);
                let mask = N * joins(x, y - 1) as u8 + E * joins(x + 1, y) as u8 + S * joins(x, y + 1) as u8 + W * joins(x - 1, y) as u8;
                let i = self.i(x, y);
                self.ground[i] = path_art[mask as usize];
            }
        }
    }

    fn field(&mut self, rng: &mut Rng) {
        self.field_of(rng, &farm::farm().field, ground::ground());
    }

    /// A field as `rule` lays one, with the codes it names from `ground`: every cell a frame of its soil, then a
    /// bare kind's own frame over it, or a crop kind's crop in rows.
    fn field_of(&mut self, rng: &mut Rng, rule: &farm::Fields, ground: &[ground::Ground]) {
        let ([wl, wh], [hl, hh]) = (rule.width, rule.height);
        let (fw, fh) = (wl + rng.below(wh - wl + 1), hl + rng.below(hh - hl + 1));
        let fenced = rule.fenced > 0 && rng.below(rule.fenced) == 0;
        if self.w < fw + 5 || self.h < fh + 5 {
            return;
        }
        let art_of = |code: u8| ground.iter().find(|g| g.code == code).unwrap_or(&ground[0]).art;
        let soil = art_of(rule.soil);
        let total: usize = rule.kinds().map(|(_, w, _)| w).sum();
        for _ in 0..200 {
            let x = 2 + rng.below(self.w - fw - 4);
            let y = 2 + rng.below(self.h - fh - 4);
            if !self.free(x, y, fw, fh, 2) {
                continue;
            }
            let (pick, mut upto) = (rng.below(total), 0);
            let (code, _, grows) = rule.kinds().find(|&(_, w, _)| {
                upto += w;
                pick < upto
            }).unwrap_or((rule.soil, 0, false));
            let crop = ground.iter().find(|g| grows && g.code == code).and_then(|g| Some((g, g.crop?)));
            let own = art_of(code);
            for yy in y..y + fh {
                for xx in x..x + fw {
                    let i = self.i(xx, yy);
                    let under = soil[rng.below(soil.len())];
                    self.ground[i] = if crop.is_none() && code != rule.soil { own[rng.below(own.len())] } else { under };
                    let Some((g, (top, middle, bottom))) = crop else { continue };
                    let (flat, pick) = match yy {
                        _ if yy == y => {
                            let k = rng.below(top.len());
                            (top[k], k)
                        }
                        _ if yy == y + fh - 1 => (bottom, xx * 7 + yy * 13),
                        _ => (middle, xx * 7 + yy * 13),
                    };
                    let (up, down) = (yy > y, yy + 1 < y + fh);
                    self.overlay[i] = Some(flat);
                    self.standing[i] = g.standing().map(|p| Stalk::of(p, pick, up, down, g.tall, g.sway));
                }
            }
            self.claim(x, y, fw, fh, Cell::Taken);
            let (scarecrow, every) = rule.scarecrow;
            if !grows && every > 0 && rng.below(every) == 0 {
                let set = one_of(rng, scarecrow);
                self.prop(set, 0.0, x + fw / 2, y + fh / 2 - 1);
            }
            if fenced {
                self.fence(rng, &rule.fence, x - 1, y - 1, fw + 2, fh + 2);
            }
            return;
        }
    }

    /// A fence around the rect, with a two-tile gate in the bottom.
    fn fence(&mut self, rng: &mut Rng, style: &Fence, x: usize, y: usize, w: usize, h: usize) {
        let gate = x + 1 + rng.below(w - 3);
        self.fence_gated(style, [x, y, w, h], Some(gate), Toughness::fence(), |n| rng.below(n));
    }

    /// A fence around the rect, with a two-tile gate in the bottom from column `gate` if any, every piece as
    /// `tough`; `pick` chooses among a style's gate posts.
    fn fence_gated(&mut self, style: &Fence, [x, y, w, h]: [usize; 4], gate: Option<usize>, tough: Toughness, mut pick: impl FnMut(usize) -> usize) {
        let first = self.props.len();
        for xx in x..x + w {
            let k = if xx == x { 0 } else if xx == x + w - 1 { 2 } else { 1 };
            self.prop_as(&style.top[k..k + 1], 0.0, xx, y, 0, tough);
            let post = gate.is_some_and(|g| xx + 1 == g || xx == g + 2) && k == 1;
            match style.gate_post {
                Some(posts) if post => {
                    let p = pick(posts.len());
                    self.prop_as(&posts[p..p + 1], 0.0, xx, y + h - 1, 0, tough);
                }
                _ if !gate.is_some_and(|g| xx == g || xx == g + 1) => {
                    self.prop_as(&style.bottom[k..k + 1], 0.0, xx, y + h - 1, 0, tough);
                }
                _ => {}
            }
        }
        for yy in y + 1..y + h - 1 {
            self.prop_as(&style.side[..1], 0.0, x, yy, 0, tough);
            self.prop_as(&style.side[1..], 0.0, x + w - 1, yy, 0, tough);
        }
        self.props[first..].iter_mut().for_each(|p| p.fence = true);
    }

    /// A barrier on the tile at `x`, `y`, as the road's are.
    pub(super) fn barrier(&mut self, x: usize, y: usize) {
        self.prop(&[art::BARRIER], 0.0, x, y);
    }

    /// Signs on the shoulders and barriers half-closing the road near each end.
    fn roadside(&mut self, rng: &mut Rng, road: usize) {
        let r = &farm::farm().road;
        for _ in 0..r.signs {
            let x = if rng.below(2) == 0 { road - 1 } else { road + 3 };
            let y = 2 + rng.below(self.h - 4);
            if self.cells[self.i(x, y)] == Cell::Open {
                let set = r.sign[rng.below(r.sign.len())];
                let k = rng.below(set.len());
                self.prop(&set[k..k + 1], 0.0, x, y);
            }
        }
        for (lanes, y) in [([0, 2], 3), ([0, 1], self.h - 4)] {
            for lane in lanes {
                let barrier = one_of(rng, r.barrier);
                self.prop(barrier, 0.0, road + lane, y);
            }
        }
    }

    /// Wrecked cars on the road, each blocking one lane.
    fn wrecks(&mut self, rng: &mut Rng, road: usize) {
        let r = &farm::farm().road;
        let [least, most] = r.wrecks;
        for _ in 0..least + rng.below(most - least + 1) {
            let (x, y) = (road + rng.below(3), 2 + rng.below(self.h - 6));
            let k = rng.below(r.wreck.len());
            let long = tiles(r.wreck[k].h());
            if (y..(y + long).min(self.h)).all(|yy| !self.solid[self.i(x, yy)]) {
                self.prop(&r.wreck[k..k + 1], 0.0, x, y);
            }
        }
    }

    fn scatter(&mut self, rng: &mut Rng) {
        self.scatter_props(rng);
        self.scatter_ground(rng);
    }

    /// How many of a thing `farm.txt` scatters one per `n` tiles puts on this map.
    fn per(&self, n: usize) -> usize {
        (self.w * self.h).checked_div(n).unwrap_or(0)
    }

    fn scatter_props(&mut self, rng: &mut Rng) {
        for &(set, n) in &farm::farm().scatter {
            for _ in 0..self.per(n) {
                let k = rng.below(set.len());
                self.place_anywhere(rng, &set[k..k + 1]);
            }
        }
    }

    /// Corpses and grass patches on the open ground the props left.
    fn scatter_ground(&mut self, rng: &mut Rng) {
        let (f, per) = (farm::farm(), |n: usize| self.per(n));
        let (corpses, grass) = (per(f.corpses), per(f.grass));
        for _ in 0..corpses {
            let (x, y) = (1 + rng.below(self.w - 2), 1 + rng.below(self.h - 2));
            if self.cells[self.i(x, y)] == Cell::Open {
                let a = f.corpse[rng.below(f.corpse.len())];
                self.decals.push((a, x as f32 * TILE, y as f32 * TILE));
            }
        }
        let patch = ground_art(f.patch);
        for _ in 0..grass {
            let (cx, cy, r) = (rng.below(self.w) as i32, rng.below(self.h) as i32, rng.below(2) as i32);
            for y in (cy - r).max(1)..(cy + r + 1).min(self.h as i32 - 1) {
                for x in (cx - r).max(1)..(cx + r + 1).min(self.w as i32 - 1) {
                    let i = self.i(x as usize, y as usize);
                    if (x - cx).pow(2) + (y - cy).pow(2) <= r * r && self.cells[i] == Cell::Open {
                        self.ground[i] = patch[rng.below(patch.len())];
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

    /// The tiles of one shortest walk from tile `from` to tile `to`, both ends in; empty when there's none.
    fn way(&self, from: [usize; 2], to: [usize; 2]) -> Vec<[usize; 2]> {
        let dist = self.reach([(to[0] as f32 + 0.5) * TILE, (to[1] as f32 + 0.5) * TILE]);
        let [mut x, mut y] = from;
        if x >= self.w || y >= self.h || dist[self.i(x, y)] == u16::MAX {
            return Vec::new();
        }
        let mut way = vec![[x, y]];
        while dist[self.i(x, y)] > 0 {
            let d = dist[self.i(x, y)];
            let next = [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)].into_iter().find(|&(nx, ny)| nx < self.w && ny < self.h && dist[self.i(nx, ny)] == d - 1);
            let Some((nx, ny)) = next else { break };
            (x, y) = (nx, ny);
            way.push([x, y]);
        }
        way
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Everything a generated farm lays down, hashed, so a change to how farm.txt is read can't move a thing.
    fn layout_hash(map: &Map) -> u64 {
        use std::fmt::Write;
        let mut s = format!("{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}", map.ground, map.overlay, map.standing, map.solid, map.shape, map.cover, map.cost, map.cells);
        for p in &map.props {
            write!(s, "{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}", p.frames, p.fps, p.x, p.y, p.base, p.depth, p.hp, p.cells, p.area, p.reach, p.cover).unwrap();
        }
        write!(s, "{:?}{:?}{:?}{:?}{:?}", map.decals, map.barrels, map.barrel_hp, map.spawn, map.home).unwrap();
        s.bytes().fold(0xcbf29ce484222325, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
    }

    #[test]
    fn the_shipped_farm_lays_the_same_farm_from_the_same_seed() {
        let _lock = art::test_lock();
        let hashes: Vec<u64> = [(1, 72, 54), (7, 40, 30), (11, 72, 54), (12345, 96, 64)].iter().map(|&(seed, w, h)| layout_hash(&Map::generate(&mut Rng::new(seed), w, h))).collect();
        assert_eq!(hashes, [0x216474493fffbe8, 0x624ac506f4a60258, 0xea8adc850f90fb5a, 0x662b956b01708bac], "{hashes:#x?}");
    }

    #[test]
    fn a_field_grows_the_crop_its_code_names() {
        let built_in = |f: &str| crate::harvest::built_in_table(f).unwrap();
        let ground = ground::parse(&format!("{}\n[k]\nname Cane\nart SOIL\ncell field\ncrop SIGN BARRIER TOMBSTONE\ntall 2\n", built_in("ground.txt"))).unwrap();
        let rule = farm::parse(&format!("{}\n[fields]\nplain\ncrops k 1\nfenced 0", built_in("farm.txt"))).unwrap().field;
        let mut map = Map::blank(40, 30);
        map.field_of(&mut Rng::new(5), &rule, &ground);
        let grown: Vec<usize> = (0..map.w * map.h).filter(|&i| map.overlay[i].is_some()).collect();
        assert!(grown.len() >= 5 * 4, "a whole field");
        for &i in &grown {
            let o = map.overlay[i].unwrap();
            assert!(art::SIGN.contains(&o) || o == art::BARRIER || o == art::TOMBSTONE, "{o:?}");
            assert!(art::SOIL.contains(&map.ground[i]));
            assert_eq!(map.standing[i].map(|s| s.tall), Some(2));
        }
        assert!(grown.iter().any(|&i| map.overlay[i] == Some(art::BARRIER)), "rows between its ends");
        assert!(map.props.is_empty(), "no fence, and no scarecrow in a crop");
    }

    #[test]
    fn every_seed_builds_a_farm_with_a_clear_spawn() {
        for seed in 1..40 {
            let map = Map::generate(&mut Rng::new(seed), 72, 54);
            let [x, y] = map.spawn;
            assert!(!map.hits(x, y, [6.0, 6.0]), "seed {seed}: spawn blocked");
            let has = |a: Art| map.props.iter().any(|p| p.frames[0] == a);
            assert!(has(art::HOUSE) || has(art::BARN_RED) || has(art::BARN_GREEN), "seed {seed}: no farmstead");
            assert!(!map.hits(map.home[0], map.home[1], [4.0, 4.0]), "seed {seed}: the front step is blocked");
            assert!(map.reach(map.home)[map.i((map.spawn[0] / TILE) as usize, (map.spawn[1] / TILE) as usize)] < u16::MAX, "seed {seed}: no way home");
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
    fn the_flow_field_is_the_cheapest_way_through() {
        let (mut steps, mut reused) = (Steps::default(), Vec::new());
        for seed in 1..6 {
            let map = Map::generate(&mut Rng::new(seed), 72, 54);
            let start = map.i((map.spawn[0] / TILE) as usize, (map.spawn[1] / TILE) as usize);
            let mut dist = vec![u16::MAX; map.w * map.h];
            dist[start] = 0;
            let mut heap = std::collections::BinaryHeap::from([std::cmp::Reverse((0u16, start))]);
            while let Some(std::cmp::Reverse((d, i))) = heap.pop() {
                let (x, y) = (i % map.w, i / map.w);
                for (ok, j) in [(x > 0, i.wrapping_sub(1)), (x + 1 < map.w, i + 1), (y > 0, i.wrapping_sub(map.w)), (y + 1 < map.h, i + map.w)] {
                    let step = if ok { map.cost[j] as u16 } else { 0 };
                    if d == dist[i] && step > 0 && d + step < dist[j] {
                        dist[j] = d + step;
                        heap.push(std::cmp::Reverse((d + step, j)));
                    }
                }
            }
            let flow = map.flow(map.spawn);
            assert_eq!(flow, dist, "seed {seed}");
            map.flow_into(map.home, &mut reused, &mut steps);
            map.flow_into(map.spawn, &mut reused, &mut steps);
            assert_eq!(reused, dist, "seed {seed}: and the same in room left from other fields");
            assert!(map.cost.iter().any(|c| *c > 1), "seed {seed}: fences to chew through");
            assert!(flow.iter().zip(map.reach(map.spawn)).all(|(f, r)| *f <= r), "seed {seed}: chewing is never further");
        }
    }

    #[test]
    fn a_prop_blocks_only_where_its_picture_is() {
        let mut map = Map::blank(10, 10);
        let a = art::WRECK_V[0];
        map.prop(&art::WRECK_V[..1], 0.0, 2, 2);
        let [x0, y0, x1, y1] = opaque_bounds(a).map(|v| v + 2.0 * TILE);
        let half = [4.0, 3.0];
        assert!(!map.hits(x1 + 4.5, (y0 + y1) / 2.0, half), "beside it, in the tile its last pixels reach into");
        assert!(map.hits((x0 + x1) / 2.0, (y0 + y1) / 2.0, half));
        assert!(!map.hits((x0 + x1) / 2.0, y1 + 3.5, half), "just below it");
        assert!(map.hits((x0 + x1) / 2.0, y1 + 2.5, half));
    }

    #[test]
    fn a_detail_2_prop_blocks_what_its_plain_sprite_does() {
        let _lock = art::test_lock();
        let plain = [art::WRECK_V[0], art::GAS_STATION, art::HAY];
        let fine = art::live_detailed(&plain, 2);
        let named = plain.iter().zip(&fine).enumerate().map(|(i, (&a, &b))| {
            let mut f = super::super::props::prop_fields(&format!("[detail_test_{i}]")).unwrap();
            f.open = open_of(a);
            (f, vec![(vec![b], a.stand())])
        });
        super::super::props::set_props(named.collect());
        for (a, b) in plain.into_iter().zip(fine) {
            let place = |a: Art| {
                let mut map = Map::blank(16, 12);
                map.prop(Vec::leak(vec![a]), 0.0, 2, 2);
                (map.solid, map.shape, map.props[0].cells, map.props[0].area)
            };
            assert_eq!(place(a), place(b), "{a:?}");
        }
        super::super::props::set_props(Vec::new());
        art::set_live_detail(Vec::new());
    }

    #[test]
    fn a_thin_upright_prop_blocks_only_as_deep_as_it_stands() {
        let mut map = Map::blank(10, 10);
        map.prop(&art::PICKET_BOTTOM[..1], 0.0, 2, 2);
        let base = 3.0 * TILE;
        assert!(!map.hits(2.5 * TILE, base - 2.0 - 3.5, [4.0, 3.0]), "up to the board");
        assert!(map.hits(2.5 * TILE, base - 2.0 - 2.5, [4.0, 3.0]));
    }

    #[test]
    fn a_fence_side_runs_on_into_its_corner() {
        let mut map = Map::blank(10, 10);
        map.prop(&art::RAIL_SIDE[..1], 0.0, 2, 2);
        map.prop(&art::RAIL_BOTTOM[..1], 0.0, 2, 3);
        let side = map.props[0].area;
        let gap = [(side[0] + side[2]) / 2.0, 3.0 * TILE + 6.0];
        assert!(!map.hits(gap[0], gap[1], [1.0, 1.0]), "the corner's board alone leaves a gap under the side");
        map.join_fences();
        assert!(map.hits(gap[0], gap[1], [1.0, 1.0]));
        assert!(!map.hits(side[2] + 6.0, gap[1], [1.0, 1.0]), "the rest of the corner stays open down to its board");
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
