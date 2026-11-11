//! Sprites from the Zombie Apocalypse Tileset, packed at build time from `art.txt`.

use std::sync::{OnceLock, RwLock};

use crate::worn::Worn;

/// Sprites the editor adds while it runs, numbered after the packed ones: rects in the scratch area,
/// and a copy of the texture with the editor's pixels drawn in.
struct Live {
    rects: Vec<[u16; 4]>,
    pixels: Vec<u8>,
    /// Each live sprite with the copy the 3D view stands up in its place, as `Art::solid` says.
    solids: Vec<(Art, Art)>,
    /// Where live frames wear a ready grenade, as `Art::worn` says.
    worn: Vec<(Art, Worn)>,
    /// Each live sprite's detail, by place, as `Art::detail` says.
    detail: Vec<u8>,
    /// Each live sprite's alpha, by place, as `Art::alpha` says.
    alpha: Vec<f32>,
}

static LIVE: RwLock<Live> = RwLock::new(Live { rects: Vec::new(), pixels: Vec::new(), solids: Vec::new(), worn: Vec::new(), detail: Vec::new(), alpha: Vec::new() });

/// Counts `set_live` calls, so work done from the live sprites can be kept until they change.
static LIVE_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub(crate) fn live_generation() -> u64 {
    LIVE_GENERATION.load(std::sync::atomic::Ordering::Acquire)
}

/// The details a sprite can be drawn at: how many texture pixels make one game pixel across and down.
pub const DETAILS: [u8; 3] = [1, 2, 4];

/// Sprites from enemy cards, drawn into the texture's free rows at startup and numbered after the packed ones.
static ADDED: OnceLock<Vec<[u16; 4]>> = OnceLock::new();

fn added() -> &'static [[u16; 4]] {
    ADDED.get().map_or(&[], Vec::as_slice)
}

/// Added sprites paired with the copies the 3D view stands up in their place, as `Art::solid` says.
static ADDED_SOLIDS: OnceLock<Vec<(Art, Art)>> = OnceLock::new();

pub(crate) fn set_added_solids(solids: Vec<(Art, Art)>) {
    ADDED_SOLIDS.set(solids).ok();
}

/// Where added sprites wear a ready grenade, as `Art::worn` says.
static ADDED_WORN: OnceLock<Vec<(Art, Worn)>> = OnceLock::new();

pub(crate) fn set_added_worn(worn: Vec<(Art, Worn)>) {
    ADDED_WORN.set(worn).ok();
}

/// Each added sprite's detail, by place, as `Art::detail` says.
static ADDED_DETAIL: OnceLock<Vec<u8>> = OnceLock::new();

/// Sets the added sprites' rects and details; the texture the game reads must already hold their pixels.
pub(crate) fn set_added(rects: Vec<[u16; 4]>, detail: Vec<u8>) {
    ADDED.set(rects).ok();
    ADDED_DETAIL.set(detail).ok();
}

/// Packed and added sprites, every one but the editor's live ones.
pub fn count() -> usize {
    RECTS.len() + added().len()
}

/// The first texture row below every packed sprite.
pub(crate) fn free_top() -> u16 {
    RECTS.iter().map(|r| r[1] + r[3]).max().unwrap_or(0)
}

/// Replaces the live sprites with `rects` into `pixels`, a full texture the size of the packed one, and returns them.
pub fn set_live(rects: Vec<[u16; 4]>, pixels: Vec<u8>) -> Vec<Art> {
    assert_eq!(pixels.len(), self::pixels().len(), "a live texture matches the packed one");
    let arts = (0..rects.len()).map(|i| Art((count() + i) as u16)).collect();
    *LIVE.write().unwrap() = Live { rects, pixels, solids: Vec::new(), worn: Vec::new(), detail: Vec::new(), alpha: Vec::new() };
    LIVE_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Release);
    arts
}

/// Pairs live sprites with the copies the 3D view stands up in their place.
pub fn set_live_solids(solids: Vec<(Art, Art)>) {
    LIVE.write().unwrap().solids = solids;
}

/// Says where live frames wear a ready grenade.
pub fn set_live_worn(worn: Vec<(Art, Worn)>) {
    LIVE.write().unwrap().worn = worn;
}

/// Sets the live sprites' details, by place; those left out are drawn at 1.
pub fn set_live_detail(detail: Vec<u8>) {
    LIVE.write().unwrap().detail = detail;
}

/// Sets the live sprites' alphas, by place; those left out are opaque.
pub fn set_live_alpha(alpha: Vec<f32>) {
    LIVE.write().unwrap().alpha = alpha;
}

