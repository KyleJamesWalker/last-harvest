//! Online co-op's host end: the client's view pulled out of a night, and a whole night played to a client over a
//! simulated network, to measure what the sync costs and how far the client's picture strays.

use std::collections::{HashMap, HashSet, VecDeque};
use std::f32::consts::TAU;

use super::player::{ME, PlayerId};
use super::sandbox::{Bot, Plan, Walk};
use super::weapons::{Hazard, Kit};
use super::{DropKind, Harvest, Phase, Thrown, dawn};
use crate::netsync::input::{self, InputFrame, InputLog, InputMsg, InputQueue};
use crate::netsync::snapshot::*;
use crate::netsync::{ClientLink, Event, HostLink, Mirror, Pipe};
use crate::rng::Rng;
use crate::sfx::Sfx;

/// Which end of an online night this run is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Role {
    #[default]
    Off,
    Host,
    Client,
}

/// A shot's start and velocity as sent, which name it while it flies.
type ShotKey = (u16, u16, i16, i16);

/// A placed sound's pan and gain, and where on the farm it came from.
pub(super) type Placed = (i8, u8, [f32; 2]);

/// A run's part in online play.
#[derive(Default)]
pub(super) struct Net {
    pub role: Role,
    /// Views drawn on this device, from ME on; 0 draws every view.
    pub drawn: usize,
    /// The seat each place in `players` stands for, where it differs from the place.
    pub seats: Vec<usize>,
    /// Sounds placed this tick, with where on the farm they came from.
    pub placed: std::cell::RefCell<Vec<Placed>>,
    pub client: Option<Box<super::remote::Client>>,
}

impl Harvest {
    /// Hosts an online night: the first `local` players play here, and the rest, each with a view of their own on a
    /// client, by the input `steer` gives them.
    pub fn hosting(mut self, local: usize) -> Self {
        let views = self.players.len();
        self = self.with_views(views);
        self.net.role = Role::Host;
        self.net.drawn = local.clamp(1, views);
        self.juice.still = true;
        self
    }

    pub fn is_host(&self) -> bool {
        self.net.role == Role::Host
    }

    #[cfg(test)]
    pub(crate) fn paused_for_test(&self) -> bool {
        self.menu.is_some()
    }

    #[cfg(test)]
    pub(crate) fn alive_for_test(&self, p: PlayerId) -> bool {
        self.players[p].alive()
    }

    pub fn player_count(&self) -> usize {
        self.players.len()
    }

    /// Each seat in its card's look and starting kit, where it stands; on a client the seats are in view order.
    pub fn dressed(mut self, looks: &[(usize, String)]) -> Self {
        for (i, pl) in self.players.iter_mut().enumerate() {
            let seat = self.net.seats.get(i).copied().unwrap_or(i);
            if let Some((_, card)) = looks.iter().find(|l| l.0 == seat) {
                pl.look = card.clone();
            }
        }
        for i in 0..self.players.len() {
            let look = super::players::look(&self.players[i].look);
            self.players[i] = self.new_player(self.players[i].pos, look);
        }
        self.juice.tally = super::tally::Tally::sized(super::book().defs.len(), self.players.len());
        self
    }

    pub fn zombie_spots(&self) -> Vec<(u32, [f32; 2])> {
        self.zombies.iter().chain(&self.burrowed).map(|z| (z.id, z.pos)).collect()
    }

    pub fn player_spots(&self) -> Vec<[f32; 2]> {
        self.players.iter().map(|p| p.pos).collect()
    }

