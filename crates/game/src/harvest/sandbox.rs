//! The DPS sandbox: a whole night played headless at a fixed step by a scripted player with a forced build, and
//! what each weapon in it did. The script never aims: it walks a pattern or steers away from the zombies it has
//! seen, lobs a grenade straight ahead on a timer, and takes level-up cards by a strategy or none at all.

use std::f32::consts::{PI, TAU};

use super::map::TILE;
use super::powerups::{Kind, book};
use super::weapons::Kit;
use super::{Choice, Harvest, Phase, dawn, len, sub};
use crate::rng::Rng;
use crate::sfx::Sfx;
use crate::{Buttons, Difficulty, Input};

/// How the scripted player walks: always ahead, turning steadily one way, one way a loop then the other, or turning
/// away from the zombies the map shows near it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Walk {
    #[default]
    Circle,
    Eight,
    Avoid,
}

impl Walk {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "circle" => Some(Walk::Circle),
            "eight" => Some(Walk::Eight),
            "avoid" => Some(Walk::Avoid),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Walk::Circle => "circle",
            Walk::Eight => "eight",
            Walk::Avoid => "avoid",
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
    /// Seconds into the night to stop at; past dawn the night goes on, with the Harvester awake.
    pub until: Option<f32>,
    pub step: f32,
}

