//! The DPS sandbox: a whole night played headless at a fixed step by a scripted player with a forced build, and
//! what each weapon in it did. The script never aims: it walks a pattern or steers away from the zombies it has
//! seen, lobs a grenade straight ahead on a timer, and takes level-up cards by a strategy or none at all.

use crate::fmath as fm;
use std::f32::consts::{PI, TAU};

use super::map::TILE;
use super::powerups::{Kind, book};
use super::player::PlayerId;
use super::replay::{Kind as Made, Op, Setup};
use super::{Choice, Harvest, ME, Phase, Progress, dawn, len, sub};
use crate::rng::Rng;
use crate::sfx::Sfx;
use crate::{Buttons, Difficulty, Input};

/// How the scripted player walks: always ahead, turning steadily one way, one way a loop then the other, turning
/// away from the zombies the map shows near it, or to the farm's nearest corner to stand there facing out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Walk {
    #[default]
    Circle,
    Eight,
    Avoid,
    Corner,
}

impl Walk {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "circle" => Some(Walk::Circle),
            "eight" => Some(Walk::Eight),
            "avoid" => Some(Walk::Avoid),
            "corner" => Some(Walk::Corner),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Walk::Circle => "circle",
            Walk::Eight => "eight",
            Walk::Avoid => "avoid",
            Walk::Corner => "corner",
        }
    }
}

/// How the scripted player takes level-up cards: not at all, the forced build staying fixed; any of the three; one
/// for a powerup it holds, rerolling and banishing for one; or the first a priority list names, else as `Held`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Pick {
    #[default]
    None,
    Random,
    Held,
    First(Vec<usize>),
}

impl Pick {
    /// `none`, `random`, `held`, or `first:` and powerup ids joined by `+` or `,`.
    pub fn from_name(name: &str) -> Result<Self, String> {
        match name {
            "none" => Ok(Pick::None),
            "random" => Ok(Pick::Random),
            "held" => Ok(Pick::Held),
            _ => {
                let list = name.strip_prefix("first:").ok_or_else(|| format!("{name} is not none, random, held or first:ids"))?;
                let ids = list.split(['+', ',']).map(str::trim).filter(|s| !s.is_empty());
                let order = ids.map(|id| book().index(id).ok_or_else(|| format!("{id}: no such powerup"))).collect::<Result<Vec<_>, _>>()?;
                if order.is_empty() { Err("first: names no powerup".into()) } else { Ok(Pick::First(order)) }
            }
        }
    }

    pub fn name(&self) -> String {
        match self {
            Pick::None => "none".into(),
            Pick::Random => "random".into(),
            Pick::Held => "held".into(),
            Pick::First(order) => format!("first:{}", order.iter().map(|&d| book().defs[d].id.as_str()).collect::<Vec<_>>().join("+")),
        }
    }
}

/// A level-up card the scripted player took, and when.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Took {
    pub secs: f32,
    pub up: usize,
    pub rare: bool,
}

/// One night to play.
#[derive(Clone, Debug)]
pub struct Plan {
    pub seed: u64,
    pub difficulty: Difficulty,
    /// The generated farm's size in tiles.
    pub size: [usize; 2],
    /// Weapons and passives by place in the book and level, one past a weapon's top being its breaker's.
    pub build: Vec<(usize, u32)>,
    pub walk: Walk,
    pub pick: Pick,
    /// Every powerup can come up on a card, as for a player who has unlocked them all.
    pub unlocked: bool,
    /// How hard the walk turns, as a share of the turn buttons' full speed.
    pub turn: f32,
    /// Seconds between turnarounds, and between grenades; 0 for never.
    pub turn_every: f32,
    pub throw_every: f32,
    /// The player can die, which ends the night.
    pub mortal: bool,
    /// Players in the night, each scripted the same way and carrying the build.
    pub players: usize,
    /// Seconds into the night to stop at; past dawn the night goes on, with the Harvester awake.
    pub until: Option<f32>,
    pub step: f32,
    /// Records the night as a replay headed by these lines.
    pub record: Option<Vec<(String, String)>>,
}

