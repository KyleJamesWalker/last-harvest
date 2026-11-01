//! A sprite being edited: its frames as plain RGBA pixels, loaded from the packed texture or a saved folder.

#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

use last_harvest_game::Stand;
use last_harvest_game::art::{self, ATLAS_W, Art};
use last_harvest_game::paint::{self, Paint};

pub type Rgba = [u8; 4];
pub const CLEAR: Rgba = [0, 0, 0, 0];

#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub w: usize,
    pub h: usize,
    pub px: Vec<Rgba>,
}

impl Frame {
    pub fn blank(w: usize, h: usize) -> Self {
        Self { w, h, px: vec![CLEAR; w * h] }
    }

    fn from_art(a: Art) -> Self {
        let [x, y, w, h] = a.rect().map(|v| v as usize);
        let px = (0..h)
            .flat_map(|row| (0..w).map(move |col| ((y + row) * ATLAS_W as usize + x + col) * 4))
            .map(|i| {
                let p = art::pixels();
                [p[i], p[i + 1], p[i + 2], p[i + 3]]
            })
            .collect();
        Self { w, h, px }
    }

    pub fn get(&self, x: i32, y: i32) -> Option<Rgba> {
        self.inside(x, y).then(|| self.px[y as usize * self.w + x as usize])
    }

    pub fn set(&mut self, x: i32, y: i32, c: Rgba) {
        if self.inside(x, y) {
            self.px[y as usize * self.w + x as usize] = c;
        }
    }

    fn inside(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h
    }

    /// Paints every pixel joined to (x, y) through pixels of its color.
    pub fn fill(&mut self, x: i32, y: i32, c: Rgba) {
        let Some(target) = self.get(x, y) else { return };
        if target == c {
            return;
        }
        let mut stack = vec![(x, y)];
        while let Some((x, y)) = stack.pop() {
            if self.get(x, y) == Some(target) {
                self.set(x, y, c);
                stack.extend([(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]);
            }
        }
    }

    /// This frame repainted in coat `k` of `paint`.
    pub fn in_coat(&self, paint: &Paint, k: usize) -> Self {
        let mut bytes = self.px.concat();
        paint.repaint(k, &mut bytes);
        Self { px: bytes.chunks_exact(4).map(|p| [p[0], p[1], p[2], p[3]]).collect(), ..self.clone() }
    }

    /// Left to right reversed, as a view from the opposite side sees it.
    pub fn mirrored(&self) -> Self {
        Self { px: self.px.chunks(self.w).flat_map(|row| row.iter().rev().copied()).collect(), ..self.clone() }
    }

    /// A new size, keeping the pixels anchored at the bottom center, where a sprite meets the ground.
    pub fn resized(&self, w: usize, h: usize) -> Self {
        let mut out = Self::blank(w, h);
        let dx = (w as i32 - self.w as i32) / 2;
        let dy = h as i32 - self.h as i32;
        for y in 0..self.h as i32 {
            for x in 0..self.w as i32 {
                out.set(x + dx, y + dy, self.px[y as usize * self.w + x as usize]);
            }
        }
        out
    }
}

#[derive(Clone, Debug)]
pub struct Sprite {
    pub name: String,
    pub frames: Vec<Frame>,
    /// Drawings of it from the south and from the east, for carving it into a solid.
    pub views: Option<[Frame; 2]>,
    /// Its outline from above, for carving, when the sprite's own outline isn't it.
    pub footprint: Option<Frame>,
    /// The colors a carving's top takes in place of the sprite's, as big as the sprite; clear leaves the sprite's.
    pub roof: Option<Frame>,
    /// Drawings from the north and the west that color those sides in place of the south and east views; clear
    /// leaves theirs.
    pub north: Option<Frame>,
    pub west: Option<Frame>,
    /// One row whose colors cap a figure's columns in the 3D view; a clear pixel leaves that column's own.
    pub top: Option<Frame>,
    /// An enemy card's `enemies.txt` section, when this is one of a card's parts.
    pub card: Option<String>,
    /// Colors the 3D view leaves out, where the 2D view keeps them.
    pub clear_in_3d: Vec<Rgba>,
    /// Its colors that are paint, and the coats each frame comes in.
    pub paint: Paint,
}

pub const FOOTPRINT_FILE: &str = "footprint.png";
pub const ROOF_FILE: &str = "roof.png";
pub const NORTH_FILE: &str = "north.png";
pub const WEST_FILE: &str = "west.png";
pub const TOP_FILE: &str = "top.png";
pub const CARD_FILE: &str = "enemy.txt";
/// The colors a sprite clears in 3D, one hex color a line.
pub const CLEAR_FILE: &str = "clear.txt";
pub use last_harvest_pack::card::Kind as CardKind;

/// A card lists as six sprites, like the game's own zombies: `CARD_<ID>_FRONT` and so on for an enemy card, or
/// `PLAYER_CARD_<ID>_FRONT` for a player card, each a row of the card's sheet.
pub const CARD_PREFIX: &str = "CARD_";
pub const PLAYER_CARD_PREFIX: &str = "PLAYER_CARD_";
pub const PLAYER_SECTION: &str = "Player cards";
pub const CARD_PARTS: [&str; 6] = ["FRONT", "SIDE", "BACK", "FLASH", "HURT", "DEAD"];
pub const CARD_PART_FRAMES: usize = 3;
pub const CARD_FRAMES: usize = CARD_PARTS.len() * CARD_PART_FRAMES;
pub const CARD_SECTION: &str = "Enemy cards";
/// The biggest frame a card can have, as the game reads cards.
pub const CARD_MAX_FRAME: usize = 64;

fn prefix(kind: CardKind) -> &'static str {
    match kind {
        CardKind::Enemy => CARD_PREFIX,
        CardKind::Player => PLAYER_CARD_PREFIX,
    }
}

