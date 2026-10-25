//! A sprite being edited: its frames as plain RGBA pixels, loaded from the packed texture or a saved folder.

use std::path::{Path, PathBuf};

use last_harvest_game::Stand;
use last_harvest_game::art::{self, ATLAS_W, Art, PIXELS};

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
            .map(|i| [PIXELS[i], PIXELS[i + 1], PIXELS[i + 2], PIXELS[i + 3]])
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
}

pub const FOOTPRINT_FILE: &str = "footprint.png";

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

/// Where saved sprites live; the game's build packs them in place of, or beside, the tileset's.
pub fn folder() -> PathBuf {
    std::env::var_os("LAST_HARVEST_SPRITES").map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/sprites"))
}

/// A name the build can turn into a constant: a capital letter, then capitals, digits and underscores.
pub fn valid_name(n: &str) -> bool {
    n.starts_with(|c: char| c.is_ascii_uppercase()) && n.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

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
    pub fn load(dir: &Path, name: &str) -> Option<Self> {
        let mut frames = Vec::new();
        while let Ok(f) = read_png(&dir.join(name).join(format!("{}.png", frames.len()))) {
            frames.push(f);
        }
        if frames.is_empty() {
            let (_, arts) = art::ALL.iter().find(|(n, _)| *n == name)?;
            frames = arts.iter().map(|&a| Frame::from_art(a)).collect();
        }
        let view = |f: &str| read_png(&dir.join(name).join(f)).ok();
        let views = view(VIEW_FILES[0]).zip(view(VIEW_FILES[1])).map(|(s, e)| [s, e]);
        Some(Self { name: name.to_string(), frames, views, footprint: view(FOOTPRINT_FILE) })
    }

    /// Writes `0.png`, `1.png` and so on, removing frames left over from a longer save.
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
        match &self.footprint {
            Some(f) => write_png(&path.join(FOOTPRINT_FILE), f)?,
            None => {
                let _ = std::fs::remove_file(path.join(FOOTPRINT_FILE));
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
fn catalog_line(text: &str, name: &str) -> Option<(usize, String)> {
    text.lines().enumerate().find(|(_, l)| l.split_whitespace().next() == Some(name) && !l.starts_with('#')).map(|(i, l)| (i, l.to_string()))
}

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

fn read_png(path: &Path) -> Result<Frame, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let channels = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        other => return Err(format!("{}: unsupported {other:?}", path.display())),
    };
    let px = buf[..info.buffer_size()].chunks_exact(channels).map(|p| [p[0], p[1], p[2], if channels == 4 { p[3] } else { 255 }]).collect();
    Ok(Frame { w: info.width as usize, h: info.height as usize, px })
}

fn write_png(path: &Path, f: &Frame) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), f.w as u32, f.h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let data: Vec<u8> = f.px.iter().flatten().copied().collect();
    enc.write_header().and_then(|mut w| w.write_image_data(&data)).map_err(|e| format!("{}: {e}", path.display()))
}

/// Every opaque color in the tileset, darkest first within each hue band, for drawing in the pack's style.
pub fn palette() -> Vec<Rgba> {
    let mut colors: Vec<Rgba> = PIXELS.chunks_exact(4).filter(|p| p[3] > 0).map(|p| [p[0], p[1], p[2], 255]).collect();
    colors.sort_unstable();
    colors.dedup();
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
        let mut s = Sprite { name: "TEST_THING".into(), frames: vec![Frame::blank(3, 2), Frame::blank(3, 2), Frame::blank(3, 2)], views: None, footprint: None };
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
