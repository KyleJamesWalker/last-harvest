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

/// Makes clear every pixel of a `key` color reachable from the border through other such pixels,
/// so a sprite drawn on a baked-in ground loses the ground but keeps the same colors inside its outline.
fn clear_from_edges(img: &mut Image, key: &[[u8; 3]]) {
    if key.is_empty() {
        return;
    }
    let (w, h) = (img.w as i32, img.h as i32);
    let idx = |x: i32, y: i32| ((y * w + x) * 4) as usize;
    let keyed = |img: &Image, x: i32, y: i32| {
        let i = idx(x, y);
        img.rgba[i + 3] != 0 && key.contains(&[img.rgba[i], img.rgba[i + 1], img.rgba[i + 2]])
    };
    let mut stack: Vec<(i32, i32)> = (0..w).flat_map(|x| [(x, 0), (x, h - 1)]).chain((0..h).flat_map(|y| [(0, y), (w - 1, y)])).collect();
    while let Some((x, y)) = stack.pop() {
        if x < 0 || y < 0 || x >= w || y >= h || !keyed(img, x, y) {
            continue;
        }
        let i = idx(x, y);
        img.rgba[i + 3] = 0;
        stack.extend([(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]);
    }
}

fn decode(path: &Path) -> Result<Image, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    if info.bit_depth != png::BitDepth::Eight || !matches!(info.color_type, png::ColorType::Rgba | png::ColorType::Rgb) {
        return Err(format!("expected 8-bit RGBA or RGB, got {:?} {:?}", info.color_type, info.bit_depth));
    }
    buf.truncate(info.buffer_size());
    if info.color_type == png::ColorType::Rgb {
        buf = buf.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
    }
    // A few stray, nearly clear pixels would otherwise show as faint squares.
    for px in buf.chunks_exact_mut(4) {
        px[3] = if px[3] >= 128 { 255 } else { 0 };
    }
    Ok(Image { w: info.width, h: info.height, rgba: buf })
}