    /// Seats a player who joined mid-run at `p`: a new player, or a fresh one in place of one who left, beside
    /// whoever still stands.
    pub fn seat(&mut self, p: PlayerId, look: &str) {
        let look = super::players::look(look);
        let near = (0..self.players.len()).find(|&q| self.players[q].alive()).map(|q| self.players[q].pos);
        let half = crate::tuning::tuning().player_half();
        let pos = match near {
            Some(at) => self.map.slide(at, [14.0, 0.0], half),
            None => self.beside_spawn(p),
        };
        let fresh = self.new_player(pos, look);
        if p < self.players.len() {
            self.players[p] = fresh;
        } else {
            while self.players.len() < p {
                let filler = self.new_player(pos, look);
                self.players.push(filler);
                self.players.last_mut().unwrap().down = Some(super::TURNED);
            }
            self.players.push(fresh);
        }
        self.inputs.resize(self.players.len(), crate::Input::default());
        self.inputs[p] = crate::Input::default();
        self.juice.tally.book_mut(p);
        while self.locals.len() < self.players.len() {
            let l = super::local::Local::of(self, self.locals.len());
            self.locals.push(l);
        }
        self.locals[p] = super::local::Local::of(self, p);
        self.views = self.views.max(p + 1);
        self.juice.still = true;
    }

    /// Player `p`'s device left: they turn where they stand, and their seat waits for whoever drops in next.
    pub fn unseat(&mut self, p: PlayerId) {
        let Some(pl) = self.players.get_mut(p) else { return };
        if pl.alive() {
            pl.hp = 0.0;
            pl.down = Some(0.0);
            pl.revived = u32::MAX;
        }
        if let Some(i) = self.inputs.get_mut(p) {
            *i = crate::Input::default();
        }
        if let Some(l) = self.locals.get_mut(p) {
            l.cards = None;
        }
        if self.paused_by == p {
            self.menu = None;
        }
        if matches!(self.phase, Phase::Playing) && self.players.iter().all(|pl| !pl.alive()) {
            self.phase = Phase::Over { t: 0.0, won: false };
        }
    }
}

/// Tracks a night tick by tick for its clients.
#[derive(Default)]
pub struct Extract {
    /// Each zombie's most health seen, for its share left.
    most_hp: HashMap<u32, f32>,
    last: HashMap<u32, (u8, [f32; 2])>,
    kills: u32,
    /// Each player's kills so far, which name who made each new one.
    kills_by: Vec<u32>,
    phase: u8,
    /// Shots and spits in flight by their content key: the spawn id each went out as, its tick and life in ticks.
    shots_seen: HashMap<ShotKey, (u32, u32, u32)>,
    fx_seen: HashSet<(u16, u16, u16)>,
    next_spawn: u32,
    /// Every critical event sent, in order.
    pub made: Vec<Event>,
}

/// One tick's news for every client: events, and spawns to play out.
#[derive(Default)]
pub struct Fresh {
    pub tick: u32,
    pub events: Vec<Event>,
    pub spawns: Vec<Spawned>,
}

impl Fresh {
    pub fn hand(&self, link: &mut HostLink) {
        self.events.iter().for_each(|&e| link.push_event(self.tick, e));
        self.spawns.iter().for_each(|s| link.push_spawn(s.clone()));
    }
}

fn phase_of(p: &Phase) -> u8 {
    match p {
        Phase::Playing => 0,
        Phase::LevelUp { .. } => 1,
        Phase::Over { .. } => 2,
    }
}

impl Extract {
    fn emit(&mut self, out: &mut Fresh, ev: Event) {
        if ev.critical() {
            self.made.push(ev);
        }
        out.events.push(ev);
    }

    /// Hands `link` this tick's events and spawns; `sounds` are the cues the tick played.
    pub fn tick(&mut self, h: &Harvest, tick: u32, sounds: &[Sfx], link: &mut HostLink) {
        self.record(h, tick, sounds).hand(link);
    }

