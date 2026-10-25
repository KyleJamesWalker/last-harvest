//! A sprite being edited: its frames as plain RGBA pixels, loaded from the packed texture or a saved folder.

use std::path::{Path, PathBuf};

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
        Some(Self { name: name.to_string(), frames })
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
        Ok(())
    }
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
        let mut s = Sprite { name: "TEST_THING".into(), frames: vec![Frame::blank(3, 2), Frame::blank(3, 2), Frame::blank(3, 2)] };
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
}
