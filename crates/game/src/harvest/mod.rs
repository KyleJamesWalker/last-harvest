//! Last Harvest: survive the night on a randomly generated farm while the horde closes in.

mod carve;
mod draw;
mod effects;
mod enemies;
mod juice;
mod map;
mod props;
mod players;
mod powerups;
pub mod showcase;
mod single;
mod view3d;
mod weapons;

use std::f32::consts::{PI, TAU};

use crate::art::{self, Art};
use crate::rng::Rng;
use crate::{Buttons, Color, Difficulty, Input, SCREEN_H, SCREEN_W, Sfx};
use enemies::Enemy;
pub(crate) use enemies::{install_cards, off_from_ids, off_ids, roster};
pub use props::{MAX_FRAME as PROP_MAX_FRAME, MAX_FRAMES as PROP_MAX_FRAMES, Named as NamedProp, PREFIX as PROP_PREFIX, ByViews as PropByViews, PropFields, load_for_checks as load_props_for_checks, turned_depth, prop_fields, prop_text, props as named_props, set_props};
pub(crate) use props::stand_of;
pub use map::{default_depth as default_prop_depth, ground_codes as map_ground, MAP_EXTENSION, MAP_MAX, MAP_MIN, MAX_HP as PROP_MAX_HP, MapSource, map_props, parse_props, PropKind, PropLine, Tough as PropTough, check_map, valid_map_id};
pub use enemies::{ABOUT_MAX, MAX_FRAME as CARD_MAX_FRAME, MAX_WALK, flash_pixel, frame_limits, hurt_pixel};
pub use players::{BASE_KEYS as PLAYER_BASE_KEYS, ID_MAX as PLAYER_ID_MAX, PlayerFields, PlayerLook, default_start, looks as player_looks, player_fields, set_player_start, weapon_slots, weapons};
pub use enemies::{CardFields, FLAGS as CARD_FLAGS, NUMBERS as CARD_NUMBERS, SpawnMix, card_fields, set_card_field, spawn_mix};
use juice::{Corpse, Juice, Particle, Tone};
use map::{Map, TILE};
pub use powerups::{Progress, Run, found_from_ids, found_ids};
use powerups::{Kind as Card, book};
use weapons::{Bolt, Decoy, Dog, Kit, Patch, Swing};

/// Screen pixels per art pixel on the bottom screen's map, which shows 512x384 art pixels.
const SCALE: f32 = 2.0;
const VIEW_W: f32 = SCREEN_W / SCALE;
const VIEW_H: f32 = SCREEN_H / SCALE;
/// Seconds for the quick 180-degree turn.
const SPIN_TIME: f32 = 0.2;
pub const MAP_SIZE: [usize; 2] = [72, 54];
/// How long a night lasts, in minutes, as `tuning.txt` says.
pub fn night_minutes() -> f32 {
    dawn() / 60.0
}

/// Seconds from dusk to dawn.
fn dawn() -> f32 {
    crate::tuning::tuning().night
}

/// Where zombies rise around the player at dusk and at dawn, nearest and furthest, in world pixels.
fn spawn_rings() -> ([f32; 2], [f32; 2]) {
    let t = crate::tuning::tuning();
    ([t.dusk_near, t.dusk_far], [t.dawn_near, t.dawn_far])
}
const MAX_STAINS: usize = 160;
const FLASH_TIME: f32 = 0.08;
/// Seconds the ground churns before a zombie breaks out.
const TELL_TIME: f32 = 0.6;
/// Seconds a struck zombie shows its red frame.
const HURT_FRAMES: f32 = 0.18;
/// Real seconds of slow motion between filling the bar and choosing an upgrade.
const LEVEL_SLOW: f32 = 0.7;
/// Seconds after a pick before play resumes, so the choice can land.
const PICK_TIME: f32 = 0.3;
/// Seconds the upgrade cards ignore input while they slide in.
const CARDS_IN: f32 = 0.35;
/// Seconds into a death: the player drops, sinks into the dirt, then claws back out as one of them.
const TURN_FALL: f32 = 0.4;
const TURN_SINK: f32 = 1.2;
const TURN_RISE: f32 = 1.6;
const TURNED: f32 = 2.6;
/// How fast the turned player and the dead with them walk off, and how near they gather around the player.
const MARCH_SPEED: f32 = 16.0;
const MARCH_CLOSE: f32 = 40.0;
const MAX_BIRDS: usize = 12;
const WHITE: Color = [1.0; 4];
const GOLD: Color = [1.0, 0.85, 0.3, 1.0];
const RED: Color = [0.85, 0.2, 0.15, 1.0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Facing {
    Front,
    Side,
    Back,
}

impl Facing {
    /// Facing for a direction, and whether the left-facing side frames need flipping.
    fn of(d: [f32; 2]) -> (Facing, bool) {
        if d[0].abs() > d[1].abs() {
            (Facing::Side, d[0] < 0.0)
        } else if d[1] < 0.0 {
            (Facing::Back, false)
        } else {
            (Facing::Front, false)
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// Walk frames for each facing, as many as its cycle has, then the flash and hurt frames for each facing.
#[derive(Clone)]
pub(crate) struct Look {
    pub walk: [Vec<Art>; 3],
    pub flash: [Art; 3],
    pub hurt: [Art; 3],
    /// Dead, per facing, tipped over as a corpse; without it a corpse is the front hurt frame.
    pub dead: Option<[Art; 3]>,
}

fn player_look() -> Look {
    Look { walk: [art::PLAYER_FRONT.to_vec(), art::PLAYER_SIDE.to_vec(), art::PLAYER_BACK.to_vec()], flash: art::PLAYER_FLASH, hurt: art::PLAYER_HURT, dead: None }
}

/// A kind of zombie: its place in `enemies.txt`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Kind(u8);

impl Kind {
    fn stats(self) -> &'static Enemy {
        &roster().enemies[self.0 as usize]
    }

    fn common() -> Self {
        Kind(roster().common() as u8)
    }

    #[cfg(test)]
    fn named(id: &str) -> Self {
        Kind(roster().index(id).unwrap_or_else(|| panic!("no enemy {id}")) as u8)
    }
}

/// The death sound of the zombie kind a `Sfx::Kill` names.
pub fn kill_sound(kind: u8) -> &'static str {
    roster().enemies.get(kind as usize).map_or("", |e| e.sound.as_str())
}

/// Checks a pack's own powerups.txt, enemies.txt and props.txt as the game will read them, without reading the
/// game's data, so a mod that would break the game can be left out before it's installed.
pub fn check_tables(powerups: Option<&str>, enemies: Option<&str>, props: Option<&str>) -> Result<(), String> {
    if let Some(text) = props {
        map::parse_props(text).map_err(|e| format!("props.txt:{e}"))?;
    }
    if let Some(text) = powerups {
        powerups::parse(text).map_err(|e| format!("powerups.txt:{e}"))?;
    }
    if let Some(text) = enemies {
        enemies::parse(text).map_err(|e| format!("enemies.txt:{e}"))?;
    }
    Ok(())
}

/// The data files the game reads its rules from, which the editors change and a mod can replace.
pub const TABLES: [&str; 7] = ["tuning.txt", "props.txt", "ground.txt", "farm.txt", "effects.txt", "powerups.txt", "enemies.txt"];

/// A data file as the game has it built in.
pub fn built_in_table(file: &str) -> Option<&'static str> {
    match file {
        "tuning.txt" => Some(include_str!("../../tuning.txt")),
        "props.txt" => Some(include_str!("../../props.txt")),
        "effects.txt" => Some(include_str!("../../effects.txt")),
        "ground.txt" => Some(include_str!("../../ground.txt")),
        "farm.txt" => Some(include_str!("../../farm.txt")),
        "powerups.txt" => Some(include_str!("../../powerups.txt")),
        "enemies.txt" => Some(include_str!("../../enemies.txt")),
        _ => None,
    }
}

/// Checks a data file's text as the game will read it; errors name the line.
pub fn check_table(file: &str, text: &str) -> Result<(), String> {
    let fail = |e: String| format!("{file}:{e}");
    match file {
        "tuning.txt" => crate::tuning::parse(text, Some(&crate::tuning::built_in())).map(|_| ()).map_err(fail),
        "props.txt" => map::parse_props(text).map(|_| ()).map_err(fail),
        "effects.txt" => effects::parse(text).and_then(|_| effects::parse_colors(text, Some(&effects::built_in_colors()))).map(|_| ()).map_err(fail),
        "ground.txt" => map::parse_ground(text).map(|_| ()).map_err(fail),
        "farm.txt" => map::parse_farm(text).map(|_| ()).map_err(fail),
        "powerups.txt" => powerups::parse(text).map(|_| ()).map_err(fail),
        "enemies.txt" => enemies::parse(text).map(|_| ()).map_err(fail),
        _ => Err(format!("{file} isn't one of the game's data files")),
    }
}

/// Checks a data file's text as the game's own, built in: it must set every number and name only sprites in
/// art.txt, since nothing lies under it. Errors name the line, or the sprite.
pub fn check_own_table(file: &str, text: &str) -> Result<(), String> {
    check_table(file, text)?;
    let fail = |e: String| format!("{file}:{e}");
    let missing = |name: &str| !art::ALL.iter().any(|(n, _)| *n == name);
    match file {
        "tuning.txt" => crate::tuning::parse(text, None).map(|_| ()).map_err(fail),
        "effects.txt" => {
            effects::parse_colors(text, None).map_err(fail)?;
            let (own, extra) = effects::parse(text).map_err(fail)?;
            match own.iter().chain(extra.iter().map(|(_, w)| w)).find(|w| missing(&w.art)) {
                Some(w) => Err(format!("{file}: no sprite called {} in art.txt", w.art)),
                None => Ok(()),
            }
        }
        "props.txt" => match map::parse_props(text).map_err(fail)?.props.iter().find(|k| missing(k.name)) {
            Some(k) => Err(format!("{file}: no sprite called {} in art.txt", k.name)),
            None => Ok(()),
        },
        "powerups.txt" => match text.lines().filter_map(|l| l.split('#').next()?.trim().strip_prefix("icon")).map(str::trim).find(|n| missing(n)) {
            Some(name) => Err(format!("{file}: no sprite called {name} in art.txt")),
            None => Ok(()),
        },
        "farm.txt" => {
            let mut section = "";
            for line in text.lines().map(|l| l.split('#').next().unwrap_or("").trim()).filter(|l| !l.is_empty()) {
                if let Some(s) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                    section = s;
                    continue;
                }
                let words: Vec<&str> = line.split_whitespace().collect();
                let names = match (section, &words[..]) {
                    ("farm", _) => &[][..],
                    ("scatter", [name, ..]) => std::slice::from_ref(name),
                    (_, ["sprites", names @ ..]) => names,
                    (_, ["near", name, _, ..]) => std::slice::from_ref(name),
                    _ => &[][..],
                };
                if let Some(name) = names.iter().find(|n| missing(n)) {
                    return Err(format!("{file}: no sprite called {name} in art.txt"));
                }
            }
            let farm = map::parse_farm(text).map_err(fail)?;
            let codes = map::ground_codes();
            match [farm.ground, farm.tufted, farm.patch].into_iter().find(|c| !codes.iter().any(|(k, _)| k == c)) {
                Some(c) => Err(format!("{file}: no ground code {} in ground.txt", c as char)),
                None => Ok(()),
            }
        }
        _ => Ok(()),
    }
}

/// How heavy the zombie kind a `Sfx::Kill` names is, which divides its knockback; 1 for a kind that's gone.
pub fn kill_weight(kind: u8) -> f32 {
    roster().enemies.get(kind as usize).map_or(1.0, |e| e.mass)
}

/// Every zombie kind's death sound.
pub fn kill_sounds() -> impl Iterator<Item = &'static str> {
    roster().enemies.iter().map(|e| e.sound.as_str())
}

struct Zombie {
    kind: Kind,
    pos: [f32; 2],
    /// Direction of travel, for picking frames in the 3D view.
    dir: [f32; 2],
    /// How far out of the ground, 0 to 1.
    rise: f32,
    push: [f32; 2],
    hp: f32,
    flash: f32,
    /// Seconds left of the red frame.
    hurt: f32,
    /// 1 just after a hit, easing back to 0: squashes and leans the sprite.
    squash: f32,
    /// Direction of the last hit, which a dying zombie falls along.
    knock: [f32; 2],
    /// Speed a blast throws the body at, if a blast killed it.
    fling: f32,
    /// Seconds left rooted by a heavy blow; only big zombies stagger, the rest are knocked back.
    stagger: f32,
    facing: (Facing, bool),
    anim: f32,
    /// Tells zombies apart from one frame to the next, for crows and dogs.
    id: u32,
    /// Seconds left slowed, and the fraction of speed taken away.
    slow: f32,
    slow_k: f32,
    /// Seconds left burning, the damage a second, and time since the last burn tick.
    burn: f32,
    burn_dps: f32,
    burn_tick: f32,
    /// A horde's leader, which drops a chest.
    leader: bool,
    /// Dawn's Harvester: faster, heavier, harder biting, and drawn twice the size.
    boss: bool,
}

impl Zombie {
    fn new(kind: Kind, pos: [f32; 2], hp: f32, rise: f32, anim: f32) -> Self {
        Self {
            kind,
            pos,
            dir: [0.0, 1.0],
            rise,
            push: [0.0; 2],
            hp,
            flash: 0.0,
            hurt: 0.0,
            squash: 0.0,
            knock: [0.0; 2],
            fling: 0.0,
            stagger: 0.0,
            facing: (Facing::Front, false),
            anim,
            id: 0,
            slow: 0.0,
            slow_k: 0.0,
            burn: 0.0,
            burn_dps: 0.0,
            burn_tick: 0.0,
            leader: false,
            boss: false,
        }
    }

    /// The frame showing now, for a facing.
    fn art(&self, facing: Facing) -> Art {
        let look = &self.kind.stats().look;
        if self.flash > 0.0 {
            look.flash[facing.index()]
        } else if self.hurt > 0.0 || self.stagger > 0.0 {
            look.hurt[facing.index()]
        } else {
            let walk = &look.walk[facing.index()];
            walk[self.step() % walk.len()]
        }
    }
}

/// A zombie about to break out of the ground.
struct Tell {
    kind: Kind,
    pos: [f32; 2],
    hp: f32,
    t: f32,
    leader: bool,
}

/// A crow pecking at the fields until something startles it.
struct Bird {
    pos: [f32; 2],
    z: f32,
    vel: [f32; 2],
    vz: f32,
    flying: bool,
    anim: f32,
    flip: bool,
}

impl Bird {
    fn art(&self) -> Art {
        if self.flying || self.z > 0.0 {
            effects::effects().crow.looping(self.anim)
        } else {
            // Mostly standing, with a peck now and then.
            let k = (self.anim * 5.0) as usize % 14;
            art::CROW_PECK[k.saturating_sub(9).min(art::CROW_PECK.len() - 1)]
        }
    }
}

/// Buzzes the motor; stronger kinds win when several land on one frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rumble {
    Tick,
    Fanfare,
    Hit,
    Blast,
}

struct Bullet {
    pos: [f32; 2],
    vel: [f32; 2],
    damage: f32,
    pierce: u32,
    life: f32,
    force: f32,
    hit: Vec<u32>,
    /// The effect it flies as, when its weapon names one.
    look: Option<effects::Effect>,
    age: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DropKind {
    Money(u32),
    Health,
    /// A horde leader's: a free level-up, holding a find when one is waiting.
    Chest,
}

struct Drop {
    kind: DropKind,
    pos: [f32; 2],
    /// Height of the hop it makes when it falls out of a zombie.
    z: f32,
    vz: f32,
    vel: [f32; 2],
    age: f32,
    /// Seconds since the magnet caught it.
    pulled: Option<f32>,
}

/// What's in the air.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Thrown {
    Grenade,
    /// A small one a grenade belt scatters.
    Bomblet,
    Molotov { dps: f32, radius: f32, life: f32 },
    Flare { dps: f32, radius: f32, life: f32 },
    /// A `does lob` weapon's throw, by its place in the powerup book.
    Lob { def: usize, damage: f32, radius: f32, life: f32, slow: f32 },
}

