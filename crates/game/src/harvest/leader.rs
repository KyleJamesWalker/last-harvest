//! A turned player in a run others still play leads the dead: they steer a leader zombie and the swarm that rose
//! with it, and once it falls, pick where the next one rises.

use crate::fmath as fm;
use std::f32::consts::{PI, TAU};

use super::map::TILE;
use super::player::{ME, PlayerId};
use super::{
    Facing, GOLD, Harvest, Kind, Phase, RED, SPIN_TIME, Tell, WHITE, Zombie, clock, len, map_scale,
    norm, roster, sub,
};
use crate::input::{Buttons, Input};
use crate::sfx::Sfx;
use crate::{Align, Canvas, SCREEN_W};

/// A leader's zombie id, the way its player steers it, and where it stands.
pub(super) type Leader = (u32, [f32; 2], [f32; 2]);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::harvest) enum Lead {
    /// Steering the zombie with this id.
    Leading(u32),
    /// Picking where the next leader rises: the cursor, in map pixels, and seconds left.
    Picking { at: [f32; 2], left: f32 },
}

impl Lead {
    /// How fast the pad moves a turned player, as a client predicts it: their leader's walk, or the pick's cross.
    pub(super) fn speed(self) -> f32 {
        let t = crate::tuning::tuning();
        match self {
            Lead::Leading(_) => t.leader_speed,
            Lead::Picking { .. } => t.aim_speed,
        }
    }
}

impl Harvest {
    /// Whether a player turning now leads the dead: only while someone else still plays the run.
    pub(super) fn may_lead(&self) -> bool {
        self.players.len() > 1
            && self.range.is_none()
            && matches!(self.phase, Phase::Playing)
            && self.players.iter().any(|p| p.alive())
    }

    /// Player `p` stops leading: their swarm joins the ordinary dead.
    pub(super) fn drop_lead(&mut self, p: PlayerId) {
        for z in self
            .zombies
            .iter_mut()
            .chain(self.burrowed.iter_mut())
            .filter(|z| z.led == Some(p))
        {
            z.led = None;
        }
        for t in self.tells.iter_mut().filter(|t| t.led == Some(p)) {
            t.led = None;
        }
        if let Some(pl) = self.players.get_mut(p) {
            pl.lead = None;
        }
    }

    /// Whether player `p`'s view is of the dead they lead or the map they pick a rise on.
    pub fn leads(&self, p: PlayerId) -> bool {
        self.players.get(p).is_some_and(|pl| pl.lead.is_some())
    }

    /// Player `p` takes a leader of the dead at `at`, `rise` out of the ground, and a swarm climbs out around it.
    pub(super) fn rise_leading(&mut self, p: PlayerId, at: [f32; 2], rise: f32) {
        let t = crate::tuning::tuning();
        let grow = (1.0 + self.t / 60.0 * t.growth) * self.fury * self.toughness();
        let kind = Kind(roster().leader as u8);
        let full = kind.stats().hp * grow * t.leader_hp;
        let mut z = Zombie::new(kind, at, full, rise, 0.0);
        let yaw = self.players[p].view_yaw;
        (z.id, z.leader, z.led, z.barren, z.dir) = (
            self.next_id,
            true,
            Some(p),
            true,
            [fm::cos(yaw), fm::sin(yaw)],
        );
        z.facing = Facing::of(z.dir);
        self.next_id += 1;
        let id = z.id;
        self.zombies.push(z);
        if rise < 1.0 {
            self.fx.push(super::Fx::play(
                super::effects::effects().zombie_rises,
                [at[0], at[1] - 3.0],
                0.0,
            ));
        }
        let pl = &mut self.players[p];
        (pl.lead, pl.pos, pl.steer, pl.lead_full, pl.lead_hp) =
            (Some(Lead::Leading(id)), at, [0.0; 2], full, 1.0);
        let common = Kind::common();
        let n = t.leader_swarm as usize;
        for k in 0..n {
            let a = k as f32 / n as f32 * TAU;
            let spot = [14.0, 24.0, 34.0]
                .into_iter()
                .map(|r| [at[0] + fm::cos(a) * r, at[1] + fm::sin(a) * r])
                .find(|s| !self.map.hits_cells(s[0], s[1], t.zombie_half()));
            if let Some(pos) = spot {
                self.tells.push(Tell {
                    kind: common,
                    pos,
                    hp: common.stats().hp * grow,
                    t: 0.0,
                    leader: false,
                    led: Some(p),
                });
            }
        }
        self.juice.banner(
            format!("PLAYER {} LEADS THE DEAD", self.number(p)),
            RED,
            2.4,
        );
    }

