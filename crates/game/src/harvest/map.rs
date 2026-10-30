//! Procedural farm map: a ground tile per cell, solid cells for collision, and props drawn in depth order.

use crate::art::{self, Art};
use crate::rng::Rng;

mod ground;
mod text;
pub use ground::parse as parse_ground;
pub use text::{EXTENSION as MAP_EXTENSION, ground_codes, MAX_SIZE as MAP_MAX, MIN_SIZE as MAP_MIN, MapSource, PropKind, parse_props, props as map_props, PropLine, Tough, check as check_map, valid_id as valid_map_id};

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
    /// Hit points left, and whole; 0 whole for one that doesn't break.
    pub hp: f32,
    full: f32,
    /// Seconds left of the flash from a hit.
    pub flash: f32,
    /// What shows once it breaks; without one it goes.
    broken: Option<&'static [Art]>,
    /// The cells it blocks, as x, y, width and height in tiles, and whether they stop shots.
    cells: [usize; 4],
    cover: bool,
}

impl Prop {
    /// A prop that blocks nothing and never breaks.
    fn new(frames: &'static [Art], fps: f32, x: f32, y: f32, base: f32, depth: f32) -> Self {
        Prop { frames, fps, x, y, base, depth, hp: 0.0, full: 0.0, flash: 0.0, broken: None, cells: [0; 4], cover: false }
    }

