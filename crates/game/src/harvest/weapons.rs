//! What the player carries, and what each weapon does when its cooldown runs out.

use std::f32::consts::{PI, TAU};

use super::juice::{self, Tone};
use super::powerups::{Aim, Does, Kind, Lands, Pattern, Stat, book};
use super::{
    Barrel, Bullet, FLASH_TIME, Fx, Grenade, Harvest, TILE, Thrown, Zombie, len, norm, strike, sub,
};
use crate::{Difficulty, Sfx};
use crate::art::Art;

const MAX_PATCHES: usize = 80;
const PATCH_TICK: f32 = 0.25;
const BURN_TICK: f32 = 0.3;
pub(super) const BOLT_LIFE: f32 = 0.18;

/// A weapon the player carries; `phase` turns the crows around.
#[derive(Clone, Copy, Debug)]
pub(super) struct Armed {
    pub def: usize,
    pub level: u32,
    pub cd: f32,
    pub phase: f32,
}

impl Armed {
    pub fn does(&self) -> Does {
        book().defs[self.def].does.unwrap_or(Does::Pistol)
    }
}

/// The `k`th of a gun's `n` shots at `base`: across `spread` for a fan, all the way round for a ring, or at the
/// random `jitter` (-0.5 to 0.5) of `spread` for a spray.
fn shot_angle(pattern: Pattern, k: u32, n: u32, base: f32, spread: f32, jitter: f32) -> f32 {
    match pattern {
        Pattern::Fan if n > 1 => base + (k as f32 / (n - 1) as f32 - 0.5) * spread,
        Pattern::Fan => base,
        Pattern::Ring => base + k as f32 * TAU / n as f32,
        Pattern::Spray => base + jitter * spread,
    }
}

/// The effect a weapon plays: the one its `effect` line names, else its behaviour's own.
fn look(a: Armed, own: super::effects::Effect) -> super::effects::Effect {
    book().defs[a.def].effect.as_deref().and_then(|n| super::effects::effects().named(n)).unwrap_or(own)
}

/// What a weapon's bullets fly as, when its `bullet` line names an effect.
fn bullet(a: Armed) -> Option<super::effects::Effect> {
    book().defs[a.def].bullet.as_deref().and_then(|n| super::effects::effects().named(n))
}

/// The sound a weapon makes: the one its `sound` line names, else its behaviour's own.
fn sound(a: Armed, own: Sfx) -> Sfx {
    book().defs[a.def].sound.unwrap_or(own)
}

/// Everything the passives add up to.
#[derive(Clone, Copy, Debug)]
pub(super) struct Mods {
    pub damage: f32,
    pub cooldown: f32,
    pub speed: f32,
    pub magnet: f32,
    pub area: f32,
    pub reach: f32,
    pub greed: f32,
    pub knockback: f32,
    pub view: f32,
    pub recharge: f32,
    pub blast: f32,
    pub max_hp: f32,
    pub armor: f32,
    pub regen: f32,
    pub crit: f32,
    pub luck: f32,
    pub pierce: f32,
    pub revive: f32,
    pub grenades: f32,
    pub cluster: f32,
    pub drums: f32,
}

impl Mods {
    fn of(passives: &[(usize, u32)]) -> Self {
        let mut m = Mods {
            damage: 1.0,
            cooldown: 1.0,
            speed: 1.0,
            magnet: 1.0,
            area: 1.0,
            reach: 1.0,
            greed: 1.0,
            knockback: 1.0,
            view: 1.0,
            recharge: 1.0,
            blast: 1.0,
            max_hp: 0.0,
            armor: 0.0,
            regen: 0.0,
            crit: 0.0,
            luck: 0.0,
            pierce: 0.0,
            revive: 0.0,
            grenades: 0.0,
            cluster: 0.0,
            drums: 0.0,
        };
        for &(d, level) in passives {
            let def = &book().defs[d];
            let at = |s| def.at(s, level);
            m.damage *= at(Stat::Damage);
            m.cooldown *= at(Stat::Cooldown);
            m.speed *= at(Stat::Speed);
            m.magnet *= at(Stat::Magnet);
            m.area *= at(Stat::Area);
            m.reach *= at(Stat::Reach);
            m.greed *= at(Stat::Greed);
            m.knockback *= at(Stat::Knockback);
            m.view *= at(Stat::View);
            m.recharge *= at(Stat::Recharge);
            m.blast *= at(Stat::Blast);
            m.max_hp += at(Stat::MaxHp);
            m.armor += at(Stat::Armor);
            m.regen += at(Stat::Regen);
            m.crit += at(Stat::Crit);
            m.luck += at(Stat::Luck);
            m.pierce += at(Stat::Pierce);
            m.revive += at(Stat::Revive);
            m.grenades += at(Stat::Grenades);
            m.cluster += at(Stat::Cluster);
            m.drums += at(Stat::Drums);
        }
        m
    }
}

/// The run's weapons and passives, with the passives already added up.
#[derive(Clone, Debug)]
pub(super) struct Kit {
    pub weapons: Vec<Armed>,
    pub passives: Vec<(usize, u32)>,
    pub mods: Mods,
    /// The health, speed and magnet the player has before upgrades: their card's, else tuning.txt's.
    pub base: [f32; 3],
}

impl Kit {
    /// The weapons the file starts a run with at `difficulty`.
    pub fn start(difficulty: Difficulty) -> Self {
        Self::with_weapons(&book().start[difficulty as usize])
    }

    /// A kit carrying these weapons at level 1, and nothing else.
    pub fn with_weapons(defs: &[usize]) -> Self {
        let weapons = defs.iter().map(|&def| Armed { def, level: 1, cd: 0.5, phase: 0.0 }).collect();
        let t = crate::tuning::tuning();
        Self { weapons, passives: Vec::new(), mods: Mods::of(&[]), base: [t.player_hp, t.player_speed, t.player_magnet] }
    }

    pub fn level(&self, def: usize) -> u32 {
        self.weapons.iter().find(|a| a.def == def).map(|a| a.level).or_else(|| self.passives.iter().find(|p| p.0 == def).map(|p| p.1)).unwrap_or(0)
    }

    /// Whether a card for `def` can come up: not at its top level, with a slot free if it's new.
    pub fn offerable(&self, def: usize) -> bool {
        let b = book();
        let d = &b.defs[def];
        let level = self.level(def);
        match d.kind {
            Kind::Filler => false,
            _ if level >= d.max => false,
            _ if level > 0 => true,
            Kind::Weapon => self.weapons.len() < b.weapon_slots,
            Kind::Passive => self.passives.len() < b.passive_slots,
        }
    }

    /// Raises a weapon or passive `by` levels, up to its top; returns the levels it was at and is at now.
    pub fn raise(&mut self, def: usize, by: u32) -> (u32, u32) {
        let d = &book().defs[def];
        let from = self.level(def);
        let to = (from + by).min(d.max);
        match d.kind {
            Kind::Weapon => match self.weapons.iter_mut().find(|a| a.def == def) {
                Some(a) => a.level = to,
                None => self.weapons.push(Armed { def, level: to, cd: 0.3, phase: 0.0 }),
            },
            Kind::Passive => {
                match self.passives.iter_mut().find(|p| p.0 == def) {
                    Some(p) => p.1 = to,
                    None => self.passives.push((def, to)),
                }
                self.mods = Mods::of(&self.passives);
            }
            Kind::Filler => {}
        }
        (from, to)
    }

    pub fn max_hp(&self) -> f32 {
        self.base[0] + self.mods.max_hp
    }

    pub fn speed(&self) -> f32 {
        self.base[1] * self.mods.speed
    }

    pub fn magnet(&self) -> f32 {
        self.base[2] * self.mods.magnet
    }

