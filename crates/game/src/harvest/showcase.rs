//! One sprite placed on a quiet farm and drawn the way the game draws both screens, for the sprite editor.

use std::f32::consts::FRAC_PI_2;

use super::juice::{CORPSE_FALL, Corpse};
use super::map::TILE;
use super::{FLASH_TIME, HURT_FRAMES, Harvest, Look, SCALE, VIEW_H, VIEW_W, view3d};
use crate::art::{Art, Stand};
use crate::{Canvas, Color};

/// The sprite on show, centered at `pos` with its feet on the ground.
pub(super) struct Shown {
    pub frames: Vec<Art>,
    pub stand: Stand,
    pub fps: f32,
    pub pos: [f32; 2],
    /// The strip capping a figure's top, from the editor.
    pub top: Option<Art>,
    /// Whether the player stands beside it, or is left out so the view can come close.
    pub player: bool,
}

impl Shown {
    pub fn frame(&self, t: f32) -> Art {
        self.frames[(t * self.fps) as usize % self.frames.len()]
    }
}

/// A card's figure for Hurt and Kill: per facing, its standing, flash, hurt and maybe dead frames.
#[derive(Clone)]
pub struct Figure {
    pub stand: [Art; 3],
    pub flash: [Art; 3],
    pub hurt: [Art; 3],
    pub dead: Option<[Art; 3]>,
    pub gibs: usize,
}

/// Seconds a killed figure stays gone before it stands again.
const DEAD_TIME: f32 = 3.0;

pub struct Showcase {
    harvest: Harvest,
    /// A hit playing on the figure: what it shows, whether it dies, and seconds since.
    strike: Option<(Figure, bool, f32)>,
    /// The figure lying dead in its place, as a corpse lies in the game.
    lying: Option<Figure>,
}

impl Showcase {
    pub fn new(seed: u64) -> Self {
        let mut harvest = Harvest::calm(seed);
        harvest.birds.clear();
        let pos = [harvest.map.spawn[0], harvest.map.spawn[1] - 3.0 * TILE];
        harvest.shown = Some(Shown { frames: Vec::new(), stand: Stand::Figure, fps: 6.0, pos, top: None, player: true });
        Self { harvest, strike: None, lying: None }
    }

    /// Hits the figure as a bullet would: a flash, then its hurt frame.
    pub fn hurt(&mut self, figure: Figure) {
        self.strike = Some((figure, false, 0.0));
    }

    /// Kills the figure as the game does, then stands it up again.
    pub fn kill(&mut self, figure: Figure) {
        self.strike = Some((figure, true, 0.0));
    }

    /// Lays the figure down dead in its place, or stands it up again.
    pub fn lie(&mut self, figure: Option<Figure>) {
        self.lying = figure;
    }

    /// What a strike shows at `t`: a frame, nothing once dead, or None when it's over.
    fn struck(figure: &Figure, kill: bool, t: f32) -> Option<Vec<Art>> {
        let front = 0;
        match t {
            t if t < FLASH_TIME => Some(vec![figure.flash[front]]),
            t if kill && t < FLASH_TIME + DEAD_TIME => Some(Vec::new()),
            t if !kill && t < FLASH_TIME + HURT_FRAMES => Some(vec![figure.hurt[front]]),
            _ => None,
        }
    }

    /// Shows `frames`, animated at `fps`, standing as `stand`, with `top` capping a figure.
    pub fn show(&mut self, frames: Vec<Art>, stand: Stand, fps: f32, top: Option<Art>) {
        let lying = self.lying.is_some().then(Vec::new);
        let struck = self.strike.as_ref().and_then(|(f, kill, t)| Self::struck(f, *kill, *t)).or(lying);
        if let Some(s) = &mut self.harvest.shown {
            s.frames = struck.unwrap_or(frames);
            s.stand = stand;
            s.fps = fps;
            s.top = top;
        }
    }

    /// Puts the player beside the sprite, or leaves them out.
    pub fn show_player(&mut self, on: bool) {
        if let Some(s) = &mut self.harvest.shown {
            s.player = on;
        }
    }

    /// Moves the clock on and walks the player around the sprite to look at it from `yaw`, `distance` map pixels away.
    pub fn update(&mut self, dt: f32, yaw: f32, distance: f32) {
        let (forward, right) = ([yaw.cos(), yaw.sin()], [-yaw.sin(), yaw.cos()]);
        if let Some((figure, kill, t)) = &mut self.strike {
            let before = *t;
            *t += dt;
            let pos = self.harvest.shown.as_ref().map(|s| s.pos);
            if let Some(pos) = pos.filter(|_| *kill && before < FLASH_TIME && *t >= FLASH_TIME) {
                let look = Look { walk: figure.stand.map(|a| vec![a]), flash: figure.flash, hurt: figure.hurt, dead: figure.dead };
                let gibs = figure.gibs;
                self.harvest.gore(pos, forward, 0.0, [-forward[0], -forward[1]], &look, gibs);
            }
            if Self::struck(figure, *kill, *t).is_none() {
                self.strike = None;
            }
        }
        let h = &mut self.harvest;
        h.t += dt;
        for f in &mut h.fx {
            f.t += dt;
        }
        h.fx.retain(|f| !f.done());
        h.juice.tick(dt);
        let Some(s) = &h.shown else { return };
        // The lying corpse is laid fresh each frame, so it never sinks; a killed one always moves at first.
        h.juice.corpses.retain(|c| c.pos != s.pos || c.vel != [0.0; 2]);
        if let Some(f) = &self.lying {
            let dead = f.dead.map(|d| (d, [-forward[0], -forward[1]]));
            h.juice.corpses.push(Corpse { art: f.hurt[0], dead, flip: false, pos: s.pos, z: 0.0, vel: [0.0; 2], vz: 0.0, fall: forward, age: CORPSE_FALL, spin: 0.0 });
        }
        // A step to the side keeps a shown player from standing between the camera and the sprite.
        let aside = if s.player { 14.0 } else { 0.0 };
        h.pos = [s.pos[0] - forward[0] * distance - right[0] * aside, s.pos[1] - forward[1] * distance - right[1] * aside];
        h.view_yaw = yaw;
        h.aim = forward;
    }