/// A grenade or the like in the air from `from` to `to`, `t` seconds after the throw.
struct Grenade {
    from: [f32; 2],
    to: [f32; 2],
    t: f32,
    kind: Thrown,
}

impl Grenade {
    fn art(&self) -> Art {
        match self.kind {
            Thrown::Grenade | Thrown::Bomblet => effects::effects().grenade.looping(self.t),
            Thrown::Molotov { .. } => effects::effects().molotov.looping(self.t),
            Thrown::Flare { .. } => effects::effects().flare.looping(self.t),
            Thrown::Lob { def, .. } => {
                let named = powerups::book().defs[def].bullet.as_deref().and_then(|n| effects::effects().named(n));
                named.unwrap_or(effects::effects().grenade).looping(self.t)
            }
        }
    }

    /// Ground position and height above it, in world pixels.
    fn at(&self) -> ([f32; 2], f32) {
        let k = (self.t / crate::tuning::tuning().grenade_flight).min(1.0);
        let p = [self.from[0] + (self.to[0] - self.from[0]) * k, self.from[1] + (self.to[1] - self.from[1]) * k];
        (p, (k * PI).sin() * 28.0)
    }
}

/// How much brighter a prop draws while it flashes from a hit.
const PROP_FLASH: f32 = 3.0;

struct Barrel {
    /// Top-left in world pixels; the barrel fills one cell.
    pos: [f32; 2],
    hp: f32,
    flash: f32,
}

impl Barrel {
    fn center(&self) -> [f32; 2] {
        [self.pos[0] + TILE / 2.0, self.pos[1] + TILE / 2.0]
    }
}

impl Zombie {
    /// How many walk frames it has stepped through; each facing's cycle wraps at its own length.
    fn step(&self) -> usize {
        (self.anim * crate::tuning::tuning().walk_fps * self.kind.stats().speed / 20.0) as usize
    }
}

/// The player's frame for a facing: flashing while invulnerable, red just after a hit, else walking.
fn art_for(h: &Harvest, look: &Look, facing: Facing) -> Art {
    let i = facing.index();
    if h.hurt > 0.0 && ((h.hurt * 20.0) as u32).is_multiple_of(2) {
        look.flash[i]
    } else if h.hurt > h.difficulty.hurt_time() - 0.2 {
        look.hurt[i]
    } else {
        look.walk[i][if h.walk > 0.0 { 1 + (h.walk * crate::tuning::tuning().walk_fps) as usize % 2 } else { 0 }]
    }
}

/// The player's frame for a facing, and how far out of the ground it stands, 0 to 1. Once dead, it sinks
/// and climbs back out as a zombie.
fn player_pose(h: &Harvest, facing: Facing) -> (Art, f32) {
    let look = &players::look(&h.look).look;
    let Phase::Over { t, won: false } = h.phase else { return (art_for(h, look, facing), 1.0) };
    let i = facing.index();
    if t < TURN_RISE {
        (look.dead.map_or(look.hurt[i], |d| d[i]), 1.0 - ((t - TURN_FALL) / (TURN_SINK - TURN_FALL)).clamp(0.0, 1.0))
    } else {
        let walk = &Kind::common().stats().look.walk[i];
        (walk[(t * crate::tuning::tuning().walk_fps * 0.6) as usize % walk.len()], ((t - TURN_RISE) / (TURNED - TURN_RISE)).min(1.0))
    }
}

/// A short animation in world space.
struct Fx {
    frames: &'static [Art],
    fps: f32,
    pos: [f32; 2],
    angle: f32,
    t: f32,
}

impl Fx {
    fn play(e: effects::Effect, pos: [f32; 2], angle: f32) -> Self {
        Fx { frames: e.frames, fps: e.fps, pos, angle, t: 0.0 }
    }

    fn done(&self) -> bool {
        self.t * self.fps >= self.frames.len() as f32
    }
}

/// An upgrade on offer; a rare one counts twice, and a found one unlocks for good when taken.
#[derive(Clone, Copy, Debug)]
struct Choice {
    /// Which powerup, by its place in the book.
    up: usize,
    rare: bool,
    found: bool,
}

#[derive(Clone, Copy)]
enum Phase {
    Playing,
    /// `t` counts up from when the cards appear; `picked` from when one was chosen.
    LevelUp { choices: [Choice; 3], selected: usize, t: f32, picked: Option<f32> },
    Over { t: f32, won: bool },
}

/// What the run asks of the rest of the game after an update.
#[derive(Default)]
pub struct Flow {
    pub rumble: Option<Rumble>,
    pub quit: bool,
    /// The pause menu chose Settings; the run stays paused under it.
    pub settings: bool,
    /// How the run went, on the frame it ends.
    pub ended: Option<Run>,
    /// Powerups found this frame, to keep even if the run is abandoned.
    pub found: u64,
}

pub struct Harvest {
    rng: Rng,
    map: Map,
    /// The hand-made map this run is on, by id; empty for a generated farm.
    map_id: String,
    t: f32,
    phase: Phase,
    pos: [f32; 2],
    hp: f32,
    hurt: f32,
    facing: (Facing, bool),
    aim: [f32; 2],
    walk: f32,
    kit: Kit,
    zombies: Vec<Zombie>,
    bullets: Vec<Bullet>,
    drops: Vec<Drop>,
    fx: Vec<Fx>,
    /// Blood and scorch marks: art, top-left, and a tint.
    stains: Vec<(Art, f32, f32, [f32; 3])>,
    barrels: Vec<Barrel>,
    /// Where barrels stood when the farm was made, which fuel drums fill again.
    barrel_spots: Vec<[f32; 2]>,
    /// Seconds toward the next barrel coming back.
    drum_t: f32,
    patches: Vec<Patch>,
    swings: Vec<Swing>,
    bolts: Vec<Bolt>,
    dogs: Vec<Dog>,
    decoys: Vec<Decoy>,
    /// Zombies a crow pecked lately, by id, and the seconds before it can peck them again.
    pecked: Vec<(u32, f32)>,
    /// Seconds left of the lantern beam's flare.
    beam: f32,
    next_id: u32,
    /// Money's worth of XP not yet a whole point, from a pawn shop's share.
    xp_frac: f32,
    revived: u32,
    /// What earlier runs unlocked, what this one found, and how many more it made findable.
    progress: Progress,
    /// Zombie kinds the settings turned off, by place in the roster.
    enemies_off: u64,
    /// The id of the player's look; the farmer's is empty.
    look: String,
    newly: Vec<usize>,
    ready: usize,
    found_now: u64,
    /// The next level-up comes from a chest, and the chests picked up whose level-up hasn't begun.
    chest: bool,
    chests: u32,
    /// The next zombie to rise leads the horde.
    leader_owed: bool,
    /// What dawn has multiplied every zombie's health and bite by, and the time toward its next step.
    fury: f32,
    fury_tick: f32,
    /// Powerups every run on this farm starts with, by place in the book and level, for trying them out.
    trial: Vec<(usize, u32)>,
    /// Seconds into the night every run on this farm starts at.
    start_t: f32,
    /// The Harvester has risen this run.
    woke: bool,
    /// Grenades ready to throw; the fraction is the next one recharging.
    grenades: f32,
    thrown: Vec<Grenade>,
    owed: f32,
    xp: u32,
    next_xp: u32,
    level: u32,
    kills: u32,
    /// Holding L1 pauses and shows the whole map.
    overview: bool,
    /// Steps to the player from each cell, rebuilt when the player changes cell.
    flow: Vec<u16>,
    flow_cell: [usize; 2],
    sep: Vec<[f32; 2]>,
    steps: map::Steps,
    /// The player's heading, which the 3D camera looks along, in radians from +x toward +y.
    view_yaw: f32,
    /// Radians left of a quick turn in progress.
    spin: f32,
    /// The heading the player keeps while B swings the camera round, so it sweeps past their front.
    turned_from: [f32; 2],
    /// The pause menu's highlighted choice, while Start has the run paused.
    menu: Option<usize>,
    /// X is held: aimed weapons go for explosive barrels in view first.
    targeting: bool,
    /// False until the turn buttons are let go, so the A that started the run or picked an upgrade doesn't turn.
    turn_ready: bool,
    /// No zombies: a walk around the farm, for looking at it.
    calm: bool,
    /// A frozen swarm with every effect showing, turning in place, for measuring frame rate.
    bench: bool,
    juice: Juice,
    tells: Vec<Tell>,
    birds: Vec<Bird>,
    /// Whether a shot or blast went off this frame, which startles birds.
    noise: Option<[f32; 2]>,
    /// The XP bar as drawn, catching up with `xp`.
    xp_shown: f32,
    /// The health bar's trailing chunk, catching down to `hp`.
    hp_shown: f32,
    /// Seconds since the last money pickup, and how many came in a row.
    chain: (f32, u32),
    /// Real seconds of slow motion left before the upgrade cards appear.
    leveling: f32,
    /// Seconds since the kill count last went up, for its bump.
    kill_bump: f32,
    /// Cooldowns on sounds that would otherwise stack up in a swarm: hits, kills, groans, rising, birds.
    hush: [f32; 5],
    heartbeat: f32,
    /// Seconds of the full-screen gold flash when the cards appear.
    glory: f32,
    /// A tap that landed during hit-stop, thrown when the world moves again.
    queued_touch: Option<[f32; 2]>,
    /// Sideways input, eased, which tilts the camera into a strafe.
    lean: f32,
    /// A sprite the editor is showing off.
    shown: Option<showcase::Shown>,
    difficulty: Difficulty,
    /// One screen: the map shows on the top screen only while looked at.
    single: bool,
    aiming: Option<single::Aiming>,
    /// The map is up on the one screen without a grenade being aimed.
    glance: bool,
    /// A thrown grenade is never spent.
    endless: bool,
    /// The last single-screen throw's angle off the view and distance from the player, and real seconds since.
    last_throw: Option<[f32; 2]>,
    since_throw: f32,
}

/// Weapons and passives unlocked so far, out of all of them, and how many are waiting to be found.
pub fn unlocked(p: &Progress) -> (usize, usize, usize) {
    let all: Vec<usize> = (0..book().defs.len()).filter(|&i| book().defs[i].kind != Card::Filler).collect();
    (all.iter().filter(|&&i| p.owns(i)).count(), all.len(), all.iter().filter(|&&i| p.findable(i)).count())
}

/// The powerup closest to becoming findable, and what it takes.
pub fn next_unlock(p: &Progress) -> Option<(String, String)> {
    book()
        .defs
        .iter()
        .enumerate()
        .filter(|&(i, d)| !p.owns(i) && !d.unlock.met(p))
        .map(|(_, d)| d)
        .min_by(|a, b| a.unlock.effort(p).total_cmp(&b.unlock.effort(p)))
        .map(|d| (d.name.clone(), d.unlock.text()))
}

fn len(v: [f32; 2]) -> f32 {
    (v[0] * v[0] + v[1] * v[1]).sqrt()
}

fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn norm(v: [f32; 2]) -> [f32; 2] {
    let l = len(v);
    if l > 1e-4 { [v[0] / l, v[1] / l] } else { [0.0, 0.0] }
}

impl Harvest {
    /// What a keyboard's Esc stands for: Start to pause a run in play, else B to back out of what is open.
    pub fn escape(&self) -> Buttons {
        if self.menu.is_none() && matches!(self.phase, Phase::Playing) { Buttons::START } else { Buttons::B }
    }

    /// The player's position and the solid cells and props around them, for the debug pipe's `state`.
    pub fn debug_state(&self) -> String {
        let half = crate::tuning::tuning().player_half();
        let [px, py] = self.pos;
        let (tx, ty) = ((px / TILE) as i32, (py / TILE) as i32);
        let mut s = format!("pos {px:.1},{py:.1} tile {tx},{ty} half {half:?} yaw {:.2}\n", self.view_yaw);
        let r = 4;
        for y in ty - r..=ty + r {
            s += &format!("{y:4} ");
            for x in tx - r..=tx + r {
                let solid = self.map.solid_at(x as f32 * TILE + 1.0, y as f32 * TILE + 1.0);
                s.push(if (x, y) == (tx, ty) { 'P' } else if solid { '#' } else { '.' });
            }
            s.push('\n');
        }
        let name = |a: art::Art| art::ALL.iter().find(|(_, f)| f.contains(&a)).map_or("?", |(n, _)| n);
        for p in &self.map.props {
            let [cx, cy, cw, ch] = p.cells().map(|v| v as i32);
            if cx + cw >= tx - r && cx <= tx + r && cy + ch >= ty - r && cy <= ty + r {
                let a = p.frames[0];
                s += &format!("{} at px {},{} size {}x{} blocks tiles x {}..{} y {}..{}\n", name(a), p.x, p.y, a.w(), a.h(), cx, cx + cw, cy, cy + ch);
            }
        }
        s
    }

    pub fn new(seed: u64) -> Self {
        Self::sized(seed, MAP_SIZE)
    }

    /// A run on the hand-made map `id`, or on a generated farm when no map that reads has that id.
    pub fn on(seed: u64, id: &str) -> Self {
        let mut rng = Rng::new(seed);
        let Some(file) = crate::data::maps().iter().find(|m| m.id == id) else { return Self::new(seed) };
        match Map::from_text(&file.text, &mut rng) {
            Ok(map) => Self { map_id: id.to_string(), ..Self::with_map(seed, rng, map) },
            Err(e) => {
                crate::data::note(format!("{id}{}: {e}, so this run is on a random farm", MAP_EXTENSION));
                Self::new(seed)
            }
        }
    }

    fn sized(seed: u64, [w, h]: [usize; 2]) -> Self {
        let mut rng = Rng::new(seed);
        let map = Map::generate(&mut rng, w, h);
        Self::with_map(seed, rng, map)
    }

    fn with_map(seed: u64, rng: Rng, map: Map) -> Self {
        let kit = Kit::start(Difficulty::Normal);
        let own = crate::tuning::tuning().barrel_hp;
        let barrels = map.barrels.iter().enumerate().map(|(i, &pos)| Barrel { pos, hp: map.barrel_hp.get(i).copied().filter(|h| *h > 0.0).unwrap_or(own), flash: 0.0 }).collect();
        Self {
            barrels,
            barrel_spots: map.barrels.clone(),
            drum_t: 0.0,
            patches: Vec::new(),
            swings: Vec::new(),
            bolts: Vec::new(),
            dogs: Vec::new(),
            decoys: Vec::new(),
            pecked: Vec::new(),
            beam: 0.0,
            next_id: 1,
            xp_frac: 0.0,
            revived: 0,
            progress: Progress::default(),
            enemies_off: 0,
            look: String::new(),
            newly: Vec::new(),
            ready: 0,
            found_now: 0,
            chest: false,
            chests: 0,
            leader_owed: false,
            fury: 1.0,
            fury_tick: 0.0,
            trial: Vec::new(),
            start_t: 0.0,
            woke: false,
            grenades: book().grenades[Difficulty::Normal as usize] as f32,
            thrown: Vec::new(),
            pos: map.spawn,
            hp: kit.max_hp(),
            rng,
            map,
            map_id: String::new(),
            t: 0.0,
            phase: Phase::Playing,
            hurt: 0.0,
            facing: (Facing::Front, false),
            aim: [0.0, 1.0],
            walk: 0.0,
            kit,
            zombies: Vec::new(),
            bullets: Vec::new(),
            drops: Vec::new(),
            fx: Vec::new(),
            stains: Vec::new(),
            owed: 0.0,
            xp: 0,
            next_xp: book().first_level,
            level: 1,
            kills: 0,
            overview: false,
            flow: Vec::new(),
            flow_cell: [usize::MAX; 2],
            sep: Vec::new(),
            steps: map::Steps::default(),
            view_yaw: -PI / 2.0,
            spin: 0.0,
            turned_from: [0.0; 2],
            menu: None,
            targeting: false,
            turn_ready: false,
            calm: false,
            bench: false,
            juice: Juice::new(seed),
            tells: Vec::new(),
            birds: Vec::new(),
            noise: None,
            xp_shown: 0.0,
            hp_shown: 0.0,
            chain: (1.0, 0),
            leveling: 0.0,
            kill_bump: 1.0,
            hush: [0.0; 5],
            heartbeat: 0.0,
            glory: 0.0,
            queued_touch: None,
            lean: 0.0,
            shown: None,
            difficulty: Difficulty::Normal,
            single: false,
            aiming: None,
            glance: false,
            endless: false,
            last_throw: None,
            since_throw: 0.0,
        }
        .with_birds()
    }

