//! Feedback that makes the night feel alive: particles, damage numbers, falling bodies, shockwaves,
//! light flashes, screen shake and hit-stop. None of it changes the outcome of a run.

use std::f32::consts::TAU;

use crate::art::Art;
use crate::rng::Rng;

const MAX_PARTICLES: usize = 400;
const MAX_POPUPS: usize = 40;
const MAX_CORPSES: usize = 60;
const MAX_FLASHES: usize = 4;
const POPUP_TIME: f32 = 0.7;
pub(super) const CORPSE_FALL: f32 = 0.22;
const CORPSE_STAY: f32 = 7.0;
const CORPSE_SINK: f32 = 1.2;
/// Trauma lost per second; shake is trauma squared, so small knocks fade fast.
const TRAUMA_DECAY: f32 = 1.6;
/// No new hit-stop for this long after one ends, so a swarm of kills doesn't stutter.
const HITSTOP_REST: f32 = 0.35;

pub const BLOOD: [f32; 3] = [0.62, 0.08, 0.06];
pub const DARK_BLOOD: [f32; 3] = [0.35, 0.03, 0.03];
pub const DIRT: [f32; 3] = [0.42, 0.3, 0.2];
pub const SPARK: [f32; 3] = [1.0, 0.85, 0.4];
pub const EMBER: [f32; 3] = [1.0, 0.5, 0.15];
pub const GOLD: [f32; 3] = [1.0, 0.85, 0.3];
pub const FIREFLY: [f32; 3] = [0.75, 1.0, 0.45];

/// A speck in the world, in map pixels with `z` its height above the ground.
pub struct Particle {
    pub pos: [f32; 2],
    pub z: f32,
    pub vel: [f32; 2],
    pub vz: f32,
    pub age: f32,
    pub life: f32,
    pub color: [f32; 3],
    /// Width in map pixels; it shrinks to nothing over its life when `shrink` is set.
    pub size: f32,
    pub shrink: bool,
    pub gravity: f32,
    pub drag: f32,
    pub glow: f32,
    /// Blinks and drifts, ignoring gravity.
    pub wander: bool,
}

impl Particle {
    pub fn size_now(&self) -> f32 {
        let k = if self.shrink { 1.0 - self.age / self.life } else { 1.0 };
        let blink = if self.wander { 0.5 + 0.5 * (self.age * 3.0 + self.pos[0]).sin().max(0.0) } else { 1.0 };
        self.size * k * blink
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Hit,
    Crit,
    /// Damage the player took.
    Hurt,
    Heal,
}

/// A number drifting up from where it landed.
pub struct Popup {
    pub pos: [f32; 2],
    pub value: u32,
    pub tone: Tone,
    pub age: f32,
}

impl Popup {
    /// Height above the ground in map pixels, and how big it is now: a quick pop, then easing up and away.
    pub fn rise(&self) -> (f32, f32) {
        let k = self.age / POPUP_TIME;
        let pop = if k < 0.15 { 0.6 + k / 0.15 * 0.7 } else { 1.3 - (k - 0.15) * 0.5 };
        (18.0 + 14.0 * (1.0 - (1.0 - k).powi(2)), pop * if self.tone == Tone::Hit { 1.0 } else { 1.4 })
    }

    pub fn fade(&self) -> f32 {
        (1.0 - (self.age / POPUP_TIME - 0.7) / 0.3).clamp(0.0, 1.0)
    }
}

/// A zombie that has stopped: it topples over, lies there a while, then sinks into the dirt.
pub struct Corpse {
    pub art: Art,
    /// Its own dead frames, per facing, and the way the body faces; they tip over in place of `art`.
    pub dead: Option<([Art; 3], [f32; 2])>,
    pub flip: bool,
    pub pos: [f32; 2],
    pub z: f32,
    pub vel: [f32; 2],
    pub vz: f32,
    /// Direction it falls toward, in map space.
    pub fall: [f32; 2],
    pub age: f32,
    pub spin: f32,
}

impl Corpse {
    /// How far over it has toppled, 0 standing to 1 flat.
    pub fn tipped(&self) -> f32 {
        let k = (self.age / CORPSE_FALL).min(1.0);
        k * k
    }

