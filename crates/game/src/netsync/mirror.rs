//! The client's picture of the host's world: zombies and other players drawn a little in the past between
//! snapshots, local players predicted from their own input and eased toward where the host has them.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::f32::consts::{PI, TAU};

use super::snapshot::{P_DOWN, P_LEADING, P_PICKING, Part, PlayerState, RunState, Spawn, Spawned, TICK_HZ, Thing, ZombieState, unq_angle, unq_pos};
use crate::{Buttons, Input};

/// How far behind the newest snapshot others are drawn, in ticks: two snapshots' worth.
pub const INTERP_TICKS: f64 = 6.0;
/// The most a zombie runs on past its newest snapshot, in ticks.
const EXTRAPOLATE_TICKS: f64 = 12.0;
/// Pixels a second a drawn zombie closes on where it should be, on top of its own movement.
pub const CATCH_UP: f32 = 120.0;
/// The fastest a drawn zombie follows its target's own movement, in pixels a second.
pub const RUN: f32 = 120.0;
/// Further off than this, a drawn zombie jumps straight there.
pub const SNAP: f32 = 48.0;
/// A local player further than this from the host's place for it jumps there.
const OWN_SNAP: f32 = 64.0;
/// The share of a local player's error taken away per snapshot.
const RECONCILE: f32 = 0.3;
const SAMPLES: usize = 16;
const SPIN_TIME: f32 = 0.2;
const DT: f32 = 1.0 / TICK_HZ as f32;

struct Track {
    samples: VecDeque<(u32, [f32; 2])>,
    shown: [f32; 2],
    last_target: [f32; 2],
    state: ZombieState,
}

impl Track {
    fn new(tick: u32, pos: [f32; 2], state: ZombieState) -> Self {
        Self { samples: VecDeque::from([(tick, pos)]), shown: pos, last_target: pos, state }
    }

    fn add(&mut self, tick: u32, pos: [f32; 2]) {
        let at = self.samples.partition_point(|s| s.0 < tick);
        if self.samples.get(at).is_some_and(|s| s.0 == tick) {
            return;
        }
        self.samples.insert(at, (tick, pos));
        while self.samples.len() > SAMPLES {
            self.samples.pop_front();
        }
    }

    fn newest(&self) -> u32 {
        self.samples.back().map_or(0, |s| s.0)
    }

    fn target(&self, at: f64) -> [f32; 2] {
        let s = &self.samples;
        let (first, last) = (s[0], s[s.len() - 1]);
        if at <= first.0 as f64 {
            return first.1;
        }
        if at >= last.0 as f64 {
            if s.len() < 2 {
                return last.1;
            }
            let prev = s[s.len() - 2];
            let k = (at - last.0 as f64).min(EXTRAPOLATE_TICKS) as f32 / (last.0 - prev.0).max(1) as f32;
            return [last.1[0] + (last.1[0] - prev.1[0]) * k, last.1[1] + (last.1[1] - prev.1[1]) * k];
        }
        let i = s.partition_point(|x| (x.0 as f64) <= at);
        let (a, b) = (s[i - 1], s[i]);
        let k = ((at - a.0 as f64) / (b.0 - a.0) as f64) as f32;
        [a.1[0] + (b.1[0] - a.1[0]) * k, a.1[1] + (b.1[1] - a.1[1]) * k]
    }

