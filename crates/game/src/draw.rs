use crate::art::Art;
use crate::scene3d::CardScene;
use crate::{SCREEN_H, SCREEN_W, font};

pub type Color = [f32; 4];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Color,
    },
    Circle {
        x: f32,
        y: f32,
        r: f32,
        color: Color,
    },
    /// Pixel art from the tileset, top-left at (x, y), each texel `scale` pixels across, rotated about its center.
    /// Only the top `show` fraction of its rows is drawn.
    Art {
        art: Art,
        x: f32,
        y: f32,
        scale: f32,
        flip: bool,
        angle: f32,
        show: f32,
        color: Color,
    },
    /// TrueType text; `y` is the top of the line and `size` its height in pixels.
    Text {
        text: String,
        x: f32,
        y: f32,
        size: f32,
        color: Color,
        align: Align,
    },
    /// Shapes after this are cut to the rectangle (x, y, w, h), or no longer cut for None.
    Clip { rect: Option<[f32; 4]> },
}

#[derive(Clone, Debug, Default)]
pub struct Canvas {
    pub clear: Color,
    /// Drawn first, under the 2D shapes.
    pub cards: Option<CardScene>,
    pub shapes: Vec<Shape>,
    /// Its width and height in its own pixels, when not a screen's 1024x768.
    pub size: Option<[f32; 2]>,
    /// From each shape index on, the edge `refit` pins what follows to: 0 left or top, 1 right or bottom, below 0
    /// stretched across, above 1 left as drawn. Before the first, the middle.
    pub anchors: Vec<(usize, [f32; 2])>,
}

impl Canvas {
    /// Draws `src`'s shapes here, scaled by `k` with its top-left at `at`; its clear color and 3D cards are left out.
    pub fn place(&mut self, src: &Canvas, at: [f32; 2], k: f32) {
        let p = |x: f32, y: f32| (at[0] + x * k, at[1] + y * k);
        for shape in &src.shapes {
            self.shapes.push(match shape.clone() {
                Shape::Rect { x, y, w, h, color } => {
                    let (x, y) = p(x, y);
                    Shape::Rect {
                        x,
                        y,
                        w: w * k,
                        h: h * k,
                        color,
                    }
                }
                Shape::Circle { x, y, r, color } => {
                    let (x, y) = p(x, y);
                    Shape::Circle {
                        x,
                        y,
                        r: r * k,
                        color,
                    }
                }
                Shape::Art {
                    art,
                    x,
                    y,
                    scale,
                    flip,
                    angle,
                    show,
                    color,
                } => {
                    let (x, y) = p(x, y);
                    Shape::Art {
                        art,
                        x,
                        y,
                        scale: scale * k,
                        flip,
                        angle,
                        show,
                        color,
                    }
                }
                Shape::Text {
                    text,
                    x,
                    y,
                    size,
                    color,
                    align,
                } => {
                    let (x, y) = p(x, y);
                    Shape::Text {
                        text,
                        x,
                        y,
                        size: size * k,
                        color,
                        align,
                    }
                }
                Shape::Clip { rect } => Shape::Clip {
                    rect: rect.map(|[x, y, w, h]| {
                        let (x, y) = p(x, y);
                        [x, y, w * k, h * k]
                    }),
                },
            });
        }
    }

    /// Pins what's drawn after this to an edge or the middle of a view not shaped 4:3; see `anchors`.
    pub fn anchor(&mut self, at: [f32; 2]) {
        self.anchors.push((self.shapes.len(), at));
    }

    /// Leaves what's drawn after this as it is, as it was drawn to the size `refit` will lay the canvas out to.
    pub fn laid_out(&mut self) {
        self.anchor([2.0, 2.0]);
    }

    /// Lays a screen's 1024x768 drawing out in a canvas `size` across and down, at least that large: each shape keeps
    /// its size and moves with its anchor, and a rectangle as wide or tall as the screen spans the new one.
    pub fn refit(&mut self, size: [f32; 2]) {
        let base = [SCREEN_W, SCREEN_H];
        let extra = [size[0] - base[0], size[1] - base[1]];
        let mut at = [0.5, 0.5];
        let mut next = 0;
        let point = |v: f32, axis: usize, at: [f32; 2]| match at[axis] {
            a if a < 0.0 => v * size[axis] / base[axis],
            a if a > 1.0 => v,
            a => v + a * extra[axis],
        };
        let span = |v: f32, len: f32, axis: usize, at: [f32; 2]| {
            if at[axis] > 1.0 {
                (v, len)
            } else if v <= 0.5 && len >= base[axis] - 0.5 {
                (0.0, size[axis])
            } else if at[axis] < 0.0 {
                (point(v, axis, at), len * size[axis] / base[axis])
            } else {
                (point(v, axis, at), len)
            }
        };
        for (i, shape) in self.shapes.iter_mut().enumerate() {
            while let Some(&(_, a)) = self.anchors.get(next).filter(|(from, _)| *from <= i) {
                (at, next) = (a, next + 1);
            }
            match shape {
                Shape::Rect { x, y, w, h, .. } => {
                    ((*x, *w), (*y, *h)) = (span(*x, *w, 0, at), span(*y, *h, 1, at));
                }
                Shape::Clip {
                    rect: Some([x, y, w, h]),
                } => {
                    ((*x, *w), (*y, *h)) = (span(*x, *w, 0, at), span(*y, *h, 1, at));
                }
                Shape::Clip { rect: None } => {}
                Shape::Circle { x, y, .. } | Shape::Art { x, y, .. } | Shape::Text { x, y, .. } => {
                    (*x, *y) = (point(*x, 0, at), point(*y, 1, at));
                }
            }
        }
        self.anchors.clear();
        self.size = Some(size);
    }

