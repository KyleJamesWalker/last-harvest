//! One RGBA texture holding a rasterized font.

use std::collections::HashMap;

use last_harvest_game::Align;

/// Glyphs are rasterized once at this height and scaled when drawn; mipmaps keep small text smooth.
const GLYPH_PX: f32 = 48.0;
const PAD: u32 = 4;
pub const SIZE: u32 = 1024;

#[derive(Clone, Copy)]
pub struct Glyph {
    pub uv: [f32; 4],
    /// Offset from the pen position on the baseline to the bitmap's top-left, in `GLYPH_PX` units.
    pub offset: [f32; 2],
    pub size: [f32; 2],
    pub advance: f32,
}

pub struct Atlas {
    pub rgba: Vec<u8>,
    glyphs: HashMap<char, Glyph>,
    ascent: f32,
}

/// A positioned glyph quad in screen pixels.
pub struct Placed {
    pub rect: [f32; 4],
    pub uv: [f32; 4],
}

impl Atlas {
    /// The text atlas from the pack's font, else the game's own when a mod's won't read or fit.
    pub fn build() -> Result<Self, String> {
        Self::from_font(last_harvest_game::data::font()).or_else(|e| {
            eprintln!("font: {e}; using the game's own");
            Self::from_font(last_harvest_game::data::built_in_font())
        })
    }

    fn from_font(bytes: &[u8]) -> Result<Self, String> {
        let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
        let font = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default())?;
        let ascent = font.horizontal_line_metrics(GLYPH_PX).map_or(GLYPH_PX * 0.8, |m| m.ascent);
        let mut glyphs = HashMap::new();
        let (mut x, mut y, mut row_h) = (PAD, PAD, 0);
        for ch in (32u8..127).map(char::from) {
            let m = font.metrics(ch, GLYPH_PX);
            if m.width as u32 + 2 * PAD > SIZE || m.height as u32 + 2 * PAD > SIZE {
                return Err(format!("its {ch:?} is too big for the {SIZE}px text texture"));
            }
            let (m, bitmap) = font.rasterize(ch, GLYPH_PX);
            let (gw, gh) = (m.width as u32, m.height as u32);
            if x + gw + PAD > SIZE {
                (x, y, row_h) = (PAD, y + row_h + PAD, 0);
            }
            if x + gw + PAD > SIZE || y + gh + PAD > SIZE {
                return Err(format!("its glyphs don't fit the {SIZE}px text texture"));
            }
            for row in 0..gh {
                for col in 0..gw {
                    let a = bitmap[(row * gw + col) as usize];
                    let i = (((y + row) * SIZE + x + col) * 4) as usize;
                    rgba[i..i + 4].copy_from_slice(&[255, 255, 255, a]);
                }
            }
            let s = SIZE as f32;
            glyphs.insert(ch, Glyph {
                uv: [x as f32 / s, y as f32 / s, (x + gw) as f32 / s, (y + gh) as f32 / s],
                offset: [m.xmin as f32, -(m.ymin as f32 + gh as f32)],
                size: [gw as f32, gh as f32],
                advance: m.advance_width,
            });
            x += gw + PAD;
            row_h = row_h.max(gh);
        }
        Ok(Self { rgba, glyphs, ascent })
    }

    /// Lays out one line of text; unknown characters advance like a space.
    pub fn layout(&self, text: &str, x: f32, y: f32, size: f32, align: Align) -> Vec<Placed> {
        let scale = size / GLYPH_PX;
        let space = self.glyphs.get(&' ').map_or(GLYPH_PX * 0.3, |g| g.advance);
        let width: f32 = text.chars().map(|c| self.glyphs.get(&c).map_or(space, |g| g.advance)).sum::<f32>() * scale;
        let mut pen = match align {
            Align::Left => x,
            Align::Center => x - width / 2.0,
            Align::Right => x - width,
        };
        let baseline = y + self.ascent * scale;
        let mut out = Vec::new();
        for c in text.chars() {
            let Some(g) = self.glyphs.get(&c) else {
                pen += space * scale;
                continue;
            };
            if g.size[0] > 0.0 {
                let (gx, gy) = (pen + g.offset[0] * scale, baseline + g.offset[1] * scale);
                out.push(Placed { rect: [gx, gy, g.size[0] * scale, g.size[1] * scale], uv: g.uv });
            }
            pen += g.advance * scale;
        }
        out
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_font_that_wont_read_is_an_error() {
        assert!(Atlas::from_font(b"not a font").is_err());
    }

    #[test]
    fn builds_and_lays_out_text() {
        let atlas = Atlas::build().unwrap();
        assert!(atlas.glyphs.contains_key(&'A'));
        let left = atlas.layout("Hi", 0.0, 0.0, 24.0, Align::Left);
        let centered = atlas.layout("Hi", 100.0, 0.0, 24.0, Align::Center);
        assert_eq!(left.len(), 2);
        assert!(centered[0].rect[0] < 100.0 && centered[1].rect[0] > 100.0 - 24.0);
    }
}
