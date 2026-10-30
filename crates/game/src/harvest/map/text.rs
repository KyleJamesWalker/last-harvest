//! Hand-made maps: a `.map.txt` file read into a `MapSource`, which the editor changes and writes back, and
//! built into the same `Map` the generator builds.
//!
//! ```text
//! name Old Farm
//! size 40 30
//! spawn 20 15
//! home 22 13
//! scatter
//! ground
//! ..........vvv..........   (one row per line, `size` wide, one code per cell)
//! props
//! HOUSE 18 8 open 1
//! DEAD_TREE 5 5 frame 2
//! fence rail 4 20 10 6 gate 3
//! ```
//!
//! Ground codes are in `GROUND`. Roads and paths join up with their neighbours, and corn grows tops and bottoms at
//! a field's edges. A prop is a sprite's name and the tile its top-left sits on; every row of it but the top
//! `open` ones blocks movement, and a windmill brings its fan. The edge is always a ring of trees and bushes, and
//! `scatter` sprinkles the rest of the open ground as the random farm does, differently each run.

use super::{Cell, MAX_HP, Map, PICKET, Prop, RAIL, TILE, Toughness, toughness};
use crate::art::{self, Art};
use crate::data::LoadedMap;
use crate::rng::Rng;

/// What a map's file name ends in.
pub const EXTENSION: &str = ".map.txt";
/// More than the biggest map's ground and a prop line for every cell, read before parsing, since a map may come
/// from anyone.
const MAX_TEXT: usize = 1024 * 1024;

/// The smallest and largest a map can be, in tiles.
pub const MIN_SIZE: [usize; 2] = [16, 12];
pub const MAX_SIZE: [usize; 2] = [160, 120];

/// Each ground code and what it lays.
pub const GROUND: [(u8, &str); 9] =
    [(b'.', "Dirt"), (b',', "Tufts"), (b'g', "Grass"), (b's', "Soil"), (b'p', "Sprouts"), (b'c', "Corn"), (b'v', "Road |"), (b'h', "Road -"), (b'=', "Path")];

/// A sprite a map can place, and how it stands in the way when its line doesn't say.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PropKind {
    pub name: &'static str,
    /// How many of its top rows a figure walks through.
    pub open: usize,
    /// Whether it stops shots.
    pub cover: bool,
    /// Hit points before zombies or shots break it; 0 for one that doesn't break.
    pub hp: f32,
}

/// The game's own props as `props.txt` lists them: the ones a map can place, then how a fence piece and any other
/// sprite stand in the way.
#[derive(Debug)]
pub struct PropBook {
    pub props: Vec<PropKind>,
    pub fence: PropKind,
    pub other: PropKind,
}

/// Reads `props.txt`; errors name the line.
pub fn parse_props(text: &str) -> Result<PropBook, String> {
    let mut sections: Vec<PropKind> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            if sections.iter().any(|k| k.name == name) {
                return Err(format!("{n}: [{name}] is listed twice"));
            }
            sections.push(PropKind { name: name.to_string().leak(), open: 0, cover: true, hp: 0.0 });
            continue;
        }
        let Some(k) = sections.last_mut() else { return Err(format!("{n}: a line before any [NAME]")) };
        let (key, value) = line.split_once(char::is_whitespace).map_or((line, ""), |(k, v)| (k, v.trim()));
        match key {
            "open" => k.open = value.parse::<usize>().ok().filter(|v| *v <= 64).ok_or_else(|| format!("{n}: open is 0 to 64 rows"))?,
            "cover" => k.cover = yes_no(Some(value)).ok_or_else(|| format!("{n}: cover is yes or no"))?,
            "hp" => k.hp = value.parse::<f32>().ok().filter(|v| (0.0..=MAX_HP).contains(v)).ok_or_else(|| format!("{n}: hp is a number from 0 to {MAX_HP}"))?,
            _ => return Err(format!("{n}: {key} isn't something a prop says")),
        }
    }
    let mut take = |name: &str| sections.iter().position(|k| k.name == name).map(|i| sections.remove(i)).ok_or_else(|| format!("no [{name}] section"));
    let (fence, other) = (take("fence")?, take("other")?);
    Ok(PropBook { props: sections, fence, other })
}

/// The game's own props, read from `props.txt` the first time they're asked for.
pub fn book() -> &'static PropBook {
    static BOOK: std::sync::OnceLock<PropBook> = std::sync::OnceLock::new();
    BOOK.get_or_init(|| parse_props(crate::data::props()).unwrap_or_else(|e| panic!("props.txt:{e}")))
}

/// The sprites a map can place.
pub fn props() -> &'static [PropKind] {
    &book().props
}

/// The sprite a map can place by this name, if it can.
pub fn prop_kind(name: &str) -> Option<&'static PropKind> {
    props().iter().find(|k| k.name == name)
}

/// What a prop line says over its sprite's own: its hit points, 0 for one that doesn't break, and whether it
/// stops shots. None keeps the sprite's.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Tough {
    pub hp: Option<f32>,
    pub cover: Option<bool>,
}

impl Tough {
    fn text(&self) -> String {
        let mut s = String::new();
        if let Some(hp) = self.hp {
            s += &format!(" hp {hp}");
        }
        if let Some(c) = self.cover {
            s += &format!(" cover {}", if c { "yes" } else { "no" });
        }
        s
    }