    /// How far it has sunk into the ground, in map pixels.
    pub fn sunk(&self) -> f32 {
        ((self.age - CORPSE_STAY) / CORPSE_SINK).clamp(0.0, 1.0) * 8.0
    }
}

/// A ring rolling out across the ground from a blast or a level-up.
pub struct Ring {
    pub center: [f32; 2],
    pub radius: f32,
    pub age: f32,
    pub life: f32,
    pub color: [f32; 3],
}

impl Ring {
    pub fn now(&self) -> f32 {
        let k = self.age / self.life;
        self.radius * (1.0 - (1.0 - k).powi(3))
    }
}

/// A brief light in the world, in map pixels with a height.
pub struct Flash {
    pub pos: [f32; 2],
    pub z: f32,
    pub color: [f32; 3],
    /// Reach in tiles at full strength.
    pub range: f32,
    pub age: f32,
    pub life: f32,
}

impl Flash {
    pub fn strength(&self) -> f32 {
        (1.0 - self.age / self.life).max(0.0)
    }
}

/// A line of text across the top screen that slides in and fades out.
pub struct Banner {
    pub text: String,
    pub color: [f32; 4],
    pub age: f32,
    pub life: f32,
    /// Huge pixel letters that punch in, for the level-up.
    pub big: bool,
}

/// Money on its way from where it was picked up to the XP bar.
pub struct Mote {
    pub pos: [f32; 2],
    pub age: f32,
}

pub const MOTE_TIME: f32 = 0.4;

pub struct Juice {
    pub rng: Rng,
    pub motes: Vec<Mote>,
    pub particles: Vec<Particle>,
    pub popups: Vec<Popup>,
    pub corpses: Vec<Corpse>,
    pub rings: Vec<Ring>,
    pub flashes: Vec<Flash>,
    pub banner: Option<Banner>,
    /// 0 to 1; the camera shakes by its square.
    pub trauma: f32,
    /// Camera recoil from firing, in radians of pitch, springing back.
    pub kick: f32,
    /// Extra field of view in degrees, springing back.
    pub punch: f32,
    pub hitstop: f32,
    rest: f32,
    /// Seconds of the red flash over the top screen after a hit.
    pub hurt: f32,
    /// Real time, which drives the shake noise.
    pub clock: f32,
}

impl Juice {
    pub fn new(seed: u64) -> Self {
        Self {
            rng: Rng::new(seed ^ 0x5EED_1E55),
            motes: Vec::new(),
            particles: Vec::new(),
            popups: Vec::new(),
            corpses: Vec::new(),
            rings: Vec::new(),
            flashes: Vec::new(),
            banner: None,
            trauma: 0.0,
            kick: 0.0,
            punch: 0.0,
            hitstop: 0.0,
            rest: 0.0,
            hurt: 0.0,
            clock: 0.0,
        }
    }

    /// Springs, shake and hit-stop run on real time, even while the world is frozen.
    pub fn tick_real(&mut self, dt: f32) {
        self.clock += dt;
        self.trauma = (self.trauma - TRAUMA_DECAY * dt).max(0.0);
        self.kick *= (1.0 - 18.0 * dt).max(0.0);
        self.punch *= (1.0 - 10.0 * dt).max(0.0);
        self.hurt = (self.hurt - dt).max(0.0);
        if self.hitstop > 0.0 {
            self.hitstop -= dt;
        } else {
            self.rest = (self.rest - dt).max(0.0);
        }
        for m in &mut self.motes {
            m.age += dt;
        }
        self.motes.retain(|m| m.age < MOTE_TIME);
        if let Some(b) = &mut self.banner {
            b.age += dt;
            if b.age > b.life {
                self.banner = None;
            }
        }
    }

