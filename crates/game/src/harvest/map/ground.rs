//! The ground a map paints, one code a cell, read from `ground.txt`.

use std::sync::RwLock;

use super::Cell;
use crate::art::{self, Art};

/// How a run of the same ground lays its art's first, middle and last frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Joins {
    Across,
    Down,
}

/// How a crop leans in the wind in 3D: at most `degrees` either way, once each `seconds`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sway {
    pub degrees: f32,
    pub seconds: f32,
}

/// The most a crop may lean, in degrees, and the quickest and slowest sway, in seconds.
const SWAY_MAX: f32 = 30.0;
const SWAY_SECONDS: [f32; 2] = [0.2, 60.0];

impl Sway {
    /// How far a crop on cell (x, y) leans at `t` seconds, in radians, east positive. A wave runs across the field
    /// so neighbours lean at different moments, and a slower gust swells and eases it; never past `degrees`.
    pub fn lean(self, t: f32, x: usize, y: usize) -> f32 {
        if self.degrees == 0.0 || self.seconds <= 0.0 {
            return 0.0;
        }
        let tau = std::f32::consts::TAU;
        let (x, y) = (x as f32, y as f32);
        let wave = (tau * t / self.seconds - x * 0.7 - y * 0.35).sin();
        let gust = 0.65 + 0.35 * (tau * t / (self.seconds * 5.3) - (x + y) * 0.12).sin();
        self.degrees.to_radians() * gust * wave
    }
}

/// A crop's pieces: its north end's frames, its middle, and its south end.
pub type Pieces = (&'static [Art], Art, Art);

/// The most cards a crop stands one above another in 3D.
pub const TALL_MAX: u8 = 6;

/// A crop standing on one cell in 3D: a column of `tall` cards, `lower` ones under a `top`, swaying as one stalk.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stalk {
    pub lower: Art,
    pub top: Art,
    pub tall: u8,
    pub sway: Sway,
}

impl Stalk {
    /// The column for a cell of a run: with `tall` 1, the piece the cell's place picks, as `crop_piece` does; taller,
    /// the top's frame `pick` caps cards of the cell's piece, or of the middle at the run's north end, which the cap
    /// already ends.
    pub fn of(pieces: (&[Art], Art, Art), pick: usize, up: bool, down: bool, tall: u8, sway: Sway) -> Self {
        let (top, middle, bottom) = pieces;
        let crown = top[pick % top.len()];
        let piece = match (up, down) {
            (false, _) => crown,
            (_, false) => bottom,
            _ => middle,
        };
        let tall = tall.max(1);
        Self { lower: if tall > 1 && !up { middle } else { piece }, top: if tall > 1 { crown } else { piece }, tall, sway }
    }

    /// The cards from the ground up.
    pub fn cards(self) -> impl Iterator<Item = Art> {
        (0..self.tall).map(move |k| if k + 1 == self.tall { self.top } else { self.lower })
    }

    /// Each card's bottom edge as how high it is and how far east it leans, and how far east its top leans past
    /// that, in game pixels, at `t` seconds on cell (x, y). `ends` gives a card's clear rows under and over its
    /// picture: cards stack picture on picture, the first planted at the ground. The column bends as one stalk,
    /// leaning more the higher it goes, each picture's top where the next picture's bottom is.
    pub fn bend(self, t: f32, x: usize, y: usize, ends: impl Fn(Art) -> [f32; 2] + Copy) -> impl Iterator<Item = (Art, f32, f32, f32)> {
        let tan = self.sway.lean(t, x, y).tan();
        let seen = move |a: Art| {
            let [under, over] = ends(a);
            (under, (a.h() - under - over).max(0.0))
        };
        let total: f32 = self.cards().map(|a| seen(a).1).sum::<f32>().max(1.0);
        let off = move |h: f32| tan * h * h / total;
        let mut base = 0.0;
        self.cards().map(move |a| {
            let (under, tall) = seen(a);
            let slope = if tall > 0.0 { (off(base + tall) - off(base)) / tall } else { 0.0 };
            let card = (a, base - under, off(base) - slope * under, slope * a.h());
            base += tall;
            card
        })
    }
}