    /// Takes `key value` if it's one of these; false if it isn't, and an error if the value doesn't do.
    fn read(&mut self, key: &str, value: Option<&str>) -> Result<bool, String> {
        match key {
            "hp" => {
                let hp = value.and_then(|v| v.parse::<f32>().ok()).filter(|v| (0.0..=super::MAX_HP).contains(v));
                self.hp = Some(hp.ok_or_else(|| format!("hp is a number from 0 to {}", super::MAX_HP))?);
            }
            "cover" => self.cover = Some(yes_no(value).ok_or("cover is yes or no")?),
            _ => return Ok(false),
        }
        Ok(true)
    }
}

fn yes_no(word: Option<&str>) -> Option<bool> {
    match word? {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    }
}

/// One line of a map's props.
#[derive(Clone, Debug, PartialEq)]
pub enum PropLine {
    /// A sprite by name with its top-left on `at`; `frame` picks one of its frames, else they animate at `fps`.
    Sprite { name: String, at: [usize; 2], frame: Option<usize>, fps: f32, open: usize, tough: Tough },
    /// A fence round `rect` (x, y, width, height), with a two-tile gate `gate` tiles from its left.
    Fence { picket: bool, rect: [usize; 4], gate: Option<usize>, tough: Tough },
}

impl PropLine {
    /// The tiles it covers, as x, y, width and height.
    pub fn rect(&self) -> [usize; 4] {
        match self {
            PropLine::Sprite { name, at, frame, .. } => {
                let (tw, th) = sprite(name).map_or((1, 1), |f| {
                    let a = f[frame.unwrap_or(0).min(f.len() - 1)];
                    (super::tiles(a.w()), super::tiles(a.h()))
                });
                [at[0], at[1], tw, th]
            }
            PropLine::Fence { rect, .. } => *rect,
        }
    }

    /// Its hit points and whether it stops shots: what its line says, else what its sprite does.
    pub fn hp_and_cover(&self) -> (f32, bool) {
        let (own, tough) = match self {
            PropLine::Sprite { name, tough, .. } => (sprite(name).map_or(Toughness::other(), |f| toughness(f[0])), tough),
            PropLine::Fence { tough, .. } => (Toughness::fence(), tough),
        };
        let t = own.with(*tough);
        (t.hp, t.cover)
    }

    pub fn tough(&self) -> Tough {
        match self {
            PropLine::Sprite { tough, .. } | PropLine::Fence { tough, .. } => *tough,
        }
    }

    pub fn tough_mut(&mut self) -> &mut Tough {
        match self {
            PropLine::Sprite { tough, .. } | PropLine::Fence { tough, .. } => tough,
        }
    }

    fn text(&self) -> String {
        match self {
            PropLine::Sprite { name, at, frame, fps, open, tough } => {
                let mut s = format!("{name} {} {}", at[0], at[1]);
                if let Some(k) = frame {
                    s += &format!(" frame {k}");
                }
                if *fps > 0.0 {
                    s += &format!(" fps {fps}");
                }
                if *open > 0 {
                    s += &format!(" open {open}");
                }
                s + &tough.text()
            }
            PropLine::Fence { picket, rect: [x, y, w, h], gate, tough } => {
                let style = if *picket { "picket" } else { "rail" };
                let gate = gate.map_or(String::new(), |g| format!(" gate {g}"));
                format!("fence {style} {x} {y} {w} {h}{gate}{}", tough.text())
            }
        }
    }
}

/// A map as its file says it, before it is built.
#[derive(Clone, Debug, PartialEq)]
pub struct MapSource {
    pub name: String,
    pub size: [usize; 2],
    pub spawn: [usize; 2],
    pub home: [usize; 2],
    pub scatter: bool,
    /// One ground code per tile, row by row.
    pub ground: Vec<u8>,
    /// Each prop and the file line it came from, 0 for one added since.
    pub props: Vec<(usize, PropLine)>,
}

/// The sections of a map file, in order.
#[derive(PartialEq)]
enum Section {
    Header,
    Ground,
    Props,
}

/// A sprite's frames by name: the game's own, else a prop's.
fn sprite(name: &str) -> Option<&'static [Art]> {
    art::ALL.iter().find(|(n, _)| *n == name).map(|(_, f)| *f).or_else(|| super::super::props::prop(name).map(|p| p.frames))
}

impl MapSource {
    /// A map of dirt `size` tiles, with the spawn in the middle and home just above it.
    pub fn new(name: &str, size: [usize; 2]) -> Self {
        let [w, h] = size;
        Self { name: name.into(), size, spawn: [w / 2, h / 2], home: [w / 2, h / 2 - 1], scatter: false, ground: vec![b'.'; w * h], props: Vec::new() }
    }

