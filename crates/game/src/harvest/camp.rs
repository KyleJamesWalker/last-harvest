//! A farmer who stops moving is a camper: stay within `[camp] radius` of one spot for `after` seconds and the dead
//! climb out of the ground all around, a bigger wave every `every` seconds until they move on.

use std::f32::consts::TAU;

use super::map::TILE;
use super::player::PlayerId;
use super::{Harvest, Kind, Phase, Tell, len, roster, sub};
use crate::tuning::Tuning;

/// Radians between one camp zombie and the next, so any number of them spread round every side.
const GOLDEN: f32 = 2.399_963;

impl Harvest {
    /// Each living player keeps the spot they've stayed near; once they've stayed `[camp] after` seconds, waves
    /// rise around them.
    pub(super) fn camp(&mut self, dt: f32) {
        self.camp_by(crate::tuning::tuning(), dt);
    }

    fn camp_by(&mut self, t: &Tuning, dt: f32) {
        if t.camp_after <= 0.0 || self.range.is_some() || !matches!(self.phase, Phase::Playing) {
            return;
        }
        let every = t.camp_every.max(0.25);
        for p in 0..self.players.len() {
            let pl = &mut self.players[p];
            if !pl.alive() || pl.lead.is_some() || pl.left || len(sub(pl.pos, pl.camp_at)) > t.camp_radius * TILE {
                (pl.camp_at, pl.camp, pl.camp_waves) = (pl.pos, 0.0, 0);
                continue;
            }
            pl.camp += dt;
            let due = if pl.camp >= t.camp_after { ((pl.camp - t.camp_after) / every) as u32 + 1 } else { 0 };
            while self.players[p].camp_waves < due {
                let k = self.players[p].camp_waves;
                self.camp_wave(t, p, k);
                self.players[p].camp_waves += 1;
            }
        }
    }

    /// Wave `k` of the dead rising around player `p`, on open ground near them, up to `[camp] over` past the cap.
    fn camp_wave(&mut self, t: &Tuning, p: PlayerId, k: u32) {
        let n = (t.camp_wave + k as f32 * t.camp_grow).round().max(0.0) as usize;
        let most = (self.cap() as f32 * (1.0 + t.camp_over)) as usize;
        let minutes = self.t / 60.0;
        let grow = (1.0 + minutes * t.growth) * self.fury;
        let at = self.players[p].pos;
        let ring = t.camp_ring * TILE;
        let turn = self.rng.range(0.0, TAU);
        for i in 0..n {
            if self.zombies.len() + self.tells.len() + self.burrowed.len() >= most {
                break;
            }
            let r = ring + (i / 8) as f32 * TILE / 2.0;
            let a = turn + i as f32 * GOLDEN;
            let spot = (0..8).map(|j| a + j as f32 * GOLDEN).flat_map(|a| [r, r * 0.7, r * 1.4].map(|r| [at[0] + a.cos() * r, at[1] + a.sin() * r])).find(|&s| self.open(s));
            let Some(pos) = spot else { continue };
            let roll = self.rng.range(0.0, 1.0);
            let kind = Kind(roster().pick(minutes, roll, self.enemies_off) as u8);
            self.tells.push(Tell { kind, pos, hp: kind.stats().hp * grow, t: 0.0, leader: false, led: None });
        }
    }

