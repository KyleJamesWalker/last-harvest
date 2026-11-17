/// Sound cues the game asks the backend to play; the backend owns the synth.
/// `pan` runs from -100 (left) to 100 (right) and `gain` from 0 to 255, for sounds placed in the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sfx {
    /// Button tester blip; the value picks the pitch.
    Tick(u8),
    /// Stereo test tones, one per speaker.
    Left,
    Right,
    Start,
    Miss,
    GameOver,
    /// The dead player climbing back out of the ground as a zombie.
    Turn,
    Shot,
    Blast,
    Slash,
    /// A bullet or blade landing; true for a critical hit.
    Thud(bool),
    /// A zombie going down; the value is its kind, by its place in `enemies.txt`.
    Kill(u8),
    Boom { pan: i8, gain: u8 },
    Throw,
    /// Money picked up; each one in a quick run climbs a step.
    Pickup(u8),
    Heal,
    Hurt,
    Heartbeat,
    LevelUp,
    /// An upgrade chosen; true for a rare one.
    Pick(bool),
    Rise { pan: i8, gain: u8 },
    Groan { pan: i8, gain: u8 },
    Caw { pan: i8, gain: u8 },
    /// A ranged zombie spitting, and its spit landing.
    Spit { pan: i8, gain: u8 },
    Splat { pan: i8, gain: u8 },
    /// The bug zapper's bolt.
    Zap,
    /// The farm dog getting stuck in.
    Bark,
    /// The minute bell.
    Bell,
    Horde,
    Dawn,
    /// A weapon's own sound group, by its place among `powerups.txt`'s `sound` names; one with no clips plays
    /// `or`'s voice, by its place in `Sfx::WEAPON`.
    Group { group: u8, or: u8 },
}

impl Sfx {
    /// The weapon sounds that have a voice of their own when no clip plays.
    pub const WEAPON: [Sfx; 6] = [Sfx::Shot, Sfx::Blast, Sfx::Slash, Sfx::Zap, Sfx::Bark, Sfx::Throw];

    /// The group `group` of a weapon whose behaviour's own sound is `own`.
    pub fn group(group: u8, own: Sfx) -> Sfx {
        Sfx::Group { group, or: Sfx::WEAPON.iter().position(|s| *s == own).unwrap_or(0) as u8 }
    }

    /// What plays when a group has no clips: its weapon's own voice. Any other cue is itself.
    pub fn voice(self) -> Sfx {
        match self {
            Sfx::Group { or, .. } => Sfx::WEAPON[(or as usize).min(Sfx::WEAPON.len() - 1)],
            s => s,
        }
    }
}