    fn with_birds(mut self) -> Self {
        self.hp_shown = self.hp;
        for _ in 0..MAX_BIRDS * 2 / 3 {
            if let Some(pos) = self.perch(80.0) {
                let flip = self.juice.rng.below(2) == 0;
                let anim = self.juice.rng.range(0.0, 3.0);
                self.birds.push(Bird { pos, z: 0.0, vel: [0.0; 2], vz: 0.0, flying: false, anim, flip });
            }
        }
        self
    }

    /// Somewhere open for a bird at least `away` from the player.
    fn perch(&mut self, away: f32) -> Option<[f32; 2]> {
        for _ in 0..20 {
            let p = [self.juice.rng.range(TILE, self.map.width_px() - TILE), self.juice.rng.range(TILE, self.map.height_px() - TILE)];
            if !self.map.hits(p[0], p[1], [2.0, 2.0]) && len(sub(p, self.pos)) > away {
                return Some(p);
            }
        }
        None
    }

    /// The same frames every run: 300 zombies, bodies, stains and specks around the player, frozen, with the
    /// camera turning a full circle every 600 frames, on a generated farm `size` tiles across.
    pub fn bench(size: [usize; 2]) -> Self {
        let [w, hgt] = size.map(|v| v.clamp(16, 400));
        let mut h = Self { calm: true, bench: true, ..Self::sized(7, [w, hgt]) };
        h.birds.clear();
        for i in 0..(crate::tuning::tuning().max_zombies as usize) {
            let a = i as f32 * 2.399;
            let r = 14.0 + (i as f32).sqrt() * 9.0;
            let pos = [h.pos[0] + a.cos() * r, h.pos[1] + a.sin() * r];
            let kinds = roster().enemies.len();
            let mut z = Zombie::new(Kind((kinds - 1 - i % kinds) as u8), pos, 10.0, 1.0, i as f32 * 0.1);
            z.dir = norm(sub(h.pos, pos));
            h.zombies.push(z);
        }
        for i in 0..40 {
            let a = i as f32 * 0.9;
            let at = [h.pos[0] + a.cos() * 30.0, h.pos[1] + a.sin() * 30.0];
            let stain = art::BLOOD_STAIN[i % art::BLOOD_STAIN.len()];
            h.stains.push((stain, at[0], at[1], [1.0; 3]));
            h.juice.corpse(Corpse { art: art::SKINNY_HURT[0], dead: None, flip: false, pos: at, z: 0.0, vel: [0.0; 2], vz: 0.0, fall: [a.cos(), a.sin()], age: 1.0, spin: 0.0 });
            h.juice.burst(at, 8.0, 8, [0.0; 2], 0.0, [10.0, 40.0], juice::blood(), 1.5, 100.0, 0.0);
            h.juice.popup(at, 12.0 + i as f32, if i % 5 == 0 { Tone::Crit } else { Tone::Hit });
            let vel = [a.cos() * 200.0, a.sin() * 200.0];
            h.bullets.push(Bullet { pos: [h.pos[0] + a.cos() * 20.0, h.pos[1] - 6.0 + a.sin() * 20.0], vel, damage: 0.0, pierce: 0, life: 1.0, force: 0.0, hit: Vec::new(), look: None, age: 0.0 });
        }
        h.juice.flash(h.pos, 10.0, [1.2, 0.85, 0.4], 3.5, f32::MAX);
        h
    }

    /// The map's size and prop count and how long a flow field over it takes, which a moving player pays each time
    /// they change tile; `time` runs the work it's given and returns the seconds it took.
    pub fn bench_report(&self, time: impl FnOnce(&mut dyn FnMut()) -> f64) -> String {
        let (mut dist, mut steps) = (Vec::new(), map::Steps::default());
        let secs = time(&mut || {
            for _ in 0..20 {
                self.map.flow_into(self.pos, &mut dist, &mut steps);
                std::hint::black_box(&dist);
            }
        });
        let (w, hgt) = (self.map.width_px() / TILE, self.map.height_px() / TILE);
        format!("bench: {w}x{hgt} tiles, {} props, flow field {:.2} ms", self.map.props.len(), secs * 1000.0 / 20.0)
    }

    pub fn at(self, difficulty: Difficulty) -> Self {
        let kit = Kit::start(difficulty);
        Self { difficulty, hp: kit.max_hp(), grenades: kit.grenades(), kit, ..self }
    }

    /// Offers only what earlier runs have unlocked.
    pub fn with_progress(self, progress: Progress) -> Self {
        Self { progress, ..self }
    }

    pub fn without_enemies(self, off: u64) -> Self {
        Self { enemies_off: off, ..self }
    }

    /// Dresses the player as the look with `id`, or as the farmer when there's none.
    /// Comes after `at`, since the look may start the run with its own weapons for that difficulty.
    pub fn looking_like(self, id: &str) -> Self {
        let mine = players::look(id);
        let own = mine.start_defs(self.difficulty).map(|defs| Kit::with_weapons(&defs, self.difficulty));
        let grenades = own.as_ref().map_or(self.grenades, Kit::grenades);
        let mut kit = own.unwrap_or(self.kit);
        for (b, theirs) in kit.base.iter_mut().zip(mine.base) {
            *b = theirs.unwrap_or(*b);
        }
        Self { look: mine.id.clone(), hp: kit.max_hp(), kit, grenades, ..self }
    }

    #[cfg(test)]
    pub fn enemies_off(&self) -> u64 {
        self.enemies_off
    }

    /// Starts carrying powerups named `id` or `id:level`, comma separated; unknown names are skipped.
    /// Starts the night `secs` in, and every restart on this farm too.
    pub fn endless_grenades(mut self, on: bool) -> Self {
        self.endless = on;
        self
    }

    pub fn starting_at(mut self, secs: f32) -> Self {
        self.t = secs;
        self.start_t = secs;
        self
    }

    pub fn with_kit(mut self, list: &str) -> Self {
        self.trial = list
            .split(',')
            .filter_map(|item| {
                let (id, level) = item.trim().split_once(':').unwrap_or((item.trim(), "1"));
                Some((book().index(id)?, level.parse().unwrap_or(1)))
            })
            .collect();
        for &(def, level) in &self.trial {
            self.kit.raise(def, level);
        }
        self.hp = self.kit.max_hp();
        self.grenades = self.kit.grenades();
        self
    }

    pub fn calm(seed: u64) -> Self {
        Self { calm: true, ..Self::new(seed) }
    }

    /// A calm farm on a hand-made map, built loosely so a half-made one still shows.
    pub(super) fn calm_on(seed: u64, src: &map::MapSource) -> Result<Self, String> {
        let mut rng = Rng::new(seed);
        let map = Map::from_source(src, &mut rng, false)?;
        Ok(Self { calm: true, ..Self::with_map(seed, rng, map) })
    }

    pub fn summary(&self) -> String {
        format!("harvest {} t={:.0} level={} kills={} zombies={} hp={:.0}", self.difficulty.key(), self.t, self.level, self.kills, self.zombies.len(), self.hp)
    }

    pub fn update(&mut self, input: &Input, dt: f32, sounds: &mut Vec<Sfx>) -> Flow {
        let mut flow = Flow::default();
        if self.bench {
            self.view_yaw = (self.view_yaw + TAU / 600.0).rem_euclid(TAU);
            return flow;
        }
        self.overview = input.held.contains(if self.single { Buttons::R2 } else { Buttons::L1 });
        if self.overview {
            return flow;
        }
        let own;
        let (input, dt) = if self.single {
            own = self.single_input(input, dt, sounds);
            (&own.0, own.1)
        } else {
            (input, dt)
        };
        if let Some(at) = self.menu {
            self.pause_menu(at, input, sounds, &mut flow);
            return flow;
        }
        if input.pressed.contains(Buttons::START) && matches!(self.phase, Phase::Playing) {
            self.menu = Some(0);
            sounds.push(Sfx::Tick(0));
            return flow;
        }
        self.juice.tick_real(dt);
        self.glory = (self.glory - dt).max(0.0);
        match self.phase {
            // Taps and the turnaround still count while hit-stop holds the world still.
            Phase::Playing if self.juice.frozen() => {
                if let Some(touch) = input.touch.filter(|_| input.touch_started) {
                    self.queued_touch = Some(touch);
                }
                if input.pressed.contains(Buttons::B) {
                    self.turn_around();
                }
            }
            Phase::Playing => self.play(input, dt, sounds, &mut flow),
            Phase::LevelUp { .. } => self.choose(input, dt, sounds),
            Phase::Over { t: before, won } => {
                let t = before + dt;
                self.phase = Phase::Over { t, won };
                self.juice.tick(if t < TURN_SINK { dt * 0.4 } else { dt });
                if !won && t >= TURNED {
                    self.march(dt);
                }
                if !won && before < TURN_RISE && t >= TURN_RISE {
                    self.juice.burst(self.pos, 0.0, 16, [0.0; 2], 0.0, [20.0, 60.0], juice::dirt(), 1.8, 0.7, 0.0);
                    self.fx.push(Fx::play(effects::effects().player_rises, [self.pos[0], self.pos[1] - 3.0], 0.0));
                    self.juice.shake(0.35);
                    sounds.push(Sfx::Turn);
                }
                for f in &mut self.fx {
                    f.t += dt;
                }
                self.fx.retain(|f| !f.done());
                let ready = if won { 1.0 } else { TURNED + 0.6 };
                if t > ready && input.pressed.any(Buttons::A | Buttons::START) {
                    self.restart(sounds);
                } else if t > ready && input.pressed.contains(Buttons::B) {
                    flow.quit = true;
                }
            }
        }
        flow.found = std::mem::take(&mut self.found_now);
        flow
    }

    /// The upgrade cards: they slide in, take a pick, and hold it a moment before play resumes.
    fn choose(&mut self, input: &Input, dt: f32, sounds: &mut Vec<Sfx>) {
        let Phase::LevelUp { choices, mut selected, t, picked } = self.phase else { return };
        let t = t + dt;
        if let Some(p) = picked {
            let p = p + dt;
            if p >= PICK_TIME {
                self.phase = Phase::Playing;
                self.turn_ready = false;
            } else {
                self.phase = Phase::LevelUp { choices, selected, t, picked: Some(p) };
            }
            return;
        }
        let mut picked = None;
        if t > CARDS_IN {
            let n = choices.len();
            if input.pressed.any(Buttons::DOWN | Buttons::RIGHT) {
                selected = (selected + 1) % n;
                sounds.push(Sfx::Tick(selected as u8 * 3));
            }
            if input.pressed.any(Buttons::UP | Buttons::LEFT) {
                selected = (selected + n - 1) % n;
                sounds.push(Sfx::Tick(selected as u8 * 3));
            }
            let tapped = input.touch.filter(|_| input.touch_started).and_then(|[_, y]| choice_row(y)).filter(|&i| i < 3);
            // X picks: A turns, and a player still holding it as the cards appear shouldn't pick by accident.
            if let Some(i) = tapped.or(input.pressed.contains(Buttons::X).then_some(selected)) {
                let c = choices[i];
                self.pick(c);
                self.xp_shown = 0.0;
                sounds.push(Sfx::Pick(c.rare));
                selected = i;
                picked = Some(0.0);
            }
        }
        self.phase = Phase::LevelUp { choices, selected, t, picked };
    }

    fn play(&mut self, input: &Input, dt: f32, sounds: &mut Vec<Sfx>, flow: &mut Flow) {
        let dt = if self.leveling > 0.0 {
            self.leveling -= dt;
            if self.leveling <= 0.0 {
                self.phase = Phase::LevelUp { choices: self.roll_choices(), selected: 0, t: 0.0, picked: None };
                self.glory = 0.25;
                return;
            }
            dt * 0.15
        } else {
            dt
        };
        self.t += dt;
        self.hurt = (self.hurt - dt).max(0.0);
        // Up and down walk along the heading, left and right strafe, Y and A turn, B turns around.
        let dir = input.dir();
        let turning = Buttons::A | Buttons::Y;
        self.turn_ready |= !input.held.any(turning);
        let held = if self.turn_ready { input.held } else { Buttons::NONE };
        self.targeting = input.held.contains(Buttons::X);
        let turn = held.contains(Buttons::A) as i32 as f32 - held.contains(Buttons::Y) as i32 as f32;
        if input.pressed.contains(Buttons::B) {
            self.turn_around();
        }
        let spun = self.spin.min(PI / SPIN_TIME * dt);
        self.spin -= spun;
        self.view_yaw = (self.view_yaw + turn * crate::tuning::tuning().turn_speed * dt + spun).rem_euclid(TAU);
        let f = [self.view_yaw.cos(), self.view_yaw.sin()];
        let (side, ahead) = (dir[0], -dir[1]);
        self.lean += (side - self.lean) * (8.0 * dt).min(1.0);
        let mv = [f[0] * ahead - f[1] * side, f[1] * ahead + f[0] * side];
        self.aim = f;
        self.facing = Facing::of(f);
        if len(mv) > 0.2 {
            let k = self.kit.speed() * dt * len(mv).min(1.0) / len(mv);
            self.pos = self.map.slide(self.pos, [mv[0] * k, mv[1] * k], crate::tuning::tuning().player_half());
            let before = (self.walk * crate::tuning::tuning().walk_fps) as u32;
            self.walk += dt;
            if (self.walk * crate::tuning::tuning().walk_fps) as u32 / 2 != before / 2 {
                let back = [-mv[0], -mv[1]];
                self.juice.burst(self.pos, 0.5, 2, back, 0.8, [8.0, 20.0], juice::dirt(), 1.0, 0.35, 0.0);
            }
        } else {
            self.walk = 0.0;
        }
        self.noise = None;
        self.grenades = (self.grenades + dt / (crate::tuning::tuning().grenade_recharge * self.kit.mods.recharge)).min(self.kit.grenades());
        self.hp = (self.hp + self.kit.mods.regen * dt).min(self.kit.max_hp());
        if let Some(touch) = input.touch.filter(|_| input.touch_started).or(self.queued_touch.take()) {
            self.throw(touch, sounds);
        }
        self.fly(dt, sounds, flow);

        if !self.calm {
            self.spawn(dt);
        } else if input.pressed.contains(Buttons::X) {
            // The quiet farm takes one zombie at a time, for trying out weapons and effects.
            let pos = [self.pos[0] + f[0] * 50.0, self.pos[1] + f[1] * 50.0];
            if !self.map.hits_cells(pos[0], pos[1], crate::tuning::tuning().zombie_half()) {
                self.tells.push(Tell { kind: Kind::common(), pos, hp: Kind::common().stats().hp, t: 0.0, leader: false });
            }
        }
        self.rise(dt, sounds);
        self.move_zombies(dt);
        self.grow_fury(dt);
        self.contact(sounds, flow);
        self.fire(dt, sounds);
        self.move_bullets(dt, sounds);
        self.hazards(dt);
        self.decoys(dt, sounds);
        self.restock(dt);
        if self.blow(dt, sounds) {
            flow.rumble = flow.rumble.max(Some(Rumble::Blast));
        }
        self.crumble(dt, sounds);
        self.reap(sounds, flow);
        self.collect(dt, sounds);
        self.flock(dt, sounds);
        self.ambience(dt, sounds, flow);
        for f in &mut self.fx {
            f.t += dt;
        }
        self.fx.retain(|f| !f.done());
        self.juice.tick(dt);
        self.feel(dt);

        if self.hp <= 0.0 && (self.revived as f32) < self.kit.mods.revive {
            self.revive(sounds, flow);
        } else if self.hp <= 0.0 {
            self.hp = 0.0;
            sounds.push(Sfx::GameOver);
            self.juice.shake(0.8);
            self.juice.burst(self.pos, 8.0, 30, [0.0; 2], 0.0, [30.0, 120.0], juice::blood(), 2.0, 0.9, 0.0);
            flow.rumble = Some(Rumble::Blast);
            self.leveling = 0.0;
            self.phase = Phase::Over { t: 0.0, won: false };
            flow.ended = Some(self.finish(false));
        } else if self.t >= dawn() && len(sub(self.pos, self.map.home)) < crate::tuning::tuning().home {
            sounds.push(Sfx::Dawn);
            // Home: the door shuts and first light burns the dead away.
            for z in std::mem::take(&mut self.zombies) {
                self.juice.burst(z.pos, 8.0, 3, [0.0; 2], 0.0, [10.0, 40.0], juice::ember(), 1.4, 1.2, 1.0);
                self.fx.push(Fx::play(effects::effects().dawn_burns, [z.pos[0], z.pos[1] - 6.0], 0.0));
            }
            self.tells.clear();
            self.juice.flash(self.pos, 40.0, [1.6, 1.1, 0.7], 14.0, 1.5);
            self.juice.shake(0.3);
            flow.rumble = flow.rumble.max(Some(Rumble::Fanfare));
            self.phase = Phase::Over { t: 0.0, won: true };
            flow.ended = Some(self.finish(true));
        } else if self.t >= dawn() && !self.woke {
            self.daybreak(sounds, flow);
        } else if self.leveling <= 0.0 && self.xp >= self.next_xp {
            self.level_up(sounds, flow);
        } else if self.leveling <= 0.0 && self.chests > 0 {
            self.chests -= 1;
            self.chest = true;
            self.leveling = LEVEL_SLOW;
        }
    }