/// Runs `f` on the texture a sprite's pixels live in.
pub(crate) fn with_pixels<T>(art: Art, f: impl FnOnce(&[u8]) -> T) -> T {
    if art.is_live() { f(&LIVE.read().unwrap().pixels) } else { f(self::pixels()) }
}

/// Runs `f` on a sprite's outline in game pixels, solid where any of a pixel's texture pixels is, and its size.
pub(crate) fn with_outline<T>(art: Art, f: impl FnOnce(&dyn Fn(usize, usize) -> bool, [usize; 2]) -> T) -> T {
    let [rx, ry, w, h] = art.rect().map(usize::from);
    let d = art.detail() as usize;
    with_pixels(art, |px| {
        let solid = |x: usize, y: usize| x < w && y < h && px[((ry + y) * ATLAS_W as usize + rx + x) * 4 + 3] > 0;
        f(&|x, y| (0..d).any(|j| (0..d).any(|i| solid(x * d + i, y * d + j))), [w.div_ceil(d), h.div_ceil(d)])
    })
}

/// Held by tests that set the live sprites or the players' props, which every test in the run shares.
#[cfg(test)]
pub(crate) fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Live copies of packed sprites drawn `d` times finer, each pixel a `d` by `d` block, at detail `d`.
#[cfg(test)]
pub(crate) fn live_detailed(arts: &[Art], d: u16) -> Vec<Art> {
    let mut px = pixels().to_vec();
    let (mut x, mut y, mut shelf, mut rects) = (0u16, free_top() + 1, 0u16, Vec::new());
    for a in arts {
        let [ax, ay, w, h] = a.rect();
        if x + w * d > ATLAS_W as u16 {
            (x, y, shelf) = (0, y + shelf + 1, 0);
        }
        for j in 0..h * d {
            for i in 0..w * d {
                let from = ((ay + j / d) as usize * ATLAS_W as usize + (ax + i / d) as usize) * 4;
                let to = ((y + j) as usize * ATLAS_W as usize + (x + i) as usize) * 4;
                px.copy_within(from..from + 4, to);
            }
        }
        rects.push([x, y, w * d, h * d]);
        (x, shelf) = (x + w * d + 1, shelf.max(h * d));
    }
    let n = rects.len();
    let live = set_live(rects, px);
    set_live_detail(vec![d as u8; n]);
    live
}

/// The packed texture, `ATLAS_W` by `ATLAS_H` RGBA.
pub fn pixels() -> &'static [u8] {
    crate::data::atlas()
}

/// How a sprite stands in the 3D view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stand {
    /// Upright and fixed to the map like a wall, extruded `depth` pixels back.
    Prop { depth: f32 },
    /// Lying flat as a roof `height` pixels up, its edges drawn down to the ground: for sprites drawn from above, like
    /// cars. Floated `lift` pixels off the ground, it is a plate `height` thick there, as an effect seen from above is.
    Slab { height: f32, lift: f32 },
    /// Turning to face the camera like a character, floated `lift` pixels off the ground.
    Figure { lift: f32 },
    /// Flat on the ground like a tile or a stain.
    Ground,
    /// A solid carved where the view from above agrees with drawings of it seen from the south and the east.
    /// The outline from above is `footprint` when given, else the sprite's own; `roof` colors the top when given,
    /// else the sprite does. `north` and `west` color those sides when given, else the south and east views do.
    Carve { footprint: Option<Art>, roof: Option<Art>, south: Art, east: Art, north: Option<Art>, west: Option<Art> },
    /// Upright like a prop below row `rows`, with the rows above it laid flat over it as its roof, as deep as they are
    /// tall: for buildings drawn with their roof seen from above and their front seen from the south.
    Fold { rows: f32 },
    /// Upright like a prop but running north to south, as `east` draws it seen from the east, centered where the
    /// sprite's outline from above puts it and `depth` pixels thick, or as thick as that outline: for pieces drawn
    /// from above, like a fence's sides.
    Turned { east: Art, depth: Option<f32> },
}

/// One sprite in the packed texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Art(pub u16);

impl Art {
    /// Pixel rect in the texture: x, y, w, h.
    pub fn rect(self) -> [u16; 4] {
        let i = self.0 as usize;
        match RECTS.get(i).or_else(|| added().get(i - RECTS.len())) {
            Some(&r) => r,
            None => LIVE.read().unwrap().rects[i - count()],
        }
    }