    /// Each turned player steers their leader, or moves the cursor where the next one rises; ME by `input`.
    pub(super) fn steer_leads(&mut self, input: &Input, dt: f32, sounds: &mut Vec<Sfx>) {
        for p in 0..self.players.len() {
            let Some(lead) = self.players[p].lead else {
                continue;
            };
            let own = if p == ME {
                *input
            } else {
                self.inputs.get(p).copied().unwrap_or_default()
            };
            match lead {
                Lead::Leading(id) => self.steer_leader(p, id, &own, dt, sounds),
                Lead::Picking { at, left } => self.pick_rise(p, at, left, &own, dt, sounds),
            }
        }
    }

    /// Turns and moves as walking does, at `[leader] speed`; X calls the dead near the leader into the swarm.
    fn steer_leader(&mut self, p: PlayerId, id: u32, own: &Input, dt: f32, sounds: &mut Vec<Sfx>) {
        let Some(at) = self.zombies.iter().find(|z| z.id == id).map(|z| z.pos) else {
            return self.leader_fell(p, sounds);
        };
        self.juice.tally.book_mut(p).led += dt;
        if own.pressed.contains(Buttons::B) {
            self.turn_around(p);
        }
        let t = crate::tuning::tuning();
        let turn = (own.held.contains(Buttons::A) as i32 as f32
            - own.held.contains(Buttons::Y) as i32 as f32
            + own.look)
            .clamp(-1.0, 1.0);
        let pl = &mut self.players[p];
        let spun = pl.spin.min(PI / SPIN_TIME * dt);
        pl.spin -= spun;
        pl.view_yaw = (pl.view_yaw + turn * t.turn_speed * dt + spun + own.yaw).rem_euclid(TAU);
        let f = [fm::cos(pl.view_yaw), fm::sin(pl.view_yaw)];
        let [side, ahead] = [own.dir()[0], -own.dir()[1]];
        pl.steer = [f[0] * ahead - f[1] * side, f[1] * ahead + f[0] * side];
        pl.aim = f;
        if own.pressed.contains(Buttons::X) {
            self.call(p, at, sounds);
        }
    }

    /// The dead within `[leader] call` of the leader join player `p`'s swarm.
    fn call(&mut self, p: PlayerId, at: [f32; 2], sounds: &mut Vec<Sfx>) {
        let reach = crate::tuning::tuning().leader_call;
        for z in self
            .zombies
            .iter_mut()
            .filter(|z| z.led.is_none() && !z.boss && len(sub(z.pos, at)) <= reach)
        {
            z.led = Some(p);
        }
        let (pan, gain) = self.place(at);
        sounds.push(Sfx::Groan {
            pan,
            gain: gain.max(80),
        });
        self.juice.burst(
            at,
            0.0,
            12,
            [0.0; 2],
            0.0,
            [30.0, 70.0],
            super::juice::dirt(),
            1.2,
            0.5,
            0.0,
        );
    }

