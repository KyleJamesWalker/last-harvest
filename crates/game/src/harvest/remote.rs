//! Online co-op's client end: a night drawn from the host's snapshots. The client never runs the rules; each frame
//! it lays the `Mirror` into the run's own fields, so the drawing, HUD and maps work as in any run, and plays the
//! host's events as sounds and gore.

use crate::fmath as fm;
use std::collections::HashMap;
use std::f32::consts::TAU;

use super::effects::{self, Effect};
use super::local::Cards;
use super::player::{ME, PlayerId};
use super::weapons::{Bolt, Decoy, Hazard, Patch};
use super::*;
use crate::netsync::snapshot::*;
use crate::netsync::{Event, Mirror, Spawn, Spawned};

/// What a client keeps between frames besides the mirror.
#[derive(Default)]
pub(super) struct Client {
    /// The host's seat each place in `players` stands for: the local players first.
    pub order: Vec<usize>,
    kits: Vec<Vec<(u8, u8)>>,
    tells: Vec<([f32; 2], f32)>,
    /// When each local view's cards came up, and what they were.
    cards: Vec<(f32, [(u8, u8); 3])>,
    over: f32,
    hp: Vec<f32>,
    looks: HashMap<u16, Effect>,
}

impl Client {
    fn index(&self, seat: usize) -> Option<PlayerId> {
        self.order.iter().position(|&s| s == seat)
    }
}

/// The effect whose first frame is `art`.
fn effect_of(looks: &mut HashMap<u16, Effect>, art: u16) -> Option<Effect> {
    if looks.is_empty() {
        let all = effects::effects();
        for e in effects::NAMES.iter().filter_map(|n| all.named(n)).chain(all.extra.iter().map(|x| x.1)) {
            if let Some(a) = e.frames.first() {
                looks.entry(a.0).or_insert(e);
            }
        }
    }
    looks.get(&art).copied()
}

impl Harvest {
    /// Turns this run into a client's picture of the host's: `local` are this device's seats, in view order, and
    /// `seats` every seat in the game.
    pub fn client_of(mut self, local: &[usize], seats: &[usize]) -> Self {
        let mut order: Vec<usize> = local.to_vec();
        order.extend(seats.iter().copied().filter(|s| !local.contains(s)));
        self.net.role = online::Role::Client;
        self.net.seats = order.clone();
        self.juice.still = true;
        self.net.client = Some(Box::new(Client { order, ..Client::default() }));
        self
    }

    pub fn is_client(&self) -> bool {
        self.net.client.is_some()
    }

    /// This device's seats, in view order.
    pub fn local_seats(&self) -> Vec<usize> {
        self.net.client.as_ref().map_or_else(Vec::new, |c| c.order.iter().take(self.views).copied().collect())
    }

    /// Whether the host holds every player in place just now.
    pub fn held(&self) -> bool {
        self.menu.is_some() || !matches!(self.phase, Phase::Playing)
    }

    /// A client leaves from its own pause menu's last row, or from the end screen.
    pub fn client_wants_out(&self, pressed: Buttons) -> bool {
        match self.phase {
            Phase::Over { t, .. } => t > 1.0 && pressed.any(Buttons::A | Buttons::B | Buttons::START),
            _ => self.menu == Some(PAUSE_CHOICES.len() - 1) && self.paused_by < self.views && pressed.any(Buttons::A | Buttons::X),
        }
    }

    /// A seat that joined after the run began.
    pub fn client_seat(&mut self, seat: usize, look: &str) {
        let Some(c) = self.net.client.as_mut() else { return };
        if !c.order.contains(&seat) {
            c.order.push(seat);
            self.net.seats = c.order.clone();
        }
        let n = c.order.len();
        while self.players.len() < n {
            let pos = self.map.spawn;
            let look = super::players::look(look);
            let pl = self.new_player(pos, look);
            self.players.push(pl);
        }
        self.inputs.resize(self.players.len(), Input::default());
    }