    pub fn frame(&self, t: f32) -> Art {
        self.frames[(t * self.fps) as usize % self.frames.len()]
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
/// No prop breaks with more hit points than this.
pub const MAX_HP: f32 = 1000.0;
const NO_OWNER: u32 = u32::MAX;

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
fn toughness(a: Art) -> Toughness {
    if let Some(p) = super::props::named_of(a) {
        return Toughness { cover: p.cover, hp: p.hp, broken: p.broken };
    }
    if [&RAIL, &PICKET].iter().any(|f| f.top.contains(&a) || f.side.contains(&a) || f.bottom.contains(&a)) {
        return Toughness::fence();
    }
    let kind = art::ALL.iter().find(|(_, f)| f.contains(&a)).and_then(|(n, _)| text::prop_kind(n));
    kind.map_or(Toughness::other(), Toughness::of)
}

/// Top rows a figure walks through for a sprite the game places, as `props.txt` lists it.
fn open_of(a: Art) -> usize {
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

pub struct Map {
    pub w: usize,
    pub h: usize,
    ground: Vec<Art>,
    overlay: Vec<Option<Art>>,
    /// Cells nobody walks through.
    solid: Vec<bool>,
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
        let road = map.road(rng);
        map.gas_station(rng, road);
        let house = map.place_near(rng, &[art::HOUSE], [w / 2, h / 2], 14, 2);
        let barn_art: &'static [Art] = if rng.below(2) == 0 { &[art::BARN_RED] } else { &[art::BARN_GREEN] };
        let barn = map.place_near(rng, barn_art, [w / 2, h / 2], 16, 2);
        for (spot, door) in [(house, 2), (barn, 2)] {
            if let Some((x, y, _, bh)) = spot {
                map.path_to_road(x + door, y + bh, road);
            }
        }
        map.autotile_paths();
        if let Some((x, y, bw, bh)) = barn {
            let at = [x + bw / 2, y + bh / 2];
            map.windmill(rng, at);
            map.place_near(rng, &[art::HAY_STACK], at, 7, 0);
            map.place_near(rng, &[art::HAY], at, 7, 0);
            map.place_near(rng, &[art::TRACTOR], at, 8, 0);
        }
        if let Some((x, y, bw, bh)) = house {
            map.place_near(rng, &[art::MAILBOX], [x + 2, y + bh + 2], 3, 0);
            map.graveyard(rng, [x + bw / 2, y + bh / 2]);
        }
        for _ in 0..4 + rng.below(4) {
            map.field(rng);
        }
        map.roadside(rng, road);
        map.wrecks(rng, road);
        map.scatter(rng);
        map.keep_barrels_apart();
        map.spawn = map.find_spawn();
        map.home = match house.or(barn) {
            Some((x, y, _, bh)) => [(x + 2) as f32 * TILE + TILE / 2.0, (y + bh) as f32 * TILE + TILE / 2.0],
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

    /// What it costs a zombie to get to the cell under `target` from every cell: a step for each open cell, and
    /// more through a prop it can chew through; `u16::MAX` where it can't.
    pub fn flow(&self, target: [f32; 2]) -> Vec<u16> {
        self.steps::<true>(target)
    }

    /// Steps from every open cell to the cell under `target` over open ground, as the player walks; `u16::MAX`
    /// where unreachable.
    pub fn reach(&self, target: [f32; 2]) -> Vec<u16> {
        self.steps::<false>(target)
    }

    /// Dial's algorithm: cells a step away queue for the next distance, and those further through something to
    /// chew in a ring of buckets as long as the dearest step. Queued cells carry x in the low half and y in the
    /// high, which saves a division a cell on the device.
    fn steps<const CHEW: bool>(&self, target: [f32; 2]) -> Vec<u16> {
        let (w, h) = (self.w, self.h);
        let mut dist = vec![u16::MAX; w * h];
        let (tx, ty) = ((target[0] / TILE) as usize, (target[1] / TILE) as usize);
        if tx >= w || ty >= h {
            return dist;
        }
        let mut ring: [Vec<u32>; MAX_STEP + 1] = Default::default();
        let (mut now, mut next) = (vec![(ty << 16 | tx) as u32], Vec::new());
        dist[self.i(tx, ty)] = 0;
        let (mut d, mut idle) = (0u16, 0);
        while idle <= MAX_STEP && d < u16::MAX - MAX_STEP as u16 {
            now.append(&mut ring[d as usize % ring.len()]);
            idle = if now.is_empty() { idle + 1 } else { 0 };
            for &c in &now {
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
            std::mem::swap(&mut now, &mut next);
            d += 1;
        }
        dist
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
                self.solid[k] = self.props.iter().any(|q| q.blocks(xx, yy));
                self.cover[k] = self.props.iter().any(|q| q.cover && q.blocks(xx, yy));
            }
        }
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
        self.index_breakables();
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
            (self.solid[i], self.cover[i], self.cost[i]) = (true, true, 0);
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

    /// Adds a prop with its top-left on tile (x, y); every row but the top ones `props.txt` opens blocks movement.
    fn prop(&mut self, frames: &'static [Art], fps: f32, x: usize, y: usize) -> (usize, usize) {
        self.prop_as(frames, fps, x, y, open_of(frames[0]), toughness(frames[0]))
    }

    /// Adds a prop as `prop` does, standing in the way of shots as `tough` says.
    fn prop_as(&mut self, frames: &'static [Art], fps: f32, x: usize, y: usize, open_rows: usize, tough: Toughness) -> (usize, usize) {
        let (tw, th) = (tiles(frames[0].w()), tiles(frames[0].h()));
        let (px, py) = (x as f32 * TILE, y as f32 * TILE);
        let depth = thickness(frames[0], tw, th, open_rows);
        let top = y + open_rows.min(th - 1);
        let cells = [x, top, (x + tw).min(self.w) - x, (y + th).min(self.h).saturating_sub(top)];
        let hp = tough.hp.clamp(0.0, MAX_HP);
        self.props.push(Prop { hp, full: hp, broken: tough.broken, cells, cover: tough.cover, ..Prop::new(frames, fps, px, py, py + frames[0].h(), depth) });
        for yy in top..(y + th).min(self.h) {
            for xx in x..(x + tw).min(self.w) {
                let i = self.i(xx, yy);
                self.solid[i] = true;
                self.cover[i] |= tough.cover;
            }
        }
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

    /// Dead trees and bushes around the edge keep everyone on the map.
    fn border(&mut self) {
        let (w, h) = (self.w, self.h);
        let ring = (0..w).flat_map(|x| [(x, 0), (x, h - 1)]).chain((1..h - 1).flat_map(|y| [(0, y), (w - 1, y)]));
        for (n, (x, y)) in ring.enumerate() {
            let art = if n % 3 == 0 { &art::DEAD_TREE[..1] } else { std::slice::from_ref(&art::BUSH) };
            self.prop(art, 0.0, x, y);
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
                self.prop(&[art::GAS_STATION], 0.0, x, y);
                self.prop(&[art::GAS_PUMP], 0.0, x + 1, y + th);
                self.prop(&[art::GAS_PUMP], 0.0, x + tw - 3, y + th);
                if self.place_near(rng, &art::TRUCK, [x + tw / 2, y + th + 3], 4, 0).is_some() {
                    let last = self.props.len() - 1;
                    self.props[last].fps = 3.0;
                }
                self.place_near(rng, &[art::CAR_V], [x + tw / 2, y + th + 2], 4, 0);
                return;
            }
        }
    }

    fn windmill(&mut self, rng: &mut Rng, at: [usize; 2]) {
        if let Some((x, y, _, _)) = self.place_near(rng, &[art::WINDMILL], at, 8, 1) {
            let (px, py) = (x as f32 * TILE, y as f32 * TILE - 6.0);
            let base = py + 6.0 + art::WINDMILL.h() + 0.1;
            self.props.push(Prop::new(&art::WINDMILL_FAN, 8.0, px, py, base, 2.0));
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
                        self.prop(&[art::TOMBSTONE], 0.0, x + dx, y + dy);
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
                self.prop(&[art::SCARECROW], 0.0, x + fw / 2, y + fh / 2 - 1);
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
        self.fence_gated(style, [x, y, w, h], Some(gate), Toughness::fence(), |n| rng.below(n));
    }

    /// A fence around the rect, with a two-tile gate in the bottom from column `gate` if any, every piece as
    /// `tough`; `pick` chooses among a style's gate posts.
    fn fence_gated(&mut self, style: &Fence, [x, y, w, h]: [usize; 4], gate: Option<usize>, tough: Toughness, mut pick: impl FnMut(usize) -> usize) {
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
    }

    /// Signs on the shoulders and barriers half-closing the road near each end.
    fn roadside(&mut self, rng: &mut Rng, road: usize) {
        for _ in 0..4 {
            let x = if rng.below(2) == 0 { road - 1 } else { road + 3 };
            let y = 2 + rng.below(self.h - 4);
            if self.cells[self.i(x, y)] == Cell::Open {
                let set: &'static [Art] = if rng.below(2) == 0 { &art::SIGN } else { &art::SIGN_LOW };
                let k = rng.below(set.len());
                self.prop(&set[k..k + 1], 0.0, x, y);
            }
        }
        for (lanes, y) in [([0, 2], 3), ([0, 1], self.h - 4)] {
            for lane in lanes {
                self.prop(&[art::BARRIER], 0.0, road + lane, y);
            }
        }
    }

    /// Wrecked cars on the road, each blocking one lane.
    fn wrecks(&mut self, rng: &mut Rng, road: usize) {
        for _ in 0..3 + rng.below(3) {
            let (x, y) = (road + rng.below(3), 2 + rng.below(self.h - 6));
            let k = rng.below(art::WRECK_V.len());
            if (y..y + 3).all(|yy| !self.solid[self.i(x, yy)]) {
                self.prop(&art::WRECK_V[k..k + 1], 0.0, x, y);
            }
        }
    }

    fn scatter(&mut self, rng: &mut Rng) {
        let area = self.w * self.h;
        let counts: [(&'static [Art], usize); 10] = [
            (&art::DEAD_TREE, area / 120),
            (std::slice::from_ref(&art::BUSH), area / 140),
            (std::slice::from_ref(&art::BUSHES), area / 900),
            (std::slice::from_ref(&art::TALL_TREE), area / 400),
            (&art::LOG, area / 400),
            (std::slice::from_ref(&art::STUMP), area / 600),
            (std::slice::from_ref(&art::BARREL), area / 500),
            (std::slice::from_ref(&art::TANK), area / 1200),
            (&art::WRECK_H, area / 1200),
            (&art::SIGN, area / 1200),
        ];
        for (set, n) in counts {
            for _ in 0..n {
                let k = rng.below(set.len());
                self.place_anywhere(rng, &set[k..k + 1]);
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
            assert!(map.cost.iter().any(|c| *c > 1), "seed {seed}: fences to chew through");
            assert!(flow.iter().zip(map.reach(map.spawn)).all(|(f, r)| *f <= r), "seed {seed}: chewing is never further");
        }
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