    /// Reads a map file's syntax; whether the map works is `Map::from_source`'s to say. Errors name the line.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut size = None;
        let (mut name, mut spawn, mut home, mut scatter) = (String::new(), None, None, false);
        let mut rows: Vec<(usize, &str)> = Vec::new();
        let mut props = Vec::new();
        let mut section = Section::Header;
        for (i, raw) in text.lines().enumerate() {
            let n = i + 1;
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let words: Vec<&str> = line.split_whitespace().collect();
            match (words[0], &section) {
                ("ground", Section::Header) if words.len() == 1 => section = Section::Ground,
                ("props", Section::Header | Section::Ground) if words.len() == 1 => section = Section::Props,
                (_, Section::Ground) => rows.push((n, line)),
                (_, Section::Props) => props.push((n, prop_line(&words).map_err(|e| format!("{n}: {e}"))?)),
                ("name", _) => name = line["name".len()..].trim().chars().take(40).collect(),
                ("scatter", _) if words.len() == 1 => scatter = true,
                (key @ ("size" | "spawn" | "home"), _) => {
                    let pair = numbers(&words[1..]).filter(|v| v.len() == 2).ok_or_else(|| format!("{n}: {key} is two whole numbers"))?;
                    let at = [pair[0], pair[1]];
                    match key {
                        "size" => size = Some(at),
                        "spawn" => spawn = Some(at),
                        _ => home = Some(at),
                    }
                }
                (word, _) => return Err(format!("{n}: {word} isn't something a map says")),
            }
        }
        let [w, h] = size.ok_or("no size line")?;
        if w < MIN_SIZE[0] || h < MIN_SIZE[1] || w > MAX_SIZE[0] || h > MAX_SIZE[1] {
            return Err(format!("a map is {}x{} to {}x{} tiles, not {w}x{h}", MIN_SIZE[0], MIN_SIZE[1], MAX_SIZE[0], MAX_SIZE[1]));
        }
        if rows.len() != h {
            return Err(format!("the ground has {} rows, but the map is {h} tall", rows.len()));
        }
        let mut ground = Vec::with_capacity(w * h);
        for (n, row) in &rows {
            if row.len() != w || !row.is_ascii() {
                return Err(format!("{n}: a ground row is {w} codes, one per cell"));
            }
            if let Some(c) = row.bytes().find(|c| !GROUND.iter().any(|(g, _)| g == c)) {
                return Err(format!("{n}: {} isn't a ground code", c as char));
            }
            ground.extend(row.bytes());
        }
        let spawn = spawn.ok_or("no spawn line")?;
        let home = home.ok_or("no home line")?;
        Ok(Self { name, size: [w, h], spawn, home, scatter, ground, props })
    }

    pub fn to_text(&self) -> String {
        let [w, h] = self.size;
        let mut s = format!("name {}\nsize {w} {h}\nspawn {} {}\nhome {} {}\n", self.name, self.spawn[0], self.spawn[1], self.home[0], self.home[1]);
        if self.scatter {
            s += "scatter\n";
        }
        s += "ground\n";
        for row in self.ground.chunks(w) {
            s += std::str::from_utf8(row).unwrap_or("");
            s.push('\n');
        }
        s += "props\n";
        for (_, p) in &self.props {
            s += &p.text();
            s.push('\n');
        }
        s
    }

    /// The map `by` tiles wider and taller, kept within the sizes a map can be; new ground is dirt, props past the
    /// new edge go, and the spawn and home come in.
    pub fn resized(&self, by: [i32; 2]) -> Self {
        let size: [usize; 2] = std::array::from_fn(|i| (self.size[i] as i32 + by[i]).clamp(MIN_SIZE[i] as i32, MAX_SIZE[i] as i32) as usize);
        let [w, h] = size;
        let old = self.size;
        let ground = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| if x < old[0] && y < old[1] { self.ground[y * old[0] + x] } else { b'.' }).collect();
        let inside = |[x, y]: [usize; 2]| [x.clamp(1, w - 2), y.clamp(1, h - 2)];
        let fits = |p: &PropLine| {
            let [x, y, pw, ph] = p.rect();
            x + pw < w && y + ph < h
        };
        let props = self.props.iter().filter(|(_, p)| fits(p)).cloned().collect();
        Self { size, ground, spawn: inside(self.spawn), home: inside(self.home), props, ..self.clone() }
    }
}

impl MapSource {
    /// A farm the random generator lays out at `size`, as a map to edit: its ground as codes and every prop it
    /// placed as a line, fence pieces and all. Corpses on the ground and the windmill's fan, which a windmill
    /// brings, are left out.
    pub fn random(name: &str, size: [usize; 2], seed: u64) -> Self {
        let size = std::array::from_fn(|i| size[i].clamp(RANDOM_MIN[i], MAX_SIZE[i]));
        let [w, h] = size;
        let map = Map::generate(&mut Rng::new(seed), w, h);
        let code = |i: usize| {
            let (g, o) = (map.ground[i], map.overlay[i]);
            let is = |set: &[Art]| set.contains(&g);
            match () {
                _ if o.is_some_and(|o| o == art::CORN || o == art::CORN_BOTTOM || art::CORN_TOP.contains(&o)) => b'c',
                _ if is(&art::ROAD_V) => b'v',
                _ if is(&art::ROAD_H) => b'h',
                _ if is(&art::PATH) => b'=',
                _ if is(&art::SOIL) => b's',
                _ if is(&art::SPROUTS) => b'p',
                _ if is(&art::GRASS) => b'g',
                _ if is(&art::DIRT_TUFTS) && !is(&art::DIRT) => b',',
                _ => b'.',
            }
        };
        let tile = |px: f32| (px / TILE) as usize;
        let mut props = Vec::new();
        let barrels = map.barrels.iter().map(|&[x, y]| (std::slice::from_ref(&art::BARREL), x, y));
        for (frames, x, y) in map.props.iter().map(|p| (p.frames, p.x, p.y)).chain(barrels) {
            if frames[0] == art::WINDMILL_FAN[0] {
                continue;
            }
            let Some((name, all)) = art::ALL.iter().find(|(_, all)| all.contains(&frames[0])) else { continue };
            let frame = (frames.len() == 1 && all.len() > 1).then(|| all.iter().position(|a| *a == frames[0])).flatten();
            let open = prop_kind(name).map_or(0, |k| k.open);
            let line = PropLine::Sprite { name: name.to_string(), at: [tile(x), tile(y)], frame, fps: 0.0, open, tough: Tough::default() };
            let [x, y, pw, ph] = line.rect();
            if x > 0 && y > 0 && x + pw < w && y + ph < h {
                props.push((0, line));
            }
        }
        let at = |[x, y]: [f32; 2]| [tile(x), tile(y)];
        Self { name: name.into(), size, spawn: at(map.spawn), home: at(map.home), scatter: false, ground: (0..w * h).map(code).collect(), props }
    }
}

