//! Single-screen play: the 3D view fills the top screen, and the map takes its place while L1 is held or L2
//! aims a grenade in slowed time.

use super::{Harvest, Phase, SCALE, VIEW_H, VIEW_W, len};
use crate::input::{Buttons, Input};
use crate::sfx::Sfx;
use crate::{Align, Canvas, Frame, SCREEN_H, SCREEN_W};

/// A grenade being aimed: L2 is down and one was ready.
#[derive(Clone, Copy, Debug)]
pub(super) struct Aiming {
    /// Where it will land, in map pixels.
    pub at: [f32; 2],
    /// Real seconds left before it throws itself.
    pub left: f32,
    /// The d-pad and stick as L2 went down; the player keeps walking that way.
    pub walk: [f32; 2],
    /// The pad has moved off `walk`; until then, still holding the run's direction leaves the cursor alone.
    pub steer: bool,
}

const PAD: Buttons = Buttons(Buttons::UP.0 | Buttons::DOWN.0 | Buttons::LEFT.0 | Buttons::RIGHT.0);

impl Harvest {
    pub fn single_screen(mut self, on: bool) -> Self {
        self.single = on;
        self
    }

    pub fn one_screen(&self) -> bool {
        self.single
    }

    pub fn set_touch_aim(&mut self, on: bool) {
        self.touch_aim = on;
        self.touch_map &= on;
    }

    pub fn set_one_screen(&mut self, on: bool) {
        self.single = on;
        if !on {
            self.aiming = None;
            self.glance = false;
        }
    }

    /// The input the run sees and the game time that passes: L2 and R2 never turn, a touch in play only throws
    /// from the held-up map with `touch_aim`, and while aiming the walk is held and the world slows.
    pub(super) fn single_input(&mut self, input: &Input, dt: f32, sounds: &mut Vec<Sfx>) -> (Input, f32) {
        let mut own = without(*input, Buttons::L2 | Buttons::R2);
        self.since_throw += dt;
        let playing = matches!(self.phase, Phase::Playing) && self.menu.is_none();
        if !playing {
            self.aiming = None;
            self.glance = false;
            self.touch_map = false;
            return (own, dt);
        }
        if self.touch_aim && input.pressed.contains(Buttons::L2) {
            self.touch_map = !self.touch_map;
        }
        own.touch = None;
        own.touches = Default::default();
        own.touch_started = false;
        if input.held.contains(Buttons::L1) || (self.touch_aim && self.touch_map) {
            self.aiming = None;
            self.glance = true;
            if let Some(at) = input.touch.filter(|_| self.glance && self.touch_aim && input.touch_started) {
                self.throw(at, sounds);
            }
            return (own, dt);
        }
        if input.pressed.contains(Buttons::L2) && !self.touch_aim {
            if self.grenades >= 1.0 {
                let t = crate::tuning::tuning();
                self.aiming = Some(Aiming { at: self.default_target(), left: t.aim_window, walk: input.dir(), steer: false });
            } else {
                sounds.push(Sfx::Miss);
            }
        }
        let Some(mut aim) = self.aiming else {
            self.glance = input.held.contains(Buttons::L2);
            return (own, dt);
        };
        self.glance = false;
        if input.pressed.contains(Buttons::B) {
            self.aiming = None;
            return (without(own, Buttons::B), dt);
        }
        let t = crate::tuning::tuning();
        let d = input.dir();
        aim.steer |= len([d[0] - aim.walk[0], d[1] - aim.walk[1]]) > 0.3;
        let d = if aim.steer { d } else { [0.0; 2] };
        aim.at = self.on_view([aim.at[0] + d[0] * t.aim_speed * dt, aim.at[1] + d[1] * t.aim_speed * dt]);
        aim.left -= dt;
        if !input.held.contains(Buttons::L2) || aim.left <= 0.0 {
            self.aiming = None;
            self.throw_at(aim.at, sounds);
            let off = [aim.at[0] - self.pos[0], aim.at[1] - self.pos[1]];
            self.last_throw = Some([off[1].atan2(off[0]) - self.view_yaw, len(off)]);
            self.since_throw = 0.0;
            return (own, dt);
        }
        self.aiming = Some(aim);
        let own = Input { stick: aim.walk, look: 0.0, ..without(own, PAD | Buttons::A | Buttons::Y | Buttons::B) };
        (own, dt * t.aim_slow)
    }

    /// Where the cursor starts: the last throw's angle off the view and distance, if it was within
    /// `aim_memory` seconds, else straight ahead `aim_ahead` away.
    fn default_target(&self) -> [f32; 2] {
        let t = crate::tuning::tuning();
        let [turn, dist] = self.last_throw.filter(|_| self.since_throw < t.aim_memory).unwrap_or([0.0, t.aim_ahead]);
        let a = self.view_yaw + turn;
        self.on_view([self.pos[0] + a.cos() * dist, self.pos[1] + a.sin() * dist])
    }

    /// A map point kept within the map view around the player.
    fn on_view(&self, p: [f32; 2]) -> [f32; 2] {
        let cam = self.camera();
        [p[0].clamp(cam[0], cam[0] + VIEW_W), p[1].clamp(cam[1], cam[1] + VIEW_H)]
    }