    /// Plays one of the host's events.
    pub fn client_event(&mut self, ev: Event, sounds: &mut Vec<Sfx>) {
        match ev {
            Event::Sound(s) => sounds.push(s),
            Event::SoundAt(s, pos) => {
                let (pan, gain) = self.place(unq_pos(pos));
                sounds.push(match s {
                    Sfx::Boom { .. } => Sfx::Boom { pan, gain },
                    Sfx::Rise { .. } => Sfx::Rise { pan, gain },
                    Sfx::Groan { .. } => Sfx::Groan { pan, gain },
                    Sfx::Caw { .. } => Sfx::Caw { pan, gain },
                    Sfx::Spit { .. } => Sfx::Spit { pan, gain },
                    Sfx::Splat { .. } => Sfx::Splat { pan, gain },
                    other => other,
                });
            }
            Event::Kill { kind, pos, by, .. } => {
                let pos = unq_pos(pos);
                let Some(s) = roster().enemies.get(kind as usize) else { return };
                let killer = self.net.client.as_ref().and_then(|c| c.index(by as usize)).unwrap_or(ME);
                let fall = norm(sub(pos, self.players[killer].pos));
                self.gore(pos, fall, 0.0, fall, &s.look, s.gibs as usize, s.blood);
                self.kill_bump = 0.0;
            }
            Event::LevelUp { .. } | Event::LevelDone | Event::RunEnd { .. } | Event::Stop { .. } => {}
        }
    }

    /// Shots, spits, bolts and effects that just began on the host.
    pub fn client_spawned(&mut self, fresh: &[Spawned]) {
        let Some(c) = self.net.client.as_mut() else { return };
        for s in fresh {
            match &s.what {
                Spawn::Bolt { points } => self.bolts.push(Bolt { points: points.iter().map(|&p| unq_pos(p)).collect(), age: 0.0 }),
                &Spawn::Fx { art, pos, angle } => {
                    if let Some(e) = effect_of(&mut c.looks, art) {
                        self.fx.push(Fx::play(e, unq_pos(pos), angle as f32 / 256.0 * TAU));
                    }
                }
                Spawn::Shot { .. } | Spawn::Spit { .. } => {}
            }
        }
    }

    /// One frame of a client's run: everything laid out from `mirror`, and the effects only this end plays.
    pub fn client_frame(&mut self, mirror: &Mirror, dt: f32, sounds: &mut Vec<Sfx>) {
        let Some(mut c) = self.net.client.take() else { return };
        self.juice.still = true;
        self.take_before();
        self.lay_players(&mut c, mirror, dt, sounds);
        self.lay_zombies(mirror, dt);
        self.pin_leaders();
        self.lay_things(&mut c, mirror, dt);
        self.lay_shots(&c, mirror);
        self.lay_run(&mut c, mirror, dt);
        self.net.client = Some(c);
        self.juice.tick_real(dt);
        self.juice.tick(dt);
        for v in 1..self.views {
            self.feel_for(v, |j| j.tick_feel(dt));
        }
        self.glory = (self.glory - dt).max(0.0);
        for f in &mut self.fx {
            f.t += dt;
        }
        self.fx.retain(|f| !f.done());
        for b in &mut self.bolts {
            b.age += dt;
        }
        self.bolts.retain(|b| b.age < 0.3);
        self.feel(dt);
        for v in 1..self.views {
            self.relabel(v);
            self.feel_hp(dt);
            self.relabel(v);
        }
        self.flock(dt, &mut Vec::new());
        self.remember(dt);
    }