/// The most frames a prop line can pick among.
const MAX_FRAME_INDEX: usize = 64;

/// The smallest map the random generator lays a whole farm out on.
pub const RANDOM_MIN: [usize; 2] = [40, 30];

/// One line of the props section, read but not yet placed.
fn prop_line(words: &[&str]) -> Result<PropLine, String> {
    if words[0] == "fence" {
        let usage = "a fence is rail or picket, then x, y, width and height, then maybe gate and a column, hp and a number, and cover yes or no";
        let picket = match words.get(1) {
            Some(&"rail") => false,
            Some(&"picket") => true,
            _ => return Err("a fence is rail or picket".into()),
        };
        let r = words.get(2..6).and_then(numbers).ok_or(usage)?;
        let (mut gate, mut tough) = (None, Tough::default());
        for pair in words.get(6..).unwrap_or_default().chunks(2) {
            match (pair[0], pair.get(1)) {
                ("gate", Some(g)) => gate = Some(g.parse::<u16>().map_err(|_| usage)? as usize),
                (key, value) if tough.read(key, value.copied())? => {}
                _ => return Err(usage.into()),
            }
        }
        return Ok(PropLine::Fence { picket, rect: [r[0], r[1], r[2], r[3]], gate, tough });
    }
    if !words[0].bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_') {
        return Err(format!("{} isn't a sprite name", words[0]));
    }
    // Only the frame count is checked here: a player's prop may not be loaded yet when its map is read.
    let frames = sprite(words[0]).map_or(usize::MAX, <[Art]>::len);
    let at = words.get(1..3).and_then(numbers).ok_or("a prop's name is followed by its x and y")?;
    let (mut frame, mut fps, mut open, mut tough) = (None, 0.0, 0, Tough::default());
    for pair in words[3..].chunks(2) {
        if tough.read(pair[0], pair.get(1).copied())? {
            continue;
        }
        let value = pair.get(1).and_then(|v| v.parse::<f32>().ok()).filter(|v| v.is_finite() && *v >= 0.0);
        match (pair[0], value) {
            ("frame", Some(k)) if (k as usize) < frames.min(MAX_FRAME_INDEX) => frame = Some(k as usize),
            ("fps", Some(v)) => fps = v.min(60.0),
            ("open", Some(v)) => open = (v as usize).min(64),
            (word, _) => return Err(format!("{word} after a prop is frame, fps, open or hp with a number in range, or cover yes or no")),
        }
    }
    Ok(PropLine::Sprite { name: words[0].to_string(), at: [at[0], at[1]], frame, fps, open, tough })
}

impl Map {
    /// Reads and builds a map file, refusing one that doesn't work; `rng` drives the scatter.
    pub fn from_text(text: &str, rng: &mut Rng) -> Result<Map, String> {
        Map::from_source(&MapSource::parse(text)?, rng, true)
    }

    /// Builds a map. A `strict` build refuses a prop past the edge and a spawn or home that's blocked or can't
    /// reach the other; otherwise those props are left out and the rest let be, as an editor needs while a map
    /// is half made.
    pub fn from_source(src: &MapSource, rng: &mut Rng, strict: bool) -> Result<Map, String> {
        let [w, h] = src.size;
        let mut map = Map::blank(w, h);
        map.lay_ground(&src.ground);
        map.border();
        for (n, p) in &src.props {
            if let Err(e) = map.place(p)
                && strict
            {
                return Err(if *n > 0 { format!("{n}: {e}") } else { e });
            }
        }
        if src.scatter {
            map.scatter(rng);
        }
        map.keep_barrels_apart();
        let centre = |[x, y]: [usize; 2]| [(x.min(w - 1) as f32 + 0.5) * TILE, (y.min(h - 1) as f32 + 0.5) * TILE];
        (map.spawn, map.home) = (centre(src.spawn), centre(src.home));
        if strict {
            map.check_ends(src)?;
        }
        Ok(map)
    }

    /// What keeps a built map from playing: a spawn or home on the edge or under something, or no way between them.
    pub fn check_ends(&self, src: &MapSource) -> Result<(), String> {
        let [w, h] = src.size;
        for (what, [x, y]) in [("spawn", src.spawn), ("home", src.home)] {
            if x == 0 || y == 0 || x + 1 >= w || y + 1 >= h || self.solid[self.i(x, y)] {
                return Err(format!("the {what} at {x} {y} is on the edge or under something solid"));
            }
        }
        if self.reach(self.home)[self.i(src.spawn[0], src.spawn[1])] == u16::MAX {
            return Err("there's no way from the spawn to home".into());
        }
        Ok(())
    }

    fn blank(w: usize, h: usize) -> Map {
        Map {
            w,
            h,
            ground: vec![art::DIRT[0]; w * h],
            overlay: vec![None; w * h],
            solid: vec![false; w * h],
            cover: vec![false; w * h],
            owner: vec![super::NO_OWNER; w * h],
            cost: vec![1; w * h],
            cells: vec![Cell::Open; w * h],
            props: Vec::new(),
            decals: Vec::new(),
            barrels: Vec::new(),
            spawn: [0.0; 2],
            home: [0.0; 2],
        }
    }