impl Plan {
    pub fn new(build: Vec<(usize, u32)>) -> Self {
        Self { seed: 1, difficulty: Difficulty::NORMAL, size: super::MAP_SIZE, build, walk: Walk::Circle, pick: Pick::None, unlocked: true, turn: 0.25, turn_every: 6.0, throw_every: 12.0, mortal: false, players: 1, until: None, step: 1.0 / 60.0, record: None }
    }

    /// What the night is built from, as a replay keeps it.
    pub fn setup(&self) -> Setup {
        let kind = Made::Sandbox { size: self.size, build: self.build.clone(), mortal: self.mortal };
        let progress = Progress { everything: self.unlocked, ..Progress::default() };
        Setup { seed: self.seed, difficulty: self.difficulty, progress, players: self.players, walk: self.walk, turn: self.turn, turn_every: self.turn_every, throw_every: self.throw_every, kind, ..Setup::night(self.seed) }
    }
}

/// What one weapon in the build did, or grenades and barrels (id `grenades`, level 0).
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub id: String,
    pub level: u32,
    pub raw: f32,
    /// Damage up to each zombie's remaining health: raw less overkill.
    pub useful: f32,
    /// Killing blows.
    pub kills: u32,
    /// Seconds held.
    pub held: f32,
}

/// The run as it stood at the end of a minute of the night.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Minute {
    pub kills: u32,
    pub hearts: u32,
    pub level: u32,
    /// Zombies on the field (risen, rising and burrowed): the mean and the most at once over the minute.
    pub zombies: f32,
    pub zombies_most: u32,
    /// The most shots in flight at once: bullets, grenades, spit and bolts.
    pub shots_most: u32,
    pub fx_most: u32,
    /// Players still standing at the minute's end.
    pub living: u32,
}

/// How one player's night went.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Seat {
    pub fell: Option<f32>,
    pub hearts: u32,
    pub kills: u32,
    pub useful: f32,
    /// Turned: seconds leading the dead, and the bites their swarm landed.
    pub led: f32,
    pub bites: u32,
}

/// How a night went.
#[derive(Clone, Debug)]
pub struct Night {
    /// Seconds of night played.
    pub secs: f32,
    pub dawn: bool,
    /// When the player first fell, though an undying run carries on.
    pub fell: Option<f32>,
    pub kills: u32,
    pub hearts: u32,
    pub level: u32,
    /// Zombies that rose, and seconds the night spent at its zombie cap, where spawns wait for room.
    pub spawned: u32,
    pub capped: f32,
    /// The zombie cap, and when the field first reached it.
    pub cap: u32,
    pub capped_at: Option<f32>,
    pub minutes: Vec<Minute>,
    pub lines: Vec<Line>,
    /// The build at the end of the night, and the cards taken on the way.
    pub build: Vec<(usize, u32)>,
    pub took: Vec<Took>,
    /// Each player's own numbers, the local player first.
    pub seats: Vec<Seat>,
    pub replay: Option<Vec<u8>>,
}

/// A build from `id` or `id:level` items, comma separated; `top` is a weapon's last level and `break` its breaker's.
pub fn parse_build(text: &str) -> Result<Vec<(usize, u32)>, String> {
    let b = book();
    let mut build = Vec::new();
    for item in text.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let (id, level) = item.split_once(':').unwrap_or((item, "1"));
        let def = b.index(id).ok_or_else(|| format!("{id}: no such powerup"))?;
        let d = &b.defs[def];
        let top = d.max + (d.kind == Kind::Weapon) as u32;
        let level = match level {
            "top" => d.max,
            "break" if d.kind == Kind::Weapon => d.max + 1,
            n => n.parse().ok().filter(|n| (1..=top).contains(n)).ok_or_else(|| format!("{item}: the level is 1 to {top}, top or break"))?,
        };
        if build.iter().any(|&(o, _)| o == def) {
            return Err(format!("{id}: named twice"));
        }
        build.push((def, level));
    }
    Ok(build)
}

