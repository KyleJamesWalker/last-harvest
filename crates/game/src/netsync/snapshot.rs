//! The host's world as a client draws it, quantized, and the packets that carry it: each delta coded against a
//! snapshot the client acked and split into parts that each decode alone.

use std::f32::consts::TAU;

use super::events::Event;
use super::wire::{Reader, Writer};

pub const VERSION: u8 = 3;
pub const TICK_HZ: u32 = 60;
/// Host ticks between snapshots: 20 a second.
pub const SEND_EVERY: u32 = 3;
/// The most bytes a packet may hold.
pub const BUDGET: usize = 1100;
/// Positions travel in 1/16 px.
pub const UNIT: f32 = 16.0;
/// A range's end that takes in every id above its start.
const OPEN: u32 = u32::MAX;

pub fn q(v: f32) -> u16 {
    (v * UNIT).round().clamp(0.0, u16::MAX as f32) as u16
}

pub fn unq(v: u16) -> f32 {
    v as f32 / UNIT
}

pub fn q_pos(p: [f32; 2]) -> [u16; 2] {
    p.map(q)
}

pub fn unq_pos(p: [u16; 2]) -> [f32; 2] {
    p.map(unq)
}

pub fn q_angle(a: f32) -> u16 {
    (a.rem_euclid(TAU) / TAU * 65536.0).round() as u32 as u16
}

pub fn unq_angle(a: u16) -> f32 {
    a as f32 / 65536.0 * TAU
}

pub const P_DOWN: u8 = 1;
pub const P_WALKING: u8 = 2;
pub const P_READY_FLASH: u8 = 4;
/// Turned, and steering a leader of the dead; its zombie rides in `zombies` with `Z_LEADER`.
pub const P_LEADING: u8 = 8;
/// Turned, and picking where the next leader rises; `pos` is the cursor, for that player only.
pub const P_PICKING: u8 = 16;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlayerState {
    pub pos: [u16; 2],
    pub yaw: u16,
    /// Health and its most, in quarter points.
    pub hp: u16,
    pub max_hp: u16,
    /// Hundredths of a second.
    pub hurt: u8,
    pub flags: u8,
    /// Tenths of a second since falling.
    pub down: u8,
    /// Sixteenths of a grenade.
    pub grenades: u8,
    /// Sixteenths of a pixel a second.
    pub speed: u16,
    /// The last of this player's client inputs the host has played, for reconciling the client's prediction.
    pub input_tick: u32,
    /// Weapons then passives: place in the powerup book and level.
    pub kit: Vec<(u8, u8)>,
    /// With `P_LEADING`, the leader's zombie id; with `P_PICKING`, tenths of a second left to pick.
    pub lead: u32,
    /// With `P_LEADING`, the leader's health left, in 1/255.
    pub lead_hp: u8,
}

impl PlayerState {
    fn write(&self, w: &mut Writer) {
        w.u16(self.pos[0]);
        w.u16(self.pos[1]);
        w.u16(self.yaw);
        w.var(self.hp as u32);
        w.var(self.max_hp as u32);
        w.u8(self.hurt);
        w.u8(self.flags);
        w.u8(self.down);
        w.u8(self.grenades);
        w.var(self.speed as u32);
        w.var(self.input_tick);
        w.u8(self.kit.len() as u8);
        for &(d, l) in &self.kit {
            w.u8(d);
            w.u8(l);
        }
        if self.flags & (P_LEADING | P_PICKING) != 0 {
            w.var(self.lead);
            w.u8(self.lead_hp);
        }
    }