    fn map_up(&self) -> bool {
        self.aiming.is_some() || self.glance || self.overview || self.menu.is_some() || matches!(self.phase, Phase::LevelUp { .. })
    }

    /// The cursor, its blast ring and the time left, over the map.
    pub(super) fn aim_mark(&self, c: &mut Canvas, cam: [f32; 2]) {
        let Some(aim) = self.aiming else { return };
        let t = crate::tuning::tuning();
        let (x, y) = ((aim.at[0] - cam[0]) * SCALE, (aim.at[1] - cam[1]) * SCALE);
        let red = [1.0, 0.25, 0.2, 0.9];
        let r = t.grenade_radius * SCALE;
        for k in 0..24 {
            let a = k as f32 / 24.0 * std::f32::consts::TAU;
            c.circle(x + a.cos() * r, y + a.sin() * r, 4.0, red);
        }
        c.rect(x - 14.0, y - 2.0, 28.0, 4.0, red);
        c.rect(x - 2.0, y - 14.0, 4.0, 28.0, red);
        let k = (aim.left / t.aim_window).clamp(0.0, 1.0);
        c.rect(0.0, 0.0, SCREEN_W, 10.0, [0.0, 0.0, 0.0, 0.6]);
        c.rect(0.0, 0.0, SCREEN_W * k, 10.0, red);
    }

    /// Puts the map on the top screen while it's up, and leaves the bottom screen dark.
    pub(super) fn single_frame(&self, f: &mut Frame) {
        if self.map_up() {
            std::mem::swap(&mut f.top, &mut f.bottom);
        }
        f.bottom = Canvas::default();
        f.bottom.clear = [0.0, 0.0, 0.0, 1.0];
        let dim = [1.0, 1.0, 1.0, 0.45];
        f.bottom.label("One screen: Settings > Screens brings this one back", SCREEN_W / 2.0, SCREEN_H / 2.0 - 30.0, 32.0, dim, Align::Center);
        f.bottom.label("Hold L1: map.   L2: aim a grenade, release to throw, B cancels.   R2: whole map", SCREEN_W / 2.0, SCREEN_H / 2.0 + 20.0, 24.0, dim, Align::Center);
    }
}

fn without(mut input: Input, b: Buttons) -> Input {
    input.held = input.held & !b;
    input.pressed = input.pressed & !b;
    input
}

#[cfg(test)]
mod tests {
    use super::super::{Harvest, len};
    use crate::input::{Buttons, Input};
    use crate::sfx::Sfx;

    const DT: f32 = 1.0 / 60.0;

    fn hold(h: &mut Harvest, held: Buttons, secs: f32) -> Vec<Sfx> {
        let mut sounds = Vec::new();
        for _ in 0..(secs / DT).round() as usize {
            h.update(&Input { held, ..Default::default() }, DT, &mut sounds);
        }
        sounds
    }

    fn press(h: &mut Harvest, held: Buttons, pressed: Buttons) -> Vec<Sfx> {
        let mut sounds = Vec::new();
        h.update(&Input { held, pressed, ..Default::default() }, DT, &mut sounds);
        sounds
    }

    fn single(seed: u64) -> Harvest {
        let mut h = Harvest::calm(seed).single_screen(true);
        h.view_yaw = 0.0;
        hold(&mut h, Buttons::NONE, 0.05);
        h
    }

    #[test]
    fn aiming_slows_time_keeps_the_walk_and_throws_on_release() {
        let mut h = single(12);
        let (start, t) = (h.pos, h.t);
        press(&mut h, Buttons::L2 | Buttons::UP, Buttons::L2 | Buttons::UP);
        let at = h.aiming.unwrap().at;
        let ahead = crate::tuning::tuning().aim_ahead;
        assert!(len([at[0] - start[0] - ahead, at[1] - start[1]]) < 1.0, "starts straight ahead: {at:?}");
        hold(&mut h, Buttons::L2 | Buttons::UP, 0.5);
        assert_eq!(h.aiming.map(|a| a.at), Some(at), "holding the run's direction leaves the cursor");
        hold(&mut h, Buttons::L2, 0.5);
        let slow = crate::tuning::tuning().aim_slow;
        assert!((h.t - t - (1.0 + DT) * slow).abs() < 0.02, "time ran at {}", (h.t - t) / (1.0 + DT));
        assert!(h.pos[0] > start[0] + 1.0, "kept walking along the heading");
        assert!(h.thrown.is_empty());
        hold(&mut h, Buttons::NONE, DT);
        assert_eq!(h.thrown.len(), 1);
        assert!(h.aiming.is_none() && h.grenades < 1.0);
    }

    #[test]
    fn the_cursor_moves_with_the_pad_while_aiming() {
        let mut h = single(12);
        press(&mut h, Buttons::L2, Buttons::L2);
        let at = h.aiming.unwrap().at;
        hold(&mut h, Buttons::L2 | Buttons::LEFT, 0.25);
        assert!(h.aiming.unwrap().at[0] < at[0] - 10.0);
    }