    /// This tick's events and spawns, for every client; `sounds` are the cues the tick played.
    pub fn record(&mut self, h: &Harvest, tick: u32, sounds: &[Sfx]) -> Fresh {
        let mut out = Fresh { tick, ..Fresh::default() };
        let placed = std::mem::take(&mut *h.net.placed.borrow_mut());
        let mut heard: Vec<Sfx> = Vec::new();
        for s in sounds {
            if heard.contains(s) {
                continue;
            }
            heard.push(*s);
            let at = match *s {
                Sfx::Boom { pan, gain } | Sfx::Rise { pan, gain } | Sfx::Groan { pan, gain } | Sfx::Caw { pan, gain } | Sfx::Spit { pan, gain } | Sfx::Splat { pan, gain } => placed.iter().find(|p| (p.0, p.1) == (pan, gain)).map(|p| p.2),
                _ => None,
            };
            out.events.push(match at {
                Some(pos) => Event::SoundAt(*s, q_pos(pos)),
                None => Event::Sound(*s),
            });
        }
        let now: HashMap<u32, (u8, [f32; 2])> = h.zombies.iter().chain(&h.burrowed).map(|z| (z.id, (z.kind.0, z.pos))).collect();
        let mut killed = h.kills.saturating_sub(self.kills);
        self.kills = h.kills;
        let mut killers = Vec::new();
        self.kills_by.resize(h.players.len(), 0);
        for (p, before) in self.kills_by.iter_mut().enumerate() {
            let now = h.juice.tally.sum(p).0;
            killers.extend(std::iter::repeat_n(p as u8, now.saturating_sub(*before) as usize));
            *before = now;
        }
        let mut killers = killers.into_iter();
        let mut gone: Vec<(u32, (u8, [f32; 2]))> = self.last.iter().filter(|(id, _)| !now.contains_key(id)).map(|(&id, &z)| (id, z)).collect();
        gone.sort_by_key(|g| g.0);
        for (id, (kind, pos)) in gone {
            self.most_hp.remove(&id);
            if killed > 0 {
                killed -= 1;
                let by = killers.next().unwrap_or(0);
                self.emit(&mut out, Event::Kill { id, kind, pos: q_pos(pos), by });
            }
        }
        self.last = now;
        let phase = phase_of(&h.phase);
        if phase != self.phase {
            let ev = match (self.phase, phase) {
                (_, 1) => Some(Event::LevelUp { level: h.level.min(255) as u8 }),
                (1, 0) => Some(Event::LevelDone),
                (_, 2) => Some(Event::RunEnd { won: matches!(h.phase, Phase::Over { won: true, .. }) }),
                _ => None,
            };
            self.phase = phase;
            if let Some(ev) = ev {
                self.emit(&mut out, ev);
            }
        }
        let mut new = Vec::new();
        let vel8 = |v: [f32; 2]| v.map(|c| (c * 8.0).clamp(i16::MIN as f32, i16::MAX as f32) as i16);
        let life50 = |l: f32| (l * 50.0).clamp(0.0, 255.0) as u8;
        let mut shots = HashMap::new();
        let ticks = |secs: f32| (secs * TICK_HZ as f32).round() as u32;
        for b in &h.bullets {
            let from = q_pos([b.pos[0] - b.vel[0] * b.age, b.pos[1] - b.vel[1] * b.age]);
            let key = (from[0], from[1], vel8(b.vel)[0], vel8(b.vel)[1]);
            let seen = self.shots_seen.get(&key).copied().unwrap_or_else(|| {
                let look = b.by.map_or(0, |d| d as u16 + 1);
                new.push(Spawn::Shot { owner: b.owner as u8, look, from, vel: vel8(b.vel), life: life50(b.life + b.age) });
                (self.next_spawn + new.len() as u32, tick, ticks(b.life + b.age))
            });
            shots.insert(key, seen);
        }
        for s in &h.spits {
            let from = q_pos([s.pos[0] - s.vel[0] * s.age, s.pos[1] - s.vel[1] * s.age]);
            let key = (from[0], from[1], vel8(s.vel)[0], vel8(s.vel)[1]);
            let seen = self.shots_seen.get(&key).copied().unwrap_or_else(|| {
                new.push(Spawn::Spit { kind: s.kind.0, from, vel: vel8(s.vel), life: life50(s.life + s.age) });
                (self.next_spawn + new.len() as u32, tick, ticks(s.life + s.age))
            });
            shots.insert(key, seen);
        }
        for (key, &(id, born, life)) in &self.shots_seen {
            if !shots.contains_key(key) && tick.saturating_sub(born) + 2 < life {
                out.events.push(Event::Stop { id });
            }
        }
        for b in h.bolts.iter().filter(|b| b.age == 0.0) {
            new.push(Spawn::Bolt { points: b.points.iter().map(|&p| q_pos(p)).collect() });
        }
        self.shots_seen = shots;
        let mut fx = HashSet::new();
        for f in &h.fx {
            let pos = q_pos(f.pos);
            let art = f.frames.first().map_or(0, |a| a.0);
            let key = (art, pos[0], pos[1]);
            if !self.fx_seen.contains(&key) {
                new.push(Spawn::Fx { art, pos, angle: (f.angle.rem_euclid(TAU) / TAU * 256.0) as u8 });
            }
            fx.insert(key);
        }
        self.fx_seen = fx;
        for what in new {
            self.next_spawn += 1;
            out.spawns.push(Spawned { id: self.next_spawn, tick, what });
        }
        out
    }