    fn read(r: &mut Reader) -> Option<Self> {
        let mut p = PlayerState {
            pos: [r.u16()?, r.u16()?],
            yaw: r.u16()?,
            hp: r.var()? as u16,
            max_hp: r.var()? as u16,
            hurt: r.u8()?,
            flags: r.u8()?,
            down: r.u8()?,
            grenades: r.u8()?,
            speed: r.var()? as u16,
            input_tick: r.var()?,
            kit: Vec::new(),
            lead: 0,
            lead_hp: 0,
        };
        for _ in 0..r.u8()? {
            p.kit.push((r.u8()?, r.u8()?));
        }
        if p.flags & (P_LEADING | P_PICKING) != 0 {
            (p.lead, p.lead_hp) = (r.var()?, r.u8()?);
        }
        Some(p)
    }
}

/// The run as a whole: the night's clock, the shared XP and kills, the pause menu and every player's level-up cards.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunState {
    /// Tenths of a second into the night.
    pub t: u32,
    pub phase: u8,
    pub level: u16,
    pub xp: u32,
    pub next_xp: u32,
    pub kills: u32,
    /// The pause menu's row plus one, 0 when not paused, and the seat that opened it.
    pub menu: u8,
    pub paused_by: u8,
    /// The level-up on its way is a chest's.
    pub chest: bool,
    pub cards: Vec<SeatCards>,
    /// Tenths of a second into the night the truck driver's next collection is ready, rounded up.
    pub ticket: u32,
    /// Once the night is over: hundredths of a second played, and every player's numbers for the end screen.
    pub played: u32,
    pub books: Vec<BookState>,
}

/// One player's damage, hearts and time leading the dead over the night, as the end screen shows them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BookState {
    pub seat: u8,
    /// Each weapon held or that dealt damage: its place in the powerup book, hundredths of a second held, damage.
    pub weapons: Vec<(u8, u32, u32)>,
    /// Grenades' and barrels' damage.
    pub other: u32,
    /// Killing blows and useful damage, every weapon's and grenades' and barrels'.
    pub kills: u32,
    pub useful: u32,
    pub hearts: u32,
    /// Tenths of a second into the night the player first fell, plus one; 0 if they never fell.
    pub fell: u32,
    /// Tenths of a second spent leading the dead.
    pub led: u32,
    pub bites: u32,
}

impl BookState {
    fn write(&self, w: &mut Writer) {
        w.u8(self.seat);
        w.u8(self.weapons.len() as u8);
        for &(d, held, damage) in &self.weapons {
            w.u8(d);
            w.var(held);
            w.var(damage);
        }
        for v in [self.other, self.kills, self.useful, self.hearts, self.fell, self.led, self.bites] {
            w.var(v);
        }
    }

    fn read(r: &mut Reader) -> Option<Self> {
        let seat = r.u8()?;
        let weapons = (0..r.u8()?).map(|_| Some((r.u8()?, r.var()?, r.var()?))).collect::<Option<Vec<_>>>()?;
        Some(BookState { seat, weapons, other: r.var()?, kills: r.var()?, useful: r.var()?, hearts: r.var()?, fell: r.var()?, led: r.var()?, bites: r.var()? })
    }
}

pub const PHASE_PLAYING: u8 = 0;
pub const PHASE_LEVELING: u8 = 1;
pub const PHASE_CARDS: u8 = 2;
pub const PHASE_WON: u8 = 3;
pub const PHASE_LOST: u8 = 4;

/// One seat's level-up cards: each as its place in the powerup book with rare and found bits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SeatCards {
    pub seat: u8,
    pub choices: [(u8, u8); 3],
    pub selected: u8,
    pub picked: bool,
    pub banishes: u8,
    pub rerolls: u8,
    pub favors: u8,
}

pub const CARD_RARE: u8 = 1;
pub const CARD_FOUND: u8 = 2;

