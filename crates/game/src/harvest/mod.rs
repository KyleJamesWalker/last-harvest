//! Last Harvest: survive the night on a randomly generated farm while the horde closes in.

mod blend;
mod camp;
mod carve;
mod draw;
mod effects;
pub use effects::{SPRITES as SHOT_SPRITES, Shot, shot_named, shot_sprite_names};
mod enemies;
mod juice;
mod leader;
mod local;
mod map;
pub mod online;
mod photo;
pub mod replay;
mod remote;
mod props;
mod quick;
mod range;
pub mod sandbox;
mod player;
mod players;
mod powerups;
pub mod showcase;
mod sight;
mod tally;
#[cfg(test)]
pub use powerups::powerup_icon;
pub use weapons::{SwingShow, swing_shown};
pub(crate) use draw::powerup_icons;
mod single;
mod view3d;
mod weapons;

use crate::fmath as fm;
use std::f32::consts::{FRAC_PI_4, PI, TAU};

use crate::art::{self, Art};
use crate::rng::Rng;
use crate::{Buttons, Color, Difficulty, Input, SCREEN_H, SCREEN_W, Sfx};
use enemies::Enemy;
pub(crate) use enemies::{install_cards, off_from_ids, off_ids, roster};
pub use props::{MAX_FRAME as PROP_MAX_FRAME, MAX_FRAMES as PROP_MAX_FRAMES, Named as NamedProp, PREFIX as PROP_PREFIX, ByViews as PropByViews, PropFields, load_for_checks as load_props_for_checks, turned_depth, prop_fields, prop_text, props as named_props, set_props};
pub(crate) use props::{alpha_of, stand_of};
pub use map::{default_depth as default_prop_depth, ground_codes as map_ground, set_ground as set_map_ground, Sway as CropSway, MAP_EXTENSION, MAP_MAX, MAP_MIN, MAX_HP as PROP_MAX_HP, MapSource, map_props, parse_props, PropKind, PropLine, Tough as PropTough, check_map, valid_map_id, FARM_DIR, FARM_GROUND_END, FARM_ID_MAX, farm_file, farm_ids, farm_of_file, install_farms, set_farm, use_farm, use_farm_text, valid_farm_id};
pub use map::check_map_on_any_farm;
pub use enemies::{ABOUT_MAX, MAX_FRAME as CARD_MAX_FRAME, card_frame_most, MAX_WALK, flash_pixel, frame_limits, hurt_pixel};
pub use players::{BASE_KEYS as PLAYER_BASE_KEYS, ID_MAX as PLAYER_ID_MAX, PlayerFields, PlayerLook, default_start, looks as player_looks, player_fields, set_player_farm, set_player_start, use_player_farm, player_powerups, weapon_slots, weapons};
pub use enemies::{CardFields, FLAGS as CARD_FLAGS, NUMBERS as CARD_NUMBERS, SpawnMix, card_fields, set_card_field, spawn_mix};
use juice::{Corpse, Juice, Particle, Tone};
use map::{Map, TILE};
pub use range::{RangeSetup, effects_with, powerups_with};
pub use powerups::{DEFAULT_SOUND_DELAY, Progress, RUN_KIT, Run, WeaponLevels, found_from_ids, found_ids, hidden_for as hidden_powerups, owners as powerup_owners, powerup_id, powerup_index, powerup_zones, sound_group, sound_groups, valid_sound_group, unlock_conditions, weapon_behaviours, weapon_levels, weapon_patterns, weapon_sound_names, weapon_stats, weapon_targets};
use player::{ME, Player, PlayerId};
use powerups::{Kind as Card, book};
use weapons::{Bolt, Decoy, Kit, Patch};

/// Screen pixels per art pixel on the bottom screen's map, as `tuning.txt` [map] sets it.
fn map_scale() -> f32 {
    crate::tuning::tuning().map_scale.clamp(1.0, 8.0)
}

/// The art pixels the map shows across and down.
fn view_w() -> f32 {
    SCREEN_W / map_scale()
}

fn view_h() -> f32 {
    SCREEN_H / map_scale()
}
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
/// How near a spit's path passes the player to splash them.
const SPIT_REACH: f32 = 6.0;
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
#[cfg(test)]
pub(crate) const TURNED_FOR_TEST: f32 = TURNED;
/// Seconds the worn grenade flashes, and how bright, when one comes ready.
const READY_FLASH: f32 = 0.1;
const READY_BRIGHT: f32 = 3.0;
/// How fast the turned player wanders with the dead.
const TURNED_SPEED: f32 = 16.0;
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
        let all = &roster().enemies;
        #[cfg(test)]
        if self.0 as usize >= all.len() {
            return tests::added_kind(self.0 as usize - all.len());
        }
        &all[self.0 as usize]
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

/// The kind of data file `file` is: one of `TABLES`, or `farm.txt` or `ground.txt` for a pack farm's files.
pub fn table_kind(file: &str) -> Option<&'static str> {
    match map::farm_of_file(file) {
        Some((_, ground)) => Some(if ground { "ground.txt" } else { "farm.txt" }),
        None => TABLES.into_iter().find(|t| *t == file),
    }
}