    /// The world at `tick`, with the last client input the host has played for each player.
    pub fn world(&mut self, h: &Harvest, tick: u32, input_ticks: &[u32]) -> World {
        let players = h
            .players
            .iter()
            .enumerate()
            .map(|(p, pl)| PlayerState {
                pos: q_pos(pl.pos),
                yaw: q_angle(pl.view_yaw),
                hp: (pl.hp.max(0.0) * 4.0).min(u16::MAX as f32) as u16,
                max_hp: (pl.kit.max_hp() * 4.0).min(u16::MAX as f32) as u16,
                hurt: (pl.hurt * 100.0).clamp(0.0, 255.0) as u8,
                flags: if pl.down.is_some() { P_DOWN } else { 0 } | if pl.walk > 0.0 { P_WALKING } else { 0 } | if pl.ready_flash > 0.0 { P_READY_FLASH } else { 0 },
                down: pl.down.map_or(0, |t| (t * 10.0).clamp(0.0, 255.0) as u8),
                grenades: (pl.grenades * 16.0).clamp(0.0, 255.0) as u8,
                speed: (pl.kit.speed() * 16.0).clamp(0.0, u16::MAX as f32) as u16,
                input_tick: input_ticks.get(p).copied().unwrap_or(0),
                kit: pl.kit.weapons.iter().map(|a| (a.def, a.level)).chain(pl.kit.passives.iter().copied()).map(|(d, l)| (d.min(255) as u8, l.min(255) as u8)).collect(),
            })
            .collect();
        let mut zombies = Vec::with_capacity(h.zombies.len() + h.burrowed.len());
        for (z, under) in h.zombies.iter().map(|z| (z, false)).chain(h.burrowed.iter().map(|z| (z, true))) {
            let most = self.most_hp.entry(z.id).or_insert(z.hp);
            *most = most.max(z.hp);
            let bit = |on: bool, b: u8| if on { b } else { 0 };
            let flags = bit(under, Z_BURROWED) | bit(z.leader, Z_LEADER) | bit(z.boss, Z_BOSS) | bit(z.hurt > 0.0, Z_HURT) | bit(z.burn > 0.0, Z_BURNING) | bit(z.rise < 1.0, Z_RISING) | bit(z.slow > 0.0, Z_SLOWED);
            zombies.push(ZombieState { id: z.id, kind: z.kind.0, pos: q_pos(z.pos), hp: (z.hp.max(0.0) / most.max(1e-3) * 255.0).round() as u8, flags });
        }
        let born = |age: f32| (tick as f32 - age * TICK_HZ as f32).round() as i64 as u16;
        let px = |p: [f32; 2]| q_pos(p.map(f32::round));
        let mut things = Vec::new();
        for g in &h.thrown {
            let sub = match g.kind {
                Thrown::Grenade => 0,
                Thrown::Bomblet => 1,
                Thrown::Molotov { .. } => 2,
                Thrown::Flare { .. } => 3,
                Thrown::Lob { def, .. } => 4 + def.min(250) as u8,
            };
            let to = q_pos(g.to);
            things.push(Thing { kind: T_GRENADE, sub, pos: q_pos(g.from), data: [to[0], to[1], born(g.t)] });
        }
        for d in &h.drops {
            let (kind, value) = match d.kind {
                DropKind::Money(v) => (T_MONEY, v.min(u16::MAX as u32) as u16),
                DropKind::Health => (T_HEALTH, 0),
                DropKind::Chest => (T_CHEST, 0),
            };
            things.push(Thing { kind, sub: 0, pos: px(d.pos), data: [value, 0, 0] });
        }
        for p in &h.patches {
            let sub = match p.hazard {
                Hazard::Tacks => 0,
                Hazard::Fire => 1,
                Hazard::Flare => 2,
            };
            things.push(Thing { kind: T_PATCH, sub, pos: q_pos(p.pos), data: [q(p.radius), born(p.age), (p.life * 10.0).clamp(0.0, u16::MAX as f32) as u16] });
        }
        for b in &h.barrels {
            things.push(Thing { kind: T_BARREL, sub: 0, pos: q_pos(b.pos), data: [b.hp.max(0.0).ceil().min(u16::MAX as f32) as u16, 0, 0] });
        }
        for d in &h.decoys {
            things.push(Thing { kind: T_DECOY, sub: 0, pos: q_pos(d.pos), data: [born(d.age), (d.life * 10.0).clamp(0.0, u16::MAX as f32) as u16, 0] });
        }
        for t in &h.tells {
            things.push(Thing { kind: T_TELL, sub: t.kind.0, pos: q_pos(t.pos), data: [t.leader as u16, 0, 0] });
        }
        let mut world = World::new(tick, players, zombies, things);
        world.run = run_state(h);
        world
    }
}