    fn lay_players(&mut self, c: &mut Client, mirror: &Mirror, dt: f32, sounds: &mut Vec<Sfx>) {
        let seats = mirror.player_count().min(last_harvest_lobby::MAX_SEATS as usize);
        for seat in 0..seats {
            if !c.order.contains(&seat) {
                c.order.push(seat);
            }
        }
        self.net.seats = c.order.clone();
        while self.players.len() < c.order.len() {
            let look = super::players::look("");
            let pl = self.new_player(self.map.spawn, look);
            self.players.push(pl);
        }
        self.inputs.resize(self.players.len(), Input::default());
        c.kits.resize(self.players.len(), Vec::new());
        c.hp.resize(self.players.len(), f32::MAX);
        for (i, &seat) in c.order.iter().enumerate() {
            let Some(ps) = mirror.player_state(seat) else { continue };
            let (pos, yaw) = mirror.player(seat).unwrap_or((unq_pos(ps.pos), unq_angle(ps.yaw)));
            let difficulty = self.difficulty;
            let pl = &mut self.players[i];
            let d = sub(pos, pl.pos);
            let moved = len(d) > 0.05;
            pl.walk = if moved || ps.flags & P_WALKING != 0 { pl.walk + dt } else { 0.0 };
            if moved {
                pl.heading = norm(d);
            }
            pl.pos = pos;
            pl.view_yaw = yaw;
            pl.aim = [fm::cos(yaw), fm::sin(yaw)];
            pl.facing = Facing::of(pl.aim);
            pl.hurt = ps.hurt as f32 / 100.0;
            let was_down = pl.down.is_some();
            pl.down = (ps.flags & P_DOWN != 0).then_some(ps.down as f32 / 10.0);
            if i < self.views && !was_down && pl.down.is_some() && c.hp[i] != f32::MAX && !matches!(mirror.run.phase, PHASE_WON | PHASE_LOST) {
                sounds.push(Sfx::GameOver);
            }
            pl.lead = match ps.flags {
                f if f & P_LEADING != 0 => Some(super::leader::Lead::Leading(ps.lead)),
                f if f & P_PICKING != 0 => Some(super::leader::Lead::Picking { at: pos, left: ps.lead as f32 / 10.0 }),
                _ => None,
            };
            pl.lead_hp = ps.lead_hp as f32 / 255.0;
            pl.grenades = ps.grenades as f32 / 16.0;
            pl.ready_flash = if ps.flags & P_READY_FLASH != 0 { pl.ready_flash.max(0.01) } else { 0.0 };
            if c.kits[i] != ps.kit {
                let build: Vec<(usize, u32)> = ps.kit.iter().map(|&(d, l)| (d as usize, l as u32)).filter(|&(d, _)| d < book().defs.len()).collect();
                pl.kit = pl.kit.rebuilt(&build, difficulty);
                c.kits[i] = ps.kit.clone();
            }
            pl.hp = ps.hp as f32 / 4.0;
            if i < self.views && pl.hp < c.hp[i] - 0.5 && pl.down.is_none() {
                self.ouch(i, 0.25);
                sounds.push(Sfx::Hurt);
            }
            c.hp[i] = self.players[i].hp;
        }
    }

    fn lay_zombies(&mut self, mirror: &Mirror, dt: f32) {
        let mut old: HashMap<u32, Zombie> = self.zombies.drain(..).chain(self.burrowed.drain(..)).map(|z| (z.id, z)).collect();
        let roster = roster();
        for (pos, s) in mirror.zombies() {
            if s.kind as usize >= roster.enemies.len() {
                continue;
            }
            let kind = Kind(s.kind);
            let full = kind.stats().hp;
            let rising = s.flags & Z_RISING != 0;
            let mut z = old.remove(&s.id).unwrap_or_else(|| {
                let mut z = Zombie::new(kind, pos, full, if rising { 0.0 } else { 1.0 }, s.id as f32 * 0.37);
                z.id = s.id;
                z
            });
            let d = sub(pos, z.pos);
            if len(d) > 0.01 {
                z.dir = norm(d);
                z.anim += dt;
            }
            z.facing = Facing::of(z.dir);
            z.pos = pos;
            z.kind = kind;
            z.hp = s.hp as f32 / 255.0 * full;
            z.hurt = if s.flags & Z_HURT != 0 { z.hurt.max(0.05) } else { (z.hurt - dt).max(0.0) };
            z.flash = (z.flash - dt).max(0.0);
            z.burn = if s.flags & Z_BURNING != 0 { 1.0 } else { 0.0 };
            (z.slow, z.slow_k) = if s.flags & Z_SLOWED != 0 { (1.0, 0.5) } else { (0.0, 0.0) };
            z.leader = s.flags & Z_LEADER != 0;
            z.boss = s.flags & Z_BOSS != 0;
            z.rise = if rising { (z.rise + dt).min(0.95) } else { 1.0 };
            if s.flags & Z_BURROWED != 0 {
                self.burrowed.push(z);
            } else {
                self.zombies.push(z);
            }
        }
    }