impl RunState {
    fn write(&self, w: &mut Writer) {
        w.var(self.t);
        w.u8(self.phase);
        w.var(self.level as u32);
        w.var(self.xp);
        w.var(self.next_xp);
        w.var(self.kills);
        w.u8(self.menu);
        w.u8(self.paused_by);
        w.u8(self.chest as u8);
        w.u8(self.cards.len() as u8);
        for c in &self.cards {
            w.u8(c.seat);
            for (up, bits) in c.choices {
                w.u8(up);
                w.u8(bits);
            }
            w.u8(c.selected);
            w.u8(c.picked as u8);
            w.u8(c.banishes);
            w.u8(c.rerolls);
            w.u8(c.favors);
        }
        w.var(self.ticket);
        w.u8(self.books.len() as u8);
        if !self.books.is_empty() {
            w.var(self.played);
            self.books.iter().for_each(|b| b.write(w));
        }
    }

    fn read(r: &mut Reader) -> Option<Self> {
        let mut s = RunState { t: r.var()?, phase: r.u8()?, level: r.var()? as u16, xp: r.var()?, next_xp: r.var()?, kills: r.var()?, menu: r.u8()?, paused_by: r.u8()?, chest: r.u8()? != 0, ..RunState::default() };
        for _ in 0..r.u8()? {
            let seat = r.u8()?;
            let mut choices = [(0, 0); 3];
            for c in &mut choices {
                *c = (r.u8()?, r.u8()?);
            }
            s.cards.push(SeatCards { seat, choices, selected: r.u8()?, picked: r.u8()? != 0, banishes: r.u8()?, rerolls: r.u8()?, favors: r.u8()? });
        }
        s.ticket = r.var()?;
        let books = r.u8()?;
        if books > 0 {
            s.played = r.var()?;
            s.books = (0..books).map(|_| BookState::read(r)).collect::<Option<Vec<_>>>()?;
        }
        Some(s)
    }
}

pub const Z_BURROWED: u8 = 1;
pub const Z_LEADER: u8 = 2;
pub const Z_BOSS: u8 = 4;
pub const Z_HURT: u8 = 8;
pub const Z_BURNING: u8 = 16;
pub const Z_RISING: u8 = 32;
pub const Z_SLOWED: u8 = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ZombieState {
    pub id: u32,
    /// Place in enemies.txt.
    pub kind: u8,
    pub pos: [u16; 2],
    /// Health left as a share of 255.
    pub hp: u8,
    pub flags: u8,
}

const ZD_POS: u8 = 1;
const ZD_HP: u8 = 2;
const ZD_FLAGS: u8 = 4;
const ZD_KIND: u8 = 8;

impl ZombieState {
    fn write_full(&self, w: &mut Writer) {
        w.u8(self.kind);
        w.u16(self.pos[0]);
        w.u16(self.pos[1]);
        w.u8(self.hp);
        w.u8(self.flags);
    }

    fn read_full(id: u32, r: &mut Reader) -> Option<Self> {
        Some(ZombieState { id, kind: r.u8()?, pos: [r.u16()?, r.u16()?], hp: r.u8()?, flags: r.u8()? })
    }

    /// Nothing when it hasn't changed from `was`.
    fn write_delta(&self, was: &ZombieState, w: &mut Writer) {
        let bit = |changed: bool, b: u8| if changed { b } else { 0 };
        let mask = bit(self.pos != was.pos, ZD_POS) | bit(self.hp != was.hp, ZD_HP) | bit(self.flags != was.flags, ZD_FLAGS) | bit(self.kind != was.kind, ZD_KIND);
        if mask == 0 {
            return;
        }
        w.u8(mask);
        if mask & ZD_POS != 0 {
            w.svar(self.pos[0] as i32 - was.pos[0] as i32);
            w.svar(self.pos[1] as i32 - was.pos[1] as i32);
        }
        if mask & ZD_HP != 0 {
            w.u8(self.hp);
        }
        if mask & ZD_FLAGS != 0 {
            w.u8(self.flags);
        }
        if mask & ZD_KIND != 0 {
            w.u8(self.kind);
        }
    }

