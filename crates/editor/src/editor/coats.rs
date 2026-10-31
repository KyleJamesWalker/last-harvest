//! The Coats page: which of a sprite's colors are paint, and the coats each of its frames comes in.

use last_harvest_game::paint::{MAX_COATS, MAX_COLORS, Rgb};
use last_harvest_game::{Align, Canvas};

use super::{Action, CANVAS, DIM, Editor, HOT, PANEL, TEXT};
use crate::sprite::{self, Frame, Rgba};

const TOP: f32 = CANVAS.y + 96.0;
const ROW: f32 = 50.0;
const THUMB: f32 = 46.0;
const SHADE: f32 = 30.0;
const SHADES_X: f32 = CANVAS.x + 150.0;

fn rgb(c: Rgba) -> Rgb {
    [c[0], c[1], c[2]]
}

impl Editor {
    /// Marks the brush's color as paint, or not; paint colors keep light to dark.
    pub(super) fn toggle_paint(&mut self) {
        let c = rgb(self.color);
        let mut colors = self.sprite.paint.colors.clone();
        let gone = match colors.iter().position(|k| *k == c) {
            Some(i) => {
                colors.remove(i);
                true
            }
            None if colors.len() == MAX_COLORS => return self.say(format!("A sprite has at most {MAX_COLORS} paint colors")),
            None => {
                colors.push(c);
                false
            }
        };
        colors.sort_by_key(|k| std::cmp::Reverse(k.iter().map(|&v| v as u32).sum::<u32>()));
        self.sprite.paint.set_colors(colors);
        let n = self.sprite.paint.colors.len();
        self.coat = self.coat.filter(|_| n > 0);
        self.dirty = true;
        self.changed = true;
        self.say(format!("{} {} paint; {n} paint color{}", sprite::hex(self.color), if gone { "isn't" } else { "is" }, if n == 1 { "" } else { "s" }));
    }

    /// Adds a coat of the brush's color, its darker shades following the drawing's, and shows it.
    pub(super) fn add_coat(&mut self) {
        let p = &mut self.sprite.paint;
        if p.colors.is_empty() {
            return self.say("Mark the colors that are paint first: pick one, then press Paint beside it");
        }
        if p.coats.len() == MAX_COATS {
            return self.say(format!("A sprite has at most {MAX_COATS} coats"));
        }
        p.coats.push(vec![rgb(self.color)]);
        // Each darker shade becomes the palette's nearest, so the coat keeps to the palette.
        let k = p.coats.len() - 1;
        let shades = p.shades(k).into_iter().enumerate().map(|(j, c)| if j == 0 { c } else { rgb(sprite::nearest([c[0], c[1], c[2], 255], &self.palette)) }).collect();
        p.coats[k] = shades;
        self.coat = Some(k);
        self.dirty = true;
        self.changed = true;
    }

    /// Sets coat `k`'s shade of paint color `j` to the brush's color.
    pub(super) fn set_shade(&mut self, k: usize, j: usize) {
        let p = &mut self.sprite.paint;
        let Some(mut shades) = (k < p.coats.len()).then(|| p.shades(k)) else { return };
        shades[j] = rgb(self.color);
        p.coats[k] = shades;
        self.coat = Some(k);
        self.dirty = true;
        self.changed = true;
    }

    pub(super) fn drop_coat(&mut self, k: usize) {
        if k < self.sprite.paint.coats.len() {
            self.sprite.paint.coats.remove(k);
            self.coat = None;
            self.dirty = true;
            self.changed = true;
        }
    }

    /// A frame as the previews show it: in the coat picked on the Coats page, else as drawn.
    pub(super) fn in_coat(&self, f: &Frame) -> Frame {
        match self.coat.filter(|&k| k < self.sprite.paint.coats.len()) {
            Some(k) => f.in_coat(&self.sprite.paint, k),
            None => f.clone(),
        }
    }

    /// The frame being edited as drawn, then in each coat, for the page's rows.
    pub(super) fn coat_rows(&self) -> Vec<Frame> {
        let f = &self.sprite.frames[self.frame];
        std::iter::once(f.clone()).chain((0..self.sprite.paint.coats.len()).map(|k| f.in_coat(&self.sprite.paint, k))).collect()
    }

    /// Each row's picture, shades and Drop, then Add coat.
    pub(super) fn coat_buttons(&self, push: &mut impl FnMut(f32, f32, f32, f32, Action, &str, bool)) {
        let p = &self.sprite.paint;
        for row in 0..=p.coats.len() {
            let y = TOP + row as f32 * ROW;
            let k = row.checked_sub(1);
            push(CANVAS.x + 20.0, y, THUMB, THUMB, Action::Coat(k), "", self.coat == k);
            let Some(k) = k else { continue };
            for j in 0..p.colors.len() {
                push(SHADES_X + j as f32 * (SHADE + 4.0), y + 8.0, SHADE, SHADE, Action::Shade(k, j), "", false);
            }
            push(CANVAS.x + CANVAS.w - 90.0, y + 8.0, 70.0, 30.0, Action::DropCoat(k), "Drop", false);
        }
        if p.coats.len() < MAX_COATS {
            push(CANVAS.x + 20.0, TOP + (p.coats.len() + 1) as f32 * ROW, 140.0, 30.0, Action::AddCoat, "Add coat", false);
        }
    }

    /// A shade swatch's color.
    pub(super) fn shade_color(&self, k: usize, j: usize) -> Option<Rgba> {
        let p = &self.sprite.paint;
        (k < p.coats.len()).then(|| p.shades(k)).and_then(|s| s.get(j).map(|c| [c[0], c[1], c[2], 255]))
    }

    pub(super) fn draw_coats(&self, c: &mut Canvas) {
        c.rect(CANVAS.x, CANVAS.y, CANVAS.w, CANVAS.h, PANEL);
        let (x, p) = (CANVAS.x + 20.0, &self.sprite.paint);
        c.label("Each frame comes once in each coat; the previews show the one picked", x, CANVAS.y + 8.0, 16.0, DIM, Align::Left);
        c.label("Click a shade to give it the brush's color. Add coat starts one whose shades follow the drawing's, from the palette.", x, CANVAS.y + 30.0, 13.0, DIM, Align::Left);
        c.label("Paint", x, CANVAS.y + 60.0, 18.0, TEXT, Align::Left);
        if p.colors.is_empty() {
            c.label("Pick a color, then press Paint beside it", x + 70.0, CANVAS.y + 60.0, 16.0, DIM, Align::Left);
        }
        for (j, col) in p.colors.iter().enumerate() {
            c.rect(SHADES_X + j as f32 * (SHADE + 4.0), CANVAS.y + 54.0, SHADE, SHADE, [col[0] as f32 / 255.0, col[1] as f32 / 255.0, col[2] as f32 / 255.0, 1.0]);
        }
        for row in 0..=p.coats.len() {
            let y = TOP + row as f32 * ROW;
            let label = if row == 0 { "As drawn".to_string() } else { format!("Coat {row}") };
            c.label(label, CANVAS.x + 76.0, y + 14.0, 16.0, if self.coat == row.checked_sub(1) { HOT } else { TEXT }, Align::Left);
        }
    }
}
