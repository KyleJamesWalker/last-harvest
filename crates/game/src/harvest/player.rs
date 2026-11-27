//! Each player's own state in a run: where they stand, their health and what they carry.

use crate::fmath as fm;
use std::f32::consts::PI;

use super::players::{self, PlayerLook};
use super::sandbox::{Bot, Walk};
use super::weapons::{Dog, Drone, Kit, Swing};
use super::{Facing, Harvest, sight};
use crate::Input;
use crate::sfx::Sfx;

/// A player's place in `Harvest::players`, which owns what they fire, throw and call up.
pub(super) type PlayerId = usize;

/// The local player: the one the screens follow.
pub(super) const ME: PlayerId = 0;

/// The most players a run takes.
pub const MAX_PLAYERS: usize = 4;

pub(super) struct Player {
    pub pos: [f32; 2],
    pub hp: f32,
    pub hurt: f32,
    pub facing: (Facing, bool),
    pub aim: [f32; 2],
    pub walk: f32,
    /// The way the player last walked.
    pub heading: [f32; 2],
    /// Turned: seconds until the player picks a new way to wander.
    pub wander: f32,
    /// The player's heading, which the 3D camera looks along, in radians from +x toward +y.
    pub view_yaw: f32,
    /// Radians left of a quick turn in progress.
    pub spin: f32,
    /// The heading the player keeps while B swings the camera round, so it sweeps past their front.
    pub turned_from: [f32; 2],
    /// The id of the player's look; the farmer's is empty.
    pub look: String,
    /// Zombies as the map remembers them.
    pub marks: Vec<sight::Mark>,
    pub kit: Kit,
    /// Grenades ready to throw; the fraction is the next one recharging.
    pub grenades: f32,
    /// Seconds left of the worn grenade's flash after one comes ready.
    pub ready_flash: f32,
    /// Revives the kit has already spent this run.
    pub revived: u32,
    /// The zombies each weapon's burst has shot so far, by its section, for `targets each`.
    pub burst_shot: Vec<(usize, u32)>,
    /// Rounds each weapon has fired toward its next sound, by its def, for a `sound_every`.
    pub heard: Vec<u32>,
    /// Zombies a crow pecked lately, by id, and the seconds before it can peck them again.
    pub pecked: Vec<(u32, f32)>,
    /// Seconds left of the lantern beam's flare.
    pub beam: f32,
    pub swings: Vec<Swing>,
    pub dogs: Vec<Dog>,
    pub drones: Vec<Drone>,
    pub down: Option<f32>,
    /// Turned in a run others still play: steering a leader of the dead, or picking where the next one rises.
    pub lead: Option<super::leader::Lead>,
    /// Leading: the way the pad steers the leader, as long as the push.
    pub steer: [f32; 2],
    /// Leading: the leader's health as it rose, and the share of it left.
    pub lead_full: f32,
    pub lead_hp: f32,
    /// Their device left the online night, so they never lead.
    pub left: bool,
    /// The spot they've stayed within `[camp] radius` of, for how many seconds, and the waves that rose for it.
    pub camp_at: [f32; 2],
    pub camp: f32,
    pub camp_waves: u32,
}

impl Player {
    pub fn new(pos: [f32; 2], kit: Kit, grenades: f32) -> Self {
        Self {
            pos,
            hp: kit.max_hp(),
            hurt: 0.0,
            facing: (Facing::Front, false),
            aim: [0.0, 1.0],
            walk: 0.0,
            heading: [0.0, 1.0],
            wander: 0.0,
            view_yaw: -PI / 2.0,
            spin: 0.0,
            turned_from: [0.0; 2],
            look: String::new(),
            marks: Vec::new(),
            kit,
            grenades,
            ready_flash: 0.0,
            revived: 0,
            burst_shot: Vec::new(),
            heard: Vec::new(),
            pecked: Vec::new(),
            beam: 0.0,
            swings: Vec::new(),
            dogs: Vec::new(),
            drones: Vec::new(),
            down: None,
            lead: None,
            steer: [0.0; 2],
            lead_full: 0.0,
            lead_hp: 0.0,
            left: false,
            camp_at: pos,
            camp: 0.0,
            camp_waves: 0,
        }
    }

    pub fn alive(&self) -> bool {
        self.down.is_none()
    }

    /// Left the night and sunk out of it: nothing draws or names them.
    pub fn sunk_away(&self) -> bool {
        self.left && self.down.is_some_and(|t| t >= super::TURN_SINK)
    }

    /// Picking where their next leader rises, so where they stand is a cursor nobody else sees.
    pub fn picking(&self) -> bool {
        matches!(self.lead, Some(super::leader::Lead::Picking { .. }))
    }
}

impl Harvest {
    pub(super) fn me(&self) -> &Player {
        &self.players[ME]
    }

    /// The player the view being drawn belongs to; ME outside `draw_view`.
    pub(super) fn eye(&self) -> &Player {
        &self.players[self.viewer]
    }

    pub(super) fn me_mut(&mut self) -> &mut Player {
        &mut self.players[ME]
    }

    /// The player whose weapons, throws and blasts are being played out.
    pub(super) fn actor(&self) -> &Player {
        &self.players[self.actor]
    }

