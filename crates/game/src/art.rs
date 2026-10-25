//! Sprites from the Zombie Apocalypse Tileset, packed at build time from `art.txt`.

use std::sync::RwLock;

/// Sprites the editor adds while it runs, numbered after the packed ones: rects in the scratch area,
/// and a copy of the texture with the editor's pixels drawn in.
struct Live {
    rects: Vec<[u16; 4]>,
    pixels: Vec<u8>,
}

static LIVE: RwLock<Live> = RwLock::new(Live { rects: Vec::new(), pixels: Vec::new() });

/// Replaces the live sprites with `rects` into `pixels`, a full texture the size of the packed one, and returns them.
pub fn set_live(rects: Vec<[u16; 4]>, pixels: Vec<u8>) -> Vec<Art> {
    assert_eq!(pixels.len(), PIXELS.len(), "a live texture matches the packed one");
    let arts = (0..rects.len()).map(|i| Art((RECTS.len() + i) as u16)).collect();
    *LIVE.write().unwrap() = Live { rects, pixels };
    arts
}

/// Runs `f` on the texture a sprite's pixels live in.
pub(crate) fn with_pixels<T>(art: Art, f: impl FnOnce(&[u8]) -> T) -> T {
    if art.is_live() { f(&LIVE.read().unwrap().pixels) } else { f(PIXELS) }
}

/// One sprite in the packed texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Art(pub u16);

impl Art {
    /// Pixel rect in the texture: x, y, w, h.
    pub fn rect(self) -> [u16; 4] {
        match RECTS.get(self.0 as usize) {
            Some(&r) => r,
            None => LIVE.read().unwrap().rects[self.0 as usize - RECTS.len()],
        }
    }

    /// Whether the editor added this sprite while running.
    pub fn is_live(self) -> bool {
        self.0 as usize >= RECTS.len()
    }

    pub fn w(self) -> f32 {
        self.rect()[2] as f32
    }

    pub fn h(self) -> f32 {
        self.rect()[3] as f32
    }
}

include!(concat!(env!("OUT_DIR"), "/art_gen.rs"));
