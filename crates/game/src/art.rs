//! Sprites from the Zombie Apocalypse Tileset, packed at build time from `art.txt`.

use std::sync::{OnceLock, RwLock};

/// Sprites the editor adds while it runs, numbered after the packed ones: rects in the scratch area,
/// and a copy of the texture with the editor's pixels drawn in.
struct Live {
    rects: Vec<[u16; 4]>,
    pixels: Vec<u8>,
}

static LIVE: RwLock<Live> = RwLock::new(Live { rects: Vec::new(), pixels: Vec::new() });

/// Sprites from enemy cards, drawn into the texture's free rows at startup and numbered after the packed ones.
static ADDED: OnceLock<Vec<[u16; 4]>> = OnceLock::new();

fn added() -> &'static [[u16; 4]] {
    ADDED.get().map_or(&[], Vec::as_slice)
}

/// Sets the added sprites' rects; the texture the game reads must already hold their pixels.
pub(crate) fn set_added(rects: Vec<[u16; 4]>) {
    ADDED.set(rects).ok();
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
    *LIVE.write().unwrap() = Live { rects, pixels };
    arts
}

/// Runs `f` on the texture a sprite's pixels live in.
pub(crate) fn with_pixels<T>(art: Art, f: impl FnOnce(&[u8]) -> T) -> T {
    if art.is_live() { f(&LIVE.read().unwrap().pixels) } else { f(self::pixels()) }
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
    /// Lying flat as a roof `height` pixels up, its edges drawn down to the ground: for sprites drawn from above, like cars.
    Slab { height: f32 },
    /// Turning to face the camera like a character.
    Figure,
    /// Flat on the ground like a tile or a stain.
    Ground,
    /// A solid carved where the view from above agrees with drawings of it seen from the south and the east.
    /// The outline from above is `footprint` when given, else the sprite's own; the sprite colors the top either way.
    Carve { footprint: Option<Art>, south: Art, east: Art },
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
        STANDS.get(self.0 as usize).copied().flatten()
    }

    /// The top strip drawn for this sprite in the editor: one row whose colors cap its columns in the 3D view.
    pub fn top(self) -> Option<Art> {
        TOPS.get(self.0 as usize).copied().flatten()
    }

    /// Whether the editor added this sprite while running.
    pub fn is_live(self) -> bool {
        self.0 as usize >= count()
    }

    pub fn w(self) -> f32 {
        self.rect()[2] as f32
    }

    pub fn h(self) -> f32 {
        self.rect()[3] as f32
    }
}

include!(concat!(env!("OUT_DIR"), "/art_gen.rs"));