    /// Ground, roads, paths and corn from one code per cell. Variants are picked by position, so a map looks
    /// the same every run.
    fn lay_ground(&mut self, codes: &[u8]) {
        let (w, h) = (self.w, self.h);
        let code = |x: usize, y: usize| if x < w && y < h { codes[y * w + x] } else { 0 };
        for y in 0..h {
            for x in 0..w {
                let i = self.i(x, y);
                let pick = (x * 7 + y * 13) % 2;
                let (left, right, up, down) = (code(x.wrapping_sub(1), y), code(x + 1, y), code(x, y.wrapping_sub(1)), code(x, y + 1));
                let lane = |before: u8, after: u8, c: u8| if before != c { 0 } else if after != c { 2 } else { 1 };
                self.ground[i] = match code(x, y) {
                    b',' => art::DIRT_TUFTS[pick],
                    b'g' => art::GRASS[pick],
                    b's' | b'c' => art::SOIL[pick],
                    b'p' => art::SPROUTS[pick],
                    b'v' => art::ROAD_V[lane(left, right, b'v')],
                    b'h' => art::ROAD_H[lane(up, down, b'h')],
                    _ => art::DIRT[pick],
                };
                match code(x, y) {
                    b'v' | b'h' => self.cells[i] = Cell::Road,
                    b'=' => self.cells[i] = Cell::Path,
                    b'c' => {
                        self.overlay[i] = Some(match (up == b'c', down == b'c') {
                            (false, _) => art::CORN_TOP[pick],
                            (_, false) => art::CORN_BOTTOM,
                            _ => art::CORN,
                        });
                        self.cells[i] = Cell::Taken;
                    }
                    b's' | b'p' => self.cells[i] = Cell::Taken,
                    _ => {}
                }
            }
        }
        self.autotile_paths();
    }

    /// Places one prop line, or says why it can't go there.
    fn place(&mut self, p: &PropLine) -> Result<(), String> {
        let [x, y, w, h] = p.rect();
        match p {
            PropLine::Fence { picket, gate, tough, .. } => {
                if w < 4 || h < 3 || x == 0 || y == 0 || x + w >= self.w || y + h >= self.h {
                    return Err(format!("a fence at {x} {y} is at least 4x3 and inside the edge"));
                }
                if gate.is_some_and(|g| !(1..=w - 3).contains(&g)) {
                    return Err("a gate sits inside the fence, 1 to width - 3".into());
                }
                let mut post = 0;
                self.fence_gated(if *picket { &PICKET } else { &RAIL }, [x, y, w, h], gate.map(|g| g + x), Toughness::fence().with(*tough), |n| {
                    post += 1;
                    post % n
                });
            }
            PropLine::Sprite { name, frame, fps, open, tough, .. } => {
                if x == 0 || y == 0 || x + w >= self.w || y + h >= self.h {
                    return Err(format!("{name} at {x} {y} reaches the edge"));
                }
                let frames = sprite(name).ok_or_else(|| format!("no sprite called {name}"))?;
                let frames = match frame {
                    Some(k) => &frames[(*k).min(frames.len() - 1)..][..1],
                    None => frames,
                };
                let fps = if *fps == 0.0 && frame.is_none() { super::super::props::prop(name).map_or(0.0, |p| p.fps) } else { *fps };
                self.prop_as(frames, fps, x, y, *open, toughness(frames[0]).with(*tough));
                if frames[0] == art::WINDMILL {
                    let (px, py) = (x as f32 * TILE, y as f32 * TILE - 6.0);
                    let base = py + 6.0 + art::WINDMILL.h() + 0.1;
                    self.props.push(Prop::new(&art::WINDMILL_FAN, 8.0, px, py, base, 2.0));
                }
            }
        }
        Ok(())
    }
}

/// A map file as the game keeps it, if it reads: its id is the file name without `EXTENSION`, and its name the
/// `name` line's, else the id.
pub fn check(file: &str, text: String) -> Result<LoadedMap, String> {
    let fail = |e: &dyn std::fmt::Display| format!("{file}: {e}");
    let id = file.strip_suffix(EXTENSION).ok_or_else(|| fail(&format!("a map's file name ends in {EXTENSION}")))?;
    if !valid_id(id) {
        return Err(fail(&format!("a map's file name is 1 to {} lowercase letters, digits and _ before {EXTENSION}", crate::PLAYER_ID_MAX)));
    }
    if text.len() > MAX_TEXT {
        return Err(fail(&"bigger than a map can be"));
    }
    let src = MapSource::parse(&text).map_err(|e| fail(&e))?;
    Map::from_source(&src, &mut Rng::new(0), true).map_err(|e| fail(&e))?;
    let name = if src.name.is_empty() { id.to_string() } else { src.name };
    Ok(LoadedMap { id: id.to_string(), name, text })
}