    fn read_delta(was: &ZombieState, r: &mut Reader) -> Option<Self> {
        let mask = r.u8()?;
        let mut z = *was;
        if mask & ZD_POS != 0 {
            let step = |v: u16, d: i32| (v as i32 + d).clamp(0, u16::MAX as i32) as u16;
            z.pos = [step(z.pos[0], r.svar()?), step(z.pos[1], r.svar()?)];
        }
        if mask & ZD_HP != 0 {
            z.hp = r.u8()?;
        }
        if mask & ZD_FLAGS != 0 {
            z.flags = r.u8()?;
        }
        if mask & ZD_KIND != 0 {
            z.kind = r.u8()?;
        }
        Some(z)
    }
}

pub const T_GRENADE: u8 = 0;
pub const T_MONEY: u8 = 1;
pub const T_HEALTH: u8 = 2;
pub const T_CHEST: u8 = 3;
pub const T_PATCH: u8 = 4;
pub const T_BARREL: u8 = 5;
pub const T_DECOY: u8 = 6;
pub const T_TELL: u8 = 7;

/// Something that stays put a while: in the air, on the ground, or about to rise. The meaning of `data` is the
/// kind's: a throw's landing spot and the low 16 bits of its throw tick; a patch's radius, birth tick and life in
/// tenths of a second.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Thing {
    pub kind: u8,
    /// Which one of its kind: a drop's, patch's or throw's variety, a tell's zombie kind.
    pub sub: u8,
    pub pos: [u16; 2],
    pub data: [u16; 3],
}

impl Thing {
    fn write(&self, w: &mut Writer) {
        w.u8(self.kind);
        w.u8(self.sub);
        w.u16(self.pos[0]);
        w.u16(self.pos[1]);
        self.data.iter().for_each(|&d| w.var(d as u32));
    }

    fn read(r: &mut Reader) -> Option<Self> {
        Some(Thing { kind: r.u8()?, sub: r.u8()?, pos: [r.u16()?, r.u16()?], data: [r.var()? as u16, r.var()? as u16, r.var()? as u16] })
    }
}

/// A thing's key: its content, so an unchanged thing keeps it, and `nth` among identical ones.
pub fn thing_key(t: &Thing, nth: u32) -> u32 {
    let mut h: u32 = 0x811C_9DC5;
    let mut w = Writer::new();
    t.write(&mut w);
    w.var(nth);
    for b in w.buf {
        h = (h ^ b as u32).wrapping_mul(0x0100_0193);
    }
    h % OPEN
}

/// Something short-lived the client plays out itself from where and when it began.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Spawn {
    /// `look` is the firing weapon's place in the powerup book plus one, 0 for none; velocity in 1/8 px a second
    /// and life in 1/50 s.
    Shot { owner: u8, look: u16, from: [u16; 2], vel: [i16; 2], life: u8 },
    Spit { kind: u8, from: [u16; 2], vel: [i16; 2], life: u8 },
    Bolt { points: Vec<[u16; 2]> },
    /// An effect by its first frame's art.
    Fx { art: u16, pos: [u16; 2], angle: u8 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spawned {
    pub id: u32,
    pub tick: u32,
    pub what: Spawn,
}

impl Spawn {
    fn write(&self, w: &mut Writer) {
        let at = |w: &mut Writer, p: [u16; 2]| {
            w.u16(p[0]);
            w.u16(p[1]);
        };
        match self {
            Spawn::Shot { owner, look, from, vel, life } => {
                w.u8(0);
                w.u8(*owner);
                w.var(*look as u32);
                at(w, *from);
                w.i16(vel[0]);
                w.i16(vel[1]);
                w.u8(*life);
            }
            Spawn::Spit { kind, from, vel, life } => {
                w.u8(1);
                w.u8(*kind);
                at(w, *from);
                w.i16(vel[0]);
                w.i16(vel[1]);
                w.u8(*life);
            }
            Spawn::Bolt { points } => {
                w.u8(2);
                w.u8(points.len().min(255) as u8);
                points.iter().take(255).for_each(|&p| at(w, p));
            }
            Spawn::Fx { art, pos, angle } => {
                w.u8(3);
                w.var(*art as u32);
                at(w, *pos);
                w.u8(*angle);
            }
        }
    }

    fn read(r: &mut Reader) -> Option<Self> {
        Some(match r.u8()? {
            0 => Spawn::Shot { owner: r.u8()?, look: r.var()? as u16, from: [r.u16()?, r.u16()?], vel: [r.i16()?, r.i16()?], life: r.u8()? },
            1 => Spawn::Spit { kind: r.u8()?, from: [r.u16()?, r.u16()?], vel: [r.i16()?, r.i16()?], life: r.u8()? },
            2 => {
                let n = r.u8()?;
                Spawn::Bolt { points: (0..n).map(|_| Some([r.u16()?, r.u16()?])).collect::<Option<_>>()? }
            }
            3 => Spawn::Fx { art: r.var()? as u16, pos: [r.u16()?, r.u16()?], angle: r.u8()? },
            _ => return None,
        })
    }
}

/// Everything a client draws at one host tick: zombies by id, things by key, both ascending.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct World {
    pub tick: u32,
    pub run: RunState,
    pub players: Vec<PlayerState>,
    pub zombies: Vec<ZombieState>,
    pub things: Vec<(u32, Thing)>,
}