    /// Moves the drawn place with the target, closing any gap at `CATCH_UP` and snapping past `SNAP`.
    fn follow(&mut self, target: [f32; 2], dt: f32) {
        let moved = dist(target, self.last_target);
        self.last_target = target;
        let gap = dist(target, self.shown);
        let step = CATCH_UP * dt + moved.min(RUN * dt);
        if gap <= step || gap > SNAP {
            self.shown = target;
        } else {
            let k = step / gap;
            self.shown = [self.shown[0] + (target[0] - self.shown[0]) * k, self.shown[1] + (target[1] - self.shown[1]) * k];
        }
    }
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn wrap(a: f32) -> f32 {
    (a + PI).rem_euclid(TAU) - PI
}

/// A local player, moved by its own input ahead of the host.
struct Own {
    pos: [f32; 2],
    yaw: f32,
    spin: f32,
    history: VecDeque<(u32, [f32; 2], f32)>,
    reconciled: u32,
    placed: bool,
}

struct Other {
    track: Track,
    yaw: f32,
}

struct Live {
    spawned: Spawned,
}

#[derive(Default)]
pub struct Mirror {
    /// The host's tick as the client reckons it now.
    clock: f64,
    synced: bool,
    zombies: HashMap<u32, Track>,
    players: Vec<PlayerState>,
    players_tick: u32,
    others: BTreeMap<usize, Other>,
    own: BTreeMap<usize, Own>,
    things: BTreeMap<u32, Thing>,
    things_tick: u32,
    live: Vec<Live>,
    /// Paused for the level-up cards, as the host is.
    pub paused: bool,
    /// The run as the newest snapshot has it.
    pub run: RunState,
    /// The host holds every player still: the pause menu or the level-up cards are up.
    pub frozen: bool,
}

impl Mirror {
    /// Player `seat` is played here: predicted, not drawn from snapshots.
    pub fn play_locally(&mut self, seat: usize) {
        self.own.entry(seat).or_insert(Own { pos: [0.0; 2], yaw: 0.0, spin: 0.0, history: VecDeque::new(), reconciled: 0, placed: false });
    }

    pub fn render_tick(&self) -> f64 {
        self.clock - INTERP_TICKS
    }

    pub fn apply(&mut self, part: &Part) {
        let tick = part.tick;
        if !self.synced || (tick as f64 - self.clock).abs() > 30.0 {
            (self.clock, self.synced) = (tick as f64, true);
        } else if tick as f64 > self.clock {
            self.clock += (tick as f64 - self.clock) * 0.1;
        }
        for z in &part.zombies {
            let pos = unq_pos(z.pos);
            let track = self.zombies.entry(z.id).or_insert_with(|| Track::new(tick, pos, *z));
            if tick >= track.newest() {
                track.state = *z;
            }
            track.add(tick, pos);
        }
        let present: std::collections::HashSet<u32> = part.zombies.iter().map(|z| z.id).collect();
        self.zombies.retain(|&id, t| !part.has_zombie_id(id) || present.contains(&id) || t.newest() > tick);
        if tick >= self.things_tick {
            self.things_tick = tick;
            let (lo, hi) = part.thing_range;
            self.things.retain(|&k, _| k < lo || (hi != u32::MAX && k >= hi));
            self.things.extend(part.things.iter().copied());
        }
        if tick >= self.players_tick {
            self.players_tick = tick;
            self.players = part.players.clone();
            self.run = part.run.clone();
        }
        for (seat, p) in part.players.iter().enumerate() {
            let pos = unq_pos(p.pos);
            match self.own.get_mut(&seat) {
                Some(own) => reconcile(own, p, tick),
                None => {
                    let other = self.others.entry(seat).or_insert_with(|| Other { track: Track::new(tick, pos, ZombieState::default()), yaw: 0.0 });
                    other.track.add(tick, pos);
                    if tick >= other.track.newest() {
                        other.yaw = unq_angle(p.yaw);
                    }
                }
            }
        }
    }

    pub fn spawn(&mut self, fresh: Vec<Spawned>) {
        self.live.extend(fresh.into_iter().map(|spawned| Live { spawned }));
    }

    /// Moves the clock and every drawn place on by `dt` seconds.
    pub fn advance(&mut self, dt: f32) {
        if !self.paused {
            self.clock += (dt * TICK_HZ as f32) as f64;
        }
        let at = self.render_tick();
        for t in self.zombies.values_mut() {
            let target = t.target(at);
            t.follow(target, dt);
        }
        for o in self.others.values_mut() {
            let target = o.track.target(at);
            o.track.follow(target, dt);
        }
        self.live.retain(|l| !spent(&l.spawned, at));
    }