    /// Dawn: the night doesn't end, it turns. The Harvester rises behind the player, and from here on the
    /// dead grow stronger every second until the player reaches the farmhouse.
    fn daybreak(&mut self, sounds: &mut Vec<Sfx>, flow: &mut Flow) {
        self.woke = true;
        sounds.push(Sfx::Horde);
        flow.rumble = flow.rumble.max(Some(Rumble::Blast));
        self.juice.shake(0.6);
        self.juice.banner("DAWN.  THE HARVESTER WAKES.  GET HOME.", [1.0, 0.45, 0.3, 1.0], 4.0);
        let away = norm(sub(self.pos, self.map.home));
        let away = if away == [0.0; 2] { [0.0, 1.0] } else { away };
        let mut at = self.pos;
        for k in 0..24 {
            let a = (k as f32 * 0.4).sin() * (k as f32 * 0.15);
            let d = [away[0] * a.cos() - away[1] * a.sin(), away[0] * a.sin() + away[1] * a.cos()];
            let p = [self.pos[0] + d[0] * crate::tuning::tuning().harvester_rise, self.pos[1] + d[1] * crate::tuning::tuning().harvester_rise];
            if !self.map.hits_cells(p[0], p[1], crate::tuning::tuning().zombie_half()) {
                at = p;
                break;
            }
        }
        let mut z = Zombie::new(Kind(roster().harvester as u8), at, crate::tuning::tuning().harvester_hp, 0.0, 0.0);
        z.boss = true;
        z.id = self.next_id;
        self.next_id += 1;
        self.zombies.push(z);
        self.juice.burst(at, 0.0, 40, [0.0; 2], 0.0, [30.0, 110.0], juice::dirt(), 2.2, 1.0, 0.0);
        self.juice.ring(at, 60.0, 0.8, [0.9, 0.2, 0.15]);
    }

    /// Every second past dawn, every zombie grows by the fury `tuning.txt` sets, the ones still rising too.
    fn grow_fury(&mut self, dt: f32) {
        if self.t < dawn() {
            return;
        }
        self.fury_tick += dt;
        while self.fury_tick >= 1.0 {
            self.fury_tick -= 1.0;
            self.fury *= crate::tuning::tuning().fury;
            for z in &mut self.zombies {
                z.hp *= crate::tuning::tuning().fury;
            }
            for t in &mut self.tells {
                t.hp *= crate::tuning::tuning().fury;
            }
        }
    }

    /// Start pauses: Up and Down choose, A or X (or a tap) takes it, and Start or B resumes.
    fn pause_menu(&mut self, at: usize, input: &Input, sounds: &mut Vec<Sfx>, flow: &mut Flow) {
        let n = PAUSE_CHOICES.len();
        let mut at = at;
        if input.pressed.any(Buttons::DOWN | Buttons::RIGHT) {
            at = (at + 1) % n;
            sounds.push(Sfx::Tick(at as u8 * 3));
        }
        if input.pressed.any(Buttons::UP | Buttons::LEFT) {
            at = (at + n - 1) % n;
            sounds.push(Sfx::Tick(at as u8 * 3));
        }
        let tapped = input.touch.filter(|_| input.touch_started).and_then(|[_, y]| choice_row(y)).filter(|&i| i < n);
        let chosen = tapped.or(input.pressed.any(Buttons::A | Buttons::X).then_some(at));
        self.menu = Some(at);
        match chosen {
            _ if input.pressed.any(Buttons::START | Buttons::B) => self.menu = None,
            Some(0) => self.menu = None,
            Some(1) => flow.settings = true,
            Some(2) => self.restart(sounds),
            Some(_) => flow.quit = true,
            None => {}
        }
        self.turn_ready &= self.menu.is_some();
    }

    /// A fresh farm with the same difficulty, progress, enemies, look and trial powerups.
    fn restart(&mut self, sounds: &mut Vec<Sfx>) {
        let seed = self.rng.next_u32() as u64 + 1;
        let kit: Vec<String> = self.trial.iter().map(|&(d, l)| format!("{}:{l}", book().defs[d].id)).collect();
        let look = std::mem::take(&mut self.look);
        *self = Harvest::on(seed, &self.map_id).at(self.difficulty).with_progress(self.progress).without_enemies(self.enemies_off).looking_like(&look).with_kit(&kit.join(",")).starting_at(self.start_t).single_screen(self.single).endless_grenades(self.endless);
        sounds.push(Sfx::Start);
    }

    /// The way the player's sprite faces: the old heading while B swings the camera round, else where they aim.
    fn showing(&self) -> [f32; 2] {
        if self.spin > 0.0 { self.turned_from } else { self.aim }
    }

    /// B: the camera swings round while the player keeps their old heading, so it sweeps past their front.
    fn turn_around(&mut self) {
        if self.spin > 0.0 {
            return;
        }
        self.turned_from = [self.view_yaw.cos(), self.view_yaw.sin()];
        self.spin = PI;
    }

    /// The bar fills: a gold shockwave shoves the nearest zombies back while time slows, then the cards come.
    fn level_up(&mut self, sounds: &mut Vec<Sfx>, flow: &mut Flow) {
        self.xp -= self.next_xp;
        self.next_xp = (self.next_xp as f32 * 1.35).ceil() as u32 + 2;
        self.level += 1;
        self.xp_shown = self.next_xp as f32;
        self.leveling = LEVEL_SLOW;
        sounds.push(Sfx::LevelUp);
        flow.rumble = flow.rumble.max(Some(Rumble::Fanfare));
        self.shove(320.0);
        self.juice.ring(self.pos, crate::tuning::tuning().push, 0.5, juice::gold());
        self.juice.flash(self.pos, 20.0, [1.8, 1.4, 0.6], 9.0, 0.8);
        self.juice.punch += 6.0;
        self.juice.shake(0.3);
        self.juice.shout("LEVEL UP", GOLD, 0.9);
        for i in 0..40 {
            let a = i as f32 / 40.0 * TAU;
            let r = self.juice.rng.range(3.0, 12.0);
            let rise = self.juice.rng.range(50.0, 130.0);
            let life = self.juice.rng.range(0.8, 1.4);
            let z = self.juice.rng.range(0.0, 6.0);
            self.juice.particle(Particle {
                pos: [self.pos[0] + a.cos() * r, self.pos[1] + a.sin() * r],
                z,
                vel: [a.cos() * 18.0, a.sin() * 18.0],
                vz: rise,
                age: 0.0,
                life,
                color: juice::gold(),
                size: 1.6,
                shrink: true,
                gravity: 40.0,
                drag: 1.0,
                glow: 1.0,
                wander: false,
            });
        }
    }

    /// Throws the nearest zombies back from the player.
    fn shove(&mut self, force: f32) {
        for z in &mut self.zombies {
            let d = sub(z.pos, self.pos);
            let l = len(d);
            if l < crate::tuning::tuning().push && z.rise >= 1.0 {
                let k = force * (1.0 - l / crate::tuning::tuning().push) / z.kind.stats().mass;
                let a = norm(d);
                z.push = [a[0] * k, a[1] * k];
                z.squash = 1.0;
                z.knock = a;
            }
        }
    }

    /// Three cards from what's unlocked and still has room to grow; fillers make up the rest.
    fn roll_choices(&mut self) -> [Choice; 3] {
        let b = book();
        let chest = std::mem::take(&mut self.chest);
        let mut pool: Vec<usize> = (0..b.defs.len()).filter(|&i| self.progress.owns(i) && self.kit.offerable(i)).collect();
        let mut fillers: Vec<usize> = (0..b.defs.len()).filter(|&i| b.defs[i].kind == Card::Filler).collect();
        let rare = crate::tuning::tuning().rare + self.kit.mods.luck;
        let mut out = [Choice { up: 0, rare: false, found: false }; 3];
        for slot in &mut out {
            let from = if pool.is_empty() { &mut fillers } else { &mut pool };
            let up = if from.is_empty() { 0 } else { from.swap_remove(self.rng.below(from.len())) };
            let roll = self.rng.range(0.0, 1.0);
            *slot = Choice { up, rare: b.defs[up].kind != Card::Filler && roll < rare, found: false };
        }
        if let Some(up) = self.find(chest) {
            out[self.rng.below(3)] = Choice { up, rare: false, found: true };
        }
        out
    }

    /// A powerup to offer as a find, if one turns up: only one whose unlock is met, whose minute has come and
    /// that has a free slot to go in. A chest always holds one if any qualify, its own kind first.
    fn find(&mut self, chest: bool) -> Option<usize> {
        let b = book();
        if self.newly.len() >= b.finds {
            return None;
        }
        let minutes = self.t / 60.0;
        let finds: Vec<usize> = (0..b.defs.len())
            .filter(|&i| self.progress.findable(i) && self.kit.offerable(i) && minutes >= b.defs[i].after && (chest || !b.defs[i].chest))
            .collect();
        if chest {
            let own: Vec<usize> = finds.iter().copied().filter(|&i| b.defs[i].chest).collect();
            let from = if own.is_empty() { &finds } else { &own };
            return (!from.is_empty()).then(|| from[self.rng.below(from.len())]);
        }
        let i = *finds.get(self.rng.below(finds.len().max(1)))?;
        (self.rng.range(0.0, 1.0) < b.defs[i].find).then_some(i)
    }

    /// Takes a card: a level, or two for a rare one, and anything it gives right away.
    fn pick(&mut self, c: Choice) {
        let def = &book().defs[c.up];
        if c.found {
            self.progress.found |= 1 << c.up;
            self.found_now |= 1 << c.up;
            self.newly.push(c.up);
            self.juice.banner(format!("FOUND: {}", def.name.to_uppercase()), [GOLD[0], GOLD[1], GOLD[2], 1.0], 2.4);
        }
        let (from, to) = self.kit.raise(c.up, if c.rare { 2 } else { 1 });
        let heal = def.at(powerups::Stat::Heal, to) * (to - from).max(1) as f32;
        self.hp = (self.hp + heal).min(self.kit.max_hp());
        if def.at(powerups::Stat::Refill, 1) > 0.0 {
            self.grenades = self.kit.grenades();
        }
        self.xp += def.at(powerups::Stat::Money, 1) as u32;
    }

    /// A vaccine pulls the player back from turning: half health, a moment untouchable, and the dead thrown back.
    fn revive(&mut self, sounds: &mut Vec<Sfx>, flow: &mut Flow) {
        self.revived += 1;
        self.hp = self.kit.max_hp() * 0.5;
        self.hurt = 2.0;
        self.shove(600.0);
        self.juice.ring(self.pos, crate::tuning::tuning().push, 0.6, [0.4, 1.0, 0.5]);
        self.juice.flash(self.pos, 20.0, [0.6, 1.8, 0.7], 9.0, 0.8);
        self.juice.shake(0.4);
        self.juice.banner("THE VACCINE HOLDS", [0.5, 1.0, 0.55, 1.0], 2.4);
        sounds.push(Sfx::Heal);
        flow.rumble = flow.rumble.max(Some(Rumble::Fanfare));
    }

    /// How the run went, and what it unlocked.
    fn finish(&mut self, won: bool) -> Run {
        let run = Run { secs: self.t as u32, kills: self.kills, level: self.level, won };
        let after = self.progress.after(run);
        self.ready = (0..book().defs.len()).filter(|&i| !self.progress.findable(i) && after.findable(i)).count();
        self.progress = after;
        run
    }

    /// Zombies arrive just out of sight, faster and tougher as the night goes on.
    fn spawn(&mut self, dt: f32) {
        let minutes = self.t / 60.0;
        self.owed += (0.7 + minutes * 0.9).min(9.0) * dt;
        if horde_now(self.t, dt) {
            self.owed += 20.0 + minutes * 4.0;
            self.leader_owed = true;
        }
        while self.owed >= 1.0 && self.zombies.len() + self.tells.len() < (crate::tuning::tuning().max_zombies as usize) {
            self.owed -= 1.0;
            let roll = self.rng.range(0.0, 1.0);
            let leader = std::mem::take(&mut self.leader_owed);
            let r = roster();
            let kind = Kind(if leader && self.enemies_off & 1 << r.leader == 0 { r.leader } else { r.pick(minutes, roll, self.enemies_off) } as u8);
            for _ in 0..8 {
                let a = self.rng.range(0.0, TAU);
                let [near, far] = spawn_ring(self.t);
                let r = self.rng.range(near, far);
                let pos = [self.pos[0] + a.cos() * r, self.pos[1] + a.sin() * r];
                if !self.map.hits_cells(pos[0], pos[1], crate::tuning::tuning().zombie_half()) {
                    let hp = kind.stats().hp * (1.0 + minutes * crate::tuning::tuning().growth) * if leader { crate::tuning::tuning().leader_hp } else { 1.0 } * self.fury;
                    self.tells.push(Tell { kind, pos, hp, t: 0.0, leader });
                    break;
                }
            }
        }
        self.owed = self.owed.min(40.0);
    }

    /// The ground churns where a zombie is coming, then it breaks out in a spray of dirt.
    fn rise(&mut self, dt: f32, sounds: &mut Vec<Sfx>) {
        for i in (0..self.tells.len()).rev() {
            let (pos, age) = {
                let t = &mut self.tells[i];
                t.t += dt;
                (t.pos, t.t)
            };
            if age < TELL_TIME {
                if self.juice.rng.range(0.0, 1.0) < dt * 20.0 {
                    self.juice.burst(pos, 0.0, 1, [0.0; 2], 0.0, [6.0, 18.0], juice::dirt(), 1.4, 0.4, 0.0);
                }
                continue;
            }
            let t = self.tells.swap_remove(i);
            let anim = self.rng.range(0.0, 1.0);
            let mut z = Zombie::new(t.kind, t.pos, t.hp, 0.0, anim);
            z.leader = t.leader;
            z.id = self.next_id;
            self.next_id += 1;
            self.zombies.push(z);
            self.fx.push(Fx::play(effects::effects().zombie_rises, [pos[0], pos[1] - 3.0], 0.0));
            self.juice.burst(pos, 0.0, 8, [0.0; 2], 0.0, [15.0, 45.0], juice::dirt(), 1.6, 0.6, 0.0);
            let (pan, gain) = self.place(pos);
            if self.hush[3] <= 0.0 && gain > 60 {
                sounds.push(Sfx::Rise { pan, gain });
                self.hush[3] = 0.3;
            }
        }
    }