/// Checks a data file's text as the game will read it, a pack farm's by its kind; errors name the line.
pub fn check_table(file: &str, text: &str) -> Result<(), String> {
    let fail = |e: String| format!("{file}:{e}");
    match table_kind(file).unwrap_or(file) {
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
            let all = || own.iter().chain(extra.iter().map(|(_, w)| w));
            if let Some(w) = all().find(|w| !(w.art.is_empty() && w.is_shot()) && missing(&w.art)) {
                return Err(format!("{file}: no sprite called {} in art.txt", w.art));
            }
            match all().flat_map(|w| w.sprites.iter().flatten()).find(|n| missing(n)) {
                Some(n) => Err(format!("{file}: no sprite called {n} in art.txt")),
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
                let sprite = |w: &&&str| w.len() > 1 && w.starts_with(|c: char| c.is_ascii_uppercase());
                let names = match (section, &words[..]) {
                    ("scatter", [name, ..]) => std::slice::from_ref(name),
                    (_, [_, rest @ ..]) => rest,
                    _ => &[][..],
                };
                if let Some(name) = names.iter().filter(sprite).find(|n| missing(n)) {
                    return Err(format!("{file}: no sprite called {name} in art.txt"));
                }
            }
            let farm = map::parse_farm(text).map_err(fail)?;
            map::check_farm_codes(&farm).map_err(|e| format!("{file}: {e}"))
        }
        "ground.txt" => {
            let crops = text.lines().filter_map(|l| {
                let l = l.split('#').next()?.trim();
                l.strip_prefix("crop ").or_else(|| l.strip_prefix("crop_3d "))
            });
            match crops.flat_map(str::split_whitespace).find(|n| missing(n)) {
                Some(name) => Err(format!("{file}: no sprite called {name} in art.txt")),
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

#[derive(Clone)]
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
    /// The weapon that set it burning, for the run's damage by weapon.
    burn_by: Option<usize>,
    burn_owner: PlayerId,
    /// A horde's leader, which drops a chest.
    leader: bool,
    /// Dawn's Harvester: faster, heavier, harder biting, and drawn twice the size.
    boss: bool,
    /// Seconds until a ranged zombie can spit again.
    cd: f32,
    /// Seconds above ground since it last surfaced, which a burrower digs back under after.
    up: f32,
    /// Seconds off screen and past `[spawn] straggler` from the player.
    away: f32,
    /// Once the run is over: seconds still to stand if below 0, else seconds until it picks a new way to wander.
    roam: f32,
    /// In the swarm of the turned player who leads it.
    led: Option<PlayerId>,
    /// Rose with a turned player's leader: its death drops nothing and counts no kill.
    barren: bool,
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
            burn_by: None,
            burn_owner: ME,
            leader: false,
            boss: false,
            cd: 0.0,
            up: 0.0,
            away: 0.0,
            roam: 0.0,
            led: None,
            barren: false,
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
    /// Rises into this turned player's swarm.
    led: Option<PlayerId>,
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

impl Flow {
    /// Buzzes player `p`'s pad, the stronger kind winning; nothing for a player past the seats.
    pub(crate) fn buzz(&mut self, p: PlayerId, r: Rumble) {
        if let Some(s) = self.rumble.get_mut(p) {
            *s = (*s).max(Some(r));
        }
    }

    /// Buzzes every player's pad.
    fn buzz_all(&mut self, r: Rumble) {
        for p in 0..crate::SEATS {
            self.buzz(p, r);
        }
    }
}

/// A ranged zombie's spit, flying straight at where the player stood.
struct Spit {
    kind: Kind,
    pos: [f32; 2],
    vel: [f32; 2],
    damage: f32,
    life: f32,
    age: f32,
}

impl Spit {
    /// The spit in its kind's own blood, if it has one.
    fn tint(&self) -> [f32; 3] {
        let spit = effects::effects().spit;
        spit.frames.first().map_or([1.0; 3], |&a| juice::blood_tint(a, self.kind.stats().blood))
    }
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
    /// The weapon that fired it.
    by: Option<usize>,
    owner: PlayerId,
    /// Pixels above a gun's height it was fired from, and the seconds it takes to come down to it.
    lift: [f32; 2],
}

impl Bullet {
    /// Pixels above a gun's height it flies at `age` seconds.
    fn up(&self, age: f32) -> f32 {
        let [up, fall] = self.lift;
        if fall > 0.0 { up * (1.0 - age / fall).clamp(0.0, 1.0) } else { 0.0 }
    }

    /// Where it draws on the map: its place, raised by how far it has yet to come down.
    fn drawn(&self) -> [f32; 2] {
        [self.pos[0], self.pos[1] - self.up(self.age)]
    }
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
    /// The player it flies to once caught.
    to: PlayerId,
    /// The dog going for it or carrying it.
    dog: Option<Fetch>,
}

/// Which dog has a heart: its player's, by card and slot.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Fetch {
    player: PlayerId,
    def: usize,
    slot: usize,
    carried: bool,
}

impl Fetch {
    fn same(self, o: Fetch) -> bool {
        (self.player, self.def, self.slot) == (o.player, o.def, o.slot)
    }
}

impl Drop {
    /// The magnet takes it, toward the player along `n`: a flick to one side first makes it swoop in.
    fn catch(&mut self, n: [f32; 2], side: f32) {
        self.vel = [-n[1] * side * 70.0 - n[0] * 40.0, n[0] * side * 70.0 - n[1] * 40.0];
        self.pulled = Some(0.0);
    }
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
    /// The weapon that threw it; None for a grenade.
    by: Option<usize>,
    owner: PlayerId,
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
        (p, fm::sin(k * PI) * 28.0)
    }
}

/// How much brighter a prop draws while it flashes from a hit.
const PROP_FLASH: f32 = 3.0;

struct Barrel {
    /// Top-left in world pixels; the barrel fills one cell.
    pos: [f32; 2],
    hp: f32,
    flash: f32,
    /// The player whose shot or blast set it off.
    by: PlayerId,
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

/// Whether player `p` shows its flash frame this instant, blinking while invulnerable.
fn blinking(h: &Harvest, p: PlayerId) -> bool {
    let pl = &h.players[p];
    pl.hurt > 0.0 && ((pl.hurt * 20.0) as u32).is_multiple_of(2)
}

/// Which walk frame player `p` shows: standing, or one of two steps.
fn walk_frame(h: &Harvest, p: PlayerId) -> usize {
    let pl = &h.players[p];
    if pl.walk > 0.0 { 1 + (pl.walk * crate::tuning::tuning().walk_fps) as usize % 2 } else { 0 }
}

/// Player `p`'s frame for a facing: flashing while invulnerable, red just after a hit, else walking.
fn art_for(h: &Harvest, p: PlayerId, look: &Look, facing: Facing) -> Art {
    let i = facing.index();
    if blinking(h, p) {
        look.flash[i]
    } else if h.players[p].hurt > h.difficulty.hurt_time() - 0.2 {
        look.hurt[i]
    } else {
        look.walk[i][walk_frame(h, p)]
    }
}

/// Where player `p` wears a ready grenade on its frame for `facing`, in that frame's pixels, and how bright it
/// draws; None with none ready, while they blink, once they or the run are over, or where their look hides it.
fn player_worn(h: &Harvest, p: PlayerId, facing: Facing) -> Option<([u8; 2], f32)> {
    let pl = &h.players[p];
    if pl.grenades < 1.0 || blinking(h, p) || !pl.alive() || matches!(h.phase, Phase::Over { .. }) {
        return None;
    }
    let walk = &players::look(&pl.look).look.walk[facing.index()];
    // A hurt frame wears it where the standing frame does.
    let frame = if pl.hurt > h.difficulty.hurt_time() - 0.2 { 0 } else { walk_frame(h, p) };
    let at = walk[frame.min(walk.len() - 1)].worn_at()?;
    Some((at, if pl.ready_flash > 0.0 { READY_BRIGHT } else { 1.0 }))
}

/// Where the worn grenade's top-left goes on frame `a`, in game pixels, for its middle on the frame's texture pixel
/// `at`, mirrored with the frame.
fn worn_corner(at: [u8; 2], a: Art, flip: bool) -> [f32; 2] {
    let (g, k) = (art::GRENADE_WORN, a.detail() as f32);
    let [x, y] = at.map(|v| (v as f32 + 0.5) / k);
    let x = if flip { a.w() - x } else { x };
    [x - 0.5 - (g.w() / 2.0).floor(), y - 0.5 - (g.h() / 2.0).floor()]
}

/// Player `p`'s frame for a facing, and how far out of the ground it stands, 0 to 1. Once fallen, it sinks
/// and climbs back out as a zombie.
fn player_pose(h: &Harvest, p: PlayerId, facing: Facing) -> (Art, f32) {
    let look = &players::look(&h.players[p].look).look;
    let fallen = match h.phase {
        Phase::Over { t, won: false } if p == ME => Some(t),
        _ => h.players[p].down,
    };
    let Some(t) = fallen else { return (art_for(h, p, look, facing), 1.0) };
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
    tint: [f32; 3],
    fade: effects::Fade,
}

impl Fx {
    fn play(e: effects::Effect, pos: [f32; 2], angle: f32) -> Self {
        Fx { frames: e.frames, fps: e.fps, pos, angle, t: 0.0, tint: [1.0; 3], fade: e.fade }
    }

    fn done(&self) -> bool {
        self.t * self.fps >= self.frames.len() as f32
    }

    fn frame(&self) -> Art {
        self.frames[((self.t * self.fps) as usize).min(self.frames.len() - 1)]
    }

    /// How opaque its effect's fade draws it now, over its sprite's own alpha.
    fn alpha(&self) -> f32 {
        self.fade.at(self.t * self.fps / self.frames.len() as f32)
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
    /// Each seated player's pad's buzz, by place in `players`.
    pub rumble: [Option<Rumble>; crate::SEATS],
    pub quit: bool,
    /// The pause menu chose Settings; the run stays paused under it.
    pub settings: bool,
    /// How the run went, on the frame it ends.
    pub ended: Option<Run>,
    /// Powerups found this frame, to keep even if the run is abandoned.
    pub found: u64,
    /// Photo mode asked for a capture.
    pub capture: bool,
}

pub struct Harvest {
    rng: Rng,
    map: Map,
    /// The hand-made map this run is on, by id; empty for a generated farm.
    map_id: String,
    t: f32,
    phase: Phase,
    players: Vec<Player>,
    /// The player whose weapons, throws and blasts are playing out; ME outside them.
    actor: PlayerId,
    /// What moves each player but the local one, by place in `players`.
    inputs: Vec<Input>,
    /// Scripted players and the place in `players` each drives.
    bots: Vec<(PlayerId, sandbox::Bot)>,
    /// Which living player the next zombie rises around.
    ring_turn: usize,
    zombies: Vec<Zombie>,
    /// Burrowers tunneling toward the player, out of reach of anything that hits.
    burrowed: Vec<Zombie>,
    spits: Vec<Spit>,
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
    bolts: Vec<Bolt>,
    decoys: Vec<Decoy>,
    next_id: u32,
    /// Money's worth of XP not yet a whole point, from a pawn shop's share.
    xp_frac: f32,
    /// What earlier runs unlocked, what this one found, and how many more it made findable.
    progress: Progress,
    /// Zombie kinds the settings turned off, by place in the roster.
    enemies_off: u64,
    newly: Vec<usize>,
    ready: usize,
    found_now: u64,
    /// Whether a level breaker took a weapon past its top this run.
    broke: bool,
    /// The next level-up comes from a chest, and the chests picked up whose level-up hasn't begun.
    chest: bool,
    chests: u32,
    /// How many times as likely a card for a held powerup is as one for a new powerup.
    held_weight: f32,
    /// Powerups banished from this run's cards, by place in the book.
    banished: u64,
    /// Banishes and rerolls the run has left.
    banishes: u32,
    rerolls: u32,
    /// Banish is armed from its on-screen key: the next card tapped is banished, not taken.
    banishing: bool,
    /// The powerup the player favored, by place in the book, and the favors the run has left.
    favored: Option<usize>,
    favors: u32,
    /// Favor is armed from its on-screen key: the next card tapped is taken and favored.
    favoring: bool,
    /// The level-up's `t` when each card on show was dealt, so a card dealt again slides in again.
    dealt: [f32; 3],
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
    /// Once the first Harvester falls: seconds to the next wave of them, and how many that wave brings.
    waves: Option<(f32, u32)>,
    /// The night's time the truck driver's next collection ticket is ready.
    ticket: f32,
    thrown: Vec<Grenade>,
    owed: f32,
    /// Zombies owed while the night was full, and the release toward the next of them rising.
    banked: f32,
    let_out: f32,
    xp: u32,
    next_xp: u32,
    level: u32,
    kills: u32,
    /// Steps to the nearest living player from each cell, rebuilt when one of them changes cell.
    flow: Vec<u16>,
    flow_cells: Vec<[usize; 2]>,
    sep: Vec<[f32; 2]>,
    steps: map::Steps,
    /// The pause menu's highlighted choice, while Start has the run paused.
    menu: Option<usize>,
    /// X is held: aimed weapons go for explosive barrels in view first.
    targeting: bool,
    /// False until the turn buttons are let go, so the A that started the run or picked an upgrade doesn't turn.
    turn_ready: bool,
    /// No zombies: a walk around the farm, for looking at it.
    calm: bool,
    /// The player stands straight back up at full health instead of dying, for the sandbox's whole nights.
    undying: bool,
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
    /// Cooldowns on sounds that would otherwise stack up in a swarm: hits, kills, groans, rising, birds, spit, drones.
    hush: [f32; 8],
    /// Follow-up sounds and the seconds until each plays.
    later: Vec<(f32, Sfx)>,
    heartbeat: f32,
    /// Seconds of the full-screen gold flash when the cards appear.
    glory: f32,
    /// A tap that landed during hit-stop, thrown when the world moves again.
    queued_touch: Option<[f32; 2]>,
    /// Sideways input, eased, which tilts the camera into a strafe.
    lean: f32,
    /// How many times the 3D view is magnified, for the icon scene; the game keeps 1.
    zoom: f32,
    /// Tiles the 3D camera is raised, or lowered when negative, for the icon scene; the game keeps 0.
    lift: f32,
    /// The range's camera swung round the player by this many radians and backed off this many tiles.
    orbit: Option<[f32; 2]>,
    /// A sprite the editor is showing off.
    shown: Option<showcase::Shown>,
    difficulty: Difficulty,
    /// One screen: the map shows on the top screen only while looked at.
    single: bool,
    /// On one screen, the corner map shows while playing; Settings turns it off.
    corner_map: bool,
    aiming: Option<single::Aiming>,
    /// Seconds R1 has held a quick throw lined up.
    quick: Option<f32>,
    /// The map is up on the one screen without a grenade being aimed.
    glance: bool,
    /// A thrown grenade is never spent.
    endless: bool,
    /// The last single-screen throw's angle off the view and distance from the player, and real seconds since.
    last_throw: Option<[f32; 2]>,
    since_throw: f32,
    /// Touch grenades, as on a phone: L2 flips the map up or back, and a tap on it throws, instead of L2 aiming.
    touch_aim: bool,
    /// Touch grenades have the map up.
    touch_map: bool,
    photo: Option<photo::Photo>,
    saves_photos: bool,
    taps: bool,
    /// The on-screen photo keys show, so the guides name them instead of a pad's buttons.
    touch_keys: bool,
    /// The weapon range, trying one weapon against a ring of zombies.
    range: Option<range::Range>,
    before: blend::Before,
    /// Players with a view of their own, from ME on; 1 outside local co-op.
    views: usize,
    /// Each viewed player's own controls, cards and camera while another's are in the fields above.
    locals: Vec<local::Local>,
    /// The player whose Start opened the pause menu, who alone works it.
    paused_by: PlayerId,
    /// The player ME stands for while `relabel` has traded them; ME otherwise.
    swapped: PlayerId,
    net: online::Net,
    /// The size the view being drawn is laid out to, as `Canvas::refit` takes it; a screen's outside split-screen.
    canvas: [f32; 2],
    setup: Option<Box<replay::Setup>>,
    tape: Option<Box<replay::Tape>>,
    /// Inside `update`, where a scripted player's throw is the tick's own and not recorded apart.
    updating: bool,
    picking: ([Choice; 3], f32),
}

/// Weapons and passives unlocked so far, out of all of them, and how many are waiting to be found.
pub fn unlocked(p: &Progress) -> (usize, usize, usize) {
    let all: Vec<usize> = (0..book().defs.len()).filter(|&i| book().defs[i].kind != Card::Filler && p.hidden & (1 << i) == 0).collect();
    (all.iter().filter(|&&i| p.owns(i)).count(), all.len(), all.iter().filter(|&&i| p.findable(i)).count())
}

/// The powerup closest to becoming findable, and what it takes.
pub fn next_unlock(p: &Progress) -> Option<(String, String)> {
    book()
        .defs
        .iter()
        .enumerate()
        .filter(|&(i, d)| p.hidden & (1 << i) == 0 && !p.owns(i) && !d.unlock.met(p))
        .map(|(_, d)| d)
        .min_by(|a, b| a.unlock.effort(p).total_cmp(&b.unlock.effort(p)))
        .map(|d| (d.name.clone(), d.unlock.text()))
}

/// Whether a shot from one spot on the ground to another crosses nothing that stops bullets.
fn clear_shot(map: &Map, from: [f32; 2], to: [f32; 2]) -> bool {
    let d = sub(to, from);
    let steps = (len(d) / 4.0).ceil() as usize;
    (1..steps).all(|k| {
        let f = k as f32 / steps as f32;
        !map.cover_at(from[0] + d[0] * f, from[1] + d[1] * f)
    })
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
        if self.menu.is_none() && self.photo.is_none() && matches!(self.phase, Phase::Playing) { Buttons::START } else { Buttons::B }
    }

    /// The player's position and the solid cells and props around them, for the debug pipe's `state`.
    pub fn debug_state(&self) -> String {
        let half = crate::tuning::tuning().player_half();
        let [px, py] = self.me().pos;
        let (tx, ty) = ((px / TILE) as i32, (py / TILE) as i32);
        let mut s = format!("pos {px:.1},{py:.1} tile {tx},{ty} half {half:?} yaw {:.2}\n", self.me().view_yaw);
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
        let mut juice = Juice::new(seed);
        juice.tally = tally::Tally::sized(book().defs.len(), 1);
        let own = crate::tuning::tuning().barrel_hp;
        let barrels = map.barrels.iter().enumerate().map(|(i, &pos)| Barrel { pos, hp: map.barrel_hp.get(i).copied().filter(|h| *h > 0.0).unwrap_or(own), flash: 0.0, by: ME }).collect();
        Self {
            barrels,
            barrel_spots: map.barrels.clone(),
            drum_t: 0.0,
            patches: Vec::new(),
            bolts: Vec::new(),
            decoys: Vec::new(),
            next_id: 1,
            xp_frac: 0.0,
            progress: Progress::default(),
            enemies_off: 0,
            newly: Vec::new(),
            ready: 0,
            found_now: 0,
            broke: false,
            chest: false,
            chests: 0,
            held_weight: crate::tuning::tuning().held_weight,
            banished: 0,
            banishes: crate::tuning::tuning().banish as u32,
            rerolls: crate::tuning::tuning().reroll as u32,
            banishing: false,
            favored: None,
            favors: crate::tuning::tuning().favor as u32,
            favoring: false,
            dealt: [0.0; 3],
            leader_owed: false,
            fury: 1.0,
            fury_tick: 0.0,
            trial: Vec::new(),
            start_t: 0.0,
            woke: false,
            waves: None,
            ticket: crate::tuning::tuning().driver_after,
            thrown: Vec::new(),
            players: vec![Player::new(map.spawn, kit, book().grenades[Difficulty::Normal as usize] as f32)],
            actor: ME,
            inputs: Vec::new(),
            bots: Vec::new(),
            ring_turn: 0,
            rng,
            map,
            map_id: String::new(),
            t: 0.0,
            phase: Phase::Playing,
            zombies: Vec::new(),
            burrowed: Vec::new(),
            spits: Vec::new(),
            bullets: Vec::new(),
            drops: Vec::new(),
            fx: Vec::new(),
            stains: Vec::new(),
            owed: 0.0,
            banked: 0.0,
            let_out: 0.0,
            xp: 0,
            next_xp: book().first_level,
            level: 1,
            kills: 0,
            flow: Vec::new(),
            flow_cells: Vec::new(),
            sep: Vec::new(),
            steps: map::Steps::default(),
            menu: None,
            targeting: false,
            turn_ready: false,
            calm: false,
            undying: false,
            bench: false,
            juice,
            tells: Vec::new(),
            birds: Vec::new(),
            noise: None,
            xp_shown: 0.0,
            hp_shown: 0.0,
            chain: (1.0, 0),
            leveling: 0.0,
            kill_bump: 1.0,
            hush: [0.0; 8],
            later: Vec::new(),
            heartbeat: 0.0,
            glory: 0.0,
            queued_touch: None,
            lean: 0.0,
            zoom: 1.0,
            lift: 0.0,
            orbit: None,
            shown: None,
            difficulty: Difficulty::Normal,
            single: false,
            corner_map: true,
            aiming: None,
            quick: None,
            glance: false,
            endless: false,
            last_throw: None,
            since_throw: 0.0,
            touch_aim: false,
            touch_map: false,
            photo: None,
            saves_photos: false,
            taps: true,
            touch_keys: false,
            range: None,
            before: blend::Before::default(),
            views: 1,
            locals: Vec::new(),
            paused_by: ME,
            swapped: ME,
            net: online::Net::default(),
            canvas: [SCREEN_W, SCREEN_H],
            setup: None,
            tape: None,
            updating: false,
            picking: ([Choice { up: 0, rare: false, found: false }; 3], 0.0),
        }
        .with_birds()
    }

    fn with_birds(mut self) -> Self {
        self.hp_shown = self.me().hp;
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
            if !self.map.hits(p[0], p[1], [2.0, 2.0]) && self.players.iter().all(|q| len(sub(p, q.pos)) > away) {
                return Some(p);
            }
        }
        None
    }

    /// The same frames every run: 300 zombies, bodies, stains, specks and half-faded blasts around the player, frozen, with the
    /// camera turning a full circle every 600 frames, on a generated farm `size` tiles across.
    pub fn bench(size: [usize; 2]) -> Self {
        let [w, hgt] = size.map(|v| v.clamp(16, 400));
        let mut h = Self { calm: true, bench: true, ..Self::sized(7, [w, hgt]) };
        h.birds.clear();
        for i in 0..(crate::tuning::tuning().max_zombies as usize) {
            let a = i as f32 * 2.399;
            let r = 14.0 + (i as f32).sqrt() * 9.0;
            let pos = [h.me().pos[0] + fm::cos(a) * r, h.me().pos[1] + fm::sin(a) * r];
            let kinds = roster().enemies.len();
            let mut z = Zombie::new(Kind((kinds - 1 - i % kinds) as u8), pos, 10.0, 1.0, i as f32 * 0.1);
            z.dir = norm(sub(h.me().pos, pos));
            h.zombies.push(z);
        }
        for i in 0..40 {
            let a = i as f32 * 0.9;
            let at = [h.me().pos[0] + fm::cos(a) * 30.0, h.me().pos[1] + fm::sin(a) * 30.0];
            let stain = art::BLOOD_STAIN[i % art::BLOOD_STAIN.len()];
            h.stains.push((stain, at[0], at[1], [1.0; 3]));
            h.juice.corpse(Corpse { art: art::SKINNY_HURT[0], dead: None, flip: false, pos: at, z: 0.0, vel: [0.0; 2], vz: 0.0, fall: [fm::cos(a), fm::sin(a)], age: 1.0, spin: 0.0 });
            h.juice.burst(at, 8.0, 8, [0.0; 2], 0.0, [10.0, 40.0], juice::blood(), 1.5, 100.0, 0.0);
            h.juice.popup(at, 12.0 + i as f32, if i % 5 == 0 { Tone::Crit } else { Tone::Hit });
            let vel = [fm::cos(a) * 200.0, fm::sin(a) * 200.0];
            h.bullets.push(Bullet { pos: [h.me().pos[0] + fm::cos(a) * 20.0, h.me().pos[1] - 6.0 + fm::sin(a) * 20.0], vel, damage: 0.0, pierce: 0, life: 1.0, force: 0.0, hit: Vec::new(), look: None, age: 0.0, by: None, owner: ME, lift: [0.0; 2] });
        }
        let e = effects::effects();
        for (i, fx) in [e.explosion, e.explosion_smoke, e.zombie_rises, e.mound_breaks].into_iter().cycle().take(16).enumerate() {
            let a = i as f32 * 0.4;
            let mut f = Fx::play(fx, [h.me().pos[0] + fm::cos(a) * 40.0, h.me().pos[1] + fm::sin(a) * 40.0], 0.0);
            f.t = 0.6 * fx.frames.len() as f32 / fx.fps;
            h.fx.push(f);
        }
        h.juice.flash(h.me().pos, 10.0, [1.2, 0.85, 0.4], 3.5, f32::MAX);
        h
    }

    /// The bench's horde and effects standing in a field of the corn ground's crop, `ground.txt`'s `c`, planted past
    /// the view's reach on every side, with the player in a lane between its rows.
    pub fn bench_corn(size: [usize; 2]) -> Self {
        let mut h = Self::bench(size);
        let tile = [h.me().pos[0], h.me().pos[1]].map(|v| (v / TILE) as usize);
        h.map.plant(b'c', tile, view3d::VIEW_RADIUS as usize + 4);
        h
    }

    /// The map's size and prop count and how long a flow field over it takes, which a moving player pays each time
    /// they change tile; `time` runs the work it's given and returns the seconds it took.
    pub fn bench_report(&self, time: impl FnOnce(&mut dyn FnMut()) -> f64) -> String {
        let (mut dist, mut steps) = (Vec::new(), map::Steps::default());
        let secs = time(&mut || {
            for _ in 0..20 {
                self.map.flow_into(self.me().pos, &mut dist, &mut steps);
                std::hint::black_box(&dist);
            }
        });
        let (w, hgt) = (self.map.width_px() / TILE, self.map.height_px() / TILE);
        let crops = match view3d::crops_in_view(self) {
            (0, _) => String::new(),
            (cells, cards) => format!(", {cells} crop cells and {cards} crop cards in view"),
        };
        format!("bench: {w}x{hgt} tiles, {} props, flow field {:.2} ms{crops}", self.map.props.len(), secs * 1000.0 / 20.0)
    }

    pub fn at(mut self, difficulty: Difficulty) -> Self {
        let kit = Kit::start(difficulty);
        let me = self.me_mut();
        (me.hp, me.grenades, me.kit) = (kit.max_hp(), kit.grenades(), kit);
        Self { difficulty, ..self }
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
    pub fn looking_like(mut self, id: &str) -> Self {
        let mine = players::look(id);
        let own = mine.start_defs(self.difficulty).map(|defs| Kit::with_weapons(&defs, self.difficulty));
        let me = self.me_mut();
        if let Some(kit) = own {
            me.grenades = kit.grenades();
            me.kit = kit;
        }
        for (b, theirs) in me.kit.base.iter_mut().zip(mine.base) {
            *b = theirs.unwrap_or(*b);
        }
        me.hp = me.kit.max_hp();
        me.look = mine.id.clone();
        self
    }

    /// The level-up cards are up.
    pub(crate) fn choosing(&self) -> bool {
        matches!(self.phase, Phase::LevelUp { .. })
    }

    #[cfg(test)]
    pub(crate) fn deal_cards_with_rerolls(&mut self, rerolls: u32) {
        self.rerolls = rerolls;
        let choices = self.roll_choices(&[]);
        self.phase = Phase::LevelUp { choices, selected: 0, t: 1.0, picked: None };
    }

    #[cfg(test)]
    pub(crate) fn rerolls_left(&self) -> u32 {
        self.rerolls
    }

    #[cfg(test)]
    pub fn enemies_off(&self) -> u64 {
        self.enemies_off
    }

    #[cfg(test)]
    pub(crate) fn night_t(&self) -> f32 {
        self.t
    }

    #[cfg(test)]
    pub fn grenades_in_the_air(&self) -> usize {
        self.thrown.iter().filter(|g| g.kind == Thrown::Grenade).count()
    }

    #[cfg(test)]
    pub(crate) fn last_thrown_distance(&self) -> Option<f32> {
        let g = self.thrown.iter().rev().find(|g| g.kind == Thrown::Grenade)?;
        Some(len(sub(g.to, g.from)))
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
            self.players[ME].kit.raise(def, level);
        }
        self.me_mut().hp = self.me().kit.max_hp();
        self.me_mut().grenades = self.me().kit.grenades();
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
        if let Some(line) = self.range_summary() {
            return line;
        }
        let crops = match view3d::crops_in_view(self) {
            (cells, cards) if self.bench && cells > 0 => format!(" crops={cells} crop_cards={cards}"),
            _ => String::new(),
        };
        format!("harvest {} t={:.0} level={} kills={} zombies={} hp={:.0}{crops}", self.difficulty.key(), self.t, self.level, self.kills, self.zombies.len(), self.me().hp)
    }

    pub fn update(&mut self, input: &Input, dt: f32, sounds: &mut Vec<Sfx>) -> Flow {
        self.tape_before(input, dt);
        self.updating = true;
        let flow = self.update_tick(input, dt, sounds);
        self.updating = false;
        self.tape_after(flow.ended.is_some());
        flow
    }

    fn update_tick(&mut self, input: &Input, dt: f32, sounds: &mut Vec<Sfx>) -> Flow {
        self.take_before();
        if self.range.is_none() {
            return self.update_run(input, dt, sounds);
        }
        let own = self.range_input(input, sounds);
        let flow = self.update_run(own.as_ref().unwrap_or(input), dt, sounds);
        self.range_after(dt);
        flow
    }

    fn update_run(&mut self, input: &Input, dt: f32, sounds: &mut Vec<Sfx>) -> Flow {
        let mut flow = Flow::default();
        if self.bench {
            self.me_mut().view_yaw = (self.me().view_yaw + TAU / 600.0).rem_euclid(TAU);
            self.remember(dt);
            return flow;
        }
        if self.photo.is_some() && self.views > 1 {
            self.photo_coop(input, dt, sounds, &mut flow);
            return flow;
        }
        if self.photo.is_some() {
            self.photo(input, dt, sounds, &mut flow);
            return flow;
        }
        if self.views > 1 {
            return self.update_coop(input, dt, sounds);
        }
        let own;
        let (input, dt) = if self.single {
            own = self.single_input(input, dt, sounds);
            (&own.0, own.1)
        } else {
            (input, dt)
        };
        self.quick_off();
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
                    self.turn_around(ME);
                }
            }
            Phase::Playing => {
                self.drive_bots(sounds);
                self.play(input, dt, sounds, &mut flow);
            }
            Phase::LevelUp { .. } => self.choose(input, dt, sounds),
            Phase::Over { t: before, won } => {
                let t = before + dt;
                self.phase = Phase::Over { t, won };
                self.juice.tick(if t < TURN_SINK { dt * 0.4 } else { dt });
                self.wind_down(dt, sounds, &mut flow);
                for p in 0..self.players.len() {
                    if self.players[p].down.is_some() {
                        self.turning(p, dt, sounds);
                    }
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
        let Phase::LevelUp { mut choices, mut selected, t, picked } = self.phase else { return };
        let t = t + dt;
        if let Some(p) = picked {
            let p = p + dt;
            if p >= PICK_TIME && self.views > 1 {
                self.phase = Phase::LevelUp { choices, selected, t, picked: Some(PICK_TIME) };
            } else if p >= PICK_TIME {
                self.phase = Phase::Playing;
                self.turn_ready = false;
            } else {
                self.phase = Phase::LevelUp { choices, selected, t, picked: Some(p) };
            }
            return;
        }
        let mut picked = None;
        if t > CARDS_IN + self.dealt.iter().copied().fold(0.0, f32::max) {
            let n = choices.len();
            if input.pressed.any(Buttons::DOWN | Buttons::RIGHT) {
                selected = (selected + 1) % n;
                sounds.push(Sfx::Tick(selected as u8 * 3));
            }
            if input.pressed.any(Buttons::UP | Buttons::LEFT) {
                selected = (selected + n - 1) % n;
                sounds.push(Sfx::Tick(selected as u8 * 3));
            }
            let tap = input.touch.filter(|_| input.touch_started);
            let key = tap.and_then(|p| level_key(p, self.level));
            let tapped = tap.and_then(|[_, y]| choice_row(y)).filter(|&i| i < 3);
            // X picks, not A: A turns, and a player still holding it as the cards appear shouldn't pick by accident.
            if input.pressed.contains(Buttons::R1) || key == Some(LevelKey::Reroll) {
                self.reroll(&mut choices, t, sounds);
            } else if key == Some(LevelKey::Banish) {
                let armed = !self.banishing && self.banishes > 0 && choices.iter().any(|c| book().defs[c.up].kind != Card::Filler);
                sounds.push(if armed || self.banishing { Sfx::Tick(6) } else { Sfx::Miss });
                (self.banishing, self.favoring) = (armed, false);
            } else if key == Some(LevelKey::Favor) {
                let armed = !self.favoring && self.can_favor() && choices.iter().any(|c| book().defs[c.up].kind != Card::Filler);
                sounds.push(if armed || self.favoring { Sfx::Tick(9) } else { Sfx::Miss });
                (self.favoring, self.banishing) = (armed, false);
            } else if let Some(i) = tapped.filter(|_| self.banishing).or(input.pressed.contains(Buttons::L1).then_some(selected)) {
                self.banish(&mut choices, i, t, sounds);
                selected = i;
            } else if let Some(i) = tapped.filter(|_| self.favoring).or(input.pressed.contains(Buttons::Y).then_some(selected)) {
                self.favoring = false;
                if self.favor(choices[i].up) {
                    self.take(choices[i], sounds);
                    (selected, picked) = (i, Some(0.0));
                } else {
                    sounds.push(Sfx::Miss);
                }
            } else if let Some(i) = tapped.or(input.pressed.contains(Buttons::X).then_some(selected)) {
                self.take(choices[i], sounds);
                (selected, picked) = (i, Some(0.0));
            }
        }
        self.phase = Phase::LevelUp { choices, selected, t, picked };
    }

    /// Takes a card as the run's pick.
    fn take(&mut self, c: Choice, sounds: &mut Vec<Sfx>) {
        self.pick(c);
        self.chest = false;
        (self.banishing, self.favoring) = (false, false);
        self.xp_shown = 0.0;
        sounds.push(Sfx::Pick(c.rare));
    }

    /// Whether the run can favor a card now: it has a favor left and has reached `favor_level`.
    fn can_favor(&self) -> bool {
        self.favors > 0 && self.level as f32 >= crate::tuning::tuning().favor_level
    }

    /// Y, or Favor then a card: `up` becomes `favor_weight` times as likely for the rest of the run, and the card is
    /// taken. A filler can't be favored. False when it isn't.
    fn favor(&mut self, up: usize) -> bool {
        if !self.can_favor() || book().defs[up].kind == Card::Filler {
            return false;
        }
        self.favors -= 1;
        self.favored = Some(up);
        true
    }

    /// L1, or Banish then a card: that card's powerup leaves this run's cards, and its place is dealt again. A
    /// filler can't be banished, as fillers only come up once nothing else can.
    fn banish(&mut self, choices: &mut [Choice; 3], i: usize, t: f32, sounds: &mut Vec<Sfx>) {
        (self.banishing, self.favoring) = (false, false);
        let up = choices[i].up;
        if self.banishes == 0 || book().defs[up].kind == Card::Filler {
            sounds.push(Sfx::Miss);
            return;
        }
        self.banishes -= 1;
        self.banished |= 1 << up;
        let others: Vec<Choice> = (0..3).filter(|&j| j != i).map(|j| choices[j]).collect();
        choices[i] = self.deal(&others, &[]);
        self.dealt[i] = t;
        sounds.push(Sfx::Slash);
    }

    /// R1, or Reroll: three new cards, kept apart from the three on show while the pool allows, with the same chance
    /// of a find, or a chest's find.
    fn reroll(&mut self, choices: &mut [Choice; 3], t: f32, sounds: &mut Vec<Sfx>) {
        (self.banishing, self.favoring) = (false, false);
        if self.rerolls == 0 {
            sounds.push(Sfx::Miss);
            return;
        }
        self.rerolls -= 1;
        let shown = *choices;
        *choices = self.roll_choices(&shown);
        self.dealt = [t; 3];
        sounds.push(Sfx::Throw);
    }

    fn play(&mut self, input: &Input, dt: f32, sounds: &mut Vec<Sfx>, flow: &mut Flow) {
        let dt = if self.leveling > 0.0 {
            self.leveling -= dt;
            if self.leveling <= 0.0 {
                self.dealt = [0.0; 3];
                (self.banishing, self.favoring) = (false, false);
                self.phase = Phase::LevelUp { choices: self.roll_choices(&[]), selected: 0, t: 0.0, picked: None };
                self.glory = 0.25;
                if self.views > 1 {
                    self.deal_others();
                }
                return;
            }
            dt * 0.15
        } else {
            dt
        };
        self.t += dt;
        self.juice.tally.tick(dt);
        for (p, player) in self.players.iter().enumerate() {
            self.juice.tally.hold(p, player.kit.weapons.iter().map(|a| a.def), dt);
        }
        self.targeting = input.held.contains(Buttons::X);
        let side = input.dir()[0];
        self.lean += (side - self.lean) * (8.0 * dt).min(1.0);
        for p in 0..self.players.len() {
            if self.players[p].alive() {
                let own = if p == ME { *input } else { self.inputs.get(p).copied().unwrap_or_default() };
                self.walk(p, &own, dt);
            }
        }
        if self.players.len() > 1 {
            self.steer_leads(input, dt, sounds);
        }
        self.noise = None;
        for p in 0..self.players.len() {
            if self.players[p].alive() {
                self.recharge(p, dt);
            }
        }
        if let Some(touch) = input.touch.filter(|_| input.touch_started).or(self.queued_touch.take()) {
            self.throw(touch, sounds);
        }
        self.quick_throw(input, dt, sounds);
        for v in 1..self.views {
            if self.players[v].alive() {
                self.relabel(v);
                let own = self.inputs[ME];
                self.targeting = own.held.contains(Buttons::X);
                self.lean += (own.dir()[0] - self.lean) * (8.0 * dt).min(1.0);
                self.quick_throw(&own, dt, sounds);
                self.relabel(v);
            }
        }
        self.fly(dt, sounds, flow);

        if !self.calm {
            self.spawn(dt);
        } else if input.pressed.contains(Buttons::X) {
            // The quiet farm takes one zombie at a time, for trying out weapons and effects.
            let f = self.me().aim;
            let pos = [self.me().pos[0] + f[0] * 50.0, self.me().pos[1] + f[1] * 50.0];
            if !self.map.hits_cells(pos[0], pos[1], crate::tuning::tuning().zombie_half()) {
                self.tells.push(Tell { kind: Kind::common(), pos, hp: Kind::common().stats().hp, t: 0.0, leader: false, led: None });
            }
        }
        if !self.calm {
            self.camp(dt);
        }
        self.rise(dt, sounds);
        self.move_zombies(dt, sounds);
        self.follow_leads();
        self.tunnel(dt, sounds);
        self.grow_fury(dt);
        self.harvester_waves(dt, sounds, flow);
        self.contact(sounds, flow);
        self.move_spits(dt, sounds, flow);
        self.fire(dt, sounds);
        self.move_bullets(dt, sounds);
        self.hazards(dt);
        self.decoys(dt, sounds);
        self.restock(dt);
        if let Some(by) = self.blow(dt, sounds) {
            flow.buzz(by, Rumble::Blast);
        }
        self.crumble(dt, sounds);
        self.reap(sounds, flow);
        self.cash_in(sounds);
        self.collect(dt, sounds);
        self.flock(dt, sounds);
        self.ambience(dt, sounds, flow);
        for f in &mut self.fx {
            f.t += dt;
        }
        self.fx.retain(|f| !f.done());
        self.juice.tick(dt);
        self.feel(dt);
        for v in 1..self.views {
            self.relabel(v);
            self.feel_hp(dt);
            self.relabel(v);
        }
        self.remember(dt);

        let (mut stood, mut fell) = (false, None);
        for p in 0..self.players.len() {
            let pl = &self.players[p];
            if !pl.alive() || pl.hp > 0.0 {
                continue;
            }
            stood = true;
            if (pl.revived as f32) < pl.kit.mods.revive {
                self.revive(p, sounds, flow);
            } else if self.undying {
                self.juice.tally.book_mut(p).fell.get_or_insert(self.t);
                self.players[p].hp = self.players[p].kit.max_hp();
            } else {
                self.juice.tally.book_mut(p).fell.get_or_insert(self.t);
                self.players[p].hp = 0.0;
                self.players[p].down = Some(0.0);
                fell = Some(p);
            }
        }
        for p in 0..self.players.len() {
            if self.players[p].down.is_none() || fell == Some(p) {
                continue;
            }
            self.actor = p;
            self.scatter(dt);
            self.actor = ME;
            self.turning(p, dt, sounds);
        }
        if let Some(p) = fell.filter(|_| self.players.iter().all(|pl| !pl.alive())) {
            sounds.push(Sfx::GameOver);
            self.shake_all(0.8);
            self.juice.burst(self.players[p].pos, 8.0, 30, [0.0; 2], 0.0, [30.0, 120.0], juice::blood(), 2.0, 0.9, 0.0);
            flow.buzz_all(Rumble::Blast);
            self.leveling = 0.0;
            self.spits.clear();
            for mut z in std::mem::take(&mut self.burrowed) {
                z.rise = 1.0;
                self.zombies.push(z);
            }
            self.phase = Phase::Over { t: 0.0, won: false };
            self.lose_interest(p);
            flow.ended = Some(self.finish(false));
        } else if let Some(p) = fell {
            self.falls(p, sounds);
        } else if stood {
        } else if self.t >= dawn() && self.players.iter().any(|pl| pl.alive() && len(sub(pl.pos, self.map.home)) < crate::tuning::tuning().home) {
            sounds.push(Sfx::Dawn);
            // Home: the door shuts and first light burns the dead away.
            let climbing: Vec<Zombie> = self.burrowed.drain(..).filter(|z| z.rise > 0.0).collect();
            for z in std::mem::take(&mut self.zombies).into_iter().chain(climbing) {
                self.juice.burst(z.pos, 8.0, 3, [0.0; 2], 0.0, [10.0, 40.0], juice::ember(), 1.4, 1.2, 1.0);
                self.fx.push(Fx::play(effects::effects().dawn_burns, [z.pos[0], z.pos[1] - 6.0], 0.0));
            }
            self.tells.clear();
            self.burrowed.clear();
            self.spits.clear();
            self.juice.flash(self.me().pos, 40.0, [1.6, 1.1, 0.7], 14.0, 1.5);
            self.shake_all(0.3);
            flow.buzz_all(Rumble::Fanfare);
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

    /// Moves player `p` by its input: up and down walk along the heading, left and right strafe, Y and A turn, B
    /// turns around.
    fn walk(&mut self, p: PlayerId, input: &Input, dt: f32) {
        self.players[p].hurt = (self.players[p].hurt - dt).max(0.0);
        let dir = input.dir();
        let held = match self.turn_ready_mut(p) {
            Some(ready) => {
                *ready |= !input.held.any(Buttons::A | Buttons::Y);
                if *ready { input.held } else { Buttons::NONE }
            }
            None => input.held,
        };
        let turn = (held.contains(Buttons::A) as i32 as f32 - held.contains(Buttons::Y) as i32 as f32 + input.look).clamp(-1.0, 1.0);
        if input.pressed.contains(Buttons::B) {
            self.turn_around(p);
        }
        let t = crate::tuning::tuning();
        let pl = &mut self.players[p];
        let spun = pl.spin.min(PI / SPIN_TIME * dt);
        pl.spin -= spun;
        pl.view_yaw = (pl.view_yaw + turn * t.turn_speed * dt + spun).rem_euclid(TAU);
        let f = [fm::cos(pl.view_yaw), fm::sin(pl.view_yaw)];
        let (side, ahead) = (dir[0], -dir[1]);
        let mv = [f[0] * ahead - f[1] * side, f[1] * ahead + f[0] * side];
        pl.aim = f;
        pl.facing = Facing::of(f);
        if len(mv) > 0.2 {
            let k = pl.kit.speed() * dt * len(mv).min(1.0) / len(mv);
            pl.heading = norm(mv);
            pl.pos = self.map.slide(pl.pos, [mv[0] * k, mv[1] * k], t.player_half());
            let before = (pl.walk * t.walk_fps) as u32;
            pl.walk += dt;
            if (pl.walk * t.walk_fps) as u32 / 2 != before / 2 {
                let at = pl.pos;
                self.juice.burst(at, 0.5, 2, [-mv[0], -mv[1]], 0.8, [8.0, 20.0], juice::dirt(), 1.0, 0.35, 0.0);
            }
        } else {
            pl.walk = 0.0;
        }
    }

    /// Player `p`'s grenades recharge and their health regenerates.
    fn recharge(&mut self, p: PlayerId, dt: f32) {
        let pl = &mut self.players[p];
        let before = pl.grenades;
        pl.grenades = (pl.grenades + dt / (crate::tuning::tuning().grenade_recharge * pl.kit.mods.recharge)).min(pl.kit.grenades());
        pl.ready_flash = if before < 1.0 && pl.grenades >= 1.0 { READY_FLASH } else { (pl.ready_flash - dt).max(0.0) };
        pl.hp = (pl.hp + pl.kit.mods.regen * dt).min(pl.kit.max_hp());
    }

    /// Dawn: the night doesn't end, it turns. The Harvester rises behind the player, and from here on the
    /// dead grow stronger every second until the player reaches the farmhouse.
    fn daybreak(&mut self, sounds: &mut Vec<Sfx>, flow: &mut Flow) {
        self.woke = true;
        sounds.push(Sfx::Horde);
        flow.buzz_all(Rumble::Blast);
        self.shake_all(0.6);
        self.juice.banner("DAWN.  THE HARVESTER WAKES.  GET HOME.", [1.0, 0.45, 0.3, 1.0], 4.0);
        self.raise_harvester(0.0);
    }

    /// Once the first Harvester falls, a wave of them rises every `again` seconds, each one bigger than the last.
    fn harvester_waves(&mut self, dt: f32, sounds: &mut Vec<Sfx>, flow: &mut Flow) {
        let t = crate::tuning::tuning();
        let Some((left, size)) = &mut self.waves else {
            return;
        };
        *left -= dt;
        if *left > 0.0 {
            return;
        }
        *left += t.harvester_again;
        let n = (*size).min(t.harvester_most as u32);
        *size += 1;
        if n == 0 {
            return;
        }
        sounds.push(Sfx::Horde);
        flow.buzz_all(Rumble::Blast);
        self.shake_all(0.6);
        self.juice.banner("MORE HARVESTERS RISE.", [1.0, 0.45, 0.3, 1.0], 2.5);
        for i in 0..n {
            self.raise_harvester((i as f32 - (n - 1) as f32 / 2.0) * 0.45);
        }
    }

    /// Raises one Harvester `rise` from the player, on the side away from home turned by `turn` radians, at the
    /// health dawn's fury has grown it to.
    fn raise_harvester(&mut self, turn: f32) {
        let t = crate::tuning::tuning();
        let p = self.ring_player();
        let from = self.players[p].pos;
        let away = norm(sub(from, self.map.home));
        let away = if away == [0.0; 2] { [0.0, 1.0] } else { away };
        let away = [away[0] * fm::cos(turn) - away[1] * fm::sin(turn), away[0] * fm::sin(turn) + away[1] * fm::cos(turn)];
        let mut at = from;
        for k in 0..24 {
            let a = fm::sin(k as f32 * 0.4) * (k as f32 * 0.15);
            let d = [away[0] * fm::cos(a) - away[1] * fm::sin(a), away[0] * fm::sin(a) + away[1] * fm::cos(a)];
            let p = [from[0] + d[0] * t.harvester_rise, from[1] + d[1] * t.harvester_rise];
            if !self.map.hits_cells(p[0], p[1], t.zombie_half()) {
                at = p;
                break;
            }
        }
        let mut z = Zombie::new(Kind(roster().harvester as u8), at, t.harvester_hp * self.fury, 0.0, 0.0);
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
            for z in self.zombies.iter_mut().chain(&mut self.burrowed) {
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
        let tapped = input.touch.filter(|_| input.touch_started).and_then(|[_, y]| pause_row(y));
        let chosen = tapped.or(input.pressed.any(Buttons::A | Buttons::X).then_some(at));
        self.menu = Some(at);
        let remote = self.net.role == online::Role::Host && self.paused_by >= self.views();
        match chosen {
            _ if input.pressed.any(Buttons::START | Buttons::B) => self.menu = None,
            Some(0) => self.menu = None,
            Some(_) if remote => self.menu = None,
            Some(3) if self.net.role != online::Role::Off => sounds.push(Sfx::Miss),
            Some(1) => flow.settings = true,
            Some(PAUSE_PHOTO) => self.open_photo(),
            Some(3) => self.restart(sounds),
            Some(_) => flow.quit = true,
            None => {}
        }
        self.turn_ready &= self.menu.is_some();
    }

    /// A fresh farm with the same difficulty, progress, enemies, look and trial powerups.
    fn restart(&mut self, sounds: &mut Vec<Sfx>) {
        let seed = self.rng.next_u32() as u64 + 1;
        if self.range.is_some() {
            self.restart_range(seed);
            return;
        }
        let kit: Vec<String> = self.trial.iter().map(|&(d, l)| format!("{}:{l}", book().defs[d].id)).collect();
        let look = std::mem::take(&mut self.me_mut().look);
        let (count, bots, views) = (self.players.len(), std::mem::take(&mut self.bots), self.views);
        self.tape_restart();
        let (tape, progress) = (self.tape.take(), self.progress);
        let setup = self.setup.take().map(|s| replay::Setup { seed, progress, look: look.clone(), kit: kit.join(","), players: count - bots.len(), bots: bots.len(), views, kind: replay::Kind::Night, ..*s });
        *self = Harvest::on(seed, &self.map_id).at(self.difficulty).with_progress(self.progress).without_enemies(self.enemies_off).looking_like(&look).with_kit(&kit.join(",")).starting_at(self.start_t).single_screen(self.single).endless_grenades(self.endless).with_players(count).with_views(views);
        self.bots = bots.into_iter().map(|(p, b)| (p, b.again())).collect();
        (self.tape, self.setup) = (tape, setup.map(Box::new));
        sounds.push(Sfx::Start);
    }

    /// The way the player's sprite faces: the old heading while B swings the camera round, else where they aim.
    #[cfg(test)]
    fn showing(&self) -> [f32; 2] {
        self.showing_of(ME)
    }

    fn showing_of(&self, p: PlayerId) -> [f32; 2] {
        let pl = &self.players[p];
        if pl.down.is_some() { pl.heading } else if pl.spin > 0.0 { pl.turned_from } else { pl.aim }
    }

    /// B: the camera swings round while the player keeps their old heading, so it sweeps past their front.
    fn turn_around(&mut self, p: PlayerId) {
        let pl = &mut self.players[p];
        if pl.spin > 0.0 {
            return;
        }
        pl.turned_from = [fm::cos(pl.view_yaw), fm::sin(pl.view_yaw)];
        pl.spin = PI;
    }

    /// The bar fills: a gold shockwave shoves the nearest zombies back while time slows, then the cards come.
    fn level_up(&mut self, sounds: &mut Vec<Sfx>, flow: &mut Flow) {
        self.xp -= self.next_xp;
        self.next_xp = (self.next_xp as f32 * 1.35).ceil() as u32 + 2;
        self.level += 1;
        self.xp_shown = self.next_xp as f32;
        self.leveling = LEVEL_SLOW;
        sounds.push(Sfx::LevelUp);
        flow.buzz_all(Rumble::Fanfare);
        for p in 0..self.players.len() {
            if self.players[p].alive() {
                self.shove(self.players[p].pos, 320.0);
            }
        }
        self.juice.ring(self.me().pos, crate::tuning::tuning().push, 0.5, juice::gold());
        self.juice.flash(self.me().pos, 20.0, [1.8, 1.4, 0.6], 9.0, 0.8);
        self.shake_each(|_| 0.3, 6.0);
        self.juice.shout("LEVEL UP", GOLD, 0.9);
        for i in 0..40 {
            let a = i as f32 / 40.0 * TAU;
            let r = self.juice.rng.range(3.0, 12.0);
            let rise = self.juice.rng.range(50.0, 130.0);
            let life = self.juice.rng.range(0.8, 1.4);
            let z = self.juice.rng.range(0.0, 6.0);
            self.juice.particle(Particle {
                fade: effects::Fade::OPAQUE,
                pos: [self.me().pos[0] + fm::cos(a) * r, self.me().pos[1] + fm::sin(a) * r],
                z,
                vel: [fm::cos(a) * 18.0, fm::sin(a) * 18.0],
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
                grow: 0.0,
                art: None,
            });
        }
    }

    /// Throws the zombies nearest `at` back from it.
    fn shove(&mut self, at: [f32; 2], force: f32) {
        for z in &mut self.zombies {
            let d = sub(z.pos, at);
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

    /// Three cards from what's unlocked, unbanished and still has room to grow, apart from the cards in `avoid`
    /// while others are left; fillers make up the rest.
    fn roll_choices(&mut self, avoid: &[Choice]) -> [Choice; 3] {
        let mut out = [Choice { up: 0, rare: false, found: false }; 3];
        for i in 0..3 {
            out[i] = self.deal(&out[..i], avoid);
        }
        if let Some(up) = self.find(self.chest) {
            out[self.rng.below(3)] = Choice { up, rare: false, found: true };
        }
        out
    }

    /// One card that isn't among `shown`, nor among `avoid` while others are left. A powerup the player holds
    /// counts `held_weight` times in the draw, and the favored one `favor_weight` times that.
    fn deal(&mut self, shown: &[Choice], avoid: &[Choice]) -> Choice {
        let b = book();
        let free = |i: &usize| !shown.iter().any(|c| c.up == *i);
        let pool: Vec<usize> = (0..b.defs.len()).filter(|&i| self.progress.owns(i) && self.me().kit.offerable(i) && self.banished & 1 << i == 0).filter(free).collect();
        let fresh: Vec<usize> = pool.iter().copied().filter(|i| !avoid.iter().any(|c| c.up == *i)).collect();
        let up = match if fresh.is_empty() { pool } else { fresh } {
            from if !from.is_empty() => {
                let favor = crate::tuning::tuning().favor_weight;
                let weights: Vec<f32> = from.iter().map(|&i| if self.me().kit.level(i) > 0 { self.held_weight } else { 1.0 } * if self.favored == Some(i) { favor } else { 1.0 }).collect();
                let mut r = self.rng.range(0.0, weights.iter().sum());
                let at = weights.iter().position(|w| {
                    r -= w;
                    r < 0.0
                });
                from[at.unwrap_or(from.len() - 1)]
            }
            _ => {
                let fillers: Vec<usize> = (0..b.defs.len()).filter(|&i| b.defs[i].kind == Card::Filler).collect();
                let open: Vec<usize> = fillers.iter().copied().filter(free).collect();
                let from = if open.is_empty() { fillers } else { open };
                if from.is_empty() { 0 } else { from[self.rng.below(from.len())] }
            }
        };
        let rare = crate::tuning::tuning().rare + self.me().kit.mods.luck;
        let roll = self.rng.range(0.0, 1.0);
        Choice { up, rare: b.defs[up].kind != Card::Filler && roll < rare, found: false }
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
            .filter(|&i| self.progress.findable(i) && self.me().kit.offerable(i) && self.banished & 1 << i == 0 && minutes >= b.defs[i].after && (chest || !b.defs[i].chest))
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
        let (from, to) = self.me_mut().kit.raise(c.up, if c.rare { 2 } else { 1 });
        if to > def.max {
            self.broke = true;
            self.juice.banner(format!("LEVEL BREAKER: {} {to}", def.name.to_uppercase()), [GOLD[0], GOLD[1], GOLD[2], 1.0], 2.8);
            self.juice.shake(0.3);
        }
        let heal = def.at(powerups::Stat::Heal, to) * (to - from).max(1) as f32;
        self.me_mut().hp = (self.me().hp + heal).min(self.me().kit.max_hp());
        if def.at(powerups::Stat::Refill, 1) > 0.0 {
            self.me_mut().grenades = self.me().kit.grenades();
        }
        self.xp += def.at(powerups::Stat::Money, 1) as u32;
    }

    /// A vaccine pulls the player back from turning: half health, a moment untouchable, and the dead thrown back.
    fn revive(&mut self, p: PlayerId, sounds: &mut Vec<Sfx>, flow: &mut Flow) {
        let pl = &mut self.players[p];
        pl.revived += 1;
        pl.hp = pl.kit.max_hp() * 0.5;
        pl.hurt = 2.0;
        let at = pl.pos;
        self.shove(at, 600.0);
        self.juice.ring(at, crate::tuning::tuning().push, 0.6, [0.4, 1.0, 0.5]);
        self.juice.flash(at, 20.0, [0.6, 1.8, 0.7], 9.0, 0.8);
        self.feel_for(p, |j| j.shake(0.4));
        flow.buzz(p, Rumble::Fanfare);
        if p < self.views {
            let line = if self.views > 1 { format!("P{}: THE VACCINE HOLDS", self.number(p)) } else { "THE VACCINE HOLDS".into() };
            self.juice.banner(line, [0.5, 1.0, 0.55, 1.0], 2.4);
            sounds.push(Sfx::Heal);
        }
    }

    /// Every player has turned with no fall this tick to end the night, as when the last device left mid-level-up.
    pub(super) fn all_turned(&mut self, sounds: &mut Vec<Sfx>, flow: &mut Flow) {
        sounds.push(Sfx::GameOver);
        self.leveling = 0.0;
        self.spits.clear();
        for mut z in std::mem::take(&mut self.burrowed) {
            z.rise = 1.0;
            self.zombies.push(z);
        }
        self.phase = Phase::Over { t: 0.0, won: false };
        flow.ended = Some(self.finish(false));
    }

    /// How the run went, and what it unlocked.
    fn finish(&mut self, won: bool) -> Run {
        self.juice.tally.shut = true;
        let mut kit = [None; RUN_KIT];
        let held = self.me().kit.weapons.iter().map(|a| (a.def, a.level)).chain(self.me().kit.passives.iter().copied());
        for (slot, (def, level)) in kit.iter_mut().zip(held) {
            *slot = Some((def as u16, level.min(u8::MAX as u32) as u8));
        }
        let run = Run { secs: self.t as u32, kills: self.kills, level: self.level, won, broke: self.broke, difficulty: self.difficulty, dps: self.juice.tally.dps(ME).round() as u32, kit };
        let after = self.progress.after(run);
        self.ready = (0..book().defs.len()).filter(|&i| !self.progress.findable(i) && after.findable(i)).count();
        self.progress = after;
        run
    }

    /// Zombies arrive just out of sight, faster and tougher as the night goes on. Once the night is full, what's owed
    /// banks and rises a few a second as room opens, and stragglers left far behind rise again ahead.
    fn spawn(&mut self, dt: f32) {
        let t = crate::tuning::tuning();
        let minutes = self.t / 60.0;
        self.recycle(dt);
        let more = 1.0 + (self.players.len() - 1) as f32 * t.spawn_per_player;
        self.owed += (t.spawn_rate + minutes * t.spawn_growth).min(t.spawn_top) * more * dt;
        if horde_now(self.t, dt) {
            self.owed += (t.horde_size + minutes * t.horde_growth) * more;
            self.leader_owed = true;
        }
        while self.owed >= 1.0 && self.room() {
            self.owed -= 1.0;
            self.raise(minutes);
        }
        self.let_out = (self.let_out + t.bank_release * dt).min(1.0 + t.bank_release * dt);
        while self.banked >= 1.0 && self.let_out >= 1.0 && self.room() {
            self.banked -= 1.0;
            self.let_out -= 1.0;
            self.raise(minutes);
        }
        if self.owed >= 1.0 {
            self.banked = (self.banked + self.owed.floor()).min(t.bank);
            self.owed = self.owed.fract();
        }
    }

    /// Whether the night has room for another zombie under `[zombies] max`, raised for each player past the first.
    fn room(&self) -> bool {
        self.zombies.len() + self.tells.len() + self.burrowed.len() < self.cap()
    }

    /// The most zombies up at once: `[zombies] max`, and `per_player` of it more for each player past the first.
    pub(super) fn cap(&self) -> usize {
        let t = crate::tuning::tuning();
        (t.max_zombies * (1.0 + (self.players.len() - 1) as f32 * t.max_per_player)) as usize
    }

    /// One zombie starts to rise somewhere on the ring around the player.
    fn raise(&mut self, minutes: f32) {
        let roll = self.rng.range(0.0, 1.0);
        let leader = std::mem::take(&mut self.leader_owed);
        let r = roster();
        let kind = Kind(if leader && self.enemies_off & 1 << r.leader == 0 { r.leader } else { r.pick(minutes, roll, self.enemies_off) } as u8);
        let hp = kind.stats().hp * (1.0 + minutes * crate::tuning::tuning().growth) * if leader { crate::tuning::tuning().leader_hp } else { 1.0 } * self.fury;
        let p = self.ring_player();
        if let Some(pos) = self.ring_spot(p, None) {
            self.tells.push(Tell { kind, pos, hp, t: 0.0, leader, led: None });
        }
    }

    /// The living player the next zombie rises around, taking turns.
    fn ring_player(&mut self) -> PlayerId {
        let living: Vec<PlayerId> = (0..self.players.len()).filter(|&p| self.players[p].alive()).collect();
        if living.len() < 2 {
            return living.first().copied().unwrap_or(ME);
        }
        self.ring_turn = self.ring_turn.wrapping_add(1);
        living[self.ring_turn % living.len()]
    }

    /// An open spot on the ring zombies rise in around player `p`, within a quarter turn either side of `ahead`
    /// when given.
    fn ring_spot(&mut self, p: PlayerId, ahead: Option<[f32; 2]>) -> Option<[f32; 2]> {
        let at = self.players[p].pos;
        for _ in 0..8 {
            let a = match ahead {
                Some(d) => fm::atan2(d[1], d[0]) + self.rng.range(-FRAC_PI_4, FRAC_PI_4),
                None => self.rng.range(0.0, TAU),
            };
            let [near, far] = spawn_ring(self.t);
            let r = self.rng.range(near, far);
            let pos = [at[0] + fm::cos(a) * r, at[1] + fm::sin(a) * r];
            if !self.map.hits_cells(pos[0], pos[1], crate::tuning::tuning().zombie_half()) {
                return Some(pos);
            }
        }
        None
    }

    /// Whether `pos` shows on either screen: on the map, or near enough for the view ahead to draw it.
    fn on_screen(&self, pos: [f32; 2]) -> bool {
        let cam = self.camera();
        let [x, y] = [pos[0] - cam[0], pos[1] - cam[1]];
        let on_map = (-TILE..=view_w() + TILE).contains(&x) && (-TILE..=view_h() + TILE).contains(&y);
        let view = (view3d::VIEW_RADIUS + view3d::CAMERA_BACK) * TILE;
        on_map || self.players.iter().any(|p| p.alive() && len(sub(pos, p.pos)) <= view) || len(sub(pos, self.me().pos)) <= view
    }

    /// A zombie left off screen and far behind long enough rises again ahead of the player, unchanged. The Harvester
    /// never does.
    fn recycle(&mut self, dt: f32) {
        let t = crate::tuning::tuning();
        let mut i = 0;
        while i < self.zombies.len() {
            let z = &self.zombies[i];
            let far = !z.boss && z.led.is_none() && z.rise >= 1.0 && self.players.iter().all(|p| !p.alive() || len(sub(z.pos, p.pos)) > t.straggler_reach) && !self.on_screen(z.pos);
            let z = &mut self.zombies[i];
            z.away = if far { z.away + dt } else { 0.0 };
            if !far || z.away < t.straggler_time {
                i += 1;
                continue;
            }
            let p = self.ring_player();
            let ahead = self.players[p].heading;
            match self.ring_spot(p, Some(ahead)).or_else(|| self.ring_spot(p, None)) {
                Some(pos) => {
                    let z = self.zombies.swap_remove(i);
                    self.tells.push(Tell { kind: z.kind, pos, hp: z.hp, t: 0.0, leader: z.leader, led: None });
                }
                None => i += 1,
            }
        }
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
            (z.led, z.barren) = (t.led, t.led.is_some());
            z.id = self.next_id;
            self.next_id += 1;
            let burrows = t.kind.stats().burrow > 0.0;
            if burrows {
                self.burrowed.push(z);
            } else {
                self.zombies.push(z);
            }
            let puff = if burrows { effects::effects().mound_breaks } else { effects::effects().zombie_rises };
            self.fx.push(Fx::play(puff, [pos[0], pos[1] - 3.0], 0.0));
            self.juice.burst(pos, 0.0, 8, [0.0; 2], 0.0, [15.0, 45.0], juice::dirt(), 1.6, 0.6, 0.0);
            let (pan, gain) = self.place(pos);
            if self.hush[3] <= 0.0 && gain > 60 {
                sounds.push(Sfx::Rise { pan, gain });
                self.hush[3] = 0.3;
            }
        }
    }

    /// Where the living players stand, which the dead hunt; the local player's spot when none stands.
    fn targets(&self) -> Vec<[f32; 2]> {
        let living: Vec<[f32; 2]> = self.players.iter().filter(|p| p.alive()).map(|p| p.pos).collect();
        if living.is_empty() { vec![self.me().pos] } else { living }
    }

    /// How hard each zombie is pushed off its neighbors, in `self.sep`'s buffer; give it back when done.
    fn spread(&mut self) -> Vec<[f32; 2]> {
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
        sep
    }

    fn move_zombies(&mut self, dt: f32, sounds: &mut Vec<Sfx>) {
        let radius = self.ring_radius();
        let sep = self.spread();
        let targets = self.targets();
        let cells: Vec<[usize; 2]> = targets.iter().map(|p| [(p[0] / TILE) as usize, (p[1] / TILE) as usize]).collect();
        if cells != self.flow_cells {
            self.map.flow_from(&targets, &mut self.flow, &mut self.steps);
            self.flow_cells = cells;
        }
        let nearest = |at: [f32; 2]| targets.iter().copied().min_by(|a, b| len(sub(*a, at)).total_cmp(&len(sub(*b, at)))).unwrap_or(at);
        let leaders = self.leaders();
        let mut bites = Vec::new();
        let mut spits = Vec::new();
        for (z, &s) in self.zombies.iter_mut().zip(&sep) {
            z.flash = (z.flash - dt).max(0.0);
            z.hurt = (z.hurt - dt).max(0.0);
            z.squash = (z.squash - dt * 5.0).max(0.0);
            z.cd = (z.cd - dt).max(0.0);
            if z.rise < 1.0 {
                z.rise = (z.rise + dt / crate::tuning::tuning().rise).min(1.0);
                continue;
            }
            z.up += dt;
            if z.stagger > 0.0 {
                z.stagger -= dt;
                continue;
            }
            let t = crate::tuning::tuning();
            let prey = nearest(z.pos);
            let ranged = z.kind.stats().ranged;
            let led = z.led.and_then(|q| leaders.get(q).copied().flatten()).filter(|&(id, ..)| z.id == id || len(sub(prey, z.pos)) > t.leader_chase);
            let (to, speed, spitting) = if let Some((id, wish, at)) = led {
                let (to, speed) = if z.id == id {
                    let k = len(wish).min(1.0);
                    if k > 0.2 { (norm(wish), t.leader_speed * k) } else { (z.dir, 0.0) }
                } else {
                    let d = sub(at, z.pos);
                    (norm(d), z.kind.stats().speed * ((len(d) - 16.0) / 32.0).clamp(0.0, 1.0))
                };
                (to, speed * (1.0 - z.slow_k), false)
            } else {
                let charge = if len(sub(prey, z.pos)) < t.charge_reach { z.kind.stats().charge } else { 1.0 };
                let speed = if z.boss { t.harvester_speed } else { z.kind.stats().speed * charge } * (1.0 - z.slow_k);
                let direct = sub(prey, z.pos);
                let lure = self.decoys.iter().find(|d| len(sub(d.pos, z.pos)) < d.lure);
                let spitting = self.range.is_none() && !z.boss && lure.is_none() && len(direct) <= ranged && clear_shot(&self.map, z.pos, prey);
                let to = match lure {
                    Some(d) => norm(sub(d.pos, z.pos)),
                    None if spitting || len(direct) < TILE * 1.5 => norm(direct),
                    None => self.map.downhill(&self.flow, z.pos).unwrap_or(norm(direct)),
                };
                let speed = if spitting { 0.0 } else { speed };
                let (to, speed) = match &self.range {
                    Some(r) => r.hold(z.pos, prey, radius, speed),
                    None => (to, speed),
                };
                (to, speed, spitting)
            };
            let s = if led.is_some_and(|(id, ..)| id == z.id) { [0.0; 2] } else { s };
            if spitting && z.cd <= 0.0 {
                z.cd = t.spit_every.max(0.1);
                let fly = t.spit_speed.max(1.0);
                let pos = [z.pos[0], z.pos[1] - 6.0];
                spits.push(Spit { kind: z.kind, pos, vel: [to[0] * fly, to[1] * fly], damage: z.kind.stats().damage, life: ranged * 1.5 / fly, age: 0.0 });
            }
            let v = [to[0] * speed + s[0] * 20.0 + z.push[0], to[1] * speed + s[1] * 20.0 + z.push[1]];
            let half = t.zombie_half();
            // Whole cells can't reach prey in a solid cell's open part, so near it, or caught in one, it bumps as the player does.
            let as_player = (len(sub(prey, z.pos)) < TILE * 1.5 && self.map.hits_cells(prey[0], prey[1], half)) || self.map.hits_cells(z.pos[0], z.pos[1], half);
            z.pos = if as_player { self.map.slide(z.pos, [v[0] * dt, v[1] * dt], half) } else { self.map.slide_cells(z.pos, [v[0] * dt, v[1] * dt], half) };
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
        if let Some(s) = spits.first()
            && self.hush[5] <= 0.0
        {
            let (pan, gain) = self.place(s.pos);
            sounds.push(Sfx::Spit { pan, gain });
            self.hush[5] = 0.12;
        }
        self.spits.extend(spits);
        let reach = crate::tuning::tuning().burrow_reach;
        let mut i = 0;
        while i < self.zombies.len() {
            let z = &self.zombies[i];
            let burrow = z.kind.stats().burrow;
            if z.boss || burrow <= 0.0 || z.up < burrow || len(sub(nearest(z.pos), z.pos)) <= reach {
                i += 1;
                continue;
            }
            let mut z = self.zombies.swap_remove(i);
            z.up = 0.0;
            z.rise = 0.0;
            self.fx.push(Fx::play(effects::effects().mound_breaks, [z.pos[0], z.pos[1] - 3.0], 0.0));
            self.juice.burst(z.pos, 0.0, 8, [0.0; 2], 0.0, [15.0, 45.0], juice::dirt(), 1.6, 0.6, 0.0);
            self.burrowed.push(z);
        }
        for (i, at, damage) in bites {
            self.map.hurt_prop(i, damage, 0.0);
            if self.juice.rng.range(0.0, 1.0) < dt * 3.0 {
                self.juice.burst(at, 4.0, 2, [0.0; 2], 0.0, [20.0, 50.0], juice::dirt(), 1.0, 0.3, 0.0);
            }
        }
    }

    /// Burrowers tunnel along the way the dead walk, churning the dirt above them, and break out near the player.
    /// One climbing out stays in `burrowed`, out of every weapon's reach, until it stands.
    fn tunnel(&mut self, dt: f32, sounds: &mut Vec<Sfx>) {
        let t = crate::tuning::tuning();
        let targets = self.targets();
        let nearest = |at: [f32; 2]| targets.iter().copied().min_by(|a, b| len(sub(*a, at)).total_cmp(&len(sub(*b, at)))).unwrap_or(at);
        for i in (0..self.burrowed.len()).rev() {
            let z = &mut self.burrowed[i];
            z.flash = (z.flash - dt).max(0.0);
            z.hurt = (z.hurt - dt).max(0.0);
            z.squash = (z.squash - dt * 5.0).max(0.0);
            z.stagger = (z.stagger - dt).max(0.0);
            if z.rise > 0.0 {
                z.rise += dt / t.rise;
                if z.rise >= 1.0 {
                    let mut z = self.burrowed.swap_remove(i);
                    z.rise = 1.0;
                    z.up = 0.0;
                    self.zombies.push(z);
                }
                continue;
            }
            let direct = sub(nearest(z.pos), z.pos);
            let to = if len(direct) < TILE * 1.5 { norm(direct) } else { self.map.downhill(&self.flow, z.pos).unwrap_or(norm(direct)) };
            let speed = z.kind.stats().speed;
            z.pos = self.map.slide_cells(z.pos, [to[0] * speed * dt, to[1] * speed * dt], t.zombie_half());
            z.dir = to;
            z.facing = Facing::of(to);
            let pos = z.pos;
            if self.juice.rng.range(0.0, 1.0) < dt * 24.0 {
                self.juice.burst(pos, 0.0, 2, [0.0; 2], 0.0, [8.0, 24.0], juice::dirt(), 1.6, 0.5, 0.0);
            }
            if len(sub(nearest(pos), pos)) > t.burrow_reach {
                continue;
            }
            self.burrowed[i].rise = f32::MIN_POSITIVE;
            self.fx.push(Fx::play(effects::effects().mound_breaks, [pos[0], pos[1] - 3.0], 0.0));
            self.juice.burst(pos, 0.0, 16, [0.0; 2], 0.0, [25.0, 70.0], juice::dirt(), 2.0, 0.7, 0.0);
            let (pan, gain) = self.place(pos);
            sounds.push(Sfx::Rise { pan, gain });
            self.hush[3] = 0.3;
        }
    }

    /// Spit flies straight until it splashes on the player or on cover, or falls short.
    fn move_spits(&mut self, dt: f32, sounds: &mut Vec<Sfx>, flow: &mut Flow) {
        let mut hit = vec![0.0f32; self.players.len()];
        let mut splat = None;
        for s in &mut self.spits {
            s.pos = [s.pos[0] + s.vel[0] * dt, s.pos[1] + s.vel[1] * dt];
            s.life -= dt;
            s.age += dt;
            let ground = [s.pos[0], s.pos[1] + 6.0];
            let player = self.players.iter().position(|p| p.alive() && len(sub(ground, p.pos)) < SPIT_REACH);
            if player.is_some() || self.map.cover_at(ground[0], ground[1]) {
                if let Some(p) = player {
                    hit[p] = hit[p].max(s.damage);
                }
                splat = Some(ground);
                self.juice.burst(ground, 6.0, 4, norm([-s.vel[0], -s.vel[1]]), 0.9, [20.0, 50.0], juice::blood_of(s.kind.stats().blood), 1.0, 0.3, 0.0);
                s.life = 0.0;
            }
        }
        self.spits.retain(|s| s.life > 0.0);
        if let Some(at) = splat
            && self.hush[6] <= 0.0
        {
            let (pan, gain) = self.place(at);
            sounds.push(Sfx::Splat { pan, gain });
            self.hush[6] = 0.12;
        }
        for (p, hit) in hit.into_iter().enumerate() {
            if hit > 0.0 && self.players[p].hurt <= 0.0 {
                self.bitten(p, hit * self.fury * self.difficulty.damage(), sounds, flow);
            }
        }
    }

    /// Breaks a prop chewed or shot to pieces, one per frame, and sends the dead the new way through.
    fn crumble(&mut self, dt: f32, sounds: &mut Vec<Sfx>) {
        let Some(c) = self.map.crumble(dt) else { return };
        self.flow_cells.clear();
        self.fx.push(Fx::play(effects::effects().prop_crumbles, [c[0], c[1] - 4.0], 0.0));
        self.juice.burst(c, 4.0, 12, [0.0; 2], 0.0, [30.0, 80.0], juice::dirt(), 1.6, 0.6, 0.0);
        self.noise = Some(c);
        self.thud((1, false), sounds);
    }

    /// The run is over, but what's in flight isn't: shots fly on, grenades land, fires burn down and scarecrows
    /// burst, still hurting the dead, while the dead wander. Nothing new is fired, and the run's numbers stay final.
    fn wind_down(&mut self, dt: f32, sounds: &mut Vec<Sfx>, flow: &mut Flow) {
        for h in &mut self.hush {
            *h -= dt;
        }
        self.fly(dt, sounds, flow);
        self.rise(dt, sounds);
        self.roam(dt);
        for p in 0..self.players.len() {
            self.actor = p;
            self.scatter(dt);
        }
        self.actor = ME;
        self.move_bullets(dt, sounds);
        self.hazards(dt);
        self.decoys(dt, sounds);
        if let Some(by) = self.blow(dt, sounds) {
            flow.buzz(by, Rumble::Blast);
        }
        self.crumble(dt, sounds);
        self.reap(sounds, flow);
    }

    /// The player is down: each zombie stands over them a moment, then turns away to look for someone else.
    fn lose_interest(&mut self, p: PlayerId) {
        let after = crate::tuning::tuning().roam_after.max(0.0);
        for z in &mut self.zombies {
            let away = sub(z.pos, self.players[p].pos);
            let a = fm::atan2(away[1], away[0]) + self.rng.range(-0.6, 0.6);
            z.dir = [fm::cos(a), fm::sin(a)];
            z.roam = -self.rng.range(0.0, after);
        }
        self.players[p].heading = self.players[p].aim;
        self.players[p].wander = self.leg();
    }

    /// Player `p` falls while others stand: they stop acting, and the run goes on without them.
    fn falls(&mut self, p: PlayerId, sounds: &mut Vec<Sfx>) {
        let at = self.players[p].pos;
        self.juice.burst(at, 8.0, 30, [0.0; 2], 0.0, [30.0, 120.0], juice::blood(), 2.0, 0.9, 0.0);
        self.juice.banner(format!("PLAYER {} TURNED", self.number(p)), RED, 2.4);
        if p < self.views() {
            sounds.push(Sfx::GameOver);
        }
        self.feel_for(p, |j| j.shake(0.8));
        self.players[p].heading = self.players[p].aim;
        self.players[p].wander = self.leg();
    }

    /// A fallen player sinks, climbs back out as one of the dead and, once turned, wanders.
    fn turning(&mut self, p: PlayerId, dt: f32, sounds: &mut Vec<Sfx>) {
        let before = self.players[p].down.unwrap_or(0.0);
        let t = before + dt;
        self.players[p].down = Some(t);
        if self.players[p].lead.is_some() {
            return self.follow_leads();
        }
        if self.players[p].left {
            self.players[p].down = Some(t.min(TURN_SINK));
            return;
        }
        if t >= TURNED && before < TURNED && self.may_lead() {
            let at = self.players[p].pos;
            return self.rise_leading(p, at, 1.0);
        }
        if t >= TURNED {
            self.wander_turned(p, dt);
        }
        if before < TURN_RISE && t >= TURN_RISE {
            let at = self.players[p].pos;
            self.juice.burst(at, 0.0, 16, [0.0; 2], 0.0, [20.0, 60.0], juice::dirt(), 1.8, 0.7, 0.0);
            self.fx.push(Fx::play(effects::effects().player_rises, [at[0], at[1] - 3.0], 0.0));
            if p < self.views {
                sounds.push(Sfx::Turn);
            }
            self.feel_for(p, |j| j.shake(0.35));
        }
    }

    /// Seconds a wanderer keeps one way.
    fn leg(&mut self) -> f32 {
        crate::tuning::tuning().roam_turn.max(0.1) * self.rng.range(0.5, 1.5)
    }

    /// A new way to wander from `dir`: a little off it, or well round when the way ahead is blocked.
    fn veer(&mut self, dir: [f32; 2], blocked: bool) -> [f32; 2] {
        let turn = if blocked { self.rng.range(PI / 2.0, PI * 1.5) } else { self.rng.range(-1.2, 1.2) };
        let a = fm::atan2(dir[1], dir[0]) + turn;
        [fm::cos(a), fm::sin(a)]
    }

    /// After the run the dead wander the farm in search of another human, each its own way, turning now and then
    /// and off whatever stops it. The turned player is one of them, so nothing gathers on them.
    fn roam(&mut self, dt: f32) {
        let t = crate::tuning::tuning();
        let sep = self.spread();
        for (i, &s) in sep.iter().enumerate() {
            let z = &mut self.zombies[i];
            z.flash = (z.flash - dt).max(0.0);
            z.hurt = (z.hurt - dt).max(0.0);
            z.squash = (z.squash - dt * 5.0).max(0.0);
            if z.rise < 1.0 {
                z.rise = (z.rise + dt / t.rise).min(1.0);
                continue;
            }
            if z.stagger > 0.0 {
                z.stagger -= dt;
                continue;
            }
            if z.roam < 0.0 {
                z.roam = (z.roam + dt).min(0.0);
                continue;
            }
            z.roam -= dt;
            if z.roam <= 0.0 {
                let dir = z.dir;
                let dir = self.veer(dir, false);
                let leg = self.leg();
                let z = &mut self.zombies[i];
                (z.dir, z.roam) = (dir, leg);
            }
            let z = &mut self.zombies[i];
            let speed = if z.boss { t.harvester_speed } else { z.kind.stats().speed } * t.roam_speed * (1.0 - z.slow_k);
            let v = [z.dir[0] * speed + s[0] * 20.0 + z.push[0], z.dir[1] * speed + s[1] * 20.0 + z.push[1]];
            let from = z.pos;
            z.pos = self.map.slide_cells(from, [v[0] * dt, v[1] * dt], t.zombie_half());
            z.push = [z.push[0] * (1.0 - 8.0 * dt).max(0.0), z.push[1] * (1.0 - 8.0 * dt).max(0.0)];
            z.facing = Facing::of(z.dir);
            z.anim += dt;
            if speed > 0.0 && len(sub(z.pos, from)) < speed * dt * 0.3 {
                let dir = z.dir;
                let dir = self.veer(dir, true);
                let leg = self.leg();
                let z = &mut self.zombies[i];
                (z.dir, z.roam) = (dir, leg);
            }
        }
        self.sep = sep;
    }

    /// Turned, the player wanders like the rest, starting off the way they faced.
    fn wander_turned(&mut self, p: PlayerId, dt: f32) {
        if self.players[p].wander <= 0.0 {
            self.players[p].heading = self.veer(self.players[p].heading, false);
            self.players[p].wander = self.leg();
        }
        self.players[p].wander -= dt;
        let from = self.players[p].pos;
        let step = TURNED_SPEED * dt;
        let heading = self.players[p].heading;
        self.players[p].pos = self.map.slide(from, [heading[0] * step, heading[1] * step], crate::tuning::tuning().player_half());
        self.players[p].facing = Facing::of(heading);
        if len(sub(self.players[p].pos, from)) < step * 0.3 {
            self.players[p].heading = self.veer(self.players[p].heading, true);
            self.players[p].wander = self.leg();
        }
    }

    fn contact(&mut self, sounds: &mut Vec<Sfx>, flow: &mut Flow) {
        if self.range.is_some() {
            return;
        }
        for p in 0..self.players.len() {
            let pl = &self.players[p];
            if pl.hurt > 0.0 || !pl.alive() {
                continue;
            }
            let biters = self.zombies.iter().filter(|z| z.rise >= 1.0 && len(sub(z.pos, pl.pos)) < crate::tuning::tuning().bite_reach);
            let (hit, by) = biters.map(|z| (z.kind.stats().damage * if z.boss { crate::tuning::tuning().harvester_bite } else { 1.0 } * self.fury * self.difficulty.damage(), z.led)).fold((0.0, None), |a, b| if b.0 > a.0 { b } else { a });
            if hit > 0.0 {
                if let Some(q) = by {
                    self.juice.tally.book_mut(q).bites += 1;
                }
                self.bitten(p, hit, sounds, flow);
            }
        }
    }

    /// The player takes `hit`, less armor, and can't be hurt again for a moment.
    fn bitten(&mut self, p: PlayerId, hit: f32, sounds: &mut Vec<Sfx>, flow: &mut Flow) {
        let pl = &mut self.players[p];
        let hit = (hit - pl.kit.mods.armor).max(1.0);
        pl.hp -= hit;
        pl.hurt = self.difficulty.hurt_time();
        let at = pl.pos;
        self.juice.popup(at, hit, Tone::Hurt);
        flow.buzz(p, Rumble::Hit);
        if p < self.views() {
            sounds.push(Sfx::Hurt);
        }
        self.ouch(p, 0.45);
    }

    /// Player `p` takes a hit and bleeds; for the local player the screen flashes red and shakes, and the world
    /// catches its breath.
    fn ouch(&mut self, p: PlayerId, shake: f32) {
        self.feel_for(p, |j| {
            j.hurt = 0.35;
            j.shake(shake);
            j.stop(0.08);
        });
        self.juice.burst(self.players[p].pos, 8.0, 10, [0.0; 2], 0.0, [30.0, 80.0], juice::blood(), 1.6, 0.5, 0.0);
    }

    /// One hit sound for however many landed this frame, unless one just played.
    fn thud(&mut self, (hits, crit): (u32, bool), sounds: &mut Vec<Sfx>) {
        if hits > 0 && (self.hush[0] <= 0.0 || crit) {
            sounds.push(Sfx::Thud(crit));
            self.hush[0] = 0.06;
        }
    }

    fn move_bullets(&mut self, dt: f32, sounds: &mut Vec<Sfx>) {
        let crits: Vec<f32> = self.players.iter().map(|p| crate::tuning::tuning().crit + p.kit.mods.crit).collect();
        let own_shot = effects::effects().bullet.shot;
        let mut hits = (0, false);
        for b in &mut self.bullets {
            b.pos = [b.pos[0] + b.vel[0] * dt, b.pos[1] + b.vel[1] * dt];
            b.life -= dt;
            b.age += dt;
            let ground = [b.pos[0], b.pos[1] + 6.0];
            let back = norm([-b.vel[0], -b.vel[1]]);
            let shot = b.look.map_or(own_shot, |e| e.shot);
            let from_above = b.by.and_then(|d| book().defs[d].does) == Some(powerups::Does::Drone);
            if !from_above && self.map.cover_at(ground[0], ground[1]) {
                let cell = |p: [f32; 2]| [(p[0] / TILE).floor(), (p[1] / TILE).floor()];
                let at = cell(ground);
                if let Some(barrel) = self.barrels.iter_mut().find(|r| cell(r.pos) == at) {
                    if barrel.hp > 0.0 {
                        barrel.by = b.owner;
                    }
                    barrel.hp -= b.damage;
                    barrel.flash = FLASH_TIME;
                    self.juice.burst(ground, 6.0, 4, back, 0.8, [40.0, 100.0], juice::spark(), 0.8, 0.2, 1.0);
                } else if let Some(i) = self.map.breakable_at(ground[0], ground[1]) {
                    self.map.hurt_prop(i, b.damage, FLASH_TIME);
                    self.juice.burst(ground, 6.0, 4, back, 0.9, [30.0, 70.0], juice::dirt(), 1.0, 0.3, 0.0);
                } else {
                    self.juice.burst(ground, 6.0, 2, back, 0.9, [20.0, 50.0], juice::dirt(), 1.0, 0.3, 0.0);
                }
                if let Some(s) = &shot {
                    self.juice.impact(ground, 6.0, back, s);
                }
                b.life = 0.0;
                continue;
            }
            let spent = b.life <= 0.0;
            for z in &mut self.zombies {
                if b.life <= 0.0 || b.hit.contains(&z.id) {
                    continue;
                }
                let d = sub([z.pos[0], z.pos[1] - 6.0], b.pos);
                if len(d) < 7.0 {
                    self.juice.tally.credit(b.owner, b.by);
                    hits.1 |= strike(z, &mut self.juice, b.damage, norm(b.vel), b.force, crits[b.owner]);
                    hits.0 += 1;
                    if let Some(s) = &shot {
                        self.juice.impact(ground, 6.0, back, s);
                    }
                    b.hit.push(z.id);
                    if b.pierce == 0 {
                        b.life = 0.0;
                    } else {
                        b.pierce -= 1;
                    }
                }
            }
            if let (true, Some(s)) = (spent, &shot) {
                self.juice.impact(ground, 1.0, back, s);
            }
        }
        self.juice.tally.by = None;
        self.bullets.retain(|b| b.life > 0.0);
        self.thud(hits, sounds);
    }

    /// Sets off a barrel shot to pieces, one per frame so neighbors chain; returns who set off the one that blew.
    fn blow(&mut self, dt: f32, sounds: &mut Vec<Sfx>) -> Option<PlayerId> {
        for r in &mut self.barrels {
            r.flash = (r.flash - dt).max(0.0);
        }
        let i = self.barrels.iter().position(|r| r.hp <= 0.0)?;
        let barrel = self.barrels.swap_remove(i);
        let c = barrel.center();
        self.map.open(c[0], c[1]);
        self.flow_cells.clear();
        self.juice.tally.by = None;
        self.actor = barrel.by;
        self.explode(c, crate::tuning::tuning().barrel_radius, crate::tuning::tuning().barrel_damage, sounds);
        self.actor = ME;
        Some(barrel.by)
    }

    /// Damages zombies in `radius`, sets off barrels in it, and hurts the acting player if too close, never another.
    /// Bigger with the acting player's fuel drums' blast, area passive's reach and damage passives.
    fn explode(&mut self, c: [f32; 2], radius: f32, damage: f32, sounds: &mut Vec<Sfx>) {
        let m = self.actor().kit.mods;
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
                if r.hp > 0.0 {
                    r.by = self.actor;
                }
                r.hp = r.hp.min(0.0);
            }
        }
        for i in self.map.breakables_near(c, radius) {
            self.map.hurt_prop(i, damage, FLASH_TIME);
        }
        let t = crate::tuning::tuning();
        let playing = !matches!(self.phase, Phase::Over { .. });
        let own = self.actor();
        if playing && own.alive() && len(sub(own.pos, c)) < radius * t.blast_near && own.hurt <= 0.0 {
            let hurt = (t.blast_hurt - m.armor).max(1.0);
            let at = own.pos;
            self.actor_mut().hp -= hurt;
            self.juice.popup(at, hurt, Tone::Hurt);
            self.actor_mut().hurt = self.difficulty.hurt_time();
            self.ouch(self.actor, 0.3);
        }
        self.fx.push(Fx::play(effects::effects().explosion, c, 0.0));
        self.fx.push(Fx::play(effects::effects().explosion_smoke, [c[0], c[1] - 8.0], 0.0));
        self.shake_each(|at| (0.75 * (1.0 - len(sub(at, c)) / 260.0)).max(0.12), 4.0);
        self.juice.stop(0.07);
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
        self.throw_at([cam[0] + touch[0] / map_scale(), cam[1] + touch[1] / map_scale()], sounds);
    }

    fn throw_at(&mut self, to: [f32; 2], sounds: &mut Vec<Sfx>) {
        self.throw_by(ME, to, sounds);
    }

    /// Player `p` lobs a grenade at `to`, if one is ready.
    fn throw_by(&mut self, p: PlayerId, to: [f32; 2], sounds: &mut Vec<Sfx>) {
        let pl = &mut self.players[p];
        if pl.grenades < 1.0 || !pl.alive() {
            if p < self.views {
                sounds.push(Sfx::Miss);
            }
            return;
        }
        if !self.endless {
            pl.grenades -= 1.0;
        }
        let from = pl.pos;
        self.thrown.push(Grenade { from, to, t: 0.0, kind: Thrown::Grenade, by: None, owner: p });
        sounds.push(Sfx::Throw);
    }

    fn fly(&mut self, dt: f32, sounds: &mut Vec<Sfx>, flow: &mut Flow) {
        for g in &mut self.thrown {
            g.t += dt;
        }
        let (landed, flying): (Vec<Grenade>, Vec<Grenade>) = std::mem::take(&mut self.thrown).into_iter().partition(|g| g.t >= crate::tuning::tuning().grenade_flight);
        self.thrown = flying;
        for g in landed {
            self.juice.tally.credit(g.owner, g.by);
            self.actor = g.owner;
            self.land(&g, sounds);
            self.actor = ME;
            self.juice.tally.by = None;
            let blast = matches!(g.kind, Thrown::Lob { def, .. } if powerups::book().defs[def].lands == powerups::Lands::Blast);
            if blast || matches!(g.kind, Thrown::Grenade | Thrown::Bomblet) {
                flow.buzz(g.owner, Rumble::Blast);
            }
        }
    }

    /// A zombie dying at `pos`, knocked toward `fall` and flung `fling` hard: blood in its `own` color or the game's, a
    /// stain, gibs and its corpse.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn gore(&mut self, pos: [f32; 2], fall: [f32; 2], fling: f32, dir: [f32; 2], look: &Look, gibs: usize, own: Option<[u8; 3]>) {
        let splat = effects::effects().blood;
        let tint = splat.frames.first().map_or([1.0; 3], |&a| juice::blood_tint(a, own));
        self.fx.push(Fx { tint, ..Fx::play(splat, [pos[0], pos[1] - 4.0], 0.0) });
        let stain = art::BLOOD_STAIN[self.rng.below(art::BLOOD_STAIN.len())];
        self.stain((stain, pos[0] - stain.w() / 2.0, pos[1] - stain.h() / 2.0, juice::blood_tint(stain, own)));
        let (vel, vz) = if fling > 0.0 { ([fall[0] * fling, fall[1] * fling], fling * 0.9) } else { ([fall[0] * 25.0, fall[1] * 25.0], 0.0) };
        let flip = self.juice.rng.below(2) == 0;
        let dead = look.dead.map(|d| (d, dir));
        self.juice.corpse(Corpse { art: look.hurt[Facing::Front.index()], dead, flip, pos, z: 0.0, vel, vz, fall, age: 0.0, spin: 0.0 });
        self.juice.burst(pos, 8.0, gibs, fall, 1.2, [30.0, 110.0], juice::blood_of(own), 1.6, 0.6, 0.0);
        self.juice.burst(pos, 8.0, gibs / 3, fall, 0.8, [20.0, 60.0], juice::dark_blood_of(own), 2.4, 0.8, 0.0);
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
            if z.barren || matches!(self.phase, Phase::Over { .. }) {
                let s = z.kind.stats();
                let fall = if z.knock == [0.0; 2] { norm(sub(z.pos, self.players[self.nearest_player(z.pos)].pos)) } else { z.knock };
                self.gore(z.pos, fall, z.fling, z.dir, &s.look, s.gibs as usize, s.blood);
                died = Some(z.kind);
                flow.buzz(self.nearest_player(z.pos), Rumble::Tick);
                continue;
            }
            if z.boss && self.waves.is_none() {
                self.waves = Some((crate::tuning::tuning().harvester_again, 2));
            }
            self.kills += 1;
            self.kill_bump = 0.0;
            flow.buzz(self.nearest_player(z.pos), Rumble::Tick);
            died = Some(match died {
                Some(k) if k.stats().heavy => k,
                _ => z.kind,
            });
            let s = z.kind.stats();
            let fall = if z.knock == [0.0; 2] { norm(sub(z.pos, self.players[self.nearest_player(z.pos)].pos)) } else { z.knock };
            self.gore(z.pos, fall, z.fling, z.dir, &s.look, s.gibs as usize, s.blood);
            if s.heavy {
                self.juice.stop(0.06);
                self.feel_for(self.nearest_player(z.pos), |j| j.shake(0.2));
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
            self.drops.push(Drop { kind, pos: z.pos, z: 4.0, vz, vel: [fm::cos(a) * speed, fm::sin(a) * speed], age: 0.0, pulled: None, to: ME, dog: None });
        }
        if let Some(kind) = died
            && (self.hush[1] <= 0.0 || kind.stats().heavy)
        {
            sounds.push(Sfx::Kill(kind.0));
            self.hush[1] = 0.05;
        }
    }

    /// Where the truck driver stands while he holds a collection ticket.
    fn ticket(&self) -> Option<[f32; 2]> {
        let on = crate::tuning::tuning().driver_after > 0.0;
        self.map.driver.filter(|_| on && self.t >= self.ticket)
    }

    /// Collection day: at the driver with a ticket ready, every coin on the map flies to the player.
    fn cash_in(&mut self, sounds: &mut Vec<Sfx>) {
        let t = crate::tuning::tuning();
        let Some(at) = self.ticket() else { return };
        let Some(p) = (0..self.players.len()).find(|&p| self.players[p].alive() && len(sub(self.players[p].pos, at)) <= t.driver_reach) else { return };
        self.ticket = self.t + t.driver_every;
        for d in self.drops.iter_mut().filter(|d| matches!(d.kind, DropKind::Money(_)) && d.pulled.is_none()) {
            let side = if self.juice.rng.below(2) == 0 { 1.0 } else { -1.0 };
            d.catch(norm(sub(self.players[p].pos, d.pos)), side);
            (d.to, d.dog) = (p, None);
        }
        sounds.push(Sfx::Pick(true));
        self.soul_harvest(at);
    }

    fn soul_harvest(&mut self, at: [f32; 2]) {
        self.juice.banner("SOUL HARVEST", GOLD, 2.4);
        self.juice.flash(at, 24.0, [1.8, 1.4, 0.6], 9.0, 0.8);
    }

    fn collect(&mut self, dt: f32, sounds: &mut Vec<Sfx>) {
        let mut money = None;
        let mut healed = None;
        for i in 0..self.drops.len() {
            let nearest = self.nearest_player(self.drops[i].pos);
            let d = &mut self.drops[i];
            d.age += dt;
            if d.dog.is_some_and(|f| f.carried) {
                continue;
            }
            if d.z > 0.0 || d.vz > 0.0 {
                d.vz -= 320.0 * dt;
                d.z += d.vz * dt;
                if d.z <= 0.0 {
                    d.z = 0.0;
                    d.vz = if d.vz < -40.0 { -d.vz * 0.35 } else { 0.0 };
                }
            }
            if !self.players[d.to].alive() {
                d.to = nearest;
            }
            if d.pulled.is_none()
                && let Some(p) = (0..self.players.len()).find(|&p| self.players[p].alive() && len(sub(self.players[p].pos, d.pos)) < self.players[p].kit.magnet())
            {
                let side = if self.juice.rng.below(2) == 0 { 1.0 } else { -1.0 };
                d.catch(norm(sub(self.players[p].pos, d.pos)), side);
                (d.to, d.dog) = (p, None);
            }
            let to = sub(self.players[d.to].pos, d.pos);
            let dist = len(to);
            let n = norm(to);

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
                let p = d.to;
                match d.kind {
                    DropKind::Money(v) => {
                        self.juice.tally.book_mut(p).hearts += 1;
                        self.xp_frac += v as f32 * self.players[p].kit.mods.greed;
                        self.xp += self.xp_frac as u32;
                        self.xp_frac = self.xp_frac.fract();
                        money = Some(p);
                        if self.juice.motes.len() < 24 {
                            self.juice.motes.push(juice::Mote { pos: d.pos, age: 0.0 });
                        }
                    }
                    DropKind::Chest => {
                        self.chests += 1;
                        sounds.push(Sfx::LevelUp);
                        self.juice.shout("A CHEST", GOLD, 0.9);
                        self.juice.flash(self.players[p].pos, 20.0, [1.8, 1.4, 0.6], 9.0, 0.8);
                    }
                    DropKind::Health => {
                        let gain = self.difficulty.heal().min(self.players[p].kit.max_hp() - self.players[p].hp);
                        self.players[p].hp += gain;
                        if gain > 0.0 {
                            healed = Some(p);
                            self.juice.popup(self.players[p].pos, gain, Tone::Heal);
                        }
                    }
                }
                d.age = -1.0;
            } else {
                d.pos = [d.pos[0] + step[0], d.pos[1] + step[1]];
            }
        }
        self.drops.retain(|d| d.age >= 0.0);
        if let Some(p) = money {
            let (since, n) = self.chain;
            let n = if since < 0.5 { n + 1 } else { 0 };
            self.chain = (0.0, n);
            sounds.push(Sfx::Pickup(n.min(20) as u8));
            self.juice.burst(self.players[p].pos, 10.0, 2, [0.0; 2], 0.0, [10.0, 30.0], juice::gold(), 0.8, 0.25, 1.0);
        }
        if let Some(p) = healed {
            sounds.push(Sfx::Heal);
            self.juice.burst(self.players[p].pos, 8.0, 14, [0.0; 2], 0.0, [10.0, 40.0], [0.4, 1.0, 0.5], 1.2, 0.7, 1.0);
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
            let near = self.players.iter().filter(|p| p.alive()).map(|p| p.pos).min_by(|p, q| len(sub(b.pos, *p)).total_cmp(&len(sub(b.pos, *q)))).unwrap_or(self.players[ME].pos);
            let startled = len(sub(b.pos, near)) < 40.0 || self.noise.is_some_and(|n| len(sub(b.pos, n)) < 110.0);
            if startled {
                let away = sub(b.pos, near);
                let a = fm::atan2(away[1], away[0]) + self.juice.rng.range(-0.7, 0.7);
                let speed = self.juice.rng.range(55.0, 85.0);
                b.vel = [fm::cos(a) * speed, fm::sin(a) * speed];
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
            let vel = [fm::cos(a) * 20.0, fm::sin(a) * 20.0];
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
            self.shake_all(0.3);
            flow.buzz_all(Rumble::Hit);
            sounds.push(Sfx::Horde);
        } else if minute > ((self.t - dt) / 60.0) as usize && (1..10).contains(&minute) {
            self.juice.banner(format!("{minute}:00   {}", MINUTE_LINES[minute - 1]), WHITE, 3.0);
            sounds.push(Sfx::Bell);
            // Where they will rise from now on.
            self.juice.ring(self.me().pos, spawn_ring(self.t)[0], 1.6, [0.85, 0.2, 0.15]);
        }

        let night = (1.0 - self.t / dawn()).max(0.0);
        let fireflies = self.juice.particles.iter().filter(|p| p.wander).count();
        if (fireflies as f32) < 4.0 + 14.0 * night && self.juice.rng.range(0.0, 1.0) < dt * 4.0 {
            let a = self.juice.rng.range(0.0, TAU);
            let r = self.juice.rng.range(30.0, 130.0);
            let life = self.juice.rng.range(5.0, 9.0);
            let pos = [self.me().pos[0] + fm::cos(a) * r, self.me().pos[1] + fm::sin(a) * r];
            let z = self.juice.rng.range(4.0, 18.0);
            self.juice.particle(Particle {
                fade: effects::Fade::OPAQUE,
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
                grow: 0.0,
                art: None,
            });
        }

        if !self.drops.is_empty() && self.juice.rng.range(0.0, 1.0) < dt * 6.0 {
            let i = self.juice.rng.below(self.drops.len());
            let d = &self.drops[i];
            let (pos, z) = (d.pos, d.z + 6.0);
            if len(sub(pos, self.me().pos)) < 150.0 {
                self.juice.particle(Particle {
                fade: effects::Fade::OPAQUE,
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
                    grow: 0.0,
                    art: None,
                });
            }
        }

        if self.hush[2] <= 0.0 && !self.zombies.is_empty() {
            let i = self.juice.rng.below(self.zombies.len());
            let z = &self.zombies[i];
            let at = z.pos;
            if z.rise >= 1.0 && len(sub(at, self.me().pos)) < 160.0 {
                let (pan, gain) = self.place(at);
                sounds.push(Sfx::Groan { pan, gain });
                self.hush[2] = self.juice.rng.range(0.7, 2.4);
            }
        }

        let health = self.me().hp / self.me().kit.max_hp();
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
        self.feel_hp(dt);
        self.chain.0 += dt;
        self.kill_bump += dt;
    }

    /// The health bar's trailing chunk catches down to ME's health.
    fn feel_hp(&mut self, dt: f32) {
        if self.me().hurt < self.difficulty.hurt_time() - 0.35 {
            self.hp_shown = (self.hp_shown - 60.0 * dt).max(self.me().hp);
        }
        self.hp_shown = self.hp_shown.max(self.me().hp);
    }

    /// Shakes every viewed player's camera by `amount` of where they stand, and punches it in by `punch`.
    fn shake_each(&mut self, amount: impl Fn([f32; 2]) -> f32, punch: f32) {
        for p in 0..self.views.min(self.players.len()) {
            let a = amount(self.players[p].pos);
            self.feel_for(p, |j| {
                j.shake(a);
                j.punch += punch;
            });
        }
    }

    fn shake_all(&mut self, amount: f32) {
        self.shake_each(|_| amount, 0.0);
    }

    /// Stereo pan (-100 left to 100 right) and loudness (0 to 255) for a sound at `at`, from where the nearest
    /// player with a view faces.
    fn place(&self, at: [f32; 2]) -> (i8, u8) {
        let far = |p: &PlayerId| len(sub(at, self.players[*p].pos));
        let near = (0..self.views().min(self.players.len())).min_by(|a, b| far(a).total_cmp(&far(b))).unwrap_or(ME);
        let pl = &self.players[near];
        let d = sub(at, pl.pos);
        let right = [-fm::sin(pl.view_yaw), fm::cos(pl.view_yaw)];
        let n = norm(d);
        let pan = (n[0] * right[0] + n[1] * right[1]) * 0.8;
        let gain = (1.0 - len(d) / 260.0).clamp(0.0, 1.0);
        let placed = ((pan * 100.0) as i8, (gain * 255.0) as u8);
        if self.net.role == online::Role::Host {
            self.net.placed.borrow_mut().push((placed.0, placed.1, at));
        }
        placed
    }

    /// How dread the music should feel, 0 to 1: the night wearing on and the dead closing in.
    pub fn dread(&self) -> f32 {
        if self.calm || !matches!(self.phase, Phase::Playing | Phase::LevelUp { .. }) {
            return 0.0;
        }
        let close = self.zombies.iter().filter(|z| len(sub(z.pos, self.me().pos)) < 90.0).count() as f32;
        (0.25 + 0.35 * self.t / dawn() + close / 40.0).min(1.0)
    }

    /// Top-left of the view in world pixels, rounded to whole screen pixels.
    fn camera(&self) -> [f32; 2] {
        self.camera_over([view_w(), view_h()])
    }

    /// Top-left of the map as drawn into the view being laid out, which may show more than a screen's worth.
    pub(super) fn map_camera(&self) -> [f32; 2] {
        self.camera_over(self.canvas.map(|n| n / map_scale()))
    }

    /// Top-left of a map view `[w, h]` world pixels across and down around the player, kept on the field.
    fn camera_over(&self, [w, h]: [f32; 2]) -> [f32; 2] {
        let x = (self.me().pos[0] - w / 2.0).clamp(0.0, (self.map.width_px() - w).max(0.0));
        let y = (self.me().pos[1] - 8.0 - h / 2.0).clamp(0.0, (self.map.height_px() - h).max(0.0));
        [(x * map_scale()).round() / map_scale(), (y * map_scale()).round() / map_scale()]
    }

    /// Lays the views drawn from now on out to `size`, as `Canvas::refit` will; a screen's 1024x768 for None.
    pub fn set_canvas(&mut self, size: Option<[f32; 2]>) {
        self.canvas = size.unwrap_or([SCREEN_W, SCREEN_H]);
    }
}

const CHOICE_TOP: f32 = 160.0;
const PAUSE_CHOICES: [&str; 5] = ["Resume", "Settings", "Photo mode", "New farm", "Quit to title"];
const PAUSE_PHOTO: usize = 2;
const CHOICE_ROW: f32 = 150.0;
const PAUSE_ROW: f32 = 110.0;

/// Which pause row a bottom-screen touch at `y` lands on.
fn pause_row(y: f32) -> Option<usize> {
    let row = ((y - CHOICE_TOP) / PAUSE_ROW).floor();
    (0.0..PAUSE_CHOICES.len() as f32).contains(&row).then_some(row as usize)
}

/// Which level-up card a bottom-screen touch at `y` lands on.
fn choice_row(y: f32) -> Option<usize> {
    let row = ((y - CHOICE_TOP) / CHOICE_ROW).floor();
    (0.0..4.0).contains(&row).then_some(row as usize)
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum LevelKey {
    Banish,
    Reroll,
    Favor,
}

/// The level-up's on-screen keys under the cards, each with its x, y, width and height on the bottom screen.
const LEVEL_KEYS: [(LevelKey, [f32; 4]); 3] = [
    (LevelKey::Banish, [112.0, CHOICE_TOP + 3.0 * CHOICE_ROW, 256.0, CHOICE_ROW - 40.0]),
    (LevelKey::Reroll, [384.0, CHOICE_TOP + 3.0 * CHOICE_ROW, 256.0, CHOICE_ROW - 40.0]),
    (LevelKey::Favor, [656.0, CHOICE_TOP + 3.0 * CHOICE_ROW, 256.0, CHOICE_ROW - 40.0]),
];

/// Whether a level-up key shows: `tuning.txt` gives the run some of it, and Favor only from `favor_level`.
fn level_key_shows(key: LevelKey, level: u32) -> bool {
    let t = crate::tuning::tuning();
    match key {
        LevelKey::Banish => t.banish >= 1.0,
        LevelKey::Reroll => t.reroll >= 1.0,
        LevelKey::Favor => t.favor >= 1.0 && level as f32 >= t.favor_level,
    }
}

/// Which level-up key a bottom-screen touch lands on, among those showing at `level`.
fn level_key([x, y]: [f32; 2], level: u32) -> Option<LevelKey> {
    LEVEL_KEYS.iter().find(|(k, [kx, ky, w, h])| (*kx..kx + w).contains(&x) && (*ky..ky + h).contains(&y) && level_key_shows(*k, level)).map(|(k, _)| *k)
}

/// Lands a hit: damage (now and then doubled), knockback, a red flash, a squash, a number and a spray of blood.
/// Returns whether it was a critical hit.
fn strike(z: &mut Zombie, juice: &mut Juice, damage: f32, dir: [f32; 2], force: f32, crit_chance: f32) -> bool {
    let crit = juice.rng.range(0.0, 1.0) < crit_chance;
    let dealt = if crit { damage * 2.0 } else { damage };
    juice.tally.land(dealt, z.hp);
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
    juice.burst(z.pos, 8.0, if crit { 6 } else { 3 }, dir, 0.6, [30.0, 90.0], juice::blood_of(z.kind.stats().blood), 1.4, 0.4, 0.0);
    crit
}

/// Whether a horde breaks out during the frame ending at `t`; one comes every `[spawn] horde_every` seconds.
fn horde_now(t: f32, dt: f32) -> bool {
    let every = crate::tuning::tuning().horde_every;
    (t - dt).rem_euclid(every) > t.rem_euclid(every)
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
mod coop;

#[cfg(test)]
mod tests {
    use super::*;

    static ADDED: std::sync::Mutex<Vec<&'static Enemy>> = std::sync::Mutex::new(Vec::new());

    #[test]
    fn the_corn_bench_stands_tall_swaying_corn_and_its_horde_in_view_every_way_it_turns() {
        let mut h = Harvest::bench_corn(MAP_SIZE);
        assert_eq!(h.zombies.len(), crate::tuning::tuning().max_zombies as usize);
        let [tx, ty] = h.me().pos.map(|v| (v / TILE) as usize);
        assert!(h.map.standing(tx, ty).is_none(), "the player stands in a lane");
        let stalk = h.map.standing(tx + 1, ty + 1).expect("corn beside the lane");
        assert!(stalk.tall > 1 && stalk.sway.degrees > 0.0, "the corn ground's own height and sway: {stalk:?}");
        for k in 0..8 {
            h.me_mut().view_yaw = k as f32 * TAU / 8.0;
            let (cells, cards) = view3d::crops_in_view(&h);
            assert!(cells > 300 && cards == cells * stalk.tall as usize, "at {k}/8 of a turn: {cells} cells, {cards} cards");
        }
        assert!(h.summary().contains(" crops="), "{}", h.summary());
        assert!(h.bench_report(|_| 0.0).contains("crop cells"));
    }

    pub(super) fn added_kind(i: usize) -> &'static Enemy {
        ADDED.lock().unwrap()[i]
    }

    /// A kind past the roster's end: a skinny with `change` made, for trying a behavior the shipped kinds lack.
    fn skinny_but(change: impl FnOnce(&mut Enemy)) -> Kind {
        let mut e = Kind::named("skinny").stats().clone();
        change(&mut e);
        let mut added = ADDED.lock().unwrap();
        added.push(Box::leak(Box::new(e)));
        Kind((roster().enemies.len() + added.len() - 1) as u8)
    }

    #[test]
    fn a_kind_with_its_own_blood_bleeds_splats_and_stains_in_it() {
        let own = [0x7A, 0xA0, 0x62];
        let green = skinny_but(|e| e.blood = Some(own));
        let color = own.map(|v| v as f32 / 255.0);
        let mut h = Harvest::calm(1);
        let mut sounds = Vec::new();
        // A burst shades each speck from 0.8 to 1.15 times its color.
        let of = |p: &[f32; 3], c: [f32; 3]| (0..3).all(|k| (0.79..1.16).contains(&(p[k] / c[k])));
        for (kind, blood, dark) in [(green, color, juice::dark_blood_of(Some(own))), (Kind::named("skinny"), juice::blood(), juice::dark_blood())] {
            h.juice.particles.clear();
            let mut z = Zombie::new(kind, [h.me().pos[0] + 40.0, h.me().pos[1]], 1.0, 1.0, 0.0);
            strike(&mut z, &mut h.juice, 5.0, [1.0, 0.0], 0.0, 0.0);
            assert!(h.juice.particles.iter().all(|p| of(&p.color, blood)), "a hit sprays its blood");
            h.juice.particles.clear();
            h.zombies.push(z);
            h.reap(&mut sounds, &mut Flow::default());
            let colors: Vec<[f32; 3]> = h.juice.particles.iter().map(|p| p.color).collect();
            assert!(colors.iter().any(|p| of(p, blood)) && colors.iter().any(|p| of(p, dark)), "its death sprays its blood and the darker shade");
            let (stain, fx) = (h.stains.last().unwrap(), h.fx.last().unwrap());
            let mean = stain.0.mean().unwrap();
            let drawn: Vec<f32> = (0..3).map(|k| mean[k] * stain.3[k]).collect();
            if kind == green {
                assert!(drawn.iter().zip(blood).all(|(d, b)| (d - b).abs() < 1e-4), "its stain averages its color: {drawn:?}");
                assert_ne!(fx.tint, [1.0; 3], "its splat is tinted");
                assert!(dark.iter().zip(blood).all(|(d, b)| d < &b), "the darker shade is darker");
            } else {
                assert_eq!((stain.3, fx.tint), ([1.0; 3], [1.0; 3]), "the game's own kinds keep the art's red");
            }
            let spit = Spit { kind, pos: h.me().pos, vel: [0.0; 2], damage: 1.0, life: 1.0, age: 0.0 };
            assert_eq!(spit.tint() == [1.0; 3], kind != green, "a spit flies in its kind's blood");
        }
    }

    #[test]
    fn a_ranged_zombie_keeps_its_distance_and_spits_until_cover_stops_it() {
        let mut h = Harvest::calm_on(1, &map::MapSource::new("Field", [20, 14])).unwrap();
        h.me_mut().kit.weapons.clear();
        let spitter = skinny_but(|e| {
            e.ranged = 60.0;
            e.damage = 12.0;
        });
        h.zombies.push(Zombie::new(spitter, [h.me().pos[0] + 100.0, h.me().pos[1]], 1000.0, 1.0, 0.0));
        let mut sounds = Vec::new();
        for _ in 0..60 * 3 {
            h.update(&Input::default(), 1.0 / 60.0, &mut sounds);
            if !h.spits.is_empty() {
                break;
            }
        }
        let gap = len(sub(h.zombies[0].pos, h.me().pos));
        assert!(!h.spits.is_empty() && gap <= 60.0 && gap > 50.0, "it stopped at its range, {gap}, and spat");
        assert!(sounds.iter().any(|s| matches!(s, Sfx::Spit { .. })), "with a sound");
        let before = h.zombies[0].pos;
        for _ in 0..60 * 2 {
            h.update(&Input::default(), 1.0 / 60.0, &mut sounds);
        }
        assert_eq!(h.zombies[0].pos, before, "and holds its ground");
        assert!(h.me().hp < h.me().kit.max_hp(), "the spit landed");
        h.me_mut().hurt = 0.0;
        h.spits = vec![Spit { kind: Kind(0), pos: [h.me().pos[0] - 1.0, h.me().pos[1] - 6.0], vel: [10.0, 0.0], damage: 12.0, life: 1.0, age: 0.0 }];
        let hp = h.me().hp;
        h.hush[6] = 0.0;
        sounds.clear();
        h.move_spits(0.016, &mut sounds, &mut Flow::default());
        assert!(h.spits.is_empty() && h.me().hp == hp - 12.0, "a spit on the player hurts by its damage");
        assert!(sounds.iter().any(|s| matches!(s, Sfx::Splat { .. })), "and splats");
        h.me_mut().hurt = 0.0;
        h.spits = vec![Spit { kind: Kind(0), pos: [h.me().pos[0] - 40.0, h.me().pos[1] - 6.0], vel: [10.0, 0.0], damage: 12.0, life: 1.0, age: 0.0 }];
        h.move_spits(0.016, &mut sounds, &mut Flow::default());
        assert_eq!(h.spits.len(), 1, "one off to the side flies on");

        let mut h = Harvest::calm(6);
        let house = h.map.props.iter().find(|p| p.frames[0] == art::HOUSE).map(|p| [p.x + TILE / 2.0, p.base - TILE / 2.0]).unwrap();
        h.spits = vec![Spit { kind: Kind(0), pos: [house[0], house[1] - 6.0], vel: [0.0; 2], damage: 12.0, life: 1.0, age: 0.0 }];
        h.move_spits(0.016, &mut sounds, &mut Flow::default());
        assert!(h.spits.is_empty(), "cover that stops a bullet stops spit");
        assert!(!clear_shot(&h.map, [house[0] - 2.0 * TILE, house[1]], [house[0] + 2.0 * TILE, house[1]]), "nor spits through a house");
    }

    #[test]
    fn a_burrower_tunnels_unhittable_and_breaks_out_beside_the_player() {
        let mut h = Harvest::calm_on(1, &map::MapSource::new("Field", [20, 14])).unwrap();
        h.me_mut().kit.weapons.clear();
        let mole = skinny_but(|e| e.burrow = 2.0);
        h.tells.push(Tell { kind: mole, pos: [h.me().pos[0] + 90.0, h.me().pos[1]], hp: 1000.0, t: 0.0, leader: false, led: None });
        let mut sounds = Vec::new();
        h.update(&Input::default(), TELL_TIME + 0.01, &mut sounds);
        assert_eq!((h.zombies.len(), h.burrowed.len()), (0, 1), "it breaks ground straight into a tunnel");
        h.explode(h.burrowed[0].pos, 40.0, 500.0, &mut sounds);
        assert_eq!(h.burrowed[0].hp, 1000.0, "nothing reaches it down there");
        for _ in 0..60 * 6 {
            sounds.clear();
            h.update(&Input::default(), 1.0 / 60.0, &mut sounds);
            if h.burrowed[0].rise > 0.0 {
                break;
            }
        }
        assert!(h.burrowed[0].rise > 0.0, "it breaks ground");
        assert!(h.fx.iter().any(|f| f.frames == effects::effects().mound_breaks.frames), "in a puff of dirt");
        assert!(sounds.iter().any(|s| matches!(s, Sfx::Rise { .. })), "with a sound to warn of it");
        assert!(len(sub(h.burrowed[0].pos, h.me().pos)) <= crate::tuning::tuning().burrow_reach, "beside the player");
        h.explode(h.burrowed[0].pos, 40.0, 500.0, &mut sounds);
        assert_eq!(h.burrowed[0].hp, 1000.0, "nothing reaches it while it climbs out");
        for _ in 0..60 * 2 {
            h.update(&Input::default(), 1.0 / 60.0, &mut sounds);
        }
        assert_eq!((h.zombies.len(), h.burrowed.len()), (1, 0), "then it stands");
        assert_eq!(h.zombies[0].rise, 1.0);
        h.zombies[0].pos = [h.me().pos[0] + 80.0, h.me().pos[1]];
        h.zombies[0].hp = 1000.0;
        h.update(&Input::default(), 1.0 / 60.0, &mut sounds);
        assert_eq!(h.zombies.len(), 1, "it stays up a while");
        h.zombies[0].up = 1.99;
        for _ in 0..5 {
            h.update(&Input::default(), 1.0 / 60.0, &mut sounds);
        }
        assert_eq!((h.zombies.len(), h.burrowed.len()), (0, 1), "then digs back under");
        assert_eq!(h.burrowed[0].rise, 0.0, "to tunnel, not to climb straight back out");
    }

    #[test]
    fn the_games_own_data_must_be_whole_and_name_its_sprites() {
        for file in TABLES {
            assert_eq!(check_own_table(file, built_in_table(file).unwrap()), Ok(()), "{file}");
        }
        assert!(check_table("tuning.txt", "[player]\nhp 120").is_ok(), "a mod sets only what it changes");
        assert!(check_own_table("tuning.txt", "[player]\nhp 120").unwrap_err().contains("doesn't set"));
        let sprite = built_in_table("effects.txt").unwrap().replacen("spark SHOT_SPARK", "spark NOPE", 1);
        assert!(check_table("effects.txt", &sprite).is_ok() && check_own_table("effects.txt", &sprite).is_err(), "the game's own shots draw its own sprites");
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
        let farm = format!("{}\n[fields]\ncrops s 1", built_in_table("farm.txt").unwrap());
        assert!(check_table("farm.txt", &farm).is_ok(), "a mod's crops may be its own ground's");
        assert_eq!(check_own_table("farm.txt", &farm), Err("farm.txt: crops s: not a ground code with cell field and a crop line in ground.txt".into()));
        let farm = format!("{}\n[windmill]\nmill PROP_MINE", built_in_table("farm.txt").unwrap());
        assert!(check_table("farm.txt", &farm).is_ok() && check_own_table("farm.txt", &farm).is_err(), "only a mod's windmill can be its prop");
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
        h.me_mut().view_yaw = 0.0;
        let start = h.me().pos;
        run(&mut h, &Input { held: Buttons::UP, ..Default::default() }, 0.5);
        assert!(h.me().pos[0] > start[0] && (h.me().pos[1] - start[1]).abs() < 1.0);
        run(&mut h, &Input { held: Buttons::UP, ..Default::default() }, 120.0);
        assert!(h.me().pos[0] < h.map.width_px() - TILE);
    }

    #[test]
    fn y_and_a_turn_without_moving_and_the_shoulder_triggers_do_not() {
        let mut h = Harvest::calm(9);
        run(&mut h, &Input::default(), 0.05);
        let (start, yaw) = (h.me().pos, h.me().view_yaw);
        run(&mut h, &Input { held: Buttons::L2 | Buttons::R2, ..Default::default() }, 0.25);
        assert_eq!(h.me().view_yaw, yaw);
        run(&mut h, &Input { held: Buttons::A, ..Default::default() }, 0.25);
        assert_eq!(h.me().pos, start);
        assert!((h.me().view_yaw - yaw).rem_euclid(TAU) > 0.5);
        run(&mut h, &Input { held: Buttons::Y, ..Default::default() }, 0.25);
        assert!((h.me().view_yaw - yaw).rem_euclid(TAU) < 0.01 || (h.me().view_yaw - yaw).rem_euclid(TAU) > TAU - 0.01);
    }

    #[test]
    fn the_a_that_starts_a_run_does_not_turn() {
        let mut h = Harvest::calm(9);
        let yaw = h.me().view_yaw.rem_euclid(TAU);
        run(&mut h, &Input { held: Buttons::A, ..Default::default() }, 0.2);
        assert_eq!(h.me().view_yaw, yaw);
        run(&mut h, &Input::default(), 0.05);
        run(&mut h, &Input { held: Buttons::A, ..Default::default() }, 0.2);
        assert_ne!(h.me().view_yaw, yaw);
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
        press(&mut h, Buttons::DOWN);
        press(&mut h, Buttons::A);
        assert!(h.menu.is_none() && h.t == 0.0, "New farm starts over");
        press(&mut h, Buttons::START);
        let tap = Input { touch: Some([500.0, CHOICE_TOP + PAUSE_ROW * 4.5]), touch_started: true, ..Default::default() };
        assert!(h.update(&tap, 1.0 / 60.0, &mut Vec::new()).quit, "a tap on the last choice quits");
    }

    #[test]
    fn b_turns_all_the_way_around_quickly() {
        let mut h = Harvest::calm(9);
        let yaw = h.me().view_yaw;
        h.update(&Input { pressed: Buttons::B, ..Default::default() }, 1.0 / 60.0, &mut Vec::new());
        run(&mut h, &Input::default(), SPIN_TIME * 0.8);
        assert_eq!(h.showing(), h.me().turned_from, "the player keeps the old heading while the camera swings");
        run(&mut h, &Input::default(), SPIN_TIME * 0.2 + 0.05);
        assert!(((h.me().view_yaw - yaw).rem_euclid(TAU) - PI).abs() < 1e-3, "the camera swings right round");
        assert_eq!(h.showing(), h.me().aim, "and turns to match as it lands");
    }

    #[test]
    fn left_and_right_strafe_across_the_heading() {
        let mut h = Harvest::calm(9);
        h.me_mut().view_yaw = -PI / 2.0;
        let start = h.me().pos;
        run(&mut h, &Input { held: Buttons::RIGHT, ..Default::default() }, 0.3);
        assert!(h.me().pos[0] > start[0] && (h.me().pos[1] - start[1]).abs() < 1.0);
    }

    #[test]
    fn the_player_wears_a_grenade_only_while_one_is_ready_and_it_flashes_as_one_comes_ready() {
        let mut h = Harvest::calm(12);
        let mut sounds = Vec::new();
        assert!(h.me().grenades >= 1.0);
        let look = &players::look(&h.me().look).look;
        // The look may hide it on some facings; this one shows it.
        let Some((facing, at)) = [Facing::Front, Facing::Side, Facing::Back].into_iter().find_map(|f| Some((f, look.walk[f.index()][0].worn_at()?))) else { return };
        assert_eq!(player_worn(&h, ME, facing), Some((at, 1.0)));
        h.me_mut().grenades = 0.5;
        assert_eq!(player_worn(&h, ME, facing), None, "none ready, none worn");
        h.update(&Input::default(), crate::tuning::tuning().grenade_recharge * h.me().kit.mods.recharge, &mut sounds);
        assert_eq!(player_worn(&h, ME, facing).map(|w| w.1), Some(READY_BRIGHT), "it flashes as it comes ready");
        h.update(&Input::default(), READY_FLASH, &mut sounds);
        assert_eq!(player_worn(&h, ME, facing).map(|w| w.1), Some(1.0));
        h.me_mut().hurt = 0.5;
        assert!(blinking(&h, ME) && player_worn(&h, ME, facing).is_none(), "it blinks with the player");
        h.me_mut().hurt = 0.0;
        h.phase = Phase::Over { t: 0.0, won: false };
        assert_eq!(player_worn(&h, ME, facing), None, "the dead wear nothing");
    }

    #[test]
    fn the_worn_grenade_centers_on_its_pixel_and_mirrors_with_the_frame() {
        let g = art::GRENADE_WORN;
        assert_eq!((g.w(), g.h()), (3.0, 4.0));
        let farmer = art::PLAYER_FRONT[0];
        assert_eq!(farmer.w(), 16.0);
        assert_eq!(worn_corner([11, 10], farmer, false), [10.0, 8.0]);
        assert_eq!(worn_corner([11, 10], farmer, true), [3.0, 8.0], "pixel 11 of 16 mirrors to pixel 4");
    }

    #[test]
    fn a_tap_on_the_map_throws_a_grenade_that_lands_there() {
        let mut h = Harvest::calm(12);
        let cam = h.camera();
        let target = [h.me().pos[0] + 60.0, h.me().pos[1]];
        h.zombies.push(Zombie::new(Kind::named("skinny"), target, 20.0, 1.0, 0.0));
        let touch = [(target[0] - cam[0]) * map_scale(), (target[1] - cam[1]) * map_scale()];
        let mut sounds = Vec::new();
        h.update(&Input { touch: Some(touch), touch_started: true, ..Default::default() }, 0.016, &mut sounds);
        assert_eq!(h.thrown.len(), 1);
        assert!(h.me().grenades < h.me().kit.grenades());
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
        let before = h.me().kit.level(book().index("pistol").unwrap());
        h.update(&tap, 0.016, &mut Vec::new());
        assert_eq!(h.me().kit.level(book().index("pistol").unwrap()), before + 1);
        h.update(&Input::default(), PICK_TIME, &mut Vec::new());
        assert!(matches!(h.phase, Phase::Playing));
    }

    #[test]
    fn a_held_powerup_comes_up_held_weight_times_as_often() {
        let mut h = Harvest::new(2);
        h.held_weight = 3.0;
        let held: Vec<usize> = (0..book().defs.len()).filter(|&i| h.me().kit.level(i) > 0 && h.me().kit.offerable(i)).collect();
        let new: Vec<usize> = (0..book().defs.len()).filter(|&i| h.me().kit.level(i) == 0 && h.progress.owns(i) && h.me().kit.offerable(i)).collect();
        assert!(!held.is_empty() && !new.is_empty());
        let (mut on_held, mut on_new) = (0.0, 0.0);
        for _ in 0..20000 {
            let up = h.deal(&[], &[]).up;
            if held.contains(&up) { on_held += 1.0 } else { on_new += 1.0 }
        }
        let ratio = (on_held / held.len() as f32) / (on_new / new.len() as f32);
        assert!((ratio - 3.0).abs() < 0.45, "held cards came up {ratio} times as often, for a weight of 3");
    }

    fn level_up_on(h: &mut Harvest, ids: [&str; 3]) {
        let choice = |id| Choice { up: book().index(id).unwrap(), rare: false, found: false };
        h.phase = Phase::LevelUp { choices: ids.map(choice), selected: 1, t: 1.0, picked: None };
    }

    fn shown(h: &Harvest) -> [usize; 3] {
        let Phase::LevelUp { choices, .. } = h.phase else { panic!("no level-up") };
        choices.map(|c| c.up)
    }

    #[test]
    fn l1_banishes_the_selected_card_for_the_run_and_deals_its_place_again() {
        let mut h = Harvest::new(2);
        h.banishes = 1;
        level_up_on(&mut h, ["shotgun", "pistol", "running_shoes"]);
        let pistol = book().index("pistol").unwrap();
        let mut sounds = Vec::new();
        h.update(&Input { pressed: Buttons::L1, ..Default::default() }, 0.016, &mut sounds);
        let now = shown(&h);
        assert!(now[1] != pistol && now[0] != now[1] && now[2] != now[1], "{now:?}");
        assert_eq!((now[0], now[2]), (book().index("shotgun").unwrap(), book().index("running_shoes").unwrap()), "the others stay");
        assert!(matches!(h.phase, Phase::LevelUp { picked: None, .. }), "a banish isn't a pick");
        assert_eq!(h.banishes, 0);
        assert!((0..300).all(|_| h.roll_choices(&[]).iter().all(|c| c.up != pistol)), "gone for the run");
        h.update(&Input::default(), CARDS_IN, &mut sounds);
        sounds.clear();
        h.update(&Input { pressed: Buttons::L1, ..Default::default() }, 0.016, &mut sounds);
        assert_eq!(shown(&h), now, "none left");
        assert!(sounds.contains(&Sfx::Miss));
    }

    #[test]
    fn a_filler_cannot_be_banished() {
        let mut h = Harvest::new(2);
        h.banishes = 3;
        let snack = (0..book().defs.len()).find(|&i| book().defs[i].kind == Card::Filler).unwrap();
        level_up_on(&mut h, ["shotgun", book().defs[snack].id.as_str(), "running_shoes"]);
        let mut sounds = Vec::new();
        h.update(&Input { pressed: Buttons::L1, ..Default::default() }, 0.016, &mut sounds);
        assert_eq!((shown(&h)[1], h.banishes, h.banished), (snack, 3, 0));
        assert!(sounds.contains(&Sfx::Miss));
    }

    #[test]
    fn r1_deals_three_new_cards_and_they_slide_in_before_taking_input() {
        let mut h = Harvest::new(2);
        h.rerolls = 1;
        level_up_on(&mut h, ["shotgun", "pistol", "running_shoes"]);
        let before = shown(&h);
        h.update(&Input { pressed: Buttons::R1, ..Default::default() }, 0.016, &mut Vec::new());
        let after = shown(&h);
        assert!(after.iter().all(|up| !before.contains(up)), "{before:?} then {after:?}");
        assert_eq!(h.rerolls, 0);
        let level: Vec<u32> = after.iter().map(|&up| h.me().kit.level(up)).collect();
        h.update(&Input { pressed: Buttons::X, ..Default::default() }, 0.016, &mut Vec::new());
        assert!(matches!(h.phase, Phase::LevelUp { picked: None, .. }), "too soon after the deal");
        h.update(&Input::default(), CARDS_IN, &mut Vec::new());
        let mut sounds = Vec::new();
        h.update(&Input { pressed: Buttons::R1, ..Default::default() }, 0.016, &mut sounds);
        assert_eq!(shown(&h), after, "none left");
        assert!(sounds.contains(&Sfx::Miss));
        assert_eq!(after.iter().map(|&up| h.me().kit.level(up)).collect::<Vec<_>>(), level, "nothing picked for the player");
    }

    #[test]
    fn the_banish_and_reroll_keys_take_taps() {
        let mut h = Harvest::new(2);
        (h.banishes, h.rerolls) = (1, 1);
        level_up_on(&mut h, ["shotgun", "pistol", "running_shoes"]);
        let tap = |at: [f32; 2]| Input { touch: Some(at), touch_started: true, ..Default::default() };
        let mid = |key: LevelKey| LEVEL_KEYS.iter().find(|k| k.0 == key).map(|(_, [x, y, w, h])| [x + w / 2.0, y + h / 2.0]).unwrap();
        let card = |i: f32| [500.0, CHOICE_TOP + CHOICE_ROW * (i + 0.5)];
        h.update(&tap(mid(LevelKey::Banish)), 0.016, &mut Vec::new());
        assert!(h.banishing);
        h.update(&tap(mid(LevelKey::Banish)), 0.016, &mut Vec::new());
        assert!(!h.banishing, "a second tap keeps them all");
        h.update(&tap(mid(LevelKey::Banish)), 0.016, &mut Vec::new());
        let shotgun = book().index("shotgun").unwrap();
        h.update(&tap(card(0.0)), 0.016, &mut Vec::new());
        assert!(!h.banishing && h.banished == 1 << shotgun && shown(&h)[0] != shotgun);
        assert!(matches!(h.phase, Phase::LevelUp { picked: None, .. }), "the tap banished instead of taking");
        h.update(&Input::default(), CARDS_IN, &mut Vec::new());
        let before = shown(&h);
        h.update(&tap(mid(LevelKey::Reroll)), 0.016, &mut Vec::new());
        assert!(h.rerolls == 0 && shown(&h) != before);
        h.update(&Input::default(), CARDS_IN, &mut Vec::new());
        h.update(&tap(card(2.0)), 0.016, &mut Vec::new());
        assert!(matches!(h.phase, Phase::LevelUp { picked: Some(_), .. }), "a plain tap still takes a card");
    }

    #[test]
    fn y_favors_and_takes_the_selected_card_only_from_favor_level() {
        let t = crate::tuning::tuning();
        let mut h = Harvest::new(2);
        h.favors = 1;
        h.level = t.favor_level as u32 - 1;
        level_up_on(&mut h, ["shotgun", "pistol", "running_shoes"]);
        let mut sounds = Vec::new();
        h.update(&Input { pressed: Buttons::Y, ..Default::default() }, 0.016, &mut sounds);
        assert!(matches!(h.phase, Phase::LevelUp { picked: None, .. }) && h.favored.is_none() && sounds.contains(&Sfx::Miss), "not before favor_level");
        h.level = t.favor_level as u32;
        let pistol = book().index("pistol").unwrap();
        let was = h.me().kit.level(pistol);
        h.update(&Input { pressed: Buttons::Y, ..Default::default() }, 0.016, &mut Vec::new());
        assert!(matches!(h.phase, Phase::LevelUp { picked: Some(_), .. }), "a favor is a pick");
        assert_eq!((h.favored, h.favors, h.me().kit.level(pistol)), (Some(pistol), 0, was + 1));
    }

    #[test]
    fn another_players_own_powerup_is_never_dealt_nor_found() {
        let crows = book().index("crows").unwrap();
        let mut h = Harvest::new(2).with_progress(Progress { kills: u32::MAX, everything: true, hidden: 1 << crows, ..Default::default() });
        h.chest = true;
        for _ in 0..2000 {
            assert!(h.roll_choices(&[]).iter().all(|c| c.up != crows));
        }
        h.t = 3600.0;
        let found = |h: &mut Harvest, hidden| {
            h.progress = Progress { kills: u32::MAX, best: u32::MAX, hidden, ..Default::default() };
            (0..4000).any(|_| h.find(false) == Some(crows))
        };
        assert!(found(&mut h, 0), "found as a rule");
        assert!(!found(&mut h, 1 << crows));
    }

    #[test]
    fn a_favored_powerup_comes_up_favor_weight_times_as_often() {
        let mut h = Harvest::new(2);
        h.held_weight = 1.0;
        let pool: Vec<usize> = (0..book().defs.len()).filter(|&i| h.progress.owns(i) && h.me().kit.offerable(i)).collect();
        let favored = pool[0];
        h.favored = Some(favored);
        let n = 30000;
        let hits = (0..n).filter(|_| h.deal(&[], &[]).up == favored).count() as f32;
        let w = crate::tuning::tuning().favor_weight;
        let expected = w / (w + pool.len() as f32 - 1.0);
        assert!((hits / n as f32 - expected).abs() < 0.02, "came up {} of the time, for {expected}", hits / n as f32);
    }

    #[test]
    fn the_favor_key_shows_from_favor_level_and_a_tap_on_a_card_takes_and_favors_it() {
        let t = crate::tuning::tuning();
        let mut h = Harvest::new(2);
        h.favors = 1;
        h.level = t.favor_level as u32 - 1;
        level_up_on(&mut h, ["shotgun", "pistol", "running_shoes"]);
        let tap = |at: [f32; 2]| Input { touch: Some(at), touch_started: true, ..Default::default() };
        let mid = LEVEL_KEYS.iter().find(|k| k.0 == LevelKey::Favor).map(|(_, [x, y, w, h])| [x + w / 2.0, y + h / 2.0]).unwrap();
        assert_eq!(level_key(mid, h.level), None, "hidden before favor_level");
        h.level += 1;
        h.update(&tap(mid), 0.016, &mut Vec::new());
        assert!(h.favoring);
        h.update(&tap(mid), 0.016, &mut Vec::new());
        assert!(!h.favoring, "a second tap cancels");
        h.update(&tap(mid), 0.016, &mut Vec::new());
        h.update(&tap([500.0, CHOICE_TOP + CHOICE_ROW * 2.5]), 0.016, &mut Vec::new());
        assert_eq!(h.favored, book().index("running_shoes"));
        assert!(!h.favoring && matches!(h.phase, Phase::LevelUp { picked: Some(_), .. }));
    }

    #[test]
    fn a_reroll_on_a_chest_still_holds_a_find() {
        let mut h = Harvest::new(2).with_progress(Progress { kills: 100, ..Default::default() });
        h.rerolls = 5;
        h.chest = true;
        let choices = h.roll_choices(&[]);
        h.phase = Phase::LevelUp { choices, selected: 0, t: 1.0, picked: None };
        for _ in 0..5 {
            h.update(&Input { pressed: Buttons::R1, ..Default::default() }, 0.016, &mut Vec::new());
            let Phase::LevelUp { choices, .. } = h.phase else { panic!() };
            assert!(choices.iter().any(|c| c.found));
            h.update(&Input::default(), CARDS_IN, &mut Vec::new());
        }
    }

    #[test]
    fn a_shot_barrel_blows_up_nearby_zombies_and_opens_its_cell() {
        let mut h = Harvest::new(6);
        let barrel = h.barrels.first().expect("seed 6 has barrels");
        let c = barrel.center();
        h.zombies.push(Zombie::new(Kind::named("big"), [c[0] + 20.0, c[1]], 50.0, 1.0, 0.0));
        let mut sounds = Vec::new();
        h.bullets.push(Bullet { pos: [c[0], c[1] - 6.0], vel: [1.0, 0.0], damage: 50.0, pierce: 0, life: 1.0, force: 30.0, hit: Vec::new(), look: None, age: 0.0, by: None, owner: ME, lift: [0.0; 2] });
        h.move_bullets(0.016, &mut sounds);
        assert!(h.blow(0.016, &mut sounds).is_some());
        assert!(h.zombies[0].hp <= 0.0);
        assert!(!h.map.solid_at(c[0], c[1]));
    }

    #[test]
    fn a_bullet_throws_its_impact_sparks_where_it_hits_a_zombie_or_runs_out_on_the_ground() {
        let look = effects::effects().named("pistol_shot").expect("the pistol's style");
        let shot = look.shot.unwrap();
        let sparks = |h: &Harvest| h.juice.particles.iter().filter(|p| p.glow > 0.0 && p.color == shot.color).count();
        let mut h = Harvest::calm(4);
        let at = h.me().pos;
        h.zombies.push(Zombie::new(Kind::named("big"), [at[0] + 3.0, at[1]], 500.0, 1.0, 0.0));
        h.bullets = vec![Bullet { pos: [at[0], at[1] - 6.0], vel: [10.0, 0.0], damage: 1.0, pierce: 0, life: 1.0, force: 0.0, hit: Vec::new(), look: Some(look), age: 0.0, by: None, owner: ME, lift: [0.0; 2] }];
        h.juice.particles.clear();
        h.move_bullets(0.016, &mut Vec::new());
        assert!(h.bullets.is_empty());
        assert_eq!(sparks(&h), shot.impact as usize, "sparks beside the blood");
        assert!(h.juice.particles.len() > shot.impact as usize + 1, "and the blood still sprays");
        h.zombies.clear();
        h.juice.particles.clear();
        h.bullets = vec![Bullet { pos: [at[0], at[1] - 6.0], vel: [10.0, 0.0], damage: 1.0, pierce: 0, life: 0.01, force: 0.0, hit: Vec::new(), look: Some(look), age: 0.0, by: None, owner: ME, lift: [0.0; 2] }];
        h.move_bullets(0.016, &mut Vec::new());
        assert_eq!(sparks(&h), shot.impact as usize, "a bullet that flies its range sparks on the ground");
        h.juice.particles.clear();
        let sprite = effects::effects().slash;
        h.bullets = vec![Bullet { pos: [at[0], at[1] - 6.0], vel: [10.0, 0.0], damage: 1.0, pierce: 0, life: 0.01, force: 0.0, hit: Vec::new(), look: Some(sprite), age: 0.0, by: None, owner: ME, lift: [0.0; 2] }];
        h.move_bullets(0.016, &mut Vec::new());
        assert!(h.juice.particles.is_empty(), "a bullet that flies as a sprite throws none");
    }

    #[test]
    fn resuming_with_a_held_doesnt_turn_the_camera() {
        let mut h = Harvest::calm(4);
        h.update(&Input::default(), 1.0 / 60.0, &mut Vec::new());
        h.update(&Input { pressed: Buttons::START, ..Default::default() }, 1.0 / 60.0, &mut Vec::new());
        assert_eq!(h.menu, Some(0));
        h.update(&Input { pressed: Buttons::A, held: Buttons::A, ..Default::default() }, 1.0 / 60.0, &mut Vec::new());
        assert_eq!(h.menu, None);
        let yaw = h.me().view_yaw;
        h.update(&Input { held: Buttons::A, ..Default::default() }, 0.5, &mut Vec::new());
        assert_eq!(h.me().view_yaw, yaw.rem_euclid(TAU), "the A that resumed has to be let go before it turns");
    }

    #[test]
    fn a_piercing_shot_hits_each_zombie_once_while_the_dead_are_cleared() {
        let mut h = penned();
        let at = [h.me().pos[0] + 20.0, h.me().pos[1]];
        let mut dead = Zombie::new(Kind::named("skinny"), [h.me().pos[0] - 40.0, h.me().pos[1]], 0.0, 1.0, 0.0);
        dead.id = 1;
        let mut live = Zombie::new(Kind::named("skinny"), at, 1000.0, 1.0, 0.0);
        live.id = 2;
        h.zombies = vec![dead, live];
        h.bullets = vec![Bullet { pos: [at[0], at[1] - 6.0], vel: [0.0; 2], damage: 10.0, pierce: 5, life: 1.0, force: 0.0, hit: Vec::new(), look: None, age: 0.0, by: None, owner: ME, lift: [0.0; 2] }];
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
            h.bullets = vec![Bullet { pos: [at[0], at[1] - 6.0], vel: [0.0; 2], damage: 1.0, pierce: 0, life: 1.0, force: 0.0, hit: Vec::new(), look: None, age: 0.0, by: None, owner: ME, lift: [0.0; 2] }];
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
            if h.me().hp < h.me().kit.max_hp() {
                break;
            }
        }
        assert!(h.map.props.len() < fence, "a piece of fence is gone");
        assert!(h.me().hp < h.me().kit.max_hp(), "and the zombie came through it");
    }

    #[test]
    fn shots_and_blasts_break_cover_but_fly_past_a_fence() {
        let mut h = penned();
        let post = [7.5 * TILE, 6.5 * TILE];
        let shoot = |h: &mut Harvest| {
            h.bullets = vec![Bullet { pos: [post[0], post[1] - 6.0], vel: [0.0; 2], damage: 10.0, pierce: 0, life: 1.0, force: 0.0, hit: Vec::new(), look: None, age: 0.0, by: None, owner: ME, lift: [0.0; 2] }];
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
        let at = [h.me().pos[0] + 4.0, h.me().pos[1]];
        h.zombies.push(Zombie::new(Kind::named("skinny"), at, 1000.0, 0.0, 0.0));
        let mut sounds = Vec::new();
        h.update(&Input::default(), crate::tuning::tuning().rise * 0.5, &mut sounds);
        assert_eq!(h.zombies[0].pos, at);
        assert_eq!(h.me().hp, h.me().kit.max_hp());
        for _ in 0..60 {
            h.update(&Input::default(), 1.0 / 60.0, &mut sounds);
        }
        assert!(h.me().hp < h.me().kit.max_hp());
    }

    /// A calm farm whose player has just gone down at its spawn, ringed by `n` zombies.
    fn down_among(n: usize) -> Harvest {
        let mut h = Harvest::calm(4);
        h.zombies = (0..n).map(|k| {
            let a = k as f32 * TAU / n as f32;
            let mut z = Zombie::new(Kind::named("skinny"), [h.me().pos[0] + fm::cos(a) * 14.0, h.me().pos[1] + fm::sin(a) * 14.0], 50.0, 1.0, 0.0);
            z.id = k as u32 + 1;
            z
        }).collect();
        h.me_mut().hp = 0.0;
        h.me_mut().down = Some(0.0);
        h.phase = Phase::Over { t: 0.0, won: false };
        h.lose_interest(ME);
        h.juice.tally.shut = true;
        h
    }

    #[test]
    fn after_the_player_turns_the_dead_wander_off_and_never_gather_on_them() {
        let mut h = down_among(12);
        let fell = h.me().pos;
        let near = |h: &Harvest, p: [f32; 2]| h.zombies.iter().map(|z| len(sub(z.pos, p))).sum::<f32>() / h.zombies.len() as f32;
        let before = near(&h, fell);
        let starts: Vec<[f32; 2]> = h.zombies.iter().map(|z| z.pos).collect();
        for _ in 0..(12 * 60) {
            h.update(&Input::default(), 1.0 / 60.0, &mut Vec::new());
        }
        assert!(near(&h, fell) > before + 20.0, "they drift off where the player fell: {before} -> {}", near(&h, fell));
        assert!(near(&h, h.me().pos) > 30.0, "and don't gather on the turned player");
        let moved = h.zombies.iter().zip(&starts).filter(|(z, s)| len(sub(z.pos, **s)) > 16.0).count();
        assert!(moved >= 10, "none stands stuck: {moved} of 12 moved");
        assert!(len(sub(h.me().pos, fell)) > 30.0, "the turned player wanders too");
    }

    #[test]
    fn what_was_in_flight_when_the_run_ended_still_lands_but_the_run_keeps_its_numbers() {
        for won in [false, true] {
            let mut h = down_among(0);
            h.phase = Phase::Over { t: 0.0, won };
            let target = [h.me().pos[0] + 40.0, h.me().pos[1]];
            let mut z = Zombie::new(Kind::named("skinny"), target, 5.0, 1.0, 0.0);
            z.id = 99;
            h.zombies.push(z);
            h.shoot_from([h.me().pos[0] + 20.0, h.me().pos[1] - 6.0], 0.0, 200.0, 50.0, 0, 1.0, 0.0, None, [0.0; 2]);
            h.thrown.push(Grenade { from: h.me().pos, to: [h.me().pos[0] - 60.0, h.me().pos[1]], t: 0.0, kind: Thrown::Grenade, by: None, owner: ME });
            let (kills, total, hp) = (h.kills, h.juice.tally.total(ME), h.me().hp);
            for _ in 0..120 {
                h.update(&Input::default(), 1.0 / 60.0, &mut Vec::new());
            }
            assert!(h.bullets.is_empty() && h.thrown.is_empty(), "won {won}: the shot and the grenade finish");
            assert!(h.zombies.iter().all(|z| z.id != 99), "won {won}: the shot still kills");
            assert_eq!((h.kills, h.juice.tally.total(ME), h.me().hp), (kills, total, hp), "won {won}: no kill, damage or hurt counts");
            assert!(h.drops.is_empty(), "won {won}: nothing drops");
        }
    }

    /// A calm run holding its start weapons among a few zombies too tough to kill, played for `secs`, then ended by
    /// dying and played on until the summary shows.
    fn fought_then_died(secs: f32) -> (Harvest, f32) {
        let mut h = Harvest::calm(3);
        let hp = 1.0e6;
        h.zombies = (0..4).map(|k| {
            let mut z = Zombie::new(Kind::named("big"), [h.me().pos[0] + 16.0, h.me().pos[1] - 6.0 + k as f32 * 4.0], hp, 1.0, 0.0);
            z.id = 100 + k;
            z
        }).collect();
        h.me_mut().hp = 1.0e6;
        for _ in 0..(secs * 60.0) as usize {
            h.update(&Input::default(), 1.0 / 60.0, &mut Vec::new());
        }
        let hurt: f32 = h.zombies.iter().map(|z| hp - z.hp).sum();
        h.me_mut().hp = 0.0;
        h.update(&Input::default(), 1.0 / 60.0, &mut Vec::new());
        h.update(&Input::default(), TURNED + 1.5, &mut Vec::new());
        (h, hurt)
    }

    fn top_text(h: &Harvest) -> Vec<String> {
        let mut f = crate::Frame::default();
        h.draw(&mut f, false);
        f.top.shapes.iter().filter_map(|s| if let crate::Shape::Text { text, .. } = s { Some(text.clone()) } else { None }).collect()
    }

    #[test]
    fn the_end_screen_lists_each_weapon_held_by_damage_then_the_whole_run() {
        let (h, hurt) = fought_then_died(5.0);
        assert!(hurt > 0.0);
        let tally = &h.juice.tally;
        assert!((tally.total(ME) - hurt).abs() < 0.01 * hurt, "every hit counts: {} of {hurt}", tally.total(ME));
        assert!((tally.secs - h.t).abs() < 1e-3 && tally.dps(ME) == tally.total(ME) / tally.secs, "over the night's time");
        let names: Vec<&str> = tally.rows(ME).iter().map(|r| book().defs[r.by.unwrap()].name.as_str()).collect();
        let held: Vec<&str> = h.me().kit.weapons.iter().map(|a| book().defs[a.def].name.as_str()).collect();
        assert_eq!(names.len(), held.len(), "{names:?} {held:?}");
        assert!(held.iter().all(|n| names.contains(n)));
        assert!(tally.rows(ME).windows(2).all(|w| w[0].damage >= w[1].damage), "most damage first");
        let text = top_text(&h);
        let at = |s: &str| text.iter().position(|t| t == s).unwrap_or_else(|| panic!("{s} not on the end screen: {text:?}"));
        let order: Vec<usize> = names.iter().map(|n| at(n)).chain([at("Whole run")]).collect();
        assert!(order.windows(2).all(|w| w[0] < w[1]), "in that order, the whole run last");
    }

    #[test]
    fn the_end_screen_shows_the_build_on_the_top_screen_on_one_screen_and_two() {
        for single in [false, true] {
            let (mut h, _) = fought_then_died(1.0);
            h.single = single;
            let points = book().index("hollow_points").unwrap();
            h.me_mut().kit.raise(points, 2);
            let mut f = crate::Frame::default();
            h.draw(&mut f, false);
            let icons: Vec<Art> = f.top.shapes.iter().filter_map(|s| if let crate::Shape::Art { art, .. } = s { Some(*art) } else { None }).collect();
            for def in h.me().kit.weapons.iter().map(|a| a.def).chain([points]) {
                assert!(icons.contains(&book().defs[def].icon), "{} on the top screen, one screen {single}", book().defs[def].id);
            }
            assert!(top_text(&h).contains(&"Build".to_string()));
        }
    }

    #[test]
    fn a_runs_record_keeps_its_weapons_then_passives_at_their_levels() {
        let mut h = Harvest::calm(3);
        let crows = book().index("crows").unwrap();
        let points = book().index("hollow_points").unwrap();
        h.me_mut().kit.raise(crows, 3);
        h.me_mut().kit.raise(points, 2);
        h.me_mut().hp = 0.0;
        let run = h.update(&Input::default(), 1.0 / 60.0, &mut Vec::new()).ended.expect("the run ends");
        let mut want: Vec<(u16, u8)> = h.me().kit.weapons.iter().map(|a| (a.def as u16, a.level as u8)).collect();
        want.push((points as u16, 2));
        assert!(want.contains(&(crows as u16, 3)));
        assert_eq!(run.kit.iter().flatten().copied().collect::<Vec<_>>(), want);
    }

    #[test]
    fn a_new_run_starts_its_tally_over() {
        let (mut h, _) = fought_then_died(1.0);
        h.update(&Input { pressed: Buttons::A, ..Default::default() }, 1.0 / 60.0, &mut Vec::new());
        assert!(matches!(h.phase, Phase::Playing));
        assert_eq!((h.juice.tally.total(ME), h.juice.tally.secs), (0.0, 0.0));
    }

    #[test]
    fn a_map_smaller_than_the_view_draws_and_takes_a_tap() {
        let mut h = Harvest::calm_on(1, &MapSource::new("Tiny", MAP_MIN)).unwrap();
        h.draw(&mut crate::Frame::default(), false);
        h.throw([SCREEN_W / 2.0, SCREEN_H / 2.0], &mut Vec::new());
        assert_eq!(h.camera(), [0.0; 2]);
    }

    #[test]
    fn dying_turns_the_player_then_offers_a_new_farm_at_the_same_difficulty() {
        let mut h = Harvest::new(4).at(Difficulty::Hard).without_enemies(0b10).looking_like("");
        let mut sounds = Vec::new();
        h.me_mut().hp = 0.0;
        h.update(&Input::default(), 0.016, &mut sounds);
        assert!(matches!(h.phase, Phase::Over { won: false, .. }));
        let a = Input { pressed: Buttons::A, ..Default::default() };
        h.update(&Input::default(), TURN_SINK, &mut sounds);
        assert_eq!(player_pose(&h, ME, Facing::Front).1, 0.0, "under the dirt");
        h.update(&a, 0.016, &mut sounds);
        assert!(matches!(h.phase, Phase::Over { .. }), "too soon to start over");
        h.update(&Input::default(), TURN_RISE - TURN_SINK, &mut sounds);
        assert!(sounds.contains(&Sfx::Turn));
        h.update(&Input::default(), TURNED - TURN_RISE + 0.7, &mut sounds);
        let (art, rise) = player_pose(&h, ME, Facing::Front);
        assert_eq!(rise, 1.0);
        assert!(art::SKINNY_FRONT.contains(&art), "back up as one of them");
        h.update(&a, 0.016, &mut sounds);
        assert!(matches!(h.phase, Phase::Playing));
        assert_eq!(h.kills, 0);
        assert_eq!(h.difficulty, Difficulty::Hard);
        assert_eq!(h.me().kit.weapons.len(), 2, "and its kit");
        assert_eq!(h.enemies_off, 0b10, "the kinds turned off stay off");
        assert_eq!(h.me().look, "", "and the player keeps their look");
    }

    #[test]
    fn every_difficulty_starts_with_a_pistol_and_knife_and_easy_with_two_grenades() {
        for (d, grenades) in [(Difficulty::Easy, 2.0), (Difficulty::Normal, 1.0), (Difficulty::Hard, 1.0)] {
            let h = Harvest::new(1).at(d).with_kit("lantern");
            assert_eq!(h.me().kit.weapons.len(), 3, "{d:?}");
            assert_eq!((h.me().grenades, h.me().kit.grenades()), (grenades, grenades), "{d:?}");
        }
    }

    #[test]
    fn zombies_bite_a_player_tucked_into_the_edge_rings_open_part() {
        let t = crate::tuning::tuning();
        let (ph, zh) = (t.player_half(), t.zombie_half());
        for (players, hider) in [(1, ME), (2, 1)] {
            let mut h = Harvest::calm(3).with_players(players);
            h.birds.clear();
            for p in &mut h.players {
                p.kit.weapons.clear();
            }
            let walk_in = |y: f32| (0..40).fold([2.5 * TILE, y], |p, _| h.map.slide(p, [-1.0, 0.0], ph));
            let start = (2..h.map.h - 2).flat_map(|ty| (0..16).map(move |k| ty as f32 * TILE + k as f32)).find(|&y| !h.map.hits(2.5 * TILE, y, ph) && walk_in(y)[0] < 8.0);
            let at = walk_in(start.expect("a way into the edge ring"));
            assert!(h.map.hits_cells(at[0], at[1], zh), "{at:?} is somewhere whole cells keep zombies out of");
            h.players[hider].pos = at;
            if players > 1 {
                assert!(len(sub(h.players[ME].pos, at)) > 100.0, "the other player is far off");
            }
            let z = (3..8).map(|k| [k as f32 * 0.5 * TILE + TILE, at[1]]).find(|q| !h.map.hits_cells(q[0], q[1], zh)).unwrap();
            h.zombies.push(Zombie::new(Kind::named("skinny"), z, 1000.0, 1.0, 0.0));
            let mut sounds = Vec::new();
            for _ in 0..180 {
                h.update(&Input::default(), 1.0 / 60.0, &mut sounds);
            }
            assert!(h.players[hider].hp < h.players[hider].kit.max_hp(), "{players} players: unbitten at {at:?}, the zombie at {:?}", h.zombies[0].pos);
        }
    }

    #[test]
    fn harder_difficulties_hit_harder_and_drop_less_health() {
        let bite = |d: Difficulty| {
            let mut h = Harvest::calm(3).at(d);
            h.zombies.push(Zombie::new(Kind::named("skinny"), [h.me().pos[0] + 3.0, h.me().pos[1]], 1000.0, 1.0, 0.0));
            h.update(&Input::default(), 1.0 / 60.0, &mut Vec::new());
            h.me().kit.max_hp() - h.me().hp
        };
        let [easy, normal, hard] = Difficulty::ALL.map(bite);
        assert!(easy < normal && normal < hard, "{easy} {normal} {hard}");
        assert!(Difficulty::Easy.health_drop() < Difficulty::Hard.health_drop());
    }

    #[test]
    fn a_kill_drops_a_body_a_number_and_money_that_hops() {
        let mut h = Harvest::calm(3);
        h.me_mut().kit.raise(book().index("pistol").unwrap(), 1);
        let at = [-30.0, 30.0, 0.0].into_iter().flat_map(|dx| [-30.0, 30.0].map(|dy| [h.me().pos[0] + dx, h.me().pos[1] + dy])).find(|p| !h.map.hits(p[0], p[1], crate::tuning::tuning().zombie_half())).unwrap();
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

    /// A calm run on an open field 80 by 60 tiles, the player walking east from its middle.
    fn open_field() -> Harvest {
        let mut h = Harvest::calm_on(1, &map::MapSource::new("Field", [80, 60])).unwrap();
        h.me_mut().pos = [640.0, 480.0];
        h.me_mut().heading = [1.0, 0.0];
        h.t = 10.0;
        h
    }

    fn standing(kind: &str, pos: [f32; 2], hp: f32) -> Zombie {
        Zombie::new(Kind::named(kind), pos, hp, 1.0, 0.0)
    }

    #[test]
    fn a_straggler_left_far_off_screen_rises_again_ahead_of_the_player_unchanged() {
        let mut h = open_field();
        let t = crate::tuning::tuning();
        let mut z = standing("big", [h.me().pos[0] - 400.0, h.me().pos[1]], 321.0);
        z.leader = true;
        h.zombies.push(z);
        h.recycle(t.straggler_time - 0.1);
        assert_eq!((h.zombies.len(), h.tells.len()), (1, 0), "not before its time is up");
        h.recycle(0.2);
        assert_eq!((h.zombies.len(), h.tells.len()), (0, 1), "then it goes back in the ground");
        let tell = &h.tells[0];
        assert_eq!((tell.kind, tell.hp, tell.leader), (Kind::named("big"), 321.0, true), "the same zombie, no boost");
        let off = sub(tell.pos, h.me().pos);
        let [near, far] = spawn_ring(h.t);
        assert!((near - 0.01..=far + 0.01).contains(&len(off)), "on the ring, {}", len(off));
        assert!(off[0] / len(off) >= fm::cos(FRAC_PI_4) - 1e-3, "within a quarter turn of the way the player walks, {off:?}");
    }

    #[test]
    fn a_zombie_on_screen_near_or_a_harvester_is_never_recycled() {
        let mut h = open_field();
        let t = crate::tuning::tuning();
        let sight = (view3d::VIEW_RADIUS + view3d::CAMERA_BACK) * TILE;
        assert!(t.straggler_reach >= sight, "the straggler distance reaches past the view ahead");
        h.me_mut().pos = [8.0, 480.0];
        let on_map = [h.me().pos[0] + t.straggler_reach + 5.0, h.me().pos[1]];
        assert!(on_map[0] < view_w() && h.on_screen(on_map), "the map, pinned to the field's edge, still shows it");
        h.zombies.push(standing("skinny", on_map, 10.0));
        let mut boss = standing("big", [h.me().pos[0], h.me().pos[1] + 420.0], 2500.0);
        boss.boss = true;
        h.zombies.push(boss);
        h.zombies.push(standing("skinny", [h.me().pos[0] + 20.0, h.me().pos[1]], 10.0));
        for _ in 0..10 {
            h.recycle(t.straggler_time);
        }
        assert_eq!((h.zombies.len(), h.tells.len()), (3, 0));
    }

    #[test]
    fn a_straggler_that_comes_back_into_view_starts_its_time_again() {
        let mut h = open_field();
        let t = crate::tuning::tuning();
        h.zombies.push(standing("skinny", [h.me().pos[0] - 400.0, h.me().pos[1]], 10.0));
        h.recycle(t.straggler_time * 0.9);
        h.zombies[0].pos = [h.me().pos[0] - 100.0, h.me().pos[1]];
        h.recycle(0.01);
        h.zombies[0].pos = [h.me().pos[0] - 400.0, h.me().pos[1]];
        h.recycle(t.straggler_time * 0.9);
        assert_eq!((h.zombies.len(), h.tells.len()), (1, 0));
    }

    #[test]
    fn spawns_owed_at_the_cap_bank_and_rise_a_few_a_second_once_there_is_room() {
        let mut h = open_field();
        let t = crate::tuning::tuning();
        let max = t.max_zombies as usize;
        h.zombies = (0..max).map(|_| standing("skinny", h.me().pos, 10.0)).collect();
        h.owed = t.bank + 25.0;
        h.spawn(0.0);
        assert!(h.tells.is_empty(), "no room");
        assert_eq!(h.banked, t.bank, "the bank holds only so many");
        assert!(h.owed < 1.0);
        h.zombies.truncate(max - t.bank as usize);
        let dt = 1.0 / 60.0;
        let mut risen = Vec::new();
        for _ in 0..120 {
            h.owed = 0.0;
            h.spawn(dt);
            risen.push(h.tells.len());
        }
        let release = t.bank_release;
        assert!(risen[0] <= 2, "room opening doesn't raise the bank at once, {}", risen[0]);
        assert!((risen[119] as f32 - release * 2.0).abs() <= 1.0, "{release} a second, {} in two", risen[119]);
        assert_eq!(h.banked, t.bank - risen[119] as f32);
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
        h.zombies.push(Zombie::new(Kind::named("skinny"), [h.me().pos[0] + 3.0, h.me().pos[1]], 1000.0, 1.0, 0.0));
        let mut sounds = Vec::new();
        h.update(&Input::default(), 1.0 / 60.0, &mut sounds);
        assert!(h.me().hp < h.me().kit.max_hp() && h.juice.frozen());
        let t = h.t;
        h.update(&Input::default(), 1.0 / 60.0, &mut sounds);
        assert_eq!(h.t, t);
    }

    #[test]
    fn a_shot_scares_the_crows_off() {
        let mut h = Harvest::calm(5);
        h.birds = vec![Bird { pos: [h.me().pos[0] + 80.0, h.me().pos[1]], z: 0.0, vel: [0.0; 2], vz: 0.0, flying: false, anim: 0.0, flip: false }];
        let mut sounds = Vec::new();
        h.flock(0.016, &mut sounds);
        assert!(!h.birds[0].flying);
        h.noise = Some(h.me().pos);
        h.flock(0.016, &mut sounds);
        assert!(h.birds[0].flying);
        assert!(sounds.iter().any(|s| matches!(s, Sfx::Caw { .. })));
    }

    #[test]
    fn quick_pickups_climb_the_scale() {
        let mut h = Harvest::calm(5);
        let mut sounds = Vec::new();
        for _ in 0..3 {
            h.drops.push(Drop { kind: DropKind::Money(1), pos: h.me().pos, z: 0.0, vz: 0.0, vel: [0.0; 2], age: 0.0, pulled: None, to: ME, dog: None });
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
        h.drops.push(Drop { kind: DropKind::Chest, pos: h.me().pos, z: 0.0, vz: 0.0, vel: [0.0; 2], age: 0.0, pulled: None, to: ME, dog: None });
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
            h.drops.push(Drop { kind: DropKind::Chest, pos: h.me().pos, z: 0.0, vz: 0.0, vel: [0.0; 2], age: 0.0, pulled: None, to: ME, dog: None });
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
        h.me_mut().kit.weapons.clear();
        h.zombies.clear();
        h.tells.clear();
        h.t = dawn() - 0.01;
        h.me_mut().pos = h.map.spawn;
        h
    }

    #[test]
    fn dawn_wakes_the_harvester_instead_of_ending_the_night() {
        let mut h = before_dawn();
        let flow = h.update(&Input::default(), 0.02, &mut Vec::new());
        assert!(flow.ended.is_none() && matches!(h.phase, Phase::Playing), "the night goes on");
        let boss = h.zombies.iter().find(|z| z.boss).expect("the Harvester rises");
        let (to_home, to_boss) = (sub(h.map.home, h.me().pos), sub(boss.pos, h.me().pos));
        if len(to_home) > 1.0 {
            assert!(to_home[0] * to_boss[0] + to_home[1] * to_boss[1] <= 0.0, "it rises on the side away from home");
        }
    }

    #[test]
    fn once_the_harvester_falls_its_waves_rise_one_bigger_each_time() {
        let t = crate::tuning::tuning();
        let bosses = |h: &Harvest| h.zombies.iter().filter(|z| z.boss).count();
        let wait = |h: &mut Harvest, secs: f32| {
            for _ in 0..(secs / 0.5).round() as usize {
                h.harvester_waves(0.5, &mut Vec::new(), &mut Flow::default());
            }
        };
        let mut h = before_dawn();
        h.update(&Input::default(), 0.02, &mut Vec::new());
        wait(&mut h, t.harvester_again * 3.0);
        assert_eq!(bosses(&h), 1, "no wave while the first lives");
        h.zombies.iter_mut().filter(|z| z.boss).for_each(|z| z.hp = 0.0);
        h.reap(&mut Vec::new(), &mut Flow::default());
        assert_eq!(bosses(&h), 0);
        wait(&mut h, t.harvester_again - 1.0);
        assert_eq!(bosses(&h), 0, "not before `again`");
        wait(&mut h, 1.0);
        assert_eq!(bosses(&h), 2);
        let spots: Vec<[f32; 2]> = h.zombies.iter().filter(|z| z.boss).map(|z| z.pos).collect();
        assert!(len(sub(spots[0], spots[1])) > 10.0, "a wave spreads out");
        wait(&mut h, t.harvester_again);
        assert_eq!(bosses(&h), 2 + 3, "the next comes whether or not the last is dead");
        h.zombies.retain(|z| !z.boss);
        h.fury = 2.0;
        wait(&mut h, t.harvester_again * 10.0);
        let most = t.harvester_most as usize;
        let waves = (4..14).map(|n| n.min(most)).sum::<usize>();
        assert_eq!(bosses(&h), waves, "no wave above `most`");
        assert!(h.zombies.iter().filter(|z| z.boss).all(|z| z.hp >= t.harvester_hp * 2.0 - 0.01), "fury grows them");
    }

    #[test]
    fn harvester_waves_stop_when_the_run_ends() {
        let mut h = before_dawn();
        h.update(&Input::default(), 0.02, &mut Vec::new());
        h.zombies.iter_mut().filter(|z| z.boss).for_each(|z| z.hp = 0.0);
        h.update(&Input::default(), 0.02, &mut Vec::new());
        assert!(h.waves.is_some());
        h.zombies.clear();
        h.me_mut().pos = h.map.home;
        // The Harvester's fall holds the world still a moment first.
        for _ in 0..50 {
            h.update(&Input::default(), 0.02, &mut Vec::new());
        }
        assert!(matches!(h.phase, Phase::Over { won: true, .. }));
        for _ in 0..(crate::tuning::tuning().harvester_again * 3.0 / 0.1) as usize {
            h.update(&Input::default(), 0.1, &mut Vec::new());
        }
        assert_eq!(h.zombies.iter().filter(|z| z.boss).count(), 0, "home shuts the door on them");
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
        h.zombies.push(Zombie::new(Kind::named("skinny"), [h.me().pos[0] + 200.0, h.me().pos[1]], 100.0, 1.0, 0.0));
        for _ in 0..10 {
            h.grow_fury(1.0);
        }
        assert!((h.fury - fm::powi(1.01f32, 10)).abs() < 1e-4);
        let hp = h.zombies.last().unwrap().hp;
        assert!((hp - 100.0 * fm::powi(1.01f32, 10)).abs() < 0.01, "{hp}");
        let bitten = |h: &mut Harvest| {
            h.zombies.retain(|z| !z.boss);
            h.zombies[0].pos = h.me().pos;
            h.me_mut().hurt = 0.0;
            let before = h.me().hp;
            h.contact(&mut Vec::new(), &mut Flow::default());
            before - h.me().hp
        };
        let late = bitten(&mut h);
        h.fury = 1.0;
        assert!(late > bitten(&mut h) * 1.09, "bites grow too");
    }

    #[test]
    fn only_the_farmhouse_step_after_dawn_ends_the_night() {
        let mut h = before_dawn();
        h.t = dawn() - 5.0;
        h.me_mut().pos = h.map.home;
        h.update(&Input::default(), 0.02, &mut Vec::new());
        assert!(matches!(h.phase, Phase::Playing), "home before dawn is just a house");
        let mut h = before_dawn();
        h.update(&Input::default(), 0.02, &mut Vec::new());
        h.zombies.clear();
        h.me_mut().pos = h.map.home;
        let flow = h.update(&Input::default(), 0.02, &mut Vec::new());
        assert!(matches!(h.phase, Phase::Over { won: true, .. }));
        assert!(flow.ended.is_some_and(|r| r.won));
    }

    /// Where the bottom map draws a zombie of `kind` standing up, as world spots.
    fn on_map(h: &Harvest, kind: Kind) -> Vec<[f32; 2]> {
        let look = &kind.stats().look;
        let frames: Vec<Art> = look.walk.iter().flatten().chain(&look.flash).chain(&look.hurt).copied().collect();
        let mut f = crate::Frame::default();
        h.draw(&mut f, false);
        let cam = h.camera();
        f.bottom.shapes.iter().filter_map(|s| match *s {
            crate::Shape::Art { art: a, x, y, scale, .. } if frames.contains(&a) => Some([x / scale + cam[0] + a.w() / 2.0, y / scale + cam[1] + a.h() - 2.0]),
            _ => None,
        }).collect()
    }

    #[test]
    fn once_dead_the_map_shows_the_horde_walking_where_it_is() {
        let mut h = Harvest::calm(4);
        h.zombies.clear();
        h.me_mut().view_yaw = 0.0;
        h.me_mut().aim = [1.0, 0.0];
        let mut z = Zombie::new(Kind::named("skinny"), [h.me().pos[0] - 40.0, h.me().pos[1]], 50.0, 1.0, 0.0);
        z.id = 9;
        h.zombies.push(z);
        h.update(&Input::default(), 1.0 / 60.0, &mut Vec::new());
        assert!(on_map(&h, Kind::named("skinny")).is_empty(), "alive, the map shows only what the view has seen");
        h.me_mut().hp = 0.0;
        h.update(&Input::default(), 1.0 / 60.0, &mut Vec::new());
        assert!(matches!(h.phase, Phase::Over { won: false, .. }));
        let mut seen = Vec::new();
        for _ in 0..3 {
            for _ in 0..((TURNED / 2.0) * 60.0) as usize {
                h.update(&Input::default(), 1.0 / 60.0, &mut Vec::new());
            }
            let z = &h.zombies[0];
            let at = on_map(&h, z.kind);
            assert!(at.iter().any(|p| len(sub(*p, z.pos)) < 0.5), "the map draws the zombie behind the dead player where it is: {at:?} vs {:?}", z.pos);
            seen.push(z.pos);
        }
        assert!(seen[0] != seen[2], "and it walks");
    }

    #[test]
    fn home_at_dawn_leaves_no_zombie_on_the_map() {
        let mut h = before_dawn();
        h.update(&Input::default(), 0.02, &mut Vec::new());
        h.me_mut().pos = h.map.home;
        h.me_mut().view_yaw = 0.0;
        let mut z = Zombie::new(Kind::named("skinny"), [h.me().pos[0] + 12.0, h.me().pos[1]], 50.0, 1.0, 0.0);
        z.id = 9;
        let kinds: Vec<Kind> = h.zombies.iter().chain([&z]).map(|z| z.kind).collect();
        h.zombies.push(z);
        h.update(&Input::default(), 0.02, &mut Vec::new());
        assert!(matches!(h.phase, Phase::Over { won: true, .. }));
        h.update(&Input::default(), 0.5, &mut Vec::new());
        for kind in kinds {
            assert!(on_map(&h, kind).is_empty(), "first light burned every zombie away");
        }
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
        let at = [h.me().pos[0] + 60.0, h.me().pos[1]];
        h.zombies.push(big);
        h.zombies[0].pos = at;
        h.move_zombies(0.1, &mut Vec::new());
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

    /// A calm farm with the truck's driver at the player's feet and coins worth `each` strewn far from them.
    fn collection_day(each: u32) -> Harvest {
        let mut h = Harvest::calm(5);
        h.map.driver = Some(h.me().pos);
        for (dx, dy) in [(200.0, 0.0), (-200.0, 40.0), (0.0, 180.0), (30.0, -190.0), (150.0, 150.0)] {
            let pos = [h.me().pos[0] + dx, h.me().pos[1] + dy];
            h.drops.push(Drop { kind: DropKind::Money(each), pos, z: 0.0, vz: 0.0, vel: [0.0; 2], age: 0.0, pulled: None, to: ME, dog: None });
        }
        h.drops.push(Drop { kind: DropKind::Health, pos: [h.me().pos[0] + 200.0, h.me().pos[1] + 200.0], z: 0.0, vz: 0.0, vel: [0.0; 2], age: 0.0, pulled: None, to: ME, dog: None });
        h
    }

    #[test]
    fn the_driver_holds_no_ticket_before_after() {
        let t = crate::tuning::tuning();
        let mut h = collection_day(1);
        h.t = t.driver_after - 1.0;
        assert!(h.ticket().is_none());
        h.cash_in(&mut Vec::new());
        assert!(h.drops.iter().all(|d| d.pulled.is_none()), "no ticket, no cash-in");
        h.t = t.driver_after;
        assert_eq!(h.ticket(), h.map.driver);
    }

    #[test]
    fn cashing_in_pulls_every_coin_on_the_map_and_the_next_ticket_waits_every() {
        let t = crate::tuning::tuning();
        let mut h = collection_day(1);
        h.t = t.driver_after + 5.0;
        h.map.driver = Some([h.me().pos[0] + t.driver_reach + 1.0, h.me().pos[1]]);
        h.cash_in(&mut Vec::new());
        assert!(h.drops.iter().all(|d| d.pulled.is_none()), "out of reach of the driver");
        h.map.driver = Some([h.me().pos[0] + t.driver_reach - 1.0, h.me().pos[1]]);
        let mut sounds = Vec::new();
        h.cash_in(&mut sounds);
        assert!(h.drops.iter().filter(|d| matches!(d.kind, DropKind::Money(_))).all(|d| d.pulled.is_some()), "every coin flies in");
        assert!(h.drops.iter().filter(|d| matches!(d.kind, DropKind::Health)).all(|d| d.pulled.is_none()), "only money");
        assert!(sounds.contains(&Sfx::Pick(true)));
        assert!(h.ticket().is_none(), "the ticket is spent");
        h.t += t.driver_every - 1.0;
        assert!(h.ticket().is_none());
        h.t += 1.0;
        assert!(h.ticket().is_some(), "the next ticket comes `every` after the cash-in");
        for _ in 0..300 {
            h.collect(1.0 / 60.0, &mut Vec::new());
        }
        assert_eq!(h.drops.iter().filter(|d| matches!(d.kind, DropKind::Money(_))).count(), 0, "every coin reached the player");
        assert_eq!(h.xp, 5);
    }

    #[test]
    fn a_map_without_a_driver_never_offers_a_ticket() {
        let mut h = collection_day(1);
        h.map.driver = None;
        h.t = crate::tuning::tuning().driver_after * 4.0;
        assert!(h.ticket().is_none());
        h.cash_in(&mut Vec::new());
        assert!(h.drops.iter().all(|d| d.pulled.is_none()));
    }

    #[test]
    fn one_collection_brings_level_ups_one_after_another() {
        let mut h = collection_day(0);
        let mut owed = 0;
        let mut need = h.next_xp;
        for _ in 0..3 {
            owed += need;
            need = (need as f32 * 1.35).ceil() as u32 + 2;
        }
        for d in &mut h.drops {
            if let DropKind::Money(v) = &mut d.kind {
                *v = owed.div_ceil(5);
            }
        }
        h.t = crate::tuning::tuning().driver_after;
        let x = Input { pressed: Buttons::X, ..Default::default() };
        let mut level_ups = 0;
        for _ in 0..3000 {
            if let Phase::LevelUp { picked: None, t, .. } = h.phase
                && t > CARDS_IN
            {
                level_ups += 1;
                h.update(&x, 0.016, &mut Vec::new());
            } else {
                h.update(&Input::default(), 0.016, &mut Vec::new());
            }
        }
        assert!(h.ticket().is_none(), "the player cashed in standing at the driver");
        assert_eq!((level_ups, h.level), (3, 4), "three levels' worth of cash, three level-ups in a row");
    }
}
