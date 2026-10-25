//! Sprites from the Zombie Apocalypse Tileset, packed at build time from `art.txt`.

/// One sprite in the packed texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Art(pub u16);

impl Art {
    /// Pixel rect in the texture: x, y, w, h.
    pub fn rect(self) -> [u16; 4] {
        RECTS[self.0 as usize]
    }

    pub fn w(self) -> f32 {
        self.rect()[2] as f32
    }

    pub fn h(self) -> f32 {
        self.rect()[3] as f32
    }
}

include!(concat!(env!("OUT_DIR"), "/art_gen.rs"));
