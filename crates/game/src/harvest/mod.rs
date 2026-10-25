//! Last Harvest: survive the night on a randomly generated farm while the horde closes in.

mod juice;
mod map;
mod view3d;

use std::f32::consts::{PI, TAU};

use crate::art::{self, Art};
use crate::rng::Rng;
use crate::{Align, Buttons, Canvas, Color, Frame, Input, SCREEN_H, SCREEN_W, Sfx};
use juice::{Corpse, Juice, Particle, Tone};
use map::{Map, TILE};

/// Screen pixels per art pixel on the bottom screen's map, which shows 512x384 art pixels.
const SCALE: f32 = 2.0;
const VIEW_W: f32 = SCREEN_W / SCALE;
const VIEW_H: f32 = SCREEN_H / SCALE;
/// Zombies erupt in a ring around the player, in world pixels: at the fog's edge at dusk,
/// closing to a few tiles away by dawn.
const SPAWN_RING_DUSK: [f32; 2] = [200.0, 250.0];
const SPAWN_RING_DAWN: [f32; 2] = [56.0, 110.0];
/// Seconds a zombie takes to climb out of the ground, harmless and rooted.
const RISE_TIME: f32 = 0.9;
const PISTOL_RANGE: f32 = 150.0;
const SHOTGUN_RANGE: f32 = 90.0;
/// Radians per second at full turn.
const TURN_SPEED: f32 = 2.6;
/// Seconds for the quick 180-degree turn.
const SPIN_TIME: f32 = 0.2;
const GRENADES: f32 = 3.0;
const GRENADE_RECHARGE: f32 = 12.0;
const GRENADE_FLIGHT: f32 = 0.7;
const GRENADE_RADIUS: f32 = 36.0;
const MAP_W: usize = 72;
const MAP_H: usize = 54;
const DAWN: f32 = 600.0;
const MAX_ZOMBIES: usize = 300;
const MAX_STAINS: usize = 160;
const PLAYER_HALF: [f32; 2] = [4.0, 3.0];
const ZOMBIE_HALF: [f32; 2] = [3.0, 2.0];
const HURT_TIME: f32 = 0.7;
const FLASH_TIME: f32 = 0.08;
const WALK_FPS: f32 = 8.0;
const PICKUP_RADIUS: f32 = 7.0;
/// Seconds the ground churns before a zombie breaks out.
const TELL_TIME: f32 = 0.6;
const CRIT_CHANCE: f32 = 0.1;
/// Seconds a struck zombie shows its red frame.
const HURT_FRAMES: f32 = 0.18;
/// Real seconds of slow motion between filling the bar and choosing an upgrade.
const LEVEL_SLOW: f32 = 0.7;
const LEVEL_PUSH_RADIUS: f32 = 80.0;
/// Seconds after a pick before play resumes, so the choice can land.
const PICK_TIME: f32 = 0.3;
/// Seconds the upgrade cards ignore input while they slide in.
const CARDS_IN: f32 = 0.35;
const RARE_CHANCE: f32 = 0.12;
const STAGGER_TIME: f32 = 0.4;
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

/// Walk frames for each facing, then the flash and hurt frames for each facing.
struct Look {
    walk: [&'static [Art; 3]; 3],
    flash: &'static [Art; 3],
    hurt: &'static [Art; 3],
}

const PLAYER: Look = Look { walk: [&art::PLAYER_FRONT, &art::PLAYER_SIDE, &art::PLAYER_BACK], flash: &art::PLAYER_FLASH, hurt: &art::PLAYER_HURT };

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Skinny,
    Kid,
    Big,
}

struct KindStats {
    hp: f32,
    speed: f32,
    damage: f32,
    xp: u32,
    /// Divides knockback.
    mass: f32,
    look: Look,
}

impl Kind {
    fn stats(self) -> KindStats {
        match self {
            Kind::Skinny => KindStats { hp: 20.0, speed: 20.0, damage: 8.0, xp: 1, mass: 1.0, look: Look { walk: [&art::SKINNY_FRONT, &art::SKINNY_SIDE, &art::SKINNY_BACK], flash: &art::SKINNY_FLASH, hurt: &art::SKINNY_HURT } },
            Kind::Kid => KindStats { hp: 10.0, speed: 32.0, damage: 5.0, xp: 1, mass: 0.6, look: Look { walk: [&art::KID_FRONT, &art::KID_SIDE, &art::KID_BACK], flash: &art::KID_FLASH, hurt: &art::KID_HURT } },
            Kind::Big => KindStats { hp: 90.0, speed: 12.0, damage: 18.0, xp: 5, mass: 3.5, look: Look { walk: [&art::BIG_FRONT, &art::BIG_SIDE, &art::BIG_BACK], flash: &art::BIG_FLASH, hurt: &art::BIG_HURT } },
        }
    }
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
}

impl Zombie {
    fn new(kind: Kind, pos: [f32; 2], hp: f32, rise: f32, anim: f32) -> Self {
        Self { kind, pos, dir: [0.0, 1.0], rise, push: [0.0; 2], hp, flash: 0.0, hurt: 0.0, squash: 0.0, knock: [0.0; 2], fling: 0.0, stagger: 0.0, facing: (Facing::Front, false), anim }
    }

    /// The frame showing now, for a facing.
    fn art(&self, facing: Facing) -> Art {
        let look = &self.kind.stats().look;
        if self.flash > 0.0 {
            look.flash[facing.index()]
        } else if self.hurt > 0.0 || self.stagger > 0.0 {
            look.hurt[facing.index()]
        } else {
            look.walk[facing.index()][self.frame()]
        }
    }
}