    /// The leader is down: its swarm scatters among the dead, and player `p` picks where the next one rises.
    fn leader_fell(&mut self, p: PlayerId, sounds: &mut Vec<Sfx>) {
        self.drop_lead(p);
        let at = self.rise_start(self.players[p].pos);
        let pl = &mut self.players[p];
        (pl.lead, pl.pos, pl.steer) = (
            Some(Lead::Picking {
                at,
                left: crate::tuning::tuning().leader_pick,
            }),
            at,
            [0.0; 2],
        );
        if p == ME {
            sounds.push(Sfx::Turn);
        }
        self.feel_for(p, |j| j.shake(0.4));
    }

    /// Where the cursor starts: `from`, pushed out past the minimum distance from the nearest living player.
    fn rise_start(&self, from: [f32; 2]) -> [f32; 2] {
        let q = self.nearest_player(from);
        let away = sub(from, self.players[q].pos);
        let least = crate::tuning::tuning().leader_min_tiles * TILE;
        let at = if len(away) >= least + TILE {
            from
        } else {
            let d = if len(away) > 1.0 {
                norm(away)
            } else {
                [0.0, 1.0]
            };
            [
                self.players[q].pos[0] + d[0] * (least + TILE),
                self.players[q].pos[1] + d[1] * (least + TILE),
            ]
        };
        self.on_map(at)
    }

    fn on_map(&self, at: [f32; 2]) -> [f32; 2] {
        [
            at[0].clamp(TILE / 2.0, self.map.width_px() - TILE / 2.0),
            at[1].clamp(TILE / 2.0, self.map.height_px() - TILE / 2.0),
        ]
    }

    /// Whether a leader may rise at `at`: open ground, at least `[leader] respawn_min_tiles` from every living player.
    pub(super) fn rise_ok(&self, at: [f32; 2]) -> bool {
        let t = crate::tuning::tuning();
        let least = t.leader_min_tiles * TILE;
        self.on_map(at) == at
            && !self.map.hits_cells(at[0], at[1], t.zombie_half())
            && self
                .players
                .iter()
                .filter(|pl| pl.alive())
                .all(|pl| len(sub(pl.pos, at)) >= least)
    }

    /// The pad moves the cursor, A or X (or a tap on the map) rises there if it may; when time runs out the leader
    /// rises where a horde's leader would.
    fn pick_rise(
        &mut self,
        p: PlayerId,
        at: [f32; 2],
        left: f32,
        own: &Input,
        dt: f32,
        sounds: &mut Vec<Sfx>,
    ) {
        let t = crate::tuning::tuning();
        let d = own.dir();
        let mut at = self.on_map([
            at[0] + d[0] * t.aim_speed * dt,
            at[1] + d[1] * t.aim_speed * dt,
        ]);
        let tap = own.touch.filter(|_| own.touch_started && p == ME);
        if let Some(touch) = tap {
            let cam = self.camera();
            at = self.on_map([
                cam[0] + touch[0] / map_scale(),
                cam[1] + touch[1] / map_scale(),
            ]);
        }
        let left = left - dt;
        if own.pressed.any(Buttons::A | Buttons::X) || tap.is_some() {
            if self.rise_ok(at) {
                return self.rise_leading(p, at, 0.0);
            }
            if p == ME {
                sounds.push(Sfx::Miss);
            }
        }
        if left <= 0.0 {
            let q = self.ring_player();
            if let Some(spot) = self.ring_spot(q, None) {
                return self.rise_leading(p, spot, 0.0);
            }
        }
        let pl = &mut self.players[p];
        (pl.lead, pl.pos) = (
            Some(Lead::Picking {
                at,
                left: left.max(0.0),
            }),
            at,
        );
    }

    /// Turned players stand where their leader is.
    pub(super) fn follow_leads(&mut self) {
        for pl in &mut self.players {
            if let Some(Lead::Leading(id)) = pl.lead
                && let Some(z) = self.zombies.iter().find(|z| z.id == id)
            {
                (pl.pos, pl.facing, pl.lead_hp) = (
                    z.pos,
                    z.facing,
                    (z.hp / pl.lead_full.max(1.0)).clamp(0.0, 1.0),
                );
            }
        }
    }