    pub(super) fn actor_mut(&mut self) -> &mut Player {
        &mut self.players[self.actor]
    }

    /// Starts the run with `n` players, 1 to `MAX_PLAYERS`: the local one, then each beside the spawn, in another
    /// look where there is one, with their own kit. Comes after the local player's look and kit are set.
    pub fn with_players(mut self, n: usize) -> Self {
        let n = n.clamp(1, MAX_PLAYERS);
        while self.players.len() < n {
            let p = self.players.len();
            let mine = self.me().look.clone();
            let others: Vec<&'static PlayerLook> =
                players::looks().iter().filter(|l| l.id != mine).collect();
            let look = if others.is_empty() {
                players::look(&mine)
            } else {
                others[(p - 1) % others.len()]
            };
            let pos = self.beside_spawn(p);
            let player = self.new_player(pos, look);
            self.players.push(player);
        }
        self.players.truncate(n);
        self.inputs.resize(n, Input::default());
        self.juice.tally = super::tally::Tally::sized(super::book().defs.len(), n);
        self
    }

    /// Dresses players 2 onward in these cards, in order, each starting with its card's kit.
    pub fn seated_as(mut self, cards: &[String]) -> Self {
        for (p, card) in cards.iter().enumerate().map(|(i, c)| (i + 1, c)) {
            if p < self.players.len() {
                let pos = self.players[p].pos;
                self.players[p] = self.new_player(pos, players::look(card));
            }
        }
        self
    }

    /// Player `p`'s card, health, speed, magnet and weapons as the night starts them, one line, for tests.
    #[doc(hidden)]
    pub fn seat_summary(&self, p: PlayerId) -> String {
        let Some(pl) = self.players.get(p) else {
            return String::new();
        };
        let k = &pl.kit;
        let weapons: Vec<&str> = k
            .weapons
            .iter()
            .map(|w| super::book().defs[w.def].id.as_str())
            .collect();
        format!(
            "{} hp={}/{} speed={} magnet={} weapons={}",
            pl.look,
            pl.hp,
            k.max_hp(),
            k.speed(),
            k.magnet(),
            weapons.join(",")
        )
    }

    /// Adds `n` scripted players after the players already in the run, each walking and steering by what its own map
    /// shows, for trying out several players.
    pub fn with_bots(self, n: usize) -> Self {
        let first = self.players.len();
        let mut h = self.with_players(first + n);
        h.bots = (first..h.players.len())
            .map(|p| (p, Bot::new(Walk::Avoid, 0.25, 0.0, 12.0)))
            .collect();
        h
    }

    /// What moves player `p` from the next update on; the local player moves by the input `update` is given.
    pub fn steer(&mut self, p: PlayerId, input: Input) {
        if let Some(i) = self.inputs.get_mut(p) {
            *i = input;
        }
    }

    /// The scripted players choose their input for this step.
    pub(super) fn drive_bots(&mut self, sounds: &mut Vec<Sfx>) {
        let mut bots = std::mem::take(&mut self.bots);
        for (p, bot) in &mut bots {
            if self.players[*p].alive() || self.players[*p].lead.is_some() {
                let input = bot.step(self, *p, sounds);
                self.steer(*p, input);
            }
        }
        self.bots = bots;
    }

    /// An open spot a step from the spawn for player `p`, each player on their own side.
    pub(super) fn beside_spawn(&self, p: PlayerId) -> [f32; 2] {
        let (spawn, half) = (self.map.spawn, crate::tuning::tuning().player_half());
        for r in [14.0, 22.0, 30.0, 40.0] {
            for k in 0..8 {
                let a = (p as f32 + k as f32 * 0.5) * PI / 2.0;
                let at = [spawn[0] + fm::cos(a) * r, spawn[1] + fm::sin(a) * r];
                if !self.map.hits(at[0], at[1], half) {
                    return at;
                }
            }
        }
        spawn
    }

    /// A player at `pos` dressed as `look`, carrying its starting weapons and the run's trial powerups.
    pub(super) fn new_player(&self, pos: [f32; 2], look: &PlayerLook) -> Player {
        let d = self.difficulty;
        let t = d.tier();
        let mut kit = look
            .start_defs(t)
            .map_or_else(|| Kit::start(t), |defs| Kit::with_weapons(&defs, t));
        for (b, theirs) in kit.base.iter_mut().zip(look.base) {
            *b = theirs.unwrap_or(*b);
        }
        for &(def, level) in &self.trial {
            kit.raise(def, level);
        }
        let grenades = kit.grenades();
        let mut pl = Player::new(pos, kit, grenades);
        pl.hp = pl.kit.max_hp();
        pl.look = look.id.clone();
        pl.view_yaw = self.me().view_yaw;
        pl
    }

    /// The living player nearest `at`, or the local player when none stands.
    pub(super) fn nearest_player(&self, at: [f32; 2]) -> PlayerId {
        let living = self.players.iter().enumerate().filter(|(_, p)| p.alive());
        living
            .min_by(|a, b| {
                super::len(super::sub(a.1.pos, at)).total_cmp(&super::len(super::sub(b.1.pos, at)))
            })
            .map_or(ME, |(i, _)| i)
    }
}
