//! Last Harvest: survive the night on a randomly generated farm while the horde closes in.

mod map;
mod view3d;

use std::f32::consts::{PI, TAU};

use crate::art::{self, Art};
use crate::rng::Rng;
use crate::{Align, Buttons, Canvas, Color, Frame, Input, SCREEN_H, SCREEN_W, Sfx};
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

/// Walk frames for each facing, then the hurt and flash frames for each facing.
struct Look {
    walk: [&'static [Art; 3]; 3],
    flash: &'static [Art; 3],
}

const PLAYER: Look = Look { walk: [&art::PLAYER_FRONT, &art::PLAYER_SIDE, &art::PLAYER_BACK], flash: &art::PLAYER_FLASH };

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
    look: Look,
}

impl Kind {
    fn stats(self) -> KindStats {
        match self {
            Kind::Skinny => KindStats { hp: 20.0, speed: 20.0, damage: 8.0, xp: 1, look: Look { walk: [&art::SKINNY_FRONT, &art::SKINNY_SIDE, &art::SKINNY_BACK], flash: &art::SKINNY_FLASH } },
            Kind::Kid => KindStats { hp: 10.0, speed: 32.0, damage: 5.0, xp: 1, look: Look { walk: [&art::KID_FRONT, &art::KID_SIDE, &art::KID_BACK], flash: &art::KID_FLASH } },
            Kind::Big => KindStats { hp: 90.0, speed: 12.0, damage: 18.0, xp: 5, look: Look { walk: [&art::BIG_FRONT, &art::BIG_SIDE, &art::BIG_BACK], flash: &art::BIG_FLASH } },
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
    facing: (Facing, bool),
    anim: f32,
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
    age: f32,
    pulled: bool,
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

#[derive(Clone, Copy)]
enum Phase {
    Playing,
    LevelUp { choices: [Upgrade; 3], selected: usize },
    Over { t: f32, won: bool },
}

/// What the run asks of the rest of the game after an update.
#[derive(Default)]
pub struct Flow {
    pub rumble: bool,
    pub quit: bool,
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
    stains: Vec<(Art, f32, f32)>,
    barrels: Vec<Barrel>,
    /// Grenades ready to throw; the fraction is the next one recharging.
    grenades: f32,
    thrown: Vec<Grenade>,
    owed: f32,
    xp: u32,
    next_xp: u32,
    level: u32,
    kills: u32,
    /// Holding R2 pauses and shows the whole map.
    overview: bool,
    /// Steps to the player from each cell, rebuilt when the player changes cell.
    flow: Vec<u16>,
    flow_cell: [usize; 2],
    /// The player's heading, which the 3D camera looks along, in radians from +x toward +y.
    view_yaw: f32,
    /// No zombies: a walk around the farm, for looking at it.
    calm: bool,
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
            calm: false,
        }
    }

    pub fn calm(seed: u64) -> Self {
        Self { calm: true, ..Self::new(seed) }
    }

    pub fn summary(&self) -> String {
        format!("harvest t={:.0} level={} kills={} zombies={} hp={:.0}", self.t, self.level, self.kills, self.zombies.len(), self.hp)
    }

    pub fn update(&mut self, input: &Input, dt: f32, sounds: &mut Vec<Sfx>) -> Flow {
        let mut flow = Flow::default();
        self.overview = input.held.contains(Buttons::R2);
        if self.overview {
            return flow;
        }
        match self.phase {
            Phase::Playing => self.play(input, dt, sounds, &mut flow),
            Phase::LevelUp { choices, selected } => {
                let n = choices.len();
                let mut selected = selected;
                if input.pressed.any(Buttons::DOWN | Buttons::RIGHT) {
                    selected = (selected + 1) % n;
                    sounds.push(Sfx::Tick(selected as u8 * 3));
                }
                if input.pressed.any(Buttons::UP | Buttons::LEFT) {
                    selected = (selected + n - 1) % n;
                    sounds.push(Sfx::Tick(selected as u8 * 3));
                }
                let tapped = input.touch.filter(|_| input.touch_started).and_then(|[_, y]| choice_row(y));
                match tapped.or(input.pressed.any(Buttons::A | Buttons::START).then_some(selected)) {
                    Some(i) => {
                        let heal = self.gear.apply(choices[i]);
                        self.hp = (self.hp + heal).min(self.gear.max_hp);
                        sounds.push(Sfx::Start);
                        self.phase = Phase::Playing;
                    }
                    None => self.phase = Phase::LevelUp { choices, selected },
                }
            }
            Phase::Over { t, won } => {
                let t = t + dt;
                self.phase = Phase::Over { t, won };
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

    fn play(&mut self, input: &Input, dt: f32, sounds: &mut Vec<Sfx>, flow: &mut Flow) {
        self.t += dt;
        self.hurt = (self.hurt - dt).max(0.0);
        // Tank controls: left and right turn, up and down walk along the heading, L1 and R1 strafe.
        let dir = input.dir();
        self.view_yaw = (self.view_yaw + dir[0] * TURN_SPEED * dt).rem_euclid(TAU);
        let f = [self.view_yaw.cos(), self.view_yaw.sin()];
        let side = input.held.contains(Buttons::R1) as i32 as f32 - input.held.contains(Buttons::L1) as i32 as f32;
        let ahead = -dir[1];
        let mv = [f[0] * ahead - f[1] * side, f[1] * ahead + f[0] * side];
        self.aim = f;
        self.facing = Facing::of(f);
        if len(mv) > 0.2 {
            let k = self.gear.speed * dt * len(mv).min(1.0) / len(mv);
            self.pos = self.map.slide(self.pos, [mv[0] * k, mv[1] * k], PLAYER_HALF);
            self.walk += dt;
        } else {
            self.walk = 0.0;
        }
        self.grenades = (self.grenades + dt / GRENADE_RECHARGE).min(GRENADES);
        if let Some(touch) = input.touch.filter(|_| input.touch_started) {
            self.throw(touch, sounds);
        }
        self.fly(dt, sounds, flow);

        if !self.calm {
            self.spawn(dt);
        }
        self.move_zombies(dt);
        self.contact(sounds, flow);
        self.fire(dt, sounds);
        self.move_bullets(dt);
        if self.blow(dt, sounds) {
            flow.rumble = true;
        }
        self.reap(sounds);
        self.collect(dt, sounds);
        for f in &mut self.fx {
            f.t += dt;
        }
        self.fx.retain(|f| !f.done());

        if self.hp <= 0.0 {
            self.hp = 0.0;
            sounds.push(Sfx::GameOver);
            self.phase = Phase::Over { t: 0.0, won: false };
        } else if self.t >= DAWN {
            sounds.push(Sfx::Nice);
            self.phase = Phase::Over { t: 0.0, won: true };
        } else if self.xp >= self.next_xp {
            self.xp -= self.next_xp;
            self.next_xp = (self.next_xp as f32 * 1.35).ceil() as u32 + 2;
            self.level += 1;
            sounds.push(Sfx::Nice);
            self.phase = Phase::LevelUp { choices: self.roll_choices(), selected: 0 };
        }
    }

    fn roll_choices(&mut self) -> [Upgrade; 3] {
        let mut pool: Vec<Upgrade> = Upgrade::ALL.to_vec();
        let mut out = [Upgrade::Damage; 3];
        for slot in &mut out {
            *slot = pool.swap_remove(self.rng.below(pool.len()));
        }
        out
    }

    /// Zombies arrive just out of sight, faster and tougher as the night goes on.
    fn spawn(&mut self, dt: f32) {
        let minutes = self.t / 60.0;
        self.owed += (0.7 + minutes * 0.9).min(9.0) * dt;
        // A horde every ninety seconds.
        if (self.t - dt).rem_euclid(90.0) > self.t.rem_euclid(90.0) {
            self.owed += 20.0 + minutes * 4.0;
        }
        while self.owed >= 1.0 && self.zombies.len() < MAX_ZOMBIES {
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
                    let s = kind.stats();
                    self.zombies.push(Zombie { kind, pos, dir: [0.0, 1.0], rise: 0.0, push: [0.0; 2], hp: s.hp * (1.0 + minutes * 0.08), flash: 0.0, facing: (Facing::Front, false), anim: self.rng.range(0.0, 1.0) });
                    self.fx.push(Fx { frames: &art::SMOKE, fps: 7.0, pos: [pos[0], pos[1] - 3.0], angle: 0.0, t: 0.0 });
                    break;
                }
            }
        }
        self.owed = self.owed.min(40.0);
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
            if z.rise < 1.0 {
                z.rise = (z.rise + dt / RISE_TIME).min(1.0);
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
            flow.rumble = true;
            sounds.push(Sfx::Hit);
        }
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
            sounds.push(Sfx::Clack(18));
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
            sounds.push(Sfx::Pop);
        }

        if self.gear.knife > 0 && self.gear.cooldowns[2] <= 0.0 {
            let reach = 18.0 + 4.0 * self.gear.knife as f32;
            if let Some(d) = aim_at(reach, self).or_else(|| (self.nearest(reach + 6.0).is_some()).then_some(self.aim)) {
                self.gear.cooldowns[2] = 1.1 * self.gear.rate;
                let damage = (10.0 + 5.0 * self.gear.knife as f32) * self.gear.damage;
                for z in &mut self.zombies {
                    if len(sub(z.pos, self.pos)) < reach {
                        z.hp -= damage;
                        z.flash = FLASH_TIME;
                        let away = norm(sub(z.pos, self.pos));
                        z.push = [away[0] * 60.0, away[1] * 60.0];
                    }
                }
                let at = [self.pos[0] + d[0] * 8.0, self.pos[1] - 5.0 + d[1] * 8.0];
                self.fx.push(Fx { frames: &art::SLASH, fps: 20.0, pos: at, angle: d[1].atan2(d[0]) - PI / 2.0, t: 0.0 });
                sounds.push(Sfx::Catch);
            }
        }
    }

    fn shoot(&mut self, angle: f32, speed: f32, damage: f32, pierce: u32, life: f32) {
        let vel = [angle.cos() * speed, angle.sin() * speed];
        self.bullets.push(Bullet { pos: [self.pos[0], self.pos[1] - 6.0], vel, damage, pierce, life, hit: Vec::new() });
    }

    fn move_bullets(&mut self, dt: f32) {
        for b in &mut self.bullets {
            b.pos = [b.pos[0] + b.vel[0] * dt, b.pos[1] + b.vel[1] * dt];
            b.life -= dt;
            if self.map.solid_at(b.pos[0], b.pos[1] + 6.0) {
                let cell = |p: [f32; 2]| [(p[0] / TILE).floor(), (p[1] / TILE).floor()];
                let at = cell([b.pos[0], b.pos[1] + 6.0]);
                if let Some(barrel) = self.barrels.iter_mut().find(|r| cell(r.pos) == at) {
                    barrel.hp -= b.damage;
                    barrel.flash = FLASH_TIME;
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
                    z.hp -= b.damage;
                    z.flash = FLASH_TIME;
                    let k = 30.0 / len(b.vel).max(1.0);
                    z.push = [b.vel[0] * k, b.vel[1] * k];
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
        for z in &mut self.zombies {
            let d = sub(z.pos, c);
            if len(d) < radius {
                z.hp -= damage * self.gear.damage;
                z.flash = FLASH_TIME;
                let away = norm(d);
                z.push = [away[0] * 120.0, away[1] * 120.0];
            }
        }
        for r in &mut self.barrels {
            if len(sub(r.center(), c)) < radius {
                r.hp = r.hp.min(0.0);
            }
        }
        if len(sub(self.pos, c)) < radius * 0.7 && self.hurt <= 0.0 {
            self.hp -= 15.0;
            self.hurt = HURT_TIME;
        }
        self.fx.push(Fx { frames: &art::EXPLOSION, fps: 14.0, pos: c, angle: 0.0, t: 0.0 });
        self.fx.push(Fx { frames: &art::SMOKE, fps: 8.0, pos: [c[0], c[1] - 8.0], angle: 0.0, t: 0.0 });
        sounds.push(Sfx::Hit);
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
        sounds.push(Sfx::Prompt);
    }

    fn fly(&mut self, dt: f32, sounds: &mut Vec<Sfx>, flow: &mut Flow) {
        for g in &mut self.thrown {
            g.t += dt;
        }
        let landed: Vec<[f32; 2]> = self.thrown.iter().filter(|g| g.t >= GRENADE_FLIGHT).map(|g| g.to).collect();
        self.thrown.retain(|g| g.t < GRENADE_FLIGHT);
        for c in landed {
            self.explode(c, GRENADE_RADIUS, 50.0, sounds);
            flow.rumble = true;
        }
    }

    fn reap(&mut self, sounds: &mut Vec<Sfx>) {
        let mut died = false;
        let mut i = 0;
        while i < self.zombies.len() {
            if self.zombies[i].hp > 0.0 {
                i += 1;
                continue;
            }
            let z = self.zombies.swap_remove(i);
            self.kills += 1;
            died = true;
            self.fx.push(Fx { frames: &art::BLOOD_SPLAT, fps: 12.0, pos: [z.pos[0], z.pos[1] - 4.0], angle: 0.0, t: 0.0 });
            let stain = art::BLOOD_STAIN[self.rng.below(art::BLOOD_STAIN.len())];
            self.stains.push((stain, z.pos[0] - stain.w() / 2.0, z.pos[1] - stain.h() / 2.0));
            if self.stains.len() > MAX_STAINS {
                self.stains.remove(0);
            }
            let kind = if self.rng.below(60) == 0 { DropKind::Health } else { DropKind::Money(z.kind.stats().xp) };
            self.drops.push(Drop { kind, pos: z.pos, age: 0.0, pulled: false });
        }
        if died {
            sounds.push(Sfx::Tick(0));
        }
    }

    fn collect(&mut self, dt: f32, sounds: &mut Vec<Sfx>) {
        let mut got = false;
        for d in &mut self.drops {
            d.age += dt;
            let to = sub(self.pos, d.pos);
            let dist = len(to);
            if dist < self.gear.magnet {
                d.pulled = true;
            }
            if d.pulled {
                let speed = 90.0 + 200.0 * d.age.min(2.0);
                let n = norm(to);
                d.pos = [d.pos[0] + n[0] * speed * dt, d.pos[1] + n[1] * speed * dt];
            }
            if dist < PICKUP_RADIUS {
                match d.kind {
                    DropKind::Money(v) => self.xp += v,
                    DropKind::Health => self.hp = (self.hp + 30.0).min(self.gear.max_hp),
                }
                d.age = -1.0;
                got = true;
            }
        }
        self.drops.retain(|d| d.age >= 0.0);
        if got {
            sounds.push(Sfx::Catch);
        }
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
        f.top.cards = Some(scene);
        self.hud(&mut f.top);
        self.overlays(&mut f.top);

        let c = &mut f.bottom;
        c.clear = [0.0, 0.0, 0.0, 1.0];
        if self.overview {
            let scale = (SCREEN_W / self.map.width_px()).min(SCREEN_H / self.map.height_px());
            self.world(c, [0.0, 0.0], scale);
            c.label("Whole map (release R2 to play)", SCREEN_W / 2.0, SCREEN_H - 40.0, 28.0, WHITE, Align::Center);
            return;
        }
        let cam = self.camera();
        self.world(c, cam, SCALE);
        self.view_cone(c, cam);
        self.grenade_tray(c);
        if let Phase::LevelUp { choices, selected } = &self.phase {
            c.rect(0.0, 0.0, SCREEN_W, SCREEN_H, [0.0, 0.0, 0.0, 0.6]);
            c.label("Tap an upgrade, or Up / Down and A", SCREEN_W / 2.0, 60.0, 32.0, WHITE, Align::Center);
            self.choices(c, choices, *selected, CHOICE_TOP);
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
        for &(a, x, y) in self.map.decals.iter().chain(&self.stains) {
            if visible(x, y, a.w(), a.h()) {
                let (sx, sy) = at(x, y);
                c.art(a, sx, sy, scale, false, WHITE);
            }
        }
        for d in &self.drops {
            let a = match d.kind {
                DropKind::Money(_) => art::MONEY[((d.age * 10.0) as usize).min(3)],
                DropKind::Health => art::HEALTH,
            };
            let (sx, sy) = at(d.pos[0] - a.w() / 2.0, d.pos[1] - a.h());
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
            let s = z.kind.stats();
            let (facing, flip) = z.facing;
            let a = if z.flash > 0.0 { s.look.flash[facing.index()] } else { s.look.walk[facing.index()][z.frame()] };
            sprites.push((z.pos[1], a, z.pos[0] - a.w() / 2.0, z.pos[1] - a.h() * z.rise + 2.0, flip, z.rise));
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
        for g in &self.thrown {
            let (p, lift) = g.at();
            let (sx, sy) = at(p[0], p[1] - 6.0 - lift);
            c.art_rotated(art::GRENADE, sx, sy, scale, g.t * 12.0, WHITE);
        }
    }

    fn overlays(&self, c: &mut Canvas) {
        match &self.phase {
            Phase::Playing => {}
            Phase::LevelUp { choices, selected } => {
                c.rect(0.0, 0.0, SCREEN_W, SCREEN_H, [0.0, 0.0, 0.0, 0.55]);
                c.text_centered(150.0, 12.0, GOLD, "LEVEL UP");
                self.choices(c, choices, *selected, 270.0);
            }
            Phase::Over { won, .. } => {
                c.rect(0.0, 0.0, SCREEN_W, SCREEN_H, [0.0, 0.0, 0.0, 0.6]);
                let (title, color) = if *won { ("DAWN", GOLD) } else { ("YOU DIED", RED) };
                c.text_centered(220.0, 16.0, color, title);
                let line = format!("Survived {}   Level {}   Kills {}", clock(self.t), self.level, self.kills);
                c.label(line, SCREEN_W / 2.0, 360.0, 40.0, WHITE, Align::Center);
                c.label("A: try another farm      B: menu", SCREEN_W / 2.0, 440.0, 32.0, [1.0, 1.0, 1.0, 0.7], Align::Center);
            }
        }
    }

    fn hud(&self, c: &mut Canvas) {
        c.rect(0.0, 0.0, SCREEN_W, 14.0, [0.0, 0.0, 0.0, 0.6]);
        c.rect(0.0, 0.0, SCREEN_W * self.xp as f32 / self.next_xp as f32, 14.0, GOLD);
        let (bx, by, bw, bh) = (24.0, 30.0, 240.0, 22.0);
        c.rect(bx - 3.0, by - 3.0, bw + 6.0, bh + 6.0, [0.0, 0.0, 0.0, 0.7]);
        c.rect(bx, by, bw * (self.hp / self.gear.max_hp).clamp(0.0, 1.0), bh, RED);
        c.label(format!("LV {}", self.level), bx, by + 30.0, 32.0, WHITE, Align::Left);
        c.text_centered(30.0, 6.0, WHITE, &clock(self.t));
        c.label(format!("{} kills", self.kills), SCREEN_W - 24.0, 28.0, 32.0, WHITE, Align::Right);
    }

    fn choices(&self, c: &mut Canvas, choices: &[Upgrade; 3], selected: usize, top: f32) {
        for (i, up) in choices.iter().enumerate() {
            let y = top + i as f32 * CHOICE_ROW;
            let on = i == selected;
            c.rect(112.0, y, 800.0, CHOICE_ROW - 20.0, if on { [1.0, 0.85, 0.3, 0.35] } else { [1.0, 1.0, 1.0, 0.1] });
            let (name, blurb) = up.text(&self.gear);
            c.label(name, 140.0, y + 12.0, 40.0, if on { GOLD } else { WHITE }, Align::Left);
            c.label(blurb, 140.0, y + 62.0, 28.0, [1.0, 1.0, 1.0, 0.75], Align::Left);
        }
    }

}

const CHOICE_TOP: f32 = 160.0;
const CHOICE_ROW: f32 = 150.0;

/// Which level-up row a bottom-screen touch at `y` lands on.
fn choice_row(y: f32) -> Option<usize> {
    let row = ((y - CHOICE_TOP) / CHOICE_ROW).floor();
    (0.0..3.0).contains(&row).then_some(row as usize)
}

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
    fn left_and_right_turn_without_moving() {
        let mut h = Harvest::calm(9);
        let (start, yaw) = (h.pos, h.view_yaw);
        run(&mut h, &Input { held: Buttons::RIGHT, ..Default::default() }, 0.25);
        assert_eq!(h.pos, start);
        assert!((h.view_yaw - yaw).rem_euclid(TAU) > 0.5);
    }

    #[test]
    fn a_tap_on_the_map_throws_a_grenade_that_lands_there() {
        let mut h = Harvest::calm(12);
        let cam = h.camera();
        let target = [h.pos[0] + 60.0, h.pos[1]];
        h.zombies.push(Zombie { kind: Kind::Skinny, pos: target, dir: [0.0, 1.0], rise: 1.0, push: [0.0; 2], hp: 20.0, flash: 0.0, facing: (Facing::Front, false), anim: 0.0 });
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
        h.phase = Phase::LevelUp { choices: [Upgrade::Shotgun, Upgrade::Knife, Upgrade::Speed], selected: 0 };
        let tap = Input { touch: Some([500.0, CHOICE_TOP + CHOICE_ROW * 1.5]), touch_started: true, ..Default::default() };
        h.update(&tap, 0.016, &mut Vec::new());
        assert_eq!(h.gear.knife, 1);
        assert!(matches!(h.phase, Phase::Playing));
    }

    #[test]
    fn a_shot_barrel_blows_up_nearby_zombies_and_opens_its_cell() {
        let mut h = Harvest::new(6);
        let barrel = h.barrels.first().expect("seed 6 has barrels");
        let c = barrel.center();
        h.zombies.push(Zombie { kind: Kind::Big, pos: [c[0] + 20.0, c[1]], dir: [0.0, 1.0], rise: 1.0, push: [0.0; 2], hp: 50.0, flash: 0.0, facing: (Facing::Front, false), anim: 0.0 });
        h.bullets.push(Bullet { pos: [c[0], c[1] - 6.0], vel: [1.0, 0.0], damage: 50.0, pierce: 0, life: 1.0, hit: Vec::new() });
        let mut sounds = Vec::new();
        h.move_bullets(0.016);
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
        h.zombies.push(Zombie { kind: Kind::Skinny, pos: at, dir: [0.0, 1.0], rise: 0.0, push: [0.0; 2], hp: 1000.0, flash: 0.0, facing: (Facing::Front, false), anim: 0.0 });
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
}
