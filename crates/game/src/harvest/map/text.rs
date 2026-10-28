//! Hand-made maps: a `.map.txt` file read into the same `Map` the generator builds.
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
//! WINDMILL_FAN 30 4 fps 8
//! DEAD_TREE 5 5 frame 2
//! fence rail 4 20 10 6 gate 3
//! ```
//!
//! Ground codes: `.` dirt, `,` dirt with tufts, `g` grass, `s` soil, `p` sprouts, `c` corn, `v` a road running
//! north and south, `h` one running east and west, `=` a path. Roads and paths join up with their neighbours, and
//! corn grows tops and bottoms at a field's edges. A prop is a sprite's name and the tile its top-left sits on;
//! every row of it but the top `open` ones blocks movement. The edge is always a ring of trees and bushes, and
//! `scatter` sprinkles the rest of the open ground as the random farm does, differently each run.

use super::{Cell, Map, PICKET, RAIL, TILE};
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

/// The sections of a map file, in order.
#[derive(PartialEq)]
enum Section {
    Header,
    Ground,
    Props,
}

impl Map {
    /// Reads a map file; `rng` drives the scatter, if the map asks for it. Errors name the line.
    pub fn from_text(text: &str, rng: &mut Rng) -> Result<Map, String> {
        let mut size = None;
        let (mut spawn, mut home, mut scatter) = (None, None, false);
        let mut rows: Vec<(usize, &str)> = Vec::new();
        let mut props: Vec<(usize, Vec<&str>)> = Vec::new();
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
                (_, Section::Props) => props.push((n, words)),
                ("name", _) => {}
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
        let mut codes = vec![b'.'; w * h];
        for (y, (n, row)) in rows.iter().enumerate() {
            if row.len() != w || !row.is_ascii() {
                return Err(format!("{n}: a ground row is {w} codes, one per cell"));
            }
            for (x, c) in row.bytes().enumerate() {
                if !b".,gspcvh=".contains(&c) {
                    return Err(format!("{n}: {} isn't a ground code", c as char));
                }
                codes[y * w + x] = c;
            }
        }
        let mut map = Map::blank(w, h);
        map.lay_ground(&codes);
        map.border();
        for (n, words) in &props {
            map.place_line(words).map_err(|e| format!("{n}: {e}"))?;
        }
        if scatter {
            map.scatter(rng);
        }
        map.props.retain(|p| {
            let barrel = p.frames[0] == art::BARREL;
            if barrel {
                map.barrels.push([p.x, p.y]);
            }
            !barrel
        });
        let spawn = spawn.ok_or("no spawn line")?;
        let home = home.ok_or("no home line")?;
        for (what, [x, y]) in [("spawn", spawn), ("home", home)] {
            if x == 0 || y == 0 || x + 1 >= w || y + 1 >= h || map.solid[map.i(x, y)] {
                return Err(format!("the {what} at {x} {y} is on the edge or under something solid"));
            }
        }
        let centre = |[x, y]: [usize; 2]| [(x as f32 + 0.5) * TILE, (y as f32 + 0.5) * TILE];
        (map.spawn, map.home) = (centre(spawn), centre(home));
        if map.flow(map.home)[map.i(spawn[0], spawn[1])] == u16::MAX {
            return Err("there's no way from the spawn to home".into());
        }
        Ok(map)
    }

