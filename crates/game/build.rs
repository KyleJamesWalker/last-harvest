//! Packs the sprites listed in art.txt, cut from the tileset's reference sheet, into one RGBA texture
//! and generates `art_gen.rs` with a constant per sprite.

use std::collections::HashMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};

/// The reference sheet lays 16 px cells out with 1 px gaps across and 3 px gaps down.
const ORIGIN: [u32; 2] = [17, 19];
const PITCH: [u32; 2] = [17, 19];
const CELL: u32 = 16;
const ATLAS_W: u32 = 512;
const PAD: u32 = 1;

struct Image {
    w: u32,
    h: u32,
    rgba: Vec<u8>,
}

impl Image {
    fn new(w: u32, h: u32) -> Self {
        Self { w, h, rgba: vec![0; (w * h * 4) as usize] }
    }

    fn blit(&mut self, src: &Image, [sx, sy]: [u32; 2], [w, h]: [u32; 2], [dx, dy]: [u32; 2]) {
        for row in 0..h {
            let s = (((sy + row) * src.w + sx) * 4) as usize;
            let d = (((dy + row) * self.w + dx) * 4) as usize;
            self.rgba[d..d + (w * 4) as usize].copy_from_slice(&src.rgba[s..s + (w * 4) as usize]);
        }
    }
}

impl Image {
    /// Like `blit`, but clear source pixels leave the destination as it was.
    fn blit_over(&mut self, src: &Image, [sx, sy]: [u32; 2], [w, h]: [u32; 2], [dx, dy]: [u32; 2]) {
        for row in 0..h {
            for col in 0..w {
                let s = (((sy + row) * src.w + sx + col) * 4) as usize;
                if src.rgba[s + 3] != 0 {
                    let d = (((dy + row) * self.w + dx + col) * 4) as usize;
                    self.rgba[d..d + 4].copy_from_slice(&src.rgba[s..s + 4]);
                }
            }
        }
    }
}

fn decode(path: &Path) -> Result<Image, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Err(format!("expected 8-bit RGBA, got {:?} {:?}", info.color_type, info.bit_depth));
    }
    buf.truncate(info.buffer_size());
    // A few stray, nearly clear pixels would otherwise show as faint squares.
    for px in buf.chunks_exact_mut(4) {
        px[3] = if px[3] >= 128 { 255 } else { 0 };
    }
    Ok(Image { w: info.width, h: info.height, rgba: buf })
}

