//! Debug scene for making icons: one figure, a zombie or a player's look, close up in the 3D view on the top screen
//! and flat at a whole-number scale in a 512-pixel square on the bottom. Y and A pick the figure, Left and Right turn
//! around it, Up and Down zoom, L1 with Up and Down raises or lowers the camera, L2 with the d-pad walks the figure across the farm (with A leaving a copy where it stands, B taking the last away and
//! X all of them), Start
//! builds a new random farm, B puts the player beside it, R2 puts it behind a barricade, a tap of L1
//! changes the square's background, X hides the guides, and R1 saves both screens as a screenshot to crop.

use crate::art::{self, Art, Stand};
use crate::harvest::{player_looks, roster};
use crate::{Align, Buttons, Canvas, Color, Frame, Input, SCREEN_H, SCREEN_W, Showcase};

const SEED: u64 = 7;
/// Map pixels a second the figure walks while L2 is held.
const WALK: f32 = 96.0;
const TURN: f32 = 1.5;
/// The camera stands this far back, in map pixels, and Up and Down zoom: further back, the figure looks less
/// stretched.
const DISTANCE: f32 = 80.0;
const MIN_ZOOM: f32 = 1.01;
const MAX_ZOOM: f32 = 8.0;
const ZOOM_RATE: f32 = 1.2;
/// How far the camera rises or sinks, in tiles, and how fast.
const LIFT: [f32; 2] = [-1.3, 4.0];
const LIFT_RATE: f32 = 1.5;
/// The flat icon's square on the bottom screen, and how much of it the figure may fill.
const SQUARE: f32 = 512.0;
const FILL: f32 = 0.8;
/// Android's adaptive icons show the middle 72 of 108 units whatever the launcher's mask.
const SAFE: f32 = 72.0 / 108.0;
/// Backgrounds from palette.txt: night, deep blue, earth, black.
const BACKS: [Color; 4] = [rgb(0x24, 0x1e, 0x1e), rgb(0x2e, 0x54, 0x70), rgb(0x44, 0x38, 0x30), rgb(0, 0, 0)];
const GUIDE: Color = [1.0, 1.0, 0.6, 0.6];
/// How much of the figure shows over the flat barricade.
const OVER: f32 = 0.6;

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

/// Every figure to pick from, by name, as its walk facing the camera: the zombies, then the players' looks.
fn subjects() -> Vec<(String, Vec<Art>)> {
    let enemies = roster().enemies.iter().map(|e| (format!("Zombie: {}", e.name), e.look.walk[0].clone()));
    let players = player_looks().iter().map(|p| (format!("Player: {}", p.name), p.look.walk[0].clone()));
    enemies.chain(players).filter(|(_, walk)| !walk.is_empty()).collect()
}

pub struct Icon {
    show: Showcase,
    subject: usize,
    yaw: f32,
    zoom: f32,
    lift: f32,
    /// L1 was used to raise or lower the camera while held, so letting go keeps the background.
    lifted: bool,
    held_l1: bool,
    player: bool,
    barricade: bool,
    back: usize,
    guides: bool,
    t: f32,
    seed: u64,
    /// The figure walked since the barricade was placed, so it goes up again where the figure stopped.
    walked: bool,
}

impl Default for Icon {
    fn default() -> Self {
        let mut show = Showcase::new(SEED);
        show.show_player(false);
        Self { show, subject: 0, yaw: Showcase::yaw(), zoom: 2.0, lift: 0.0, lifted: false, held_l1: false, player: false, barricade: false, back: 0, guides: true, t: 0.0, seed: SEED, walked: false }
    }
}