    /// Plays local player `seat`'s input for client tick `tick` as the host will.
    pub fn predict(&mut self, seat: usize, tick: u32, input: &Input, turn_speed: f32) {
        let Some(own) = self.own.get_mut(&seat) else { return };
        let Some(p) = self.players.get(seat) else { return };
        if !own.placed {
            (own.pos, own.yaw, own.placed) = (unq_pos(p.pos), unq_angle(p.yaw), true);
        }
        if !self.paused && !self.frozen && p.flags & P_PICKING != 0 {
            let (d, k) = (input.dir(), p.speed as f32 / 16.0 * DT);
            own.pos = [own.pos[0] + d[0] * k, own.pos[1] + d[1] * k];
        } else if !self.paused && !self.frozen && (p.flags & P_DOWN == 0 || p.flags & P_LEADING != 0) {
            let dir = input.dir();
            let turn = (input.held.contains(Buttons::A) as i32 as f32 - input.held.contains(Buttons::Y) as i32 as f32 + input.look).clamp(-1.0, 1.0);
            if input.pressed.contains(Buttons::B) && own.spin <= 0.0 {
                own.spin = PI;
            }
            let spun = own.spin.min(PI / SPIN_TIME * DT);
            own.spin -= spun;
            own.yaw = (own.yaw + turn * turn_speed * DT + spun).rem_euclid(TAU);
            let f = [own.yaw.cos(), own.yaw.sin()];
            let (side, ahead) = (dir[0], -dir[1]);
            let mv = [f[0] * ahead - f[1] * side, f[1] * ahead + f[0] * side];
            let l = (mv[0] * mv[0] + mv[1] * mv[1]).sqrt();
            if l > 0.2 {
                let k = p.speed as f32 / 16.0 * DT * l.min(1.0) / l;
                own.pos = [own.pos[0] + mv[0] * k, own.pos[1] + mv[1] * k];
            }
        }
        own.history.push_back((tick, own.pos, own.yaw));
        while own.history.len() > 120 {
            own.history.pop_front();
        }
    }

    /// A shot or spit that hit something early stops being drawn.
    pub fn stop(&mut self, id: u32) {
        self.live.retain(|l| l.spawned.id != id);
    }

    /// The host's tick as the client reckons it now, with no interpolation delay.
    pub fn clock(&self) -> f64 {
        self.clock
    }

    pub fn player_count(&self) -> usize {
        self.players.len()
    }

    /// Every shot, spit and bolt in flight: where it is, its age in seconds, and what it is. Shots of the local
    /// `own` seats play out at the clock from the predicted player, the rest a little in the past as zombies are.
    pub fn shots_for(&self, own: &[usize]) -> Vec<([f32; 2], f32, &Spawned)> {
        let mine = |s: &Spawned| matches!(s.what, Spawn::Shot { owner, .. } if own.contains(&(owner as usize)));
        let lead = |s: &Spawned| -> [f32; 2] {
            let Spawn::Shot { owner, .. } = s.what else { return [0.0; 2] };
            let seat = owner as usize;
            match (self.own.get(&seat).filter(|o| o.placed), self.players.get(seat)) {
                (Some(o), Some(p)) => {
                    let host = unq_pos(p.pos);
                    [o.pos[0] - host[0], o.pos[1] - host[1]]
                }
                _ => [0.0; 2],
            }
        };
        let back = self.render_tick();
        self.live
            .iter()
            .filter_map(|l| {
                let s = &l.spawned;
                let (at, off) = if mine(s) { (self.clock, lead(s)) } else { (back, [0.0; 2]) };
                let p = shot_at(s, at)?;
                Some(([p[0] + off[0], p[1] + off[1]], (at - s.tick as f64) as f32 / TICK_HZ as f32, s))
            })
            .collect()
    }

