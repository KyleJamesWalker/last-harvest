//! R1's quick throw: a marker on the ground straight ahead that slides out while R1 is held, and the grenade lands on
//! it when R1 is let go. Time runs on and the player keeps moving; it never picks a target.

use super::{Harvest, Phase};
use crate::input::{Buttons, Input};
use crate::sfx::Sfx;

/// How far ahead the marker is after R1 has been held `held` seconds: `quick_near`, out to `quick_far` over
/// `quick_grow` seconds.
pub(super) fn reach(held: f32) -> f32 {
    let t = crate::tuning::tuning();
    let k = (held / t.quick_grow).clamp(0.0, 1.0);
    t.quick_near + (t.quick_far.max(t.quick_near) - t.quick_near) * k
}

impl Harvest {
    /// Lines up a quick throw as R1 goes down with a grenade ready, slides the marker out while it's held, and
    /// throws on release; with none ready, R1 only clicks.
    pub(super) fn quick_throw(&mut self, input: &Input, dt: f32, sounds: &mut Vec<Sfx>) {
        if input.pressed.contains(Buttons::R1) {
            if self.grenades >= 1.0 {
                self.quick = Some(0.0);
                return;
            }
            sounds.push(Sfx::Miss);
        }
        let Some(held) = self.quick else { return };
        if input.held.contains(Buttons::R1) {
            self.quick = Some(held + dt);
            return;
        }
        let at = self.quick_mark();
        self.quick = None;
        if let Some(at) = at {
            self.throw_at(at, sounds);
        }
    }

    /// Drops a quick throw lined up when the run stops for a menu or the cards, without throwing it.
    pub(super) fn quick_off(&mut self) {
        if self.menu.is_some() || !matches!(self.phase, Phase::Playing) {
            self.quick = None;
        }
    }

    /// Where a quick throw lined up would land: straight ahead along the heading, kept on the map.
    pub(super) fn quick_mark(&self) -> Option<[f32; 2]> {
        let d = reach(self.quick?);
        let (w, h) = (self.map.width_px(), self.map.height_px());
        Some([(self.pos[0] + self.view_yaw.cos() * d).clamp(0.0, w), (self.pos[1] + self.view_yaw.sin() * d).clamp(0.0, h)])
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Harvest, len};
    use super::reach;
    use crate::input::{Buttons, Input};
    use crate::sfx::Sfx;

    const DT: f32 = 1.0 / 60.0;

    fn step(h: &mut Harvest, held: Buttons, pressed: Buttons) -> Vec<Sfx> {
        let mut sounds = Vec::new();
        h.update(&Input { held, pressed, ..Default::default() }, DT, &mut sounds);
        sounds
    }

    fn hold(h: &mut Harvest, held: Buttons, secs: f32) {
        for _ in 0..(secs / DT).round() as usize {
            step(h, held, Buttons::NONE);
        }
    }

    fn farm(seed: u64, single: bool) -> Harvest {
        let mut h = Harvest::calm(seed).single_screen(single);
        hold(&mut h, Buttons::NONE, 0.05);
        h
    }

    fn ahead(h: &Harvest) -> f32 {
        let at = h.quick_mark().expect("a marker");
        let off = [at[0] - h.pos[0], at[1] - h.pos[1]];
        let along = off[0] * h.view_yaw.cos() + off[1] * h.view_yaw.sin();
        assert!(len(off) - along < 0.5, "straight ahead: {off:?} along {}", h.view_yaw);
        along
    }

    #[test]
    fn r1_lines_up_a_marker_straight_ahead_and_throws_there_on_release() {
        for single in [false, true] {
            let mut h = farm(21, single);
            let before = h.grenades;
            step(&mut h, Buttons::R1, Buttons::R1);
            let t = crate::tuning::tuning();
            assert!((ahead(&h) - t.quick_near).abs() < 1.0, "{single}: starts at quick_near, {}", ahead(&h));
            assert!(h.thrown.is_empty(), "{single}: nothing thrown while held");
            let at = h.quick_mark().unwrap();
            let sounds = step(&mut h, Buttons::NONE, Buttons::NONE);
            assert!(sounds.iter().any(|s| matches!(s, Sfx::Throw)));
            assert_eq!(h.thrown.len(), 1, "{single}");
            assert!(len([h.thrown[0].to[0] - at[0], h.thrown[0].to[1] - at[1]]) < 1.0, "{single}: lands on the marker");
            assert!(h.quick_mark().is_none() && h.grenades < before, "{single}: one grenade spent, as any throw");
        }
    }