    fn blank(w: usize, h: usize) -> Map {
        Map {
            w,
            h,
            ground: vec![art::DIRT[0]; w * h],
            overlay: vec![None; w * h],
            solid: vec![false; w * h],
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

    /// One line of the props section: a sprite and where, or a fence.
    fn place_line(&mut self, words: &[&str]) -> Result<(), String> {
        if words[0] == "fence" {
            return self.fence_line(&words[1..]);
        }
        let (_, frames) = art::ALL.iter().find(|(n, _)| *n == words[0]).ok_or_else(|| format!("no sprite called {}", words[0]))?;
        let at = words.get(1..3).and_then(numbers).ok_or("a prop's name is followed by its x and y")?;
        let (mut frames, mut fps, mut open): (&'static [Art], f32, usize) = (frames, 0.0, 0);
        for pair in words[3..].chunks(2) {
            let value = pair.get(1).and_then(|v| v.parse::<f32>().ok()).filter(|v| v.is_finite() && *v >= 0.0);
            match (pair[0], value) {
                ("frame", Some(k)) if (k as usize) < frames.len() => frames = &frames[k as usize..k as usize + 1],
                ("fps", Some(v)) => fps = v.min(60.0),
                ("open", Some(v)) => open = v as usize,
                (word, _) => return Err(format!("{word} after a prop is frame, fps or open with a number in range")),
            }
        }
        let [x, y] = [at[0], at[1]];
        let (tw, th) = (super::tiles(frames[0].w()), super::tiles(frames[0].h()));
        if x == 0 || y == 0 || x + tw >= self.w || y + th >= self.h {
            return Err(format!("{} at {x} {y} reaches the edge", words[0]));
        }
        self.prop(frames, fps, x, y, open);
        Ok(())
    }

    /// `rail|picket x y w h [gate g]`: a fence round the rect, with a two-tile gate `g` tiles from its left.
    fn fence_line(&mut self, words: &[&str]) -> Result<(), String> {
        let style = match words.first() {
            Some(&"rail") => &RAIL,
            Some(&"picket") => &PICKET,
            _ => return Err("a fence is rail or picket".into()),
        };
        let usage = "a fence is its style, x, y, width and height, then maybe gate and a column";
        let rect = words.get(1..5).and_then(numbers).ok_or(usage)?;
        let [x, y, w, h] = [rect[0], rect[1], rect[2], rect[3]];
        if w < 4 || h < 3 || x == 0 || y == 0 || x + w >= self.w || y + h >= self.h {
            return Err(format!("a fence at {x} {y} is at least 4x3 and inside the edge"));
        }
        let gate = match words.get(5..) {
            Some([]) | None => None,
            Some(["gate", g]) => Some(g.parse::<usize>().ok().filter(|g| (1..=w - 3).contains(g)).ok_or("a gate sits inside the fence, 1 to width - 3")? + x),
            _ => return Err(usage.into()),
        };
        let mut post = 0;
        self.fence_gated(style, [x, y, w, h], gate, |n| {
            post += 1;
            post % n
        });
        Ok(())
    }
}

/// Whole numbers small enough that adding a prop's size to one can't overflow.
/// A map file as the game keeps it, if it reads: its id is the file name without `EXTENSION`, and its name the
/// `name` line's, else the id.
pub fn check(file: &str, text: String) -> Result<LoadedMap, String> {
    let fail = |e: &dyn std::fmt::Display| format!("{file}: {e}");
    let id = file.strip_suffix(EXTENSION).ok_or_else(|| fail(&format!("a map's file name ends in {EXTENSION}")))?;
    let fits = (1..=crate::PLAYER_ID_MAX).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    if !fits {
        return Err(fail(&format!("a map's file name is 1 to {} lowercase letters, digits and _ before {EXTENSION}", crate::PLAYER_ID_MAX)));
    }
    if text.len() > MAX_TEXT {
        return Err(fail(&"bigger than a map can be"));
    }
    Map::from_text(&text, &mut Rng::new(0)).map_err(|e| fail(&e))?;
    let name = text.lines().map(|l| l.split('#').next().unwrap_or("").trim()).find_map(|l| l.strip_prefix("name ")).map(str::trim);
    let name = name.filter(|n| !n.is_empty()).unwrap_or(id).chars().take(40).collect();
    Ok(LoadedMap { id: id.to_string(), name, text })
}

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
        let map = read(&farm("HOUSE 2 2 open 1\nBARREL 13 10\nfence rail 13 2 6 6 gate 2")).unwrap();
        assert_eq!((map.w, map.h), (20, 14));
        assert_eq!(map.tile(9, 5).0, art::ROAD_V[0]);
        assert_eq!(map.tile(10, 5).0, art::ROAD_V[1]);
        assert_eq!(map.tile(11, 5).0, art::ROAD_V[2]);
        assert!(art::PATH.contains(&map.tile(6, 9).0), "paths autotile");
        assert_eq!(map.tile(15, 3).1, Some(art::CORN_TOP[(15 * 7 + 3 * 13) % 2]));
        assert_eq!(map.tile(15, 6).1, Some(art::CORN_BOTTOM));
        assert!(map.props.iter().any(|p| p.frames[0] == art::HOUSE));
        assert_eq!(map.barrels, vec![[13.0 * TILE, 10.0 * TILE]], "barrels blow up, so they're kept apart");
        assert!(map.solid_at(0.5 * TILE, 5.5 * TILE), "the edge is always solid");
        assert_eq!(map.spawn, [4.5 * TILE, 11.5 * TILE]);
        assert!(!map.solid_at(15.5 * TILE, 7.5 * TILE), "the gate is open");
        assert!(map.solid_at(14.5 * TILE, 7.5 * TILE), "the rest of the fence's bottom isn't");
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
        assert!(err(&farm("HOUSE 15 3")).contains("reaches the edge"));
        assert!(err(&farm("BARREL 3 3 spin 2")).contains("frame, fps or open"));
        assert!(err(&farm("fence wood 2 2 4 4")).contains("rail or picket"));
        assert!(err(&farm("fence rail 2 2 4 4 gate 3")).contains("inside the fence"));
        assert!(err(&farm("").replace("size 20 14", "size 8 8")).contains("16x12"));
        assert!(err(&farm("").replace("size 20 14", "size 20 15")).contains("14 rows"));
        assert!(err(&farm("").replacen("....", "..x.", 1)).contains("x isn't a ground code"));
        assert!(err(&farm("").replace("spawn 4 11\n", "")).contains("no spawn"));
        assert!(err(&farm("").replace("spawn 4 11", "spawn 0 5")).contains("edge"));
        assert!(err(&farm("").replace("name Test", "wings 2")).starts_with("1: wings"));
    }

    #[test]
    fn mangled_maps_are_refused_without_panicking() {
        let good = farm("HOUSE 2 2 open 1\nWINDMILL_FAN 13 9 fps 8\nfence picket 13 2 6 6 gate 2\nDEAD_TREE 5 5 frame 2");
        read(&good).unwrap();
        let pieces = ["99999", "65535", "0", "-1", "nan", "inf", "fence", "gate", "frame", "\n", "props", "ground", "c", "v", "=", " ", "#"];
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
        }
    }

    #[test]
    fn a_walled_in_spawn_is_refused() {
        let wall: String = (1..13).map(|y| format!("BUSH 3 {y}\n")).collect();
        let text = farm(&wall).replace("spawn 4 11", "spawn 1 11");
        assert!(read(&text).err().unwrap().contains("no way from the spawn"));
    }
}