    fn lay_things(&mut self, c: &mut Client, mirror: &Mirror, dt: f32) {
        let now = mirror.render_tick().max(0.0) as u32 as u16;
        let age = |born: u16| now.wrapping_sub(born) as f32 / TICK_HZ as f32;
        let (mut drops, mut thrown, mut patches, mut barrels, mut decoys, mut tells) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let mut tell_t = Vec::new();
        for t in mirror.things() {
            let pos = unq_pos(t.pos);
            match t.kind {
                T_GRENADE => {
                    let kind = match t.sub {
                        0 => Thrown::Grenade,
                        1 => Thrown::Bomblet,
                        2 => Thrown::Molotov { dps: 0.0, radius: 0.0, life: 0.0 },
                        3 => Thrown::Flare { dps: 0.0, radius: 0.0, life: 0.0 },
                        n => Thrown::Lob { def: (n - 4) as usize, damage: 0.0, radius: 0.0, life: 0.0, slow: 0.0 },
                    };
                    if let Thrown::Lob { def, .. } = kind
                        && def >= book().defs.len()
                    {
                        continue;
                    }
                    let by = if let Thrown::Lob { def, .. } = kind { Some(def) } else { None };
                    thrown.push(Grenade { from: pos, to: unq_pos([t.data[0], t.data[1]]), t: age(t.data[2]), kind, by, owner: ME });
                }
                T_MONEY | T_HEALTH | T_CHEST => {
                    let kind = match t.kind {
                        T_MONEY => DropKind::Money(t.data[0] as u32),
                        T_HEALTH => DropKind::Health,
                        _ => DropKind::Chest,
                    };
                    drops.push(Drop { kind, pos, z: 0.0, vz: 0.0, vel: [0.0; 2], age: 1.0, pulled: None, to: ME });
                }
                T_PATCH => {
                    let hazard = match t.sub {
                        0 => Hazard::Tacks,
                        1 => Hazard::Fire,
                        _ => Hazard::Flare,
                    };
                    patches.push(Patch::shown(hazard, pos, unq(t.data[0]), age(t.data[1]), t.data[2] as f32 / 10.0));
                }
                T_BARREL => barrels.push(Barrel { pos, hp: t.data[0] as f32, flash: 0.0, by: ME }),
                T_DECOY => decoys.push(Decoy::shown(pos, age(t.data[0]), t.data[1] as f32 / 10.0)),
                T_TELL if (t.sub as usize) < roster().enemies.len() => {
                    let was = c.tells.iter().find(|(p, _)| *p == pos).map_or(0.0, |w| w.1);
                    tell_t.push((pos, was + dt));
                    tells.push(Tell { kind: Kind(t.sub), pos, hp: 0.0, t: was + dt, leader: t.data[0] != 0, led: None });
                }
                _ => {}
            }
        }
        c.tells = tell_t;
        (self.drops, self.thrown, self.patches, self.decoys, self.tells) = (drops, thrown, patches, decoys, tells);
        self.barrels = barrels;
    }

