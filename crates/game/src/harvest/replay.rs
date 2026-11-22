//! Run replays: how a night was set up, each tick's input that changed, and checksums of where everything stood, so
//! the night plays again exactly and says the first tick it stopped matching.

use super::player::PlayerId;
use super::powerups::Progress;
use super::sandbox::{Bot, Walk, build_text, parse_build};
use super::weapons::Kit;
use super::{Choice, Harvest, Phase};
use crate::{Difficulty, Input, MAX_TOUCHES, Sfx};

pub const FORMAT: u32 = 3;
const MAGIC: &[u8; 4] = b"LHRP";
/// Ticks between full checkpoints; every tick also keeps one byte of its checksum.
pub const CHECK_EVERY: u64 = 60;

/// Where a night came from, which decides how it is built.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// A night on this device: one player or split-screen, with scripted players past the seated ones.
    Night,
    /// An online night this device hosts: each seat's card, and how many seats play here.
    Host { looks: Vec<(usize, String)>, local: usize },
    /// A sandbox night: a generated farm of this size, every player carrying the forced build.
    Sandbox { size: [usize; 2], build: Vec<(usize, u32)>, mortal: bool },
}

/// Everything a night is built from, so a replay builds the same one.
#[derive(Clone, Debug, PartialEq)]
pub struct Setup {
    pub seed: u64,
    /// A hand-made map's id; empty for a generated farm.
    pub map: String,
    pub difficulty: Difficulty,
    pub progress: Progress,
    pub enemies_off: u64,
    pub look: String,
    /// The player card whose farm the night is on; none leaves the farm in use alone.
    pub farm: Option<String>,
    pub kit: String,
    pub start_t: f32,
    pub one_screen: bool,
    pub endless: bool,
    /// Seated players, then scripted ones past them, and how many have a view.
    pub players: usize,
    pub bots: usize,
    pub views: usize,
    /// How the scripted players walk, turn round and throw.
    pub walk: Walk,
    pub turn: f32,
    pub turn_every: f32,
    pub throw_every: f32,
    pub kind: Kind,
}

impl Setup {
    /// A night on this device with the game's own scripted players.
    pub fn night(seed: u64) -> Self {
        Self { seed, map: String::new(), difficulty: Difficulty::NORMAL, progress: Progress::default(), enemies_off: 0, look: String::new(), farm: None, kit: String::new(), start_t: 0.0, one_screen: false, endless: false, players: 1, bots: 0, views: 1, walk: Walk::Avoid, turn: 0.25, turn_every: 0.0, throw_every: 12.0, kind: Kind::Night }
    }

    /// The night, after setting the farm it plays on.
    pub fn build(&self) -> Harvest {
        if let Some(look) = &self.farm {
            super::use_farm(None);
            super::use_player_farm(look);
        }
        self.harvest()
    }

    /// The night, on whatever farm is in use.
    pub(super) fn harvest(&self) -> Harvest {
        let d = self.difficulty;
        let mut h = match &self.kind {
            Kind::Night => {
                let mut h = Harvest::on(self.seed, &self.map).at(d).with_progress(self.progress).without_enemies(self.enemies_off).looking_like(&self.look).with_kit(&self.kit).starting_at(self.start_t).single_screen(self.one_screen).endless_grenades(self.endless).with_players(self.players).with_bots(self.bots).with_views(self.views);
                let first = self.players.min(h.players.len());
                h.bots = (first..h.players.len()).map(|p| (p, self.bot())).collect();
                h
            }
            Kind::Host { looks, local } => Harvest::on(self.seed, &self.map).at(d).single_screen(self.one_screen).with_players(self.players).with_progress(self.progress).dressed(looks).hosting(*local),
            Kind::Sandbox { size, build, mortal } => {
                let mut h = Harvest::sized(self.seed, *size).at(d);
                if self.players > 1 {
                    h = h.with_players(self.players).with_views(1);
                    h.bots = (1..h.players.len()).map(|p| (p, self.bot())).collect();
                }
                for pl in &mut h.players {
                    pl.kit = Kit::forced(build, d.tier());
                    pl.hp = pl.kit.max_hp();
                    pl.grenades = pl.kit.grenades();
                }
                h.undying = !mortal;
                h.progress.everything = self.progress.everything;
                h
            }
        };
        h.setup = Some(Box::new(self.clone()));
        h
    }

    fn bot(&self) -> Bot {
        Bot::new(self.walk, self.turn, self.turn_every, self.throw_every)
    }