    fn move_zombies(&mut self, dt: f32) {
        let n = self.zombies.len();
        let mut sep = std::mem::take(&mut self.sep);
        sep.clear();
        sep.resize(n, [0.0; 2]);
        for i in 0..n {
            for j in i + 1..n {
                let d = sub(self.zombies[i].pos, self.zombies[j].pos);
                let dist2 = d[0] * d[0] + d[1] * d[1];
                if dist2 < 64.0 && dist2 > 1e-4 {
                    let k = (8.0 - dist2.sqrt()) / dist2.sqrt();
                    sep[i] = [sep[i][0] + d[0] * k, sep[i][1] + d[1] * k];
                    sep[j] = [sep[j][0] - d[0] * k, sep[j][1] - d[1] * k];
                }
            }
        }
        let cell = [(self.pos[0] / TILE) as usize, (self.pos[1] / TILE) as usize];
        if cell != self.flow_cell {
            self.map.flow_into(self.pos, &mut self.flow, &mut self.steps);
            self.flow_cell = cell;
        }
        let mut bites = Vec::new();
        for (z, &s) in self.zombies.iter_mut().zip(&sep) {
            z.flash = (z.flash - dt).max(0.0);
            z.hurt = (z.hurt - dt).max(0.0);
            z.squash = (z.squash - dt * 5.0).max(0.0);
            if z.rise < 1.0 {
                z.rise = (z.rise + dt / crate::tuning::tuning().rise).min(1.0);
                continue;
            }
            if z.stagger > 0.0 {
                z.stagger -= dt;
                continue;
            }
            let t = crate::tuning::tuning();
            let charge = if len(sub(self.pos, z.pos)) < t.charge_reach { z.kind.stats().charge } else { 1.0 };
            let speed = if z.boss { t.harvester_speed } else { z.kind.stats().speed * charge } * (1.0 - z.slow_k);
            let direct = sub(self.pos, z.pos);
            let lure = self.decoys.iter().find(|d| len(sub(d.pos, z.pos)) < d.lure);
            let to = match lure {
                Some(d) => norm(sub(d.pos, z.pos)),
                None if len(direct) < TILE * 1.5 => norm(direct),
                None => self.map.downhill(&self.flow, z.pos).unwrap_or(norm(direct)),
            };
            let v = [to[0] * speed + s[0] * 20.0 + z.push[0], to[1] * speed + s[1] * 20.0 + z.push[1]];
            z.pos = self.map.slide_cells(z.pos, [v[0] * dt, v[1] * dt], crate::tuning::tuning().zombie_half());
            z.push = [z.push[0] * (1.0 - 8.0 * dt).max(0.0), z.push[1] * (1.0 - 8.0 * dt).max(0.0)];
            z.facing = Facing::of(to);
            z.dir = to;
            z.anim += dt;
            let ahead = [z.pos[0] + to[0] * TILE / 2.0, z.pos[1] + to[1] * TILE / 2.0];
            if let Some(i) = self.map.breakable_at(ahead[0], ahead[1]) {
                bites.push((i, ahead, z.kind.stats().damage * crate::tuning::tuning().chew * self.fury * dt));
            }
        }
        self.sep = sep;
        for (i, at, damage) in bites {
            self.map.hurt_prop(i, damage, 0.0);
            if self.juice.rng.range(0.0, 1.0) < dt * 3.0 {
                self.juice.burst(at, 4.0, 2, [0.0; 2], 0.0, [20.0, 50.0], juice::dirt(), 1.0, 0.3, 0.0);
            }
        }
    }

    /// Breaks a prop chewed or shot to pieces, one per frame, and sends the dead the new way through.
    fn crumble(&mut self, dt: f32, sounds: &mut Vec<Sfx>) {
        let Some(c) = self.map.crumble(dt) else { return };
        self.flow_cell = [usize::MAX; 2];
        self.fx.push(Fx::play(effects::effects().prop_crumbles, [c[0], c[1] - 4.0], 0.0));
        self.juice.burst(c, 4.0, 12, [0.0; 2], 0.0, [30.0, 80.0], juice::dirt(), 1.6, 0.6, 0.0);
        self.noise = Some(c);
        self.thud((1, false), sounds);
    }

    /// With the player down, the dead lose interest and shamble away.
    /// Turned, the player shambles on the way they faced, and the dead close in and walk with them to the next farm.
    fn march(&mut self, dt: f32) {
        let ahead = self.aim;
        self.pos = self.map.slide(self.pos, [ahead[0] * MARCH_SPEED * dt, ahead[1] * MARCH_SPEED * dt], crate::tuning::tuning().player_half());
        for z in self.zombies.iter_mut().filter(|z| z.rise >= 1.0) {
            let to = sub(self.pos, z.pos);
            let pull = ((len(to) - MARCH_CLOSE) / MARCH_CLOSE).clamp(0.0, 1.5);
            let n = norm(to);
            let way = norm([ahead[0] + n[0] * pull, ahead[1] + n[1] * pull]);
            let speed = MARCH_SPEED * (1.0 + 0.5 * pull);
            z.pos = self.map.slide(z.pos, [way[0] * speed * dt, way[1] * speed * dt], crate::tuning::tuning().zombie_half());
            z.dir = way;
            z.facing = Facing::of(way);
            z.anim += dt;
        }
    }

    fn contact(&mut self, sounds: &mut Vec<Sfx>, flow: &mut Flow) {
        if self.hurt > 0.0 {
            return;
        }
        let hit = self.zombies.iter().filter(|z| z.rise >= 1.0 && len(sub(z.pos, self.pos)) < crate::tuning::tuning().bite_reach).map(|z| z.kind.stats().damage * if z.boss { crate::tuning::tuning().harvester_bite } else { 1.0 } * self.fury * self.difficulty.damage()).fold(0.0, f32::max);
        let hit = if hit > 0.0 { (hit - self.kit.mods.armor).max(1.0) } else { 0.0 };
        if hit > 0.0 {
            self.hp -= hit;
            self.hurt = self.difficulty.hurt_time();
            self.juice.popup(self.pos, hit, Tone::Hurt);
            flow.rumble = flow.rumble.max(Some(Rumble::Hit));
            sounds.push(Sfx::Hurt);
            self.ouch(0.45);
        }
    }

    /// The player takes a hit: the screen flashes red and shakes, and the world catches its breath.
    fn ouch(&mut self, shake: f32) {
        self.juice.hurt = 0.35;
        self.juice.shake(shake);
        self.juice.stop(0.08);
        self.juice.burst(self.pos, 8.0, 10, [0.0; 2], 0.0, [30.0, 80.0], juice::blood(), 1.6, 0.5, 0.0);
    }

    /// One hit sound for however many landed this frame, unless one just played.
    fn thud(&mut self, (hits, crit): (u32, bool), sounds: &mut Vec<Sfx>) {
        if hits > 0 && (self.hush[0] <= 0.0 || crit) {
            sounds.push(Sfx::Thud(crit));
            self.hush[0] = 0.06;
        }
    }

    fn move_bullets(&mut self, dt: f32, sounds: &mut Vec<Sfx>) {
        let crit = crate::tuning::tuning().crit + self.kit.mods.crit;
        let mut hits = (0, false);
        for b in &mut self.bullets {
            b.pos = [b.pos[0] + b.vel[0] * dt, b.pos[1] + b.vel[1] * dt];
            b.life -= dt;
            b.age += dt;
            let ground = [b.pos[0], b.pos[1] + 6.0];
            let back = norm([-b.vel[0], -b.vel[1]]);
            if self.map.cover_at(ground[0], ground[1]) {
                let cell = |p: [f32; 2]| [(p[0] / TILE).floor(), (p[1] / TILE).floor()];
                let at = cell(ground);
                if let Some(barrel) = self.barrels.iter_mut().find(|r| cell(r.pos) == at) {
                    barrel.hp -= b.damage;
                    barrel.flash = FLASH_TIME;
                    self.juice.burst(ground, 6.0, 4, back, 0.8, [40.0, 100.0], juice::spark(), 0.8, 0.2, 1.0);
                } else if let Some(i) = self.map.breakable_at(ground[0], ground[1]) {
                    self.map.hurt_prop(i, b.damage, FLASH_TIME);
                    self.juice.burst(ground, 6.0, 4, back, 0.9, [30.0, 70.0], juice::dirt(), 1.0, 0.3, 0.0);
                } else {
                    self.juice.burst(ground, 6.0, 2, back, 0.9, [20.0, 50.0], juice::dirt(), 1.0, 0.3, 0.0);
                }
                b.life = 0.0;
                continue;
            }
            for z in &mut self.zombies {
                if b.life <= 0.0 || b.hit.contains(&z.id) {
                    continue;
                }
                let d = sub([z.pos[0], z.pos[1] - 6.0], b.pos);
                if len(d) < 7.0 {
                    hits.1 |= strike(z, &mut self.juice, b.damage, norm(b.vel), b.force, crit);
                    hits.0 += 1;
                    b.hit.push(z.id);
                    if b.pierce == 0 {
                        b.life = 0.0;
                    } else {
                        b.pierce -= 1;
                    }
                }
            }
        }
        self.bullets.retain(|b| b.life > 0.0);
        self.thud(hits, sounds);
    }

    /// Sets off a barrel shot to pieces, one per frame so neighbors chain; returns whether one blew.
    fn blow(&mut self, dt: f32, sounds: &mut Vec<Sfx>) -> bool {
        for r in &mut self.barrels {
            r.flash = (r.flash - dt).max(0.0);
        }
        let Some(i) = self.barrels.iter().position(|r| r.hp <= 0.0) else { return false };
        let c = self.barrels.swap_remove(i).center();
        self.map.open(c[0], c[1]);
        self.flow_cell = [usize::MAX; 2];
        self.explode(c, crate::tuning::tuning().barrel_radius, crate::tuning::tuning().barrel_damage, sounds);
        true
    }

    /// Damages zombies in `radius`, sets off barrels in it, and hurts the player if too close.
    /// Bigger with a fuel drums' blast, an area passive's reach and the damage passives.
    fn explode(&mut self, c: [f32; 2], radius: f32, damage: f32, sounds: &mut Vec<Sfx>) {
        let m = self.kit.mods;
        let (radius, damage, crit) = (radius * m.blast * m.area, damage * m.blast * m.damage, crate::tuning::tuning().crit + m.crit);
        let mut hits = (0, false);
        for z in &mut self.zombies {
            let d = sub(z.pos, c);
            if len(d) < radius {
                hits.1 |= strike(z, &mut self.juice, damage, norm(d), 120.0 * m.knockback, crit);
                hits.0 += 1;
                z.fling = 60.0 + 150.0 * (1.0 - len(d) / radius);
            }
        }
        for r in &mut self.barrels {
            if len(sub(r.center(), c)) < radius {
                r.hp = r.hp.min(0.0);
            }
        }
        for i in self.map.breakables_near(c, radius) {
            self.map.hurt_prop(i, damage, FLASH_TIME);
        }
        let t = crate::tuning::tuning();
        if len(sub(self.pos, c)) < radius * t.blast_near && self.hurt <= 0.0 {
            let hurt = (t.blast_hurt - m.armor).max(1.0);
            self.hp -= hurt;
            self.juice.popup(self.pos, hurt, Tone::Hurt);
            self.hurt = self.difficulty.hurt_time();
            self.ouch(0.3);
        }
        self.fx.push(Fx::play(effects::effects().explosion, c, 0.0));
        self.fx.push(Fx::play(effects::effects().explosion_smoke, [c[0], c[1] - 8.0], 0.0));
        let near = len(sub(self.pos, c));
        self.juice.shake((0.75 * (1.0 - near / 260.0)).max(0.12));
        self.juice.stop(0.07);
        self.juice.punch += 4.0;
        self.juice.flash(c, 14.0, [2.4, 1.3, 0.5], 8.0, 0.45);
        self.juice.ring(c, radius * 1.3, 0.45, juice::ember());
        self.juice.burst(c, 4.0, 18, [0.0; 2], 0.0, [40.0, 140.0], juice::ember(), 1.2, 0.6, 1.0);
        self.juice.burst(c, 2.0, 14, [0.0; 2], 0.0, [30.0, 90.0], juice::dirt(), 1.8, 0.8, 0.0);
        let scorch = art::BLOOD_STAIN[self.juice.rng.below(art::BLOOD_STAIN.len())];
        self.stain((scorch, c[0] - scorch.w() / 2.0, c[1] - scorch.h() / 2.0, [0.12, 0.1, 0.09]));
        self.noise = Some(c);
        let (pan, gain) = self.place(c);
        sounds.push(Sfx::Boom { pan, gain: gain.max(110) });
        self.thud(hits, sounds);
    }

    fn stain(&mut self, s: (Art, f32, f32, [f32; 3])) {
        self.stains.push(s);
        if self.stains.len() > MAX_STAINS {
            self.stains.remove(0);
        }
    }

    /// Lobs a grenade at the map point under a bottom-screen touch, if one is ready.
    fn throw(&mut self, touch: [f32; 2], sounds: &mut Vec<Sfx>) {
        let cam = self.camera();
        self.throw_at([cam[0] + touch[0] / SCALE, cam[1] + touch[1] / SCALE], sounds);
    }

    fn throw_at(&mut self, to: [f32; 2], sounds: &mut Vec<Sfx>) {
        if self.grenades < 1.0 {
            sounds.push(Sfx::Miss);
            return;
        }
        if !self.endless {
            self.grenades -= 1.0;
        }
        self.thrown.push(Grenade { from: self.pos, to, t: 0.0, kind: Thrown::Grenade });
        sounds.push(Sfx::Throw);
    }

    fn fly(&mut self, dt: f32, sounds: &mut Vec<Sfx>, flow: &mut Flow) {
        for g in &mut self.thrown {
            g.t += dt;
        }
        let (landed, flying): (Vec<Grenade>, Vec<Grenade>) = std::mem::take(&mut self.thrown).into_iter().partition(|g| g.t >= crate::tuning::tuning().grenade_flight);
        self.thrown = flying;
        for g in landed {
            self.land(&g, sounds);
            let blast = matches!(g.kind, Thrown::Lob { def, .. } if powerups::book().defs[def].lands == powerups::Lands::Blast);
            if blast || matches!(g.kind, Thrown::Grenade | Thrown::Bomblet) {
                flow.rumble = flow.rumble.max(Some(Rumble::Blast));
            }
        }
    }

    /// A zombie dying at `pos`, knocked toward `fall` and flung `fling` hard: blood, a stain, gibs and its corpse.
    pub(super) fn gore(&mut self, pos: [f32; 2], fall: [f32; 2], fling: f32, dir: [f32; 2], look: &Look, gibs: usize) {
        self.fx.push(Fx::play(effects::effects().blood, [pos[0], pos[1] - 4.0], 0.0));
        let stain = art::BLOOD_STAIN[self.rng.below(art::BLOOD_STAIN.len())];
        self.stain((stain, pos[0] - stain.w() / 2.0, pos[1] - stain.h() / 2.0, [1.0; 3]));
        let (vel, vz) = if fling > 0.0 { ([fall[0] * fling, fall[1] * fling], fling * 0.9) } else { ([fall[0] * 25.0, fall[1] * 25.0], 0.0) };
        let flip = self.juice.rng.below(2) == 0;
        let dead = look.dead.map(|d| (d, dir));
        self.juice.corpse(Corpse { art: look.hurt[Facing::Front.index()], dead, flip, pos, z: 0.0, vel, vz, fall, age: 0.0, spin: 0.0 });
        self.juice.burst(pos, 8.0, gibs, fall, 1.2, [30.0, 110.0], juice::blood(), 1.6, 0.6, 0.0);
        self.juice.burst(pos, 8.0, gibs / 3, fall, 0.8, [20.0, 60.0], juice::dark_blood(), 2.4, 0.8, 0.0);
    }

    fn reap(&mut self, sounds: &mut Vec<Sfx>, flow: &mut Flow) {
        let mut died: Option<Kind> = None;
        let mut i = 0;
        while i < self.zombies.len() {
            if self.zombies[i].hp > 0.0 {
                i += 1;
                continue;
            }
            let z = self.zombies.swap_remove(i);
            self.kills += 1;
            self.kill_bump = 0.0;
            died = Some(match died {
                Some(k) if k.stats().heavy => k,
                _ => z.kind,
            });
            let s = z.kind.stats();
            let fall = if z.knock == [0.0; 2] { norm(sub(z.pos, self.pos)) } else { z.knock };
            self.gore(z.pos, fall, z.fling, z.dir, &s.look, s.gibs as usize);
            if s.heavy {
                self.juice.stop(0.06);
                self.juice.shake(0.2);
            }
            let kind = if z.leader || z.boss {
                DropKind::Chest
            } else if self.rng.below(self.difficulty.health_drop()) == 0 {
                DropKind::Health
            } else {
                DropKind::Money(s.xp)
            };
            let a = self.juice.rng.range(0.0, TAU);
            let speed = self.juice.rng.range(8.0, 28.0);
            let vz = self.juice.rng.range(60.0, 95.0);
            self.drops.push(Drop { kind, pos: z.pos, z: 4.0, vz, vel: [a.cos() * speed, a.sin() * speed], age: 0.0, pulled: None });
        }
        if let Some(kind) = died {
            if self.hush[1] <= 0.0 || kind.stats().heavy {
                sounds.push(Sfx::Kill(kind.0));
                self.hush[1] = 0.05;
            }
            flow.rumble = flow.rumble.max(Some(Rumble::Tick));
        }
    }