    pub fn grenades(&self) -> f32 {
        book().grenades as f32 + self.mods.grenades.round()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Hazard {
    Tacks,
    Fire,
    Flare,
}

/// Something left on the ground that hurts what stands in it.
pub(super) struct Patch {
    pub hazard: Hazard,
    pub pos: [f32; 2],
    pub radius: f32,
    /// Damage a second.
    pub dps: f32,
    pub slow: f32,
    pub age: f32,
    pub life: f32,
    tick: f32,
}

/// A blade swept around the player: its angle and distance from them run from the first value to the second.
pub(super) struct Swing {
    pub art: Art,
    pub age: f32,
    pub life: f32,
    pub angle: [f32; 2],
    pub reach: [f32; 2],
}

impl Swing {
    pub fn now(&self) -> (f32, f32) {
        let k = (self.age / self.life).min(1.0);
        let k = 1.0 - (1.0 - k) * (1.0 - k);
        (self.angle[0] + (self.angle[1] - self.angle[0]) * k, self.reach[0] + (self.reach[1] - self.reach[0]) * k)
    }
}

/// Lightning from one point to the next.
pub(super) struct Bolt {
    pub points: Vec<[f32; 2]>,
    pub age: f32,
}

pub(super) struct Dog {
    def: usize,
    slot: usize,
    pub pos: [f32; 2],
    /// The way it last ran; the art faces right.
    pub heading: [f32; 2],
    pub anim: f32,
    cd: f32,
    target: Option<u32>,
    vel: [f32; 2],
    /// Where it means to wander, from the player.
    spot: [f32; 2],
    /// Seconds until it picks a new spot.
    rest: f32,
    catching_up: bool,
}

impl Dog {
    pub fn art(&self) -> Art {
        super::effects::effects().dog.looping(self.anim)
    }
}

/// A scarecrow drawing zombies in until it bursts.
pub(super) struct Decoy {
    pub pos: [f32; 2],
    pub age: f32,
    pub life: f32,
    pub lure: f32,
    blast: f32,
    damage: f32,
}

fn in_cone(d: [f32; 2], dir: [f32; 2], half: f32) -> bool {
    let l = len(d);
    l > 1e-3 && (d[0] * dir[0] + d[1] * dir[1]) / l >= half.cos()
}

fn polar(at: [f32; 2], angle: f32, r: f32) -> [f32; 2] {
    [at[0] + angle.cos() * r, at[1] + angle.sin() * r]
}

/// Sets a zombie burning, or keeps it burning longer and hotter.
fn ignite(z: &mut Zombie, dps: f32, secs: f32) {
    z.burn = z.burn.max(secs);
    z.burn_dps = z.burn_dps.max(dps);
}

/// Damage with no shove, from fire or tacks.
fn singe(z: &mut Zombie, juice: &mut juice::Juice, damage: f32) {
    z.hp -= damage;
    z.flash = FLASH_TIME;
    juice.popup(z.pos, damage, Tone::Hit);
}

impl Harvest {
    /// A weapon's stat at its level, with the passives applied.
    fn stat(&self, a: Armed, s: Stat) -> f32 {
        let v = book().defs[a.def].at(s, a.level);
        let m = &self.kit.mods;
        match s {
            Stat::Damage => v * m.damage,
            Stat::Cooldown => v * m.cooldown,
            Stat::Area => v * m.area,
            Stat::Range => v * m.reach,
            Stat::Pierce => v + m.pierce,
            Stat::Knockback => v * m.knockback,
            _ => v,
        }
    }

    pub(super) fn ahead_half(&self) -> f32 {
        crate::tuning::tuning().view_half * self.kit.mods.view
    }

    fn crit(&self) -> f32 {
        crate::tuning::tuning().crit + self.kit.mods.crit
    }

    /// The offset to the nearest zombie within `range`, inside `cone` (a direction and a half-angle) when given.
    fn nearest_in(&self, range: f32, cone: Option<([f32; 2], f32)>) -> Option<[f32; 2]> {
        self.zombies
            .iter()
            .map(|z| sub(z.pos, self.pos))
            .filter(|d| len(*d) < range && cone.is_none_or(|(dir, half)| in_cone(*d, dir, half)))
            .min_by(|a, b| len(*a).total_cmp(&len(*b)))
    }

    /// While X is held, the offset to the nearest barrel within `range` inside the view.
    pub(super) fn barrel_in_view(&self, range: f32) -> Option<[f32; 2]> {
        if !self.targeting {
            return None;
        }
        self.barrels
            .iter()
            .map(|b| sub(b.center(), self.pos))
            .filter(|d| len(*d) < range && in_cone(*d, self.aim, self.ahead_half()))
            .min_by(|a, b| len(*a).total_cmp(&len(*b)))
    }

    /// The longest reach of the carried weapons that X turns on barrels.
    pub(super) fn barrel_reach(&self) -> Option<f32> {
        let aimed = self.kit.weapons.iter().filter(|a| matches!(a.does(), Does::Pistol | Does::Shotgun | Does::Rifle | Does::Gun));
        aimed.map(|&a| self.stat(a, Stat::Range)).reduce(f32::max)
    }

    pub(super) fn fire(&mut self, dt: f32, sounds: &mut Vec<Sfx>) {
        for i in 0..self.kit.weapons.len() {
            let mut a = self.kit.weapons[i];
            a.cd -= dt;
            if a.cd <= 0.0 {
                a.cd = if self.attack(a, sounds) { self.stat(a, Stat::Cooldown) } else { 0.0 };
            }
            if a.does() == Does::Crows {
                a.phase = (a.phase + self.stat(a, Stat::Speed).to_radians() * dt).rem_euclid(TAU);
            }
            self.kit.weapons[i] = a;
        }
        self.peck(dt, sounds);
        self.walk_dogs(dt, sounds);
    }

    /// Fires a weapon whose cooldown is up; false when there was nothing for it to do.
    fn attack(&mut self, a: Armed, sounds: &mut Vec<Sfx>) -> bool {
        match a.does() {
            Does::Pistol => self.pistol(a, sounds),
            Does::Shotgun => self.spray(a, self.aim, self.ahead_half(), true, sounds),
            Does::Holster => self.spray(a, [-self.aim[0], -self.aim[1]], crate::tuning::tuning().behind_half, false, sounds),
            Does::Rifle => self.rifle(a, sounds),
            Does::Lantern => self.beam(a),
            Does::Burner => self.burner(a),
            Does::Pitchfork => self.pitchfork(a, sounds),
            Does::Knife => self.knife(a, sounds),
            Does::Scythe => self.scythe(a, sounds),
            Does::Zapper => self.zap(a, sounds),
            Does::Tacks => self.tacks(a),
            Does::Gas => self.gas(a),
            Does::Scarecrow => self.scarecrow(a),
            Does::Molotov => self.lob_molotovs(a, sounds),
            Does::Flare => self.lob_flare(a, sounds),
            Does::Gun => self.gun(a, sounds),
            Does::Blade => self.blade(a, sounds),
            Does::Lob => self.lob(a, sounds),
            Does::Crows | Does::Dog => false,
        }
    }

    fn pistol(&mut self, a: Armed, sounds: &mut Vec<Sfx>) -> bool {
        let range = self.stat(a, Stat::Range);
        let target = self.barrel_in_view(range).or_else(|| self.nearest_in(crate::tuning::tuning().point_blank.min(range), None)).or_else(|| self.nearest_in(range, Some((self.aim, self.ahead_half())))).or_else(|| self.nearest_in(range, None));
        let Some(d) = target.map(norm) else { return false };
        let (n, speed) = (self.stat(a, Stat::Count) as u32, self.stat(a, Stat::Speed));
        let base = d[1].atan2(d[0]);
        let gap = self.stat(a, Stat::Spread).to_radians();
        for k in 0..n {
            let angle = base + (k as f32 - (n - 1) as f32 / 2.0) * gap;
            self.shoot(angle, speed, self.stat(a, Stat::Damage), self.stat(a, Stat::Pierce) as u32, range / speed, self.stat(a, Stat::Knockback), bullet(a));
        }
        let muzzle = [self.pos[0] + d[0] * 8.0, self.pos[1] - 6.0 + d[1] * 8.0];
        self.fx.push(Fx::play(look(a, super::effects::effects().muzzle_pistol), muzzle, base));
        self.juice.kick += 0.015;
        self.juice.flash([self.pos[0] + d[0] * 8.0, self.pos[1] + d[1] * 8.0], 10.0, [1.2, 0.85, 0.4], 3.5, 0.07);
        self.noise = Some(self.pos);
        sounds.push(sound(a, Sfx::Shot));
        true
    }

    /// A gun as its section shapes it: where it looks for a target, and how its shots spread.
    fn gun(&mut self, a: Armed, sounds: &mut Vec<Sfx>) -> bool {
        let def = &book().defs[a.def];
        let (aim, pattern) = (def.aim, def.pattern);
        let range = self.stat(a, Stat::Range);
        let Some(d) = self.barrel_in_view(range).map(norm).or_else(|| self.aimed(aim, range)) else { return false };
        let (n, speed) = (self.stat(a, Stat::Count).max(1.0) as u32, self.stat(a, Stat::Speed));
        let (base, spread) = (d[1].atan2(d[0]), self.stat(a, Stat::Spread).to_radians());
        let (damage, pierce, force) = (self.stat(a, Stat::Damage), self.stat(a, Stat::Pierce) as u32, self.stat(a, Stat::Knockback));
        for k in 0..n {
            let angle = shot_angle(pattern, k, n, base, spread, self.juice.rng.range(-0.5, 0.5));
            self.shoot(angle, speed, damage, pierce, range / speed, force, bullet(a));
        }
        let muzzle = [self.pos[0] + d[0] * 8.0, self.pos[1] - 6.0 + d[1] * 8.0];
        self.fx.push(Fx::play(look(a, super::effects::effects().muzzle_pistol), muzzle, base));
        self.juice.kick += 0.015;
        self.noise = Some(self.pos);
        sounds.push(sound(a, Sfx::Shot));
        true
    }

    /// Where a gun or blade looks for a target within `range`: the way to it, if it finds one.
    fn aimed(&self, aim: Aim, range: f32) -> Option<[f32; 2]> {
        match aim {
            Aim::Nearest => self.nearest_in(range, None),
            Aim::Ahead => self.nearest_in(range, Some((self.aim, self.ahead_half()))),
            Aim::Behind => self.nearest_in(range, Some(([-self.aim[0], -self.aim[1]], crate::tuning::tuning().behind_half))),
            Aim::Around => self.nearest_in(range, None).map(|_| self.aim),
        }
        .map(norm)
    }

    /// A blade as its section shapes it: everything within its area across its spread, toward what its aim finds.
    fn blade(&mut self, a: Armed, sounds: &mut Vec<Sfx>) -> bool {
        let reach = self.stat(a, Stat::Area);
        let Some(d) = self.aimed(book().defs[a.def].aim, reach) else { return false };
        let half = self.stat(a, Stat::Spread).to_radians() / 2.0;
        let (damage, force, crit) = (self.stat(a, Stat::Damage), self.stat(a, Stat::Knockback), self.crit());
        let mut hits = (0, false);
        for z in &mut self.zombies {
            let to = sub(z.pos, self.pos);
            if len(to) < reach && (half >= PI || in_cone(to, d, half)) {
                hits.1 |= strike(z, &mut self.juice, damage, norm(to), force, crit);
                hits.0 += 1;
            }
        }
        let at = [self.pos[0] + d[0] * 8.0, self.pos[1] - 5.0 + d[1] * 8.0];
        self.fx.push(Fx::play(look(a, super::effects::effects().slash), at, d[1].atan2(d[0]) - PI / 2.0));
        self.juice.kick += 0.02;
        sounds.push(sound(a, Sfx::Slash));
        self.thud(hits, sounds);
        true
    }

    /// A fan of pellets at the nearest zombie inside the cone around `dir`, or at a barrel in view when `barrels`
    /// and X is held.
    fn spray(&mut self, a: Armed, dir: [f32; 2], half: f32, barrels: bool, sounds: &mut Vec<Sfx>) -> bool {
        let range = self.stat(a, Stat::Range);
        let barrel = if barrels { self.barrel_in_view(range) } else { None };
        let Some(d) = barrel.or_else(|| self.nearest_in(range, Some((dir, half)))).map(norm) else { return false };
        let (pellets, speed) = (self.stat(a, Stat::Count).max(2.0) as u32, self.stat(a, Stat::Speed));
        let (base, spread) = (d[1].atan2(d[0]), self.stat(a, Stat::Spread).to_radians());
        for k in 0..pellets {
            let angle = base + (k as f32 / (pellets - 1) as f32 - 0.5) * spread;
            self.shoot(angle, speed, self.stat(a, Stat::Damage), self.stat(a, Stat::Pierce) as u32, range / speed * 0.9, self.stat(a, Stat::Knockback), bullet(a));
        }
        let at = [self.pos[0] + d[0] * 10.0, self.pos[1] - 6.0 + d[1] * 10.0];
        self.fx.push(Fx::play(look(a, super::effects::effects().shotgun_blast), at, base));
        let ground = [self.pos[0] + d[0] * 10.0, self.pos[1] + d[1] * 10.0];
        self.juice.kick += 0.04;
        self.juice.punch += 3.0;
        self.juice.shake(0.12);
        self.juice.flash(ground, 10.0, [1.6, 1.1, 0.5], 5.0, 0.1);
        self.juice.burst(ground, 8.0, 8, d, 0.35, [80.0, 160.0], juice::SPARK, 1.0, 0.18, 1.0);
        self.noise = Some(self.pos);
        sounds.push(sound(a, Sfx::Blast));
        true
    }

    /// One heavy round straight along the heading, once something is lined up.
    fn rifle(&mut self, a: Armed, sounds: &mut Vec<Sfx>) -> bool {
        let range = self.stat(a, Stat::Range);
        let half = self.stat(a, Stat::Spread).to_radians() / 2.0;
        let barrel = self.barrel_in_view(range);
        if barrel.is_none() && self.nearest_in(range, Some((self.aim, half))).is_none() {
            return false;
        }
        let speed = self.stat(a, Stat::Speed);
        let way = barrel.map_or(self.aim, norm);
        let angle = way[1].atan2(way[0]);
        self.shoot(angle, speed, self.stat(a, Stat::Damage), self.stat(a, Stat::Pierce) as u32, range / speed, self.stat(a, Stat::Knockback), bullet(a));
        let muzzle = [self.pos[0] + self.aim[0] * 9.0, self.pos[1] - 6.0 + self.aim[1] * 9.0];
        self.fx.push(Fx::play(look(a, super::effects::effects().muzzle_rifle), muzzle, angle));
        self.juice.kick += 0.05;
        self.juice.punch += 2.0;
        self.juice.shake(0.1);
        self.juice.flash([self.pos[0] + self.aim[0] * 10.0, self.pos[1] + self.aim[1] * 10.0], 10.0, [1.6, 1.2, 0.7], 5.0, 0.1);
        self.noise = Some(self.pos);
        sounds.push(sound(a, Sfx::Blast));
        true
    }

    /// The lantern flares: everything in its light is burned a little and slowed.
    fn beam(&mut self, a: Armed) -> bool {
        let (range, half) = (self.stat(a, Stat::Range), self.ahead_half());
        let (damage, slow, secs) = (self.stat(a, Stat::Damage), self.stat(a, Stat::Slow), self.stat(a, Stat::Duration));
        let crit = self.crit();
        let mut lit = 0;
        for z in &mut self.zombies {
            let d = sub(z.pos, self.pos);
            if z.rise >= 1.0 && len(d) < range && in_cone(d, self.aim, half) {
                strike(z, &mut self.juice, damage, norm(d), 0.0, crit);
                z.slow = z.slow.max(secs);
                z.slow_k = z.slow_k.max(slow);
                if lit < 6 {
                    self.juice.burst(z.pos, 8.0, 2, [0.0; 2], 0.0, [10.0, 30.0], juice::EMBER, 1.0, 0.4, 1.0);
                }
                lit += 1;
            }
        }
        if lit > 0 {
            self.beam = 0.25;
        }
        true
    }

    fn burner(&mut self, a: Armed) -> bool {
        let (range, half) = (self.stat(a, Stat::Range), self.stat(a, Stat::Spread).to_radians() / 2.0);
        if self.nearest_in(range, Some((self.aim, half))).is_none() {
            return false;
        }
        let (dps, secs) = (self.stat(a, Stat::Damage), self.stat(a, Stat::Duration));
        for z in &mut self.zombies {
            let d = sub(z.pos, self.pos);
            if z.rise >= 1.0 && len(d) < range && in_cone(d, self.aim, half) {
                ignite(z, dps, secs);
            }
        }
        let nozzle = [self.pos[0] + self.aim[0] * 6.0, self.pos[1] + self.aim[1] * 6.0];
        let reach = range * 3.0;
        self.juice.burst(nozzle, 6.0, 5, self.aim, half, [reach * 0.6, reach], juice::EMBER, 1.6, 0.3, 1.0);
        self.juice.flash(nozzle, 8.0, [1.4, 0.7, 0.3], 4.0, 0.15);
        true
    }

    fn pitchfork(&mut self, a: Armed, sounds: &mut Vec<Sfx>) -> bool {
        let range = self.stat(a, Stat::Range);
        if self.nearest_in(range + 4.0, Some((self.aim, self.ahead_half()))).is_none() {
            return false;
        }
        let (count, spread) = (self.stat(a, Stat::Count).max(1.0) as u32, self.stat(a, Stat::Spread).to_radians());
        let (width, damage, force, crit) = (self.stat(a, Stat::Area) / 2.0 + 3.0, self.stat(a, Stat::Damage), self.stat(a, Stat::Knockback), self.crit());
        let base = self.aim[1].atan2(self.aim[0]);
        let mut hit = vec![false; self.zombies.len()];
        let mut hits = (0, false);
        for k in 0..count {
            let angle = if count == 1 { base } else { base + (k as f32 / (count - 1) as f32 - 0.5) * spread };
            let dir = [angle.cos(), angle.sin()];
            for (i, z) in self.zombies.iter_mut().enumerate() {
                let d = sub(z.pos, self.pos);
                let along = d[0] * dir[0] + d[1] * dir[1];
                let across = (d[0] * dir[1] - d[1] * dir[0]).abs();
                if !hit[i] && (0.0..range).contains(&along) && across < width {
                    hit[i] = true;
                    hits.1 |= strike(z, &mut self.juice, damage, dir, force, crit);
                    hits.0 += 1;
                }
            }
            let e = look(a, super::effects::effects().pitchfork);
            self.swings.push(Swing { art: e.frames[0], age: 0.0, life: e.life, angle: [angle; 2], reach: [2.0, range * e.reach] });
        }
        self.juice.kick += 0.02;
        sounds.push(sound(a, Sfx::Slash));
        self.thud(hits, sounds);
        true
    }

    fn knife(&mut self, a: Armed, sounds: &mut Vec<Sfx>) -> bool {
        let reach = self.stat(a, Stat::Area);
        let Some(d) = self.nearest_in(reach, None).map(norm).or_else(|| self.nearest_in(reach + 6.0, None).map(|_| self.aim)) else { return false };
        let (damage, force, crit) = (self.stat(a, Stat::Damage), self.stat(a, Stat::Knockback), self.crit());
        let mut hits = (0, false);
        for z in &mut self.zombies {
            if len(sub(z.pos, self.pos)) < reach {
                let away = norm(sub(z.pos, self.pos));
                hits.1 |= strike(z, &mut self.juice, damage, away, force, crit);
                hits.0 += 1;
            }
        }
        let at = [self.pos[0] + d[0] * 8.0, self.pos[1] - 5.0 + d[1] * 8.0];
        self.fx.push(Fx::play(look(a, super::effects::effects().slash), at, d[1].atan2(d[0]) - PI / 2.0));
        self.juice.kick += 0.02;
        self.juice.shake(0.06);
        sounds.push(sound(a, Sfx::Slash));
        self.thud(hits, sounds);
        true
    }

    fn scythe(&mut self, a: Armed, sounds: &mut Vec<Sfx>) -> bool {
        let reach = self.stat(a, Stat::Area);
        if self.nearest_in(reach * 1.1, None).is_none() {
            return false;
        }
        let (damage, force, crit) = (self.stat(a, Stat::Damage), self.stat(a, Stat::Knockback), self.crit());
        let mut hits = (0, false);
        for z in &mut self.zombies {
            let d = sub(z.pos, self.pos);
            if len(d) < reach {
                hits.1 |= strike(z, &mut self.juice, damage, norm(d), force, crit);
                hits.0 += 1;
            }
        }
        let from = self.view_yaw;
        let e = look(a, super::effects::effects().scythe);
        self.swings.push(Swing { art: e.frames[0], age: 0.0, life: e.life, angle: [from, from + TAU], reach: [reach * e.reach; 2] });
        self.juice.ring(self.pos, reach, 0.3, [0.85, 0.85, 0.8]);
        self.juice.shake(0.1);
        self.juice.punch += 1.5;
        sounds.push(sound(a, Sfx::Slash));
        self.thud(hits, sounds);
        true
    }

    /// Crows circling the player peck whatever they pass over, each zombie no more often than the cooldown.
    fn peck(&mut self, dt: f32, sounds: &mut Vec<Sfx>) {
        for p in &mut self.pecked {
            p.1 -= dt;
        }
        self.pecked.retain(|p| p.1 > 0.0);
        let crit = self.crit();
        let mut hits = (0, false);
        for a in self.kit.weapons.clone() {
            if a.does() != Does::Crows {
                continue;
            }
            let (damage, cd) = (self.stat(a, Stat::Damage), self.stat(a, Stat::Cooldown));
            for at in self.crow_spots(a) {
                for z in &mut self.zombies {
                    if z.rise >= 1.0 && len(sub(z.pos, at)) < crate::tuning::tuning().crow_reach && !self.pecked.iter().any(|p| p.0 == z.id) {
                        hits.1 |= strike(z, &mut self.juice, damage, norm(sub(z.pos, self.pos)), 20.0 * self.kit.mods.knockback, crit);
                        hits.0 += 1;
                        self.pecked.push((z.id, cd));
                    }
                }
            }
        }
        if hits.0 > 0 && self.hush[4] <= 0.0 && self.juice.rng.below(4) == 0 {
            let (pan, gain) = self.place(self.pos);
            sounds.push(Sfx::Caw { pan, gain: gain / 2 });
            self.hush[4] = 1.5;
        }
        self.thud(hits, sounds);
    }

    /// Where each of a crow weapon's birds is now.
    pub(super) fn crow_spots(&self, a: Armed) -> Vec<[f32; 2]> {
        let (n, r) = (self.stat(a, Stat::Count).max(1.0) as usize, self.stat(a, Stat::Area));
        (0..n).map(|k| polar(self.pos, a.phase + k as f32 / n as f32 * TAU, r)).collect()
    }

    /// Every crow, with which way it flies, for drawing.
    pub(super) fn crows(&self) -> Vec<([f32; 2], bool)> {
        self.kit.weapons.iter().filter(|a| a.does() == Does::Crows).flat_map(|&a| self.crow_spots(a).into_iter().map(move |p| (p, a.phase.sin() < 0.0))).collect()
    }

    /// Lightning from the zapper to the nearest zombie, then on to the next nearest it hasn't touched.
    fn zap(&mut self, a: Armed, sounds: &mut Vec<Sfx>) -> bool {
        let range = self.stat(a, Stat::Range);
        let Some(first) = self.nearest_in(range, None) else { return false };
        let (count, jump, damage, crit) = (self.stat(a, Stat::Count).max(1.0) as usize, self.stat(a, Stat::Area), self.stat(a, Stat::Damage), self.crit());
        let mut at = [self.pos[0] + first[0], self.pos[1] + first[1]];
        let mut points = vec![[self.pos[0], self.pos[1] - 10.0]];
        let mut done: Vec<u32> = Vec::new();
        let mut hits = (0, false);
        for _ in 0..count {
            let next = self.zombies.iter().enumerate().filter(|(_, z)| !done.contains(&z.id) && len(sub(z.pos, at)) < if done.is_empty() { 2.0 } else { jump }).min_by(|a, b| len(sub(a.1.pos, at)).total_cmp(&len(sub(b.1.pos, at))));
            let Some((i, _)) = next else { break };
            let z = &mut self.zombies[i];
            hits.1 |= strike(z, &mut self.juice, damage, norm(sub(z.pos, at)), 15.0, crit);
            hits.0 += 1;
            done.push(z.id);
            at = z.pos;
            points.push([at[0], at[1] - 6.0]);
        }
        self.juice.flash(points[1], 10.0, [0.6, 0.8, 1.8], 5.0, 0.12);
        self.bolts.push(Bolt { points, age: 0.0 });
        sounds.push(sound(a, Sfx::Zap));
        self.thud(hits, sounds);
        true
    }

    /// Dogs run down the zombie nearest the player within their leash and bite; with nothing to chase they wander near the player.
    fn walk_dogs(&mut self, dt: f32, sounds: &mut Vec<Sfx>) {
        let want: Vec<(usize, usize)> = self.kit.weapons.iter().filter(|a| a.does() == Does::Dog).flat_map(|&a| (0..self.stat(a, Stat::Count).max(1.0) as usize).map(move |s| (a.def, s))).collect();
        self.dogs.retain(|d| want.contains(&(d.def, d.slot)));
        for (def, slot) in want {
            if !self.dogs.iter().any(|d| d.def == def && d.slot == slot) {
                self.dogs.push(Dog { def, slot, pos: self.pos, heading: [1.0, 0.0], anim: 0.0, cd: 0.0, target: None, vel: [0.0; 2], spot: [0.0; 2], rest: 0.0, catching_up: false });
            }
        }
        let crit = self.crit();
        let mut hits = (0, false);
        let mut barked = false;
        for i in 0..self.dogs.len() {
            let Some(a) = self.kit.weapons.iter().copied().find(|a| a.def == self.dogs[i].def) else { continue };
            let (leash, speed, damage, cd, force) = (self.stat(a, Stat::Range), self.stat(a, Stat::Speed), self.stat(a, Stat::Damage), self.stat(a, Stat::Cooldown), self.stat(a, Stat::Knockback));
            let dog = &self.dogs[i];
            let near_player = |z: &Zombie| z.rise >= 1.0 && len(sub(z.pos, self.pos)) < leash;
            let held = dog.target.and_then(|id| self.zombies.iter().position(|z| z.id == id && near_player(z)));
            let target = held.or_else(|| self.zombies.iter().enumerate().filter(|(_, z)| near_player(z)).min_by(|a, b| len(sub(a.1.pos, dog.pos)).total_cmp(&len(sub(b.1.pos, dog.pos)))).map(|(i, _)| i));
            let (player, yaw) = (self.pos, self.view_yaw);
            let away = len(sub(player, dog.pos));
            let goal = target.map(|t| self.zombies[t].pos);
            let dog = &mut self.dogs[i];
            dog.cd -= dt;
            dog.rest -= dt;
            dog.target = target.map(|t| self.zombies[t].id);
            dog.catching_up = target.is_none() && (away > crate::tuning::tuning().dog_stray || (dog.catching_up && away > crate::tuning::tuning().dog_stray / 2.0));
            if dog.rest <= 0.0 {
                let rng = &mut self.juice.rng;
                dog.spot = polar([0.0; 2], yaw + rng.range(-0.55, 0.55) * PI, rng.range(14.0, 32.0));
                dog.rest = rng.range(1.5, 4.0);
            }
            let (to, pace, stop) = match goal {
                Some(g) => (sub(g, dog.pos), speed, crate::tuning::tuning().dog_reach * 0.6),
                None if dog.catching_up => (sub(player, dog.pos), speed, 0.0),
                None => (sub([player[0] + dog.spot[0], player[1] + dog.spot[1]], dog.pos), speed * crate::tuning::tuning().dog_trot, 3.0),
            };
            let n = norm(to);
            let want = if len(to) > stop { [n[0] * pace, n[1] * pace] } else { [0.0; 2] };
            let k = (dt * 8.0).min(1.0);
            dog.vel = [dog.vel[0] + (want[0] - dog.vel[0]) * k, dog.vel[1] + (want[1] - dog.vel[1]) * k];
            dog.pos = [dog.pos[0] + dog.vel[0] * dt, dog.pos[1] + dog.vel[1] * dt];
            let moving = len(dog.vel);
            if moving > 5.0 {
                dog.heading = norm(dog.vel);
                dog.anim += dt * (moving / speed * 1.6).min(1.0);
            }
            if let Some(t) = target
                && len(to) < crate::tuning::tuning().dog_reach
                && dog.cd <= 0.0
            {
                dog.cd = cd;
                hits.1 |= strike(&mut self.zombies[t], &mut self.juice, damage, n, force, crit);
                hits.0 += 1;
                barked |= self.juice.rng.below(5) == 0;
            }
        }
        if barked && self.hush[4] <= 0.0 {
            sounds.push(Sfx::Bark);
            self.hush[4] = 1.0;
        }
        self.thud(hits, sounds);
    }

    fn tacks(&mut self, a: Armed) -> bool {
        let n = self.stat(a, Stat::Count).max(1.0) as u32;
        let behind = [self.pos[0] - self.aim[0] * 10.0, self.pos[1] - self.aim[1] * 10.0];
        for _ in 0..n {
            let at = polar(behind, self.juice.rng.range(0.0, TAU), self.juice.rng.range(0.0, 6.0));
            self.lay(Hazard::Tacks, at, self.stat(a, Stat::Area), self.stat(a, Stat::Damage), self.stat(a, Stat::Duration), self.stat(a, Stat::Slow));
        }
        true
    }

    /// Burning fuel where the player walks; nothing while standing still.
    fn gas(&mut self, a: Armed) -> bool {
        if self.walk <= 0.0 {
            return false;
        }
        let at = [self.pos[0] - self.aim[0] * 4.0, self.pos[1] - self.aim[1] * 4.0];
        self.lay(Hazard::Fire, at, self.stat(a, Stat::Area), self.stat(a, Stat::Damage), self.stat(a, Stat::Duration), 0.0);
        true
    }

    fn scarecrow(&mut self, a: Armed) -> bool {
        if !self.decoys.is_empty() {
            return false;
        }
        let at = [self.pos[0] - self.aim[0] * 12.0, self.pos[1] - self.aim[1] * 12.0];
        if self.map.hits(at[0], at[1], crate::tuning::tuning().zombie_half()) {
            return false;
        }
        let (lure, blast, damage, life) = (self.stat(a, Stat::Range), self.stat(a, Stat::Area), self.stat(a, Stat::Damage), self.stat(a, Stat::Duration));
        self.decoys.push(Decoy { pos: at, age: 0.0, life, lure, blast, damage });
        self.juice.burst(at, 0.0, 8, [0.0; 2], 0.0, [15.0, 40.0], juice::DIRT, 1.4, 0.5, 0.0);
        true
    }

    /// A zombie in a crowd within `range`: the best of a few picked at random, by how many stand near it.
    fn crowd(&mut self, range: f32) -> Option<[f32; 2]> {
        let near: Vec<[f32; 2]> = self.zombies.iter().filter(|z| z.rise >= 1.0 && len(sub(z.pos, self.pos)) < range).map(|z| z.pos).collect();
        if near.is_empty() {
            return None;
        }
        let tries: Vec<[f32; 2]> = (0..6).map(|_| near[self.juice.rng.below(near.len())]).collect();
        tries.into_iter().max_by_key(|p| near.iter().filter(|q| len(sub(**q, *p)) < 30.0).count())
    }

    fn lob_molotovs(&mut self, a: Armed, sounds: &mut Vec<Sfx>) -> bool {
        let range = self.stat(a, Stat::Range);
        let mut thrown = false;
        for _ in 0..self.stat(a, Stat::Count).max(1.0) as u32 {
            let Some(to) = self.crowd(range) else { break };
            let kind = Thrown::Molotov { dps: self.stat(a, Stat::Damage), radius: self.stat(a, Stat::Area), life: self.stat(a, Stat::Duration) };
            self.thrown.push(Grenade { from: self.pos, to, t: 0.0, kind });
            thrown = true;
        }
        if thrown {
            sounds.push(Sfx::Throw);
        }
        thrown
    }

    /// Throws a lob weapon's `count` at crowds within its range.
    fn lob(&mut self, a: Armed, sounds: &mut Vec<Sfx>) -> bool {
        let range = self.stat(a, Stat::Range);
        let (damage, radius, life, slow) = (self.stat(a, Stat::Damage), self.stat(a, Stat::Area), self.stat(a, Stat::Duration), self.stat(a, Stat::Slow));
        let mut thrown = false;
        for _ in 0..self.stat(a, Stat::Count).max(1.0) as u32 {
            let Some(to) = self.crowd(range) else { break };
            self.thrown.push(Grenade { from: self.pos, to, t: 0.0, kind: Thrown::Lob { def: a.def, damage, radius, life, slow } });
            thrown = true;
        }
        if thrown {
            sounds.push(sound(a, Sfx::Throw));
        }
        thrown
    }

    fn lob_flare(&mut self, a: Armed, sounds: &mut Vec<Sfx>) -> bool {
        let Some(to) = self.crowd(self.stat(a, Stat::Range)) else { return false };
        let kind = Thrown::Flare { dps: self.stat(a, Stat::Damage), radius: self.stat(a, Stat::Area), life: self.stat(a, Stat::Duration) };
        self.thrown.push(Grenade { from: self.pos, to, t: 0.0, kind });
        sounds.push(Sfx::Throw);
        true
    }

    /// What a thrown thing does where it lands.
    pub(super) fn land(&mut self, g: &Grenade, sounds: &mut Vec<Sfx>) {
        match g.kind {
            Thrown::Grenade => {
                self.explode(g.to, crate::tuning::tuning().grenade_radius, crate::tuning::tuning().grenade_damage, sounds);
                for _ in 0..self.kit.mods.cluster.round() as u32 {
                    let to = polar(g.to, self.juice.rng.range(0.0, TAU), self.juice.rng.range(16.0, 34.0));
                    self.thrown.push(Grenade { from: g.to, to, t: crate::tuning::tuning().grenade_flight * 0.45, kind: Thrown::Bomblet });
                }
            }
            Thrown::Bomblet => self.explode(g.to, crate::tuning::tuning().bomblet_radius, crate::tuning::tuning().bomblet_damage, sounds),
            Thrown::Molotov { dps, radius, life } => {
                // A pool is a ring of fires, so it reads as one in both views.
                self.lay(Hazard::Fire, g.to, radius * 0.5, dps, life, 0.0);
                let n = (radius / 6.0) as usize;
                for k in 0..n {
                    let at = polar(g.to, k as f32 / n as f32 * TAU, radius * 0.6);
                    let life = life * self.juice.rng.range(0.8, 1.0);
                    self.lay(Hazard::Fire, at, radius * 0.5, dps, life, 0.0);
                }
                self.juice.flash(g.to, 6.0, [1.8, 0.9, 0.3], 6.0, 0.3);
                self.juice.burst(g.to, 4.0, 14, [0.0; 2], 0.0, [30.0, 90.0], juice::EMBER, 1.4, 0.5, 1.0);
                let (pan, gain) = self.place(g.to);
                sounds.push(Sfx::Boom { pan, gain: gain / 2 });
            }
            Thrown::Flare { dps, radius, life } => {
                self.lay(Hazard::Flare, g.to, radius, dps, life, 0.0);
                self.juice.ring(g.to, radius, 0.6, [1.0, 0.3, 0.2]);
            }
            Thrown::Lob { def, damage, radius, life, slow } => {
                let d = &book().defs[def];
                match d.lands {
                    Lands::Blast => self.explode(g.to, radius, damage, sounds),
                    Lands::Fire => self.lay(Hazard::Fire, g.to, radius, damage, life, 0.0),
                    Lands::Flare => self.lay(Hazard::Flare, g.to, radius, damage, life, 0.0),
                    Lands::Tacks => self.lay(Hazard::Tacks, g.to, radius, damage, life, slow),
                }
                if let Some(e) = d.effect.as_deref().and_then(|n| super::effects::effects().named(n)) {
                    self.fx.push(Fx::play(e, g.to, 0.0));
                }
            }
        }
    }

    fn lay(&mut self, hazard: Hazard, pos: [f32; 2], radius: f32, dps: f32, life: f32, slow: f32) {
        if self.patches.len() >= MAX_PATCHES {
            self.patches.remove(0);
        }
        self.patches.push(Patch { hazard, pos, radius, dps, slow, age: 0.0, life, tick: 0.0 });
    }

    /// Fire, tacks and flares hurt what's in them a few times a second; burning and slowed zombies wear off.
    pub(super) fn hazards(&mut self, dt: f32) {
        for p in &mut self.patches {
            p.age += dt;
            p.tick += dt;
            if p.tick < PATCH_TICK {
                continue;
            }
            p.tick -= PATCH_TICK;
            for z in &mut self.zombies {
                if z.rise < 1.0 || len(sub(z.pos, p.pos)) > p.radius {
                    continue;
                }
                match p.hazard {
                    Hazard::Tacks => {
                        singe(z, &mut self.juice, p.dps * PATCH_TICK);
                        z.slow = z.slow.max(0.5);
                        z.slow_k = z.slow_k.max(p.slow);
                    }
                    Hazard::Fire | Hazard::Flare => ignite(z, p.dps, crate::tuning::tuning().fire_burn),
                }
            }
            match p.hazard {
                Hazard::Fire => self.juice.burst(p.pos, 4.0, 1, [0.0; 2], 0.0, [5.0, 20.0], juice::EMBER, 1.2, 0.6, 1.0),
                Hazard::Flare => {
                    let flicker = self.juice.rng.range(0.8, 1.2);
                    self.juice.flash(p.pos, 6.0, [2.0 * flicker, 0.5, 0.35], p.radius / 6.0, 0.3);
                    self.juice.burst(p.pos, 6.0, 2, [0.0; 2], 0.0, [10.0, 30.0], [1.0, 0.35, 0.25], 1.2, 0.5, 1.0);
                }
                Hazard::Tacks => {}
            }
        }
        self.patches.retain(|p| p.age < p.life);
        for z in &mut self.zombies {
            z.slow = (z.slow - dt).max(0.0);
            if z.slow <= 0.0 {
                z.slow_k = 0.0;
            }
            if z.burn <= 0.0 {
                z.burn_dps = 0.0;
                continue;
            }
            z.burn -= dt;
            z.burn_tick += dt;
            if z.burn_tick >= BURN_TICK {
                z.burn_tick -= BURN_TICK;
                singe(z, &mut self.juice, z.burn_dps * BURN_TICK);
                self.juice.burst(z.pos, 8.0, 1, [0.0; 2], 0.0, [5.0, 15.0], juice::EMBER, 1.2, 0.5, 1.0);
            }
        }
        for s in &mut self.swings {
            s.age += dt;
        }
        self.swings.retain(|s| s.age < s.life);
        for b in &mut self.bolts {
            b.age += dt;
        }
        self.bolts.retain(|b| b.age < BOLT_LIFE);
        self.beam = (self.beam - dt).max(0.0);
    }

    /// Scarecrows age, and burst when their time is up.
    pub(super) fn decoys(&mut self, dt: f32, sounds: &mut Vec<Sfx>) {
        for d in &mut self.decoys {
            d.age += dt;
        }
        while let Some(i) = self.decoys.iter().position(|d| d.age >= d.life) {
            let d = self.decoys.swap_remove(i);
            self.explode(d.pos, d.blast, d.damage / (self.kit.mods.damage * self.kit.mods.blast), sounds);
        }
    }

    /// Fuel drums bring blown barrels back where they stood, away from the player and the dead.
    pub(super) fn restock(&mut self, dt: f32) {
        if self.kit.mods.drums <= 0.0 {
            return;
        }
        self.drum_t += dt;
        if self.drum_t < self.kit.mods.drums {
            return;
        }
        self.drum_t = 0.0;
        let free: Vec<[f32; 2]> = self
            .barrel_spots
            .iter()
            .copied()
            .filter(|&p| {
                let c = [p[0] + TILE / 2.0, p[1] + TILE / 2.0];
                !self.barrels.iter().any(|b| b.pos == p) && len(sub(c, self.pos)) > 24.0 && !self.zombies.iter().any(|z| len(sub(z.pos, c)) < 12.0)
            })
            .collect();
        if free.is_empty() {
            return;
        }
        let pos = free[self.juice.rng.below(free.len())];
        self.barrels.push(Barrel { pos, hp: crate::tuning::tuning().barrel_hp, flash: 0.0 });
        self.map.close(pos[0] + TILE / 2.0, pos[1] + TILE / 2.0);
        self.flow_cell = [usize::MAX; 2];
        self.fx.push(Fx::play(super::effects::effects().drum_returns, [pos[0] + TILE / 2.0, pos[1] + TILE / 2.0 - 4.0], 0.0));
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn shoot(&mut self, angle: f32, speed: f32, damage: f32, pierce: u32, life: f32, force: f32, look: Option<super::effects::Effect>) {
        let vel = [angle.cos() * speed, angle.sin() * speed];
        self.bullets.push(Bullet { pos: [self.pos[0], self.pos[1] - 6.0], vel, damage, pierce, life, force, hit: Vec::new(), look, age: 0.0 });
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Choice, Kind as ZKind, Phase};
    use super::*;
    use crate::{Buttons, Input};

    #[test]
    fn a_gun_spreads_its_shots_as_its_pattern_says() {
        let all = |p: Pattern, n: u32| (0..n).map(|k| shot_angle(p, k, n, 1.0, 0.5, 0.25)).collect::<Vec<_>>();
        assert_eq!(all(Pattern::Fan, 3), [0.75, 1.0, 1.25]);
        assert_eq!(all(Pattern::Fan, 1), [1.0], "one shot goes straight at the target");
        let ring = all(Pattern::Ring, 4);
        assert!(ring.windows(2).all(|w| (w[1] - w[0] - TAU / 4.0).abs() < 1e-6), "evenly all the way round");
        assert_eq!(all(Pattern::Spray, 2), [1.125, 1.125]);
    }

    fn armed(h: &mut Harvest, id: &str, level: u32) {
        let def = book().index(id).unwrap();
        h.kit.raise(def, level);
    }

    /// A quiet farm with one zombie `at` the given offset, holding the given weapon and nothing else.
    fn with(id: &str, offset: [f32; 2]) -> Harvest {
        let mut h = Harvest::calm(3);
        h.kit.weapons.clear();
        armed(&mut h, id, 1);
        h.view_yaw = 0.0;
        h.aim = [1.0, 0.0];
        h.pos = (0..40).map(|k| [h.pos[0] + k as f32 * 16.0, h.pos[1]]).find(|p| !h.map.hits(p[0] + offset[0], p[1] + offset[1], crate::tuning::tuning().zombie_half())).unwrap();
        h.zombies.push(Zombie::new(ZKind::named("big"), [h.pos[0] + offset[0], h.pos[1] + offset[1]], 500.0, 1.0, 0.0));
        h
    }

    /// Damage done to the first zombie over `secs`, holding it where it stands.
    fn hurt_after(h: &mut Harvest, secs: f32) -> f32 {
        let mut sounds = Vec::new();
        let (start, home) = (h.zombies[0].hp, h.zombies[0].pos);
        for _ in 0..(secs * 60.0) as usize {
            h.fire(1.0 / 60.0, &mut sounds);
            h.move_bullets(1.0 / 60.0, &mut sounds);
            h.hazards(1.0 / 60.0);
            h.decoys(1.0 / 60.0, &mut sounds);
            let mut flow = super::super::Flow::default();
            h.fly(1.0 / 60.0, &mut sounds, &mut flow);
            if let Some(z) = h.zombies.first_mut() {
                z.pos = home;
            }
        }
        start - h.zombies.first().map_or(0.0, |z| z.hp)
    }

    #[test]
    fn every_weapon_in_the_file_hurts_a_zombie_where_its_zone_says() {
        for d in book().defs.iter().filter(|d| d.kind == Kind::Weapon) {
            let offset = match d.zone {
                _ if d.does == Some(Does::Gas) => [-4.0, 0.0],
                Some(super::super::powerups::Zone::Behind) => [-20.0, 0.0],
                _ => [20.0, 0.0],
            };
            let mut h = with(&d.id, offset);
            h.walk = 0.5;
            let hurt = hurt_after(&mut h, 15.0);
            assert!(hurt > 0.0, "{} did nothing to a zombie at {offset:?}", d.id);
        }
    }

    #[test]
    fn ahead_weapons_hold_fire_at_what_is_behind() {
        for id in ["shotgun", "rifle", "pitchfork", "burner"] {
            let mut h = with(id, [-20.0, 0.0]);
            assert_eq!(hurt_after(&mut h, 3.0), 0.0, "{id}");
        }
        let mut h = with("holster", [20.0, 0.0]);
        assert_eq!(hurt_after(&mut h, 3.0), 0.0, "the holster only fires behind");
    }

    #[test]
    fn holding_x_turns_aimed_weapons_on_a_barrel_in_view() {
        for id in ["pistol", "shotgun", "rifle"] {
            let mut h = with(id, [30.0, 0.0]);
            h.barrels.clear();
            h.barrels.push(super::super::Barrel { pos: [h.pos[0] + 60.0, h.pos[1] - 40.0], hp: 1.0, flash: 0.0 });
            let toward = |h: &Harvest| h.bullets.last().map(|b| b.vel[1] < -1.0).unwrap();
            h.kit.weapons[0].cd = 0.0;
            h.fire(1.0 / 60.0, &mut Vec::new());
            assert!(!toward(&h), "{id}: without X it shoots the zombie");
            h.targeting = true;
            h.bullets.clear();
            h.kit.weapons[0].cd = 0.0;
            h.fire(1.0 / 60.0, &mut Vec::new());
            assert!(toward(&h), "{id}: with X it goes for the barrel");
            h.barrels[0].pos = [h.pos[0] - 60.0, h.pos[1]];
            assert!(h.barrel_in_view(200.0).is_none(), "not one behind");
        }
    }

    #[test]
    fn the_pistol_prefers_what_is_ahead() {
        let mut h = with("pistol", [120.0, 0.0]);
        h.zombies.push(Zombie::new(ZKind::named("big"), [h.pos[0] - 90.0, h.pos[1]], 500.0, 1.0, 0.0));
        h.kit.weapons[0].cd = 0.0;
        h.fire(1.0 / 60.0, &mut Vec::new());
        assert!(h.bullets[0].vel[0] > 0.0, "it shot ahead, not at the closer one behind");
        h.zombies[1].pos = [h.pos[0] - 20.0, h.pos[1]];
        h.kit.weapons[0].cd = 0.0;
        h.fire(1.0 / 60.0, &mut Vec::new());
        assert!(h.bullets.last().unwrap().vel[0] < 0.0, "but one at point blank comes first");
    }

    #[test]
    fn passives_add_up_and_scale_weapons() {
        let mut h = with("knife", [10.0, 0.0]);
        let knife = h.kit.weapons[0];
        let before = h.stat(knife, Stat::Damage);
        armed(&mut h, "hollow_points", 2);
        assert!((h.stat(knife, Stat::Damage) - before * 1.2).abs() < 1e-3);
        armed(&mut h, "canned_peaches", 1);
        assert_eq!(h.kit.max_hp(), 120.0);
    }

    #[test]
    fn slots_and_top_levels_limit_the_cards() {
        let b = book();
        let mut h = Harvest::new(1).with_progress(super::super::Progress { everything: true, ..Default::default() });
        h.kit.weapons.clear();
        for d in b.defs.iter().enumerate().filter(|(_, d)| d.kind == Kind::Weapon).map(|(i, _)| i).take(b.weapon_slots) {
            h.kit.raise(d, 1);
        }
        assert_eq!(h.kit.weapons.len(), b.weapon_slots);
        for _ in 0..50 {
            for c in h.roll_choices() {
                let d = &b.defs[c.up];
                assert!(d.kind != Kind::Weapon || h.kit.level(c.up) > 0, "{} offered with no weapon slot free", d.id);
            }
        }
        let pistol = b.index("pistol").unwrap();
        h.kit.raise(pistol, 99);
        assert_eq!(h.kit.level(pistol), b.defs[pistol].max);
        assert!(!h.kit.offerable(pistol));
    }

    #[test]
    fn with_everything_taken_the_cards_offer_fillers() {
        let b = book();
        let mut h = Harvest::new(1);
        h.kit.weapons.clear();
        h.kit.passives.clear();
        for (i, d) in b.defs.iter().enumerate().filter(|(_, d)| d.unlock == super::super::powerups::Unlock::Start && d.kind != Kind::Filler) {
            h.kit.raise(i, d.max);
        }
        let choices = h.roll_choices();
        assert!(choices.iter().all(|c| b.defs[c.up].kind == Kind::Filler));
        h.hp = 10.0;
        let snack = choices.iter().position(|c| b.defs[c.up].id == "snack").unwrap();
        h.phase = Phase::LevelUp { choices, selected: snack, t: 1.0, picked: None };
        h.update(&Input { pressed: Buttons::X, ..Default::default() }, 0.016, &mut Vec::new());
        assert!(h.hp > 10.0);
        let _ = Choice { up: 0, rare: false, found: false };
    }

    #[test]
    fn locked_powerups_stay_off_the_cards() {
        let b = book();
        let mut h = Harvest::new(1);
        for _ in 0..100 {
            for c in h.roll_choices() {
                assert_eq!(b.defs[c.up].unlock, super::super::powerups::Unlock::Start, "{}", b.defs[c.up].id);
            }
        }
    }

    #[test]
    fn fuel_drums_bring_a_blown_barrel_back() {
        let mut h = Harvest::new(6);
        armed(&mut h, "fuel_drums", 1);
        let spot = h.barrels[0].pos;
        let c = h.barrels.swap_remove(0).center();
        h.map.open(c[0], c[1]);
        h.zombies.clear();
        h.pos = [spot[0] + 200.0, spot[1]];
        let others: Vec<[f32; 2]> = h.barrels.iter().map(|b| b.pos).collect();
        h.barrel_spots.retain(|p| !others.contains(p));
        h.restock(h.kit.mods.drums + 0.1);
        assert!(h.barrels.iter().any(|b| b.pos == spot));
        assert!(h.map.solid_at(c[0], c[1]));
    }

    #[test]
    fn a_scarecrow_draws_zombies_then_bursts() {
        let mut h = with("scarecrow", [-40.0, 0.0]);
        h.kit.weapons[0].cd = 0.0;
        h.fire(0.016, &mut Vec::new());
        assert_eq!(h.decoys.len(), 1);
        let decoy = h.decoys[0].pos;
        let z = h.zombies[0].pos;
        let before = len(sub(z, decoy));
        h.move_zombies(0.2);
        assert!(len(sub(h.zombies[0].pos, decoy)) < before);
        let life = h.decoys[0].life;
        h.decoys(life, &mut Vec::new());
        assert!(h.decoys.is_empty());
    }

    #[test]
    fn an_idle_dog_wanders_near_the_player_and_catches_up_when_left() {
        let mut h = with("dog", [0.0, 0.0]);
        h.zombies.clear();
        let mut spots = Vec::new();
        for k in 0..60 * 20 {
            h.walk_dogs(1.0 / 60.0, &mut Vec::new());
            let d = &h.dogs[0];
            assert!(len(sub(d.pos, h.pos)) < crate::tuning::tuning().dog_stray + 4.0, "it stays close");
            if k % 60 == 0 {
                spots.push(d.pos);
            }
        }
        let roamed = spots.iter().map(|a| spots.iter().map(|b| len(sub(*a, *b))).fold(0.0, f32::max)).fold(0.0, f32::max);
        assert!(roamed > 12.0, "it moved about while the player stood still, {roamed} px at most");
        h.pos[0] += 120.0;
        for _ in 0..60 * 3 {
            h.walk_dogs(1.0 / 60.0, &mut Vec::new());
        }
        assert!(len(sub(h.dogs[0].pos, h.pos)) < crate::tuning::tuning().dog_stray, "it ran to catch up");
    }

    #[test]
    fn a_met_unlock_turns_up_as_a_find_only_with_room_and_stays_once_taken() {
        let b = book();
        let scythe = b.index("scythe").unwrap();
        let mut h = Harvest::new(1).with_progress(super::super::Progress { kills: 100, ..Default::default() });
        let mut seen = 0;
        for _ in 0..200 {
            for c in h.roll_choices().iter().filter(|c| c.found) {
                assert_eq!(c.up, scythe, "only what has met its unlock");
                seen += 1;
            }
        }
        assert!(seen > 10, "it turns up now and then, {seen} times in 200");
        let full = h.kit.weapons.len();
        for id in ["rifle", "lantern", "burner", "pitchfork", "crows", "zapper"].iter().take(b.weapon_slots - full) {
            armed(&mut h, id, 1);
        }
        assert!((0..200).all(|_| h.roll_choices().iter().all(|c| !c.found)), "no free weapon slot, no weapon find");
        h.kit.weapons.pop();
        let choices = loop {
            let c = h.roll_choices();
            if c.iter().any(|c| c.found) {
                break c;
            }
        };
        let at = choices.iter().position(|c| c.found).unwrap();
        h.phase = Phase::LevelUp { choices, selected: at, t: 1.0, picked: None };
        let flow = h.update(&Input { pressed: Buttons::X, ..Default::default() }, 0.016, &mut Vec::new());
        assert_eq!(flow.found, 1 << scythe, "the game keeps it right away");
        assert!(h.progress.owns(scythe) && h.kit.level(scythe) == 1);
        assert!((0..50).all(|_| h.roll_choices().iter().all(|c| !c.found)), "nothing else is waiting");
    }

    #[test]
    fn a_run_finds_two_powerups_at_most() {
        let b = book();
        let mut h = Harvest::new(3).with_progress(super::super::Progress { kills: 9999, best: 9999, best_level: 99, nights: 99, dawns: 99, ..Default::default() });
        h.t = 900.0;
        h.newly = vec![b.index("scythe").unwrap(); b.finds];
        for _ in 0..100 {
            h.chest = true;
            assert!(h.roll_choices().iter().all(|c| !c.found), "not even from a chest");
        }
        h.newly.pop();
        h.chest = true;
        assert!(h.roll_choices().iter().any(|c| c.found));
    }

    #[test]
    fn a_chest_always_holds_a_find_when_one_is_waiting() {
        let mut h = Harvest::new(2).with_progress(super::super::Progress { kills: 100, ..Default::default() });
        for _ in 0..20 {
            h.chest = true;
            assert!(h.roll_choices().iter().any(|c| c.found));
        }
        let mut h = Harvest::new(2);
        h.chest = true;
        assert!(h.roll_choices().iter().all(|c| !c.found), "with nothing waiting it's a plain level-up");
    }
}