    #[test]
    fn holding_r1_slides_the_marker_out_until_quick_far() {
        let t = crate::tuning::tuning();
        let mut h = farm(22, false);
        step(&mut h, Buttons::R1, Buttons::R1);
        let near = ahead(&h);
        hold(&mut h, Buttons::R1, t.quick_grow / 2.0);
        let mid = ahead(&h);
        assert!(mid > near + 10.0 && mid < t.quick_far - 10.0, "{near} then {mid}");
        hold(&mut h, Buttons::R1, t.quick_grow * 2.0);
        assert!((ahead(&h) - t.quick_far).abs() < 1.0, "capped at quick_far: {}", ahead(&h));
        assert_eq!((reach(-1.0), reach(1e6)), (t.quick_near, t.quick_far));
    }

    #[test]
    fn time_runs_on_and_the_player_walks_and_turns_with_the_marker() {
        let mut h = farm(23, false);
        let (start, yaw, t) = (h.pos, h.view_yaw, h.t);
        step(&mut h, Buttons::R1, Buttons::R1);
        hold(&mut h, Buttons::R1 | Buttons::UP | Buttons::A, 0.5);
        assert!(h.t - t > 0.5, "no slow motion");
        assert!(len([h.pos[0] - start[0], h.pos[1] - start[1]]) > 5.0, "kept walking");
        assert!((h.view_yaw - yaw).abs() > 0.1, "kept turning");
        ahead(&h);
    }

    #[test]
    fn without_a_grenade_r1_clicks_and_throws_nothing() {
        let mut h = farm(24, false);
        h.grenades = 0.0;
        let sounds = step(&mut h, Buttons::R1, Buttons::R1);
        assert!(sounds.iter().any(|s| matches!(s, Sfx::Miss)));
        assert!(h.quick_mark().is_none());
        hold(&mut h, Buttons::R1, 0.3);
        hold(&mut h, Buttons::NONE, 0.1);
        assert!(h.thrown.is_empty());
    }

    #[test]
    fn pausing_drops_the_throw_lined_up() {
        let mut h = farm(25, false);
        step(&mut h, Buttons::R1, Buttons::R1);
        step(&mut h, Buttons::R1 | Buttons::START, Buttons::START);
        step(&mut h, Buttons::R1, Buttons::NONE);
        assert!(h.quick_mark().is_none());
        step(&mut h, Buttons::START, Buttons::START);
        hold(&mut h, Buttons::NONE, 0.1);
        assert!(h.thrown.is_empty());
    }

    #[test]
    fn endless_grenades_quick_throw_again_and_again() {
        let mut h = farm(26, true).endless_grenades(true);
        for _ in 0..3 {
            step(&mut h, Buttons::R1, Buttons::R1);
            step(&mut h, Buttons::NONE, Buttons::NONE);
        }
        assert_eq!(h.thrown.len(), 3);
    }

    #[test]
    fn the_marker_lies_on_the_ground_in_the_3d_view_and_glows() {
        let mut h = farm(27, false);
        let marks = |h: &Harvest| {
            let uv = crate::art::QUICK_MARK.rect().map(f32::from);
            super::super::view3d::scene(h).cards.iter().filter(|c| c.normal == glam::Vec3::Y && c.glow == 1.0 && (c.uv[0] * crate::art::ATLAS_W as f32 - uv[0]).abs() < 0.01).map(|c| (c.corners[0] + c.corners[2]) / 2.0).collect::<Vec<_>>()
        };
        assert!(marks(&h).is_empty());
        step(&mut h, Buttons::R1, Buttons::R1);
        let at = h.quick_mark().unwrap();
        let shown = marks(&h);
        assert_eq!(shown.len(), 1);
        let tile = super::super::map::TILE;
        assert!((shown[0].x - at[0] / tile).abs() < 0.01 && (shown[0].z - at[1] / tile).abs() < 0.01, "{shown:?} vs {at:?}");
    }
}
