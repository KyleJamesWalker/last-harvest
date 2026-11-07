//! What the map knows: a zombie shows where the view ahead last saw it, and fades once out of sight.

use std::f32::consts::{PI, TAU};

use super::{Harvest, Phase, Zombie, clear_shot, len, sub};
use crate::art::Art;
use crate::{Canvas, SCREEN_H, SCREEN_W};

/// Screen pixels across each square of the map's shade.
const SHADE_CELL: f32 = 16.0;
/// Steps of darkness the shade takes, so a row's squares of one step draw as one.
const SHADE_STEPS: f32 = 16.0;
/// Radians past the cone's edge, and the fraction of the sight range past its end, the shade takes to darken fully.
const SHADE_SOFT: (f32, f32) = (0.3, 0.2);

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

    /// How far out of the view ahead a spot on the ground is, from 0 inside the cone and in range to 1 well outside;
    /// cover doesn't count.
    pub(super) fn unseen(&self, at: [f32; 2]) -> f32 {
        let eye = [self.pos[0], self.pos[1] - 6.0];
        let d = sub(at, eye);
        let r = len(d);
        let sight = crate::tuning::tuning().map_sight;
        let far = smoothstep((r - sight) / (sight * SHADE_SOFT.1).max(1.0));
        let off = ((d[1].atan2(d[0]) - self.view_yaw + PI).rem_euclid(TAU) - PI).abs() - self.ahead_half();
        // Right around the player every way counts as ahead.
        let aside = smoothstep(off / SHADE_SOFT.0) * smoothstep((r - 4.0) / 12.0);
        far.max(aside)
    }

    /// Darkens the map outside the view ahead, in squares whose darkness steps smoothly toward the cone's edges; a
    /// run that's over shows the map bright.
    pub(super) fn shade_unseen(&self, c: &mut Canvas, cam: [f32; 2], scale: f32) {
        let dark = crate::tuning::tuning().map_shade.clamp(0.0, 1.0);
        if dark <= 0.0 || matches!(self.phase, Phase::Over { .. }) {
            return;
        }
        let (cols, rows) = ((SCREEN_W / SHADE_CELL).ceil() as usize, (SCREEN_H / SHADE_CELL).ceil() as usize);
        let step = |sx: f32, sy: f32| (self.unseen([cam[0] + sx / scale, cam[1] + sy / scale]) * SHADE_STEPS).round();
        for row in 0..rows {
            let y = row as f32 * SHADE_CELL;
            let mid = y + SHADE_CELL / 2.0;
            let mut run = (0, step(SHADE_CELL / 2.0, mid));
            for col in 1..=cols {
                let level = if col < cols { step(col as f32 * SHADE_CELL + SHADE_CELL / 2.0, mid) } else { -1.0 };
                if level == run.1 {
                    continue;
                }
                if run.1 > 0.0 {
                    let x = run.0 as f32 * SHADE_CELL;
                    c.rect(x, y, (col - run.0) as f32 * SHADE_CELL, SHADE_CELL, [0.0, 0.0, 0.0, dark * run.1 / SHADE_STEPS]);
                }
                run = (col, level);
            }
        }
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

fn smoothstep(k: f32) -> f32 {
    let k = k.clamp(0.0, 1.0);
    k * k * (3.0 - 2.0 * k)
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

    /// How dark the bottom map's shade is over a world spot, as drawn.
    fn shade_over(h: &Harvest, at: [f32; 2]) -> f32 {
        let mut f = crate::Frame::default();
        h.draw(&mut f, false);
        let (cam, k) = (h.camera(), super::super::map_scale());
        let [sx, sy] = [(at[0] - cam[0]) * k, (at[1] - cam[1]) * k];
        f.bottom.shapes.iter().filter_map(|s| match *s {
            crate::Shape::Rect { x, y, w, h, color: [0.0, 0.0, 0.0, a] } if (x..x + w).contains(&sx) && (y..y + h).contains(&sy) => Some(a),
            _ => None,
        }).fold(0.0, f32::max)
    }

    #[test]
    fn the_map_darkens_outside_the_view_ahead_and_softly_at_its_edge() {
        let h = facing_one(60.0);
        let dark = crate::tuning::tuning().map_shade;
        let ahead = [h.pos[0] + 60.0, h.pos[1] - 6.0];
        let behind = [h.pos[0] - 60.0, h.pos[1] - 6.0];
        assert_eq!(shade_over(&h, ahead), 0.0, "the cone is full brightness");
        assert!((shade_over(&h, behind) - dark).abs() < 1e-4, "behind is as dark as [map] shade says");
        let edge = h.ahead_half();
        let along = |a: f32| h.unseen([h.pos[0] + a.cos() * 100.0, h.pos[1] - 6.0 + a.sin() * 100.0]);
        let steps: Vec<f32> = (0..8).map(|i| along(edge + i as f32 * 0.05)).collect();
        assert_eq!(steps[0], 0.0);
        assert!(steps.windows(2).all(|w| w[1] >= w[0]) && steps.iter().any(|&s| s > 0.0 && s < 1.0), "fades in past the edge: {steps:?}");
        let sight = crate::tuning::tuning().map_sight;
        assert_eq!(h.unseen([h.pos[0] + sight * 2.0, h.pos[1] - 6.0]), 1.0, "and past the sight range");
    }

    #[test]
    fn the_shade_lifts_once_the_run_is_over() {
        let mut h = facing_one(60.0);
        h.phase = super::super::Phase::Over { t: 5.0, won: false };
        assert_eq!(shade_over(&h, [h.pos[0] - 60.0, h.pos[1] - 6.0]), 0.0);
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