    /// A client draws its own leaders where it predicts them, as it does its own players.
    pub(super) fn pin_leaders(&mut self) {
        for v in 0..self.views.min(self.players.len()) {
            if let Some(Lead::Leading(id)) = self.players[v].lead
                && let Some(z) = self.zombies.iter_mut().find(|z| z.id == id)
            {
                z.pos = self.players[v].pos;
            }
        }
    }

    /// For each player, their leader's id, how they steer it and where it stands, while they lead one.
    pub(super) fn leaders(&self) -> Vec<Option<Leader>> {
        self.players
            .iter()
            .map(|pl| match pl.lead {
                Some(Lead::Leading(id)) => self
                    .zombies
                    .iter()
                    .find(|z| z.id == id)
                    .map(|z| (id, pl.steer, z.pos)),
                _ => None,
            })
            .collect()
    }

    /// A scripted player's input while turned: steer the leader at the nearest living player, and rise at once where
    /// the cursor starts if it may, else wait out the pick.
    pub(super) fn lead_bot(&self, p: PlayerId) -> Option<Input> {
        match self.players[p].lead? {
            Lead::Picking { at, .. } => Some(if self.rise_ok(at) {
                Input {
                    pressed: Buttons::X,
                    held: Buttons::X,
                    ..Input::default()
                }
            } else {
                Input::default()
            }),
            Lead::Leading(_) => {
                let pl = &self.players[p];
                let d = sub(self.players[self.nearest_player(pl.pos)].pos, pl.pos);
                let off = (fm::atan2(d[1], d[0]) - pl.view_yaw + PI).rem_euclid(TAU) - PI;
                let held = if off.abs() < 1.2 {
                    Buttons::UP
                } else {
                    Buttons::NONE
                };
                Some(Input {
                    held,
                    look: (off * 2.0).clamp(-1.0, 1.0),
                    ..Input::default()
                })
            }
        }
    }

    /// Over the view ahead, while ME leads: who they are now, the leader's health and how to call.
    pub(super) fn lead_hud(&self, c: &mut Canvas) {
        let Some(Lead::Leading(_)) = self
            .eye()
            .lead
            .filter(|_| !matches!(self.phase, Phase::Over { .. }))
        else {
            return;
        };
        c.anchor([0.5, 0.0]);
        c.rect(0.0, 184.0, SCREEN_W, 96.0, [0.0, 0.0, 0.0, 0.5]);
        c.label(
            "YOU LEAD THE DEAD",
            SCREEN_W / 2.0,
            190.0,
            36.0,
            RED,
            Align::Center,
        );
        c.hint(
            "Hunt the living.   {X}: call the dead near you",
            SCREEN_W / 2.0,
            234.0,
            26.0,
            WHITE,
            Align::Center,
        );
        let (w, x) = (300.0, SCREEN_W / 2.0 - 150.0);
        c.rect(x, 270.0, w, 8.0, [0.0, 0.0, 0.0, 0.7]);
        c.rect(x, 270.0, w * self.eye().lead_hp.clamp(0.0, 1.0), 8.0, RED);
        c.anchor([0.5, 0.5]);
    }