impl World {
    pub fn new(tick: u32, players: Vec<PlayerState>, mut zombies: Vec<ZombieState>, mut things: Vec<Thing>) -> Self {
        zombies.sort_by_key(|z| z.id);
        zombies.dedup_by_key(|z| z.id);
        things.sort();
        let mut keyed: Vec<(u32, Thing)> = Vec::with_capacity(things.len());
        let mut nth = 0;
        for (i, t) in things.iter().enumerate() {
            nth = if i > 0 && things[i - 1] == *t { nth + 1 } else { 0 };
            keyed.push((thing_key(t, nth), *t));
        }
        keyed.sort();
        for i in 1..keyed.len() {
            if keyed[i].0 <= keyed[i - 1].0 {
                keyed[i].0 = (keyed[i - 1].0 + 1) % OPEN;
            }
        }
        keyed.sort();
        Self { tick, run: RunState::default(), players, zombies, things: keyed }
    }

    pub fn zombie(&self, id: u32) -> Option<&ZombieState> {
        self.zombies.binary_search_by_key(&id, |z| z.id).ok().map(|i| &self.zombies[i])
    }

    fn has_thing(&self, key: u32) -> bool {
        self.things.binary_search_by_key(&key, |t| t.0).is_ok()
    }

    /// The whole world from every part of one snapshot.
    pub fn assemble(parts: &[Part]) -> Self {
        let mut parts: Vec<&Part> = parts.iter().collect();
        parts.sort_by_key(|p| p.index);
        World {
            tick: parts.first().map_or(0, |p| p.tick),
            run: parts.first().map_or_else(RunState::default, |p| p.run.clone()),
            players: parts.first().map_or_else(Vec::new, |p| p.players.clone()),
            zombies: parts.iter().flat_map(|p| p.zombies.iter().copied()).collect(),
            things: parts.iter().flat_map(|p| p.things.iter().copied()).collect(),
        }
    }
}

fn in_range(id: u32, (lo, hi): (u32, u32)) -> bool {
    id >= lo && (hi == OPEN || id < hi)
}

fn var_len(v: u32) -> usize {
    (32 - v.max(1).leading_zeros() as usize).div_ceil(7)
}

#[derive(Default)]
struct Group {
    zombies: Vec<(u32, Vec<u8>)>,
    zombies_gone: Vec<u32>,
    things: Vec<(u32, Vec<u8>)>,
    things_gone: Vec<u32>,
}

impl Group {
    fn is_empty(&self) -> bool {
        self.zombies.is_empty() && self.zombies_gone.is_empty() && self.things.is_empty() && self.things_gone.is_empty()
    }

