//! The DPS sandbox: a whole night played headless at a fixed step by a scripted player with a forced build, and
//! what each weapon in it did. The script never aims: it walks a fixed pattern, turns round on a schedule, lobs a
//! grenade straight ahead on a timer and takes no level-up card.

use std::f32::consts::TAU;

use super::powerups::{Kind, book};
use super::weapons::Kit;
use super::{Harvest, Phase, dawn};
use crate::{Buttons, Difficulty, Input};

/// How the scripted player walks: always ahead, turning steadily one way, or one way a loop then the other.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Walk {
    #[default]
    Circle,
    Eight,
}

impl Walk {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "circle" => Some(Walk::Circle),
            "eight" => Some(Walk::Eight),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Walk::Circle => "circle",
            Walk::Eight => "eight",
        }
    }
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
    /// How hard the walk turns, as a share of the turn buttons' full speed.
    pub turn: f32,
    /// Seconds between turnarounds, and between grenades; 0 for never.
    pub turn_every: f32,
    pub throw_every: f32,
    /// The player can die, which ends the night.
    pub mortal: bool,
    /// Seconds into the night to stop at, if before dawn.
    pub until: Option<f32>,
    pub step: f32,
}

impl Plan {
    pub fn new(build: Vec<(usize, u32)>) -> Self {
        Self { seed: 1, difficulty: Difficulty::Normal, size: super::MAP_SIZE, build, walk: Walk::Circle, turn: 0.25, turn_every: 6.0, throw_every: 12.0, mortal: false, until: None, step: 1.0 / 60.0 }
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
    let end = plan.until.map_or(dawn(), |u| u.min(dawn()));
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
        let way = match plan.walk {
            Walk::Eight if (t / loop_secs) as u32 % 2 == 1 => -1.0,
            _ => 1.0,
        };
        input.look = plan.turn * way;
        if ever(plan.turn_every, t) > turned {
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
    let mut lines: Vec<Line> = plan
        .build
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
    }
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
}