fn cut(sheet: &Image, dir: &Path, frame: &str, line: usize) -> Vec<Image> {
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
        Some("png") if (2..=3).contains(&words.len()) => {
            let frames = match words.get(2) {
                Some(n) => num(n.strip_prefix('x').unwrap_or_else(|| panic!("art.txt:{line}: bad frame count {n:?}"))),
                None => 1,
            };
            let path = dir.join(words[1]);
            println!("cargo:rerun-if-changed={}", path.display());
            let img = decode(&path).unwrap_or_else(|e| {
                println!("cargo:warning=art.txt:{line}: {}: {e}; drawn as a magenta square", path.display());
                let mut img = Image::new(8 * frames, 8);
                img.rgba.chunks_exact_mut(4).for_each(|p| p.copy_from_slice(&[255, 0, 255, 255]));
                img
            });
            let w = img.w / frames;
            (0..frames)
                .map(|f| {
                    let mut out = Image::new(w, img.h);
                    out.blit(&img, [f * w, 0], [w, img.h], [0, 0]);
                    out
                })
                .collect()
        }
        Some(kind @ ("solid" | "blank")) if words.len() == 3 => {
            let mut img = Image::new(num(words[1]), num(words[2]));
            if kind == "solid" {
                img.rgba.fill(255);
            }
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
            let mut key: Vec<[u8; 3]> = Vec::new();
            let pair = |s: &str| s.split_once(',').map(|(a, b)| (num(a), num(b)));
            for word in &words[1 + nums.len()..] {
                if let Some(n) = word.strip_prefix('x') {
                    frames = num(n);
                } else if let Some(p) = word.strip_prefix('-').and_then(pair) {
                    cleared.push(p);
                } else if let Some(list) = word.strip_prefix("key=") {
                    for hex in list.split(',') {
                        let n = u32::from_str_radix(hex, 16).unwrap_or_else(|_| panic!("art.txt:{line}: bad color {hex:?}"));
                        key.push([(n >> 16) as u8, (n >> 8) as u8, n as u8]);
                    }
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
                    clear_from_edges(&mut img, &key);
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

/// Rust for a stand written as `prop 12`, `slab 10`, `figure` or `ground`.
fn stand_code(how: &str, line: usize) -> String {
    let words: Vec<&str> = how.split_whitespace().collect();
    let size = || words.get(1).and_then(|w| w.parse::<f32>().ok()).unwrap_or_else(|| panic!("art.txt:{line}: {how:?} needs a size in pixels"));
    match words.first().copied() {
        Some("prop") => format!("Stand::Prop {{ depth: {:?} }}", size()),
        Some("slab") => format!("Stand::Slab {{ height: {:?} }}", size()),
        Some("figure") => "Stand::Figure".into(),
        Some("ground") => "Stand::Ground".into(),
        _ => panic!("art.txt:{line}: unknown stand {how:?}; use prop N, slab N, figure or ground"),
    }
}

/// Rust for a stand, packing a carved sprite's south and east views, and its footprint if it has one, from its folder.
/// Without them, as in a clone that lacks the saved sprites, a carved sprite falls back to a slab.
fn stand_for(how: &str, dir: &Path, intern: &mut impl FnMut(Image) -> usize, line: usize) -> String {
    if how != "carve" {
        return stand_code(how, line);
    }
    match (decode(&dir.join("south.png")), decode(&dir.join("east.png"))) {
        (Ok(south), Ok(east)) => {
            let footprint = decode(&dir.join("footprint.png")).ok().map_or("None".to_string(), |s| format!("Some(Art({}))", intern(s)));
            format!("Stand::Carve {{ footprint: {footprint}, south: Art({}), east: Art({}) }}", intern(south), intern(east))
        }
        _ => {
            println!("cargo:warning={} is carved but has no south.png and east.png; it stands as a slab", dir.display());
            "Stand::Slab { height: 10.0 }".into()
        }
    }
}

/// A sprite name the generated code can use as a constant.
fn is_name(n: &str) -> bool {
    n.starts_with(|c: char| c.is_ascii_uppercase()) && n.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// The frames saved for a sprite as `0.png`, `1.png` and so on, or none.
fn saved(dir: &Path) -> Vec<Image> {
    let mut frames = Vec::new();
    loop {
        let path = dir.join(format!("{}.png", frames.len()));
        if !path.exists() {
            return frames;
        }
        frames.push(decode(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())));
    }
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let catalog = manifest.join("art.txt");
    let reference = manifest.join("../../assets/private/zombie-apocalypse/Zombie Apocalypse Tileset Reference.png");
    println!("cargo:rerun-if-changed={}", catalog.display());
    println!("cargo:rerun-if-changed={}", reference.display());
    let sprites = manifest.join("../../assets/private/sprites");
    println!("cargo:rerun-if-changed={}", manifest.join("../../assets/private").display());
    let sheet = decode(&reference).unwrap_or_else(|e| {
        panic!("{}: {e}\nGet the Zombie Apocalypse Tileset from itch.io and run scripts/assets.sh <zip>.", reference.display())
    });

    // Each entry is a name and its frames, as indices into the deduplicated image list.
    let mut entries: Vec<(String, Vec<usize>)> = Vec::new();
    // How each entry stands in the 3D view, when the catalog or the editor says.
    let mut stands: Vec<(String, String)> = Vec::new();
    // A figure's top strip from the editor, by entry name and its image.
    let mut tops: Vec<(String, usize)> = Vec::new();
    let mut images: Vec<Image> = Vec::new();
    let mut seen: HashMap<Vec<u8>, usize> = HashMap::new();
    let mut intern = |img: Image| {
        let mut key = img.rgba.clone();
        key.extend(img.w.to_le_bytes());
        *seen.entry(key).or_insert_with(|| {
            images.push(img);
            images.len() - 1
        })
    };
    let text = std::fs::read_to_string(&catalog).unwrap();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap().trim();
        let Some((name, rest)) = line.split_once(char::is_whitespace) else { continue };
        let (rest, how) = rest.split_once('@').unwrap_or((rest, ""));
        let how = std::fs::read_to_string(sprites.join(name).join("stand.txt")).unwrap_or_else(|_| how.to_string());
        if !how.trim().is_empty() {
            let code = stand_for(how.trim(), &sprites.join(name), &mut intern, i + 1);
            stands.push((name.to_string(), code));
        }
        // A sprite saved from the editor takes the place of its cut from the sheet.
        if let Ok(top) = decode(&sprites.join(name).join("top.png")) {
            tops.push((name.to_string(), intern(top)));
        }
        let drawn = saved(&sprites.join(name));
        let frames = if drawn.is_empty() { rest.split(';').flat_map(|frame| cut(&sheet, &manifest, frame.trim(), i + 1)).collect() } else { drawn };
        entries.push((name.to_string(), frames.into_iter().map(&mut intern).collect()));
    }
    // Sprites drawn from scratch in the editor, with no line in the catalog.
    let mut extra: Vec<String> = std::fs::read_dir(&sprites)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| is_name(n) && !entries.iter().any(|(e, _)| e == n))
        .collect();
    extra.sort();
    for name in extra {
        let frames = saved(&sprites.join(&name));
        if let Ok(how) = std::fs::read_to_string(sprites.join(&name).join("stand.txt")) {
            let code = stand_for(how.trim(), &sprites.join(&name), &mut intern, 0);
            stands.push((name.clone(), code));
        }
        if let Ok(top) = decode(&sprites.join(&name).join("top.png")) {
            tops.push((name.clone(), intern(top)));
        }
        if !frames.is_empty() {
            entries.push((name, frames.into_iter().map(&mut intern).collect()));
        }
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
    let mut by_image = vec!["None".to_string(); images.len()];
    for (name, code) in &stands {
        if let Some((_, ids)) = entries.iter().find(|(n, _)| n == name) {
            for &id in ids {
                by_image[id] = format!("Some({code})");
            }
        }
    }
    writeln!(src, "pub(crate) static STANDS: [Option<Stand>; {}] = [{}];", by_image.len(), by_image.join(", ")).unwrap();
    let mut top_of = vec!["None".to_string(); images.len()];
    for (name, top) in &tops {
        if let Some((_, ids)) = entries.iter().find(|(n, _)| n == name) {
            for &id in ids {
                top_of[id] = format!("Some(Art({top}))");
            }
        }
    }
    writeln!(src, "pub(crate) static TOPS: [Option<Art>; {}] = [{}];", top_of.len(), top_of.join(", ")).unwrap();
    writeln!(src, "/// Every catalog entry with its frames, for the art gallery.\npub static ALL: &[(&str, &[Art])] = &[{}];", names.join(", ")).unwrap();
    std::fs::write(out.join("art_gen.rs"), src).unwrap();
}