    /// Whether a zombie fits at `at`: on the map and on open ground.
    fn open(&self, at: [f32; 2]) -> bool {
        let half = crate::tuning::tuning().zombie_half();
        (half[0]..self.map.width_px() - half[0]).contains(&at[0]) && (half[1]..self.map.height_px() - half[1]).contains(&at[1]) && !self.map.hits_cells(at[0], at[1], half)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harvest::map::MapSource;
    use crate::harvest::player::ME;

    fn open_farm() -> Harvest {
        let mut h = Harvest::calm_on(3, &MapSource::new("Open", [60, 60])).unwrap();
        h.birds.clear();
        h.me_mut().pos = [30.5 * TILE, 30.5 * TILE];
        h.me_mut().camp_at = h.me().pos;
        h
    }

    fn stay(h: &mut Harvest, secs: f32) {
        for _ in 0..(secs * 60.0).round() as u32 {
            h.camp(1.0 / 60.0);
        }
    }

    /// The quarters round `at` that camp zombies are rising in.
    fn sides(h: &Harvest, at: [f32; 2]) -> [bool; 4] {
        let mut out = [false; 4];
        for t in &h.tells {
            let d = sub(t.pos, at);
            assert!(len(d) < 5.0 * TILE, "rises right around them: {:?}", d);
            out[(d[1].atan2(d[0]).rem_euclid(TAU) / (TAU / 4.0)) as usize % 4] = true;
        }
        out
    }

    #[test]
    fn a_player_who_stays_put_has_the_dead_rise_around_them_from_every_side() {
        let t = crate::tuning::tuning();
        let mut h = open_farm();
        stay(&mut h, t.camp_after - 0.5);
        assert!(h.tells.is_empty(), "not yet");
        stay(&mut h, 1.0);
        let first = h.tells.len();
        assert_eq!(first, t.camp_wave as usize);
        assert_eq!(sides(&h, h.me().pos), [true; 4]);
        stay(&mut h, t.camp_every);
        assert_eq!(h.tells.len() - first, (t.camp_wave + t.camp_grow) as usize, "each wave is bigger");
    }

    #[test]
    fn shuffling_within_the_radius_still_counts_as_staying() {
        let t = crate::tuning::tuning();
        let mut h = open_farm();
        let start = h.me().pos;
        for s in 0..(t.camp_after as u32 + 1) {
            h.me_mut().pos = [start[0] + if s % 2 == 0 { 1.0 } else { -1.0 } * (t.camp_radius - 0.5) * TILE, start[1]];
            stay(&mut h, 1.0);
        }
        assert!(!h.tells.is_empty());
    }

    #[test]
    fn a_player_who_keeps_moving_never_has_them_rise_and_moving_on_starts_over() {
        let t = crate::tuning::tuning();
        let mut h = open_farm();
        for s in 0..120 {
            let a = s as f32 * 0.4;
            h.me_mut().pos = [30.5 * TILE + a.cos() * 8.0 * TILE, 30.5 * TILE + a.sin() * 8.0 * TILE];
            stay(&mut h, 1.0);
        }
        assert!(h.tells.is_empty(), "walking a wide loop is never camping");
        stay(&mut h, t.camp_after + 0.1);
        assert!(!h.tells.is_empty());
        h.tells.clear();
        h.me_mut().pos[0] += (t.camp_radius + 1.0) * TILE;
        stay(&mut h, t.camp_after - 0.5);
        assert!(h.tells.is_empty(), "moving on resets the count");
        assert_eq!(h.me().camp_waves, 0);
    }

    #[test]
    fn they_rise_only_on_open_ground_so_a_corner_gets_them_all_in_front() {
        let t = crate::tuning::tuning();
        let mut h = open_farm();
        let corner = [TILE, TILE];
        (h.me_mut().pos, h.me_mut().camp_at) = (corner, corner);
        stay(&mut h, t.camp_after + 0.1);
        assert_eq!(h.tells.len(), t.camp_wave as usize, "a full wave still finds room");
        assert!(h.tells.iter().all(|z| h.open(z.pos)));
    }

    #[test]
    fn nobody_down_leading_or_in_a_world_with_after_0_is_a_camper() {
        let t = crate::tuning::tuning();
        let mut h = open_farm();
        h.me_mut().down = Some(0.0);
        stay(&mut h, t.camp_after + 1.0);
        assert!(h.tells.is_empty(), "down");
        h.me_mut().down = None;
        h.phase = Phase::Over { t: 0.0, won: true };
        stay(&mut h, t.camp_after + 1.0);
        assert!(h.tells.is_empty(), "the night is over");
        h.phase = Phase::Playing;
        let off = crate::tuning::parse("[camp]\nafter 0\n", Some(t)).unwrap();
        for _ in 0..(t.camp_after as u32 + 10) * 60 {
            h.camp_by(&off, 1.0 / 60.0);
        }
        assert!(h.tells.is_empty(), "after 0 is off");
        h.me_mut().lead = Some(crate::harvest::leader::Lead::Picking { at: h.me().pos, left: 1.0 });
        stay(&mut h, t.camp_after + 1.0);
        assert!(h.tells.is_empty(), "leading the dead");
    }

    #[test]
    fn each_player_camps_on_their_own() {
        let t = crate::tuning::tuning();
        let mut h = open_farm().with_players(2);
        h.players[1].pos = [10.5 * TILE, 10.5 * TILE];
        h.players[1].camp_at = h.players[1].pos;
        for s in 0..(t.camp_after as u32 + 1) {
            h.players[ME].pos[0] = (30.5 + if s % 2 == 0 { 6.0 } else { -6.0 }) * TILE;
            stay(&mut h, 1.0);
        }
        assert!(!h.tells.is_empty() && h.tells.iter().all(|z| len(sub(z.pos, h.players[1].pos)) < 5.0 * TILE), "only round the one who stayed");
    }
}