    /// Where local player `seat` was predicted at client tick `tick`, as corrected since.
    pub fn predicted_at(&self, seat: usize, tick: u32) -> Option<[f32; 2]> {
        self.own.get(&seat)?.history.iter().find(|h| h.0 == tick).map(|h| h.1)
    }

    pub fn zombies(&self) -> impl Iterator<Item = ([f32; 2], &ZombieState)> {
        self.zombies.values().map(|t| (t.shown, &t.state))
    }

    pub fn zombie_count(&self) -> usize {
        self.zombies.len()
    }

    pub fn zombie_pos(&self, id: u32) -> Option<[f32; 2]> {
        self.zombies.get(&id).map(|t| t.shown)
    }

    /// Where to draw player `seat`, and the way it faces.
    pub fn player(&self, seat: usize) -> Option<([f32; 2], f32)> {
        if let Some(o) = self.own.get(&seat).filter(|o| o.placed) {
            return Some((o.pos, o.yaw));
        }
        self.others.get(&seat).map(|o| (o.track.shown, o.yaw))
    }

    pub fn player_state(&self, seat: usize) -> Option<&PlayerState> {
        self.players.get(seat)
    }

    pub fn things(&self) -> impl Iterator<Item = &Thing> {
        self.things.values()
    }

    /// Every shot, spit and bolt in flight, with where it is now.
    pub fn shots(&self) -> impl Iterator<Item = ([f32; 2], &Spawn)> {
        let at = self.render_tick();
        self.live.iter().filter_map(move |l| shot_at(&l.spawned, at).map(|p| (p, &l.spawned.what)))
    }
}

fn reconcile(own: &mut Own, p: &PlayerState, tick: u32) {
    if !own.placed || tick <= own.reconciled {
        return;
    }
    own.reconciled = tick;
    let Some(&(_, then, yaw)) = own.history.iter().find(|h| h.0 == p.input_tick) else { return };
    let host = unq_pos(p.pos);
    let err = [host[0] - then[0], host[1] - then[1]];
    let yaw_err = wrap(unq_angle(p.yaw) - yaw);
    let k = if dist(host, then) > OWN_SNAP { 1.0 } else { RECONCILE };
    let fix = [err[0] * k, err[1] * k];
    own.pos = [own.pos[0] + fix[0], own.pos[1] + fix[1]];
    own.yaw = (own.yaw + yaw_err * k).rem_euclid(TAU);
    for h in own.history.iter_mut().filter(|h| h.0 >= p.input_tick) {
        h.1 = [h.1[0] + fix[0], h.1[1] + fix[1]];
        h.2 = (h.2 + yaw_err * k).rem_euclid(TAU);
    }
}

fn lasts(s: &Spawned) -> f32 {
    match &s.what {
        Spawn::Shot { life, .. } | Spawn::Spit { life, .. } => *life as f32 / 50.0,
        Spawn::Bolt { .. } => 0.3,
        Spawn::Fx { .. } => 1.0,
    }
}

fn spent(s: &Spawned, at: f64) -> bool {
    (at - s.tick as f64) as f32 / TICK_HZ as f32 > lasts(s)
}

/// A spawn's place at host tick `at`, or None before it began or once it's spent.
fn shot_at(s: &Spawned, at: f64) -> Option<[f32; 2]> {
    let age = (at - s.tick as f64) as f32 / TICK_HZ as f32;
    if age < 0.0 || age > lasts(s) {
        return None;
    }
    Some(match &s.what {
        Spawn::Shot { from, vel, .. } | Spawn::Spit { from, vel, .. } => {
            let from = unq_pos(*from);
            [from[0] + vel[0] as f32 / 8.0 * age, from[1] + vel[1] as f32 / 8.0 * age]
        }
        Spawn::Bolt { points } => points.first().map_or([0.0; 2], |p| unq_pos(*p)),
        Spawn::Fx { pos, .. } => unq_pos(*pos),
    })
}