/// The kind of card a name's prefix says, and the rest of the name after it.
fn split_kind(name: &str) -> Option<(CardKind, &str)> {
    let rest = |kind| name.strip_prefix(prefix(kind)).map(|r| (kind, r));
    rest(CardKind::Player).or_else(|| rest(CardKind::Enemy))
}

/// The card a sprite name is a part of and the part's row, such as `ghoul` and 1 for `CARD_GHOUL_SIDE`.
pub fn card_part(name: &str) -> Option<(String, usize)> {
    let (_, rest) = split_kind(name)?;
    let (id, part) = rest.rsplit_once('_')?;
    let row = CARD_PARTS.iter().position(|p| *p == part)?;
    (!id.is_empty()).then(|| (id.to_ascii_lowercase(), row))
}

/// Which kind of card a sprite name is a part of.
pub fn card_kind(name: &str) -> Option<CardKind> {
    card_part(name).and(split_kind(name)).map(|(kind, _)| kind)
}

pub fn card_id(name: &str) -> Option<String> {
    card_part(name).map(|(id, _)| id)
}

/// The card a name typed for New or Copy asks for: `CARD_GHOUL` or `PLAYER_CARD_JO`, or one of their parts.
pub fn card_wanted(name: &str) -> Option<(CardKind, String)> {
    if let (Some(kind), Some(id)) = (card_kind(name), card_id(name)) {
        return Some((kind, id));
    }
    split_kind(name).filter(|(_, id)| !id.is_empty()).map(|(kind, id)| (kind, id.to_ascii_lowercase()))
}

/// The part names an enemy card id lists under, when the id is one a sprite name can carry.
pub fn card_names(id: &str) -> Option<[String; 6]> {
    card_names_of(CardKind::Enemy, id)
}