/// One kind of ground: its code, its paint button's label, its tile frames, and what the cell is.
#[derive(Clone, Debug)]
pub struct Ground {
    pub code: u8,
    pub name: String,
    pub art: &'static [Art],
    pub joins: Option<Joins>,
    pub(super) cell: Cell,
    /// A crop standing on it: its north end's frames, its middle, and its south end, as the map draws them.
    pub crop: Option<Pieces>,
    /// The crop's pieces in 3D, where they differ from the map's.
    pub crop_3d: Option<Pieces>,
    pub sway: Sway,
    /// Cards the crop stands one above another in 3D.
    pub tall: u8,
}

impl Ground {
    /// The tile at (x, y), picked between the art's frames by where it is, as every run picks the same.
    pub(super) fn tile(&self, x: usize, y: usize, before: bool, after: bool) -> Art {
        match self.joins {
            Some(_) => self.art[if !before { 0 } else if !after { 2 } else { 1 }.min(self.art.len() - 1)],
            None => self.art[(x * 7 + y * 13) % self.art.len()],
        }
    }

    /// The crop's pieces as they stand in 3D: its `crop_3d`, else its `crop`; none without a crop.
    pub fn standing(&self) -> Option<Pieces> {
        self.crop.and(self.crop_3d.or(self.crop))
    }

    /// The crop's column on cell (x, y) in 3D, where `up` and `down` say whether the same ground lies north and south.
    pub fn stalk(&self, x: usize, y: usize, up: bool, down: bool) -> Option<Stalk> {
        self.standing().map(|p| Stalk::of(p, x * 7 + y * 13, up, down, self.tall, self.sway))
    }
}

fn frames(name: &str) -> Option<&'static [Art]> {
    art::ALL.iter().find(|(n, _)| *n == name).map(|(_, f)| *f).or_else(|| crate::harvest::props::prop(name).map(|p| p.frames))
}

/// The piece of a crop at (x, y): TOP at a run's north end, picking between its frames by position, BOTTOM at
/// its south end, MIDDLE between; `up` and `down` say whether the same ground lies north and south.
pub fn crop_piece(top: &[Art], middle: Art, bottom: Art, x: usize, y: usize, up: bool, down: bool) -> Art {
    match (up, down) {
        (false, _) => top[(x * 7 + y * 13) % top.len()],
        (_, false) => bottom,
        _ => middle,
    }
}

/// Reads `ground.txt`; errors name the line.
pub fn parse(text: &str) -> Result<Vec<Ground>, String> {
    parse_noting(text).map(|(g, _)| g)
}