/// Whether `id` can name a map: what a save keeps inline.
pub fn valid_id(id: &str) -> bool {
    (1..=crate::PLAYER_ID_MAX).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// Whole numbers small enough that adding a prop's size to one can't overflow.
fn numbers(words: &[&str]) -> Option<Vec<usize>> {
    words.iter().map(|w| w.parse::<u16>().ok().map(usize::from)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn farm(props: &str) -> String {
        let mut rows = Vec::new();
        for y in 0..14 {
            let row: String = (0..20)
                .map(|x| match (x, y) {
                    (9..=11, _) => 'v',
                    (5..=8, 9) => '=',
                    (14..=17, 3..=6) => 'c',
                    _ => '.',
                })
                .collect();
            rows.push(row);
        }
        format!("name Test\nsize 20 14\nspawn 4 11\nhome 6 10\nground\n{}\nprops\n{props}\n", rows.join("\n"))
    }

    fn read(text: &str) -> Result<Map, String> {
        Map::from_text(text, &mut Rng::new(1))
    }

    #[test]
    fn a_map_file_lays_its_ground_and_props() {
        let map = read(&farm("HOUSE 2 2 open 1\nBARREL 13 10\nfence rail 13 2 6 6 gate 2\nWINDMILL 3 8")).unwrap();
        assert_eq!((map.w, map.h), (20, 14));
        assert_eq!(map.tile(9, 5).0, art::ROAD_V[0]);
        assert_eq!(map.tile(10, 5).0, art::ROAD_V[1]);
        assert_eq!(map.tile(11, 5).0, art::ROAD_V[2]);
        assert!(art::PATH.contains(&map.tile(6, 9).0), "paths autotile");
        assert_eq!(map.tile(15, 3).1, Some(art::CORN_TOP[(15 * 7 + 3 * 13) % 2]));
        assert_eq!(map.tile(15, 6).1, Some(art::CORN_BOTTOM));
        assert!(map.props.iter().any(|p| p.frames[0] == art::HOUSE));
        assert!(map.props.iter().any(|p| p.frames[0] == art::WINDMILL_FAN[0] && p.fps > 0.0), "a windmill brings its fan");
        assert_eq!(map.barrels, vec![[13.0 * TILE, 10.0 * TILE]], "barrels blow up, so they're kept apart");
        assert!(map.solid_at(0.5 * TILE, 5.5 * TILE), "the edge is always solid");
        assert_eq!(map.spawn, [4.5 * TILE, 11.5 * TILE]);
        assert!(!map.solid_at(15.5 * TILE, 7.5 * TILE), "the gate is open");
        assert!(map.solid_at(14.5 * TILE, 7.5 * TILE), "the rest of the fence's bottom isn't");
    }

    #[test]
    fn a_prop_stops_shots_as_its_sprite_does_unless_its_line_says() {
        let map = read(&farm("HOUSE 2 2\nBUSH 13 10\nTANK 15 10 cover no\nBUSH 17 10 cover yes\nfence rail 13 2 6 6 gate 2 cover yes\nfence picket 2 7 4 3")).unwrap();
        let walk = |x: usize, y: usize| map.solid_at((x as f32 + 0.5) * TILE, (y as f32 + 0.5) * TILE);
        let shot = |x: usize, y: usize| map.cover_at((x as f32 + 0.5) * TILE, (y as f32 + 0.5) * TILE);
        assert!(walk(3, 4) && shot(3, 4), "a house stops both");
        assert!(walk(13, 10) && !shot(13, 10), "a bush lets shots through");
        assert!(walk(15, 10) && !shot(15, 10) && shot(17, 10), "lines say otherwise");
        assert!(walk(13, 4) && shot(13, 4), "a fence that says it's cover");
        assert!(walk(2, 8) && !shot(2, 8), "and one that doesn't");
        assert!(map.cover_at(-1.0, 5.0 * TILE), "nothing flies off the map");
    }

    #[test]
    fn a_source_writes_back_what_it_read() {
        let text = farm("HOUSE 2 2 open 1 hp 400 cover no\nDEAD_TREE 5 5 frame 2\nfence picket 13 2 6 6 gate 2 hp 0 cover yes\nfence rail 2 7 4 3").replace("ground", "scatter\nground");
        let src = MapSource::parse(&text).unwrap();
        let lines = |s: &MapSource| s.props.iter().map(|(_, p)| p.clone()).collect::<Vec<_>>();
        assert_eq!(lines(&MapSource::parse(&src.to_text()).unwrap()), lines(&src));
        assert!(src.to_text().contains("HOUSE 2 2 open 1 hp 400 cover no\n") && src.to_text().contains("gate 2 hp 0 cover yes\n"), "{}", src.to_text());
        assert_eq!(MapSource::parse(&src.to_text()).unwrap().ground, src.ground);
        assert_eq!(src.props[1].1, PropLine::Sprite { name: "DEAD_TREE".into(), at: [5, 5], frame: Some(2), fps: 0.0, open: 0, tough: Tough::default() });
        let grown = src.resized([3, -2]);
        assert_eq!(grown.size, [23, 12]);
        assert_eq!(grown.ground[22], b'.', "new ground is dirt");
        assert!(grown.props.iter().all(|(_, p)| p.rect()[1] + p.rect()[3] < 12), "props past the new edge are dropped");
        MapSource::parse(&grown.to_text()).unwrap();
    }

    #[test]
    fn a_loose_build_lets_a_half_made_map_draw() {
        let mut src = MapSource::parse(&farm("HOUSE 15 3")).unwrap();
        assert!(Map::from_source(&src, &mut Rng::new(1), true).is_err());
        src.spawn = [0, 0];
        Map::from_source(&src, &mut Rng::new(1), false).unwrap();
    }

    #[test]
    fn the_same_map_without_scatter_is_the_same_every_time() {
        let text = farm("HOUSE 2 2");
        let (a, b) = (Map::from_text(&text, &mut Rng::new(1)).unwrap(), Map::from_text(&text, &mut Rng::new(2)).unwrap());
        assert_eq!(a.ground, b.ground);
        assert_eq!(a.props.len(), b.props.len());
        let scattered = Map::from_text(&text.replace("ground", "scatter\nground"), &mut Rng::new(1)).unwrap();
        assert!(scattered.props.len() + scattered.barrels.len() > a.props.len(), "scatter adds to the open ground");
    }

    #[test]
    fn mistakes_name_their_line() {
        let err = |text: &str| read(text).err().expect("an error");
        assert!(err(&farm("NOPE 3 3")).starts_with("21: no sprite called NOPE"));
        assert!(err(&farm("HOUSE 3")).contains("x and y"));
        assert!(err(&farm("HOUSE 15 3")).starts_with("21: HOUSE at 15 3 reaches the edge"));
        assert!(err(&farm("BARREL 3 3 spin 2")).contains("frame, fps, open or hp"));
        for bad in ["hp -1", "hp 1001", "hp nan", "hp inf", "hp"] {
            assert!(err(&farm(&format!("SIGN 3 3 {bad}"))).starts_with("21: hp is a number from 0 to 1000"), "{bad}");
            assert!(err(&farm(&format!("fence rail 2 2 4 4 {bad}"))).starts_with("21: hp is a number"), "{bad}");
        }
        assert!(err(&farm("fence wood 2 2 4 4")).contains("rail or picket"));
        assert!(err(&farm("fence rail 2 2 4 4 gate 3")).contains("inside the fence"));
        assert!(err(&farm("BARREL 3 3 cover maybe")).starts_with("21: cover is yes or no"));
        assert!(err(&farm("fence rail 2 2 4 4 cover")).contains("cover is yes or no"));
        assert!(err(&farm("").replace("size 20 14", "size 8 8")).contains("16x12"));
        assert!(err(&farm("").replace("size 20 14", "size 20 15")).contains("14 rows"));
        assert!(err(&farm("").replacen("....", "..x.", 1)).contains("x isn't a ground code"));
        assert!(err(&farm("").replace("spawn 4 11\n", "")).contains("no spawn"));
        assert!(err(&farm("").replace("spawn 4 11", "spawn 0 5")).contains("edge"));
        assert!(err(&farm("").replace("name Test", "wings 2")).starts_with("1: wings"));
    }

    #[test]
    fn mangled_maps_are_refused_without_panicking() {
        let good = farm("HOUSE 2 2 open 1 cover no\nWINDMILL 13 9 fps 8 hp 30\nfence picket 13 2 6 6 gate 2 hp 50 cover yes\nDEAD_TREE 5 5 frame 2");
        read(&good).unwrap();
        let pieces = ["99999", "65535", "1001", "0", "-1", "nan", "inf", "fence", "gate", "frame", "hp", "cover", "yes", "no", "\n", "props", "ground", "c", "v", "=", " ", "#"];
        let mut rng = Rng::new(42);
        for _ in 0..4000 {
            let mut text = good.clone();
            for _ in 0..1 + rng.below(3) {
                let at = rng.below(text.len());
                let at = (0..=at).rev().find(|&i| text.is_char_boundary(i)).unwrap_or(0);
                match rng.below(3) {
                    0 => text.insert_str(at, pieces[rng.below(pieces.len())]),
                    1 => {
                        let end = (at + 1 + rng.below(6)).min(text.len());
                        let end = (end..=text.len()).find(|&i| text.is_char_boundary(i)).unwrap_or(text.len());
                        text.replace_range(at..end, "");
                    }
                    _ => text.insert(at, (b' ' + rng.below(95) as u8) as char),
                }
            }
            let _ = Map::from_text(&text, &mut Rng::new(rng.below(1000) as u64));
            if let Ok(src) = MapSource::parse(&text) {
                let _ = Map::from_source(&src, &mut Rng::new(1), false);
                let _ = src.resized([rng.below(9) as i32 - 4, rng.below(9) as i32 - 4]);
            }
        }
    }

    #[test]
    fn zombies_find_their_way_through_a_fence_and_the_player_round_it() {
        let pen = |line: &str| read(&farm(&format!("fence rail 13 2 6 6 {line}"))).unwrap();
        let (map, at) = (pen(""), [15.5 * TILE, 4.5 * TILE]);
        let cell = |x: usize, y: usize| y * map.w + x;
        assert_eq!(map.reach(at)[cell(15, 9)], u16::MAX, "no gate: the player can't get in");
        let flow = map.flow(at);
        assert!(flow[cell(15, 9)] < u16::MAX && flow[cell(15, 9)] > 4, "zombies chew in, at a cost");
        assert!(map.downhill(&flow, [15.5 * TILE, 8.5 * TILE]).unwrap()[1] < -0.9, "straight at the fence");
        assert_eq!(pen("hp 0").flow(at)[cell(15, 9)], u16::MAX, "a fence that doesn't break is a wall");
        let gated = pen("gate 2");
        assert_eq!(gated.flow(at)[cell(15, 8)], gated.reach(at)[cell(15, 8)], "an open gate beats chewing");
        assert!(read(&farm("fence rail 2 1 7 7 hp 999").replace("home 6 10", "home 4 4")).is_err(), "a home the player can't walk to is refused");
    }

    #[test]
    fn a_line_says_its_hit_points_and_cover_over_its_sprites() {
        let mut src = MapSource::parse(&farm("SIGN 3 3\nHOUSE 2 2 hp 300\nfence rail 13 2 6 6 cover yes")).unwrap();
        let got = |s: &MapSource| s.props.iter().map(|(_, p)| p.hp_and_cover()).collect::<Vec<_>>();
        assert_eq!(got(&src), [(20.0, false), (300.0, true), (40.0, true)]);
        src.props[0].1.tough_mut().cover = Some(true);
        *src.props[1].1.tough_mut() = Tough::default();
        assert_eq!(got(&src)[..2], [(20.0, true), (0.0, true)]);
    }

    #[test]
    fn a_broken_prop_opens_its_cells_and_shows_its_broken_frame_or_goes() {
        test_props();
        let mut map = read(&farm("PROP_CRATE_TEST 3 3\nSIGN 5 3\nSIGN 6 3 hp 0")).unwrap();
        let (krate, sign) = ([3.5 * TILE, 3.5 * TILE], [5.5 * TILE, 3.5 * TILE]);
        let i = map.breakable_at(krate[0], krate[1]).expect("the crate breaks");
        assert_eq!(map.props[i].frames, &art::LOG[..2], "the broken frame doesn't animate with the rest");
        assert!(map.breakable_at(6.5 * TILE, 3.5 * TILE).is_none(), "hp 0 doesn't break");
        map.hurt_prop(i, 9.0, 0.1);
        assert!(map.crumble(0.0).is_none() && map.props[i].flash > 0.0);
        map.hurt_prop(i, 1.0, 0.1);
        assert!(map.crumble(0.0).is_some());
        assert!(!map.solid_at(krate[0], krate[1]) && map.breakable_at(krate[0], krate[1]).is_none());
        assert_eq!(map.props[i].frames, &art::LOG[2..], "it lies broken");
        let (props, j) = (map.props.len(), map.breakable_at(sign[0], sign[1]).expect("a sign breaks"));
        map.hurt_prop(j, 100.0, 0.0);
        map.crumble(0.0).unwrap();
        assert!(map.props.len() == props - 1 && !map.solid_at(sign[0], sign[1]), "a sign without a broken frame goes");
        assert!(map.breakable_at(krate[0], krate[1]).is_none() && map.solid_at(6.5 * TILE, 3.5 * TILE));
    }

    #[test]
    fn a_walled_in_spawn_is_refused() {
        let wall: String = (1..13).map(|y| format!("BUSH 3 {y}\n")).collect();
        let text = farm(&wall).replace("spawn 4 11", "spawn 1 11");
        assert!(read(&text).err().unwrap().contains("no way from the spawn"));
    }

    #[test]
    fn a_random_farm_reads_as_a_map_that_plays() {
        for (seed, size) in [(1, [72, 54]), (2, [40, 30]), (3, [160, 120]), (4, [16, 12]), (5, [100, 40])] {
            let src = MapSource::random("Farm", size, seed);
            assert!(src.size[0] >= RANDOM_MIN[0] && src.size[1] >= RANDOM_MIN[1]);
            let again = MapSource::parse(&src.to_text()).unwrap();
            let map = Map::from_source(&again, &mut Rng::new(1), true).unwrap_or_else(|e| panic!("seed {seed} {size:?}: {e}"));
            assert!(src.props.len() > 20, "seed {seed}: buildings, fences and scatter come through");
            assert!(src.ground.contains(&b'v') || src.ground.contains(&b'h'), "and the road");
            assert!(!map.barrels.is_empty() || !src.props.iter().any(|(_, p)| matches!(p, PropLine::Sprite { name, .. } if name == "BARREL")));
        }
        assert_ne!(MapSource::random("Farm", [72, 54], 1), MapSource::random("Farm", [72, 54], 2));
    }

    #[test]
    fn random_farms_build_at_every_size_the_editor_offers() {
        let mut rng = Rng::new(9);
        for seed in 0..300 {
            let size = [RANDOM_MIN[0] + rng.below(MAX_SIZE[0] - RANDOM_MIN[0] + 1), RANDOM_MIN[1] + rng.below(MAX_SIZE[1] - RANDOM_MIN[1] + 1)];
            let src = MapSource::random("Farm", size, seed);
            Map::from_source(&src, &mut Rng::new(1), true).unwrap_or_else(|e| panic!("seed {seed} {size:?}: {e}"));
        }
    }

    #[test]
    fn a_map_reads_before_its_players_props_load() {
        let src = MapSource::parse(&farm("PROP_LATER 3 3 frame 1")).unwrap();
        assert!(Map::from_source(&src, &mut Rng::new(1), true).err().unwrap().contains("no sprite called PROP_LATER"));
        assert!(MapSource::parse(&farm("prop_lower 3 3")).unwrap_err().contains("isn't a sprite name"));
    }

    /// Sets the same players' props for every test that needs some, since tests run side by side.
    fn test_props() {
        let read = super::super::super::props::prop_fields;
        let tank = read("[map_test_tank]\nfps 6\nstand slab 4").unwrap();
        let krate = read("[crate_test]\nframes 3\nhp 10\nbroken yes\ncover no").unwrap();
        super::super::super::props::set_props(vec![(tank, art::WINDMILL_FAN.to_vec()), (krate, art::LOG.to_vec())]);
    }

    #[test]
    fn a_map_places_a_players_prop_by_name() {
        test_props();
        let map = read(&farm("PROP_MAP_TEST_TANK 3 3")).unwrap();
        let p = map.props.iter().find(|p| p.frames[0] == art::WINDMILL_FAN[0]).expect("placed");
        assert_eq!(p.fps, 6.0, "it turns at the prop's own pace");
        assert!(read(&farm("PROP_NOT_LOADED 3 3")).err().unwrap().contains("no sprite called PROP_NOT_LOADED"));
    }

    #[test]
    fn every_listed_prop_is_a_sprite() {
        assert!(props().iter().all(|k| sprite(k.name).is_some()));
        assert_eq!((book().fence.cover, book().fence.hp), (false, 40.0));
        assert!(parse_props("[fence]\n[other]\n[A]\nopen 99").unwrap_err().starts_with("4: open"));
        assert!(parse_props("[fence]\n[A]").unwrap_err().contains("[other]"));
    }
}