impl Icon {
    pub fn update(&mut self, input: &Input, dt: f32) {
        let n = subjects().len().max(1);
        let walking = input.held.contains(Buttons::L2);
        if walking {
            // With L2 held, A, B and X place more figures.
            if input.pressed.contains(Buttons::A) {
                self.show.drop_copy();
            }
            if input.pressed.any(Buttons::B | Buttons::X) {
                self.show.pick_up(input.pressed.contains(Buttons::X));
            }
        }
        if input.pressed.contains(Buttons::A) && !walking {
            self.subject = (self.subject + 1) % n;
        }
        if input.pressed.contains(Buttons::Y) {
            self.subject = (self.subject + n - 1) % n;
        }
        self.subject %= n;
        if input.pressed.contains(Buttons::B) && !walking {
            self.player = !self.player;
            self.show.show_player(self.player);
        }
        if input.pressed.contains(Buttons::R2) {
            self.barricade = !self.barricade;
            self.show.barricade(self.barricade);
        }
        if input.pressed.contains(Buttons::X) && !walking {
            self.guides = !self.guides;
        }
        let axis = |plus: Buttons, minus: Buttons| input.held.contains(plus) as i32 as f32 - input.held.contains(minus) as i32 as f32;
        if input.pressed.contains(Buttons::START) {
            self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            self.show.new_farm(self.seed, self.barricade);
        }
        let lifting = input.held.contains(Buttons::L1);
        // A tap of L1 changes the background; held, it moves the camera with Up and Down instead.
        if !lifting && std::mem::replace(&mut self.held_l1, false) && !std::mem::take(&mut self.lifted) {
            self.back = (self.back + 1) % BACKS.len();
        }
        if lifting {
            self.held_l1 = true;
            let up = axis(Buttons::UP, Buttons::DOWN);
            if up != 0.0 {
                self.lift = (self.lift + up * LIFT_RATE * dt).clamp(LIFT[0], LIFT[1]);
                self.lifted = true;
            }
        } else if walking {
            let (ahead, aside) = (axis(Buttons::UP, Buttons::DOWN), axis(Buttons::RIGHT, Buttons::LEFT));
            let (forward, right) = ([self.yaw.cos(), self.yaw.sin()], [-self.yaw.sin(), self.yaw.cos()]);
            let step = WALK * dt;
            if ahead != 0.0 || aside != 0.0 {
                self.show.walk([0, 1].map(|i| (forward[i] * ahead + right[i] * aside) * step));
                self.walked = true;
            }
        } else {
            if std::mem::take(&mut self.walked) && self.barricade {
                self.show.barricade(true);
            }
            self.yaw += axis(Buttons::RIGHT, Buttons::LEFT) * TURN * dt;
            self.zoom = (self.zoom * (axis(Buttons::UP, Buttons::DOWN) * ZOOM_RATE * dt).exp()).clamp(MIN_ZOOM, MAX_ZOOM);
        }
        self.show.zoom(self.zoom, self.lift);
        self.t += dt;
        if let Some((_, walk)) = subjects().into_iter().nth(self.subject) {
            self.show.show(walk, Stand::Figure, 6.0, None);
        }
        self.show.update(dt, self.yaw, DISTANCE);
    }

    pub fn draw(&self, f: &mut Frame) {
        f.top = self.show.top();
        let subjects = subjects();
        let Some((name, walk)) = subjects.get(self.subject) else { return };
        if self.guides {
            // The top screen's middle square, for an icon cropped from the 3D view.
            let x = (SCREEN_W - SCREEN_H) / 2.0;
            outline(&mut f.top, x, 0.0, SCREEN_H, GUIDE);
            let inset = SCREEN_H * (1.0 - SAFE) / 2.0;
            outline(&mut f.top, x + inset, inset, SCREEN_H * SAFE, GUIDE);
        }
        let b = &mut f.bottom;
        b.clear = [0.12, 0.12, 0.15, 1.0];
        let (x, y) = ((SCREEN_W - SQUARE) / 2.0, (SCREEN_H - SQUARE) / 2.0);
        b.rect(x, y, SQUARE, SQUARE, BACKS[self.back]);
        let art = walk[(self.t * 6.0) as usize % walk.len()];
        let bar = art::BARRIER;
        // The figure, then the barricade over its lower part, as a block scaled to fill the square.
        let (cw, ch) = if self.barricade { (art.w().max(bar.w() * 2.0), art.h() * OVER + bar.h()) } else { (art.w(), art.h()) };
        let scale = (SQUARE * FILL / cw.max(ch)).floor().max(1.0);
        let (left, top) = (x + (SQUARE - cw * scale) / 2.0, y + (SQUARE - ch * scale) / 2.0);
        b.art(art, left + (cw - art.w()) * scale / 2.0, top, scale, false, [1.0; 4]);
        if self.barricade {
            let by = top + art.h() * OVER * scale;
            let bx = left + (cw - bar.w() * 2.0) * scale / 2.0;
            b.art(bar, bx, by, scale, false, [1.0; 4]);
            b.art(bar, bx + bar.w() * scale, by, scale, false, [1.0; 4]);
        }
        if self.guides {
            let inset = SQUARE * (1.0 - SAFE) / 2.0;
            outline(b, x + inset, y + inset, SQUARE * SAFE, GUIDE);
            b.label(format!("{name}   x{scale}   zoom {:.1}   height {:+.1}   {} more", self.zoom, self.lift, self.show.crowd()), SCREEN_W / 2.0, 30.0, 24.0, [1.0, 1.0, 0.6, 1.0], Align::Center);
            let help = [
                "Keys: arrows turn and zoom   A / X figure   Q + arrows walk   Q + X / Z / S drop, undo, clear zombies",
                "1 + arrows camera height   tap 1 background   Enter new farm   2 barricade   Z player   S guides   W screenshot",
                "Pad: Y/A figure, L2 walk (+A/B/X), L1 height (tap: background), Start farm, R2 barricade, B player, X guides, R1 shot",
            ];
            for (i, line) in help.iter().enumerate() {
                b.label(*line, SCREEN_W / 2.0, SCREEN_H - 92.0 + i as f32 * 26.0, 17.0, [0.8, 0.8, 0.85, 1.0], Align::Center);
            }
        }
    }
}