/// The run's clock, shared numbers, pause and each viewed player's cards, as a client draws them.
fn run_state(h: &Harvest) -> RunState {
    let phase = match h.phase {
        Phase::Playing if h.leveling > 0.0 => PHASE_LEVELING,
        Phase::Playing => PHASE_PLAYING,
        Phase::LevelUp { .. } => PHASE_CARDS,
        Phase::Over { won: true, .. } => PHASE_WON,
        Phase::Over { won: false, .. } => PHASE_LOST,
    };
    let cards = (0..h.views).filter_map(|p| h.cards_of(p)).collect();
    RunState {
        t: (h.t * 10.0).max(0.0) as u32,
        phase,
        level: h.level.min(u16::MAX as u32) as u16,
        xp: h.xp,
        next_xp: h.next_xp,
        kills: h.kills,
        menu: h.menu.map_or(0, |m| m as u8 + 1),
        paused_by: h.paused_by as u8,
        chest: h.chest,
        cards,
    }
}

/// The network a simulated client sits behind.
#[derive(Clone, Copy, Debug)]
pub struct NetSim {
    /// Share of packets lost each way, 0 to 1.
    pub loss: f32,
    pub latency_ms: u32,
    /// Up to this much more per packet, which reorders them.
    pub jitter_ms: u32,
}

impl Default for NetSim {
    fn default() -> Self {
        Self { loss: 0.1, latency_ms: 80, jitter_ms: 20 }
    }
}

impl NetSim {
    /// `loss=0.1,lat=80,jitter=20`, any of them left out.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut n = NetSim::default();
        for item in text.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let (k, v) = item.split_once('=').ok_or_else(|| format!("{item}: not key=value"))?;
            let num = |v: &str| v.parse::<f32>().map_err(|_| format!("{item}: {v} is not a number"));
            match k {
                "loss" => n.loss = num(v)?.clamp(0.0, 1.0),
                "lat" | "latency" => n.latency_ms = num(v)?.max(0.0) as u32,
                "jitter" => n.jitter_ms = num(v)?.max(0.0) as u32,
                _ => return Err(format!("{k}: not loss, lat or jitter")),
            }
        }
        Ok(n)
    }
}

/// How far the client's picture strayed from the host's, in pixels.
#[derive(Clone, Copy, Debug, Default)]
pub struct Drift {
    pub mean: f32,
    pub p99: f32,
    pub worst: f32,
}

fn drift(mut d: Vec<f32>) -> Drift {
    if d.is_empty() {
        return Drift::default();
    }
    d.sort_by(f32::total_cmp);
    Drift { mean: d.iter().sum::<f32>() / d.len() as f32, p99: d[(d.len() - 1) * 99 / 100], worst: d[d.len() - 1] }
}