/// The part names a card of `kind` lists under, when the id is one a sprite name can carry.
pub fn card_names_of(kind: CardKind, id: &str) -> Option<[String; 6]> {
    let names = CARD_PARTS.map(|p| format!("{}{}_{p}", prefix(kind), id.to_ascii_uppercase()));
    (!id.is_empty() && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') && names.iter().all(|n| valid_name(n))).then_some(names)
}

/// A new card's 18 frames: a plain 16x16 figure to paint over, walking front, side and back with its legs taking
/// turns, then blank flash, hurt and dead frames, which the game draws from the walk.
pub fn card_template() -> Vec<Frame> {
    const OUTLINE: Rgba = [28, 28, 34, 255];
    const HEAD: Rgba = [168, 168, 160, 255];
    const BODY: Rgba = [120, 120, 128, 255];
    const LEGS: Rgba = [84, 84, 96, 255];
    const EYE: Rgba = [40, 40, 48, 255];
    let figure = |facing: usize, step: usize| {
        let mut f = Frame::blank(16, 16);
        let mut fill = |x0: i32, y0: i32, x1: i32, y1: i32, c: Rgba| (y0..=y1).for_each(|y| (x0..=x1).for_each(|x| f.set(x, y, c)));
        let side = facing == 1;
        let (hx0, hx1) = if side { (6, 10) } else { (5, 10) };
        fill(hx0 - 1, 0, hx1 + 1, 6, OUTLINE);
        fill(hx0, 1, hx1, 5, HEAD);
        let (bx0, bx1) = if side { (5, 10) } else { (3, 12) };
        fill(bx0 - 1, 6, bx1 + 1, 11, OUTLINE);
        fill(bx0, 7, bx1, 10, BODY);
        let legs = if side { [(6, 7), (8, 9)] } else { [(5, 7), (8, 10)] };
        for (i, (l0, l1)) in legs.into_iter().enumerate() {
            let bottom = if step == i + 1 { 14 } else { 15 };
            fill(l0 - 1, 11, l1 + 1, bottom, OUTLINE);
            fill(l0, 11, l1, bottom - 1, LEGS);
        }
        match facing {
            0 => [6, 9].into_iter().for_each(|x| f.set(x, 3, EYE)),
            1 => f.set(9, 3, EYE),
            _ => {}
        }
        f
    };
    let walk = (0..9).map(|k| figure(k / 3, k % 3));
    walk.chain((0..9).map(|_| Frame::blank(16, 16))).collect()
}

/// What frame `i` of a card's part in `row` shows.
pub fn card_frame_name(row: usize, i: usize) -> String {
    let facing = ["Facing front", "Facing side", "Facing back"];
    if row < 3 { format!("Walking, step {}", i + 1) } else { facing.get(i).unwrap_or(&"").to_string() }
}

/// A top strip as the 3D view colors a figure's top without one: each column's first solid pixel, or the one
/// below it when that is outline-dark.
pub fn top_strip(f: &Frame) -> Frame {
    let mut strip = Frame::blank(f.w, 1);
    let dark = |p: Rgba| p[0] as u32 + p[1] as u32 + p[2] as u32 <= 40;
    for x in 0..f.w as i32 {
        let Some(y) = (0..f.h as i32).find(|&y| f.get(x, y).is_some_and(|p| p[3] > 0)) else { continue };
        let below = f.get(x, y + 1).filter(|p| p[3] > 0 && !dark(*p));
        let edge = f.get(x, y).unwrap_or(CLEAR);
        strip.set(x, 0, if dark(edge) { below.unwrap_or(edge) } else { edge });
    }
    strip
}

pub const VIEW_FILES: [&str; 2] = ["south.png", "east.png"];

impl Frame {
    /// The color seen walking in from (x, y) by (dx, dy): the first opaque pixel past the dark outline,
    /// or the outline itself when nothing else is there.
    fn first(&self, (mut x, mut y): (i32, i32), (dx, dy): (i32, i32)) -> Option<Rgba> {
        let dark = |c: Rgba| c[0] as u32 + c[1] as u32 + c[2] as u32 <= 60;
        let mut outline = None;
        while let Some(c) = self.get(x, y) {
            if c[3] > 0 {
                if !dark(c) {
                    return Some(c);
                }
                outline = outline.or(Some(c));
            }
            (x, y) = (x + dx, y + dy);
        }
        outline
    }
}

/// Plain views of a block as tall as `height`, colored by the top view's edges, to carve a shape out of.
pub fn block_views(top: &Frame, height: usize) -> [Frame; 2] {
    let (w, d) = (top.w as i32, top.h as i32);
    let mut south = Frame::blank(top.w, height);
    for x in 0..w {
        if let Some(c) = top.first((x, d - 1), (0, -1)) {
            (0..height as i32).for_each(|y| south.set(x, y, c));
        }
    }
    // Seen from the east, south is on the left.
    let mut east = Frame::blank(top.h, height);
    for c in 0..d {
        if let Some(col) = top.first((w - 1, d - 1 - c), (-1, 0)) {
            (0..height as i32).for_each(|y| east.set(c, y, col));
        }
    }
    [south, east]
}

#[cfg(not(target_arch = "wasm32"))]
/// Where saved sprites live; the game's build packs them in place of, or beside, the tileset's.
pub fn folder() -> PathBuf {
    std::env::var_os("LAST_HARVEST_SPRITES").map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/private/sprites"))
}

/// A name the build can turn into a constant: a capital letter, then capitals, digits and underscores.
/// The list heading for a sprite: the comment line that opens its group in the catalog, up to its first
/// `,` `:` or `;`. A sprite the catalog doesn't name is a new one.
pub fn section(name: &str) -> &'static str {
    if name.starts_with(MAP_PREFIX) {
        return MAP_SECTION;
    }
    if let Some(rest) = name.strip_prefix(DATA_PREFIX) {
        return match rest.split('_').next() {
            Some("TUNING") => "Data/tuning",
            Some("PROPS") => "Data/props",
            Some("EFFECTS") => "Data/effects",
            Some("GROUND") => "Data/ground",
            Some("FARM") => "Data/random farm",
            Some("POWERUPS") => "Data/powerups",
            Some("ENEMIES") => "Data/enemies",
            _ => "Data",
        };
    }
    if name.starts_with(last_harvest_game::PROP_PREFIX) {
        return PROP_SECTION;
    }
    match card_kind(name) {
        Some(CardKind::Enemy) => return CARD_SECTION,
        Some(CardKind::Player) => return PLAYER_SECTION,
        None => {}
    }
    static SECTIONS: std::sync::OnceLock<Vec<(String, String)>> = std::sync::OnceLock::new();
    let sections = SECTIONS.get_or_init(|| sections_of(include_str!("../../game/art.txt")));
    sections.iter().find(|(n, _)| n == name).map_or(NEW_SECTION, |(_, s)| s.as_str())
}