fn cut(sheet: &Image, frame: &str, line: usize) -> Vec<Image> {
    let words: Vec<&str> = frame.split_whitespace().collect();
    let bad = || -> Vec<Image> { panic!("art.txt:{line}: bad frame {frame:?}") };
    let num = |s: &str| s.parse::<u32>().unwrap_or_else(|_| panic!("art.txt:{line}: bad number {s:?}"));
    match words.first().copied() {
        Some("px") if words.len() == 5 => {
            let [x, y, w, h] = [1, 2, 3, 4].map(|i| num(words[i]));
            let mut img = Image::new(w, h);
            img.blit(sheet, [x, y], [w, h], [0, 0]);
            vec![img]
        }
        Some("cell") => {
            let nums: Vec<u32> = words[1..].iter().take_while(|w| w.parse::<u32>().is_ok()).map(|w| num(w)).collect();
            let (c, r, w, h) = match nums[..] {
                [c, r] => (c, r, 1, 1),
                [c, r, w, h] => (c, r, w, h),
                _ => return bad(),
            };
            let mut frames = 1;
            let mut cleared = Vec::new();
            let mut under = Vec::new();
            let mut crop = None;
            let pair = |s: &str| s.split_once(',').map(|(a, b)| (num(a), num(b)));
            for word in &words[1 + nums.len()..] {
                if let Some(n) = word.strip_prefix('x') {
                    frames = num(n);
                } else if let Some(p) = word.strip_prefix('-').and_then(pair) {
                    cleared.push(p);
                } else if let Some(p) = word.strip_prefix('/').and_then(pair) {
                    crop = Some(p);
                } else if let Some((cell, below)) = word.split_once('<').and_then(|(a, b)| Some((pair(a)?, pair(b)?))) {
                    under.push((cell, below));
                } else {
                    return bad();
                }
            }
            let at = |(cc, rr): (u32, u32)| (ORIGIN[0] + cc * PITCH[0], ORIGIN[1] + rr * PITCH[1]);
            (0..frames)
                .map(|f| {
                    let mut img = Image::new(w * CELL, h * CELL);
                    for j in 0..h {
                        for i in 0..w {
                            let cell = (c + f * w + i, r + j);
                            if let Some(&(_, below)) = under.iter().find(|(c, _)| *c == cell) {
                                let (sx, sy) = at(below);
                                img.blit_over(sheet, [sx, sy], [CELL, CELL], [i * CELL, j * CELL]);
                            }
                            if !cleared.contains(&cell) {
                                let (sx, sy) = at(cell);
                                img.blit_over(sheet, [sx, sy], [CELL, CELL], [i * CELL, j * CELL]);
                            }
                        }
                    }
                    match crop {
                        Some((cw, ch)) => {
                            let mut out = Image::new(cw, ch);
                            out.blit(&img, [0, 0], [cw, ch], [0, 0]);
                            out
                        }
                        None => img,
                    }
                })
                .collect()
        }
        _ => bad(),
    }
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let catalog = manifest.join("art.txt");
    let reference = manifest.join("../../assets/zombie-apocalypse/Zombie Apocalypse Tileset Reference.png");
    println!("cargo:rerun-if-changed={}", catalog.display());
    println!("cargo:rerun-if-changed={}", reference.display());
    let sheet = decode(&reference).unwrap_or_else(|e| {
        panic!("{}: {e}\nGet the Zombie Apocalypse Tileset from itch.io and run scripts/assets.sh <zip>.", reference.display())
    });

    // Each entry is a name and its frames, as indices into the deduplicated image list.
    let mut entries: Vec<(String, Vec<usize>)> = Vec::new();
    let mut images: Vec<Image> = Vec::new();
    let mut seen: HashMap<Vec<u8>, usize> = HashMap::new();
    let text = std::fs::read_to_string(&catalog).unwrap();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap().trim();
        let Some((name, rest)) = line.split_once(char::is_whitespace) else { continue };
        let mut ids = Vec::new();
        for frame in rest.split(';') {
            for img in cut(&sheet, frame.trim(), i + 1) {
                let mut key = img.rgba.clone();
                key.extend(img.w.to_le_bytes());
                let id = *seen.entry(key).or_insert_with(|| {
                    images.push(img);
                    images.len() - 1
                });
                ids.push(id);
            }
        }
        entries.push((name.to_string(), ids));
    }

    // Shelf packing, tallest first.
    let mut order: Vec<usize> = (0..images.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse((images[i].h, images[i].w)));
    let mut rects = vec![[0u32; 4]; images.len()];
    let (mut x, mut y, mut shelf) = (PAD, PAD, 0);
    for &i in &order {
        let img = &images[i];
        assert!(img.w + 2 * PAD <= ATLAS_W, "sprite wider than the atlas");
        if x + img.w + PAD > ATLAS_W {
            (x, y, shelf) = (PAD, y + shelf + PAD, 0);
        }
        rects[i] = [x, y, img.w, img.h];
        x += img.w + PAD;
        shelf = shelf.max(img.h);
    }
    let atlas_h = (y + shelf + PAD).next_power_of_two();
    let mut atlas = Image::new(ATLAS_W, atlas_h);
    for (img, r) in images.iter().zip(&rects) {
        atlas.blit(img, [0, 0], [img.w, img.h], [r[0], r[1]]);
    }
    std::fs::write(out.join("art.rgba"), &atlas.rgba).unwrap();

    let mut src = String::new();
    writeln!(src, "pub const ATLAS_W: u32 = {ATLAS_W};\npub const ATLAS_H: u32 = {atlas_h};").unwrap();
    writeln!(src, "pub static PIXELS: &[u8] = include_bytes!(concat!(env!(\"OUT_DIR\"), \"/art.rgba\"));").unwrap();
    writeln!(src, "pub(crate) const RECTS: [[u16; 4]; {}] = [", rects.len()).unwrap();
    for r in &rects {
        writeln!(src, "    [{}, {}, {}, {}],", r[0], r[1], r[2], r[3]).unwrap();
    }
    writeln!(src, "];").unwrap();
    let mut names = Vec::new();
    for (name, ids) in &entries {
        let arts: Vec<String> = ids.iter().map(|id| format!("Art({id})")).collect();
        if ids.len() == 1 {
            writeln!(src, "pub const {name}: Art = {};", arts[0]).unwrap();
            names.push(format!("(\"{name}\", &[{name}])"));
        } else {
            writeln!(src, "pub static {name}: [Art; {}] = [{}];", ids.len(), arts.join(", ")).unwrap();
            names.push(format!("(\"{name}\", &{name})"));
        }
    }
    writeln!(src, "/// Every catalog entry with its frames, for the art gallery.\npub static ALL: &[(&str, &[Art])] = &[{}];", names.join(", ")).unwrap();
    std::fs::write(out.join("art_gen.rs"), src).unwrap();
}