    fn first_zombie(&self) -> Option<u32> {
        [self.zombies.first().map(|z| z.0), self.zombies_gone.first().copied()].into_iter().flatten().min()
    }

    fn first_thing(&self) -> Option<u32> {
        [self.things.first().map(|t| t.0), self.things_gone.first().copied()].into_iter().flatten().min()
    }
}

enum Item {
    Zombie(u32, Vec<u8>),
    ZombieGone(u32),
    Thing(u32, Vec<u8>),
    ThingGone(u32),
}

/// Header, ranges and counts at their largest.
const PART_OVERHEAD: usize = 40;

/// `world` as packets of at most `budget` bytes, coded against `base`, the latest snapshot the client acked, or
/// whole without one. The first part also carries the events still owed from `first_seq` on and the spawns.
pub fn encode(world: &World, base: Option<&World>, first_seq: u32, events: &[(u32, Event)], spawns: &[Spawned], budget: usize) -> Vec<Vec<u8>> {
    let mut players = Writer::new();
    world.run.write(&mut players);
    players.u8(world.players.len() as u8);
    world.players.iter().for_each(|p| p.write(&mut players));

    let mut extra = Writer::new();
    extra.var(first_seq);
    let mut evs = Writer::new();
    let mut n_ev = 0u32;
    let mut prev = first_seq;
    for &(seq, ev) in events.iter().filter(|e| e.0 >= first_seq) {
        let mut one = Writer::new();
        one.var(seq - prev);
        ev.write(&mut one);
        if evs.len() + one.len() > budget / 3 {
            break;
        }
        prev = seq;
        evs.bytes(&one.buf);
        n_ev += 1;
    }
    extra.var(n_ev);
    extra.bytes(&evs.buf);
    let mut sps = Writer::new();
    let mut n_sp = 0u32;
    let mut prev_id = 0;
    for s in spawns {
        let mut one = Writer::new();
        one.var(s.id.wrapping_sub(prev_id));
        one.var(world.tick.saturating_sub(s.tick));
        s.what.write(&mut one);
        if extra.len() + sps.len() + one.len() > budget * 2 / 3 {
            break;
        }
        prev_id = s.id;
        sps.bytes(&one.buf);
        n_sp += 1;
    }
    extra.var(n_sp);
    extra.bytes(&sps.buf);

    let mut items = Vec::new();
    let (mut b, empty) = (0, Vec::new());
    let old = base.map_or(&empty, |b| &b.zombies);
    for z in &world.zombies {
        while b < old.len() && old[b].id < z.id {
            items.push(Item::ZombieGone(old[b].id));
            b += 1;
        }
        let mut w = Writer::new();
        if b < old.len() && old[b].id == z.id {
            z.write_delta(&old[b], &mut w);
            b += 1;
        } else {
            z.write_full(&mut w);
        }
        if !w.is_empty() {
            items.push(Item::Zombie(z.id, w.buf));
        }
    }
    items.extend(old[b..].iter().map(|z| Item::ZombieGone(z.id)));
    let mut things: Vec<Item> = Vec::new();
    if let Some(base) = base {
        things.extend(base.things.iter().filter(|t| !world.has_thing(t.0)).map(|t| Item::ThingGone(t.0)));
    }
    for (key, t) in &world.things {
        if base.is_none_or(|b| !b.has_thing(*key)) {
            let mut w = Writer::new();
            t.write(&mut w);
            things.push(Item::Thing(*key, w.buf));
        }
    }
    things.sort_by_key(|i| match i {
        Item::Thing(k, _) | Item::ThingGone(k) => *k,
        _ => 0,
    });
    items.extend(things);

    let fixed = PART_OVERHEAD + players.len();
    let mut groups = vec![Group::default()];
    let mut used = fixed + extra.len();
    for item in items {
        let g = groups.last().unwrap();
        let gap = |list: &[u32]| list.last().copied().unwrap_or(0);
        let size = |g: &Group| match &item {
            Item::Zombie(id, b) => var_len(id - g.zombies.last().map_or(0, |z| z.0)) + b.len(),
            Item::ZombieGone(id) => var_len(id - gap(&g.zombies_gone)),
            Item::Thing(_, b) => 4 + b.len(),
            Item::ThingGone(_) => 4,
        };
        if used + size(g) > budget && !g.is_empty() {
            groups.push(Group::default());
            used = fixed;
        }
        let g = groups.last_mut().unwrap();
        used += size(g);
        match item {
            Item::Zombie(id, b) => g.zombies.push((id, b)),
            Item::ZombieGone(id) => g.zombies_gone.push(id),
            Item::Thing(k, b) => g.things.push((k, b)),
            Item::ThingGone(k) => g.things_gone.push(k),
        }
    }

    let count = groups.len();
    let mut z_his = vec![OPEN; count];
    let mut t_his = vec![OPEN; count];
    for i in (0..count.saturating_sub(1)).rev() {
        z_his[i] = groups[i + 1].first_zombie().unwrap_or(z_his[i + 1]);
        t_his[i] = groups[i + 1].first_thing().unwrap_or(t_his[i + 1]);
    }
    groups
        .iter()
        .enumerate()
        .map(|(i, g)| {
            let z = (if i == 0 { 0 } else { z_his[i - 1] }, z_his[i]);
            let t = (if i == 0 { 0 } else { t_his[i - 1] }, t_his[i]);
            let mut w = Writer::new();
            w.u8(VERSION);
            w.u8(base.is_some() as u8 | ((i == 0) as u8) << 1);
            w.var(world.tick);
            if let Some(b) = base {
                w.var(world.tick.saturating_sub(b.tick));
            }
            w.u8(i as u8);
            w.u8(count as u8);
            w.var(z.0);
            w.var(if z.1 == OPEN { 0 } else { z.1 - z.0 + 1 });
            w.u32(t.0);
            w.u32(t.1);
            w.bytes(&players.buf);
            if i == 0 {
                w.bytes(&extra.buf);
            }
            w.var(g.zombies.len() as u32);
            let mut prev = z.0;
            for (id, b) in &g.zombies {
                w.var(id - prev);
                w.bytes(b);
                prev = *id;
            }
            w.var(g.zombies_gone.len() as u32);
            let mut prev = z.0;
            for id in &g.zombies_gone {
                w.var(id - prev);
                prev = *id;
            }
            w.var(g.things_gone.len() as u32);
            g.things_gone.iter().for_each(|&k| w.u32(k));
            w.var(g.things.len() as u32);
            for (k, b) in &g.things {
                w.u32(*k);
                w.bytes(b);
            }
            w.buf
        })
        .collect()
}