    fn collect(&mut self, dt: f32, sounds: &mut Vec<Sfx>) {
        let mut money = false;
        let mut healed = false;
        for d in &mut self.drops {
            d.age += dt;
            if d.z > 0.0 || d.vz > 0.0 {
                d.vz -= 320.0 * dt;
                d.z += d.vz * dt;
                if d.z <= 0.0 {
                    d.z = 0.0;
                    d.vz = if d.vz < -40.0 { -d.vz * 0.35 } else { 0.0 };
                }
            }
            let to = sub(self.pos, d.pos);
            let dist = len(to);
            let n = norm(to);
            if dist < self.kit.magnet() && d.pulled.is_none() {
                // A flick to one side first makes it swoop in.
                let side = if self.juice.rng.below(2) == 0 { 1.0 } else { -1.0 };
                d.vel = [-n[1] * side * 70.0 - n[0] * 40.0, n[0] * side * 70.0 - n[1] * 40.0];
                d.pulled = Some(0.0);
            }
            match &mut d.pulled {
                Some(p) => {
                    *p += dt;
                    let speed = 110.0 + 320.0 * p.min(1.5);
                    let k = if *p > 0.6 { 1.0 } else { (8.0 * dt).min(1.0) };
                    d.vel = [d.vel[0] + (n[0] * speed - d.vel[0]) * k, d.vel[1] + (n[1] * speed - d.vel[1]) * k];
                }
                None => {
                    let drag = (1.0 - 4.0 * dt).max(0.0);
                    d.vel = [d.vel[0] * drag, d.vel[1] * drag];
                }
            }
            let step = [d.vel[0] * dt, d.vel[1] * dt];
            // A fast drop would otherwise step past the player.
            if dist < crate::tuning::tuning().pickup || (d.pulled.is_some() && len(step) >= dist) {
                match d.kind {
                    DropKind::Money(v) => {
                        self.xp_frac += v as f32 * self.kit.mods.greed;
                        self.xp += self.xp_frac as u32;
                        self.xp_frac = self.xp_frac.fract();
                        money = true;
                        if self.juice.motes.len() < 24 {
                            self.juice.motes.push(juice::Mote { pos: d.pos, age: 0.0 });
                        }
                    }
                    DropKind::Chest => {
                        self.chests += 1;
                        sounds.push(Sfx::LevelUp);
                        self.juice.shout("A CHEST", GOLD, 0.9);
                        self.juice.flash(self.pos, 20.0, [1.8, 1.4, 0.6], 9.0, 0.8);
                    }
                    DropKind::Health => {
                        let gain = self.difficulty.heal().min(self.kit.max_hp() - self.hp);
                        self.hp += gain;
                        healed = gain > 0.0 || healed;
                        if gain > 0.0 {
                            self.juice.popup(self.pos, gain, Tone::Heal);
                        }
                    }
                }
                d.age = -1.0;
            } else {
                d.pos = [d.pos[0] + step[0], d.pos[1] + step[1]];
            }
        }
        self.drops.retain(|d| d.age >= 0.0);
        if money {
            let (since, n) = self.chain;
            let n = if since < 0.5 { n + 1 } else { 0 };
            self.chain = (0.0, n);
            sounds.push(Sfx::Pickup(n.min(20) as u8));
            self.juice.burst(self.pos, 10.0, 2, [0.0; 2], 0.0, [10.0, 30.0], juice::gold(), 0.8, 0.25, 1.0);
        }
        if healed {
            sounds.push(Sfx::Heal);
            self.juice.burst(self.pos, 8.0, 14, [0.0; 2], 0.0, [10.0, 40.0], [0.4, 1.0, 0.5], 1.2, 0.7, 1.0);
        }
    }

    /// Crows peck at the farm until something comes near or a shot goes off, then take to the sky.
    fn flock(&mut self, dt: f32, sounds: &mut Vec<Sfx>) {
        let mut cawed = None;
        for b in &mut self.birds {
            b.anim += dt;
            if b.flying {
                b.vz += 40.0 * dt;
                b.z += b.vz * dt;
                b.pos = [b.pos[0] + b.vel[0] * dt, b.pos[1] + b.vel[1] * dt];
                continue;
            }
            if b.z > 0.0 {
                b.z = (b.z - 35.0 * dt).max(0.0);
                b.pos = [b.pos[0] + b.vel[0] * dt, b.pos[1] + b.vel[1] * dt];
            }
            let startled = len(sub(b.pos, self.pos)) < 40.0 || self.noise.is_some_and(|n| len(sub(b.pos, n)) < 110.0);
            if startled {
                let away = sub(b.pos, self.pos);
                let a = away[1].atan2(away[0]) + self.juice.rng.range(-0.7, 0.7);
                let speed = self.juice.rng.range(55.0, 85.0);
                b.vel = [a.cos() * speed, a.sin() * speed];
                b.vz = self.juice.rng.range(30.0, 55.0);
                b.flying = true;
                b.flip = b.vel[0] < 0.0;
                cawed = Some(b.pos);
            }
        }
        self.birds.retain(|b| b.z < 160.0);
        if let Some(at) = cawed
            && self.hush[4] <= 0.0
        {
            let (pan, gain) = self.place(at);
            sounds.push(Sfx::Caw { pan, gain: gain.max(60) });
            self.hush[4] = 1.2;
        }
        if self.birds.len() < MAX_BIRDS
            && self.juice.rng.range(0.0, 1.0) < dt / 5.0
            && let Some(land) = self.perch(140.0)
        {
            let a = self.juice.rng.range(0.0, TAU);
            let vel = [a.cos() * 20.0, a.sin() * 20.0];
            let fall = 70.0 / 35.0;
            let pos = [land[0] - vel[0] * fall, land[1] - vel[1] * fall];
            self.birds.push(Bird { pos, z: 70.0, vel, vz: 0.0, flying: false, anim: 0.0, flip: vel[0] < 0.0 });
        }
    }

    /// The night around the fight: minute calls, the horde's warning, fireflies, glints, groans and a heartbeat.
    fn ambience(&mut self, dt: f32, sounds: &mut Vec<Sfx>, flow: &mut Flow) {
        for h in &mut self.hush {
            *h -= dt;
        }
        let minute = (self.t / 60.0) as usize;
        if horde_now(self.t, dt) && !self.calm {
            self.juice.banner("A HORDE CLAWS UP", RED, 2.6);
            self.juice.shake(0.3);
            flow.rumble = flow.rumble.max(Some(Rumble::Hit));
            sounds.push(Sfx::Horde);
        } else if minute > ((self.t - dt) / 60.0) as usize && (1..10).contains(&minute) {
            self.juice.banner(format!("{minute}:00   {}", MINUTE_LINES[minute - 1]), WHITE, 3.0);
            sounds.push(Sfx::Bell);
            // Where they will rise from now on.
            self.juice.ring(self.pos, spawn_ring(self.t)[0], 1.6, [0.85, 0.2, 0.15]);
        }

        let night = (1.0 - self.t / dawn()).max(0.0);
        let fireflies = self.juice.particles.iter().filter(|p| p.wander).count();
        if (fireflies as f32) < 4.0 + 14.0 * night && self.juice.rng.range(0.0, 1.0) < dt * 4.0 {
            let a = self.juice.rng.range(0.0, TAU);
            let r = self.juice.rng.range(30.0, 130.0);
            let life = self.juice.rng.range(5.0, 9.0);
            let pos = [self.pos[0] + a.cos() * r, self.pos[1] + a.sin() * r];
            let z = self.juice.rng.range(4.0, 18.0);
            self.juice.particle(Particle {
                pos,
                z,
                vel: [0.0; 2],
                vz: 0.0,
                age: 0.0,
                life,
                color: juice::firefly(),
                size: 1.0,
                shrink: false,
                gravity: 0.0,
                drag: 0.0,
                glow: 1.0,
                wander: true,
            });
        }

        if !self.drops.is_empty() && self.juice.rng.range(0.0, 1.0) < dt * 6.0 {
            let i = self.juice.rng.below(self.drops.len());
            let d = &self.drops[i];
            let (pos, z) = (d.pos, d.z + 6.0);
            if len(sub(pos, self.pos)) < 150.0 {
                self.juice.particle(Particle {
                    pos,
                    z,
                    vel: [0.0; 2],
                    vz: 0.0,
                    age: 0.0,
                    life: 0.25,
                    color: [1.0, 1.0, 0.9],
                    size: 1.2,
                    shrink: true,
                    gravity: 0.0,
                    drag: 0.0,
                    glow: 1.0,
                    wander: false,
                });
            }
        }

        if self.hush[2] <= 0.0 && !self.zombies.is_empty() {
            let i = self.juice.rng.below(self.zombies.len());
            let z = &self.zombies[i];
            let at = z.pos;
            if z.rise >= 1.0 && len(sub(at, self.pos)) < 160.0 {
                let (pan, gain) = self.place(at);
                sounds.push(Sfx::Groan { pan, gain });
                self.hush[2] = self.juice.rng.range(0.7, 2.4);
            }
        }

        let health = self.hp / self.kit.max_hp();
        if health < 0.3 {
            self.heartbeat -= dt;
            if self.heartbeat <= 0.0 {
                sounds.push(Sfx::Heartbeat);
                self.heartbeat = 0.5 + health * 2.0;
            }
        }
    }

    /// Bars and counters easing toward their real values.
    fn feel(&mut self, dt: f32) {
        self.xp_shown += (self.xp as f32 - self.xp_shown) * (10.0 * dt).min(1.0);
        if self.hurt < self.difficulty.hurt_time() - 0.35 {
            self.hp_shown = (self.hp_shown - 60.0 * dt).max(self.hp);
        }
        self.hp_shown = self.hp_shown.max(self.hp);
        self.chain.0 += dt;
        self.kill_bump += dt;
    }

    /// Stereo pan (-100 left to 100 right) and loudness (0 to 255) for a sound at `at`, from where the player faces.
    fn place(&self, at: [f32; 2]) -> (i8, u8) {
        let d = sub(at, self.pos);
        let right = [-self.view_yaw.sin(), self.view_yaw.cos()];
        let n = norm(d);
        let pan = (n[0] * right[0] + n[1] * right[1]) * 0.8;
        let gain = (1.0 - len(d) / 260.0).clamp(0.0, 1.0);
        ((pan * 100.0) as i8, (gain * 255.0) as u8)
    }

    /// How dread the music should feel, 0 to 1: the night wearing on and the dead closing in.
    pub fn dread(&self) -> f32 {
        if self.calm || !matches!(self.phase, Phase::Playing | Phase::LevelUp { .. }) {
            return 0.0;
        }
        let close = self.zombies.iter().filter(|z| len(sub(z.pos, self.pos)) < 90.0).count() as f32;
        (0.25 + 0.35 * self.t / dawn() + close / 40.0).min(1.0)
    }

    /// Top-left of the view in world pixels, rounded to whole screen pixels.
    fn camera(&self) -> [f32; 2] {
        let x = (self.pos[0] - VIEW_W / 2.0).clamp(0.0, (self.map.width_px() - VIEW_W).max(0.0));
        let y = (self.pos[1] - 8.0 - VIEW_H / 2.0).clamp(0.0, (self.map.height_px() - VIEW_H).max(0.0));
        [(x * SCALE).round() / SCALE, (y * SCALE).round() / SCALE]
    }
}

const CHOICE_TOP: f32 = 160.0;
const PAUSE_CHOICES: [&str; 4] = ["Resume", "Settings", "New farm", "Quit to title"];
const CHOICE_ROW: f32 = 150.0;

/// Which level-up or pause row a bottom-screen touch at `y` lands on.
fn choice_row(y: f32) -> Option<usize> {
    let row = ((y - CHOICE_TOP) / CHOICE_ROW).floor();
    (0.0..4.0).contains(&row).then_some(row as usize)
}

/// Lands a hit: damage (now and then doubled), knockback, a red flash, a squash, a number and a spray of blood.
/// Returns whether it was a critical hit.
fn strike(z: &mut Zombie, juice: &mut Juice, damage: f32, dir: [f32; 2], force: f32, crit_chance: f32) -> bool {
    let crit = juice.rng.range(0.0, 1.0) < crit_chance;
    let dealt = if crit { damage * 2.0 } else { damage };
    z.hp -= dealt;
    z.flash = FLASH_TIME;
    z.hurt = HURT_FRAMES;
    z.squash = 1.0;
    z.knock = dir;
    let k = force / z.kind.stats().mass / if z.boss { 6.0 } else { 1.0 };
    z.push = [dir[0] * k, dir[1] * k];
    juice.popup(z.pos, dealt, if crit { Tone::Crit } else { Tone::Hit });
    if z.kind.stats().heavy && !z.boss && (crit || force > 100.0) {
        z.stagger = crate::tuning::tuning().stagger;
    }
    juice.burst(z.pos, 8.0, if crit { 6 } else { 3 }, dir, 0.6, [30.0, 90.0], juice::blood(), 1.4, 0.4, 0.0);
    crit
}

/// Whether a horde breaks out during the frame ending at `t`; one comes every ninety seconds.
fn horde_now(t: f32, dt: f32) -> bool {
    (t - dt).rem_euclid(90.0) > t.rem_euclid(90.0)
}

const MINUTE_LINES: [&str; 9] = [
    "They know you're here",
    "The big ones are waking",
    "Keep moving",
    "The ground won't stay still",
    "Halfway to dawn",
    "They're getting hungry",
    "The sky is paling",
    "Two minutes to dawn",
    "Hold on. Almost dawn.",
];

/// Where zombies erupt `t` seconds into the night.
fn spawn_ring(t: f32) -> [f32; 2] {
    let k = (t / dawn()).clamp(0.0, 1.0);
    [0, 1].map(|i| spawn_rings().0[i] + (spawn_rings().1[i] - spawn_rings().0[i]) * k)
}

