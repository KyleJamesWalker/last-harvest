//! Draw-only blending between the last two fixed steps, so a display faster than the step still moves every frame.

use super::*;

/// Farther than this in one step is a jump, such as a respawn or a recycled zombie, drawn where it lands.
const JUMP: f32 = 2.0 * TILE;
/// More turn than this in one step is a cut, not a turn.
const CUT: f32 = 1.0;

/// The world as the last step found it.
#[derive(Default)]
pub(super) struct Before {
    taken: bool,
    /// Each player's place and yaw, by index.
    players: Vec<([f32; 2], f32)>,
    lean: f32,
    t: f32,
    over: Option<f32>,
    /// Zombies' places by id, sorted, without any id two zombies share.
    zombies: Vec<(u32, [f32; 2])>,
}

/// What a blend moved, put back once the frame is drawn.
pub(crate) struct Undo {
    players: Vec<([f32; 2], f32)>,
    lean: f32,
    zombies: Vec<[f32; 2]>,
    movers: Vec<([f32; 2], f32)>,
}

/// Something that moves at a steady velocity or along its own clock, drawn back along it by part of a step.
struct Mover<'a> {
    pos: Option<&'a mut [f32; 2]>,
    vel: [f32; 2],
    clock: Option<&'a mut f32>,
    /// Seconds the last step moved it; 0 for one born in that step.
    dt: f32,
}

fn lerp(a: f32, b: f32, k: f32) -> f32 {
    a + (b - a) * k
}

/// From `from` toward `to`, unless it jumped.
fn between(from: [f32; 2], to: [f32; 2], k: f32) -> [f32; 2] {
    if len(sub(to, from)) > JUMP { to } else { [lerp(from[0], to[0], k), lerp(from[1], to[1], k)] }
}

/// From `from` toward `to` the short way round, in radians, unless it cut.
fn turn(from: f32, to: f32, k: f32) -> f32 {
    let d = (to - from + PI).rem_euclid(TAU) - PI;
    if d.abs() > CUT { to } else { (from + d * k).rem_euclid(TAU) }
}

/// The step's seconds for one aged `age` by it, or 0 if it was born in that step.
fn lived(age: f32, dt: f32) -> f32 {
    if age > dt * 1.5 { dt } else { 0.0 }
}

impl Harvest {
    /// Notes where everything is, at the start of a step.
    pub(super) fn take_before(&mut self) {
        let b = &mut self.before;
        (b.taken, b.lean, b.t) = (true, self.lean, self.t);
        b.players.clear();
        b.players.extend(self.players.iter().map(|p| (p.pos, p.view_yaw)));
        b.over = match self.phase {
            Phase::Over { t, .. } => Some(t),
            _ => None,
        };
        b.zombies.clear();
        b.zombies.extend(self.zombies.iter().chain(&self.burrowed).map(|z| (z.id, z.pos)));
        b.zombies.sort_unstable_by_key(|e| e.0);
        let ids: Vec<u32> = b.zombies.iter().map(|e| e.0).collect();
        let shared = |i: usize| (i > 0 && ids[i - 1] == ids[i]) || ids.get(i + 1) == Some(&ids[i]);
        let mut i = 0;
        b.zombies.retain(|_| {
            i += 1;
            !shared(i - 1)
        });
    }

    /// No step ran this frame: draw everything where it is.
    pub(crate) fn hold_still(&mut self) {
        self.before.taken = false;
    }

    /// Seconds the last step moved the world, and the effects, which keep moving once the run is over.
    fn stepped(&self) -> (f32, f32) {
        let world = (self.t - self.before.t).max(0.0);
        let juice = match (self.before.over, self.phase) {
            (Some(before), Phase::Over { t, .. }) => (t - before).max(0.0) * if t < TURN_SINK { 0.4 } else { 1.0 },
            _ => world,
        };
        (world, juice)
    }