impl Plan {
    pub fn new(build: Vec<(usize, u32)>) -> Self {
        Self { seed: 1, difficulty: Difficulty::Normal, size: super::MAP_SIZE, build, walk: Walk::Circle, pick: Pick::None, unlocked: true, turn: 0.25, turn_every: 6.0, throw_every: 12.0, mortal: false, until: None, step: 1.0 / 60.0 }
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
    pub minutes: Vec<Minute>,
    pub lines: Vec<Line>,
    /// The build at the end of the night, and the cards taken on the way.
    pub build: Vec<(usize, u32)>,
    pub took: Vec<Took>,
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
    let mut h = Harvest::sized(plan.seed, plan.size).at(plan.difficulty);
    h.kit = Kit::forced(&plan.build, plan.difficulty);
    h.hp = h.kit.max_hp();
    h.grenades = h.kit.grenades();
    h.undying = !plan.mortal;
    h.progress.everything = plan.unlocked;
    let mut rng = Rng::new(plan.seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let mut took = Vec::new();
    let end = plan.until.unwrap_or(dawn());
    let loop_secs = TAU / (plan.turn.max(0.01) * crate::tuning::tuning().turn_speed);
    let ever = |every: f32, t: f32| if every > 0.0 { (t / every) as u32 } else { 0 };
    let (mut turned, mut thrown) = (0, 0);
    let mut minutes = Vec::new();
    let mut capped = 0.0;
    let mut sounds = Vec::new();
    let cap = crate::tuning::tuning().max_zombies as usize;
    // Hit-stop and the level-up's slow motion hold the night's clock, so ticks are bounded well past the night.
    for _ in 0..(end / plan.step) as u64 * 8 {
        if h.t >= end || matches!(h.phase, Phase::Over { .. }) {
            break;
        }
        let t = h.t;
        let mut input = Input { held: Buttons::UP, ..Input::default() };
        input.look = match plan.walk {
            Walk::Avoid => avoid(&h),
            Walk::Eight if (t / loop_secs) as u32 % 2 == 1 => -plan.turn,
            _ => plan.turn,
        };
        if ever(plan.turn_every, t) > turned && plan.walk != Walk::Avoid {
            turned = ever(plan.turn_every, t);
            input.pressed = Buttons::B;
        }
        if ever(plan.throw_every, t) > thrown && matches!(h.phase, Phase::Playing) {
            thrown = ever(plan.throw_every, t);
            let reach = crate::tuning::tuning().quick_near;
            let to = [h.pos[0] + h.aim[0] * reach, h.pos[1] + h.aim[1] * reach];
            h.throw_at(to, &mut sounds);
        }
        h.update(&input, plan.step, &mut sounds);
        sounds.clear();
        if let Phase::LevelUp { .. } = h.phase {
            took.extend(choose(&mut h, &plan.pick, &mut rng, &mut sounds));
            sounds.clear();
            h.phase = Phase::Playing;
            h.chest = false;
        }
        if h.zombies.len() + h.tells.len() + h.burrowed.len() >= cap {
            capped += h.t - t;
        }
        while minutes.len() < (h.t / 60.0) as usize {
            minutes.push(Minute { kills: h.kills, hearts: h.juice.tally.hearts, level: h.level });
        }
    }
    let tally = &h.juice.tally;
    let build: Vec<(usize, u32)> = h.kit.weapons.iter().map(|a| (a.def, a.level)).chain(h.kit.passives.iter().copied()).collect();
    let mut lines: Vec<Line> = build
        .iter()
        .filter(|&&(d, _)| book().defs[d].kind == Kind::Weapon)
        .map(|&(d, level)| {
            let x = tally.dealt(Some(d));
            Line { id: book().defs[d].id.clone(), level, raw: x.raw, useful: x.useful, kills: x.kills, held: x.held }
        })
        .collect();
    let x = tally.dealt(None);
    lines.push(Line { id: "grenades".into(), level: 0, raw: x.raw, useful: x.useful, kills: x.kills, held: x.held });
    Night {
        secs: h.t,
        dawn: h.t >= dawn(),
        fell: tally.fell,
        kills: h.kills,
        hearts: tally.hearts,
        level: h.level,
        spawned: h.next_id.saturating_sub(1),
        capped,
        minutes,
        lines,
        build,
        took,
    }
}

/// Zombies further than this from the player don't sway the avoiding walk, in world pixels.
const AVOID_REACH: f32 = 240.0;
/// How far along each way it might take the avoiding walk tries walking, sliding on walls as the player does.
const AVOID_WALL: f32 = 40.0;
/// How hard one zombie a pixel away weighs against a way, and the most all of them can.
const AVOID_DANGER: f32 = 20.0;
const AVOID_DANGER_MOST: f32 = 10.0;

/// The turn, -1 to 1, toward the open way that leads least toward the zombies on the map as a player reads it:
/// each where the view ahead last saw it, fading as it's forgotten, the nearer the stronger. It never stops walking,
/// would rather veer than turn back, and keeps off the farm's edge.
fn avoid(h: &Harvest) -> f32 {
    let near: Vec<([f32; 2], f32)> = h
        .marks
        .iter()
        .filter_map(|m| {
            let d = sub(m.pos, h.pos);
            let r = len(d).max(4.0);
            (r < AVOID_REACH).then(|| ([d[0] / r, d[1] / r], m.fade() * (1.0 - r / AVOID_REACH) * AVOID_DANGER / r))
        })
        .collect();
    let half = crate::tuning::tuning().player_half();
    let edge = [h.map.width_px() - TILE, h.map.height_px() - TILE];
    let inside = |p: [f32; 2]| p[0] > TILE && p[1] > TILE && p[0] < edge[0] && p[1] < edge[1];
    let middle = sub([edge[0] / 2.0, edge[1] / 2.0], h.pos);
    let back = if inside(h.pos) { [0.0; 2] } else { [middle[0] / len(middle), middle[1] / len(middle)] };
    // How far it gets along `dir`, as a share of AVOID_WALL, and how far its first step gets.
    let walk = |dir: [f32; 2]| {
        let (mut p, mut first) = (h.pos, 0.0);
        for s in 0..8 {
            let next = h.map.slide(p, [dir[0] * AVOID_WALL / 8.0, dir[1] * AVOID_WALL / 8.0], half);
            if s == 0 {
                first = len(sub(next, h.pos)) * 8.0 / AVOID_WALL;
            }
            if !inside(next) && inside(h.pos) {
                return (0.0, 0.0);
            }
            p = next;
        }
        let d = sub(p, h.pos);
        ((d[0] * dir[0] + d[1] * dir[1]) / AVOID_WALL, first)
    };
    let best = (-7..=8)
        .map(|j| {
            let off = j as f32 * PI / 8.0;
            let dir = [(h.view_yaw + off).cos(), (h.view_yaw + off).sin()];
            let danger: f32 = near.iter().map(|(u, w)| w * (dir[0] * u[0] + dir[1] * u[1]).max(0.0).powi(4)).sum();
            let (free, first) = walk(dir);
            let score = off.cos() - danger.min(AVOID_DANGER_MOST) + 3.0 * (dir[0] * back[0] + dir[1] * back[1]) + 2.0 * free - if first < 0.5 { 100.0 } else { 0.0 };
            (off, score)
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(0.0, |b| b.0);
    (best * 3.0).clamp(-1.0, 1.0)
}

/// Takes a card from the level-up on show by `pick`'s strategy: what it took, or None for `Pick::None`.
fn choose(h: &mut Harvest, pick: &Pick, rng: &mut Rng, sounds: &mut Vec<Sfx>) -> Option<Took> {
    let Phase::LevelUp { mut choices, t, .. } = h.phase else { return None };
    let i = match pick {
        Pick::None => return None,
        Pick::Random => rng.below(choices.len()),
        Pick::Held => stack(h, &mut choices, t, sounds),
        Pick::First(order) => {
            let first = |cs: &[Choice; 3]| cs.iter().enumerate().filter_map(|(i, c)| order.iter().position(|&o| o == c.up).map(|r| (r, i))).min().map(|(_, i)| i);
            if first(&choices).is_none() && h.rerolls > 0 {
                h.reroll(&mut choices, t, sounds);
            }
            match first(&choices) {
                Some(i) => i,
                None => stack(h, &mut choices, t, sounds),
            }
        }
    };
    let c = choices[i];
    if *pick != Pick::Random && h.kit.level(c.up) > 0 && book().defs[c.up].kind == Kind::Weapon {
        h.favor(c.up);
    }
    h.take(c, sounds);
    Some(Took { secs: h.t, up: c.up, rare: c.rare })
}

/// A card for a powerup the player holds, rare first; with none, it rerolls, then banishes a card it doesn't hold,
/// while it can, and failing that takes a new one, rare first.
fn stack(h: &mut Harvest, choices: &mut [Choice; 3], t: f32, sounds: &mut Vec<Sfx>) -> usize {
    let held = |h: &Harvest, c: &Choice| h.kit.level(c.up) > 0;
    while !choices.iter().any(|c| held(h, c)) {
        if h.rerolls > 0 {
            h.reroll(choices, t, sounds);
        } else if let Some(i) = choices.iter().position(|c| h.banishes > 0 && book().defs[c.up].kind != Kind::Filler) {
            h.banish(choices, i, t, sounds);
        } else {
            break;
        }
    }
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
    fn an_undying_player_lasts_the_night_where_a_mortal_one_falls() {
        let unarmed = |mortal| night(&Plan { mortal, difficulty: Difficulty::Hard, until: Some(240.0), throw_every: 0.0, ..Plan::new(Vec::new()) });
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
        let first = play("first:running_shoes+knife");
        let shoes = book().index("running_shoes").unwrap();
        assert_eq!(first.took[0].up, shoes, "the first card on the list comes first when a reroll finds it");
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
        h.pos = [20.5 * TILE, 20.5 * TILE];
        h.view_yaw = 0.0;
        assert_eq!(avoid(&h), 0.0, "nothing seen, it walks straight on");
        let mut z = super::super::Zombie::new(super::super::Kind::common(), [h.pos[0] + 30.0, h.pos[1] + 4.0], 1000.0, 1.0, 0.0);
        z.id = 9;
        h.zombies = vec![z];
        h.remember(1.0 / 60.0);
        assert!(avoid(&h) < 0.0, "a zombie ahead and to the right turns it left");
        h.marks.clear();
        h.view_yaw = PI;
        h.remember(1.0 / 60.0);
        assert!(h.marks.is_empty() && avoid(&h) == 0.0, "one behind it hasn't seen doesn't sway it");
    }
}
