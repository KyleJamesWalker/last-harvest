//! Each player's own state in a run: where they stand, their health and what they carry.

use std::f32::consts::PI;

use super::weapons::Kit;
use super::{Facing, Harvest, sight};

/// A player's place in `Harvest::players`, which owns what they fire, throw and call up.
pub(super) type PlayerId = usize;

/// The one player a run has until there are more.
pub(super) const ME: PlayerId = 0;

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
        }
    }
}

impl Harvest {
    pub(super) fn me(&self) -> &Player {
        &self.players[ME]
    }

    pub(super) fn me_mut(&mut self) -> &mut Player {
        &mut self.players[ME]
    }
}