fn clock(t: f32) -> String {
    let s = t as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_games_own_data_must_be_whole_and_name_its_sprites() {
        for file in TABLES {
            assert_eq!(check_own_table(file, built_in_table(file).unwrap()), Ok(()), "{file}");
        }
        assert!(check_table("tuning.txt", "[player]\nhp 120").is_ok(), "a mod sets only what it changes");
        assert!(check_own_table("tuning.txt", "[player]\nhp 120").unwrap_err().contains("doesn't set"));
        let effects = built_in_table("effects.txt").unwrap().replace("art SLASH", "art NOPE");
        assert_eq!(check_own_table("effects.txt", &effects), Err("effects.txt: no sprite called NOPE in art.txt".into()));
        let farm = built_in_table("farm.txt").unwrap().replace("BUSH 140", "PROP_MINE 140");
        assert!(check_table("farm.txt", &farm).is_ok() && check_own_table("farm.txt", &farm).is_err(), "only a mod's farm strews its props");
        for (from, to) in [("sprites BARN_RED", "sprites PROP_MINE"), ("near HAY 7", "near PROP_MINE 7")] {
            let farm = built_in_table("farm.txt").unwrap().replace(from, to);
            assert!(check_table("farm.txt", &farm).is_ok() && check_own_table("farm.txt", &farm).is_err(), "only a mod's farm builds with its props");
        }
        assert!(check_own_table("farm.txt", built_in_table("farm.txt").unwrap()).is_ok());
        let farm = built_in_table("farm.txt").unwrap().replace("patch g", "patch Z");
        assert_eq!(check_own_table("farm.txt", &farm), Err("farm.txt: no ground code Z in ground.txt".into()));
        let powerups = built_in_table("powerups.txt").unwrap().replacen("unlock    start", "unlock    start\nicon      PROP_MINE", 1);
        assert!(check_table("powerups.txt", &powerups).is_ok() && check_own_table("powerups.txt", &powerups).is_err(), "only a mod's icon can be its prop");
    }

    fn run(h: &mut Harvest, input: &Input, secs: f32) {
        let mut sounds = Vec::new();
        for _ in 0..(secs * 60.0) as usize {
            h.update(input, 1.0 / 60.0, &mut sounds);
            if let Phase::LevelUp { .. } = h.phase {
                h.update(&Input { pressed: Buttons::X, ..Default::default() }, 1.0 / 60.0, &mut sounds);
            }
        }
    }

    #[test]
    fn standing_still_the_pistol_kills_and_the_horde_wins_eventually() {
        let mut h = Harvest::new(5);
        run(&mut h, &Input::default(), 60.0);
        assert!(h.kills > 0, "{}", h.summary());
        assert!(h.level > 1, "{}", h.summary());
        run(&mut h, &Input::default(), 540.0);
        assert!(matches!(h.phase, Phase::Over { .. }), "{}", h.summary());
    }

    #[test]
    fn up_walks_along_the_heading_and_walls_stop_it() {
        let mut h = Harvest::calm(9);
        h.view_yaw = 0.0;
        let start = h.pos;
        run(&mut h, &Input { held: Buttons::UP, ..Default::default() }, 0.5);
        assert!(h.pos[0] > start[0] && (h.pos[1] - start[1]).abs() < 1.0);
        run(&mut h, &Input { held: Buttons::UP, ..Default::default() }, 120.0);
        assert!(h.pos[0] < h.map.width_px() - TILE);
    }

    #[test]
    fn y_and_a_turn_without_moving_and_the_shoulder_triggers_do_not() {
        let mut h = Harvest::calm(9);
        run(&mut h, &Input::default(), 0.05);
        let (start, yaw) = (h.pos, h.view_yaw);
        run(&mut h, &Input { held: Buttons::L2 | Buttons::R2, ..Default::default() }, 0.25);
        assert_eq!(h.view_yaw, yaw);
        run(&mut h, &Input { held: Buttons::A, ..Default::default() }, 0.25);
        assert_eq!(h.pos, start);
        assert!((h.view_yaw - yaw).rem_euclid(TAU) > 0.5);
        run(&mut h, &Input { held: Buttons::Y, ..Default::default() }, 0.25);
        assert!((h.view_yaw - yaw).rem_euclid(TAU) < 0.01 || (h.view_yaw - yaw).rem_euclid(TAU) > TAU - 0.01);
    }

    #[test]
    fn the_a_that_starts_a_run_does_not_turn() {
        let mut h = Harvest::calm(9);
        let yaw = h.view_yaw.rem_euclid(TAU);
        run(&mut h, &Input { held: Buttons::A, ..Default::default() }, 0.2);
        assert_eq!(h.view_yaw, yaw);
        run(&mut h, &Input::default(), 0.05);
        run(&mut h, &Input { held: Buttons::A, ..Default::default() }, 0.2);
        assert_ne!(h.view_yaw, yaw);
    }

    #[test]
    fn start_pauses_and_the_menu_resumes_opens_settings_restarts_or_quits() {
        let mut h = Harvest::new(5);
        let press = |h: &mut Harvest, b: Buttons| h.update(&Input { pressed: b, ..Default::default() }, 1.0 / 60.0, &mut Vec::new());
        run(&mut h, &Input::default(), 1.0);
        press(&mut h, Buttons::START);
        let t = h.t;
        run(&mut h, &Input::default(), 2.0);
        assert_eq!(h.t, t, "nothing moves while paused");
        assert_eq!(h.escape(), Buttons::B, "Esc backs out of the pause menu");
        press(&mut h, Buttons::B);
        assert!(h.menu.is_none());
        assert_eq!(h.escape(), Buttons::START, "and pauses a run in play");
        press(&mut h, Buttons::START);
        press(&mut h, Buttons::DOWN);
        assert!(press(&mut h, Buttons::A).settings);
        assert!(h.menu.is_some(), "the run stays paused under the settings");
        press(&mut h, Buttons::DOWN);
        press(&mut h, Buttons::A);
        assert!(h.menu.is_none() && h.t == 0.0, "New farm starts over");
        press(&mut h, Buttons::START);
        let tap = Input { touch: Some([500.0, CHOICE_TOP + CHOICE_ROW * 3.5]), touch_started: true, ..Default::default() };
        assert!(h.update(&tap, 1.0 / 60.0, &mut Vec::new()).quit, "a tap on the last choice quits");
    }

    #[test]
    fn b_turns_all_the_way_around_quickly() {
        let mut h = Harvest::calm(9);
        let yaw = h.view_yaw;
        h.update(&Input { pressed: Buttons::B, ..Default::default() }, 1.0 / 60.0, &mut Vec::new());
        run(&mut h, &Input::default(), SPIN_TIME * 0.8);
        assert_eq!(h.showing(), h.turned_from, "the player keeps the old heading while the camera swings");
        run(&mut h, &Input::default(), SPIN_TIME * 0.2 + 0.05);
        assert!(((h.view_yaw - yaw).rem_euclid(TAU) - PI).abs() < 1e-3, "the camera swings right round");
        assert_eq!(h.showing(), h.aim, "and turns to match as it lands");
    }

    #[test]
    fn left_and_right_strafe_across_the_heading() {
        let mut h = Harvest::calm(9);
        h.view_yaw = -PI / 2.0;
        let start = h.pos;
        run(&mut h, &Input { held: Buttons::RIGHT, ..Default::default() }, 0.3);
        assert!(h.pos[0] > start[0] && (h.pos[1] - start[1]).abs() < 1.0);
    }

    #[test]
    fn a_tap_on_the_map_throws_a_grenade_that_lands_there() {
        let mut h = Harvest::calm(12);
        let cam = h.camera();
        let target = [h.pos[0] + 60.0, h.pos[1]];
        h.zombies.push(Zombie::new(Kind::named("skinny"), target, 20.0, 1.0, 0.0));
        let touch = [(target[0] - cam[0]) * SCALE, (target[1] - cam[1]) * SCALE];
        let mut sounds = Vec::new();
        h.update(&Input { touch: Some(touch), touch_started: true, ..Default::default() }, 0.016, &mut sounds);
        assert_eq!(h.thrown.len(), 1);
        assert!(h.grenades < h.kit.grenades());
        h.update(&Input::default(), crate::tuning::tuning().grenade_flight, &mut sounds);
        assert!(h.thrown.is_empty());
        assert!(h.kills == 1 || h.zombies.iter().all(|z| z.hp <= 0.0));
    }

    #[test]
    fn a_tap_picks_an_upgrade() {
        let mut h = Harvest::new(2);
        let choice = |id| Choice { up: book().index(id).unwrap(), rare: false, found: false };
        h.phase = Phase::LevelUp { choices: [choice("shotgun"), choice("pistol"), choice("running_shoes")], selected: 0, t: 1.0, picked: None };
        let tap = Input { touch: Some([500.0, CHOICE_TOP + CHOICE_ROW * 1.5]), touch_started: true, ..Default::default() };
        let before = h.kit.level(book().index("pistol").unwrap());
        h.update(&tap, 0.016, &mut Vec::new());
        assert_eq!(h.kit.level(book().index("pistol").unwrap()), before + 1);
        h.update(&Input::default(), PICK_TIME, &mut Vec::new());
        assert!(matches!(h.phase, Phase::Playing));
    }

    #[test]
    fn a_shot_barrel_blows_up_nearby_zombies_and_opens_its_cell() {
        let mut h = Harvest::new(6);
        let barrel = h.barrels.first().expect("seed 6 has barrels");
        let c = barrel.center();
        h.zombies.push(Zombie::new(Kind::named("big"), [c[0] + 20.0, c[1]], 50.0, 1.0, 0.0));
        let mut sounds = Vec::new();
        h.bullets.push(Bullet { pos: [c[0], c[1] - 6.0], vel: [1.0, 0.0], damage: 50.0, pierce: 0, life: 1.0, force: 30.0, hit: Vec::new(), look: None, age: 0.0 });
        h.move_bullets(0.016, &mut sounds);
        assert!(h.blow(0.016, &mut sounds));
        assert!(h.zombies[0].hp <= 0.0);
        assert!(!h.map.solid_at(c[0], c[1]));
    }

    #[test]
    fn resuming_with_a_held_doesnt_turn_the_camera() {
        let mut h = Harvest::calm(4);
        h.update(&Input::default(), 1.0 / 60.0, &mut Vec::new());
        h.update(&Input { pressed: Buttons::START, ..Default::default() }, 1.0 / 60.0, &mut Vec::new());
        assert_eq!(h.menu, Some(0));
        h.update(&Input { pressed: Buttons::A, held: Buttons::A, ..Default::default() }, 1.0 / 60.0, &mut Vec::new());
        assert_eq!(h.menu, None);
        let yaw = h.view_yaw;
        h.update(&Input { held: Buttons::A, ..Default::default() }, 0.5, &mut Vec::new());
        assert_eq!(h.view_yaw, yaw.rem_euclid(TAU), "the A that resumed has to be let go before it turns");
    }

    #[test]
    fn a_piercing_shot_hits_each_zombie_once_while_the_dead_are_cleared() {
        let mut h = penned();
        let at = [h.pos[0] + 20.0, h.pos[1]];
        let mut dead = Zombie::new(Kind::named("skinny"), [h.pos[0] - 40.0, h.pos[1]], 0.0, 1.0, 0.0);
        dead.id = 1;
        let mut live = Zombie::new(Kind::named("skinny"), at, 1000.0, 1.0, 0.0);
        live.id = 2;
        h.zombies = vec![dead, live];
        h.bullets = vec![Bullet { pos: [at[0], at[1] - 6.0], vel: [0.0; 2], damage: 10.0, pierce: 5, life: 1.0, force: 0.0, hit: Vec::new(), look: None, age: 0.0 }];
        let mut sounds = Vec::new();
        h.move_bullets(0.016, &mut sounds);
        let once = h.zombies[1].hp;
        assert!(once < 1000.0);
        h.reap(&mut sounds, &mut Flow::default());
        h.move_bullets(0.016, &mut sounds);
        assert_eq!(h.zombies[0].hp, once, "the zombie that took the shot's place isn't hit again");
    }

    #[test]
    fn shots_fly_past_bushes_and_stop_at_buildings() {
        let mut h = Harvest::calm(6);
        let bottom = |a: Art| h.map.props.iter().find(|p| p.frames[0] == a).map(|p| [p.x + TILE / 2.0, p.base - TILE / 2.0]).expect("seed 6 has one");
        let (bush, house) = (bottom(art::BUSH), bottom(art::HOUSE));
        assert!(h.map.solid_at(bush[0], bush[1]));
        let mut stops = |at: [f32; 2]| {
            h.bullets = vec![Bullet { pos: [at[0], at[1] - 6.0], vel: [0.0; 2], damage: 1.0, pierce: 0, life: 1.0, force: 0.0, hit: Vec::new(), look: None, age: 0.0 }];
            h.move_bullets(0.016, &mut Vec::new());
            h.bullets.is_empty()
        };
        assert!(!stops(bush), "a bush blocks the way but not a shot");
        assert!(stops(house));
    }

    /// A calm run on a dirt map with a gateless rail fence round the player, from tile (7, 4) to (13, 9).
    fn penned() -> Harvest {
        let mut src = map::MapSource::new("Pen", [20, 14]);
        src.props.push((0, PropLine::Fence { picket: false, rect: [7, 4, 7, 6], gate: None, tough: Default::default() }));
        Harvest::calm_on(1, &src).unwrap()
    }

    #[test]
    fn zombies_chew_through_a_fence_to_get_at_the_player() {
        let mut h = penned();
        let fence = h.map.props.len();
        h.zombies.push(Zombie::new(Kind::named("skinny"), [3.5 * TILE, 7.5 * TILE], 1000.0, 1.0, 0.0));
        for _ in 0..60 * 20 {
            h.update(&Input::default(), 1.0 / 60.0, &mut Vec::new());
            if h.hp < h.kit.max_hp() {
                break;
            }
        }
        assert!(h.map.props.len() < fence, "a piece of fence is gone");
        assert!(h.hp < h.kit.max_hp(), "and the zombie came through it");
    }

    #[test]
    fn shots_and_blasts_break_cover_but_fly_past_a_fence() {
        let mut h = penned();
        let post = [7.5 * TILE, 6.5 * TILE];
        let shoot = |h: &mut Harvest| {
            h.bullets = vec![Bullet { pos: [post[0], post[1] - 6.0], vel: [0.0; 2], damage: 10.0, pierce: 0, life: 1.0, force: 0.0, hit: Vec::new(), look: None, age: 0.0 }];
            h.move_bullets(0.016, &mut Vec::new());
        };
        let i = h.map.breakable_at(post[0], post[1]).unwrap();
        shoot(&mut h);
        assert_eq!((h.bullets.len(), h.map.props[i].hp), (1, 40.0), "a fence isn't cover");
        let mut src = map::MapSource::new("Pen", [20, 14]);
        src.props.push((0, PropLine::Fence { picket: false, rect: [7, 4, 7, 6], gate: None, tough: map::Tough { hp: Some(30.0), cover: Some(true) } }));
        h = Harvest::calm_on(1, &src).unwrap();
        shoot(&mut h);
        assert!(h.bullets.is_empty() && h.map.props[i].hp == 20.0 && h.map.props[i].flash > 0.0, "cover that breaks takes the shot");
        h.explode([post[0] - 20.0, post[1]], crate::tuning::tuning().barrel_radius, crate::tuning::tuning().barrel_damage, &mut Vec::new());
        for _ in 0..10 {
            h.crumble(0.016, &mut Vec::new());
        }
        assert!(!h.map.solid_at(post[0], post[1]), "a blast takes it down");
    }

    #[test]
    fn the_spawn_ring_closes_in_toward_dawn() {
        assert_eq!(spawn_ring(0.0), spawn_rings().0);
        assert_eq!(spawn_ring(dawn()), spawn_rings().1);
        assert!(spawn_ring(dawn() / 2.0)[0] < spawn_rings().0[0]);
    }

    #[test]
    fn a_rising_zombie_is_rooted_and_harmless_until_it_is_out() {
        let mut h = Harvest::calm(3);
        let at = [h.pos[0] + 4.0, h.pos[1]];
        h.zombies.push(Zombie::new(Kind::named("skinny"), at, 1000.0, 0.0, 0.0));
        let mut sounds = Vec::new();
        h.update(&Input::default(), crate::tuning::tuning().rise * 0.5, &mut sounds);
        assert_eq!(h.zombies[0].pos, at);
        assert_eq!(h.hp, h.kit.max_hp());
        for _ in 0..60 {
            h.update(&Input::default(), 1.0 / 60.0, &mut sounds);
        }
        assert!(h.hp < h.kit.max_hp());
    }

    #[test]
    fn the_turned_player_leads_the_dead_off_the_way_they_faced() {
        let mut h = Harvest::calm(4);
        h.zombies.clear();
        let open = |h: &Harvest, p: [f32; 2]| !h.map.hits(p[0], p[1], crate::tuning::tuning().zombie_half());
        let rows = (0..21).map(|r| (r as f32 - 10.0) * TILE);
        let spot = rows.flat_map(|dy| (1..60).map(move |k| [h.map.spawn[0] + k as f32 * 4.0, h.map.spawn[1] + dy])).find(|p| open(&h, *p) && open(&h, [p[0] + 120.0, p[1]]) && (-8..30).all(|i| open(&h, [p[0] + i as f32 * 4.0, p[1]]))).expect("an open run east");
        h.pos = spot;
        h.view_yaw = 0.0;
        h.aim = [1.0, 0.0];
        h.zombies.push(Zombie::new(Kind::named("skinny"), [spot[0] - 30.0, spot[1]], 50.0, 1.0, 0.0));
        h.hp = 0.0;
        h.phase = Phase::Over { t: TURNED, won: false };
        for _ in 0..180 {
            h.update(&Input::default(), 1.0 / 60.0, &mut Vec::new());
        }
        assert!(h.pos[0] - spot[0] > 30.0, "the player walks on the way they faced");
        assert!(len(sub(h.zombies[0].pos, h.pos)) < MARCH_CLOSE * 1.5, "and the dead keep with them");
    }

    #[test]
    fn a_map_smaller_than_the_view_draws_and_takes_a_tap() {
        let mut h = Harvest::calm_on(1, &MapSource::new("Tiny", MAP_MIN)).unwrap();
        h.draw(&mut crate::Frame::default());
        h.throw([SCREEN_W / 2.0, SCREEN_H / 2.0], &mut Vec::new());
        assert_eq!(h.camera(), [0.0; 2]);
    }

    #[test]
    fn dying_turns_the_player_then_offers_a_new_farm_at_the_same_difficulty() {
        let mut h = Harvest::new(4).at(Difficulty::Hard).without_enemies(0b10).looking_like("");
        let mut sounds = Vec::new();
        h.hp = 0.0;
        h.update(&Input::default(), 0.016, &mut sounds);
        assert!(matches!(h.phase, Phase::Over { won: false, .. }));
        let a = Input { pressed: Buttons::A, ..Default::default() };
        h.update(&Input::default(), TURN_SINK, &mut sounds);
        assert_eq!(player_pose(&h, Facing::Front).1, 0.0, "under the dirt");
        h.update(&a, 0.016, &mut sounds);
        assert!(matches!(h.phase, Phase::Over { .. }), "too soon to start over");
        h.update(&Input::default(), TURN_RISE - TURN_SINK, &mut sounds);
        assert!(sounds.contains(&Sfx::Turn));
        h.update(&Input::default(), TURNED - TURN_RISE + 0.7, &mut sounds);
        let (art, rise) = player_pose(&h, Facing::Front);
        assert_eq!(rise, 1.0);
        assert!(art::SKINNY_FRONT.contains(&art), "back up as one of them");
        h.update(&a, 0.016, &mut sounds);
        assert!(matches!(h.phase, Phase::Playing));
        assert_eq!(h.kills, 0);
        assert_eq!(h.difficulty, Difficulty::Hard);
        assert_eq!(h.kit.weapons.len(), 2, "and its kit");
        assert_eq!(h.enemies_off, 0b10, "the kinds turned off stay off");
        assert_eq!(h.look, "", "and the player keeps their look");
    }

    #[test]
    fn every_difficulty_starts_with_a_pistol_and_knife_and_easy_with_two_grenades() {
        for (d, grenades) in [(Difficulty::Easy, 2.0), (Difficulty::Normal, 1.0), (Difficulty::Hard, 1.0)] {
            let h = Harvest::new(1).at(d).with_kit("lantern");
            assert_eq!(h.kit.weapons.len(), 3, "{d:?}");
            assert_eq!((h.grenades, h.kit.grenades()), (grenades, grenades), "{d:?}");
        }
    }

    #[test]
    fn harder_difficulties_hit_harder_and_drop_less_health() {
        let bite = |d: Difficulty| {
            let mut h = Harvest::calm(3).at(d);
            h.zombies.push(Zombie::new(Kind::named("skinny"), [h.pos[0] + 3.0, h.pos[1]], 1000.0, 1.0, 0.0));
            h.update(&Input::default(), 1.0 / 60.0, &mut Vec::new());
            h.kit.max_hp() - h.hp
        };
        let [easy, normal, hard] = Difficulty::ALL.map(bite);
        assert!(easy < normal && normal < hard, "{easy} {normal} {hard}");
        assert!(Difficulty::Easy.health_drop() < Difficulty::Hard.health_drop());
    }

    #[test]
    fn a_kill_drops_a_body_a_number_and_money_that_hops() {
        let mut h = Harvest::calm(3);
        h.kit.raise(book().index("pistol").unwrap(), 1);
        let at = [-30.0, 30.0, 0.0].into_iter().flat_map(|dx| [-30.0, 30.0].map(|dy| [h.pos[0] + dx, h.pos[1] + dy])).find(|p| !h.map.hits(p[0], p[1], crate::tuning::tuning().zombie_half())).unwrap();
        h.zombies.push(Zombie::new(Kind::named("skinny"), at, 5.0, 1.0, 0.0));
        let mut sounds = Vec::new();
        for _ in 0..90 {
            h.update(&Input::default(), 1.0 / 60.0, &mut sounds);
        }
        assert_eq!(h.kills, 1);
        assert_eq!(h.juice.corpses.len(), 1);
        assert!(sounds.iter().any(|s| matches!(s, Sfx::Kill(_))));
        let d = h.drops.first().map(|d| d.age).unwrap_or(0.0);
        assert!(h.drops.is_empty() || d > 0.0);
        let mut h = Harvest::calm(3);
        h.zombies.push(Zombie::new(Kind::named("skinny"), at, 5.0, 1.0, 0.0));
        h.zombies[0].hp = 0.0;
        h.reap(&mut sounds, &mut Flow::default());
        assert!(h.drops[0].vz > 0.0, "money hops out of the body");
    }

    #[test]
    fn a_hit_shows_a_number_and_shoves_light_zombies_further() {
        let mut j = Juice::new(1);
        let mut kid = Zombie::new(Kind::named("kid"), [0.0; 2], 100.0, 1.0, 0.0);
        let mut big = Zombie::new(Kind::named("big"), [0.0; 2], 100.0, 1.0, 0.0);
        strike(&mut kid, &mut j, 10.0, [1.0, 0.0], 30.0, crate::tuning::tuning().crit);
        strike(&mut big, &mut j, 10.0, [1.0, 0.0], 30.0, crate::tuning::tuning().crit);
        assert_eq!(j.popups.len(), 2);
        assert!(kid.push[0] > big.push[0] * 3.0);
        assert!(kid.squash > 0.0 && kid.hurt > 0.0);
    }

    #[test]
    fn the_ground_churns_before_a_zombie_breaks_out() {
        let mut h = Harvest::new(8);
        h.owed = 1.0;
        h.spawn(0.0);
        assert_eq!((h.tells.len(), h.zombies.len()), (1, 0));
        let mut sounds = Vec::new();
        h.rise(TELL_TIME * 0.5, &mut sounds);
        assert_eq!(h.zombies.len(), 0);
        h.rise(TELL_TIME, &mut sounds);
        assert_eq!((h.tells.len(), h.zombies.len()), (0, 1));
        assert_eq!(h.zombies[0].rise, 0.0);
    }

    #[test]
    fn a_full_bar_slows_time_then_the_cards_wait_before_taking_input() {
        let mut h = Harvest::calm(4);
        h.xp = h.next_xp;
        let mut sounds = Vec::new();
        h.update(&Input::default(), 1.0 / 60.0, &mut sounds);
        assert!(h.leveling > 0.0 && matches!(h.phase, Phase::Playing));
        assert!(sounds.contains(&Sfx::LevelUp));
        let t = h.t;
        h.update(&Input::default(), 0.1, &mut sounds);
        assert!(h.t - t < 0.05, "slow motion");
        h.update(&Input::default(), LEVEL_SLOW, &mut sounds);
        assert!(matches!(h.phase, Phase::LevelUp { picked: None, .. }));
        let x = Input { pressed: Buttons::X, ..Default::default() };
        h.update(&x, 1.0 / 60.0, &mut sounds);
        assert!(matches!(h.phase, Phase::LevelUp { picked: None, .. }), "too soon");
        h.update(&Input::default(), CARDS_IN, &mut sounds);
        for turn in [Buttons::A, Buttons::START] {
            h.update(&Input { pressed: turn, ..Default::default() }, 1.0 / 60.0, &mut sounds);
            assert!(matches!(h.phase, Phase::LevelUp { picked: None, .. }), "{turn:?} doesn't pick");
        }
        h.update(&x, 1.0 / 60.0, &mut sounds);
        assert!(matches!(h.phase, Phase::LevelUp { picked: Some(_), .. }));
        h.update(&Input::default(), PICK_TIME, &mut sounds);
        assert!(matches!(h.phase, Phase::Playing));
    }

    #[test]
    fn getting_hit_freezes_the_world_for_a_moment() {
        let mut h = Harvest::calm(3);
        h.zombies.push(Zombie::new(Kind::named("skinny"), [h.pos[0] + 3.0, h.pos[1]], 1000.0, 1.0, 0.0));
        let mut sounds = Vec::new();
        h.update(&Input::default(), 1.0 / 60.0, &mut sounds);
        assert!(h.hp < h.kit.max_hp() && h.juice.frozen());
        let t = h.t;
        h.update(&Input::default(), 1.0 / 60.0, &mut sounds);
        assert_eq!(h.t, t);
    }

    #[test]
    fn a_shot_scares_the_crows_off() {
        let mut h = Harvest::calm(5);
        h.birds = vec![Bird { pos: [h.pos[0] + 80.0, h.pos[1]], z: 0.0, vel: [0.0; 2], vz: 0.0, flying: false, anim: 0.0, flip: false }];
        let mut sounds = Vec::new();
        h.flock(0.016, &mut sounds);
        assert!(!h.birds[0].flying);
        h.noise = Some(h.pos);
        h.flock(0.016, &mut sounds);
        assert!(h.birds[0].flying);
        assert!(sounds.iter().any(|s| matches!(s, Sfx::Caw { .. })));
    }

    #[test]
    fn quick_pickups_climb_the_scale() {
        let mut h = Harvest::calm(5);
        let mut sounds = Vec::new();
        for _ in 0..3 {
            h.drops.push(Drop { kind: DropKind::Money(1), pos: h.pos, z: 0.0, vz: 0.0, vel: [0.0; 2], age: 0.0, pulled: None });
            h.collect(0.1, &mut sounds);
        }
        let steps: Vec<u8> = sounds.iter().filter_map(|s| if let Sfx::Pickup(n) = s { Some(*n) } else { None }).collect();
        assert_eq!(steps, [0, 1, 2]);
    }

    #[test]
    fn a_horde_brings_a_leader_whose_chest_opens_a_find() {
        let mut h = Harvest::new(3).with_progress(Progress { kills: 100, ..Default::default() });
        h.t = 90.0;
        h.spawn(0.02);
        let leader = h.tells.iter().find(|t| t.leader).expect("the horde has a leader");
        assert!(leader.kind == Kind::named("big") && h.tells.iter().filter(|t| t.leader).count() == 1);
        h.drops.push(Drop { kind: DropKind::Chest, pos: h.pos, z: 0.0, vz: 0.0, vel: [0.0; 2], age: 0.0, pulled: None });
        h.collect(0.016, &mut Vec::new());
        for _ in 0..200 {
            if let Phase::LevelUp { choices, .. } = h.phase {
                assert!(choices.iter().any(|c| c.found));
                return;
            }
            h.update(&Input::default(), 0.016, &mut Vec::new());
        }
        panic!("the chest never opened");
    }

    #[test]
    fn a_chest_picked_up_during_a_level_up_still_brings_its_own() {
        let mut h = Harvest::calm(4);
        h.xp = h.next_xp;
        let x = Input { pressed: Buttons::X, ..Default::default() };
        h.update(&Input::default(), 1.0 / 60.0, &mut Vec::new());
        assert!(h.leveling > 0.0);
        for _ in 0..2 {
            h.drops.push(Drop { kind: DropKind::Chest, pos: h.pos, z: 0.0, vz: 0.0, vel: [0.0; 2], age: 0.0, pulled: None });
        }
        h.collect(0.016, &mut Vec::new());
        let mut level_ups = 0;
        for _ in 0..2000 {
            if let Phase::LevelUp { picked: None, t, .. } = h.phase
                && t > CARDS_IN
            {
                level_ups += 1;
                h.update(&x, 0.016, &mut Vec::new());
            } else {
                h.update(&Input::default(), 0.016, &mut Vec::new());
            }
        }
        assert_eq!((level_ups, h.level), (3, 2), "the full bar's level-up, then one for each chest");
    }

    /// A farm at the edge of dawn, with the player well away from home and nothing else about.
    fn before_dawn() -> Harvest {
        let mut h = Harvest::calm(4);
        h.kit.weapons.clear();
        h.zombies.clear();
        h.tells.clear();
        h.t = dawn() - 0.01;
        h.pos = h.map.spawn;
        h
    }

    #[test]
    fn dawn_wakes_the_harvester_instead_of_ending_the_night() {
        let mut h = before_dawn();
        let flow = h.update(&Input::default(), 0.02, &mut Vec::new());
        assert!(flow.ended.is_none() && matches!(h.phase, Phase::Playing), "the night goes on");
        let boss = h.zombies.iter().find(|z| z.boss).expect("the Harvester rises");
        let (to_home, to_boss) = (sub(h.map.home, h.pos), sub(boss.pos, h.pos));
        if len(to_home) > 1.0 {
            assert!(to_home[0] * to_boss[0] + to_home[1] * to_boss[1] <= 0.0, "it rises on the side away from home");
        }
    }

    #[test]
    fn a_run_started_past_dawn_wakes_the_harvester_at_once() {
        let mut h = Harvest::calm(4).starting_at(dawn() + 30.0);
        h.zombies.clear();
        h.update(&Input::default(), 0.02, &mut Vec::new());
        assert_eq!(h.zombies.iter().filter(|z| z.boss).count(), 1);
        h.update(&Input::default(), 0.02, &mut Vec::new());
        assert_eq!(h.zombies.iter().filter(|z| z.boss).count(), 1, "and only once");
    }

    #[test]
    fn past_dawn_the_dead_grow_one_percent_a_second() {
        let mut h = before_dawn();
        h.update(&Input::default(), 0.02, &mut Vec::new());
        h.zombies.push(Zombie::new(Kind::named("skinny"), [h.pos[0] + 200.0, h.pos[1]], 100.0, 1.0, 0.0));
        for _ in 0..10 {
            h.grow_fury(1.0);
        }
        assert!((h.fury - 1.01f32.powi(10)).abs() < 1e-4);
        let hp = h.zombies.last().unwrap().hp;
        assert!((hp - 100.0 * 1.01f32.powi(10)).abs() < 0.01, "{hp}");
        let bitten = |h: &mut Harvest| {
            h.zombies.retain(|z| !z.boss);
            h.zombies[0].pos = h.pos;
            h.hurt = 0.0;
            let before = h.hp;
            h.contact(&mut Vec::new(), &mut Flow::default());
            before - h.hp
        };
        let late = bitten(&mut h);
        h.fury = 1.0;
        assert!(late > bitten(&mut h) * 1.09, "bites grow too");
    }

    #[test]
    fn only_the_farmhouse_step_after_dawn_ends_the_night() {
        let mut h = before_dawn();
        h.t = dawn() - 5.0;
        h.pos = h.map.home;
        h.update(&Input::default(), 0.02, &mut Vec::new());
        assert!(matches!(h.phase, Phase::Playing), "home before dawn is just a house");
        let mut h = before_dawn();
        h.update(&Input::default(), 0.02, &mut Vec::new());
        h.zombies.clear();
        h.pos = h.map.home;
        let flow = h.update(&Input::default(), 0.02, &mut Vec::new());
        assert!(matches!(h.phase, Phase::Over { won: true, .. }));
        assert!(flow.ended.is_some_and(|r| r.won));
    }

    #[test]
    fn a_blast_staggers_a_big_zombie_in_place() {
        let mut j = Juice::new(1);
        let mut big = Zombie::new(Kind::named("big"), [0.0; 2], 500.0, 1.0, 0.0);
        strike(&mut big, &mut j, 10.0, [1.0, 0.0], 30.0, crate::tuning::tuning().crit);
        let stagger = big.stagger;
        strike(&mut big, &mut j, 10.0, [1.0, 0.0], 120.0, crate::tuning::tuning().crit);
        assert!(big.stagger > stagger && big.stagger == crate::tuning::tuning().stagger);
        let mut h = Harvest::calm(3);
        let at = [h.pos[0] + 60.0, h.pos[1]];
        h.zombies.push(big);
        h.zombies[0].pos = at;
        h.move_zombies(0.1);
        assert_eq!(h.zombies[0].pos, at);
    }

    #[test]
    fn a_tap_during_hit_stop_still_throws() {
        let mut h = Harvest::calm(12);
        h.juice.stop(0.1);
        let mut sounds = Vec::new();
        h.update(&Input { touch: Some([600.0, 300.0]), touch_started: true, ..Default::default() }, 0.016, &mut sounds);
        assert!(h.thrown.is_empty());
        h.update(&Input::default(), 0.1, &mut sounds);
        h.update(&Input::default(), 0.016, &mut sounds);
        assert_eq!(h.thrown.len(), 1);
    }
}