/// The headings a section lists under, outermost first: `Data/tuning` is under `Data`, then `Data/tuning`.
pub fn section_path(section: &'static str) -> impl Iterator<Item = &'static str> {
    section.match_indices('/').map(move |(i, _)| &section[..i]).chain(std::iter::once(section))
}

/// Whether `section` lists under the heading `path`.
pub fn within(section: &str, path: &str) -> bool {
    section.strip_prefix(path).is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

pub const NEW_SECTION: &str = "New sprites";
/// A hand-made map lists as `MAP_<ID>` under Maps.
pub const MAP_PREFIX: &str = "MAP_";
pub const MAP_SECTION: &str = "Maps";
/// A section of one of the game's data files lists as `DATA_<FILE>_<SECTION>`, under a heading for its file.
pub const DATA_PREFIX: &str = "DATA_";
/// A player's prop lists as `PROP_<ID>` under Props.
pub const PROP_SECTION: &str = "Props";

/// Each sprite's heading in catalog text: a comment opens a group when a blank line comes before it and a
/// sprite right after it.
fn sections_of(text: &str) -> Vec<(String, String)> {
    let (mut out, mut heading, mut gap, mut pending) = (Vec::new(), String::new(), true, None::<String>);
    for line in text.lines().map(str::trim) {
        if line.is_empty() {
            (gap, pending) = (true, None);
        } else if let Some(c) = line.strip_prefix('#') {
            pending = gap.then(|| c.split([',', ':', ';']).next().unwrap_or("").trim().to_string());
            gap = false;
        } else {
            if let Some(h) = pending.take() {
                heading = h;
            }
            gap = false;
            if let Some(name) = line.split_whitespace().next() {
                out.push((name.to_string(), heading.clone()));
            }
        }
    }
    out
}

/// A color as `#5A6E3C`.
pub fn hex(c: Rgba) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

/// An opaque color from `#5A6E3C`, the `#` optional.
pub fn parse_hex(text: &str) -> Option<Rgba> {
    let t = text.trim().trim_start_matches('#');
    let byte = |i: usize| t.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok());
    (t.len() == 6).then_some(())?;
    Some([byte(0)?, byte(2)?, byte(4)?, 255])
}

pub fn valid_name(n: &str) -> bool {
    n.starts_with(|c: char| c.is_ascii_uppercase()) && n.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(not(target_arch = "wasm32"))]
/// Every sprite to choose from: the packed catalog, then new ones saved on disk, with whether each has a saved copy.
pub fn catalog(dir: &Path) -> Vec<(String, bool)> {
    let saved = |n: &str| dir.join(n).join("0.png").exists();
    let mut names: Vec<(String, bool)> = art::ALL.iter().filter(|(n, _)| *n != "SCRATCH").map(|(n, _)| (n.to_string(), saved(n))).collect();
    let mut extra: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| valid_name(n) && saved(n) && !names.iter().any(|(m, _)| m == n))
        .collect();
    extra.sort();
    names.extend(extra.into_iter().map(|n| (n, true)));
    names
}