    /// Everything that moves with the world, on world time.
    pub fn tick(&mut self, dt: f32) {
        for p in &mut self.particles {
            p.age += dt;
            if p.wander {
                let a = (p.age * 0.7 + p.pos[1] * 0.05).sin() * TAU;
                p.vel = [p.vel[0] * 0.98 + a.cos() * 6.0 * dt, p.vel[1] * 0.98 + a.sin() * 6.0 * dt];
                p.z += (p.age * 1.3).sin() * 4.0 * dt;
            } else {
                p.vz -= p.gravity * dt;
                p.z += p.vz * dt;
                if p.z < 0.0 {
                    p.z = 0.0;
                    p.vz = -p.vz * 0.3;
                    p.vel = [p.vel[0] * 0.5, p.vel[1] * 0.5];
                }
            }
            let drag = (1.0 - p.drag * dt).max(0.0);
            p.vel = [p.vel[0] * drag, p.vel[1] * drag];
            p.pos = [p.pos[0] + p.vel[0] * dt, p.pos[1] + p.vel[1] * dt];
        }
        self.particles.retain(|p| p.age < p.life);
        for p in &mut self.popups {
            p.age += dt;
        }
        self.popups.retain(|p| p.age < POPUP_TIME);
        for c in &mut self.corpses {
            c.age += dt;
            if c.z > 0.0 || c.vz > 0.0 {
                c.vz -= 400.0 * dt;
                c.z = (c.z + c.vz * dt).max(0.0);
                c.pos = [c.pos[0] + c.vel[0] * dt, c.pos[1] + c.vel[1] * dt];
                c.spin += dt * 12.0;
                if c.z == 0.0 {
                    c.vz = 0.0;
                }
            } else {
                let drag = (1.0 - 10.0 * dt).max(0.0);
                c.vel = [c.vel[0] * drag, c.vel[1] * drag];
                c.pos = [c.pos[0] + c.vel[0] * dt, c.pos[1] + c.vel[1] * dt];
            }
        }
        self.corpses.retain(|c| c.age < CORPSE_STAY + CORPSE_SINK);
        for r in &mut self.rings {
            r.age += dt;
        }
        self.rings.retain(|r| r.age < r.life);
        for f in &mut self.flashes {
            f.age += dt;
        }
        self.flashes.retain(|f| f.age < f.life);
    }

    pub fn frozen(&self) -> bool {
        self.hitstop > 0.0
    }

    /// Freezes the world for `secs`, unless it was frozen a moment ago.
    pub fn stop(&mut self, secs: f32) {
        if self.rest <= 0.0 && self.hitstop <= 0.0 {
            self.hitstop = secs;
            self.rest = HITSTOP_REST;
        }
    }

    pub fn shake(&mut self, amount: f32) {
        self.trauma = (self.trauma + amount).min(1.0);
    }

    /// Camera offset in tiles for the current trauma: right, up, and a roll in radians.
    pub fn shake_offset(&self) -> [f32; 3] {
        let s = self.trauma * self.trauma;
        let t = self.clock * 31.0;
        let n = |k: f32| (t * k).sin() * 0.6 + (t * k * 2.3 + 1.7).sin() * 0.4;
        [n(1.0) * 0.25 * s, n(1.37) * 0.18 * s, n(0.71) * 0.06 * s]
    }

    pub fn particle(&mut self, p: Particle) {
        if self.particles.len() >= MAX_PARTICLES {
            self.particles.swap_remove(0);
        }
        self.particles.push(p);
    }

    /// `n` specks thrown out from `at` at `height`, spraying around `dir` (any way when zero) by up to `spread` radians.
    #[allow(clippy::too_many_arguments)]
    pub fn burst(&mut self, at: [f32; 2], height: f32, n: usize, dir: [f32; 2], spread: f32, speed: [f32; 2], color: [f32; 3], size: f32, life: f32, glow: f32) {
        let base = if dir == [0.0, 0.0] { None } else { Some(dir[1].atan2(dir[0])) };
        for _ in 0..n {
            let a = match base {
                Some(b) => b + self.rng.range(-spread, spread),
                None => self.rng.range(0.0, TAU),
            };
            let v = self.rng.range(speed[0], speed[1]);
            let shade = self.rng.range(0.8, 1.15);
            let vz = self.rng.range(20.0, 70.0) * speed[1] / 80.0;
            let life = life * self.rng.range(0.7, 1.3);
            let size = size * self.rng.range(0.7, 1.3);
            self.particle(Particle {
                pos: at,
                z: height,
                vel: [a.cos() * v, a.sin() * v],
                vz,
                age: 0.0,
                life,
                color: color.map(|c| (c * shade).min(1.0)),
                size,
                shrink: glow > 0.0,
                gravity: 260.0,
                drag: 2.0,
                glow,
                wander: false,
            });
        }
    }