/// Reads `ground.txt`, and names each player's prop a crop asks for that isn't loaded; that crop is left off.
fn parse_noting(text: &str) -> Result<(Vec<Ground>, Vec<String>), String> {
    let (mut out, mut missing): (Vec<Ground>, Vec<String>) = (Vec::new(), Vec::new());
    // For each code, the line of a crop_3d it says and whether it says a crop.
    let mut said: Vec<(Option<usize>, bool)> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(code) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let &[c] = code.as_bytes() else { return Err(format!("{n}: a ground's code is one character, not [{code}]")) };
            if !c.is_ascii_graphic() || c == b'#' || c == b'[' {
                return Err(format!("{n}: [{code}] can't be a ground's code"));
            }
            if out.iter().any(|g| g.code == c) {
                return Err(format!("{n}: [{code}] appears twice"));
            }
            out.push(Ground { code: c, name: code.to_string(), art: &art::DIRT, joins: None, cell: Cell::Open, crop: None, crop_3d: None, sway: Sway::default(), tall: 1 });
            said.push((None, false));
            continue;
        }
        let line = line.split(" #").next().unwrap_or(line).trim();
        let Some(g) = out.last_mut() else { return Err(format!("{n}: a line before any [code]")) };
        let (key, value) = line.split_once(char::is_whitespace).map_or((line, ""), |(k, v)| (k, v.trim()));
        let art_of = |name: &str| frames(name).ok_or_else(|| format!("{n}: no sprite called {name}"));
        let mut pieces = |what: &str| -> Result<Option<Pieces>, String> {
            let words: Vec<&str> = value.split_whitespace().collect();
            let &[top, middle, bottom] = &words[..] else { return Err(format!("{n}: {what} is TOP MIDDLE BOTTOM, three sprites")) };
            let unloaded: Vec<String> = words.iter().filter(|w| w.starts_with(crate::harvest::props::PREFIX) && frames(w).is_none()).map(|w| w.to_string()).collect();
            let got = if unloaded.is_empty() { Some((art_of(top)?, art_of(middle)?[0], art_of(bottom)?[0])) } else { None };
            missing.extend(unloaded);
            Ok(got)
        };
        let at = said.len() - 1;
        match key {
            "name" => g.name = value.to_string(),
            "art" => g.art = art_of(value)?,
            "joins" => {
                g.joins = Some(match value {
                    "across" => Joins::Across,
                    "down" => Joins::Down,
                    _ => return Err(format!("{n}: joins is across or down")),
                })
            }
            "cell" => {
                g.cell = match value {
                    "open" => Cell::Open,
                    "road" => Cell::Road,
                    "path" => Cell::Path,
                    "field" => Cell::Taken,
                    _ => return Err(format!("{n}: cell is open, road, path or field")),
                }
            }
            "crop" => {
                g.crop = pieces("crop")?;
                said[at].1 = true;
            }
            "crop_3d" => {
                g.crop_3d = pieces("crop_3d")?;
                said[at].0 = Some(n);
            }
            "tall" => {
                g.tall = value.parse().ok().filter(|t| (1..=TALL_MAX).contains(t)).ok_or_else(|| format!("{n}: tall is 1 to {TALL_MAX} cards"))?;
            }
            "sway" => {
                let words: Vec<f32> = value.split_whitespace().map(str::parse).collect::<Result<_, _>>().map_err(|_| format!("{n}: sway is DEGREES SECONDS"))?;
                let &[degrees, seconds] = &words[..] else { return Err(format!("{n}: sway is DEGREES SECONDS")) };
                if !(0.0..=SWAY_MAX).contains(&degrees) || !(SWAY_SECONDS[0]..=SWAY_SECONDS[1]).contains(&seconds) {
                    return Err(format!("{n}: sway leans 0 to {SWAY_MAX} degrees, once each {} to {} seconds", SWAY_SECONDS[0], SWAY_SECONDS[1]));
                }
                g.sway = Sway { degrees, seconds };
            }
            _ => return Err(format!("{n}: {key} isn't something ground says")),
        }
    }
    if out.is_empty() {
        return Err("no ground".into());
    }
    if let Some(n) = said.iter().find_map(|&(three, crop)| three.filter(|_| !crop)) {
        return Err(format!("{n}: crop_3d needs a crop line beside it"));
    }
    if out.iter().any(|g| g.cell == Cell::Path && g.art.len() < 16) {
        return Err("a path's art has 16 frames, one for each way its neighbours join".into());
    }
    Ok((out, missing))
}

static GROUND: RwLock<Option<&'static [Ground]>> = RwLock::new(None);

/// The ground a map can paint: as an editor last set it, else `ground.txt`, read the first time it's asked for.
pub fn ground() -> &'static [Ground] {
    if let Some(g) = *GROUND.read().unwrap_or_else(|e| e.into_inner()) {
        return g;
    }
    let (g, missing) = parse_noting(crate::data::ground()).unwrap_or_else(|e| panic!("ground.txt:{e}"));
    for name in missing {
        crate::data::note(format!("ground.txt: no prop called {name}, so its crop is left off"));
    }
    GROUND.write().unwrap_or_else(|e| e.into_inner()).get_or_insert(g.leak())
}

/// Replaces the ground maps paint with `text`, as an editor does once a mod's `ground.txt` changes or the props it
/// names load.
pub fn set(text: &str) -> Result<(), String> {
    let g = parse(text)?;
    *GROUND.write().unwrap_or_else(|e| e.into_inner()) = Some(g.leak());
    Ok(())
}