    /// The 2.5D view, as on the top screen.
    pub fn top(&self) -> Canvas {
        let scene = view3d::scene(&self.harvest);
        Canvas { clear: [scene.fog[0], scene.fog[1], scene.fog[2], 1.0], cards: Some(scene), shapes: Vec::new() }
    }

    /// The map around the sprite, as on the bottom screen, magnified `zoom` times past the game's own scale.
    pub fn map(&self, zoom: f32) -> Canvas {
        let mut c = Canvas { clear: [0.0, 0.0, 0.0, 1.0], ..Default::default() };
        let h = &self.harvest;
        let Some(s) = &h.shown else { return c };
        let scale = SCALE * zoom;
        let cam = [s.pos[0] - VIEW_W / zoom / 2.0, s.pos[1] - 8.0 - VIEW_H / zoom / 2.0];
        let wash = view3d::sky(h).wash;
        let tint: Color = [0, 1, 2, 3].map(|i| if i == 3 { 1.0 } else { 1.0 - wash[3] + wash[3] * wash[i] });
        h.world(&mut c, cam, scale, tint);
        c
    }

    /// Default viewing angle: looking north, as a new run starts.
    pub fn yaw() -> f32 {
        -FRAC_PI_2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art;

    fn figure(dead: bool) -> Figure {
        let a = [art::SKINNY_HURT[0]; 3];
        Figure { stand: [art::SKINNY_FRONT[0]; 3], flash: a, hurt: a, dead: dead.then_some(a), gibs: 9 }
    }

    #[test]
    fn kill_flashes_leaves_a_corpse_and_stands_again() {
        let mut s = Showcase::new(7);
        let frames = vec![art::SKINNY_FRONT[0]];
        s.show(frames.clone(), Stand::Figure, 6.0, None);
        s.kill(figure(true));
        s.update(0.01, Showcase::yaw(), 48.0);
        s.show(frames.clone(), Stand::Figure, 6.0, None);
        assert_eq!(s.harvest.shown.as_ref().unwrap().frames, vec![art::SKINNY_HURT[0]], "it flashes first");
        s.update(FLASH_TIME, Showcase::yaw(), 48.0);
        s.show(frames.clone(), Stand::Figure, 6.0, None);
        assert!(s.harvest.shown.as_ref().unwrap().frames.is_empty(), "then it's gone");
        assert_eq!(s.harvest.juice.corpses.len(), 1);
        assert!(s.harvest.juice.corpses[0].dead.is_some(), "its own dead frames");
        assert!(!s.harvest.juice.particles.is_empty(), "with gibs");
        s.update(DEAD_TIME, Showcase::yaw(), 48.0);
        s.show(frames.clone(), Stand::Figure, 6.0, None);
        assert_eq!(s.harvest.shown.as_ref().unwrap().frames, frames, "and stands again");
    }

    #[test]
    fn a_lying_figure_is_a_corpse_that_stays_until_it_stands() {
        let mut s = Showcase::new(7);
        let frames = vec![art::SKINNY_FRONT[0]];
        s.lie(Some(figure(false)));
        for _ in 0..3 {
            s.update(5.0, Showcase::yaw(), 48.0);
        }
        s.show(frames.clone(), Stand::Figure, 6.0, None);
        assert!(s.harvest.shown.as_ref().unwrap().frames.is_empty(), "it isn't standing too");
        assert_eq!(s.harvest.juice.corpses.len(), 1);
        assert_eq!(s.harvest.juice.corpses[0].tipped(), 1.0, "tipped flat, as the game leaves one");
        assert_eq!(s.harvest.juice.corpses[0].sunk(), 0.0, "and never sinks");
        s.lie(None);
        s.update(0.01, Showcase::yaw(), 48.0);
        s.show(frames.clone(), Stand::Figure, 6.0, None);
        assert!(s.harvest.juice.corpses.is_empty());
        assert_eq!(s.harvest.shown.as_ref().unwrap().frames, frames);
    }

    #[test]
    fn hurt_shows_the_hurt_frame_then_the_walk() {
        let mut s = Showcase::new(7);
        let frames = vec![art::SKINNY_FRONT[0]];
        s.hurt(figure(false));
        s.update(FLASH_TIME + 0.01, Showcase::yaw(), 48.0);
        s.show(frames.clone(), Stand::Figure, 6.0, None);
        assert_eq!(s.harvest.shown.as_ref().unwrap().frames, vec![art::SKINNY_HURT[0]]);
        assert!(s.harvest.juice.corpses.is_empty());
        s.update(HURT_FRAMES, Showcase::yaw(), 48.0);
        s.show(frames.clone(), Stand::Figure, 6.0, None);
        assert_eq!(s.harvest.shown.as_ref().unwrap().frames, frames);
    }
}