    fn movers(&mut self, world: f32, juice: f32) -> Vec<Mover<'_>> {
        let mut out = Vec::new();
        for b in &mut self.bullets {
            let dt = lived(b.age, world);
            out.push(Mover { pos: Some(&mut b.pos), vel: b.vel, clock: Some(&mut b.age), dt });
        }
        for s in &mut self.spits {
            let dt = lived(s.age, world);
            out.push(Mover { pos: Some(&mut s.pos), vel: s.vel, clock: Some(&mut s.age), dt });
        }
        for g in &mut self.thrown {
            let dt = lived(g.t, world);
            out.push(Mover { pos: None, vel: [0.0; 2], clock: Some(&mut g.t), dt });
        }
        for p in &mut self.players {
            for s in &mut p.swings {
                let dt = lived(s.age, world);
                out.push(Mover { pos: None, vel: [0.0; 2], clock: Some(&mut s.age), dt });
            }
            for d in &mut p.dogs {
                out.push(Mover { pos: Some(&mut d.pos), vel: d.vel, clock: None, dt: world });
            }
            for d in &mut p.drones {
                out.push(Mover { pos: Some(&mut d.pos), vel: d.vel, clock: None, dt: world });
            }
        }
        for b in &mut self.birds {
            let dt = if b.flying || b.z > 0.0 { world } else { 0.0 };
            out.push(Mover { pos: Some(&mut b.pos), vel: b.vel, clock: None, dt });
        }
        for p in &mut self.juice.particles {
            let dt = lived(p.age, juice);
            out.push(Mover { pos: Some(&mut p.pos), vel: p.vel, clock: Some(&mut p.age), dt });
        }
        for c in &mut self.juice.corpses {
            let dt = lived(c.age, juice);
            out.push(Mover { pos: Some(&mut c.pos), vel: c.vel, clock: Some(&mut c.age), dt });
        }
        for p in &mut self.juice.popups {
            let dt = lived(p.age, juice);
            out.push(Mover { pos: None, vel: [0.0; 2], clock: Some(&mut p.age), dt });
        }
        for r in &mut self.juice.rings {
            let dt = lived(r.age, juice);
            out.push(Mover { pos: None, vel: [0.0; 2], clock: Some(&mut r.age), dt });
        }
        out
    }

    /// Moves what moved in the last step to `alpha` of the way from where it was to where it is, for drawing; `unblend`
    /// puts it all back exactly.
    pub(crate) fn blend(&mut self, alpha: f32) -> Option<Undo> {
        if alpha >= 1.0 || !self.before.taken {
            return None;
        }
        let alpha = alpha.max(0.0);
        let back = 1.0 - alpha;
        let (world, juice) = self.stepped();
        let mut undo = Undo { players: Vec::new(), lean: self.lean, zombies: Vec::new(), movers: Vec::new() };
        for (p, &(pos, yaw)) in self.players.iter_mut().zip(&self.before.players) {
            undo.players.push((p.pos, p.view_yaw));
            p.pos = between(pos, p.pos, alpha);
            p.view_yaw = turn(yaw, p.view_yaw, alpha);
        }
        self.lean = lerp(self.before.lean, self.lean, alpha);
        let was = &self.before.zombies;
        for z in self.zombies.iter_mut().chain(&mut self.burrowed) {
            undo.zombies.push(z.pos);
            if let Ok(i) = was.binary_search_by_key(&z.id, |e| e.0) {
                z.pos = between(was[i].1, z.pos, alpha);
            }
        }
        for m in self.movers(world, juice) {
            undo.movers.push((m.pos.as_deref().copied().unwrap_or_default(), m.clock.as_deref().copied().unwrap_or_default()));
            let s = m.dt * back;
            if let Some(p) = m.pos {
                *p = [p[0] - m.vel[0] * s, p[1] - m.vel[1] * s];
            }
            if let Some(c) = m.clock {
                *c -= s;
            }
        }
        Some(undo)
    }

    pub(crate) fn unblend(&mut self, undo: Undo) {
        for (p, (pos, yaw)) in self.players.iter_mut().zip(undo.players) {
            (p.pos, p.view_yaw) = (pos, yaw);
        }
        self.lean = undo.lean;
        for (z, p) in self.zombies.iter_mut().chain(&mut self.burrowed).zip(undo.zombies) {
            z.pos = p;
        }
        for (m, (p, c)) in self.movers(0.0, 0.0).into_iter().zip(undo.movers) {
            if let Some(pos) = m.pos {
                *pos = p;
            }
            if let Some(clock) = m.clock {
                *clock = c;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEP: f32 = 1.0 / 60.0;

    fn walking() -> Harvest {
        let mut h = Harvest::calm(3);
        h.birds.clear();
        h
    }

    fn walk(h: &mut Harvest) {
        let mut sounds = Vec::new();
        h.update(&Input { held: Buttons::UP, ..Default::default() }, STEP, &mut sounds);
    }

    fn fingerprint(h: &Harvest) -> String {
        let zombies: Vec<_> = h.zombies.iter().chain(&h.burrowed).map(|z| (z.id, z.pos, z.hp)).collect();
        let bullets: Vec<_> = h.bullets.iter().map(|b| (b.pos, b.age)).collect();
        let juice: Vec<_> = h.juice.particles.iter().map(|p| (p.pos, p.age)).chain(h.juice.corpses.iter().map(|c| (c.pos, c.age))).collect();
        let thrown: Vec<_> = h.thrown.iter().map(|g| g.t).collect();
        format!("{:?} {} {} {} {} {zombies:?} {bullets:?} {juice:?} {thrown:?}", h.me().pos, h.me().view_yaw, h.t, h.kills, h.xp)
    }

    #[test]
    fn drawing_between_steps_never_changes_the_run() {
        let (mut plain, mut drawn) = (Harvest::new(5), Harvest::new(5));
        let mut sounds = Vec::new();
        for i in 0..1500 {
            let held = if i % 300 < 180 { Buttons::UP | Buttons::A } else { Buttons::UP };
            let pressed = if i % 240 == 0 { Buttons::B } else { Buttons::NONE };
            let touch = (i % 400 == 0).then_some([SCREEN_W / 2.0, SCREEN_H / 3.0]);
            let input = Input { held, pressed, touch, touch_started: touch.is_some(), ..Default::default() };
            plain.update(&input, STEP, &mut sounds);
            drawn.update(&input, STEP, &mut sounds);
            for alpha in [0.0, 0.4, 0.9] {
                if let Some(undo) = drawn.blend(alpha) {
                    drawn.unblend(undo);
                }
            }
            assert_eq!(fingerprint(&plain), fingerprint(&drawn), "step {i}");
        }
        assert!(!plain.zombies.is_empty(), "the night had zombies to draw");
    }

    #[test]
    fn lerp_goes_from_one_end_to_the_other() {
        assert_eq!(lerp(2.0, 6.0, 0.0), 2.0);
        assert_eq!(lerp(2.0, 6.0, 0.25), 3.0);
        assert_eq!(lerp(2.0, 6.0, 1.0), 6.0);
        assert_eq!(between([0.0, 0.0], [4.0, -2.0], 0.5), [2.0, -1.0]);
    }

    #[test]
    fn a_turn_takes_the_short_way_round() {
        let k = turn(TAU - 0.1, 0.1, 0.5);
        assert!(k < 1e-5 || TAU - k < 1e-5, "{k}");
        let k = turn(0.1, TAU - 0.1, 0.25);
        assert!((k - (0.1 - 0.05)).abs() < 1e-5, "{k}");
        assert!((turn(1.0, 1.4, 0.5) - 1.2).abs() < 1e-6);
    }

    #[test]
    fn a_jump_or_a_cut_draws_where_it_landed() {
        assert_eq!(between([0.0, 0.0], [10.0 * TILE, 0.0], 0.5), [10.0 * TILE, 0.0]);
        assert_eq!(turn(0.0, PI, 0.5), PI);
    }

    #[test]
    fn a_step_is_drawn_part_way_and_put_back_exactly() {
        let mut h = walking();
        for _ in 0..30 {
            walk(&mut h);
        }
        let (now, yaw) = (h.me().pos, h.me().view_yaw);
        let was = h.before.players[0].0;
        assert_ne!(now, was, "the player walked");
        let undo = h.blend(0.5).expect("a step to blend");
        assert!((h.me().pos[0] - (was[0] + now[0]) / 2.0).abs() < 1e-3 && (h.me().pos[1] - (was[1] + now[1]) / 2.0).abs() < 1e-3, "{:?} between {was:?} and {now:?}", h.me().pos);
        h.unblend(undo);
        assert_eq!((h.me().pos, h.me().view_yaw), (now, yaw));
    }

    #[test]
    fn alpha_one_or_no_step_draws_everything_where_it_is() {
        let mut h = walking();
        assert!(h.blend(0.5).is_none(), "a run before its first step");
        walk(&mut h);
        walk(&mut h);
        assert!(h.blend(1.0).is_none());
        h.hold_still();
        assert!(h.blend(0.5).is_none(), "a frame the run didn't step");
    }

    #[test]
    fn a_paused_run_does_not_wobble() {
        let mut h = walking();
        for _ in 0..10 {
            walk(&mut h);
        }
        let mut sounds = Vec::new();
        h.update(&Input { pressed: Buttons::START, held: Buttons::START, ..Default::default() }, STEP, &mut sounds);
        h.update(&Input::default(), STEP, &mut sounds);
        let now = h.me().pos;
        for alpha in [0.0, 0.3, 0.9] {
            let undo = h.blend(alpha).expect("a step");
            assert_eq!(h.me().pos, now, "at {alpha}");
            h.unblend(undo);
        }
    }

    #[test]
    fn a_zombie_new_this_step_or_that_jumped_is_drawn_where_it_is() {
        let mut h = walking();
        walk(&mut h);
        let mut z = Zombie::new(Kind::common(), [h.me().pos[0] + 40.0, h.me().pos[1]], 10.0, 1.0, 0.0);
        z.id = 900;
        h.zombies = vec![z];
        h.burrowed.clear();
        let at = h.zombies[0].pos;
        let undo = h.blend(0.0).expect("a step");
        assert_eq!(h.zombies[0].pos, at, "born after the step began");
        h.unblend(undo);
        h.take_before();
        h.zombies[0].pos[0] += 4.0;
        let at = h.zombies[0].pos;
        let undo = h.blend(0.5).expect("a step");
        assert_eq!(h.zombies[0].pos[0], at[0] - 2.0, "walked: drawn half way");
        h.unblend(undo);
        h.zombies[0].pos[0] += 20.0 * TILE;
        let at = h.zombies[0].pos;
        let undo = h.blend(0.5).expect("a step");
        assert_eq!(h.zombies[0].pos, at, "recycled far away: drawn where it is");
        h.unblend(undo);
    }

    #[test]
    fn a_bullet_born_this_step_is_drawn_where_it_is_and_an_older_one_back_along_its_flight() {
        let mut h = walking();
        h.take_before();
        h.t += STEP;
        let shot = |age: f32| Bullet { pos: [100.0, 100.0], vel: [60.0, 0.0], damage: 1.0, pierce: 0, life: 1.0, force: 0.0, hit: Vec::new(), look: None, age, by: None, owner: ME, lift: [0.0; 2] };
        h.bullets = vec![shot(STEP), shot(STEP * 4.0)];
        let undo = h.blend(0.5).expect("a step");
        assert_eq!(h.bullets[0].pos, [100.0, 100.0]);
        assert!((h.bullets[1].pos[0] - 99.5).abs() < 1e-2, "{:?}", h.bullets[1].pos);
        h.unblend(undo);
        assert_eq!(h.bullets[1].pos, [100.0, 100.0]);
        assert_eq!(h.bullets[1].age, STEP * 4.0);
    }
}