    /// On the map while ME picks a rise: a ring around each living player inside which it can't, the cursor, and the
    /// time left.
    pub(super) fn rise_mark(&self, c: &mut Canvas, cam: [f32; 2]) {
        let Some(Lead::Picking { at, left }) = self.eye().lead else {
            return;
        };
        let t = crate::tuning::tuning();
        let r = t.leader_min_tiles * TILE * map_scale();
        for pl in self.players.iter().filter(|pl| pl.alive()) {
            let (x, y) = (
                (pl.pos[0] - cam[0]) * map_scale(),
                (pl.pos[1] - cam[1]) * map_scale(),
            );
            for k in 0..48 {
                let a = k as f32 / 48.0 * TAU;
                c.circle(
                    x + fm::cos(a) * r,
                    y + fm::sin(a) * r,
                    4.0,
                    [RED[0], RED[1], RED[2], 0.8],
                );
            }
        }
        let ok = self.rise_ok(at);
        let mark = if ok { GOLD } else { RED };
        let (x, y) = (
            (at[0] - cam[0]) * map_scale(),
            (at[1] - cam[1]) * map_scale(),
        );
        c.rect(x - 18.0, y - 3.0, 36.0, 6.0, mark);
        c.rect(x - 3.0, y - 18.0, 6.0, 36.0, mark);
        c.anchor([-1.0, 0.0]);
        c.rect(0.0, 0.0, SCREEN_W, 10.0, [0.0, 0.0, 0.0, 0.6]);
        c.rect(
            0.0,
            0.0,
            SCREEN_W * (left / t.leader_pick.max(0.01)).clamp(0.0, 1.0),
            10.0,
            RED,
        );
        c.anchor([0.5, 0.0]);
        c.rect(0.0, 20.0, SCREEN_W, 100.0, [0.0, 0.0, 0.0, 0.55]);
        c.label(
            format!("YOUR LEADER FELL  -  RISE AGAIN  {}", clock(left.ceil())),
            SCREEN_W / 2.0,
            28.0,
            36.0,
            RED,
            Align::Center,
        );
        let how = format!(
            "Move the cross, {{A}}: rise there, {} m or more from the living",
            t.leader_min_tiles
        );
        c.hint(
            &how,
            SCREEN_W / 2.0,
            74.0,
            26.0,
            if ok { WHITE } else { [1.0, 1.0, 1.0, 0.6] },
            Align::Center,
        );
        c.anchor([0.5, 0.5]);
    }
}

#[cfg(test)]
mod tests {
    use super::super::map::{MapSource, TILE};
    use super::super::*;
    use super::Lead;

    fn pair() -> Harvest {
        let mut h = Harvest::calm_on(3, &MapSource::new("Open", [60, 60]))
            .unwrap()
            .with_players(2);
        h.birds.clear();
        h.players[0].pos = [30.5 * TILE - 150.0, 30.5 * TILE];
        h.players[1].pos = [30.5 * TILE + 150.0, 30.5 * TILE];
        for p in &mut h.players {
            p.kit.weapons.clear();
        }
        h
    }

    fn step(h: &mut Harvest, input: Input, secs: f32) {
        let mut sounds = Vec::new();
        for _ in 0..(secs * 60.0).round() as u32 {
            h.update(&input, 1.0 / 60.0, &mut sounds);
        }
    }

    fn turned(h: &mut Harvest, p: usize) {
        h.players[p].hp = 0.0;
        step(h, Input::default(), TURNED + 0.1);
    }

    fn leader(h: &Harvest, p: usize) -> &Zombie {
        let Some(Lead::Leading(id)) = h.players[p].lead else {
            panic!("player {p} leads: {:?}", h.players[p].lead)
        };
        h.zombies
            .iter()
            .find(|z| z.id == id)
            .expect("the leader stands")
    }

    #[test]
    fn a_player_who_turns_while_another_stands_takes_a_leader_and_a_swarm_rises_with_them() {
        let mut h = pair();
        let fell = h.players[ME].pos;
        turned(&mut h, ME);
        let z = leader(&h, ME);
        assert!(
            z.leader && z.barren && z.led == Some(ME) && len(sub(z.pos, fell)) < 1.0,
            "the leader rises at the body"
        );
        step(&mut h, Input::default(), 1.0);
        let swarm = h
            .zombies
            .iter()
            .filter(|z| z.led == Some(ME) && !z.leader)
            .count();
        assert_eq!(swarm, crate::tuning::tuning().leader_swarm as usize);
        assert!(matches!(h.phase, Phase::Playing));
    }

    #[test]
    fn a_lone_player_who_turns_ends_the_run_as_before() {
        let mut h = Harvest::calm(3);
        h.me_mut().hp = 0.0;
        step(&mut h, Input::default(), TURNED + 1.0);
        assert!(matches!(h.phase, Phase::Over { won: false, .. }) && h.me().lead.is_none());
    }