impl Sprite {
    /// The saved copy if there is one, else the packed frames.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load(dir: &Path, name: &str) -> Option<Self> {
        Self::load_from(name, |file| std::fs::read(dir.join(name).join(file)).ok())
    }

    /// Loads from whatever `read` returns for each file in the sprite's folder, such as an earlier commit; None when a
    /// file is there but doesn't decode.
    pub fn load_from(name: &str, read: impl Fn(&str) -> Option<Vec<u8>>) -> Option<Self> {
        let broken = std::cell::Cell::new(false);
        let png = |file: &str| read(file).and_then(|bytes| decode_png(&bytes).inspect_err(|_| broken.set(true)).ok());
        let mut frames = Vec::new();
        while let Some(f) = png(&format!("{}.png", frames.len())) {
            frames.push(f);
        }
        if broken.get() {
            return None;
        }
        if frames.is_empty() {
            let (_, arts) = art::ALL.iter().find(|(n, _)| *n == name)?;
            frames = arts.iter().map(|&a| Frame::from_art(a)).collect();
        }
        // A sprite the catalog turns shows the east view it packs until one is saved.
        let packed_east = || match art::ALL.iter().find(|(n, _)| *n == name)?.1.first()?.stand()? {
            Stand::Turned { east, .. } => Some(Frame::from_art(east)),
            _ => None,
        };
        let views = png(VIEW_FILES[0]).zip(png(VIEW_FILES[1])).map(|(s, e)| [s, e]).or_else(|| {
            let east = packed_east()?;
            Some([block_views(&frames[0], east.h)[0].clone(), east])
        });
        let card = read(CARD_FILE).and_then(|b| String::from_utf8(b).ok());
        let clear = read(CLEAR_FILE).and_then(|b| String::from_utf8(b).ok()).unwrap_or_default();
        let clear_in_3d = clear.lines().filter_map(parse_hex).collect();
        let paint = read(paint::FILE).and_then(|b| paint::parse(&String::from_utf8(b).ok()?).ok()).unwrap_or_default();
        let sprite = Self { name: name.to_string(), frames, views, footprint: png(FOOTPRINT_FILE), roof: png(ROOF_FILE), north: png(NORTH_FILE), west: png(WEST_FILE), top: png(TOP_FILE), card, clear_in_3d, paint };
        (!broken.get()).then_some(sprite)
    }

    /// The files a save writes: the frames, then the views, footprint, roof, north and west views and top when it has
    /// them.
    pub fn files(&self) -> Vec<(String, Vec<u8>)> {
        let frames = self.frames.iter().enumerate().map(|(i, f)| (format!("{i}.png"), f));
        let views = self.views.iter().flat_map(|v| VIEW_FILES.iter().zip(v).map(|(n, f)| (n.to_string(), f)));
        let optional = self.optional().into_iter().filter_map(|(n, f)| Some((n.to_string(), f.as_ref()?)));
        let pngs = frames.chain(views).chain(optional).map(|(n, f)| (n, encode_png(f)));
        let clear = self.clear_text().map(|t| (CLEAR_FILE.to_string(), t.into_bytes()));
        let paint = Some(self.paint.text()).filter(|t| !t.is_empty()).map(|t| (paint::FILE.to_string(), t.into_bytes()));
        pngs.chain(self.card.iter().map(|t| (CARD_FILE.to_string(), t.clone().into_bytes()))).chain(clear).chain(paint).collect()
    }

    /// The pictures a sprite may have or not, by the file each saves as.
    fn optional(&self) -> [(&'static str, &Option<Frame>); 5] {
        [(FOOTPRINT_FILE, &self.footprint), (ROOF_FILE, &self.roof), (NORTH_FILE, &self.north), (WEST_FILE, &self.west), (TOP_FILE, &self.top)]
    }

    /// `clear.txt` as written, or None when no color is cleared in 3D.
    fn clear_text(&self) -> Option<String> {
        (!self.clear_in_3d.is_empty()).then(|| self.clear_in_3d.iter().map(|&c| format!("{}\n", hex(c))).collect())
    }

    /// A sprite from the files `files` lists, which must hold at least one frame.
    pub fn from_files(name: &str, files: &[(String, Vec<u8>)]) -> Option<Self> {
        files.iter().any(|(n, _)| n == "0.png").then_some(())?;
        Self::load_from(name, |file| files.iter().find(|(n, _)| n == file).map(|(_, b)| b.clone()))
    }

    /// Writes `0.png`, `1.png` and so on, removing frames left over from a longer save.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        let path = dir.join(&self.name);
        std::fs::create_dir_all(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        for (i, f) in self.frames.iter().enumerate() {
            write_png(&path.join(format!("{i}.png")), f)?;
        }
        let mut i = self.frames.len();
        while std::fs::remove_file(path.join(format!("{i}.png"))).is_ok() {
            i += 1;
        }
        for (file, frame) in self.optional() {
            match frame {
                Some(f) => write_png(&path.join(file), f)?,
                None => {
                    let _ = std::fs::remove_file(path.join(file));
                }
            }
        }
        for (file, text) in [(CLEAR_FILE, self.clear_text().unwrap_or_default()), (paint::FILE, self.paint.text())] {
            if text.is_empty() {
                let _ = std::fs::remove_file(path.join(file));
            } else {
                std::fs::write(path.join(file), text).map_err(|e| e.to_string())?;
            }
        }
        for (k, file) in VIEW_FILES.iter().enumerate() {
            match &self.views {
                Some(v) => write_png(&path.join(file), &v[k])?,
                None => {
                    let _ = std::fs::remove_file(path.join(file));
                }
            }
        }
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// The game's sprite catalog, where a packed sprite's stand is written.
pub fn catalog_file() -> PathBuf {
    std::env::var_os("LAST_HARVEST_CATALOG").map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../game/art.txt"))
}

/// How a stand is written in the catalog and in `stand.txt`.
pub fn stand_text(s: Stand) -> String {
    match s {
        Stand::Prop { depth } => format!("prop {depth}"),
        Stand::Fold { rows } => format!("fold {rows}"),
        Stand::Slab { height, lift: 0.0 } => format!("slab {height}"),
        Stand::Slab { height, lift } => format!("slab {height} float {lift}"),
        Stand::Figure => "figure".into(),
        Stand::Ground => "ground".into(),
        Stand::Carve { .. } => "carve".into(),
        Stand::Turned { depth: None, .. } => "turned".into(),
        Stand::Turned { depth: Some(d), .. } => format!("turned {d}"),
    }
}

pub fn parse_stand(text: &str) -> Option<Stand> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let size = || words.get(1)?.parse::<f32>().ok();
    Some(match *words.first()? {
        "prop" => Stand::Prop { depth: size()? },
        "fold" => Stand::Fold { rows: size()? },
        "slab" => {
            let lift = match words.get(2..4) {
                Some(["float", n]) => n.parse::<f32>().ok()?,
                None if words.len() == 2 => 0.0,
                _ => return None,
            };
            Stand::Slab { height: size()?, lift }
        }
        "figure" => Stand::Figure,
        "ground" => Stand::Ground,
        _ => return None,
    })
}

/// The line for `name` in the catalog, as (index, the line).
#[cfg(not(target_arch = "wasm32"))]
fn catalog_line(text: &str, name: &str) -> Option<(usize, String)> {
    text.lines().enumerate().find(|(_, l)| l.split_whitespace().next() == Some(name) && !l.starts_with('#')).map(|(i, l)| (i, l.to_string()))
}

#[cfg(not(target_arch = "wasm32"))]
/// How a sprite stands, as last saved: its folder's `stand.txt`, else its catalog line, else nothing said.
/// `carve` comes back as itself, since its views are pixels rather than words.
pub fn read_stand(dir: &Path, catalog: &Path, name: &str) -> Option<String> {
    if let Ok(t) = std::fs::read_to_string(dir.join(name).join("stand.txt")) {
        return Some(t.trim().to_string());
    }
    let text = std::fs::read_to_string(catalog).ok()?;
    let (_, line) = catalog_line(&text, name)?;
    let code = line.split('#').next()?;
    Some(code.split_once('@')?.1.trim().to_string())
}

#[cfg(not(target_arch = "wasm32"))]
/// Writes how a sprite stands: on its catalog line when it has one, which git keeps, else in its folder.
/// A carved sprite's stand always goes in its folder, beside the views it needs.
pub fn save_stand(dir: &Path, catalog: &Path, name: &str, how: &str) -> Result<(), String> {
    let text = std::fs::read_to_string(catalog).map_err(|e| format!("{}: {e}", catalog.display()))?;
    let line = catalog_line(&text, name).filter(|_| how != "carve");
    let Some((i, line)) = line else {
        let path = dir.join(name);
        std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        return std::fs::write(path.join("stand.txt"), format!("{how}\n")).map_err(|e| e.to_string());
    };
    let (code, comment) = line.split_once('#').map_or((line.as_str(), None), |(c, m)| (c, Some(m)));
    let code = code.split('@').next().unwrap_or(code).trim_end();
    let new = match comment {
        Some(m) => format!("{code}  @{how}  #{m}"),
        None => format!("{code}  @{how}"),
    };
    let mut lines: Vec<&str> = text.lines().collect();
    lines[i] = &new;
    std::fs::write(catalog, lines.join("\n") + "\n").map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(dir.join(name).join("stand.txt"));
    Ok(())
}

pub fn decode_png(bytes: &[u8]) -> Result<Frame, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16 | png::Transformations::ALPHA);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let buf = &buf[..info.buffer_size()];
    let px = match info.color_type {
        png::ColorType::Rgba => buf.chunks_exact(4).map(|p| [p[0], p[1], p[2], p[3]]).collect(),
        png::ColorType::GrayscaleAlpha => buf.chunks_exact(2).map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        other => return Err(format!("unsupported {other:?}")),
    };
    Ok(Frame { w: info.width as usize, h: info.height as usize, px })
}