    #[test]
    fn without_a_grenade_l2_only_shows_the_map_at_full_speed() {
        let mut h = single(12);
        h.grenades = 0.0;
        let t = h.t;
        let sounds = press(&mut h, Buttons::L2, Buttons::L2);
        assert!(sounds.iter().any(|s| matches!(s, Sfx::Miss)));
        assert!(h.aiming.is_none() && h.glance);
        hold(&mut h, Buttons::L2, 0.5);
        assert!(h.t - t > 0.5, "no slow motion");
        hold(&mut h, Buttons::NONE, DT);
        assert!(!h.glance && h.thrown.is_empty());
    }

    #[test]
    fn b_cancels_without_turning_around() {
        let mut h = single(12);
        press(&mut h, Buttons::L2, Buttons::L2);
        press(&mut h, Buttons::L2 | Buttons::B, Buttons::B);
        assert!(h.aiming.is_none() && h.spin == 0.0);
        hold(&mut h, Buttons::NONE, DT);
        assert!(h.thrown.is_empty() && h.grenades >= 1.0);
    }

    #[test]
    fn the_window_running_out_throws_at_the_cursor() {
        let mut h = single(12);
        press(&mut h, Buttons::L2, Buttons::L2);
        hold(&mut h, Buttons::L2, crate::tuning::tuning().aim_window + 0.1);
        assert_eq!(h.thrown.len(), 1);
        assert!(h.aiming.is_none());
    }

    #[test]
    fn a_quick_second_throw_starts_where_the_last_one_went_relative_to_the_view() {
        let mut h = single(12);
        press(&mut h, Buttons::L2, Buttons::L2);
        hold(&mut h, Buttons::L2 | Buttons::LEFT, 0.2);
        hold(&mut h, Buttons::L2 | Buttons::DOWN, 0.2);
        let first = h.aiming.unwrap().at;
        let off = [first[0] - h.pos[0], first[1] - h.pos[1]];
        hold(&mut h, Buttons::NONE, DT);
        h.view_yaw = std::f32::consts::FRAC_PI_2;
        hold(&mut h, Buttons::NONE, 1.0);
        h.grenades = 1.0;
        press(&mut h, Buttons::L2, Buttons::L2);
        let again = h.aiming.unwrap().at;
        let turned = [-off[1], off[0]];
        assert!(len([again[0] - h.pos[0] - turned[0], again[1] - h.pos[1] - turned[1]]) < 1.5, "{again:?} vs {turned:?}");
        hold(&mut h, Buttons::NONE, DT);
        hold(&mut h, Buttons::NONE, crate::tuning::tuning().aim_memory + 0.5);
        h.grenades = 1.0;
        press(&mut h, Buttons::L2, Buttons::L2);
        let late = h.aiming.unwrap().at;
        let ahead = crate::tuning::tuning().aim_ahead;
        assert!(len([late[0] - h.pos[0], late[1] - h.pos[1] - ahead]) < 1.0, "back to straight ahead: {late:?}");
    }

    #[test]
    fn with_touch_grenades_l2_flips_the_map_up_at_full_speed_and_a_tap_throws() {
        let mut h = single(12);
        h.set_touch_aim(true);
        let t = h.t;
        press(&mut h, Buttons::L2, Buttons::L2);
        assert!(h.aiming.is_none() && h.glance);
        hold(&mut h, Buttons::NONE, 0.5);
        assert!(h.glance, "it stays up with nothing held");
        assert!(h.t - t > 0.5, "no slow motion");
        let mut sounds = Vec::new();
        h.update(&Input { touch: Some([700.0, 300.0]), touch_started: true, ..Default::default() }, DT, &mut sounds);
        assert_eq!(h.thrown.len(), 1);
        press(&mut h, Buttons::L2, Buttons::L2);
        hold(&mut h, Buttons::NONE, DT);
        assert!(!h.glance && h.thrown.len() == 1, "a second press puts the 3D view back, and never throws");
    }

    #[test]
    fn endless_grenades_are_never_spent() {
        let mut h = single(12).endless_grenades(true);
        for _ in 0..3 {
            press(&mut h, Buttons::L2, Buttons::L2);
            hold(&mut h, Buttons::NONE, DT);
        }
        assert_eq!(h.thrown.len(), 3);
        assert!(h.grenades >= 1.0);
    }

    #[test]
    fn r2_holds_the_whole_map_paused_and_l1_the_live_one() {
        let mut h = single(12);
        let t = h.t;
        hold(&mut h, Buttons::R2, 0.5);
        assert!(h.overview && h.map_up());
        assert_eq!(h.t, t, "paused");
        hold(&mut h, Buttons::L1, 0.5);
        assert!(!h.overview && h.glance && h.map_up());
        assert!(h.t > t + 0.4, "the live map keeps the night going");
        hold(&mut h, Buttons::NONE, DT);
        assert!(!h.map_up());
    }

    #[test]
    fn shoulder_triggers_do_not_turn_on_one_screen() {
        let mut h = single(12);
        h.grenades = 0.0;
        hold(&mut h, Buttons::R2, 0.3);
        assert_eq!(h.view_yaw, 0.0);
    }
}