    /// One line each, as `from_text` reads them.
    pub fn to_text(&self) -> String {
        let p = &self.progress;
        let f = |v: f32| format!("{:08x}", v.to_bits());
        let mut out = format!("seed {}
map {}
difficulty {}
", self.seed, self.map, self.difficulty.key());
        out += &format!("progress {} {} {} {} {} {} {} {:x} {} {:x}
", p.kills, p.best, p.best_level, p.nights, p.dawns, p.breakers, p.breaker_dawns, p.found, p.everything as u8, p.hidden);
        out += &format!("enemies_off {:x}
look {}
", self.enemies_off, self.look);
        if let Some(farm) = &self.farm {
            out += &format!("farm {farm}
");
        }
        out += &format!("kit {}
start_t {}
one_screen {}
endless {}
", self.kit, f(self.start_t), self.one_screen as u8, self.endless as u8);
        out += &format!("players {}
bots {}
views {}
", self.players, self.bots, self.views);
        out += &format!("bot {} {} {} {}
", self.walk.name(), f(self.turn), f(self.turn_every), f(self.throw_every));
        out += &match &self.kind {
            Kind::Night => "kind night
".to_string(),
            Kind::Host { looks, local } => format!("kind host {local} {}
", looks.iter().map(|(s, c)| format!("{s}={c}")).collect::<Vec<_>>().join(";")),
            Kind::Sandbox { size, build, mortal } => format!("kind sandbox {}x{} {} {}
", size[0], size[1], *mortal as u8, build_text(build)),
        };
        out
    }

    pub fn from_text(text: &str) -> Result<Self, String> {
        let mut s = Self::night(1);
        let bad = |line: &str| format!("replay setup: can't read {line:?}");
        let num = |v: &str, line: &str| v.parse::<u64>().map_err(|_| bad(line));
        let hex = |v: &str, line: &str| u64::from_str_radix(v, 16).map_err(|_| bad(line));
        let f = |v: &str, line: &str| u32::from_str_radix(v, 16).map(f32::from_bits).map_err(|_| bad(line));
        for line in text.lines().filter(|l| !l.is_empty()) {
            let (key, rest) = line.split_once(' ').unwrap_or((line, ""));
            let w: Vec<&str> = rest.split(' ').collect();
            match key {
                "seed" => s.seed = num(rest, line)?,
                "map" => s.map = rest.to_string(),
                "difficulty" => s.difficulty = Difficulty::from_key(rest).ok_or_else(|| bad(line))?,
                "progress" if w.len() == 10 => {
                    let n = |i: usize| num(w[i], line).map(|v| v as u32);
                    s.progress = Progress { kills: n(0)?, best: n(1)?, best_level: n(2)?, nights: n(3)?, dawns: n(4)?, breakers: n(5)?, breaker_dawns: n(6)?, found: hex(w[7], line)?, everything: w[8] == "1", hidden: hex(w[9], line)? };
                }
                "enemies_off" => s.enemies_off = hex(rest, line)?,
                "look" => s.look = rest.to_string(),
                "farm" => s.farm = Some(rest.to_string()),
                "kit" => s.kit = rest.to_string(),
                "start_t" => s.start_t = f(rest, line)?,
                "one_screen" => s.one_screen = rest == "1",
                "endless" => s.endless = rest == "1",
                "players" => s.players = num(rest, line)? as usize,
                "bots" => s.bots = num(rest, line)? as usize,
                "views" => s.views = num(rest, line)? as usize,
                "bot" if w.len() == 4 => {
                    s.walk = Walk::from_name(w[0]).ok_or_else(|| bad(line))?;
                    (s.turn, s.turn_every, s.throw_every) = (f(w[1], line)?, f(w[2], line)?, f(w[3], line)?);
                }
                "kind" => {
                    s.kind = match w.as_slice() {
                        ["night"] => Kind::Night,
                        ["host", local, looks] => Kind::Host {
                            local: num(local, line)? as usize,
                            looks: looks.split(';').filter(|l| !l.is_empty()).map(|l| l.split_once('=').and_then(|(seat, card)| Some((seat.parse().ok()?, card.to_string()))).ok_or_else(|| bad(line))).collect::<Result<_, _>>()?,
                        },
                        ["sandbox", size, mortal, build @ ..] => {
                            let (w, h) = size.split_once('x').ok_or_else(|| bad(line))?;
                            Kind::Sandbox { size: [num(w, line)? as usize, num(h, line)? as usize], mortal: *mortal == "1", build: parse_build(&build.join(" "))? }
                        }
                        _ => return Err(bad(line)),
                    }
                }
                _ => return Err(bad(line)),
            }
        }
        Ok(s)
    }
}

/// What a sandbox's scripted player does to the night between ticks, where the game's player only presses buttons.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Op {
    Throw { p: PlayerId, to: [f32; 2] },
    /// The level-up's cards on show become the ones the ops below work on.
    Begin,
    Reroll,
    Banish(usize),
    Favor(usize),
    Take(usize),
    Relabel(PlayerId),
    /// Deals a level-up's cards afresh.
    Deal,
    /// Back to play from a level-up.
    Resume,
}

/// A setting the game sets on a night between ticks, recorded only when it changes something.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Set {
    OneScreen(bool),
    TouchAim(bool),
    TouchKeys(bool),
    SavesPhotos(bool),
    Taps(bool),
    Canvas([f32; 2]),
    Guides,
    CornerMap(bool),
}

impl Harvest {
    /// Does `op`, keeping it in the replay when it comes from outside a tick.
    pub(super) fn op(&mut self, op: Op, sounds: &mut Vec<Sfx>) {
        if !self.updating {
            self.taping(|r| r.op(op));
        }
        match op {
            Op::Throw { p, to } => self.throw_by(p, to, sounds),
            Op::Begin => {
                if let Phase::LevelUp { choices, t, .. } = self.phase {
                    self.picking = (choices, t);
                }
            }
            Op::Reroll => {
                let (mut c, t) = self.picking;
                self.reroll(&mut c, t, sounds);
                self.picking.0 = c;
            }
            Op::Banish(i) => {
                let (mut c, t) = self.picking;
                self.banish(&mut c, i, t, sounds);
                self.picking.0 = c;
            }
            Op::Favor(up) => {
                self.favor(up);
            }
            Op::Take(i) => self.take(self.picking.0[i], sounds),
            Op::Relabel(p) => self.relabel(p),
            Op::Deal => self.phase = Phase::LevelUp { choices: self.roll_choices(&[]), selected: 0, t: 0.0, picked: None },
            Op::Resume => {
                self.phase = Phase::Playing;
                self.chest = false;
            }
        }
    }

    /// The level-up cards the ops work on.
    pub(super) fn picking(&self) -> [Choice; 3] {
        self.picking.0
    }

    /// Keeps a setting's change in the replay; the setter calls it before it changes anything.
    pub(super) fn tape_set(&mut self, set: Set) {
        let changes = match set {
            Set::OneScreen(on) => self.single != on || (!on && (self.aiming.is_some() || self.glance)),
            Set::TouchAim(on) => self.touch_aim != on || (self.touch_map && !on),
            Set::TouchKeys(on) => self.touch_keys != on,
            Set::SavesPhotos(on) => self.saves_photos != on,
            Set::Taps(on) => self.taps != on,
            Set::Canvas(_) => false,
            Set::Guides => self.photo.is_some(),
            Set::CornerMap(on) => self.corner_map != on,
        };
        if changes {
            self.taping(|r| r.set(set));
        }
    }

    fn taping(&mut self, f: impl FnOnce(&mut Recorder)) {
        if let Some(r) = self.tape.as_mut().and_then(|t| t.rec.as_mut()) {
            f(r);
        }
    }

    /// Records this night and each one it restarts into, each headed by `info`; a night with no setup isn't recorded.
    pub fn record(&mut self, info: Vec<(String, String)>) {
        let Some(setup) = self.setup.as_deref() else { return };
        let rec = Recorder::new(&info, setup);
        self.tape = Some(Box::new(Tape { info, rec: Some(rec), done: Vec::new(), restarted: false }));
    }

    pub fn take_replays(&mut self) -> Vec<Vec<u8>> {
        self.tape.as_mut().map_or_else(Vec::new, |t| std::mem::take(&mut t.done))
    }

    /// Ends the recording where it stands, as when the night is left before its end, and stops recording.
    pub fn stop_recording(&mut self) -> Vec<Vec<u8>> {
        let Some(mut t) = self.tape.take() else { return Vec::new() };
        t.finish();
        t.done
    }

    /// Before a tick: the input it plays.
    pub(super) fn tape_before(&mut self, input: &Input, dt: f32) {
        if let Some(r) = self.tape.as_mut().and_then(|t| t.rec.as_mut()) {
            if r.canvas != Some(self.canvas.map(f32::to_bits)) {
                r.canvas = Some(self.canvas.map(f32::to_bits));
                r.set(Set::Canvas(self.canvas));
            }
            r.before(input, &self.inputs, dt);
        }
    }

    /// After a tick: its checksum, and the recording's end with the night's.
    pub(super) fn tape_after(&mut self, ended: bool) {
        let Some(t) = self.tape.as_deref_mut() else { return };
        if std::mem::take(&mut t.restarted) {
            if let Some(setup) = self.setup.as_deref() {
                t.rec = Some(Recorder::new(&t.info, setup));
            }
            return;
        }
        let Some(r) = t.rec.as_ref() else { return };
        let tick = r.tick;
        let sum = checksum(self);
        let check = ((tick + 1).is_multiple_of(CHECK_EVERY) || ended).then(|| Check::of(self, sum));
        let t = self.tape.as_mut().unwrap();
        let r = t.rec.as_mut().unwrap();
        r.after(sum, check);
        if ended {
            t.finish();
        }
    }

    /// A restart ends this night's recording, and the next tick starts the new night's.
    pub(super) fn tape_restart(&mut self) {
        if let Some(t) = self.tape.as_mut() {
            t.finish();
            t.restarted = true;
        }
    }
}

/// A night's recording and the replays it has finished.
pub(super) struct Tape {
    info: Vec<(String, String)>,
    rec: Option<Recorder>,
    done: Vec<Vec<u8>>,
    /// The night restarted mid-tick; the next tick records the new one.
    restarted: bool,
}

impl Tape {
    fn finish(&mut self) {
        if let Some(r) = self.rec.take() {
            self.done.push(r.finish());
        }
    }
}

const END: u8 = 0;
const INPUT: u8 = 1;
const DT: u8 = 2;
const SET: u8 = 3;
const CHECK: u8 = 4;
const SEAT: u8 = 5;
const UNSEAT: u8 = 6;
const OP: u8 = 7;

struct Recorder {
    bytes: Vec<u8>,
    tick: u64,
    /// The tick of the last event written.
    at: u64,
    /// The input last written for each seat: the local player's, then each in `Harvest::inputs`.
    seats: Vec<Input>,
    dt: Option<u32>,
    /// Views each set their own canvas as they draw, so only the one a tick sees is recorded.
    canvas: Option<[u32; 2]>,
    sums: Vec<u8>,
}

impl Recorder {
    fn new(info: &[(String, String)], setup: &Setup) -> Self {
        let mut head = String::new();
        for (k, v) in info {
            head += &format!("info {k} {v}
");
        }
        head += &setup.to_text();
        let mut bytes = MAGIC.to_vec();
        bytes.extend(FORMAT.to_le_bytes());
        varint(&mut bytes, head.len() as u64);
        bytes.extend(head.as_bytes());
        Self { bytes, tick: 0, at: 0, seats: Vec::new(), dt: None, canvas: None, sums: Vec::new() }
    }

    fn event(&mut self, tag: u8) -> &mut Vec<u8> {
        varint(&mut self.bytes, self.tick - self.at);
        self.at = self.tick;
        self.bytes.push(tag);
        &mut self.bytes
    }

    fn before(&mut self, input: &Input, others: &[Input], dt: f32) {
        if self.dt != Some(dt.to_bits()) {
            self.dt = Some(dt.to_bits());
            self.event(DT).extend(dt.to_bits().to_le_bytes());
        }
        for (seat, now) in std::iter::once(input).chain(others).enumerate() {
            let was = self.seats.get(seat).copied().unwrap_or_default();
            let mut diff = Vec::new();
            let mask = write_input(&mut diff, &was, now);
            if mask != 0 || seat >= self.seats.len() {
                let out = self.event(INPUT);
                out.push(seat as u8);
                out.extend(mask.to_le_bytes());
                out.extend(diff);
            }
            if seat >= self.seats.len() {
                self.seats.push(*now);
            } else {
                self.seats[seat] = *now;
            }
        }
    }

    fn after(&mut self, sum: u64, check: Option<Check>) {
        self.sums.push(sum as u8);
        if let Some(c) = check {
            let out = self.event(CHECK);
            c.write(out);
        }
        self.tick += 1;
    }

    fn op(&mut self, op: Op) {
        let out = self.event(OP);
        match op {
            Op::Throw { p, to } => {
                out.extend([0, p as u8]);
                out.extend(to[0].to_bits().to_le_bytes());
                out.extend(to[1].to_bits().to_le_bytes());
            }
            Op::Begin => out.push(1),
            Op::Reroll => out.push(2),
            Op::Banish(i) => out.extend([3, i as u8]),
            Op::Favor(up) => out.extend([4, up as u8]),
            Op::Take(i) => out.extend([5, i as u8]),
            Op::Relabel(p) => out.extend([6, p as u8]),
            Op::Deal => out.push(7),
            Op::Resume => out.push(8),
        }
    }

    fn set(&mut self, set: Set) {
        let out = self.event(SET);
        match set {
            Set::OneScreen(on) => out.extend([0, on as u8]),
            Set::TouchAim(on) => out.extend([1, on as u8]),
            Set::TouchKeys(on) => out.extend([2, on as u8]),
            Set::SavesPhotos(on) => out.extend([3, on as u8]),
            Set::Taps(on) => out.extend([4, on as u8]),
            Set::Canvas(size) => {
                out.push(5);
                out.extend(size[0].to_bits().to_le_bytes());
                out.extend(size[1].to_bits().to_le_bytes());
            }
            Set::Guides => out.push(6),
            Set::CornerMap(on) => out.extend([7, on as u8]),
        }
    }

    pub(super) fn seat(&mut self, p: PlayerId, look: &str) {
        let out = self.event(SEAT);
        out.push(p as u8);
        varint(out, look.len() as u64);
        out.extend(look.as_bytes());
    }

    pub(super) fn unseat(&mut self, p: PlayerId) {
        self.event(UNSEAT).push(p as u8);
    }

    /// The whole replay, with every tick's checksum byte after its events.
    fn finish(mut self) -> Vec<u8> {
        self.event(END);
        let ticks = self.tick;
        varint(&mut self.bytes, ticks);
        self.bytes.extend(&self.sums);
        self.bytes
    }
}

impl Harvest {
    pub(super) fn tape_seat(&mut self, p: PlayerId, look: &str) {
        self.taping(|r| r.seat(p, look));
    }

    pub(super) fn tape_unseat(&mut self, p: PlayerId) {
        self.taping(|r| r.unseat(p));
    }
}

fn varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push(v as u8 | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn put_f32s(out: &mut Vec<u8>, v: &[f32]) {
    for x in v {
        out.extend(x.to_bits().to_le_bytes());
    }
}

/// Writes the fields of `now` that differ from `was`, bit for bit, and returns which.
fn write_input(out: &mut Vec<u8>, was: &Input, now: &Input) -> u16 {
    let bits = |v: &[f32]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
    let spot = |t: &Option<[f32; 2]>| t.map(|p| bits(&p));
    let mut mask = 0;
    if was.held != now.held {
        mask |= 1;
        out.extend(now.held.0.to_le_bytes());
    }
    if was.pressed != now.pressed {
        mask |= 2;
        out.extend(now.pressed.0.to_le_bytes());
    }
    if bits(&was.stick) != bits(&now.stick) {
        mask |= 4;
        put_f32s(out, &now.stick);
    }
    if was.look.to_bits() != now.look.to_bits() {
        mask |= 8;
        put_f32s(out, &[now.look]);
    }
    if spot(&was.touch) != spot(&now.touch) {
        mask |= 16;
        out.push(now.touch.is_some() as u8);
        put_f32s(out, &now.touch.unwrap_or_default());
    }
    if was.touches.iter().map(spot).ne(now.touches.iter().map(spot)) {
        mask |= 32;
        let some = now.touches.iter().enumerate().fold(0u16, |m, (i, t)| m | (t.is_some() as u16) << i);
        out.extend(some.to_le_bytes());
        for t in now.touches.iter().flatten() {
            put_f32s(out, t);
        }
    }
    if was.touch_started != now.touch_started {
        mask |= 64;
    }
    if was.lid_closed != now.lid_closed {
        mask |= 128;
    }
    if bits(&was.orbit) != bits(&now.orbit) {
        mask |= 256;
        put_f32s(out, &now.orbit);
    }
    mask
}

/// A checkpoint: the whole checksum, the zombies standing and where each player stood.
#[derive(Clone, Debug, PartialEq)]
pub struct Check {
    pub sum: u64,
    pub zombies: u32,
    pub players: Vec<[f32; 2]>,
}

impl Check {
    fn of(h: &Harvest, sum: u64) -> Self {
        Self { sum, zombies: (h.zombies.len() + h.burrowed.len()) as u32, players: h.players.iter().map(|p| p.pos).collect() }
    }

    fn write(&self, out: &mut Vec<u8>) {
        out.extend(self.sum.to_le_bytes());
        varint(out, self.zombies as u64);
        out.push(self.players.len() as u8);
        for p in &self.players {
            put_f32s(out, p);
        }
    }
}

/// FNV-1a over the night's clock, generators, every player and every zombie's place and health, and what's in flight.
pub(super) fn checksum(h: &Harvest) -> u64 {
    let mut s = Fnv(0xcbf2_9ce4_8422_2325);
    s.f(h.t);
    s.u(h.rng.state());
    s.u(h.juice.rng.state());
    s.u(h.kills as u64);
    s.u(h.level as u64);
    s.u(h.xp as u64);
    s.u(h.next_id as u64);
    s.u(match h.phase {
        Phase::Playing => 0,
        Phase::LevelUp { .. } => 1,
        Phase::Over { .. } => 2,
    });
    for pl in &h.players {
        pl.pos.iter().chain(&[pl.hp, pl.view_yaw, pl.grenades]).for_each(|&v| s.f(v));
        for d in &pl.dogs {
            d.pos.iter().for_each(|&v| s.f(v));
        }
    }
    s.u((h.zombies.len() + h.burrowed.len()) as u64);
    for z in h.zombies.iter().chain(&h.burrowed) {
        s.u(z.id as u64);
        [z.pos[0], z.pos[1], z.hp].iter().for_each(|&v| s.f(v));
    }
    for n in [h.thrown.len(), h.bullets.len(), h.drops.len(), h.spits.len()] {
        s.u(n as u64);
    }
    s.0
}

struct Fnv(u64);

impl Fnv {
    fn u(&mut self, v: u64) {
        for b in v.to_le_bytes() {
            self.0 = (self.0 ^ b as u64).wrapping_mul(0x0100_0000_01b3);
        }
    }

    fn f(&mut self, v: f32) {
        self.u(v.to_bits() as u64);
    }
}

/// A 64-bit FNV-1a of `bytes`, for the data a night was played on.
pub fn fingerprint<'a>(parts: impl IntoIterator<Item = &'a [u8]>) -> u64 {
    let mut s = Fnv(0xcbf2_9ce4_8422_2325);
    for part in parts {
        s.u(part.len() as u64);
        for &b in part {
            s.0 = (s.0 ^ b as u64).wrapping_mul(0x0100_0000_01b3);
        }
    }
    s.0
}

#[derive(Clone, Debug)]
enum Event {
    Input(usize, Input),
    Dt(f32),
    Set(Set),
    Check(Check),
    Seat(usize, String),
    Unseat(usize),
    Op(Op),
}

/// A replay read back: its header, its events by tick and each tick's checksum byte.
#[derive(Clone, Debug)]
pub struct Replay {
    /// What the recording device said about itself: the game's version, the data's fingerprint and so on.
    pub info: Vec<(String, String)>,
    pub setup: Setup,
    events: Vec<(u64, Event)>,
    /// Ticks recorded.
    pub ticks: u64,
    sums: Vec<u8>,
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let out = self.bytes.get(self.at..self.at + n).ok_or("replay: cut short")?;
        self.at += n;
        Ok(out)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn f32(&mut self) -> Result<f32, String> {
        self.u32().map(f32::from_bits)
    }

    fn pair(&mut self) -> Result<[f32; 2], String> {
        Ok([self.f32()?, self.f32()?])
    }

    fn varint(&mut self) -> Result<u64, String> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let b = self.u8()?;
            v |= ((b & 0x7f) as u64) << shift;
            if b < 0x80 {
                return Ok(v);
            }
        }
        Err("replay: a number runs on".into())
    }

    fn input(&mut self, was: Input) -> Result<Input, String> {
        let mask = self.u16()?;
        let mut i = was;
        if mask & 1 != 0 {
            i.held = crate::Buttons(self.u32()?);
        }
        if mask & 2 != 0 {
            i.pressed = crate::Buttons(self.u32()?);
        }
        if mask & 4 != 0 {
            i.stick = self.pair()?;
        }
        if mask & 8 != 0 {
            i.look = self.f32()?;
        }
        if mask & 16 != 0 {
            let some = self.u8()? != 0;
            let at = self.pair()?;
            i.touch = some.then_some(at);
        }
        if mask & 32 != 0 {
            let some = self.u16()?;
            for t in 0..MAX_TOUCHES {
                i.touches[t] = if some & 1 << t != 0 { Some(self.pair()?) } else { None };
            }
        }
        i.touch_started ^= mask & 64 != 0;
        i.lid_closed ^= mask & 128 != 0;
        if mask & 256 != 0 {
            i.orbit = self.pair()?;
        }
        Ok(i)
    }
}

impl Replay {
    /// Reads a replay's bytes, after any compression is undone.
    pub fn read(bytes: &[u8]) -> Result<Self, String> {
        if bytes.get(..4) != Some(MAGIC.as_slice()) {
            return Err("not a Last Harvest replay".into());
        }
        let mut r = Reader { bytes, at: 4 };
        let format = r.u32()?;
        if format < FORMAT {
            return Err(format!("replay format {format}, from an older build whose math differs; this build reads {FORMAT}"));
        }
        if format != FORMAT {
            return Err(format!("replay format {format}; this build reads {FORMAT}"));
        }
        let len = r.varint()? as usize;
        let head = std::str::from_utf8(r.take(len)?).map_err(|_| "replay: the header isn't UTF-8")?;
        let mut info = Vec::new();
        let mut setup = String::new();
        for line in head.lines() {
            match line.strip_prefix("info ") {
                Some(kv) => {
                    let (k, v) = kv.split_once(' ').unwrap_or((kv, ""));
                    info.push((k.to_string(), v.to_string()));
                }
                None => setup += &format!("{line}
"),
            }
        }
        let setup = Setup::from_text(&setup)?;
        let (mut events, mut tick, mut seats) = (Vec::new(), 0u64, Vec::<Input>::new());
        loop {
            tick += r.varint()?;
            let e = match r.u8()? {
                END => break,
                INPUT => {
                    let seat = r.u8()? as usize;
                    if seats.len() <= seat {
                        seats.resize(seat + 1, Input::default());
                    }
                    seats[seat] = r.input(seats[seat])?;
                    Event::Input(seat, seats[seat])
                }
                DT => Event::Dt(r.f32()?),
                SET => Event::Set(match r.u8()? {
                    0 => Set::OneScreen(r.u8()? != 0),
                    1 => Set::TouchAim(r.u8()? != 0),
                    2 => Set::TouchKeys(r.u8()? != 0),
                    3 => Set::SavesPhotos(r.u8()? != 0),
                    4 => Set::Taps(r.u8()? != 0),
                    5 => Set::Canvas(r.pair()?),
                    6 => Set::Guides,
                    7 => Set::CornerMap(r.u8()? != 0),
                    n => return Err(format!("replay: unknown setting {n}")),
                }),
                CHECK => {
                    let sum = u64::from_le_bytes(r.take(8)?.try_into().unwrap());
                    let zombies = r.varint()? as u32;
                    let n = r.u8()? as usize;
                    Event::Check(Check { sum, zombies, players: (0..n).map(|_| r.pair()).collect::<Result<_, _>>()? })
                }
                SEAT => {
                    let p = r.u8()? as usize;
                    let len = r.varint()? as usize;
                    Event::Seat(p, String::from_utf8_lossy(r.take(len)?).into_owned())
                }
                UNSEAT => Event::Unseat(r.u8()? as usize),
                OP => Event::Op(match r.u8()? {
                    0 => Op::Throw { p: r.u8()? as usize, to: r.pair()? },
                    1 => Op::Begin,
                    2 => Op::Reroll,
                    3 => Op::Banish(r.u8()? as usize),
                    4 => Op::Favor(r.u8()? as usize),
                    5 => Op::Take(r.u8()? as usize),
                    6 => Op::Relabel(r.u8()? as usize),
                    7 => Op::Deal,
                    8 => Op::Resume,
                    n => return Err(format!("replay: unknown op {n}")),
                }),
                n => return Err(format!("replay: unknown event {n}")),
            };
            events.push((tick, e));
        }
        let ticks = r.varint()?;
        let sums = r.take(ticks as usize)?.to_vec();
        Ok(Self { info, setup, events, ticks, sums })
    }

    /// The header's `key`, if it has one.
    pub fn info(&self, key: &str) -> Option<&str> {
        self.info.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    /// The checkpoints, by the tick after which each was taken.
    pub fn checks(&self) -> impl Iterator<Item = (u64, &Check)> {
        self.events.iter().filter_map(|(t, e)| match e {
            Event::Check(c) => Some((*t, c)),
            _ => None,
        })
    }

    #[cfg(test)]
    fn nudge_input(&mut self, from: u64) -> u64 {
        let (t, e) = self.events.iter_mut().find(|(t, e)| *t >= from && matches!(e, Event::Input(0, _))).expect("an input to nudge");
        if let Event::Input(_, i) = e {
            i.look = if i.look == 0.0 { 0.5 } else { -i.look };
        }
        *t
    }
}

/// Watches a replay as it plays, counting what the checksums can't say.
pub trait Watch {
    /// After each tick.
    fn tick(&mut self, h: &Harvest, dt: f32);
    /// What it counted, a line each.
    fn report(&self, secs: f32) -> Vec<String>;
}

/// Where a replay first stopped matching its recording.
#[derive(Clone, Debug, PartialEq)]
pub struct Drift {
    /// The first tick whose checksum differs, from 0, and seconds into the night after it.
    pub tick: u64,
    pub secs: f32,
    /// The first checkpoint that differs, as recorded and as played.
    pub check: Option<(u64, Check, Check)>,
}

/// How a replay played.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub ticks: u64,
    pub secs: f32,
    pub checks: usize,
    pub drift: Option<Drift>,
    /// Kills and level when the replay ended.
    pub kills: u32,
    pub level: u32,
}

/// Plays `r` again from its setup, tick by tick, with `watch` looking on.
pub fn play(r: &Replay, watch: &mut dyn Watch) -> Outcome {
    let mut c = Cursor::new(r);
    let mut sounds = Vec::new();
    while c.step(r, &mut sounds) {
        sounds.clear();
        watch.tick(&c.h, c.dt);
    }
    Outcome { ticks: r.ticks, secs: c.h.t, checks: c.checks, drift: c.drift, kills: c.h.kills, level: c.h.level }
}

/// A replay part way through: the night as it stands and where its events are up to.
struct Cursor {
    h: Harvest,
    seats: Vec<Input>,
    dt: f32,
    /// The next event, by place in `Replay::events`.
    next: usize,
    tick: u64,
    checks: usize,
    drift: Option<Drift>,
}

impl Cursor {
    fn new(r: &Replay) -> Self {
        Self { h: r.setup.build(), seats: vec![Input::default()], dt: 1.0 / 60.0, next: 0, tick: 0, checks: 0, drift: None }
    }

    /// Plays the next tick; false once every tick has played.
    fn step(&mut self, r: &Replay, sounds: &mut Vec<Sfx>) -> bool {
        let tick = self.tick;
        if tick >= r.ticks {
            return false;
        }
        while let Some((_, e)) = r.events.get(self.next).filter(|(t, e)| *t == tick && !matches!(e, Event::Check(_))) {
            self.next += 1;
            match e {
                Event::Input(seat, i) => {
                    if self.seats.len() <= *seat {
                        self.seats.resize(seat + 1, Input::default());
                    }
                    self.seats[*seat] = *i;
                }
                Event::Dt(d) => self.dt = *d,
                Event::Set(s) => self.h.apply_set(*s),
                Event::Seat(p, look) => self.h.seat(*p, look),
                Event::Unseat(p) => self.h.unseat(*p),
                Event::Op(op) => self.h.op(*op, sounds),
                Event::Check(_) => {}
            }
        }
        for (i, input) in self.h.inputs.iter_mut().enumerate() {
            *input = self.seats.get(i + 1).copied().unwrap_or_default();
        }
        self.h.update(&self.seats[0], self.dt, sounds);
        let sum = checksum(&self.h);
        if self.drift.is_none() && r.sums.get(tick as usize) != Some(&(sum as u8)) {
            self.drift = Some(Drift { tick, secs: self.h.t, check: None });
        }
        while let Some((_, Event::Check(want))) = r.events.get(self.next).filter(|(t, e)| *t == tick && matches!(e, Event::Check(_))) {
            self.next += 1;
            self.checks += 1;
            let got = Check::of(&self.h, sum);
            if got.sum != want.sum {
                let d = self.drift.get_or_insert(Drift { tick, secs: self.h.t, check: None });
                d.check.get_or_insert((tick, want.clone(), got));
            }
        }
        self.tick += 1;
        true
    }
}

/// A replay played to watch, a tick at a time, drawn as any player in it saw the night.
pub struct Viewer {
    r: Replay,
    c: Cursor,
}

impl Viewer {
    pub fn new(r: Replay) -> Self {
        let c = Cursor::new(&r);
        Self { r, c }
    }

    /// Plays the next tick, its sounds into `sounds`; false once the replay has ended.
    pub fn step(&mut self, sounds: &mut Vec<Sfx>) -> bool {
        self.c.step(&self.r, sounds)
    }

    /// No tick plays this frame, so the night draws where it stands.
    pub fn hold(&mut self) {
        self.c.h.hold_still();
    }

    /// Back to the start of the night.
    pub fn rewind(&mut self) {
        self.c = Cursor::new(&self.r);
    }

    pub fn replay(&self) -> &Replay {
        &self.r
    }

    /// Ticks played, then seconds played.
    pub fn played(&self) -> (u64, f32) {
        (self.c.tick, self.c.tick as f32 * self.c.dt)
    }

    pub fn ended(&self) -> bool {
        self.c.tick >= self.r.ticks
    }

    /// Where the night first stopped matching its recording, once it has.
    pub fn drift(&self) -> Option<&Drift> {
        self.c.drift.as_ref()
    }

    pub fn players(&self) -> usize {
        self.c.h.players.len()
    }

    /// The player view `p` shows: them, or once they've turned while others stand, the first who still does.
    pub fn shown(&self, p: PlayerId) -> PlayerId {
        self.c.h.followed(p.min(self.players() - 1))
    }

    pub fn one_screen(&self) -> bool {
        self.c.h.one_screen()
    }

    pub fn dread(&self) -> f32 {
        self.c.h.dread()
    }

    /// The night as player `p` saw it, `alpha` of the way through the last tick.
    pub fn draw(&mut self, p: PlayerId, alpha: f32, f: &mut crate::Frame) {
        let h = &mut self.c.h;
        let undo = h.blend(alpha);
        let seen = h.followed(p.min(h.players.len() - 1));
        h.relabel(seen);
        h.draw(f, false);
        h.relabel(seen);
        if let Some(undo) = undo {
            h.unblend(undo);
        }
    }
}

impl Harvest {
    fn apply_set(&mut self, s: Set) {
        match s {
            Set::OneScreen(on) => self.set_one_screen(on),
            Set::TouchAim(on) => self.set_touch_aim(on),
            Set::TouchKeys(on) => self.set_touch_keys(on),
            Set::SavesPhotos(on) => self.set_saves_photos(on),
            Set::Taps(on) => self.set_taps(on),
            Set::Canvas(size) => self.set_canvas(Some(size)),
            Set::Guides => self.toggle_guides(),
            Set::CornerMap(on) => self.set_corner_map(on),
        }
    }
}

/// Watches nothing.
pub struct Nothing;

impl Watch for Nothing {
    fn tick(&mut self, _: &Harvest, _: f32) {}

    fn report(&self, _: f32) -> Vec<String> {
        Vec::new()
    }
}

/// What each player's dogs spend their time doing, how often they pounce, and the hearts they bring back.
#[derive(Default)]
pub struct Dogs {
    /// By player: seconds wandering, chasing, catching up, in the air and fetching, then pounces begun.
    by: Vec<([f32; 5], u32)>,
    flying: Vec<Vec<bool>>,
    fetched: Vec<u32>,
}

impl Watch for Dogs {
    fn tick(&mut self, h: &Harvest, dt: f32) {
        self.by.resize(h.players.len(), ([0.0; 5], 0));
        self.flying.resize(h.players.len(), Vec::new());
        self.fetched.resize(h.players.len(), 0);
        for (p, pl) in h.players.iter().enumerate() {
            let was = &mut self.flying[p];
            was.resize(pl.dogs.len(), false);
            for (d, dog) in pl.dogs.iter().enumerate() {
                let doing = dog.doing();
                self.by[p].0[doing] += dt;
                if doing == 3 && !was[d] {
                    self.by[p].1 += 1;
                }
                was[d] = doing == 3;
            }
            self.fetched[p] = self.fetched[p].max(pl.dogs.iter().map(|d| d.fetched).sum());
        }
    }

    fn report(&self, secs: f32) -> Vec<String> {
        let names = ["wandering", "chasing", "catching up", "pouncing", "fetching"];
        self.by
            .iter()
            .enumerate()
            .filter(|(_, (t, _))| t.iter().sum::<f32>() > 0.0)
            .map(|(p, (t, pounces))| {
                let total: f32 = t.iter().sum();
                let shares: Vec<String> = names.iter().zip(t).map(|(n, s)| format!("{n} {:.0}%", s * 100.0 / total)).collect();
                let fetched = self.fetched[p];
                format!("player {} dogs: {}; {pounces} pounces, {:.2} a second; {fetched} hearts fetched, {:.1} a minute", p + 1, shares.join(", "), *pounces as f32 / secs.max(1e-3), fetched as f32 * 60.0 / secs.max(1e-3))
            })
            .collect()
    }
}

/// Where and how the first player is hurt: by ten seconds of night, and by what they were doing when bitten.
#[derive(Default)]
pub struct Hurt {
    trail: std::collections::VecDeque<[f32; 2]>,
    hp: Option<f32>,
    by_ten: Vec<f32>,
    /// Damage while camping, stuck (moved under a third of a walk in the last second), at the farm's edge, with no
    /// zombie in bite reach (spit or blast), from a biter behind them, and in all.
    camping: f32,
    stuck: f32,
    edge: f32,
    unbitten: f32,
    behind: f32,
    total: f32,
    /// Bites and the sides (of four) holding a zombie within 3 tiles at each, summed.
    bites: u32,
    sides: u32,
    near: u32,
    fell: Vec<f32>,
    first: Vec<String>,
    born: std::collections::HashMap<u32, (f32, f32)>,
    stuck_at: Vec<String>,
    still: f32,
    /// Before the first fall: seconds played and seconds within 2 tiles of the farm's edge.
    before: [f32; 2],
}

impl Watch for Hurt {
    fn tick(&mut self, h: &Harvest, dt: f32) {
        use super::map::TILE;
        let t = crate::tuning::tuning();
        let pl = &h.players[0];
        self.trail.push_back(pl.pos);
        if self.trail.len() as f32 * dt > 1.0 {
            self.trail.pop_front();
        }
        for z in &h.zombies {
            self.born.entry(z.id).or_insert((h.t, super::len(super::sub(z.pos, pl.pos))));
        }
        let moved = super::len(super::sub(pl.pos, *self.trail.front().unwrap_or(&pl.pos)));
        self.still = if moved < t.player_speed / 3.0 { self.still + dt } else { 0.0 };
        if self.still >= 3.0 && self.still - dt < 3.0 && self.stuck_at.len() < 6 {
            self.stuck_at.push(format!("t={:.0} at {:.0},{:.0} of {:.0}x{:.0} yaw={:.2} camp={:.0}", h.t, pl.pos[0], pl.pos[1], h.map.width_px(), h.map.height_px(), pl.view_yaw, pl.camp));
        }
        if h.juice.tally.book(0).fell.is_none() {
            let edge = pl.pos[0].min(pl.pos[1]).min(h.map.width_px() - pl.pos[0]).min(h.map.height_px() - pl.pos[1]);
            self.before[0] += dt;
            self.before[1] += if edge < 2.0 * super::map::TILE { dt } else { 0.0 };
        }
        let was = self.hp.replace(pl.hp).unwrap_or(pl.hp);
        if let Some(f) = h.juice.tally.book(0).fell
            && self.fell.last() != Some(&f)
            && pl.hp > was
        {
            self.fell.push(h.t);
        }
        if pl.hp >= was {
            return;
        }
        let hit = was - pl.hp;
        if hit <= 0.0 {
            return;
        }
        let b = (h.t / 10.0) as usize;
        if self.by_ten.len() <= b {
            self.by_ten.resize(b + 1, 0.0);
        }
        self.by_ten[b] += hit;
        self.total += hit;
        if moved < t.player_speed / 3.0 {
            self.stuck += hit;
        }
        if pl.camp >= t.camp_after {
            self.camping += hit;
        }
        let [w, hh] = [h.map.width_px(), h.map.height_px()];
        if pl.pos[0] < 2.0 * TILE || pl.pos[1] < 2.0 * TILE || pl.pos[0] > w - 2.0 * TILE || pl.pos[1] > hh - 2.0 * TILE {
            self.edge += hit;
        }
        let facing = [crate::fmath::cos(pl.view_yaw), crate::fmath::sin(pl.view_yaw)];
        let biter = h.zombies.iter().filter(|z| z.rise >= 1.0).map(|z| (super::len(super::sub(z.pos, pl.pos)), z.pos)).filter(|(d, _)| *d < t.bite_reach).min_by(|a, b| a.0.total_cmp(&b.0));
        match biter {
            None => self.unbitten += hit,
            Some((_, at)) => {
                self.bites += 1;
                let d = super::sub(at, pl.pos);
                if d[0] * facing[0] + d[1] * facing[1] < 0.0 {
                    self.behind += hit;
                }
                let mut quarters = [false; 4];
                for z in &h.zombies {
                    let d = super::sub(z.pos, pl.pos);
                    if super::len(d) < 3.0 * TILE {
                        self.near += 1;
                        let a = (crate::fmath::atan2(d[1], d[0]) - pl.view_yaw).rem_euclid(std::f32::consts::TAU);
                        quarters[((a + std::f32::consts::FRAC_PI_4) / std::f32::consts::FRAC_PI_2) as usize % 4] = true;
                    }
                }
                self.sides += quarters.iter().filter(|&&q| q).count() as u32;
                if self.fell.is_empty() {
                    let z = h.zombies.iter().find(|z| z.pos == at).unwrap();
                    let seen = pl.marks.iter().find(|m| m.id == z.id).map_or(-1.0, |m| m.age);
                    let edge = pl.pos[0].min(pl.pos[1]).min(w - pl.pos[0]).min(hh - pl.pos[1]) / TILE;
                    let ang = ((crate::fmath::atan2(d[1], d[0]) - pl.view_yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI).to_degrees();
                    let near = h.zombies.iter().filter(|o| super::len(super::sub(o.pos, pl.pos)) < 3.0 * TILE).count();
                    let (bt, bd) = self.born.get(&z.id).copied().unwrap_or_default();
                    self.first.push(format!("t={:.1} hp={:.0} edge={edge:.1}t biter speed={} rose {:.1}s ago {:.0}px off, angle={ang:.0} seen_age={seen:.1} near3={near} moved1s={moved:.0}", h.t, pl.hp, z.kind.stats().speed, h.t - bt, bd));
                }
            }
        }
    }

    fn report(&self, _: f32) -> Vec<String> {
        let share = |x: f32| format!("{:.0}%", x * 100.0 / self.total.max(1e-3));
        let per_bite = |x: u32| x as f32 / self.bites.max(1) as f32;
        vec![
            format!("hurt {:.0}; fell at {:?}", self.total, self.fell.iter().map(|t| t.round() as u32).collect::<Vec<_>>()),
            format!("by 10 s: {}", self.by_ten.iter().map(|x| format!("{x:.0}")).collect::<Vec<_>>().join(" ")),
            format!("camping {}, stuck {}, at the edge {}, no biter {}, bitten from behind {}", share(self.camping), share(self.stuck), share(self.edge), share(self.unbitten), share(self.behind)),
            self.first.join("\n  "),
            format!("before the first fall: {:.0} s, {:.0}% of it within 2 tiles of the edge", self.before[0], self.before[1] * 100.0 / self.before[0].max(1e-3)),
            format!("stuck 3 s: {}", self.stuck_at.join("; ")),
            format!("{} bites; zombies within 3 tiles {:.1}, sides holding one {:.2} of 4 (ahead, left, behind, right)", self.bites, per_bite(self.near), per_bite(self.sides)),
        ]
    }
}

/// A watcher by name, for the sandbox's `--watch`.
pub fn watcher(name: &str) -> Option<Box<dyn Watch>> {
    match name {
        "dogs" => Some(Box::<Dogs>::default()),
        "hurt" => Some(Box::<Hurt>::default()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Buttons;
    use crate::harvest::sandbox::{Pick, Plan, night};

    fn info() -> Vec<(String, String)> {
        vec![("game".into(), "test".into())]
    }

    fn recorded(plan: Plan) -> Replay {
        let n = night(&Plan { record: Some(info()), ..plan });
        Replay::read(&n.replay.expect("a recording")).unwrap()
    }

    fn short() -> Plan {
        Plan { seed: 3, until: Some(50.0), pick: Pick::Held, walk: Walk::Avoid, players: 2, ..Plan::new(parse_build("pistol:2,dog:2").unwrap()) }
    }

    #[test]
    fn an_older_format_says_so() {
        let mut old = MAGIC.to_vec();
        old.extend(1u32.to_le_bytes());
        let err = Replay::read(&old).unwrap_err();
        assert!(err.contains("older build"), "{err}");
    }

    #[test]
    fn a_setup_reads_back_as_written() {
        let night = Setup { map: "orchard".into(), look: "scarecrow".into(), farm: Some("scarecrow".into()), kit: "dog:2,crows".into(), start_t: 12.5, one_screen: true, players: 2, bots: 1, views: 3, progress: Progress { kills: 9, found: 0xf0, everything: true, hidden: 3, ..Progress::default() }, ..Setup::night(77) };
        let host = Setup { kind: Kind::Host { looks: vec![(0, "".into()), (2, "crow".into())], local: 1 }, difficulty: Difficulty::DEAD, ..Setup::night(5) };
        assert!(host.to_text().contains("\ndifficulty dead\n"), "{}", host.to_text());
        let sandbox = Plan { size: [40, 30], mortal: true, ..short() }.setup();
        for s in [night, host, sandbox] {
            assert_eq!(Setup::from_text(&s.to_text()), Ok(s.clone()), "{}", s.to_text());
        }
    }

    #[test]
    fn every_field_of_an_input_survives_the_trip() {
        let mut touches = [None; MAX_TOUCHES];
        (touches[1], touches[7]) = (Some([1.5, -2.0]), Some([0.25, 99.0]));
        let a = Input { held: Buttons::A | Buttons::UP, pressed: Buttons::A, stick: [0.3, -0.7], look: -0.0, touch: Some([3.0, 4.0]), touches, touch_started: true, lid_closed: true, orbit: [2.0, 1.0] };
        let b = Input { held: Buttons::B, touches: [None; MAX_TOUCHES], touch: None, look: 0.125, ..a };
        let mut seen = Input::default();
        for want in [a, b, Input::default()] {
            let mut bytes = Vec::new();
            let mask = write_input(&mut bytes, &seen, &want);
            let mut all = mask.to_le_bytes().to_vec();
            all.extend(bytes);
            seen = Reader { bytes: &all, at: 0 }.input(seen).unwrap();
            assert_eq!(format!("{seen:?}"), format!("{want:?}"));
        }
    }

    #[test]
    fn a_recorded_sandbox_night_replays_identically() {
        let plan = short();
        let n = night(&Plan { record: Some(info()), ..plan.clone() });
        assert_eq!(n.kills, night(&plan).kills, "recording changes nothing");
        let r = Replay::read(n.replay.as_ref().unwrap()).unwrap();
        assert_eq!((r.info("game"), r.setup.clone()), (Some("test"), plan.setup()));
        let out = play(&r, &mut Nothing);
        assert_eq!(out.drift, None);
        assert_eq!((out.kills, out.level, out.secs), (n.kills, n.level, n.secs));
        assert!(out.checks as u64 >= r.ticks / CHECK_EVERY && r.ticks > 2900, "{} checks over {} ticks", out.checks, r.ticks);
    }

    #[test]
    fn a_nudged_replay_names_the_tick_it_drifted() {
        let mut r = recorded(short());
        let at = r.nudge_input(1000);
        let out = play(&r, &mut Nothing);
        let d = out.drift.expect("it drifts");
        assert_eq!(d.tick, at);
        let (check, want, got) = d.check.expect("a checkpoint differs too");
        assert!(check >= at && check < at + CHECK_EVERY && want != got, "{check}");
    }

    #[test]
    fn a_night_from_the_game_replays_through_a_restart_and_a_quit() {
        let mut game = crate::Game::new(11);
        game.add_bots(1);
        game.record_replays(info());
        game.open(crate::Scene::Harvest);
        let press = |b: Buttons| Input { held: b, pressed: b, ..Input::default() };
        let walk = |b: Buttons| Input { held: b, ..Input::default() };
        let mut script: Vec<Input> = Vec::new();
        for (b, n) in [(Buttons::UP, 200), (Buttons::Y, 60), (Buttons::LEFT | Buttons::UP, 300), (Buttons::A, 100)] {
            script.push(press(b));
            script.extend(std::iter::repeat_n(walk(b), n));
        }
        script.extend([press(Buttons::START), Input::default(), press(Buttons::DOWN), Input::default(), press(Buttons::DOWN), Input::default(), press(Buttons::DOWN), Input::default(), press(Buttons::A)]);
        script.extend(std::iter::repeat_n(walk(Buttons::UP | Buttons::RIGHT), 400));
        for i in &script {
            game.update(i, 1.0 / 60.0);
        }
        game.open(crate::Scene::Title);
        let replays = game.take_replays();
        assert_eq!(replays.len(), 2, "the restarted night and the one after it");
        for raw in &replays {
            let r = Replay::read(raw).unwrap();
            assert_eq!(r.setup.bots, 1);
            let out = play(&r, &mut Nothing);
            assert_eq!(out.drift, None, "{} ticks", r.ticks);
            assert!(r.ticks > 300);
        }
        let first = Replay::read(&replays[0]).unwrap();
        let second = Replay::read(&replays[1]).unwrap();
        assert_ne!(first.setup.seed, second.setup.seed);
        assert_eq!(first.ticks + second.ticks + 1, script.len() as u64, "the tick that restarts belongs to neither");
    }

    #[test]
    fn a_hosted_night_replays_with_the_seats_that_came_and_went() {
        let setup = Setup { players: 2, kind: Kind::Host { looks: vec![(0, String::new()), (1, String::new())], local: 1 }, ..Setup::night(21) };
        let mut h = setup.build();
        h.record(info());
        let mut sounds = Vec::new();
        for tick in 0..900 {
            match tick {
                200 => h.seat(2, ""),
                500 => h.unseat(1),
                _ => {}
            }
            let them = Input { held: if tick % 90 < 45 { Buttons::UP } else { Buttons::LEFT }, look: 0.3, ..Input::default() };
            for p in 1..h.inputs.len() {
                h.steer(p, them);
            }
            h.update(&Input { held: Buttons::DOWN, ..Input::default() }, 1.0 / 60.0, &mut sounds);
        }
        let r = Replay::read(&h.stop_recording()[0]).unwrap();
        assert_eq!((r.ticks, &r.setup), (900, &setup));
        assert_eq!(play(&r, &mut Nothing).drift, None);
    }

    #[test]
    fn dogs_are_watched() {
        let r = recorded(Plan { players: 1, ..short() });
        let mut dogs = Dogs::default();
        play(&r, &mut dogs);
        let report = dogs.report(50.0);
        assert!(report.len() == 1 && report[0].contains("pounces"), "{report:?}");
    }
}