/// The ground a code paints, else the first kind.
pub fn of(code: u8) -> &'static Ground {
    let g = ground();
    g.iter().find(|k| k.code == code).unwrap_or(&g[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ground_reads_and_mistakes_name_their_line() {
        let g = ground();
        assert_eq!(g.iter().map(|k| k.code as char).collect::<String>(), ".,gspcvh=");
        assert_eq!((of(b'v').joins, of(b'c').crop.map(|c| c.1)), (Some(Joins::Across), Some(art::CORN)));
        assert_eq!(of(b'?').code, b'.', "an unknown code lays the first ground");
        assert!(parse("[ab]\n").unwrap_err().starts_with("1: a ground's code"));
        assert!(parse("[x]\nart NOPE").unwrap_err().starts_with("2: no sprite"));
        assert!(parse("[x]\ncell lava").unwrap_err().starts_with("2: cell is"));
        assert!(parse("[x]\nart DIRT\ncell path").unwrap_err().contains("16 frames"));
    }

    #[test]
    fn crop_3d_picks_the_pieces_that_stand_and_falls_back_to_crop() {
        let corn = of(b'c');
        assert_eq!(corn.crop.map(|c| c.1), Some(art::CORN), "the map keeps the canopy");
        assert_eq!(corn.standing().map(|c| (c.1, c.2)), Some((art::CORN_BOTTOM, art::CORN_BOTTOM)), "3D stands whole stalks");
        let plain = &parse("[x]\ncrop CORN_TOP CORN CORN_BOTTOM\n").unwrap()[0];
        assert_eq!(plain.standing(), plain.crop, "without crop_3d, 3D stands the crop");
        assert_eq!(plain.sway, Sway::default());
        assert!(parse("[x]\ncrop_3d CORN_TOP CORN CORN_BOTTOM\n").unwrap_err().starts_with("2: crop_3d needs a crop"));
        assert!(parse("[x]\ncrop CORN_TOP CORN CORN_BOTTOM\ncrop_3d CORN NOPE CORN\n").unwrap_err().starts_with("3: no sprite called NOPE"));
        assert!(parse("[x]\ncrop_3d CORN CORN\n").unwrap_err().contains("crop_3d is TOP MIDDLE BOTTOM"));
        let wheat = "[w]\ncrop PROP_WHEAT_TOP PROP_WHEAT PROP_WHEAT_BOTTOM\ncrop_3d PROP_WHEAT_TOP PROP_STALK PROP_STALK\n";
        assert_eq!(parse_noting(wheat).unwrap().1.len(), 6, "props not loaded yet read");
        assert!(crate::harvest::check_table("ground.txt", wheat).is_ok());
        assert!(crate::harvest::check_own_table("ground.txt", "[x]\ncrop CORN_TOP CORN CORN_BOTTOM\ncrop_3d CORN PROP_STALK CORN\n").unwrap_err().contains("PROP_STALK"));
    }

    #[test]
    fn sway_reads_its_bounds_and_leans_no_further_by_time_and_place() {
        assert_eq!(parse("[x]\nsway 4 3.5\n").unwrap()[0].sway, Sway { degrees: 4.0, seconds: 3.5 });
        for bad in ["sway 4", "sway 31 2", "sway 4 0", "sway a b"] {
            assert!(parse(&format!("[x]\n{bad}\n")).unwrap_err().contains("sway"), "{bad}");
        }
        assert_eq!(Sway::default().lean(12.3, 4, 5), 0.0, "no line, no sway");
        let s = Sway { degrees: 4.0, seconds: 3.5 };
        let most = (0..2000).flat_map(|k| (0..8).map(move |x| s.lean(k as f32 * 0.037, x, 3).abs())).fold(0.0, f32::max);
        assert!(most <= 4f32.to_radians() + 1e-6 && most > 2f32.to_radians(), "{most}");
        assert_eq!(s.lean(7.25, 10, 4), s.lean(7.25, 10, 4), "the same time and cell lean alike");
        assert!((s.lean(7.25, 10, 4) - s.lean(7.25, 11, 4)).abs() > 1e-3, "neighbours lean at different moments");
        assert!((s.lean(7.25, 10, 4) - s.lean(7.25, 10, 5)).abs() > 1e-3);
    }

    #[test]
    fn tall_reads_its_bounds_and_stacks_stalks_under_the_tops_frame() {
        assert_eq!(parse("[x]\n").unwrap()[0].tall, 1, "without the line, one card");
        assert_eq!(parse("[x]\ntall 6\n").unwrap()[0].tall, 6);
        for bad in ["tall 0", "tall 7", "tall", "tall 2.5"] {
            assert!(parse(&format!("[x]\n{bad}\n")).unwrap_err().contains("tall is 1 to 6"), "{bad}");
        }
        let corn = of(b'c');
        assert_eq!(corn.tall, 3);
        let tops = &art::CORN_TOP;
        let column = |up, down| corn.stalk(4, 9, up, down).unwrap().cards().collect::<Vec<_>>();
        let cap = tops[(4 * 7 + 9 * 13) % 2];
        assert_eq!(column(true, true), [art::CORN_BOTTOM, art::CORN_BOTTOM, cap], "a middle cell's stalks under its cap");
        assert_eq!(column(false, true), [art::CORN_BOTTOM, art::CORN_BOTTOM, cap], "the north end stands stalks too, not tops");
        assert_eq!(column(true, false), [art::CORN_BOTTOM, art::CORN_BOTTOM, cap]);
        let short = Stalk::of((tops, art::CORN, art::CORN_BOTTOM), 1, false, true, 1, Sway::default());
        assert_eq!(short.cards().collect::<Vec<_>>(), [tops[1]], "one card is the piece its place picks, as before");
        assert_eq!(Stalk::of((tops, art::CORN, art::CORN_BOTTOM), 0, true, true, 1, Sway::default()).cards().collect::<Vec<_>>(), [art::CORN]);
    }

    #[test]
    fn a_tall_stalk_bends_as_one_without_seams_and_alike_each_time() {
        let corn = of(b'c');
        let stalk = Stalk { sway: Sway { degrees: 30.0, seconds: 2.0 }, ..corn.stalk(7, 3, true, true).unwrap() };
        // Stalks with 7 clear rows under their picture, as CORN_BOTTOM has, and a cap with 1 over.
        let ends = |a: Art| if a == art::CORN_BOTTOM { [7.0, 0.0] } else { [0.0, 1.0] };
        for k in 0..200 {
            let t = k as f32 * 0.073;
            let cards: Vec<_> = stalk.bend(t, 7, 3, ends).collect();
            assert_eq!(cards.len(), 3);
            // Where a card's picture starts and ends: height, and how far east, at its clear rows under and over.
            let at = |&(a, base, east, lean): &(Art, f32, f32, f32), row: f32| (base + row, east + lean * row / a.h());
            let [under, _] = ends(cards[0].0);
            assert!(at(&cards[0], under).0.abs() < 1e-5 && at(&cards[0], under).1.abs() < 1e-5, "its picture planted at the ground");
            for w in cards.windows(2) {
                let [_, over] = ends(w[0].0);
                let (top, next) = (at(&w[0], w[0].0.h() - over), at(&w[1], ends(w[1].0)[0]));
                assert!((top.0 - next.0).abs() < 1e-4 && (top.1 - next.1).abs() < 1e-4, "a picture's top meets the next one's bottom at {t}: {top:?} {next:?}");
            }
            let leans: Vec<f32> = cards.iter().map(|c| (c.3 / c.0.h()).abs()).collect();
            assert!(leans[0] <= leans[1] + 1e-6 && leans[1] <= leans[2] + 1e-6, "higher cards lean more: {leans:?}");
            let tip = at(&cards[2], cards[2].0.h() - 1.0);
            assert!((tip.1 - stalk.sway.lean(t, 7, 3).tan() * tip.0).abs() < 1e-3, "the tip leans by the sway's angle");
            assert_eq!(cards, stalk.bend(t, 7, 3, ends).collect::<Vec<_>>(), "the same time and cell bend alike");
        }
        let still = Stalk { sway: Sway::default(), ..stalk };
        assert!(still.bend(3.0, 7, 3, ends).all(|c| c.2 == 0.0 && c.3 == 0.0), "no sway, upright");
    }

    #[test]
    fn a_crop_of_props_reads_before_they_load_and_only_a_mod_names_them() {
        let wheat = "[w]\nname Wheat\nart SOIL\ncell field\ncrop PROP_WHEAT_TOP PROP_WHEAT PROP_WHEAT_BOTTOM\n";
        let (g, missing) = parse_noting(wheat).unwrap();
        assert!(g[0].crop.is_none(), "left off until its props load");
        assert_eq!(missing, ["PROP_WHEAT_TOP", "PROP_WHEAT", "PROP_WHEAT_BOTTOM"]);
        assert!(crate::harvest::check_table("ground.txt", wheat).is_ok());
        assert!(crate::harvest::check_own_table("ground.txt", wheat).unwrap_err().contains("no sprite called PROP_WHEAT_TOP in art.txt"));
        assert!(parse("[w]\ncrop WHEAT CORN CORN_BOTTOM").unwrap_err().starts_with("2: no sprite called WHEAT"));
    }
}
