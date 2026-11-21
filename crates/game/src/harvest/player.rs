//! Each player's own state in a run: where they stand, their health and what they carry.

use super::Harvest;

/// A player's place in `Harvest::players`, which owns what they fire, throw and call up.
pub(super) type PlayerId = usize;

/// The one player a run has until there are more.
pub(super) const ME: PlayerId = 0;

pub(super) struct Player {
    pub pos: [f32; 2],
    pub hp: f32,
    pub hurt: f32,
}

impl Player {
    pub fn new(pos: [f32; 2], hp: f32) -> Self {
        Self { pos, hp, hurt: 0.0 }
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