pub fn encode_png(f: &Frame) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, f.w as u32, f.h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let data: Vec<u8> = f.px.iter().flatten().copied().collect();
    enc.write_header().and_then(|mut w| w.write_image_data(&data)).expect("encoding into memory");
    out
}

#[cfg(not(target_arch = "wasm32"))]
fn write_png(path: &Path, f: &Frame) -> Result<(), String> {
    std::fs::write(path, encode_png(f)).map_err(|e| format!("{}: {e}", path.display()))
}

/// Files bundled into one body: each a name line, a length line, then its bytes.
pub fn bundle(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, bytes) in files {
        out.extend(format!("{name}\n{}\n", bytes.len()).into_bytes());
        out.extend(bytes);
    }
    out
}

pub fn unbundle(mut body: &[u8]) -> Option<Vec<(String, Vec<u8>)>> {
    let line = |body: &mut &[u8]| {
        let end = body.iter().position(|&b| b == b'\n')?;
        let text = String::from_utf8(body[..end].to_vec()).ok();
        *body = &body[end + 1..];
        text
    };
    let mut files = Vec::new();
    while !body.is_empty() {
        let name = line(&mut body)?;
        let len: usize = line(&mut body)?.parse().ok()?;
        files.push((name, body.get(..len)?.to_vec()));
        body = &body[len..];
    }
    Some(files)
}

/// The colors the game's own art keeps to, from `palette.txt`, for the palette the editors start with.
pub fn palette() -> Vec<Rgba> {
    paint::palette().into_iter().map(|[r, g, b]| [r, g, b, 255]).collect()
}