    /// Cuts what's drawn after this to the rectangle (x, y, w, h), or stops cutting for None.
    pub fn clip(&mut self, rect: Option<[f32; 4]>) {
        self.shapes.push(Shape::Clip { rect });
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        self.shapes.push(Shape::Rect { x, y, w, h, color });
    }

    pub fn circle(&mut self, x: f32, y: f32, r: f32, color: Color) {
        self.shapes.push(Shape::Circle { x, y, r, color });
    }

    pub fn art(&mut self, art: Art, x: f32, y: f32, scale: f32, flip: bool, color: Color) {
        self.shapes.push(Shape::Art {
            art,
            x,
            y,
            scale,
            flip,
            angle: 0.0,
            show: 1.0,
            color,
        });
    }

    /// Only the top `show` fraction of the art's rows, top-left at (x, y).
    #[allow(clippy::too_many_arguments)]
    pub fn art_top(
        &mut self,
        art: Art,
        x: f32,
        y: f32,
        scale: f32,
        flip: bool,
        show: f32,
        color: Color,
    ) {
        self.shapes.push(Shape::Art {
            art,
            x,
            y,
            scale,
            flip,
            angle: 0.0,
            show: show.clamp(0.0, 1.0),
            color,
        });
    }

    /// Pixel art centered on (cx, cy) and rotated by `angle` radians.
    pub fn art_rotated(
        &mut self,
        art: Art,
        cx: f32,
        cy: f32,
        scale: f32,
        angle: f32,
        color: Color,
    ) {
        let (x, y) = (cx - art.w() * scale / 2.0, cy - art.h() * scale / 2.0);
        self.shapes.push(Shape::Art {
            art,
            x,
            y,
            scale,
            flip: false,
            angle,
            show: 1.0,
            color,
        });
    }

    /// TrueType text at the player's text size, as far as it still fits across the canvas, keeping the middle of a
    /// line of `size` at `y`. `text` and `text_centered` draw the 3x5 bitmap font instead.
    pub fn label(
        &mut self,
        text: impl Into<String>,
        x: f32,
        y: f32,
        size: f32,
        color: Color,
        align: Align,
    ) {
        let width = self.size.map_or(SCREEN_W, |s| s[0]);
        let room = match align {
            Align::Left => width - x,
            Align::Center => 2.0 * x.min(width - x),
            Align::Right => x,
        };
        self.label_in(text, x, y, size, room, color, align);
    }

    /// A `label` that grows at the player's text size only as far as it fits `room` pixels across; it never draws
    /// smaller than `size`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn label_in(
        &mut self,
        text: impl Into<String>,
        x: f32,
        y: f32,
        size: f32,
        room: f32,
        color: Color,
        align: Align,
    ) {
        let text = text.into();
        let fits = room / (text.chars().count().max(1) as f32 * crate::menu::EM);
        let shown = crate::comfort::text(size).min(fits).max(size);
        self.label_exact(text, x, y - (shown - size) / 2.0, shown, color, align);
    }

    /// TrueType text at exactly `size`, whatever the player's text size.
    pub(crate) fn label_exact(
        &mut self,
        text: impl Into<String>,
        x: f32,
        y: f32,
        size: f32,
        color: Color,
        align: Align,
    ) {
        self.shapes.push(Shape::Text {
            text: text.into(),
            x,
            y,
            size,
            color,
            align,
        });
    }

    /// A `pad::hint`, shrunk to the width the RG DS's names give it.
    pub(crate) fn hint(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        size: f32,
        color: Color,
        align: Align,
    ) {
        let (text, fits) = crate::pad::hint_sized(text, size);
        self.label(text, x, y + (size - fits) / 2.0, fits, color, align);
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
    /// On a phone, where the screens go in the window and the controls drawn over them.
    pub phone: Option<crate::touch::Phone>,
    /// Players 2 onward's views of a split run; `top` is player 1's.
    pub views: Vec<Canvas>,
}
