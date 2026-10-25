use crate::art::Art;
use crate::scene3d::CardScene;
use crate::{SCREEN_W, font};

pub type Color = [f32; 4];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Rect { x: f32, y: f32, w: f32, h: f32, color: Color },
    Circle { x: f32, y: f32, r: f32, color: Color },
    /// Pixel art from the tileset, top-left at (x, y), each texel `scale` pixels across, rotated about its center.
    /// Only the top `show` fraction of its rows is drawn.
    Art { art: Art, x: f32, y: f32, scale: f32, flip: bool, angle: f32, show: f32, color: Color },
    /// TrueType text; `y` is the top of the line and `size` its height in pixels.
    Text { text: String, x: f32, y: f32, size: f32, color: Color, align: Align },
}

#[derive(Clone, Debug, Default)]
pub struct Canvas {
    pub clear: Color,
    /// Drawn first, under the 2D shapes.
    pub cards: Option<CardScene>,
    pub shapes: Vec<Shape>,
}

impl Canvas {
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        self.shapes.push(Shape::Rect { x, y, w, h, color });
    }

    pub fn circle(&mut self, x: f32, y: f32, r: f32, color: Color) {
        self.shapes.push(Shape::Circle { x, y, r, color });
    }

    pub fn art(&mut self, art: Art, x: f32, y: f32, scale: f32, flip: bool, color: Color) {
        self.shapes.push(Shape::Art { art, x, y, scale, flip, angle: 0.0, show: 1.0, color });
    }

    /// Only the top `show` fraction of the art's rows, top-left at (x, y).
    #[allow(clippy::too_many_arguments)]
    pub fn art_top(&mut self, art: Art, x: f32, y: f32, scale: f32, flip: bool, show: f32, color: Color) {
        self.shapes.push(Shape::Art { art, x, y, scale, flip, angle: 0.0, show: show.clamp(0.0, 1.0), color });
    }

    /// Pixel art centered on (cx, cy) and rotated by `angle` radians.
    pub fn art_rotated(&mut self, art: Art, cx: f32, cy: f32, scale: f32, angle: f32, color: Color) {
        let (x, y) = (cx - art.w() * scale / 2.0, cy - art.h() * scale / 2.0);
        self.shapes.push(Shape::Art { art, x, y, scale, flip: false, angle, show: 1.0, color });
    }

    /// TrueType text. `text` and `text_centered` draw the 3x5 bitmap font instead.
    pub fn label(&mut self, text: impl Into<String>, x: f32, y: f32, size: f32, color: Color, align: Align) {
        self.shapes.push(Shape::Text { text: text.into(), x, y, size, color, align });
    }

    pub fn text(&mut self, x: f32, y: f32, scale: f32, color: Color, s: &str) {
        let mut cx = x;
        for ch in s.chars() {
            let glyph = font::glyph(ch);
            for row in 0..5 {
                for col in 0..3 {
                    if font::lit(glyph, row, col) {
                        let (px, py) = (cx + col as f32 * scale, y + row as f32 * scale);
                        self.rect(px, py, scale, scale, color);
                    }
                }
            }
            cx += font::ADVANCE * scale;
        }
    }

    pub fn text_centered(&mut self, y: f32, scale: f32, color: Color, s: &str) {
        let w = font::width(s) * scale;
        self.text((SCREEN_W - w) / 2.0, y, scale, color, s);
    }
}

#[derive(Clone, Debug, Default)]
pub struct Frame {
    pub top: Canvas,
    pub bottom: Canvas,
}