    pub fn popup(&mut self, pos: [f32; 2], amount: f32, tone: Tone) {
        if self.popups.len() >= MAX_POPUPS {
            self.popups.remove(0);
        }
        let jitter = [self.rng.range(-4.0, 4.0), self.rng.range(-2.0, 2.0)];
        self.popups.push(Popup { pos: [pos[0] + jitter[0], pos[1] + jitter[1]], value: amount.round().max(1.0) as u32, tone, age: 0.0 });
    }

    pub fn corpse(&mut self, c: Corpse) {
        if self.corpses.len() >= MAX_CORPSES {
            self.corpses.remove(0);
        }
        self.corpses.push(c);
    }

    pub fn ring(&mut self, center: [f32; 2], radius: f32, life: f32, color: [f32; 3]) {
        self.rings.push(Ring { center, radius, age: 0.0, life, color });
    }

    pub fn flash(&mut self, pos: [f32; 2], z: f32, color: [f32; 3], range: f32, life: f32) {
        if self.flashes.len() >= MAX_FLASHES {
            let weakest = self.flashes.iter().enumerate().min_by(|a, b| a.1.strength().total_cmp(&b.1.strength())).map(|(i, _)| i).unwrap();
            self.flashes.swap_remove(weakest);
        }
        self.flashes.push(Flash { pos, z, color, range, age: 0.0, life });
    }

    pub fn banner(&mut self, text: impl Into<String>, color: [f32; 4], life: f32) {
        self.banner = Some(Banner { text: text.into(), color, age: 0.0, life, big: false });
    }

    pub fn shout(&mut self, text: impl Into<String>, color: [f32; 4], life: f32) {
        self.banner = Some(Banner { text: text.into(), color, age: 0.0, life, big: true });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_stop_rests_before_it_can_freeze_again() {
        let mut j = Juice::new(1);
        j.stop(0.05);
        assert!(j.frozen());
        j.tick_real(0.06);
        assert!(!j.frozen());
        j.stop(0.05);
        assert!(!j.frozen());
        j.tick_real(HITSTOP_REST);
        j.stop(0.05);
        assert!(j.frozen());
    }

    #[test]
    fn particles_fall_bounce_and_expire() {
        let mut j = Juice::new(1);
        j.burst([0.0, 0.0], 10.0, 20, [1.0, 0.0], 0.5, [40.0, 80.0], BLOOD, 2.0, 0.5, 0.0);
        assert_eq!(j.particles.len(), 20);
        for _ in 0..20 {
            j.tick(1.0 / 60.0);
        }
        assert!(j.particles.iter().all(|p| p.z >= 0.0 && p.pos[0] > 0.0));
        j.tick(1.0);
        assert!(j.particles.is_empty());
    }

    #[test]
    fn a_corpse_topples_lies_still_then_sinks_away() {
        let mut j = Juice::new(1);
        j.corpse(Corpse { art: Art(0), dead: None, flip: false, pos: [0.0; 2], z: 0.0, vel: [0.0; 2], vz: 0.0, fall: [1.0, 0.0], age: 0.0, spin: 0.0 });
        j.tick(CORPSE_FALL);
        assert_eq!(j.corpses[0].tipped(), 1.0);
        assert_eq!(j.corpses[0].sunk(), 0.0);
        j.tick(CORPSE_STAY);
        assert!(j.corpses[0].sunk() > 0.0);
        j.tick(CORPSE_SINK);
        assert!(j.corpses.is_empty());
    }
}
