//! Debug scene that lays out every sprite in the art catalog with its name. Up and Down scroll.

use crate::art::ALL;
use crate::{Align, Buttons, Frame, Input, SCREEN_H, SCREEN_W};

const SCALE: f32 = 3.0;
const GAP: f32 = 12.0;
const LABEL: f32 = 18.0;
const SCROLL: f32 = 600.0;

#[derive(Default)]
pub struct Gallery {
    scroll: f32,
}

impl Gallery {
    pub fn update(&mut self, input: &Input, dt: f32) {
        let dy = input.held.contains(Buttons::DOWN) as i32 - input.held.contains(Buttons::UP) as i32;
        self.scroll = (self.scroll + dy as f32 * SCROLL * dt).max(0.0);
        if input.pressed.contains(Buttons::R1) {
            self.scroll += SCREEN_H * 2.0;
        }
        if input.pressed.contains(Buttons::L1) {
            self.scroll = (self.scroll - SCREEN_H * 2.0).max(0.0);
        }
    }

    pub fn draw(&self, f: &mut Frame) {
        f.top.clear = [0.2, 0.45, 0.25, 1.0];
        f.bottom.clear = f.top.clear;
        let (mut x, mut y, mut row_h) = (GAP, GAP - self.scroll, 0.0f32);
        for (name, frames) in ALL {
            let w = frames.iter().map(|a| a.w() * SCALE + 4.0).sum::<f32>().max(name.len() as f32 * 9.0);
            let h = frames.iter().map(|a| a.h() * SCALE).fold(0.0, f32::max) + LABEL + 4.0;
            if x + w > SCREEN_W - GAP {
                (x, y, row_h) = (GAP, y + row_h + GAP, 0.0);
            }
            for (canvas, oy) in [(&mut f.top, 0.0), (&mut f.bottom, SCREEN_H)] {
                let cy = y - oy;
                if cy + h < 0.0 || cy > SCREEN_H {
                    continue;
                }
                canvas.label(*name, x, cy, LABEL, [1.0, 1.0, 0.6, 1.0], Align::Left);
                let mut fx = x;
                for art in *frames {
                    canvas.art(*art, fx, cy + LABEL + 4.0, SCALE, false, [1.0; 4]);
                    fx += art.w() * SCALE + 4.0;
                }
            }
            x += w + GAP;
            row_h = row_h.max(h);
        }
    }
}