/// One packet, decoded and laid over its baseline: every zombie and thing in its ranges as they stand at `tick`.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub tick: u32,
    pub base: Option<u32>,
    pub index: u8,
    pub count: u8,
    pub zombie_range: (u32, u32),
    pub thing_range: (u32, u32),
    pub run: RunState,
    pub players: Vec<PlayerState>,
    /// The host's oldest event still owed, and the events this part carries.
    pub events: Option<(u32, Vec<(u32, Event)>)>,
    pub spawns: Vec<Spawned>,
    pub zombies: Vec<ZombieState>,
    pub things: Vec<(u32, Thing)>,
}

impl Part {
    pub fn has_zombie_id(&self, id: u32) -> bool {
        in_range(id, self.zombie_range)
    }
}

/// A packet's tick and the baseline tick it needs, without decoding the rest.
pub fn peek(bytes: &[u8]) -> Option<(u32, Option<u32>)> {
    let mut r = Reader::new(bytes);
    if r.u8()? != VERSION {
        return None;
    }
    let flags = r.u8()?;
    let tick = r.var()?;
    let base = if flags & 1 != 0 { Some(tick.checked_sub(r.var()?)?) } else { None };
    Some((tick, base))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    Short,
    Version,
    /// Coded against a snapshot the client doesn't have.
    NoBase,
}