    fn lay_shots(&mut self, c: &Client, mirror: &Mirror) {
        let own: Vec<usize> = c.order.iter().take(self.views).copied().collect();
        self.bullets.clear();
        self.spits.clear();
        for (pos, age, s) in mirror.shots_for(&own) {
            match s.what {
                Spawn::Shot { owner, look, vel, life, .. } => {
                    let by = (look as usize).checked_sub(1).filter(|&d| d < book().defs.len());
                    let look = by.and_then(|d| book().defs[d].bullet.as_deref()).and_then(|n| effects::effects().named(n));
                    let vel = vel.map(|v| v as f32 / 8.0);
                    let owner = c.index(owner as usize).unwrap_or(ME);
                    self.bullets.push(Bullet { pos, vel, damage: 0.0, pierce: 0, life: (life as f32 / 50.0 - age).max(0.0), force: 0.0, hit: Vec::new(), look, age, by, owner, lift: [0.0; 2] });
                }
                Spawn::Spit { kind, vel, life, .. } if (kind as usize) < roster().enemies.len() => {
                    self.spits.push(Spit { kind: Kind(kind), pos, vel: vel.map(|v| v as f32 / 8.0), damage: 0.0, life: (life as f32 / 50.0 - age).max(0.0), age });
                }
                _ => {}
            }
        }
    }

    fn lay_run(&mut self, c: &mut Client, mirror: &Mirror, dt: f32) {
        let run = &mirror.run;
        self.t = run.t as f32 / 10.0;
        self.level = run.level as u32;
        (self.xp, self.next_xp) = (run.xp, run.next_xp.max(1));
        if run.kills != self.kills {
            self.kill_bump = 0.0;
        }
        self.kills = run.kills;
        self.chest = run.chest;
        self.woke = self.t >= dawn();
        self.menu = run.menu.checked_sub(1).map(usize::from);
        self.paused_by = c.index(run.paused_by as usize).unwrap_or(ME);
        self.leveling = if run.phase == PHASE_LEVELING { 0.1 } else { 0.0 };
        c.cards.resize(self.views, (0.0, [(0, 0); 3]));
        let mut mine = None;
        for v in 0..self.views.min(c.order.len()) {
            let seat = c.order[v];
            let Some(sc) = run.cards.iter().find(|k| k.seat as usize == seat).filter(|_| run.phase == PHASE_CARDS) else {
                c.cards[v] = (0.0, [(0, 0); 3]);
                if v > 0 {
                    self.locals[v].cards = None;
                }
                continue;
            };
            let (t, shown) = &mut c.cards[v];
            if *shown != sc.choices {
                *shown = sc.choices;
            }
            *t += dt;
            let choices = sc.choices.map(|(up, bits)| Choice { up: (up as usize).min(book().defs.len() - 1), rare: bits & CARD_RARE != 0, found: bits & CARD_FOUND != 0 });
            let cards = Cards { choices, selected: sc.selected as usize % 3, t: *t, picked: sc.picked.then_some(PICK_TIME) };
            if v == ME {
                mine = Some(cards);
                (self.banishes, self.rerolls, self.favors) = (sc.banishes as u32, sc.rerolls as u32, sc.favors as u32);
            } else {
                self.locals[v].cards = Some(cards);
                self.locals[v].set_counts(sc.banishes as u32, sc.rerolls as u32, sc.favors as u32);
            }
        }
        self.phase = match run.phase {
            PHASE_WON | PHASE_LOST => {
                c.over += dt;
                Phase::Over { t: c.over, won: run.phase == PHASE_WON }
            }
            _ if run.phase == PHASE_CARDS => {
                c.over = 0.0;
                let cards = mine.unwrap_or(Cards { choices: [Choice { up: 0, rare: false, found: false }; 3], selected: 0, t: 0.0, picked: Some(PICK_TIME) });
                Phase::LevelUp { choices: cards.choices, selected: cards.selected, t: cards.t, picked: cards.picked }
            }
            _ => {
                c.over = 0.0;
                Phase::Playing
            }
        };
    }
}