/// A square outline two pixels thick.
fn outline(c: &mut Canvas, x: f32, y: f32, size: f32, color: Color) {
    c.rect(x, y, size, 2.0, color);
    c.rect(x, y + size - 2.0, size, 2.0, color);
    c.rect(x, y, 2.0, size, color);
    c.rect(x + size - 2.0, y, 2.0, size, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_cycles_figures_and_keeps_the_camera_in_range() {
        let mut icon = Icon::default();
        let n = subjects().len();
        assert!(n > 1);
        icon.update(&Input { pressed: Buttons::Y, ..Default::default() }, 0.016);
        assert_eq!(icon.subject, n - 1);
        icon.update(&Input { pressed: Buttons::A, ..Default::default() }, 0.016);
        assert_eq!(icon.subject, 0);
        for _ in 0..200 {
            icon.update(&Input { held: Buttons::UP, ..Default::default() }, 0.1);
        }
        assert_eq!(icon.zoom, MAX_ZOOM);
        let back = icon.back;
        icon.update(&Input { held: Buttons::L1 | Buttons::UP, ..Default::default() }, 0.5);
        icon.update(&Input::default(), 0.016);
        assert!(icon.lift > 0.0);
        assert_eq!(icon.back, back, "raising the camera keeps the background");
        icon.update(&Input { held: Buttons::L1, ..Default::default() }, 0.016);
        icon.update(&Input::default(), 0.016);
        assert_eq!(icon.back, back + 1, "a tap changes it");
        icon.update(&Input { pressed: Buttons::R2, ..Default::default() }, 0.016);
        assert!(icon.barricade);
        let before = icon.show.shown_at();
        for _ in 0..10 {
            icon.update(&Input { held: Buttons::L2 | Buttons::UP, ..Default::default() }, 0.1);
        }
        assert_ne!(icon.show.shown_at(), before, "L2 with the d-pad walks the figure");
        assert_eq!(icon.zoom, MAX_ZOOM, "and doesn't zoom");
        let drop = |pressed| Input { held: Buttons::L2, pressed, ..Default::default() };
        icon.update(&drop(Buttons::A), 0.016);
        icon.update(&drop(Buttons::A), 0.016);
        assert_eq!(icon.show.crowd(), 2);
        icon.update(&drop(Buttons::B), 0.016);
        assert_eq!(icon.show.crowd(), 1);
        icon.update(&drop(Buttons::X), 0.016);
        assert_eq!(icon.show.crowd(), 0);
        assert!(icon.guides, "X with L2 held clears, not hides the guides");
        icon.update(&Input { pressed: Buttons::START, ..Default::default() }, 0.016);
        assert_ne!(icon.seed, SEED);
        icon.draw(&mut Frame::default());
    }
}