/// Decodes a packet against `base`, which must be the snapshot it names.
pub fn decode(bytes: &[u8], base: Option<&World>) -> Result<Part, DecodeError> {
    let (_, want) = peek(bytes).ok_or(if bytes.first() == Some(&VERSION) { DecodeError::Short } else { DecodeError::Version })?;
    let base = match want {
        Some(t) => Some(base.filter(|b| b.tick == t).ok_or(DecodeError::NoBase)?),
        None => None,
    };
    read_part(bytes, base).ok_or(DecodeError::Short)
}

fn read_part(bytes: &[u8], base: Option<&World>) -> Option<Part> {
    let mut r = Reader::new(bytes);
    r.u8()?;
    let flags = r.u8()?;
    let tick = r.var()?;
    let base_tick = if flags & 1 != 0 { Some(tick - r.var()?) } else { None };
    let (index, count) = (r.u8()?, r.u8()?);
    let z_lo = r.var()?;
    let z_hi = match r.var()? {
        0 => OPEN,
        n => z_lo.checked_add(n - 1)?,
    };
    let (zr, tr) = ((z_lo, z_hi), (r.u32()?, r.u32()?));
    let run = RunState::read(&mut r)?;
    let players = (0..r.u8()?).map(|_| PlayerState::read(&mut r)).collect::<Option<Vec<_>>>()?;
    let (mut events, mut spawns) = (None, Vec::new());
    if flags & 2 != 0 {
        let first = r.var()?;
        let mut seq = first;
        let mut list = Vec::new();
        for _ in 0..r.var()? {
            seq = seq.checked_add(r.var()?)?;
            list.push((seq, Event::read(&mut r)?));
        }
        events = Some((first, list));
        let mut id = 0u32;
        for _ in 0..r.var()? {
            id = id.wrapping_add(r.var()?);
            let born = tick.checked_sub(r.var()?)?;
            spawns.push(Spawned { id, tick: born, what: Spawn::read(&mut r)? });
        }
    }
    let empty = World::default();
    let base_w = base.unwrap_or(&empty);
    let mut changed = Vec::new();
    let mut id = zr.0;
    for _ in 0..r.var()? {
        id = id.checked_add(r.var()?)?;
        changed.push(match base_w.zombie(id) {
            Some(was) => ZombieState::read_delta(was, &mut r)?,
            None => ZombieState::read_full(id, &mut r)?,
        });
    }
    let mut gone = Vec::new();
    let mut id = zr.0;
    for _ in 0..r.var()? {
        id = id.checked_add(r.var()?)?;
        gone.push(id);
    }
    let mut things_gone = (0..r.var()?).map(|_| r.u32()).collect::<Option<Vec<_>>>()?;
    things_gone.sort_unstable();
    let added = (0..r.var()?).map(|_| Some((r.u32()?, Thing::read(&mut r)?))).collect::<Option<Vec<_>>>()?;

    let mut zombies: Vec<ZombieState> = base_w.zombies.iter().filter(|z| in_range(z.id, zr) && gone.binary_search(&z.id).is_err()).copied().collect();
    for z in changed.into_iter().filter(|z| in_range(z.id, zr)) {
        match zombies.binary_search_by_key(&z.id, |o| o.id) {
            Ok(i) => zombies[i] = z,
            Err(i) => zombies.insert(i, z),
        }
    }
    let mut things: Vec<(u32, Thing)> = base_w.things.iter().filter(|t| in_range(t.0, tr) && things_gone.binary_search(&t.0).is_err()).copied().collect();
    things.extend(added.into_iter().filter(|t| in_range(t.0, tr)));
    things.sort();
    things.dedup_by_key(|t| t.0);
    Some(Part { tick, base: base_tick, index, count, zombie_range: zr, thing_range: tr, run, players, events, spawns, zombies, things })
}