    #[test]
    fn the_pad_steers_the_leader_and_x_calls_the_dead_near_it() {
        let mut h = pair();
        turned(&mut h, ME);
        let at = leader(&h, ME).pos;
        let mut z = Zombie::new(Kind::common(), [at[0], at[1] + 30.0], 1000.0, 1.0, 0.0);
        z.id = 9999;
        h.zombies.push(z);
        step(
            &mut h,
            Input {
                held: Buttons::UP,
                ..Input::default()
            },
            2.0,
        );
        let yaw = h.me().view_yaw;
        let moved = sub(leader(&h, ME).pos, at);
        assert!(
            (moved[0] * fm::cos(yaw) + moved[1] * fm::sin(yaw)) > 40.0,
            "walks ahead through its own swarm: {moved:?}"
        );
        assert_eq!(
            h.me().pos,
            leader(&h, ME).pos,
            "the turned player is where their leader is"
        );
        step(
            &mut h,
            Input {
                held: Buttons::X,
                pressed: Buttons::X,
                ..Input::default()
            },
            1.0 / 60.0,
        );
        assert_eq!(
            h.zombies.iter().find(|z| z.id == 9999).unwrap().led,
            Some(ME),
            "called into the swarm"
        );
    }

    #[test]
    fn a_fallen_leader_gives_nothing_and_its_player_picks_a_rise_or_rises_where_a_leader_would_once_time_is_up()
     {
        let mut h = pair();
        turned(&mut h, ME);
        let (kills, drops) = (h.kills, h.drops.len());
        for z in h.zombies.iter_mut().filter(|z| z.led == Some(ME)) {
            z.hp = 0.0;
        }
        step(&mut h, Input::default(), 0.1);
        assert_eq!(
            (h.kills, h.drops.len()),
            (kills, drops),
            "no kills, hearts or chest"
        );
        let Some(Lead::Picking { left, .. }) = h.me().lead else {
            panic!("picking: {:?}", h.me().lead)
        };
        let pick = crate::tuning::tuning().leader_pick;
        assert!(left > pick - 0.2 && left <= pick);
        step(&mut h, Input::default(), pick + 0.1);
        let z = leader(&h, ME);
        let ring = spawn_ring(h.t);
        let from = len(sub(z.pos, h.players[1].pos));
        assert!(
            from >= ring[0] - 1.0 && from <= ring[1] + 1.0,
            "risen on the spawn ring, {from} px off"
        );
    }

    #[test]
    fn a_pick_rises_only_at_least_the_minimum_from_every_living_player() {
        let mut h = pair();
        turned(&mut h, ME);
        for z in h.zombies.iter_mut().filter(|z| z.led == Some(ME)) {
            z.hp = 0.0;
        }
        step(&mut h, Input::default(), 0.1);
        let least = crate::tuning::tuning().leader_min_tiles * TILE;
        let living = h.players[1].pos;
        let Some(Lead::Picking { at, .. }) = h.me().lead else {
            panic!()
        };
        assert!(
            len(sub(at, living)) >= least,
            "the cursor starts where a rise may be"
        );
        let near = [living[0] + least - 8.0, living[1]];
        h.me_mut().lead = Some(Lead::Picking {
            at: near,
            left: 10.0,
        });
        step(
            &mut h,
            Input {
                held: Buttons::A,
                pressed: Buttons::A,
                ..Input::default()
            },
            1.0 / 60.0,
        );
        assert!(
            matches!(h.me().lead, Some(Lead::Picking { .. })),
            "too near the living"
        );
        let far = [living[0] + least + 8.0, living[1]];
        h.me_mut().lead = Some(Lead::Picking {
            at: far,
            left: 10.0,
        });
        step(
            &mut h,
            Input {
                held: Buttons::A,
                pressed: Buttons::A,
                ..Input::default()
            },
            1.0 / 60.0,
        );
        assert!(
            len(sub(leader(&h, ME).pos, far)) < 1.0,
            "rises at the cross"
        );
    }