/// The color of `palette` that looks closest to `c`, weighing red, green and blue as the eye does.
pub fn nearest(c: Rgba, palette: &[Rgba]) -> Rgba {
    let d = |p: &Rgba| {
        let r = (c[0] as i32 + p[0] as i32) / 2;
        let [dr, dg, db] = [0, 1, 2].map(|i| c[i] as i32 - p[i] as i32);
        (512 + r) * dr * dr / 256 + 4 * dg * dg + (767 - r) * db * db / 256
    };
    palette.iter().copied().min_by_key(d).unwrap_or(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every color the game's own drawings and coats use that `palette.txt` doesn't list, with where.
    fn off_palette(dirs: &[PathBuf]) -> Vec<String> {
        let palette = paint::palette();
        let mut off = Vec::new();
        let files = dirs.iter().flat_map(|d| std::fs::read_dir(d).into_iter().flatten().flatten().map(|e| e.path()));
        for path in files {
            if path.is_dir() {
                off.extend(off_palette(&[path]));
                continue;
            }
            let colors: Vec<paint::Rgb> = match path.extension().and_then(|e| e.to_str()) {
                Some("png") => decode_png(&std::fs::read(&path).unwrap()).unwrap().px.iter().filter(|p| p[3] > 0).map(|p| [p[0], p[1], p[2]]).collect(),
                _ if path.file_name().is_some_and(|n| n == paint::FILE) => {
                    let p = paint::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
                    (0..p.coats.len()).flat_map(|k| p.shades(k)).collect()
                }
                _ => continue,
            };
            let mut seen = Vec::new();
            for c in colors.into_iter().filter(|c| !palette.contains(c)) {
                if !seen.contains(&c) {
                    seen.push(c);
                    off.push(format!("{}: {}", path.display(), hex([c[0], c[1], c[2], 255])));
                }
            }
        }
        off
    }

    #[test]
    fn the_games_art_keeps_to_its_palette() {
        let mut dirs = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("../game/placeholder")];
        // A clone without the private assets checks only the placeholders.
        dirs.extend(Some(folder()).filter(|f| f.is_dir()));
        let off = off_palette(&dirs);
        assert!(off.is_empty(), "colors that crates/game/palette.txt doesn't list; add each there with what it's for, or redraw with the palette's:\n{}", off.join("\n"));
    }

    #[test]
    fn headings_come_from_the_comment_that_opens_each_group() {
        let text = "# The file\n# says what it is\n\n# Ground\nDIRT cell 1 1\n# Indexed by connections: N=1\nPATH cell 2 2\n\n# Fences: top row\nRAIL cell 3 3\n";
        let got: Vec<(String, String)> = sections_of(text);
        let want = [("DIRT", "Ground"), ("PATH", "Ground"), ("RAIL", "Fences")].map(|(a, b)| (a.to_string(), b.to_string()));
        assert_eq!(got, want);
        assert_eq!(section("PLAYER_FRONT"), "Characters");
        assert_eq!(section("ICON_PISTOL"), "Placeholders");
        assert_eq!(section("NOT_IN_THE_CATALOG"), NEW_SECTION);
    }

    #[test]
    fn fill_stops_at_other_colors() {
        let mut f = Frame::blank(4, 3);
        for y in 0..3 {
            f.set(2, y, [9, 9, 9, 255]);
        }
        f.fill(0, 0, [1, 2, 3, 255]);
        assert_eq!(f.get(1, 2), Some([1, 2, 3, 255]));
        assert_eq!(f.get(3, 0), Some(CLEAR));
    }

    #[test]
    fn resizing_keeps_the_feet_on_the_ground() {
        let mut f = Frame::blank(2, 2);
        f.set(0, 1, [5, 5, 5, 255]);
        let g = f.resized(4, 5);
        assert_eq!(g.get(1, 4), Some([5, 5, 5, 255]));
        assert_eq!(g.resized(2, 2), f);
    }

    #[test]
    fn saves_and_loads_every_frame_and_drops_leftovers() {
        let dir = std::env::temp_dir().join(format!("lh-sprites-{}", std::process::id()));
        let mut s = Sprite { name: "TEST_THING".into(), frames: vec![Frame::blank(3, 2), Frame::blank(3, 2), Frame::blank(3, 2)], views: None, footprint: None, roof: None, north: None, west: None, top: None, card: None, clear_in_3d: Vec::new(), paint: Default::default() };
        s.frames[1].set(2, 1, [200, 10, 10, 255]);
        s.save(&dir).unwrap();
        s.frames.truncate(2);
        s.save(&dir).unwrap();
        let back = Sprite::load(&dir, "TEST_THING").unwrap();
        assert!(catalog(&dir).iter().any(|(n, saved)| n == "TEST_THING" && *saved));
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(back.frames, s.frames);
    }

    #[test]
    fn a_sprites_files_bundle_and_come_back_whole() {
        let mut s = Sprite { name: "THING".into(), frames: vec![Frame::blank(3, 2); 2], views: Some([Frame::blank(3, 4), Frame::blank(2, 4)]), footprint: None, roof: None, north: None, west: None, top: Some(Frame::blank(3, 1)), card: None, clear_in_3d: Vec::new(), paint: Default::default() };
        s.frames[1].set(1, 1, [1, 2, 3, 255]);
        let files = unbundle(&bundle(&s.files())).unwrap();
        assert_eq!(files.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["0.png", "1.png", "south.png", "east.png", "top.png"]);
        let back = Sprite::from_files("THING", &files).unwrap();
        assert_eq!((back.frames, back.views, back.footprint, back.top), (s.frames, s.views, s.footprint, s.top));
        assert!(Sprite::from_files("THING", &files[1..]).is_none(), "a frame at least");
        assert!(unbundle(b"0.png\n9\nshort").is_none());
    }

    fn png_of(w: u32, h: u32, color: png::ColorType, depth: png::BitDepth, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(color);
        enc.set_depth(depth);
        enc.write_header().and_then(|mut w| w.write_image_data(data)).unwrap();
        out
    }

    #[test]
    fn grayscale_and_16_bit_pngs_decode_to_their_colors() {
        let gray = png_of(2, 1, png::ColorType::Grayscale, png::BitDepth::Eight, &[10, 200]);
        assert_eq!(decode_png(&gray).unwrap().px, [[10, 10, 10, 255], [200, 200, 200, 255]]);
        let gray_alpha = png_of(1, 1, png::ColorType::GrayscaleAlpha, png::BitDepth::Eight, &[7, 0]);
        assert_eq!(decode_png(&gray_alpha).unwrap().px, [[7, 7, 7, 0]]);
        let deep = png_of(1, 1, png::ColorType::Rgba, png::BitDepth::Sixteen, &[0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xff, 0xff]);
        assert_eq!(decode_png(&deep).unwrap().px, [[0x12, 0x56, 0x9a, 0xff]]);
        let rgb = png_of(1, 1, png::ColorType::Rgb, png::BitDepth::Eight, &[1, 2, 3]);
        assert_eq!(decode_png(&rgb).unwrap().px, [[1, 2, 3, 255]]);
    }

    #[test]
    fn a_saved_picture_that_wont_decode_never_falls_back_to_the_packed_frames() {
        let good = encode_png(&Frame::blank(3, 2));
        let files = |zero: &[u8], extra: Option<(&str, &[u8])>| -> Vec<(String, Vec<u8>)> { std::iter::once(("0.png".to_string(), zero.to_vec())).chain(extra.map(|(n, b)| (n.to_string(), b.to_vec()))).collect() };
        assert!(Sprite::from_files("BARREL", &files(b"not a png", None)).is_none(), "a broken 0.png isn't swapped for the atlas");
        assert!(Sprite::from_files("BARREL", &files(&good, Some(("1.png", b"junk")))).is_none(), "nor a broken later frame dropped");
        assert!(Sprite::from_files("BARREL", &files(&good, Some(("south.png", b"junk")))).is_none(), "nor a broken view");
        assert_eq!(Sprite::from_files("BARREL", &files(&good, None)).unwrap().frames, [Frame::blank(3, 2)]);
    }

    #[test]
    fn packed_sprites_load_from_the_texture() {
        let s = Sprite::load(Path::new("/nonexistent"), "BARREL").unwrap();
        assert_eq!(s.frames[0].h, 16);
        assert!((1..=16).contains(&s.frames[0].w));
        assert!(s.frames[0].px.iter().any(|p| p[3] > 0));
    }

    #[test]
    fn names_must_work_as_constants() {
        assert!(valid_name("SCARECROW_2"));
        assert!(!valid_name("scarecrow") && !valid_name("2X") && !valid_name(""));
    }

    #[test]
    fn stands_round_trip_through_the_catalog_or_the_folder() {
        let tmp = std::env::temp_dir().join(format!("lh-stand-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let catalog = tmp.join("art.txt");
        std::fs::write(&catalog, "# a comment about CAR_V\nCAR_V  px 1 2 3 4\nCAR_H  px 5 6 7 8  @prop 4\n").unwrap();
        let dir = tmp.join("sprites");
        save_stand(&dir, &catalog, "CAR_V", &stand_text(Stand::Slab { height: 12.0, lift: 0.0 })).unwrap();
        save_stand(&dir, &catalog, "CAR_H", "slab 10").unwrap();
        save_stand(&dir, &catalog, "NEW_ONE", "figure").unwrap();
        let text = std::fs::read_to_string(&catalog).unwrap();
        assert_eq!(text, "# a comment about CAR_V\nCAR_V  px 1 2 3 4  @slab 12\nCAR_H  px 5 6 7 8  @slab 10\n");
        assert_eq!(read_stand(&dir, &catalog, "CAR_V").as_deref(), Some("slab 12"));
        assert_eq!(read_stand(&dir, &catalog, "NEW_ONE").as_deref(), Some("figure"));
        assert_eq!(read_stand(&dir, &catalog, "MISSING"), None);
        save_stand(&dir, &catalog, "CAR_H", "carve").unwrap();
        assert_eq!(read_stand(&dir, &catalog, "CAR_H").as_deref(), Some("carve"), "the folder's stand.txt wins");
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn block_views_take_the_top_views_edge_colors() {
        let mut top = Frame::blank(3, 4);
        top.set(1, 2, [10, 0, 0, 255]);
        top.set(2, 0, [0, 20, 0, 255]);
        let [south, east] = block_views(&top, 5);
        assert_eq!((south.w, south.h, east.w, east.h), (3, 5, 4, 5));
        assert_eq!(south.get(1, 4), Some([10, 0, 0, 255]));
        assert_eq!(south.get(0, 0), Some(CLEAR));
        // Row 0 is the north row, so it is the east view's last column.
        assert_eq!(east.get(3, 0), Some([0, 20, 0, 255]));
        assert_eq!(east.get(1, 0), Some([10, 0, 0, 255]));
    }
}
