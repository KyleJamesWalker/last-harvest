//! What the map knows: a zombie shows where the view ahead last saw it, and fades once out of sight.

use std::f32::consts::{PI, TAU};

use super::{Harvest, Zombie, clear_shot, len, sub};
use crate::art::Art;

/// A zombie as the map remembers it.
#[derive(Clone, Copy, Debug)]
pub(super) struct Mark {
    pub id: u32,
    pub pos: [f32; 2],
    pub art: Art,
    pub flip: bool,
    pub rise: f32,
    pub leader: bool,
    pub boss: bool,
    /// Seconds out of sight.
    pub age: f32,
}

impl Mark {
    pub fn of(z: &Zombie) -> Self {
        Mark { id: z.id, pos: z.pos, art: z.art(z.facing.0), flip: z.facing.1, rise: z.rise, leader: z.leader, boss: z.boss, age: 0.0 }
    }

    /// How strongly the map draws it, 1 while in sight down to 0 when forgotten.
    pub fn fade(&self) -> f32 {
        let memory = crate::tuning::tuning().map_memory;
        if memory > 0.0 { (1.0 - self.age / memory).clamp(0.0, 1.0) } else { 1.0 }
    }
}

impl Harvest {
    /// Whether the view ahead sees a spot on the ground: inside the aim cone, near enough, and not behind cover.
    pub(super) fn in_sight(&self, at: [f32; 2]) -> bool {
        let d = sub(at, self.pos);
        let r = len(d);
        if r > crate::tuning::tuning().map_sight {
            return false;
        }
        if r > 1.0 {
            let off = (d[1].atan2(d[0]) - self.view_yaw + PI).rem_euclid(TAU) - PI;
            if off.abs() > self.ahead_half() {
                return false;
            }
        }
        clear_shot(&self.map, self.pos, at)
    }

    /// Marks every zombie in sight where it stands, and ages the rest. A mark is forgotten once its memory runs
    /// out, or when its spot comes into sight with the zombie gone from it.
    pub(super) fn remember(&mut self, dt: f32) {
        let memory = crate::tuning::tuning().map_memory;
        let climbing = self.burrowed.iter().filter(|z| z.rise > 0.0);
        let seen: Vec<Mark> = self
            .zombies
            .iter()
            .chain(climbing)
            .filter(|z| self.in_sight(z.pos))
            .map(Mark::of)
            .collect();
        let old = std::mem::take(&mut self.marks);
        let kept: Vec<Mark> = old
            .into_iter()
            .filter(|m| !seen.iter().any(|s| s.id == m.id))
            .map(|m| Mark { age: m.age + dt, ..m })
            .filter(|m| m.age < memory && !self.in_sight(m.pos))
            .collect();
        self.marks = kept;
        self.marks.extend(seen);
    }
}

#[cfg(test)]
mod tests {
    use super::super::map::{self, TILE};
use super::super::PropLine;
    use super::super::{Harvest, Kind, Zombie};
    use std::f32::consts::PI;

    const DT: f32 = 1.0 / 60.0;

    /// A calm run on open dirt, facing east from the middle, with one zombie `ahead` pixels in front.
    fn facing_one(ahead: f32) -> Harvest {
        let src = map::MapSource::new("Open", [40, 14]);
        let mut h = Harvest::calm_on(1, &src).unwrap();
        h.pos = [20.5 * TILE, 7.5 * TILE];
        h.view_yaw = 0.0;
        h.marks.clear();
        let mut z = Zombie::new(Kind::common(), [h.pos[0] + ahead, h.pos[1]], 1000.0, 1.0, 0.0);
        z.id = 9;
        h.zombies = vec![z];
        h
    }

    #[test]
    fn a_zombie_ahead_is_marked_where_it_stands() {
        let mut h = facing_one(60.0);
        h.remember(DT);
        let m = h.marks.iter().find(|m| m.id == 9).expect("a zombie in sight is marked");
        assert_eq!((m.pos, m.age), (h.zombies[0].pos, 0.0));
    }

    #[test]
    fn a_zombie_behind_or_too_far_is_not_seen() {
        let mut h = facing_one(-60.0);
        h.remember(DT);
        assert!(h.marks.is_empty(), "behind the player");
        let mut h = facing_one(crate::tuning::tuning().map_sight + 20.0);
        h.remember(DT);
        assert!(h.marks.is_empty(), "past the map's sight");
    }

    #[test]
    fn turning_away_freezes_the_mark_and_it_fades_out() {
        let mut h = facing_one(60.0);
        h.remember(DT);
        let seen_at = h.zombies[0].pos;
        h.view_yaw = PI;
        h.zombies[0].pos[1] += 30.0;
        h.remember(DT);
        let m = h.marks[0];
        assert_eq!(m.pos, seen_at, "a mark stays where the zombie was seen");
        assert!(m.fade() < 1.0);
        h.remember(crate::tuning::tuning().map_memory);
        assert!(h.marks.is_empty(), "forgotten once its memory runs out");
    }

    #[test]
    fn looking_at_an_empty_spot_forgets_its_mark() {
        let mut h = facing_one(60.0);
        h.remember(DT);
        h.view_yaw = PI;
        h.zombies[0].pos[1] += 90.0;
        h.remember(DT);
        assert_eq!(h.marks.len(), 1);
        h.view_yaw = 0.0;
        h.remember(DT);
        assert!(h.marks.is_empty(), "the zombie moved off out of sight, and its spot is empty");
    }

    #[test]
    fn a_dead_zombie_out_of_sight_still_shows_until_it_fades() {
        let mut h = facing_one(60.0);
        h.remember(DT);
        h.view_yaw = PI;
        h.zombies.clear();
        h.remember(DT);
        assert_eq!(h.marks.len(), 1);
    }

    #[test]
    fn cover_hides_a_zombie_that_a_fence_does_not() {
        let fenced = |cover: bool| {
            let mut src = map::MapSource::new("Pen", [40, 14]);
            let tough = map::Tough { hp: None, cover: Some(cover) };
            src.props.push((0, PropLine::Fence { picket: false, rect: [17, 4, 7, 6], gate: None, tough }));
            let mut h = Harvest::calm_on(1, &src).unwrap();
            h.pos = [20.5 * TILE, 7.5 * TILE];
            h.view_yaw = 0.0;
            let mut z = Zombie::new(Kind::common(), [h.pos[0] + 96.0, h.pos[1]], 1000.0, 1.0, 0.0);
            z.id = 9;
            h.zombies = vec![z];
            h.marks.clear();
            h.remember(DT);
            h.marks.len()
        };
        assert_eq!(fenced(false), 1, "a plain fence lets the view through");
        assert_eq!(fenced(true), 0, "cover hides what's behind it");
    }
}