/// A zombie about to break out of the ground.
struct Tell {
    kind: Kind,
    pos: [f32; 2],
    hp: f32,
    t: f32,
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
            art::CROW_FLY[(self.anim * 12.0) as usize % art::CROW_FLY.len()]
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
    hit: Vec<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DropKind {
    Money(u32),
    Health,
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

/// A grenade in the air from `from` to `to`, `t` seconds after the throw.
struct Grenade {
    from: [f32; 2],
    to: [f32; 2],
    t: f32,
}

impl Grenade {
    /// Ground position and height above it, in world pixels.
    fn at(&self) -> ([f32; 2], f32) {
        let k = (self.t / GRENADE_FLIGHT).min(1.0);
        let p = [self.from[0] + (self.to[0] - self.from[0]) * k, self.from[1] + (self.to[1] - self.from[1]) * k];
        (p, (k * PI).sin() * 28.0)
    }
}

const BARREL_HP: f32 = 20.0;
const BLAST_RADIUS: f32 = 40.0;

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
    fn frame(&self) -> usize {
        (self.anim * WALK_FPS * self.kind.stats().speed / 20.0) as usize % 3
    }
}

/// The player's frame for a facing: flashing while invulnerable, red just after a hit, else walking.
fn art_for(h: &Harvest, look: &Look, facing: Facing) -> Art {
    let i = facing.index();
    if h.hurt > 0.0 && ((h.hurt * 20.0) as u32).is_multiple_of(2) {
        look.flash[i]
    } else if h.hurt > HURT_TIME - 0.2 {
        art::PLAYER_HURT[i]
    } else {
        look.walk[i][if h.walk > 0.0 { 1 + (h.walk * WALK_FPS) as usize % 2 } else { 0 }]
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
    fn done(&self) -> bool {
        self.t * self.fps >= self.frames.len() as f32
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Upgrade {
    Damage,
    FireRate,
    Multishot,
    Shotgun,
    Knife,
    Speed,
    Vitality,
    Magnet,
}

impl Upgrade {
    const ALL: [Upgrade; 8] = [
        Upgrade::Damage,
        Upgrade::FireRate,
        Upgrade::Multishot,
        Upgrade::Shotgun,
        Upgrade::Knife,
        Upgrade::Speed,
        Upgrade::Vitality,
        Upgrade::Magnet,
    ];

    fn text(self, gear: &Gear) -> (&'static str, &'static str) {
        match self {
            Upgrade::Damage => ("Hollow points", "+30% damage for every weapon"),
            Upgrade::FireRate => ("Quick hands", "Every weapon attacks 15% faster"),
            Upgrade::Multishot => ("Second pistol", "The pistol fires one more bullet"),
            Upgrade::Shotgun if gear.shotgun == 0 => ("Shotgun", "A spread of pellets at close range"),
            Upgrade::Shotgun => ("Sawn-off barrel", "The shotgun fires two more pellets"),
            Upgrade::Knife if gear.knife == 0 => ("Knife", "Slashes everything close by"),
            Upgrade::Knife => ("Longer blade", "The knife reaches further and cuts deeper"),
            Upgrade::Speed => ("Running shoes", "Move 12% faster"),
            Upgrade::Vitality => ("Canned peaches", "+25 max health, and heal 25"),
            Upgrade::Magnet => ("Deep pockets", "Pick up money from further away"),
        }
    }
}

/// The player's weapons and upgrades.
#[derive(Clone, Debug)]
struct Gear {
    damage: f32,
    rate: f32,
    bullets: u32,
    shotgun: u32,
    knife: u32,
    speed: f32,
    magnet: f32,
    max_hp: f32,
    cooldowns: [f32; 3],
}

impl Default for Gear {
    fn default() -> Self {
        Self { damage: 1.0, rate: 1.0, bullets: 1, shotgun: 0, knife: 0, speed: 48.0, magnet: 28.0, max_hp: 100.0, cooldowns: [0.5, 0.0, 0.0] }
    }
}

impl Gear {
    fn apply(&mut self, up: Upgrade) -> f32 {
        let mut heal = 0.0;
        match up {
            Upgrade::Damage => self.damage *= 1.3,
            Upgrade::FireRate => self.rate *= 0.85,
            Upgrade::Multishot => self.bullets += 1,
            Upgrade::Shotgun => self.shotgun += 1,
            Upgrade::Knife => self.knife += 1,
            Upgrade::Speed => self.speed *= 1.12,
            Upgrade::Vitality => {
                self.max_hp += 25.0;
                heal = 25.0;
            }
            Upgrade::Magnet => self.magnet *= 1.4,
        }
        heal
    }
}

/// An upgrade on offer; a rare one counts twice.
#[derive(Clone, Copy, Debug)]
struct Choice {
    up: Upgrade,
    rare: bool,
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
    /// Seconds survived, on the frame a run ends.
    pub survived: Option<f32>,
}

pub struct Harvest {
    rng: Rng,
    map: Map,
    t: f32,
    phase: Phase,
    pos: [f32; 2],
    hp: f32,
    hurt: f32,
    facing: (Facing, bool),
    aim: [f32; 2],
    walk: f32,
    gear: Gear,
    zombies: Vec<Zombie>,
    bullets: Vec<Bullet>,
    drops: Vec<Drop>,
    fx: Vec<Fx>,
    /// Blood and scorch marks: art, top-left, and a tint.
    stains: Vec<(Art, f32, f32, [f32; 3])>,
    barrels: Vec<Barrel>,
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
    /// The player's heading, which the 3D camera looks along, in radians from +x toward +y.
    view_yaw: f32,
    /// Radians left of a quick turn in progress.
    spin: f32,
    /// False until the turn buttons are let go, so the A that started the run or picked an upgrade doesn't turn.
    turn_ready: bool,
    /// No zombies: a walk around the farm, for looking at it.
    calm: bool,
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
    pub fn new(seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let map = Map::generate(&mut rng, MAP_W, MAP_H);
        let gear = Gear::default();
        let barrels = map.barrels.iter().map(|&pos| Barrel { pos, hp: BARREL_HP, flash: 0.0 }).collect();
        Self {
            barrels,
            grenades: GRENADES,
            thrown: Vec::new(),
            pos: map.spawn,
            hp: gear.max_hp,
            rng,
            map,
            t: 0.0,
            phase: Phase::Playing,
            hurt: 0.0,
            facing: (Facing::Front, false),
            aim: [0.0, 1.0],
            walk: 0.0,
            gear,
            zombies: Vec::new(),
            bullets: Vec::new(),
            drops: Vec::new(),
            fx: Vec::new(),
            stains: Vec::new(),
            owed: 0.0,
            xp: 0,
            next_xp: 5,
            level: 1,
            kills: 0,
            overview: false,
            flow: Vec::new(),
            flow_cell: [usize::MAX; 2],
            view_yaw: -PI / 2.0,
            spin: 0.0,
            turn_ready: false,
            calm: false,
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

    pub fn calm(seed: u64) -> Self {
        Self { calm: true, ..Self::new(seed) }
    }

    pub fn summary(&self) -> String {
        format!("harvest t={:.0} level={} kills={} zombies={} hp={:.0}", self.t, self.level, self.kills, self.zombies.len(), self.hp)
    }

    pub fn update(&mut self, input: &Input, dt: f32, sounds: &mut Vec<Sfx>) -> Flow {
        let mut flow = Flow::default();
        self.overview = input.held.contains(Buttons::L1);
        if self.overview {
            return flow;
        }
        self.juice.tick_real(dt);
        self.glory = (self.glory - dt).max(0.0);
        match self.phase {
            Phase::Playing if self.juice.frozen() => {}
            Phase::Playing => self.play(input, dt, sounds, &mut flow),
            Phase::LevelUp { .. } => self.choose(input, dt, sounds),
            Phase::Over { t, won } => {
                let t = t + dt;
                self.phase = Phase::Over { t, won };
                self.juice.tick(dt * 0.4);
                if t > 1.0 && input.pressed.any(Buttons::A | Buttons::START) {
                    let seed = self.rng.next_u32() as u64 + 1;
                    *self = Harvest::new(seed);
                    sounds.push(Sfx::Start);
                } else if t > 1.0 && input.pressed.contains(Buttons::B) {
                    flow.quit = true;
                }
            }
        }
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
            let tapped = input.touch.filter(|_| input.touch_started).and_then(|[_, y]| choice_row(y));
            if let Some(i) = tapped.or(input.pressed.any(Buttons::A | Buttons::START).then_some(selected)) {
                let c = choices[i];
                let mut heal = 0.0;
                for _ in 0..if c.rare { 2 } else { 1 } {
                    heal += self.gear.apply(c.up);
                }
                self.hp = (self.hp + heal).min(self.gear.max_hp);
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
        // Up and down walk along the heading, left and right strafe, Y and A (or L2 and R2) turn, B turns around.
        let dir = input.dir();
        let turning = Buttons::A | Buttons::Y | Buttons::L2 | Buttons::R2;
        self.turn_ready |= !input.held.any(turning);
        let held = if self.turn_ready { input.held } else { Buttons::NONE };
        let turn = held.any(Buttons::A | Buttons::R2) as i32 as f32 - held.any(Buttons::Y | Buttons::L2) as i32 as f32;
        if input.pressed.contains(Buttons::B) && self.spin <= 0.0 {
            self.spin = PI;
        }
        let spun = self.spin.min(PI / SPIN_TIME * dt);
        self.spin -= spun;
        self.view_yaw = (self.view_yaw + turn * TURN_SPEED * dt + spun).rem_euclid(TAU);
        let f = [self.view_yaw.cos(), self.view_yaw.sin()];
        let (side, ahead) = (dir[0], -dir[1]);
        let mv = [f[0] * ahead - f[1] * side, f[1] * ahead + f[0] * side];
        self.aim = f;
        self.facing = Facing::of(f);
        if len(mv) > 0.2 {
            let k = self.gear.speed * dt * len(mv).min(1.0) / len(mv);
            self.pos = self.map.slide(self.pos, [mv[0] * k, mv[1] * k], PLAYER_HALF);
            let before = (self.walk * WALK_FPS) as u32;
            self.walk += dt;
            if (self.walk * WALK_FPS) as u32 / 2 != before / 2 {
                let back = [-mv[0], -mv[1]];
                self.juice.burst(self.pos, 0.5, 2, back, 0.8, [8.0, 20.0], juice::DIRT, 1.0, 0.35, 0.0);
            }
        } else {
            self.walk = 0.0;
        }
        self.noise = None;
        self.grenades = (self.grenades + dt / GRENADE_RECHARGE).min(GRENADES);
        if let Some(touch) = input.touch.filter(|_| input.touch_started) {
            self.throw(touch, sounds);
        }
        self.fly(dt, sounds, flow);

        if !self.calm {
            self.spawn(dt);
        } else if input.pressed.contains(Buttons::X) {
            // The quiet farm takes one zombie at a time, for trying out weapons and effects.
            let pos = [self.pos[0] + f[0] * 50.0, self.pos[1] + f[1] * 50.0];
            if !self.map.hits(pos[0], pos[1], ZOMBIE_HALF) {
                self.tells.push(Tell { kind: Kind::Skinny, pos, hp: Kind::Skinny.stats().hp, t: 0.0 });
            }
        }
        self.rise(dt, sounds);
        self.move_zombies(dt);
        self.contact(sounds, flow);
        self.fire(dt, sounds);
        self.move_bullets(dt, sounds);
        if self.blow(dt, sounds) {
            flow.rumble = flow.rumble.max(Some(Rumble::Blast));
        }
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

        if self.hp <= 0.0 {
            self.hp = 0.0;
            sounds.push(Sfx::GameOver);
            self.juice.shake(0.8);
            self.juice.burst(self.pos, 8.0, 30, [0.0; 2], 0.0, [30.0, 120.0], juice::BLOOD, 2.0, 0.9, 0.0);
            flow.rumble = Some(Rumble::Blast);
            self.leveling = 0.0;
            self.phase = Phase::Over { t: 0.0, won: false };
            flow.survived = Some(self.t);
        } else if self.t >= DAWN {
            sounds.push(Sfx::Dawn);
            // First light burns the dead away.
            for z in std::mem::take(&mut self.zombies) {
                self.juice.burst(z.pos, 8.0, 3, [0.0; 2], 0.0, [10.0, 40.0], juice::EMBER, 1.4, 1.2, 1.0);
                self.fx.push(Fx { frames: &art::SMOKE, fps: 5.0, pos: [z.pos[0], z.pos[1] - 6.0], angle: 0.0, t: 0.0 });
            }
            self.tells.clear();
            self.juice.flash(self.pos, 40.0, [1.6, 1.1, 0.7], 14.0, 1.5);
            self.juice.shake(0.3);
            flow.rumble = flow.rumble.max(Some(Rumble::Fanfare));
            self.phase = Phase::Over { t: 0.0, won: true };
            flow.survived = Some(self.t);
        } else if self.leveling <= 0.0 && self.xp >= self.next_xp {
            self.level_up(sounds, flow);
        }
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
        for z in &mut self.zombies {
            let d = sub(z.pos, self.pos);
            let l = len(d);
            if l < LEVEL_PUSH_RADIUS && z.rise >= 1.0 {
                let k = 320.0 * (1.0 - l / LEVEL_PUSH_RADIUS) / z.kind.stats().mass;
                let a = norm(d);
                z.push = [a[0] * k, a[1] * k];
                z.squash = 1.0;
                z.knock = a;
            }
        }
        self.juice.ring(self.pos, LEVEL_PUSH_RADIUS, 0.5, juice::GOLD);
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
                color: juice::GOLD,
                size: 1.6,
                shrink: true,
                gravity: 40.0,
                drag: 1.0,
                glow: 1.0,
                wander: false,
            });
        }
    }

    fn roll_choices(&mut self) -> [Choice; 3] {
        let mut pool: Vec<Upgrade> = Upgrade::ALL.to_vec();
        let mut out = [Choice { up: Upgrade::Damage, rare: false }; 3];
        for slot in &mut out {
            let up = pool.swap_remove(self.rng.below(pool.len()));
            *slot = Choice { up, rare: self.rng.range(0.0, 1.0) < RARE_CHANCE };
        }
        out
    }

    /// Zombies arrive just out of sight, faster and tougher as the night goes on.
    fn spawn(&mut self, dt: f32) {
        let minutes = self.t / 60.0;
        self.owed += (0.7 + minutes * 0.9).min(9.0) * dt;
        if horde_now(self.t, dt) {
            self.owed += 20.0 + minutes * 4.0;
        }
        while self.owed >= 1.0 && self.zombies.len() + self.tells.len() < MAX_ZOMBIES {
            self.owed -= 1.0;
            let roll = self.rng.range(0.0, 1.0);
            let kind = if minutes > 2.0 && roll < 0.06 + minutes * 0.01 {
                Kind::Big
            } else if minutes > 1.0 && roll < 0.35 {
                Kind::Kid
            } else {
                Kind::Skinny
            };
            for _ in 0..8 {
                let a = self.rng.range(0.0, TAU);
                let [near, far] = spawn_ring(self.t);
                let r = self.rng.range(near, far);
                let pos = [self.pos[0] + a.cos() * r, self.pos[1] + a.sin() * r];
                if !self.map.hits(pos[0], pos[1], ZOMBIE_HALF) {
                    self.tells.push(Tell { kind, pos, hp: kind.stats().hp * (1.0 + minutes * 0.08), t: 0.0 });
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
                    self.juice.burst(pos, 0.0, 1, [0.0; 2], 0.0, [6.0, 18.0], juice::DIRT, 1.4, 0.4, 0.0);
                }
                continue;
            }
            let t = self.tells.swap_remove(i);
            let anim = self.rng.range(0.0, 1.0);
            self.zombies.push(Zombie::new(t.kind, t.pos, t.hp, 0.0, anim));
            self.fx.push(Fx { frames: &art::SMOKE, fps: 7.0, pos: [pos[0], pos[1] - 3.0], angle: 0.0, t: 0.0 });
            self.juice.burst(pos, 0.0, 8, [0.0; 2], 0.0, [15.0, 45.0], juice::DIRT, 1.6, 0.6, 0.0);
            let (pan, gain) = self.place(pos);
            if self.hush[3] <= 0.0 && gain > 60 {
                sounds.push(Sfx::Rise { pan, gain });
                self.hush[3] = 0.3;
            }
        }
    }

    fn move_zombies(&mut self, dt: f32) {
        let n = self.zombies.len();
        let mut sep = vec![[0.0f32; 2]; n];
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
            self.flow = self.map.flow(self.pos);
            self.flow_cell = cell;
        }
        for (z, s) in self.zombies.iter_mut().zip(sep) {
            z.flash = (z.flash - dt).max(0.0);
            z.hurt = (z.hurt - dt).max(0.0);
            z.squash = (z.squash - dt * 5.0).max(0.0);
            if z.rise < 1.0 {
                z.rise = (z.rise + dt / RISE_TIME).min(1.0);
                continue;
            }
            if z.stagger > 0.0 {
                z.stagger -= dt;
                continue;
            }
            let speed = z.kind.stats().speed;
            let direct = sub(self.pos, z.pos);
            let to = if len(direct) < TILE * 1.5 { norm(direct) } else { self.map.downhill(&self.flow, z.pos).unwrap_or(norm(direct)) };
            let v = [to[0] * speed + s[0] * 20.0 + z.push[0], to[1] * speed + s[1] * 20.0 + z.push[1]];
            z.pos = self.map.slide(z.pos, [v[0] * dt, v[1] * dt], ZOMBIE_HALF);
            z.push = [z.push[0] * (1.0 - 8.0 * dt).max(0.0), z.push[1] * (1.0 - 8.0 * dt).max(0.0)];
            z.facing = Facing::of(to);
            z.dir = to;
            z.anim += dt;
        }
    }

    fn contact(&mut self, sounds: &mut Vec<Sfx>, flow: &mut Flow) {
        if self.hurt > 0.0 {
            return;
        }
        let hit = self.zombies.iter().filter(|z| z.rise >= 1.0 && len(sub(z.pos, self.pos)) < 9.0).map(|z| z.kind.stats().damage).fold(0.0, f32::max);
        if hit > 0.0 {
            self.hp -= hit;
            self.hurt = HURT_TIME;
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
        self.juice.burst(self.pos, 8.0, 10, [0.0; 2], 0.0, [30.0, 80.0], juice::BLOOD, 1.6, 0.5, 0.0);
    }

    fn nearest(&self, range: f32) -> Option<[f32; 2]> {
        self.zombies
            .iter()
            .map(|z| sub(z.pos, self.pos))
            .filter(|d| len(*d) < range)
            .min_by(|a, b| len(*a).total_cmp(&len(*b)))
    }

    fn fire(&mut self, dt: f32, sounds: &mut Vec<Sfx>) {
        for c in &mut self.gear.cooldowns {
            *c -= dt;
        }
        let aim_at = |range: f32, this: &Harvest| this.nearest(range).map(norm);

        if self.gear.cooldowns[0] <= 0.0
            && let Some(d) = aim_at(PISTOL_RANGE, self)
        {
            self.gear.cooldowns[0] = 0.6 * self.gear.rate;
            let n = self.gear.bullets;
            let base = d[1].atan2(d[0]);
            for k in 0..n {
                let a = base + (k as f32 - (n - 1) as f32 / 2.0) * 0.14;
                self.shoot(a, 220.0, 12.0 * self.gear.damage, 0, 0.7);
            }
            let muzzle = [self.pos[0] + d[0] * 8.0, self.pos[1] - 6.0 + d[1] * 8.0];
            self.fx.push(Fx { frames: std::slice::from_ref(&art::MUZZLE), fps: 20.0, pos: muzzle, angle: base, t: 0.0 });
            self.juice.kick += 0.015;
            self.juice.flash([self.pos[0] + d[0] * 8.0, self.pos[1] + d[1] * 8.0], 10.0, [1.2, 0.85, 0.4], 3.5, 0.07);
            self.noise = Some(self.pos);
            sounds.push(Sfx::Shot);
        }

        if self.gear.shotgun > 0
            && self.gear.cooldowns[1] <= 0.0
            && let Some(d) = aim_at(SHOTGUN_RANGE, self)
        {
            self.gear.cooldowns[1] = 1.5 * self.gear.rate;
            let pellets = 3 + 2 * self.gear.shotgun;
            let base = d[1].atan2(d[0]);
            for k in 0..pellets {
                let a = base + (k as f32 / (pellets - 1) as f32 - 0.5) * 0.7;
                self.shoot(a, 200.0, 8.0 * self.gear.damage, 1, 0.4);
            }
            let at = [self.pos[0] + d[0] * 10.0, self.pos[1] - 6.0 + d[1] * 10.0];
            self.fx.push(Fx { frames: &art::SHOTGUN_BLAST, fps: 18.0, pos: at, angle: base, t: 0.0 });
            let ground = [self.pos[0] + d[0] * 10.0, self.pos[1] + d[1] * 10.0];
            self.juice.kick += 0.04;
            self.juice.punch += 3.0;
            self.juice.shake(0.12);
            self.juice.flash(ground, 10.0, [1.6, 1.1, 0.5], 5.0, 0.1);
            self.juice.burst(ground, 8.0, 8, d, 0.35, [80.0, 160.0], juice::SPARK, 1.0, 0.18, 1.0);
            self.noise = Some(self.pos);
            sounds.push(Sfx::Blast);
        }

        if self.gear.knife > 0 && self.gear.cooldowns[2] <= 0.0 {
            let reach = 18.0 + 4.0 * self.gear.knife as f32;
            if let Some(d) = aim_at(reach, self).or_else(|| (self.nearest(reach + 6.0).is_some()).then_some(self.aim)) {
                self.gear.cooldowns[2] = 1.1 * self.gear.rate;
                let damage = (10.0 + 5.0 * self.gear.knife as f32) * self.gear.damage;
                let mut hits = (0, false);
                for z in &mut self.zombies {
                    if len(sub(z.pos, self.pos)) < reach {
                        let away = norm(sub(z.pos, self.pos));
                        hits.1 |= strike(z, &mut self.juice, damage, away, 60.0);
                        hits.0 += 1;
                    }
                }
                let at = [self.pos[0] + d[0] * 8.0, self.pos[1] - 5.0 + d[1] * 8.0];
                self.fx.push(Fx { frames: &art::SLASH, fps: 20.0, pos: at, angle: d[1].atan2(d[0]) - PI / 2.0, t: 0.0 });
                self.juice.kick += 0.02;
                self.juice.shake(0.06);
                sounds.push(Sfx::Slash);
                self.thud(hits, sounds);
            }
        }
    }

    /// One hit sound for however many landed this frame, unless one just played.
    fn thud(&mut self, (hits, crit): (u32, bool), sounds: &mut Vec<Sfx>) {
        if hits > 0 && (self.hush[0] <= 0.0 || crit) {
            sounds.push(Sfx::Thud(crit));
            self.hush[0] = 0.06;
        }
    }

    fn shoot(&mut self, angle: f32, speed: f32, damage: f32, pierce: u32, life: f32) {
        let vel = [angle.cos() * speed, angle.sin() * speed];
        self.bullets.push(Bullet { pos: [self.pos[0], self.pos[1] - 6.0], vel, damage, pierce, life, hit: Vec::new() });
    }

    fn move_bullets(&mut self, dt: f32, sounds: &mut Vec<Sfx>) {
        let mut hits = (0, false);
        for b in &mut self.bullets {
            b.pos = [b.pos[0] + b.vel[0] * dt, b.pos[1] + b.vel[1] * dt];
            b.life -= dt;
            let ground = [b.pos[0], b.pos[1] + 6.0];
            let back = norm([-b.vel[0], -b.vel[1]]);
            if self.map.solid_at(ground[0], ground[1]) {
                let cell = |p: [f32; 2]| [(p[0] / TILE).floor(), (p[1] / TILE).floor()];
                let at = cell(ground);
                if let Some(barrel) = self.barrels.iter_mut().find(|r| cell(r.pos) == at) {
                    barrel.hp -= b.damage;
                    barrel.flash = FLASH_TIME;
                    self.juice.burst(ground, 6.0, 4, back, 0.8, [40.0, 100.0], juice::SPARK, 0.8, 0.2, 1.0);
                } else {
                    self.juice.burst(ground, 6.0, 2, back, 0.9, [20.0, 50.0], juice::DIRT, 1.0, 0.3, 0.0);
                }
                b.life = 0.0;
                continue;
            }
            for (i, z) in self.zombies.iter_mut().enumerate() {
                if b.life <= 0.0 || b.hit.contains(&i) {
                    continue;
                }
                let d = sub([z.pos[0], z.pos[1] - 6.0], b.pos);
                if len(d) < 7.0 {
                    let force = if b.pierce > 0 { 45.0 } else { 30.0 };
                    hits.1 |= strike(z, &mut self.juice, b.damage, norm(b.vel), force);
                    hits.0 += 1;
                    b.hit.push(i);
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
        self.explode(c, BLAST_RADIUS, 60.0, sounds);
        true
    }

    /// Damages zombies in `radius`, sets off barrels in it, and hurts the player if too close.
    fn explode(&mut self, c: [f32; 2], radius: f32, damage: f32, sounds: &mut Vec<Sfx>) {
        let mut hits = (0, false);
        for z in &mut self.zombies {
            let d = sub(z.pos, c);
            if len(d) < radius {
                hits.1 |= strike(z, &mut self.juice, damage * self.gear.damage, norm(d), 120.0);
                hits.0 += 1;
                z.fling = 60.0 + 150.0 * (1.0 - len(d) / radius);
            }
        }
        for r in &mut self.barrels {
            if len(sub(r.center(), c)) < radius {
                r.hp = r.hp.min(0.0);
            }
        }
        if len(sub(self.pos, c)) < radius * 0.7 && self.hurt <= 0.0 {
            self.hp -= 15.0;
            self.juice.popup(self.pos, 15.0, Tone::Hurt);
            self.hurt = HURT_TIME;
            self.ouch(0.3);
        }
        self.fx.push(Fx { frames: &art::EXPLOSION, fps: 14.0, pos: c, angle: 0.0, t: 0.0 });
        self.fx.push(Fx { frames: &art::SMOKE, fps: 8.0, pos: [c[0], c[1] - 8.0], angle: 0.0, t: 0.0 });
        let near = len(sub(self.pos, c));
        self.juice.shake((0.75 * (1.0 - near / 260.0)).max(0.12));
        self.juice.stop(0.07);
        self.juice.punch += 4.0;
        self.juice.flash(c, 14.0, [2.4, 1.3, 0.5], 8.0, 0.45);
        self.juice.ring(c, radius * 1.3, 0.45, juice::EMBER);
        self.juice.burst(c, 4.0, 18, [0.0; 2], 0.0, [40.0, 140.0], juice::EMBER, 1.2, 0.6, 1.0);
        self.juice.burst(c, 2.0, 14, [0.0; 2], 0.0, [30.0, 90.0], juice::DIRT, 1.8, 0.8, 0.0);
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
        if self.grenades < 1.0 {
            sounds.push(Sfx::Miss);
            return;
        }
        self.grenades -= 1.0;
        let cam = self.camera();
        let to = [cam[0] + touch[0] / SCALE, cam[1] + touch[1] / SCALE];
        self.thrown.push(Grenade { from: self.pos, to, t: 0.0 });
        sounds.push(Sfx::Throw);
    }

    fn fly(&mut self, dt: f32, sounds: &mut Vec<Sfx>, flow: &mut Flow) {
        for g in &mut self.thrown {
            g.t += dt;
        }
        let landed: Vec<[f32; 2]> = self.thrown.iter().filter(|g| g.t >= GRENADE_FLIGHT).map(|g| g.to).collect();
        self.thrown.retain(|g| g.t < GRENADE_FLIGHT);
        for c in landed {
            self.explode(c, GRENADE_RADIUS, 50.0, sounds);
            flow.rumble = flow.rumble.max(Some(Rumble::Blast));
        }
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
                Some(Kind::Big) => Kind::Big,
                _ => z.kind,
            });
            let s = z.kind.stats();
            self.fx.push(Fx { frames: &art::BLOOD_SPLAT, fps: 12.0, pos: [z.pos[0], z.pos[1] - 4.0], angle: 0.0, t: 0.0 });
            let stain = art::BLOOD_STAIN[self.rng.below(art::BLOOD_STAIN.len())];
            self.stain((stain, z.pos[0] - stain.w() / 2.0, z.pos[1] - stain.h() / 2.0, [1.0; 3]));
            let fall = if z.knock == [0.0; 2] { norm(sub(z.pos, self.pos)) } else { z.knock };
            let (vel, vz) = if z.fling > 0.0 { ([fall[0] * z.fling, fall[1] * z.fling], z.fling * 0.9) } else { ([fall[0] * 25.0, fall[1] * 25.0], 0.0) };
            let flip = self.juice.rng.below(2) == 0;
            self.juice.corpse(Corpse { art: s.look.hurt[Facing::Front.index()], flip, pos: z.pos, z: 0.0, vel, vz, fall, age: 0.0, spin: 0.0 });
            let gibs = match z.kind {
                Kind::Kid => 14,
                Kind::Skinny => 9,
                Kind::Big => 20,
            };
            self.juice.burst(z.pos, 8.0, gibs, fall, 1.2, [30.0, 110.0], juice::BLOOD, 1.6, 0.6, 0.0);
            self.juice.burst(z.pos, 8.0, gibs / 3, fall, 0.8, [20.0, 60.0], juice::DARK_BLOOD, 2.4, 0.8, 0.0);
            if z.kind == Kind::Big {
                self.juice.stop(0.06);
                self.juice.shake(0.2);
            }
            let kind = if self.rng.below(60) == 0 { DropKind::Health } else { DropKind::Money(s.xp) };
            let a = self.juice.rng.range(0.0, TAU);
            let speed = self.juice.rng.range(8.0, 28.0);
            let vz = self.juice.rng.range(60.0, 95.0);
            self.drops.push(Drop { kind, pos: z.pos, z: 4.0, vz, vel: [a.cos() * speed, a.sin() * speed], age: 0.0, pulled: None });
        }
        if let Some(kind) = died {
            if self.hush[1] <= 0.0 || kind == Kind::Big {
                sounds.push(Sfx::Kill(kind as u8));
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
            if dist < self.gear.magnet && d.pulled.is_none() {
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
            if dist < PICKUP_RADIUS || (d.pulled.is_some() && len(step) >= dist) {
                match d.kind {
                    DropKind::Money(v) => {
                        self.xp += v;
                        money = true;
                    }
                    DropKind::Health => {
                        let gain = 30f32.min(self.gear.max_hp - self.hp);
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
            self.juice.burst(self.pos, 10.0, 2, [0.0; 2], 0.0, [10.0, 30.0], juice::GOLD, 0.8, 0.25, 1.0);
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

        let night = (1.0 - self.t / DAWN).max(0.0);
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
                color: juice::FIREFLY,
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

        let health = self.hp / self.gear.max_hp;
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
        if self.hurt < HURT_TIME - 0.35 {
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
        (0.25 + 0.35 * self.t / DAWN + close / 40.0).min(1.0)
    }

    /// Top-left of the view in world pixels, rounded to whole screen pixels.
    fn camera(&self) -> [f32; 2] {
        let x = (self.pos[0] - VIEW_W / 2.0).clamp(0.0, self.map.width_px() - VIEW_W);
        let y = (self.pos[1] - 8.0 - VIEW_H / 2.0).clamp(0.0, self.map.height_px() - VIEW_H);
        [(x * SCALE).round() / SCALE, (y * SCALE).round() / SCALE]
    }

    /// The top screen is the 3D view; the bottom is the map, where a tap throws a grenade.
    pub fn draw(&self, f: &mut Frame) {
        let scene = view3d::scene(self);
        f.top.clear = [scene.fog[0], scene.fog[1], scene.fog[2], 1.0];
        self.popups(&mut f.top, &scene);
        f.top.cards = Some(scene);
        self.vignette(&mut f.top);
        self.hud(&mut f.top);
        self.banner(&mut f.top);
        self.overlays(&mut f.top);

        let c = &mut f.bottom;
        c.clear = [0.0, 0.0, 0.0, 1.0];
        let wash = view3d::sky(self).wash;
        if self.overview {
            let scale = (SCREEN_W / self.map.width_px()).min(SCREEN_H / self.map.height_px());
            self.world(c, [0.0, 0.0], scale);
            c.rect(0.0, 0.0, SCREEN_W, SCREEN_H, wash);
            c.label("Whole map (release L1 to play)", SCREEN_W / 2.0, SCREEN_H - 40.0, 28.0, WHITE, Align::Center);
            return;
        }
        let cam = self.camera();
        self.world(c, cam, SCALE);
        c.rect(0.0, 0.0, SCREEN_W, SCREEN_H, wash);
        self.view_cone(c, cam);
        self.grenade_tray(c);
        if let Phase::LevelUp { choices, selected, t, picked } = &self.phase {
            let fade = picked.map_or(1.0, |p| 1.0 - p / PICK_TIME);
            c.rect(0.0, 0.0, SCREEN_W, SCREEN_H, [0.0, 0.0, 0.0, 0.6 * (t / 0.2).min(1.0) * fade]);
            c.label("Tap an upgrade, or Up / Down and A", SCREEN_W / 2.0, 60.0, 32.0, [1.0, 1.0, 1.0, fade], Align::Center);
            self.choices(c, choices, *selected, *t, *picked, CHOICE_TOP);
        }
    }

    /// Damage numbers over the 3D view, popping up and drifting away.
    fn popups(&self, c: &mut Canvas, scene: &crate::scene3d::CardScene) {
        for p in &self.juice.popups {
            let (rise, pop) = p.rise();
            let Some(([x, y], dist)) = view3d::to_screen(scene.view_proj, p.pos, rise) else { continue };
            let size = 30.0 * pop * (3.5 / dist).clamp(0.45, 1.6);
            let a = p.fade();
            let (color, text) = match p.tone {
                Tone::Hit => ([1.0, 1.0, 1.0, a], p.value.to_string()),
                Tone::Crit => ([1.0, 0.55, 0.15, a], format!("{}!", p.value)),
                Tone::Hurt => ([1.0, 0.25, 0.2, a], format!("-{}", p.value)),
                Tone::Heal => ([0.45, 1.0, 0.5, a], format!("+{}", p.value)),
            };
            c.label(text.clone(), x + 2.0, y + 2.0, size, [0.0, 0.0, 0.0, a * 0.8], Align::Center);
            c.label(text, x, y, size, color, Align::Center);
        }
    }

    /// Red creeping in from the edges: a flash when hit, and a pulse in time with the heart when nearly dead.
    fn vignette(&self, c: &mut Canvas) {
        let health = self.hp / self.gear.max_hp;
        let low = ((0.35 - health) / 0.35).clamp(0.0, 1.0);
        let beat = (1.0 - (self.heartbeat.max(0.0) / (0.5 + health * 2.0)) * 4.0).max(0.0);
        let hit = self.juice.hurt / 0.35;
        let a = (low * (0.3 + 0.4 * beat) + hit * 0.5).min(0.9);
        if hit > 0.0 {
            c.rect(0.0, 0.0, SCREEN_W, SCREEN_H, [0.8, 0.05, 0.05, hit * 0.18]);
        }
        if a <= 0.01 {
            return;
        }
        let red = [0.6, 0.0, 0.0, a / 6.0];
        for i in 1..=6 {
            let t = i as f32 * 26.0;
            c.rect(0.0, 0.0, SCREEN_W, t, red);
            c.rect(0.0, SCREEN_H - t, SCREEN_W, t, red);
            c.rect(0.0, t, t, SCREEN_H - 2.0 * t, red);
            c.rect(SCREEN_W - t, t, t, SCREEN_H - 2.0 * t, red);
        }
    }

    fn banner(&self, c: &mut Canvas) {
        let Some(b) = &self.juice.banner else { return };
        let k = (b.age / 0.18).min(1.0);
        let fade = ((b.life - b.age) / 0.4).clamp(0.0, 1.0);
        let color = [b.color[0], b.color[1], b.color[2], b.color[3] * fade];
        if b.big {
            let scale = 16.0 * back_out(k);
            let y = 300.0 - scale * 2.5;
            c.text_centered(y + scale * 0.4, scale, [0.0, 0.0, 0.0, 0.6 * fade], &b.text);
            c.text_centered(y, scale, color, &b.text);
        } else {
            let x = SCREEN_W / 2.0 - (1.0 - back_out(k)) * 500.0;
            c.rect(0.0, 112.0, SCREEN_W, 64.0, [0.0, 0.0, 0.0, 0.45 * fade * k]);
            c.label(b.text.clone(), x, 120.0, 44.0, color, Align::Center);
        }
    }

    /// Dots along the edges of what the 3D view can see.
    fn view_cone(&self, c: &mut Canvas, cam: [f32; 2]) {
        let half = 0.6;
        for edge in [-half, half] {
            let a = self.view_yaw + edge;
            for k in 1..9 {
                let r = k as f32 * 14.0;
                let (x, y) = ((self.pos[0] + a.cos() * r - cam[0]) * SCALE, (self.pos[1] - 6.0 + a.sin() * r - cam[1]) * SCALE);
                c.circle(x, y, 5.0, [1.0, 0.9, 0.6, 0.8 - k as f32 * 0.08]);
            }
        }
    }

    /// Grenades ready, bottom right, with the next one filling in.
    fn grenade_tray(&self, c: &mut Canvas) {
        let a = art::GRENADE;
        let slot = a.w() * 5.0 + 12.0;
        let panel = GRENADES * slot + 24.0;
        c.rect(SCREEN_W - 40.0 - panel + 12.0, SCREEN_H - 40.0 - a.h() * 5.0, panel, a.h() * 5.0 + 34.0, [0.0, 0.0, 0.0, 0.6]);
        for i in 0..GRENADES as usize {
            let x = SCREEN_W - 40.0 - (i as f32 + 1.0) * slot;
            let y = SCREEN_H - 24.0 - a.h() * 5.0;
            let fill = (self.grenades - i as f32).clamp(0.0, 1.0);
            let color = if fill >= 1.0 { WHITE } else { [0.4, 0.4, 0.4, 0.6] };
            c.art(a, x, y, 5.0, false, color);
            if fill > 0.0 && fill < 1.0 {
                c.rect(x, y + a.h() * 5.0 + 4.0, a.w() * 5.0 * fill, 5.0, GOLD);
            }
        }
    }

    /// Draws the map and everything on it, with `cam` at the top-left and each art pixel `scale` screen pixels across.
    fn world(&self, c: &mut Canvas, cam: [f32; 2], scale: f32) {
        let (view_w, view_h) = (SCREEN_W / scale, SCREEN_H / scale);
        let at = |x: f32, y: f32| ((x - cam[0]) * scale, (y - cam[1]) * scale);

        let (tx0, ty0) = ((cam[0] / TILE) as usize, (cam[1] / TILE) as usize);
        let (tx1, ty1) = (((cam[0] + view_w) / TILE).ceil() as usize, ((cam[1] + view_h) / TILE).ceil() as usize);
        for ty in ty0..ty1.min(self.map.h) {
            for tx in tx0..tx1.min(self.map.w) {
                let (ground, overlay) = self.map.tile(tx, ty);
                let (sx, sy) = at(tx as f32 * TILE, ty as f32 * TILE);
                c.art(ground, sx, sy, scale, false, WHITE);
                if let Some(o) = overlay {
                    c.art(o, sx, sy, scale, false, WHITE);
                }
            }
        }
        let visible = |x: f32, y: f32, w: f32, h: f32| x + w > cam[0] && x < cam[0] + view_w && y + h > cam[1] && y < cam[1] + view_h;
        let stains = self.stains.iter().map(|&(a, x, y, [r, g, b])| (a, x, y, [r, g, b, 1.0]));
        for (a, x, y, color) in self.map.decals.iter().map(|&(a, x, y)| (a, x, y, WHITE)).chain(stains) {
            if visible(x, y, a.w(), a.h()) {
                let (sx, sy) = at(x, y);
                c.art(a, sx, sy, scale, false, color);
            }
        }
        for t in &self.tells {
            let k = (t.t / TELL_TIME).min(1.0);
            let (sx, sy) = at(t.pos[0], t.pos[1]);
            c.circle(sx, sy, (2.0 + 3.0 * k) * scale, [0.2, 0.13, 0.07, 0.8]);
        }
        for corpse in &self.juice.corpses {
            let (sx, sy) = at(corpse.pos[0], corpse.pos[1] - corpse.z - 3.0);
            let turn = if corpse.fall[0] < 0.0 { -1.0 } else { 1.0 };
            let angle = if corpse.z > 0.0 { corpse.spin } else { corpse.tipped() * PI / 2.0 * turn };
            let fade = 1.0 - corpse.sunk() / 8.0;
            c.art_rotated(corpse.art, sx, sy, scale, angle, [0.8, 0.8, 0.8, fade]);
        }
        for d in &self.drops {
            let a = match d.kind {
                DropKind::Money(_) => art::MONEY[((d.age * 10.0) as usize).min(3)],
                DropKind::Health => art::HEALTH,
            };
            let (sx, sy) = at(d.pos[0] - a.w() / 2.0, d.pos[1] - a.h() - d.z);
            c.art(a, sx, sy, scale, false, WHITE);
        }

        // Props, zombies and the player, drawn back to front by where they meet the ground.
        // The last field is how much of each sprite shows: zombies still in the ground show only their top.
        let mut sprites: Vec<(f32, Art, f32, f32, bool, f32)> = Vec::new();
        for p in &self.map.props {
            let a = p.frame(self.t);
            if visible(p.x, p.y, a.w(), a.h()) {
                sprites.push((p.base, a, p.x, p.y, false, 1.0));
            }
        }
        for r in &self.barrels {
            let a = if r.flash > 0.0 { art::BARREL_FLASH } else { art::BARREL };
            if visible(r.pos[0], r.pos[1], TILE, TILE) {
                sprites.push((r.pos[1] + a.h(), a, r.pos[0], r.pos[1], false, 1.0));
            }
        }
        for z in &self.zombies {
            let (facing, flip) = z.facing;
            let a = z.art(facing);
            sprites.push((z.pos[1], a, z.pos[0] - a.w() / 2.0, z.pos[1] - a.h() * z.rise + 2.0, flip, z.rise));
        }
        for b in &self.birds {
            let a = b.art();
            sprites.push((b.pos[1], a, b.pos[0] - a.w() / 2.0, b.pos[1] - a.h() - b.z, b.flip, 1.0));
        }
        let (facing, flip) = self.facing;
        let a = art_for(self, &PLAYER, facing);
        sprites.push((self.pos[1], a, self.pos[0] - a.w() / 2.0, self.pos[1] - a.h() + 2.0, flip, 1.0));
        sprites.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, a, x, y, flip, show) in sprites {
            let (sx, sy) = at(x, y);
            c.art_top(a, sx, sy, scale, flip, show);
        }

        for b in &self.bullets {
            let (sx, sy) = at(b.pos[0], b.pos[1]);
            c.art_rotated(art::BULLET, sx, sy, scale, b.vel[1].atan2(b.vel[0]), WHITE);
        }
        for fx in &self.fx {
            let a = fx.frames[((fx.t * fx.fps) as usize).min(fx.frames.len() - 1)];
            let (sx, sy) = at(fx.pos[0], fx.pos[1]);
            c.art_rotated(a, sx, sy, scale, fx.angle, WHITE);
        }
        for p in &self.juice.particles {
            let s = p.size_now() * scale;
            let (sx, sy) = at(p.pos[0], p.pos[1] - p.z);
            c.rect(sx - s / 2.0, sy - s / 2.0, s, s, [p.color[0], p.color[1], p.color[2], 1.0]);
        }
        for r in &self.juice.rings {
            let rr = r.now();
            let fade = 1.0 - r.age / r.life;
            for i in 0..32 {
                let a = i as f32 / 32.0 * TAU;
                let (sx, sy) = at(r.center[0] + a.cos() * rr, r.center[1] + a.sin() * rr);
                c.circle(sx, sy, 1.5 * scale * fade, [r.color[0], r.color[1], r.color[2], fade]);
            }
        }
        for g in &self.thrown {
            let (p, lift) = g.at();
            let k = (g.t / GRENADE_FLIGHT).min(1.0);
            let (gx, gy) = at(p[0], p[1]);
            c.circle(gx, gy, (2.0 + 2.0 * k) * scale, [0.0, 0.0, 0.0, 0.4]);
            for i in 0..8 {
                let a = g.t * 3.0 + i as f32 / 8.0 * TAU;
                let rr = GRENADE_RADIUS * (1.15 - 0.15 * k);
                let (sx, sy) = at(g.to[0] + a.cos() * rr, g.to[1] + a.sin() * rr);
                c.circle(sx, sy, 1.5 * scale, [1.0, 0.25, 0.15, 0.85]);
            }
            let (sx, sy) = at(p[0], p[1] - 6.0 - lift);
            c.art_rotated(art::GRENADE, sx, sy, scale, g.t * 12.0, WHITE);
        }
    }

    fn overlays(&self, c: &mut Canvas) {
        match &self.phase {
            Phase::Playing => {}
            Phase::LevelUp { choices, selected, t, picked } => {
                let fade = picked.map_or(1.0, |p| 1.0 - p / PICK_TIME);
                c.rect(0.0, 0.0, SCREEN_W, SCREEN_H, [0.0, 0.0, 0.0, 0.55 * (t / 0.2).min(1.0) * fade]);
                if self.glory > 0.0 {
                    c.rect(0.0, 0.0, SCREEN_W, SCREEN_H, [1.0, 0.9, 0.6, self.glory / 0.25 * 0.5]);
                }
                // Embers drifting up behind the title.
                for i in 0..24 {
                    let seed = i as f32 * 12.9898;
                    let x = (seed.sin() * 43758.545).fract().abs() * SCREEN_W;
                    let y = SCREEN_H - ((t * (40.0 + i as f32 * 7.0) + seed * 50.0) % (SCREEN_H + 40.0));
                    c.circle(x, y, 3.0 + (i % 3) as f32 * 2.0, [1.0, 0.8, 0.3, 0.5 * fade]);
                }
                let scale = 12.0 * back_out((t / 0.3).min(1.0)) + (t * 4.0).sin() * 0.4;
                c.text_centered(150.0 - (scale - 12.0) * 2.5, scale, [GOLD[0], GOLD[1], GOLD[2], fade], "LEVEL UP");
                self.choices(c, choices, *selected, *t, *picked, 270.0);
            }
            Phase::Over { t, won } => {
                let k = (t / 0.6).min(1.0);
                c.rect(0.0, 0.0, SCREEN_W, SCREEN_H, [0.0, 0.0, 0.0, 0.6 * k]);
                let (title, color) = if *won { ("DAWN", GOLD) } else { ("YOU DIED", RED) };
                let scale = 16.0 * back_out(k);
                c.text_centered(260.0 - scale * 2.5, scale, color, title);
                let a = ((t - 0.5) / 0.4).clamp(0.0, 1.0);
                let line = format!("Survived {}   Level {}   Kills {}", clock(self.t), self.level, self.kills);
                c.label(line, SCREEN_W / 2.0, 360.0, 40.0, [1.0, 1.0, 1.0, a], Align::Center);
                c.label("A: try another farm      B: menu", SCREEN_W / 2.0, 440.0, 32.0, [1.0, 1.0, 1.0, 0.7 * a], Align::Center);
            }
        }
    }

    fn hud(&self, c: &mut Canvas) {
        // XP across the top, with a bright leading edge; it flashes while full.
        let k = (self.xp_shown / self.next_xp as f32).clamp(0.0, 1.0);
        c.rect(0.0, 0.0, SCREEN_W, 14.0, [0.0, 0.0, 0.0, 0.6]);
        let glint = (1.0 - self.chain.0 / 0.15).max(0.0);
        let bar = if self.leveling > 0.0 && (self.juice.clock * 16.0).sin() > 0.0 {
            WHITE
        } else {
            [GOLD[0], GOLD[1] + (1.0 - GOLD[1]) * glint, GOLD[2] + (1.0 - GOLD[2]) * glint, 1.0]
        };
        c.rect(0.0, 0.0, SCREEN_W * k, 14.0, bar);
        if k > 0.01 && k < 1.0 {
            c.rect(SCREEN_W * k - 8.0, 0.0, 8.0, 14.0, [1.0, 1.0, 0.85, 0.9]);
        }
        // Health, with a pale chunk trailing what was just lost; it throbs when low.
        let (bx, by, bw, bh) = (24.0, 30.0, 240.0, 22.0);
        let max = self.gear.max_hp;
        c.rect(bx - 3.0, by - 3.0, bw + 6.0, bh + 6.0, [0.0, 0.0, 0.0, 0.7]);
        c.rect(bx, by, bw * (self.hp_shown / max).clamp(0.0, 1.0), bh, [1.0, 0.9, 0.8, 0.85]);
        let health = self.hp / max;
        let throb = if health < 0.3 { 0.5 + 0.5 * (self.juice.clock * 10.0).sin() } else { 0.0 };
        c.rect(bx, by, bw * health.clamp(0.0, 1.0), bh, [RED[0] + 0.15 * throb, RED[1] + 0.1 * throb, RED[2], 1.0]);
        c.label(format!("LV {}", self.level), bx, by + 30.0, 32.0, WHITE, Align::Left);
        c.text_centered(30.0, 6.0, WHITE, &clock(self.t));
        let bump = 1.0 + 0.35 * (1.0 - self.kill_bump / 0.15).max(0.0);
        c.label(format!("{} kills", self.kills), SCREEN_W - 24.0, 28.0 - 8.0 * (bump - 1.0), 32.0 * bump, WHITE, Align::Right);
    }

    /// The upgrade cards: each slides in a beat after the last, the selected one breathes, rare ones shimmer gold,
    /// and a pick flashes white while the rest fade.
    fn choices(&self, c: &mut Canvas, choices: &[Choice; 3], selected: usize, t: f32, picked: Option<f32>, top: f32) {
        let fade = picked.map_or(1.0, |p| 1.0 - p / PICK_TIME);
        for (i, ch) in choices.iter().enumerate() {
            let k = back_out(((t - i as f32 * 0.07) / 0.28).clamp(0.0, 1.0));
            let on = i == selected;
            let grow = if on && picked.is_none() { 4.0 + 4.0 * (t * 6.0).sin() } else { 0.0 };
            let (x, y) = (112.0 + (1.0 - k) * 1000.0 - grow, top + i as f32 * CHOICE_ROW - grow / 2.0);
            let (w, h) = (800.0 + grow * 2.0, CHOICE_ROW - 20.0 + grow);
            let alpha = if on { 1.0 } else { fade };
            if ch.rare {
                let glint = 0.6 + 0.4 * (t * 5.0).sin();
                c.rect(x - 5.0, y - 5.0, w + 10.0, h + 10.0, [GOLD[0], GOLD[1], GOLD[2], glint * alpha]);
                c.rect(x, y, w, h, [0.1, 0.08, 0.02, alpha]);
            }
            let fill = match picked {
                Some(p) if on => [1.0, 1.0, 1.0, 0.8 * (1.0 - p / PICK_TIME)],
                _ if on => [1.0, 0.85, 0.3, 0.35],
                _ => [1.0, 1.0, 1.0, 0.1 * alpha],
            };
            c.rect(x, y, w, h, fill);
            if ch.rare {
                let band = x + (t * 0.9).fract() * (w + 60.0) - 60.0;
                let (b0, b1) = (band.max(x), (band + 60.0).min(x + w));
                if b1 > b0 {
                    c.rect(b0, y, b1 - b0, h, [1.0, 1.0, 0.9, 0.12 * alpha]);
                }
                c.label("RARE  x2", x + w - 28.0, y + 16.0, 30.0, [GOLD[0], GOLD[1], GOLD[2], alpha], Align::Right);
            }
            let (name, blurb) = ch.up.text(&self.gear);
            let name_color = if on { GOLD } else { WHITE };
            c.label(name, x + 28.0, y + 12.0, 40.0, [name_color[0], name_color[1], name_color[2], alpha], Align::Left);
            c.label(blurb, x + 28.0, y + 62.0, 28.0, [1.0, 1.0, 1.0, 0.75 * alpha], Align::Left);
        }
    }
}

/// Eases from 0 to 1, overshooting a little before settling.
fn back_out(k: f32) -> f32 {
    let c = 1.70158;
    let k = k - 1.0;
    1.0 + k * k * ((c + 1.0) * k + c)
}

const CHOICE_TOP: f32 = 160.0;
const CHOICE_ROW: f32 = 150.0;

/// Which level-up row a bottom-screen touch at `y` lands on.
fn choice_row(y: f32) -> Option<usize> {
    let row = ((y - CHOICE_TOP) / CHOICE_ROW).floor();
    (0.0..3.0).contains(&row).then_some(row as usize)
}

/// Lands a hit: damage (now and then doubled), knockback, a red flash, a squash, a number and a spray of blood.
/// Returns whether it was a critical hit.
fn strike(z: &mut Zombie, juice: &mut Juice, damage: f32, dir: [f32; 2], force: f32) -> bool {
    let crit = juice.rng.range(0.0, 1.0) < CRIT_CHANCE;
    let dealt = if crit { damage * 2.0 } else { damage };
    z.hp -= dealt;
    z.flash = FLASH_TIME;
    z.hurt = HURT_FRAMES;
    z.squash = 1.0;
    z.knock = dir;
    let k = force / z.kind.stats().mass;
    z.push = [dir[0] * k, dir[1] * k];
    juice.popup(z.pos, dealt, if crit { Tone::Crit } else { Tone::Hit });
    if z.kind == Kind::Big && (crit || force > 100.0) {
        z.stagger = STAGGER_TIME;
    }
    juice.burst(z.pos, 8.0, if crit { 6 } else { 3 }, dir, 0.6, [30.0, 90.0], juice::BLOOD, 1.4, 0.4, 0.0);
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
    let k = (t / DAWN).clamp(0.0, 1.0);
    [0, 1].map(|i| SPAWN_RING_DUSK[i] + (SPAWN_RING_DAWN[i] - SPAWN_RING_DUSK[i]) * k)
}

fn clock(t: f32) -> String {
    let s = t as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(h: &mut Harvest, input: &Input, secs: f32) {
        let mut sounds = Vec::new();
        for _ in 0..(secs * 60.0) as usize {
            h.update(input, 1.0 / 60.0, &mut sounds);
            if let Phase::LevelUp { .. } = h.phase {
                h.update(&Input { pressed: Buttons::A, ..Default::default() }, 1.0 / 60.0, &mut sounds);
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
    fn y_and_a_or_the_shoulder_triggers_turn_without_moving() {
        for (left, right) in [(Buttons::Y, Buttons::A), (Buttons::L2, Buttons::R2)] {
            let mut h = Harvest::calm(9);
            run(&mut h, &Input::default(), 0.05);
            let (start, yaw) = (h.pos, h.view_yaw);
            run(&mut h, &Input { held: right, ..Default::default() }, 0.25);
            assert_eq!(h.pos, start);
            assert!((h.view_yaw - yaw).rem_euclid(TAU) > 0.5);
            run(&mut h, &Input { held: left, ..Default::default() }, 0.25);
            assert!((h.view_yaw - yaw).rem_euclid(TAU) < 0.01 || (h.view_yaw - yaw).rem_euclid(TAU) > TAU - 0.01);
        }
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
    fn b_turns_all_the_way_around_quickly() {
        let mut h = Harvest::calm(9);
        let yaw = h.view_yaw;
        h.update(&Input { pressed: Buttons::B, ..Default::default() }, 1.0 / 60.0, &mut Vec::new());
        run(&mut h, &Input::default(), SPIN_TIME + 0.05);
        assert!(((h.view_yaw - yaw).rem_euclid(TAU) - PI).abs() < 1e-3);
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
        h.zombies.push(Zombie::new(Kind::Skinny, target, 20.0, 1.0, 0.0));
        let touch = [(target[0] - cam[0]) * SCALE, (target[1] - cam[1]) * SCALE];
        let mut sounds = Vec::new();
        h.update(&Input { touch: Some(touch), touch_started: true, ..Default::default() }, 0.016, &mut sounds);
        assert_eq!(h.thrown.len(), 1);
        assert!(h.grenades < GRENADES);
        h.update(&Input::default(), GRENADE_FLIGHT, &mut sounds);
        assert!(h.thrown.is_empty());
        assert!(h.kills == 1 || h.zombies.iter().all(|z| z.hp <= 0.0));
    }

    #[test]
    fn a_tap_picks_an_upgrade() {
        let mut h = Harvest::new(2);
        let choice = |up| Choice { up, rare: false };
        h.phase = Phase::LevelUp { choices: [choice(Upgrade::Shotgun), choice(Upgrade::Knife), choice(Upgrade::Speed)], selected: 0, t: 1.0, picked: None };
        let tap = Input { touch: Some([500.0, CHOICE_TOP + CHOICE_ROW * 1.5]), touch_started: true, ..Default::default() };
        h.update(&tap, 0.016, &mut Vec::new());
        assert_eq!(h.gear.knife, 1);
        h.update(&Input::default(), PICK_TIME, &mut Vec::new());
        assert!(matches!(h.phase, Phase::Playing));
    }

    #[test]
    fn a_shot_barrel_blows_up_nearby_zombies_and_opens_its_cell() {
        let mut h = Harvest::new(6);
        let barrel = h.barrels.first().expect("seed 6 has barrels");
        let c = barrel.center();
        h.zombies.push(Zombie::new(Kind::Big, [c[0] + 20.0, c[1]], 50.0, 1.0, 0.0));
        let mut sounds = Vec::new();
        h.bullets.push(Bullet { pos: [c[0], c[1] - 6.0], vel: [1.0, 0.0], damage: 50.0, pierce: 0, life: 1.0, hit: Vec::new() });
        h.move_bullets(0.016, &mut sounds);
        assert!(h.blow(0.016, &mut sounds));
        assert!(h.zombies[0].hp <= 0.0);
        assert!(!h.map.solid_at(c[0], c[1]));
    }

    #[test]
    fn the_spawn_ring_closes_in_toward_dawn() {
        assert_eq!(spawn_ring(0.0), SPAWN_RING_DUSK);
        assert_eq!(spawn_ring(DAWN), SPAWN_RING_DAWN);
        assert!(spawn_ring(DAWN / 2.0)[0] < SPAWN_RING_DUSK[0]);
    }

    #[test]
    fn a_rising_zombie_is_rooted_and_harmless_until_it_is_out() {
        let mut h = Harvest::calm(3);
        let at = [h.pos[0] + 4.0, h.pos[1]];
        h.zombies.push(Zombie::new(Kind::Skinny, at, 1000.0, 0.0, 0.0));
        let mut sounds = Vec::new();
        h.update(&Input::default(), RISE_TIME * 0.5, &mut sounds);
        assert_eq!(h.zombies[0].pos, at);
        assert_eq!(h.hp, h.gear.max_hp);
        for _ in 0..60 {
            h.update(&Input::default(), 1.0 / 60.0, &mut sounds);
        }
        assert!(h.hp < h.gear.max_hp);
    }

    #[test]
    fn dying_offers_a_new_farm() {
        let mut h = Harvest::new(4);
        h.hp = 0.0;
        h.update(&Input::default(), 0.016, &mut Vec::new());
        assert!(matches!(h.phase, Phase::Over { won: false, .. }));
        h.update(&Input::default(), 1.5, &mut Vec::new());
        h.update(&Input { pressed: Buttons::A, ..Default::default() }, 0.016, &mut Vec::new());
        assert!(matches!(h.phase, Phase::Playing));
        assert_eq!(h.kills, 0);
    }

    #[test]
    fn a_kill_drops_a_body_a_number_and_money_that_hops() {
        let mut h = Harvest::calm(3);
        let at = [-30.0, 30.0, 0.0].into_iter().flat_map(|dx| [-30.0, 30.0].map(|dy| [h.pos[0] + dx, h.pos[1] + dy])).find(|p| !h.map.hits(p[0], p[1], ZOMBIE_HALF)).unwrap();
        h.zombies.push(Zombie::new(Kind::Skinny, at, 5.0, 1.0, 0.0));
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
        h.zombies.push(Zombie::new(Kind::Skinny, at, 5.0, 1.0, 0.0));
        h.zombies[0].hp = 0.0;
        h.reap(&mut sounds, &mut Flow::default());
        assert!(h.drops[0].vz > 0.0, "money hops out of the body");
    }

    #[test]
    fn a_hit_shows_a_number_and_shoves_light_zombies_further() {
        let mut j = Juice::new(1);
        let mut kid = Zombie::new(Kind::Kid, [0.0; 2], 100.0, 1.0, 0.0);
        let mut big = Zombie::new(Kind::Big, [0.0; 2], 100.0, 1.0, 0.0);
        strike(&mut kid, &mut j, 10.0, [1.0, 0.0], 30.0);
        strike(&mut big, &mut j, 10.0, [1.0, 0.0], 30.0);
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
        let a = Input { pressed: Buttons::A, ..Default::default() };
        h.update(&a, 1.0 / 60.0, &mut sounds);
        assert!(matches!(h.phase, Phase::LevelUp { picked: None, .. }), "too soon");
        h.update(&Input::default(), CARDS_IN, &mut sounds);
        h.update(&a, 1.0 / 60.0, &mut sounds);
        assert!(matches!(h.phase, Phase::LevelUp { picked: Some(_), .. }));
        h.update(&Input::default(), PICK_TIME, &mut sounds);
        assert!(matches!(h.phase, Phase::Playing));
    }

    #[test]
    fn getting_hit_freezes_the_world_for_a_moment() {
        let mut h = Harvest::calm(3);
        h.zombies.push(Zombie::new(Kind::Skinny, [h.pos[0] + 3.0, h.pos[1]], 1000.0, 1.0, 0.0));
        let mut sounds = Vec::new();
        h.update(&Input::default(), 1.0 / 60.0, &mut sounds);
        assert!(h.hp < h.gear.max_hp && h.juice.frozen());
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
    fn a_blast_staggers_a_big_zombie_in_place() {
        let mut j = Juice::new(1);
        let mut big = Zombie::new(Kind::Big, [0.0; 2], 500.0, 1.0, 0.0);
        strike(&mut big, &mut j, 10.0, [1.0, 0.0], 30.0);
        let stagger = big.stagger;
        strike(&mut big, &mut j, 10.0, [1.0, 0.0], 120.0);
        assert!(big.stagger > stagger && big.stagger == STAGGER_TIME);
        let mut h = Harvest::calm(3);
        let at = [h.pos[0] + 60.0, h.pos[1]];
        h.zombies.push(big);
        h.zombies[0].pos = at;
        h.move_zombies(0.1);
        assert_eq!(h.zombies[0].pos, at);
    }
}