#[derive(Clone, Debug, Default)]
pub struct NetReport {
    pub secs: f32,
    pub snapshots: u64,
    pub packets: u64,
    pub bytes_down: u64,
    pub bytes_up: u64,
    pub biggest_packet: usize,
    pub zombies_mean: f32,
    pub zombies_most: usize,
    /// Zombies drawn against where the host has them now, and against where the host had them at the tick drawn.
    pub zombies_now: Drift,
    pub zombies_then: Drift,
    /// The host's own player, drawn from snapshots; the client's, predicted, against the host now; and the
    /// prediction against the host once it has played the same input.
    pub other_player: Drift,
    pub own_player: Drift,
    pub prediction: Drift,
    /// Critical events made, and delivered once each in order.
    pub events_made: usize,
    pub events_delivered: usize,
    pub events_in_order: bool,
    pub undecodable: u64,
}

const STEP: f32 = 1.0 / TICK_HZ as f32;

/// Plays `plan`'s night, an empty build keeping the starting kit, with two players: the host's scripted one, and a client's, also scripted, whose input
/// crosses `net` to the host and whose picture of the night is a `Mirror` fed by snapshots crossing back.
pub fn net_sim(plan: &Plan, net: NetSim) -> NetReport {
    let mut h = Harvest::sized(plan.seed, plan.size).at(plan.difficulty).with_players(2);
    for pl in h.players.iter_mut().filter(|_| !plan.build.is_empty()) {
        pl.kit = Kit::forced(&plan.build, plan.difficulty);
        pl.hp = pl.kit.max_hp();
        pl.grenades = pl.kit.grenades();
    }
    h.undying = !plan.mortal;
    h.progress.everything = plan.unlocked;
    const CLIENT: PlayerId = 1;
    let ticks = |ms: u32| (ms as f32 / 1000.0 * TICK_HZ as f32).round() as u32;
    let mut down: Pipe<Vec<u8>> = Pipe::new(plan.seed ^ 0xD0, net.loss, ticks(net.latency_ms), ticks(net.jitter_ms));
    let mut up: Pipe<Vec<u8>> = Pipe::new(plan.seed ^ 0x0F, net.loss, ticks(net.latency_ms), ticks(net.jitter_ms));
    let mut me = Bot::new(plan.walk, plan.turn, plan.turn_every, plan.throw_every);
    let mut them = Bot::new(Walk::Avoid, plan.turn, 0.0, plan.throw_every);
    let (mut host, mut client, mut mirror, mut extract) = (HostLink::default(), ClientLink::default(), Mirror::default(), Extract::default());
    mirror.play_locally(CLIENT);
    let (mut log, mut queue) = (InputLog::default(), InputQueue::default());
    let mut played = [0u32; 2];
    let mut sounds = Vec::new();
    let mut report = NetReport { events_in_order: true, ..NetReport::default() };
    let (mut zn, mut zt, mut other, mut own, mut predicted) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut past: VecDeque<(u32, HashMap<u32, [f32; 2]>)> = VecDeque::new();
    let mut zombie_sum = 0u64;
    let mut delivered = Vec::new();
    let mut rng = Rng::new(plan.seed);
    let end = plan.until.unwrap_or(dawn());
    let turn_speed = crate::tuning::tuning().turn_speed;
    let mut tick = 0u32;
    while h.t < end && !matches!(h.phase, Phase::Over { .. }) && tick < (end * TICK_HZ as f32) as u32 * 8 {
        tick += 1;
        let mine = me.step(&mut h, ME, &mut sounds);
        let theirs = if h.players[CLIENT].alive() { them.step(&mut h, CLIENT, &mut sounds) } else { crate::Input::default() };
        mirror.predict(CLIENT, tick, &theirs, turn_speed);
        log.push(tick, InputFrame::of(&theirs));
        if tick.is_multiple_of(input::SEND_EVERY) {
            let (s, e) = client.ack();
            let m = InputMsg { ack_snapshot: s, ack_event: e, seats: vec![log.seat(CLIENT as u8)] }.encode();
            report.bytes_up += m.len() as u64;
            up.send(tick, m);
        }
        for m in up.receive(tick) {
            if let Some(m) = InputMsg::decode(&m) {
                host.ack(m.ack_snapshot, m.ack_event);
                m.seats.iter().filter(|s| s.seat as usize == CLIENT).for_each(|s| queue.receive(s));
            }
        }
        let (input, at) = queue.play();
        played[CLIENT] = at.unwrap_or(0);
        h.steer(CLIENT, input);
        h.update(&mine, STEP, &mut sounds);
        if let Some(p) = at.and_then(|k| mirror.predicted_at(CLIENT, k)).filter(|_| h.players[CLIENT].alive()) {
            predicted.push(dist(p, h.players[CLIENT].pos));
        }
        extract.tick(&h, tick, &sounds, &mut host);
        sounds.clear();
        if let Phase::LevelUp { .. } = h.phase {
            h.phase = Phase::Playing;
            h.chest = false;
            extract.tick(&h, tick, &[], &mut host);
        }
        if tick.is_multiple_of(SEND_EVERY) {
            for p in host.snapshot(extract.world(&h, tick, &played)) {
                report.biggest_packet = report.biggest_packet.max(p.len());
                down.send(tick, p);
            }
        }
        for p in down.receive(tick) {
            if let Some(part) = client.receive(&p) {
                mirror.apply(&part);
                let fresh = client.fresh(&part.spawns);
                mirror.spawn(fresh);
            }
        }
        delivered.extend(client.events().into_iter().filter(Event::critical));
        mirror.advance(STEP);

        let here: HashMap<u32, [f32; 2]> = h.zombies.iter().chain(&h.burrowed).map(|z| (z.id, z.pos)).collect();
        zombie_sum += here.len() as u64;
        report.zombies_most = report.zombies_most.max(here.len());
        let at = mirror.render_tick().round().max(0.0) as u32;
        let then = past.iter().find(|p| p.0 == at).map(|p| &p.1);
        let sample = rng.below(4) == 0;
        for (id, pos) in &here {
            if let Some(shown) = mirror.zombie_pos(*id).filter(|_| sample) {
                zn.push(dist(shown, *pos));
                if let Some(old) = then.and_then(|t| t.get(id)) {
                    zt.push(dist(shown, *old));
                }
            }
        }
        if let Some((p, _)) = mirror.player(ME) {
            other.push(dist(p, h.players[ME].pos));
        }
        if let Some((p, _)) = mirror.player(CLIENT).filter(|_| h.players[CLIENT].alive()) {
            own.push(dist(p, h.players[CLIENT].pos));
        }
        past.push_back((tick, here));
        while past.len() > 60 {
            past.pop_front();
        }
    }
    report.secs = tick as f32 / TICK_HZ as f32;
    report.snapshots = (tick / SEND_EVERY) as u64;
    report.packets = host.packets;
    report.bytes_down = host.bytes;
    report.zombies_mean = zombie_sum as f32 / tick.max(1) as f32;
    report.zombies_now = drift(zn);
    report.zombies_then = drift(zt);
    report.other_player = drift(other);
    report.own_player = drift(own);
    report.prediction = drift(predicted);
    report.events_made = extract.made.len();
    report.events_delivered = delivered.len();
    report.events_in_order = extract.made.starts_with(&delivered);
    report.undecodable = client.undecodable;
    report
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_client_over_a_lossy_link_keeps_close_to_the_host() {
        let plan = Plan { until: Some(40.0), ..Plan::new(Vec::new()) };
        let r = net_sim(&plan, NetSim { loss: 0.2, latency_ms: 80, jitter_ms: 30 });
        assert!(r.zombies_most > 0);
        assert!(r.biggest_packet <= BUDGET);
        assert_eq!(r.undecodable, 0);
        assert!(r.events_in_order && r.events_delivered + 5 >= r.events_made, "{} of {}", r.events_delivered, r.events_made);
        assert!(r.zombies_then.mean < 1.0 && r.zombies_now.mean < 8.0, "{:?} {:?}", r.zombies_then, r.zombies_now);
        assert!(r.prediction.mean < 4.0, "{:?}", r.prediction);
    }
}
