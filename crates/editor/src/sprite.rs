//! A sprite being edited: its frames as plain RGBA pixels, loaded from the packed texture or a saved folder.

#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

use last_harvest_game::Stand;
use last_harvest_game::art::{self, ATLAS_W, Art};

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
    /// One row whose colors cap a figure's columns in the 3D view; a clear pixel leaves that column's own.
    pub top: Option<Frame>,
    /// An enemy card's `enemies.txt` section, when this is one of a card's parts.
    pub card: Option<String>,
}

pub const FOOTPRINT_FILE: &str = "footprint.png";
pub const TOP_FILE: &str = "top.png";
pub const CARD_FILE: &str = "enemy.txt";
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

pub const NEW_SECTION: &str = "New sprites";
/// A hand-made map lists as `MAP_<ID>` under Maps.
pub const MAP_PREFIX: &str = "MAP_";
pub const MAP_SECTION: &str = "Maps";
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

    /// Loads from whatever `read` returns for each file in the sprite's folder, such as an earlier commit.
    pub fn load_from(name: &str, read: impl Fn(&str) -> Option<Vec<u8>>) -> Option<Self> {
        let png = |file: &str| read(file).and_then(|bytes| decode_png(&bytes).ok());
        let mut frames = Vec::new();
        while let Some(f) = png(&format!("{}.png", frames.len())) {
            frames.push(f);
        }
        if frames.is_empty() {
            let (_, arts) = art::ALL.iter().find(|(n, _)| *n == name)?;
            frames = arts.iter().map(|&a| Frame::from_art(a)).collect();
        }
        let views = png(VIEW_FILES[0]).zip(png(VIEW_FILES[1])).map(|(s, e)| [s, e]);
        let card = read(CARD_FILE).and_then(|b| String::from_utf8(b).ok());
        Some(Self { name: name.to_string(), frames, views, footprint: png(FOOTPRINT_FILE), top: png(TOP_FILE), card })
    }

    /// The files a save writes: the frames, then the views and the footprint when it has them.
    pub fn files(&self) -> Vec<(String, Vec<u8>)> {
        let frames = self.frames.iter().enumerate().map(|(i, f)| (format!("{i}.png"), f));
        let views = self.views.iter().flat_map(|v| VIEW_FILES.iter().zip(v).map(|(n, f)| (n.to_string(), f)));
        let footprint = self.footprint.iter().map(|f| (FOOTPRINT_FILE.to_string(), f));
        let top = self.top.iter().map(|f| (TOP_FILE.to_string(), f));
        let pngs = frames.chain(views).chain(footprint).chain(top).map(|(n, f)| (n, encode_png(f)));
        pngs.chain(self.card.iter().map(|t| (CARD_FILE.to_string(), t.clone().into_bytes()))).collect()
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
        for (file, frame) in [(FOOTPRINT_FILE, &self.footprint), (TOP_FILE, &self.top)] {
            match frame {
                Some(f) => write_png(&path.join(file), f)?,
                None => {
                    let _ = std::fs::remove_file(path.join(file));
                }
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
        Stand::Slab { height } => format!("slab {height}"),
        Stand::Figure => "figure".into(),
        Stand::Ground => "ground".into(),
        Stand::Carve { .. } => "carve".into(),
    }
}

pub fn parse_stand(text: &str) -> Option<Stand> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let size = || words.get(1)?.parse::<f32>().ok();
    Some(match *words.first()? {
        "prop" => Stand::Prop { depth: size()? },
        "slab" => Stand::Slab { height: size()? },
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
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let channels = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        other => return Err(format!("unsupported {other:?}")),
    };
    let px = buf[..info.buffer_size()].chunks_exact(channels).map(|p| [p[0], p[1], p[2], if channels == 4 { p[3] } else { 255 }]).collect();
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

/// Every opaque color in the tileset, darkest first within each hue band, for drawing in the pack's style.
/// The palette when there's no tileset to take one from, as in the mod editor: PICO-8's 32 colors.
pub const OPEN_PALETTE: [Rgba; 32] = [
    [0, 0, 0, 255], [29, 43, 83, 255], [126, 37, 83, 255], [0, 135, 81, 255], [171, 82, 54, 255], [95, 87, 79, 255], [194, 195, 199, 255], [255, 241, 232, 255],
    [255, 0, 77, 255], [255, 163, 0, 255], [255, 236, 39, 255], [0, 228, 54, 255], [41, 173, 255, 255], [131, 118, 156, 255], [255, 119, 168, 255], [255, 204, 170, 255],
    [41, 24, 20, 255], [17, 29, 53, 255], [66, 33, 54, 255], [18, 83, 89, 255], [116, 47, 41, 255], [73, 51, 59, 255], [162, 136, 121, 255], [243, 239, 125, 255],
    [190, 18, 80, 255], [255, 108, 36, 255], [168, 231, 46, 255], [0, 181, 67, 255], [6, 90, 181, 255], [117, 70, 101, 255], [255, 110, 89, 255], [255, 157, 129, 255],
];

pub fn palette() -> Vec<Rgba> {
    let mut colors: Vec<Rgba> = art::pixels().chunks_exact(4).filter(|p| p[3] > 0).map(|p| [p[0], p[1], p[2], 255]).collect();
    colors.sort_unstable();
    colors.dedup();
    // Without the tileset, as in the card editor, the atlas holds only a few flat ground colors.
    if colors.len() < 16 {
        return OPEN_PALETTE.to_vec();
    }
    let key = |c: &Rgba| {
        let [r, g, b] = [c[0], c[1], c[2]].map(|v| v as f32 / 255.0);
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let light = (max + min) / 2.0;
        // Greys first, then by hue in twelve bands, each from dark to light.
        let band = if max - min < 0.08 {
            0
        } else {
            let hue = if max == r { (g - b) / (max - min) } else if max == g { 2.0 + (b - r) / (max - min) } else { 4.0 + (r - g) / (max - min) };
            1 + ((hue.rem_euclid(6.0) / 6.0 * 12.0) as i32).min(11)
        };
        (band, (light * 1000.0) as i32)
    };
    colors.sort_by_key(key);
    colors
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut s = Sprite { name: "TEST_THING".into(), frames: vec![Frame::blank(3, 2), Frame::blank(3, 2), Frame::blank(3, 2)], views: None, footprint: None, top: None, card: None };
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
        let mut s = Sprite { name: "THING".into(), frames: vec![Frame::blank(3, 2); 2], views: Some([Frame::blank(3, 4), Frame::blank(2, 4)]), footprint: None, top: Some(Frame::blank(3, 1)), card: None };
        s.frames[1].set(1, 1, [1, 2, 3, 255]);
        let files = unbundle(&bundle(&s.files())).unwrap();
        assert_eq!(files.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["0.png", "1.png", "south.png", "east.png", "top.png"]);
        let back = Sprite::from_files("THING", &files).unwrap();
        assert_eq!((back.frames, back.views, back.footprint, back.top), (s.frames, s.views, s.footprint, s.top));
        assert!(Sprite::from_files("THING", &files[1..]).is_none(), "a frame at least");
        assert!(unbundle(b"0.png\n9\nshort").is_none());
    }

    #[test]
    fn packed_sprites_load_from_the_texture() {
        let s = Sprite::load(Path::new("/nonexistent"), "BARREL").unwrap();
        assert_eq!((s.frames[0].w, s.frames[0].h), (16, 16));
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
        save_stand(&dir, &catalog, "CAR_V", &stand_text(Stand::Slab { height: 12.0 })).unwrap();
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