/// A build's text as `parse_build` reads it.
pub fn build_text(build: &[(usize, u32)]) -> String {
    build.iter().map(|&(d, l)| format!("{}:{l}", book().defs[d].id)).collect::<Vec<_>>().join(",")
}

/// A powerup's id by its place in the book.
pub fn powerup_id(up: usize) -> &'static str {
    &book().defs[up].id
}

/// Each weapon in the book by id, and its levels 1 to its top, then its breaker's.
pub fn weapon_levels() -> Vec<(String, u32)> {
    book().defs.iter().filter(|d| d.kind == Kind::Weapon).map(|d| (d.id.clone(), d.max + 1)).collect()
}

/// Plays the night `plan` describes.
pub fn night(plan: &Plan) -> Night {
    let mut h = plan.setup().build();
    if let Some(info) = &plan.record {
        h.record(info.clone());
    }
    let mut rng = Rng::new(plan.seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let mut took = Vec::new();
    let end = plan.until.unwrap_or(dawn());
    let mut me = Bot::new(plan.walk, plan.turn, plan.turn_every, plan.throw_every);
    let mut minutes = Vec::new();
    let mut capped = 0.0;
    let mut capped_at = None;
    let (mut field, mut field_secs, mut field_most, mut shots_most, mut fx_most) = (0.0, 0.0, 0, 0, 0);
    let mut sounds = Vec::new();
    let cap = h.cap();
    // Hit-stop and the level-up's slow motion hold the night's clock, so ticks are bounded well past the night.
    for _ in 0..(end / plan.step) as u64 * 8 {
        if h.t >= end || matches!(h.phase, Phase::Over { .. }) {
            break;
        }
        let t = h.t;
        let input = me.step(&mut h, ME, &mut sounds);
        h.update(&input, plan.step, &mut sounds);
        sounds.clear();
        if let Phase::LevelUp { .. } = h.phase {
            took.extend(choose(&mut h, &plan.pick, &mut rng, &mut sounds));
            for p in 1..h.players.len() {
                if h.players[p].alive() {
                    h.op(Op::Relabel(p), &mut sounds);
                    h.op(Op::Deal, &mut sounds);
                    choose(&mut h, &plan.pick, &mut rng, &mut sounds);
                    h.op(Op::Relabel(p), &mut sounds);
                }
            }
            sounds.clear();
            h.op(Op::Resume, &mut sounds);
        }
        let on_field = h.zombies.len() + h.tells.len() + h.burrowed.len();
        if on_field >= cap {
            capped += h.t - t;
            capped_at.get_or_insert(h.t);
        }
        field += on_field as f32 * (h.t - t);
        field_secs += h.t - t;
        field_most = field_most.max(on_field as u32);
        shots_most = shots_most.max((h.bullets.len() + h.thrown.len() + h.spits.len() + h.bolts.len()) as u32);
        fx_most = fx_most.max(h.fx.len() as u32);
        while minutes.len() < (h.t / 60.0) as usize {
            let zombies = if field_secs > 0.0 { field / field_secs } else { on_field as f32 };
            let living = h.players.iter().filter(|p| p.alive()).count() as u32;
            minutes.push(Minute { kills: h.kills, hearts: h.juice.tally.book(ME).hearts, level: h.level, zombies, zombies_most: field_most, shots_most, fx_most, living });
            (field, field_secs, field_most, shots_most, fx_most) = (0.0, 0.0, 0, 0, 0);
        }
    }
    let tally = &h.juice.tally;
    let build: Vec<(usize, u32)> = h.me().kit.weapons.iter().map(|a| (a.def, a.level)).chain(h.me().kit.passives.iter().copied()).collect();
    let mut lines: Vec<Line> = build
        .iter()
        .filter(|&&(d, _)| book().defs[d].kind == Kind::Weapon)
        .map(|&(d, level)| {
            let x = tally.dealt(ME, Some(d));
            Line { id: book().defs[d].id.clone(), level, raw: x.raw, useful: x.useful, kills: x.kills, held: x.held }
        })
        .collect();
    let x = tally.dealt(ME, None);
    lines.push(Line { id: "grenades".into(), level: 0, raw: x.raw, useful: x.useful, kills: x.kills, held: x.held });
    Night {
        secs: h.t,
        dawn: h.t >= dawn(),
        fell: tally.book(ME).fell,
        kills: h.kills,
        hearts: tally.book(ME).hearts,
        level: h.level,
        spawned: h.next_id.saturating_sub(1),
        capped,
        cap: cap as u32,
        capped_at,
        minutes,
        lines,
        build,
        took,
        seats: (0..h.players.len())
            .map(|p| {
                let (kills, useful) = tally.sum(p);
                let b = tally.book(p);
                Seat { fell: b.fell, hearts: b.hearts, kills, useful, led: b.led, bites: b.bites }
            })
            .collect(),
        replay: h.stop_recording().pop(),
    }
}

/// A scripted player: it walks a pattern or steers away from the zombies its map shows, turns round on a schedule
/// and lobs a grenade straight ahead on a timer. It never aims.
#[derive(Clone, Debug)]
pub struct Bot {
    walk: Walk,
    turn: f32,
    turn_every: f32,
    throw_every: f32,
    turned: u32,
    thrown: u32,
    parked: bool,
    /// The avoiding walk's last turn.
    veer: f32,
}

impl Bot {
    /// One that walks `walk`, turning at `turn` of the turn buttons' speed, turning round every `turn_every` seconds
    /// and throwing every `throw_every`; 0 for never.
    pub fn new(walk: Walk, turn: f32, turn_every: f32, throw_every: f32) -> Self {
        Self { walk, turn, turn_every, throw_every, turned: 0, thrown: 0, parked: false, veer: 0.0 }
    }

    /// The same script from the start of a night.
    pub(super) fn again(self) -> Self {
        Self { turned: 0, thrown: 0, parked: false, veer: 0.0, ..self }
    }

    /// Player `p`'s input for the next step, after throwing if its timer is up.
    pub(super) fn step(&mut self, h: &mut Harvest, p: PlayerId, sounds: &mut Vec<Sfx>) -> Input {
        if let Some(input) = h.lead_bot(p) {
            return input;
        }
        let t = h.t;
        let loop_secs = TAU / (self.turn.max(0.01) * crate::tuning::tuning().turn_speed);
        let ever = |every: f32, t: f32| if every > 0.0 { (t / every) as u32 } else { 0 };
        let mut input = Input { held: Buttons::UP, ..Input::default() };
        input.look = match self.walk {
            Walk::Corner => self.head_for_corner(h, p, &mut input),
            Walk::Avoid => {
                self.veer = avoid(h, p, self.veer);
                self.veer
            }
            Walk::Eight if (t / loop_secs) as u32 % 2 == 1 => -self.turn,
            _ => self.turn,
        };
        if ever(self.turn_every, t) > self.turned && !matches!(self.walk, Walk::Avoid | Walk::Corner) {
            self.turned = ever(self.turn_every, t);
            input.pressed = Buttons::B;
        }
        if ever(self.throw_every, t) > self.thrown && matches!(h.phase, Phase::Playing) {
            self.thrown = ever(self.throw_every, t);
            let reach = crate::tuning::tuning().quick_near;
            let pl = &h.players[p];
            let to = [pl.pos[0] + pl.aim[0] * reach, pl.pos[1] + pl.aim[1] * reach];
            h.op(Op::Throw { p, to }, sounds);
        }
        input
    }
}

impl Bot {
    /// Heads for the farm's nearest corner, and once within `CORNER_NEAR` of it, or after `CORNER_WALK` seconds stuck
    /// short of it, turns round to face out and stands.
    fn head_for_corner(&mut self, h: &Harvest, p: PlayerId, input: &mut Input) -> f32 {
        let pl = &h.players[p];
        let [w, hh] = [h.map.width_px(), h.map.height_px()];
        let at = [if pl.pos[0] < w / 2.0 { TILE } else { w - TILE }, if pl.pos[1] < hh / 2.0 { TILE } else { hh - TILE }];
        if !self.parked && (len(sub(at, pl.pos)) < CORNER_NEAR || h.t > CORNER_WALK) {
            self.parked = true;
            input.pressed = Buttons::B;
        }
        if self.parked {
            input.held = Buttons::NONE;
            return 0.0;
        }
        let d = sub(at, pl.pos);
        let off = (fm::atan2(d[1], d[0]) - pl.view_yaw + PI).rem_euclid(TAU) - PI;
        (off * 3.0).clamp(-1.0, 1.0)
    }
}

/// Seconds the corner walk walks before it stands wherever it got to, and how near the corner it stops.
const CORNER_WALK: f32 = 60.0;
const CORNER_NEAR: f32 = 3.0 * TILE;

/// Zombies further than this from the player don't sway the avoiding walk, in world pixels.
const AVOID_REACH: f32 = 240.0;
/// How far along each way it might take the avoiding walk tries walking, sliding on walls as the player does.
const AVOID_WALL: f32 = 40.0;
/// How far ahead the avoiding walk looks for zombies beside its path, and how near the path one counts.
const AVOID_AHEAD: f32 = 48.0;
const AVOID_MISS: f32 = 16.0;
/// How hard the zombies near a way weigh against it.
const AVOID_DANGER: f32 = 3.0;
/// Within this many pixels of the farm's edge the avoiding walk leans back toward the middle.
const AVOID_EDGE: f32 = 4.0 * TILE;
/// How much the avoiding walk favors turning the way it last turned, so it doesn't dither between two ways.
const AVOID_KEEP: f32 = 0.5;

/// The turn, -1 to 1, toward the open way that passes furthest from the zombies on the map as a player reads it:
/// each where the view ahead last saw it, fading as it's forgotten, the nearer the stronger. It never stops walking,
/// would rather veer than turn back, and keeps off the farm's edge.
fn avoid(h: &Harvest, p: PlayerId, veer: f32) -> f32 {
    let me = &h.players[p];
    let near: Vec<([f32; 2], f32)> = me
        .marks
        .iter()
        .filter_map(|m| {
            let d = sub(m.pos, me.pos);
            let r = len(d);
            (r < AVOID_REACH).then(|| (d, m.fade() * (1.0 - r / AVOID_REACH)))
        })
        .collect();
    let half = crate::tuning::tuning().player_half();
    let [w, hh] = [h.map.width_px(), h.map.height_px()];
    let edge = [w - TILE, hh - TILE];
    let inside = |p: [f32; 2]| p[0] > TILE && p[1] > TILE && p[0] < edge[0] && p[1] < edge[1];
    let from_edge = me.pos[0].min(me.pos[1]).min(w - me.pos[0]).min(hh - me.pos[1]);
    let middle = sub([w / 2.0, hh / 2.0], me.pos);
    let pull = if inside(me.pos) { (1.0 - from_edge / AVOID_EDGE).max(0.0) } else { 1.0 };
    let back = [pull * middle[0] / len(middle).max(1.0), pull * middle[1] / len(middle).max(1.0)];
    // How far it gets along `dir`, as a share of AVOID_WALL, and how far its first step gets.
    let walk = |dir: [f32; 2]| {
        let (mut p, mut first) = (me.pos, 0.0);
        for s in 0..8 {
            let next = h.map.slide(p, [dir[0] * AVOID_WALL / 8.0, dir[1] * AVOID_WALL / 8.0], half);
            if s == 0 {
                first = len(sub(next, me.pos)) * 8.0 / AVOID_WALL;
            }
            if !inside(next) && inside(me.pos) {
                return (0.0, 0.0);
            }
            p = next;
        }
        let d = sub(p, me.pos);
        ((d[0] * dir[0] + d[1] * dir[1]) / AVOID_WALL, first)
    };
    let best = (-7..=8)
        .map(|j| {
            let off = j as f32 * PI / 8.0;
            let dir = [fm::cos(me.view_yaw + off), fm::sin(me.view_yaw + off)];
            let danger: f32 = near
                .iter()
                .map(|(d, w)| {
                    let along = (d[0] * dir[0] + d[1] * dir[1]).clamp(0.0, AVOID_AHEAD);
                    let miss = len(sub(*d, [dir[0] * along, dir[1] * along])) / AVOID_MISS;
                    w * fm::exp(-miss * miss)
                })
                .sum();
            let (free, first) = walk(dir);
            let score = fm::cos(off) - AVOID_DANGER * danger + 3.0 * (dir[0] * back[0] + dir[1] * back[1]) + 2.0 * free - if first < 0.5 { 100.0 } else { 0.0 } + if off * veer > 0.0 { AVOID_KEEP } else { 0.0 };
            (off, score)
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(0.0, |b| b.0);
    (best * 3.0).clamp(-1.0, 1.0)
}

/// Takes a card from the level-up on show by `pick`'s strategy: what it took, or None for `Pick::None`.
fn choose(h: &mut Harvest, pick: &Pick, rng: &mut Rng, sounds: &mut Vec<Sfx>) -> Option<Took> {
    let Phase::LevelUp { choices, .. } = h.phase else { return None };
    if *pick == Pick::None {
        return None;
    }
    h.op(Op::Begin, sounds);
    let i = match pick {
        Pick::None => return None,
        Pick::Random => rng.below(choices.len()),
        Pick::Held => stack(h, sounds),
        Pick::First(order) => {
            let first = |cs: &[Choice; 3]| cs.iter().enumerate().filter_map(|(i, c)| order.iter().position(|&o| o == c.up).map(|r| (r, i))).min().map(|(_, i)| i);
            if first(&h.picking()).is_none() && h.rerolls > 0 {
                h.op(Op::Reroll, sounds);
            }
            match first(&h.picking()) {
                Some(i) => i,
                None => stack(h, sounds),
            }
        }
    };
    let c = h.picking()[i];
    if *pick != Pick::Random && h.me().kit.level(c.up) > 0 && book().defs[c.up].kind == Kind::Weapon {
        h.op(Op::Favor(c.up), sounds);
    }
    h.op(Op::Take(i), sounds);
    Some(Took { secs: h.t, up: c.up, rare: c.rare })
}

/// A card for a powerup the player holds, rare first; with none, it rerolls, then banishes a card it doesn't hold,
/// while it can, and failing that takes a new one, rare first.
fn stack(h: &mut Harvest, sounds: &mut Vec<Sfx>) -> usize {
    let held = |h: &Harvest, c: &Choice| h.me().kit.level(c.up) > 0;
    while !h.picking().iter().any(|c| held(h, c)) {
        if h.rerolls > 0 {
            h.op(Op::Reroll, sounds);
        } else if let Some(i) = h.picking().iter().position(|c| h.banishes > 0 && book().defs[c.up].kind != Kind::Filler) {
            h.op(Op::Banish(i), sounds);
        } else {
            break;
        }
    }
    let choices = h.picking();
    (0..choices.len()).max_by_key(|&i| (held(h, &choices[i]), choices[i].rare, std::cmp::Reverse(i))).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn short(build: &str, seed: u64) -> Night {
        night(&Plan { seed, until: Some(45.0), ..Plan::new(parse_build(build).unwrap()) })
    }

    #[test]
    fn a_build_reads_levels_top_and_break_and_refuses_the_rest() {
        let b = book();
        let max = b.defs[b.index("pistol").unwrap()].max;
        let build = parse_build("pistol:top, knife, shotgun:break").unwrap();
        assert_eq!(build.iter().map(|p| p.1).collect::<Vec<_>>(), [max, 1, max + 1]);
        assert_eq!(build_text(&build), format!("pistol:{max},knife:1,shotgun:{}", max + 1));
        assert!(parse_build("pistol:0").is_err());
        assert!(parse_build("nothing").is_err());
        assert!(parse_build("knife,knife:2").is_err());
        assert!(parse_build("hollow_points:break").is_err(), "a passive has no breaker");
    }

    #[test]
    fn the_same_seed_plays_the_same_night() {
        let (a, b) = (short("pistol:3", 5), short("pistol:3", 5));
        assert_eq!((a.kills, a.hearts, &a.lines), (b.kills, b.hearts, &b.lines));
    }

    #[test]
    fn only_the_build_fights_and_every_kill_is_some_weapons() {
        let n = short("knife:4", 2);
        assert!(n.kills > 0 && n.spawned >= n.kills, "{n:?}");
        assert_eq!(n.lines.iter().map(|l| l.id.as_str()).collect::<Vec<_>>(), ["knife", "grenades"], "the starting pistol is gone");
        assert_eq!(n.lines.iter().map(|l| l.kills).sum::<u32>(), n.kills);
        for l in &n.lines {
            assert!(l.useful <= l.raw + 1e-3, "{l:?}");
        }
        assert!((n.lines[0].held - n.secs).abs() < 0.1 && n.secs >= 45.0);
        assert_eq!(n.minutes.len(), 0, "less than a minute");
    }

    #[test]
    fn each_minute_counts_the_field_the_shots_and_the_living() {
        let n = night(&Plan { seed: 3, until: Some(125.0), players: 2, ..Plan::new(parse_build("pistol:3").unwrap()) });
        assert_eq!((n.cap, n.minutes.len()), (450, 2), "{n:?}");
        for m in &n.minutes {
            assert!(m.zombies > 0.0 && m.zombies <= m.zombies_most as f32 && m.shots_most > 0 && m.living == 2, "{m:?}");
        }
    }

    #[test]
    fn an_undying_player_lasts_the_night_where_a_mortal_one_falls() {
        let unarmed = |mortal| night(&Plan { mortal, difficulty: Difficulty::HARD, until: Some(240.0), throw_every: 0.0, ..Plan::new(Vec::new()) });
        let (undying, mortal) = (unarmed(false), unarmed(true));
        assert!(undying.fell.is_some() && undying.secs >= 240.0 && undying.minutes.len() == 4, "{undying:?}");
        assert!(!mortal.dawn && mortal.secs < 240.0 && mortal.fell == Some(mortal.secs), "{:?} {}", mortal.fell, mortal.secs);
    }

    #[test]
    fn with_no_pick_the_build_stays_as_forced_and_old_nights_replay() {
        let n = night(&Plan { until: Some(120.0), ..Plan::new(parse_build("pistol:2,running_shoes:1").unwrap()) });
        assert!(n.level > 1 && n.took.is_empty(), "it levelled but took nothing");
        assert_eq!(build_text(&n.build), "pistol:2,running_shoes:1");
    }

    #[test]
    fn each_pick_takes_a_card_at_every_level_up_from_the_real_draw() {
        let start = parse_build("pistol:1").unwrap();
        let play = |pick: &str| night(&Plan { until: Some(150.0), pick: Pick::from_name(pick).unwrap(), ..Plan::new(start.clone()) });
        for pick in ["random", "held", "first:running_shoes+knife"] {
            let n = play(pick);
            assert!(n.took.len() as u32 >= n.level - 1, "{pick}: a card a level, {} for level {}", n.took.len(), n.level);
            let gained: u32 = n.took.iter().map(|t| 1 + t.rare as u32).sum();
            let levels: u32 = n.build.iter().map(|b| b.1).sum();
            assert!(levels <= 1 + gained && levels > 1, "{pick}: {} from {gained} levels taken", build_text(&n.build));
            assert!(n.lines.iter().map(|l| l.kills).sum::<u32>() == n.kills, "{pick}: every kill is a weapon's or a grenade's");
        }
        let shoes = book().index("running_shoes").unwrap();
        let first = Pick::from_name("first:running_shoes+knife").unwrap();
        let found = (1..=12).any(|seed| night(&Plan { seed, until: Some(150.0), pick: first.clone(), ..Plan::new(start.clone()) }).took.first().is_some_and(|t| t.up == shoes));
        assert!(found, "the first card on the list comes first when a reroll finds it");
        let stacked = |pick: &str| {
            let nights: Vec<Night> = (1..=4).map(|seed| night(&Plan { seed, until: Some(240.0), pick: Pick::from_name(pick).unwrap(), ..Plan::new(start.clone()) })).collect();
            nights.iter().flat_map(|n| &n.build).map(|b| b.1).sum::<u32>() as f32 / nights.iter().map(|n| n.build.len()).sum::<usize>() as f32
        };
        let (held, random) = (stacked("held"), stacked("random"));
        assert!(held > random, "held stacks more levels on each powerup: {held} against {random}");
    }

    #[test]
    fn a_pick_reads_its_names_and_refuses_the_rest() {
        assert_eq!(Pick::from_name("held"), Ok(Pick::Held));
        assert_eq!(Pick::from_name("first:knife,pistol").map(|p| p.name()), Ok("first:knife+pistol".into()));
        assert!(Pick::from_name("first:").is_err() && Pick::from_name("first:nothing").is_err() && Pick::from_name("best").is_err());
    }

    #[test]
    fn the_avoiding_walk_turns_from_a_zombie_it_sees_and_not_from_one_it_does_not() {
        let src = super::super::map::MapSource::new("Open", [40, 40]);
        let mut h = Harvest::calm_on(1, &src).unwrap();
        h.me_mut().pos = [20.5 * TILE, 20.5 * TILE];
        h.me_mut().view_yaw = 0.0;
        assert_eq!(avoid(&h, ME, 0.0), 0.0, "nothing seen, it walks straight on");
        let mut z = super::super::Zombie::new(super::super::Kind::common(), [h.me().pos[0] + 30.0, h.me().pos[1] + 4.0], 1000.0, 1.0, 0.0);
        z.id = 9;
        h.zombies = vec![z];
        h.remember(1.0 / 60.0);
        assert!(avoid(&h, ME, 0.0) < 0.0, "a zombie ahead and to the right turns it left");
        h.me_mut().marks.clear();
        h.me_mut().view_yaw = PI;
        h.remember(1.0 / 60.0);
        assert!(h.me().marks.is_empty() && avoid(&h, ME, 0.0) == 0.0, "one behind it hasn't seen doesn't sway it");
    }

    #[test]
    fn the_avoiding_walk_leans_off_the_farms_edge_and_from_a_zombie_beside_its_path() {
        let src = super::super::map::MapSource::new("Open", [40, 40]);
        let mut h = Harvest::calm_on(1, &src).unwrap();
        h.me_mut().pos = [20.5 * TILE, 1.5 * TILE];
        h.me_mut().view_yaw = 0.0;
        assert!(avoid(&h, ME, 0.0) > 0.0, "walking along the top edge, it turns toward the middle");
        h.me_mut().pos = [20.5 * TILE, 20.5 * TILE];
        let mut z = super::super::Zombie::new(super::super::Kind::common(), [h.me().pos[0] + 30.0, h.me().pos[1] + 10.0], 1000.0, 1.0, 0.0);
        z.id = 9;
        h.zombies = vec![z];
        h.remember(1.0 / 60.0);
        assert!(avoid(&h, ME, 0.0) < 0.0, "a zombie 10 px right of its path turns it left");
    }
}