    /// How the catalog says this sprite stands, if it says.
    pub fn stand(self) -> Option<Stand> {
        match STANDS.get(self.0 as usize) {
            Some(s) => *s,
            None => crate::harvest::stand_of(self),
        }
    }

    /// The top strip drawn for this sprite in the editor: one row whose colors cap its columns in the 3D view.
    pub fn top(self) -> Option<Art> {
        TOPS.get(self.0 as usize).copied().flatten()
    }

    /// What the 3D view stands up for this sprite: a copy without the colors the editor clears in 3D, else itself.
    pub fn solid(self) -> Art {
        if self.is_live() {
            return LIVE.read().unwrap().solids.iter().find(|(a, _)| *a == self).map_or(self, |&(_, s)| s);
        }
        match SOLIDS.get(self.0 as usize) {
            Some(s) => s.unwrap_or(self),
            None => ADDED_SOLIDS.get().and_then(|v| v.iter().find(|(a, _)| *a == self)).map_or(self, |&(_, s)| s),
        }
    }

    /// Where this frame wears a ready grenade, when its sprite or card says.
    pub fn worn(self) -> Option<Worn> {
        if self.is_live() {
            return LIVE.read().unwrap().worn.iter().find(|(a, _)| *a == self).map(|&(_, w)| w);
        }
        match WORN.get(self.0 as usize) {
            Some(spec) => spec.and_then(|s| crate::worn::parse(s).ok()?.first().copied()),
            None => ADDED_WORN.get().and_then(|v| v.iter().find(|(a, _)| *a == self)).map(|&(_, w)| w),
        }
    }

    /// Where this frame wears a ready grenade: where it says, else on its right hip; None when it hides it.
    pub fn worn_at(self) -> Option<[u8; 2]> {
        match self.worn() {
            Some(Worn::At(p)) => Some(p),
            Some(Worn::Hidden) => None,
            None => {
                let [x, y, w, h] = self.rect().map(usize::from);
                let stride = (ATLAS_W as usize) * 4;
                Some(with_pixels(self, |px| crate::worn::default_at(w, h, |i, j| px[(y + j) * stride + (x + i) * 4 + 3] > 0)))
            }
        }
    }

    /// Whether the editor added this sprite while running.
    pub fn is_live(self) -> bool {
        self.0 as usize >= count()
    }

    /// How many texture pixels make one game pixel across and down: 1, 2 or 4.
    pub fn detail(self) -> u8 {
        let i = self.0 as usize;
        if let Some(&d) = DETAIL.get(i) {
            return d;
        }
        let d = if i < count() { ADDED_DETAIL.get().and_then(|v| v.get(i - RECTS.len())).copied() } else { LIVE.read().unwrap().detail.get(i - count()).copied() };
        d.unwrap_or(1)
    }

    /// How opaque it draws, from `alpha::MIN` to 1: its folder's `alpha.txt`, its prop's `alpha` line, or the
    /// editor's.
    pub fn alpha(self) -> f32 {
        let i = self.0 as usize;
        if let Some(&a) = ALPHA.get(i) {
            return a as f32 / 255.0;
        }
        if i < count() {
            return crate::harvest::alpha_of(self).unwrap_or(1.0);
        }
        LIVE.read().unwrap().alpha.get(i - count()).copied().unwrap_or(1.0)
    }

    /// Width in game pixels: its texture pixels over its detail.
    pub fn w(self) -> f32 {
        self.rect()[2] as f32 / self.detail() as f32
    }

    /// Height in game pixels: its texture pixels over its detail.
    pub fn h(self) -> f32 {
        self.rect()[3] as f32 / self.detail() as f32
    }

    /// The average color of its opaque pixels, red, green and blue from 0 to 1; None for an editor's live sprite.
    pub fn mean(self) -> Option<[f32; 3]> {
        if self.is_live() {
            return None;
        }
        let [x, y, w, h] = self.rect().map(usize::from);
        let px = pixels();
        let (mut sum, mut n) = ([0.0f32; 3], 0.0f32);
        for j in y..y + h {
            for i in x..x + w {
                let Some(p) = px.get((j * ATLAS_W as usize + i) * 4..(j * ATLAS_W as usize + i) * 4 + 4) else { continue };
                if p[3] >= 128 {
                    (0..3).for_each(|k| sum[k] += p[k] as f32 / 255.0);
                    n += 1.0;
                }
            }
        }
        (n > 0.0).then(|| sum.map(|v| v / n))
    }
}

include!(concat!(env!("OUT_DIR"), "/art_gen.rs"));