    #[test]
    fn the_swarms_bites_are_counted_for_its_leader_and_the_run_ends_once_every_player_has_turned() {
        let mut h = pair();
        turned(&mut h, ME);
        let prey = h.players[1].pos;
        let mut z = Zombie::new(Kind::common(), prey, 1000.0, 1.0, 0.0);
        (z.id, z.led) = (9999, Some(ME));
        h.zombies.push(z);
        step(&mut h, Input::default(), 0.1);
        assert!(h.juice.tally.book(ME).bites >= 1 && h.juice.tally.book(ME).led > 0.0);
        h.players[1].hp = 0.0;
        step(&mut h, Input::default(), 1.0 / 60.0);
        assert!(
            matches!(h.phase, Phase::Over { won: false, .. }),
            "the last to fall ends it"
        );
        assert!(h.players[1].lead.is_none(), "nobody is left to hunt");
    }

    #[test]
    fn a_scripted_player_leads_at_the_nearest_living_player() {
        let mut h = pair().with_bots(0);
        h.bots = vec![(
            1,
            super::super::sandbox::Bot::new(super::super::sandbox::Walk::Avoid, 0.25, 0.0, 0.0),
        )];
        h.players[1].pos = [30.5 * TILE + 200.0, 30.5 * TILE];
        h.players[1].hp = 0.0;
        step(&mut h, Input::default(), TURNED + 0.1);
        let start = len(sub(leader(&h, 1).pos, h.players[ME].pos));
        step(&mut h, Input::default(), 4.0);
        let now = len(sub(leader(&h, 1).pos, h.players[ME].pos));
        assert!(now < start - 40.0, "{start} -> {now}");
    }

    #[test]
    fn the_view_names_the_dead_while_leading_and_the_map_holds_the_cross_while_picking() {
        let says = |c: &crate::Canvas, s: &str| {
            c.shapes
                .iter()
                .any(|x| matches!(x, crate::Shape::Text { text, .. } if text.contains(s)))
        };
        let mut h = pair();
        turned(&mut h, ME);
        let mut f = crate::Frame::default();
        h.draw(&mut f, false);
        assert!(says(&f.top, "YOU LEAD THE DEAD"));
        h.me_mut().lead = Some(Lead::Picking {
            at: h.me().pos,
            left: 9.0,
        });
        let mut f = crate::Frame::default();
        h.draw(&mut f, false);
        assert!(says(&f.bottom, "RISE AGAIN") && !says(&f.top, "YOU LEAD THE DEAD"));
        let h = h.single_screen(true);
        let mut f = crate::Frame::default();
        h.draw(&mut f, false);
        assert!(
            says(&f.top, "RISE AGAIN"),
            "one screen shows the map while picking"
        );
        let mut h = pair();
        turned(&mut h, ME);
        h.xp = h.next_xp;
        step(&mut h, Input::default(), 1.0);
        assert!(matches!(h.phase, Phase::LevelUp { .. }));
        let mut f = crate::Frame::default();
        h.draw(&mut f, false);
        assert!(
            !says(&f.top, "LEVEL UP") && !says(&f.bottom, "Offer your hearts"),
            "the dead get no cards"
        );
    }

    #[test]
    fn relabeling_swaps_the_swarm_with_its_player() {
        let mut h = pair();
        turned(&mut h, 1);
        h.relabel(1);
        assert!(
            h.me().lead.is_some()
                && h.zombies
                    .iter()
                    .filter(|z| z.led.is_some())
                    .all(|z| z.led == Some(ME))
        );
        h.relabel(1);
        assert!(
            h.zombies
                .iter()
                .filter(|z| z.led.is_some())
                .all(|z| z.led == Some(1))
        );
    }
}
