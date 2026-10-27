//! Debug scene that lays out every sprite in the art catalog with its name, then each card's parts, so a card can
//! be checked on the device. Up and Down scroll.

use crate::art::{ALL, Art};
use crate::harvest::{player_looks, roster};
use crate::{Align, Buttons, Frame, Input, SCREEN_H, SCREEN_W};

const PARTS: [&str; 3] = ["FRONT", "SIDE", "BACK"];

/// Every sprite to show: the catalog's but the editor's blank scratch space, then each card's walking parts, flash
/// and hurt, named after the card.
fn entries() -> Vec<(String, Vec<Art>)> {
    let catalog = ALL.iter().filter(|(n, _)| *n != "SCRATCH");
    let mut out: Vec<(String, Vec<Art>)> = catalog.map(|(n, f)| (n.to_string(), f.to_vec())).collect();
    let cards = roster().enemies.iter().filter(|e| e.from_card).map(|e| (format!("CARD {}", e.id), &e.look));
    let players = player_looks().iter().skip(1).map(|p| (format!("PLAYER CARD {}", p.id), &p.look));
    for (name, look) in cards.chain(players) {
        for (part, walk) in PARTS.iter().zip(&look.walk) {
            out.push((format!("{name} {part}"), walk.clone()));
        }
        out.push((format!("{name} FLASH"), look.flash.to_vec()));
        out.push((format!("{name} HURT"), look.hurt.to_vec()));
    }
    out
}

const SCALE: f32 = 3.0;
const GAP: f32 = 12.0;
const LABEL: f32 = 18.0;
const SCROLL: f32 = 600.0;

/// Where each entry goes, top-left in a layout scrolled to the top, and the height of it all.
fn layout(entries: &[(String, Vec<Art>)]) -> (Vec<[f32; 2]>, f32) {
    let (mut x, mut y, mut row_h) = (GAP, GAP, 0.0f32);
    let mut at = Vec::with_capacity(entries.len());
    for (name, frames) in entries {
        let (w, h) = size(name, frames);
        if x + w > SCREEN_W - GAP {
            (x, y, row_h) = (GAP, y + row_h + GAP, 0.0);
        }
        at.push([x, y]);
        x += w + GAP;
        row_h = row_h.max(h);
    }
    (at, y + row_h + GAP)
}

fn size(name: &str, frames: &[Art]) -> (f32, f32) {
    let w = frames.iter().map(|a| a.w() * SCALE + 4.0).sum::<f32>().max(name.len() as f32 * 9.0);
    let h = frames.iter().map(|a| a.h() * SCALE).fold(0.0, f32::max) + LABEL + 4.0;
    (w, h)
}

#[derive(Default)]
pub struct Gallery {
    scroll: f32,
}

impl Gallery {
    pub fn update(&mut self, input: &Input, dt: f32) {
        let dy = input.held.contains(Buttons::DOWN) as i32 - input.held.contains(Buttons::UP) as i32;
        self.scroll += dy as f32 * SCROLL * dt;
        if input.pressed.contains(Buttons::R1) {
            self.scroll += SCREEN_H * 2.0;
        }
        if input.pressed.contains(Buttons::L1) {
            self.scroll -= SCREEN_H * 2.0;
        }
        // Both screens show the gallery, so the last of it can rest at the bottom one's foot.
        let (_, height) = layout(&entries());
        self.scroll = self.scroll.clamp(0.0, (height - SCREEN_H * 2.0).max(0.0));
    }

    pub fn draw(&self, f: &mut Frame) {
        f.top.clear = [0.2, 0.45, 0.25, 1.0];
        f.bottom.clear = f.top.clear;
        let entries = entries();
        let (at, _) = layout(&entries);
        for ((name, frames), [x, y]) in entries.iter().zip(at) {
            let (_, h) = size(name, frames);
            let y = y - self.scroll;
            for (canvas, oy) in [(&mut f.top, 0.0), (&mut f.bottom, SCREEN_H)] {
                let cy = y - oy;
                if cy + h < 0.0 || cy > SCREEN_H {
                    continue;
                }
                canvas.label(name.as_str(), x, cy, LABEL, [1.0, 1.0, 0.6, 1.0], Align::Left);
                let mut fx = x;
                for art in frames {
                    canvas.art(*art, fx, cy + LABEL + 4.0, SCALE, false, [1.0; 4]);
                    fx += art.w() * SCALE + 4.0;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrolling_stops_with_the_last_row_at_the_foot_of_the_bottom_screen() {
        let mut g = Gallery::default();
        let (at, height) = layout(&entries());
        assert_eq!(at.len(), entries().len());
        for _ in 0..200 {
            g.update(&Input { pressed: Buttons::R1, ..Default::default() }, 0.016);
        }
        assert_eq!(g.scroll, (height - SCREEN_H * 2.0).max(0.0));
        g.update(&Input { pressed: Buttons::L1, ..Default::default() }, 0.016);
        assert!(g.scroll < (height - SCREEN_H * 2.0).max(0.0) || height <= SCREEN_H * 2.0);
        g.draw(&mut Frame::default());
    }
}
