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
    /// The bug zapper's bolt.
    Zap,
    /// The farm dog getting stuck in.
    Bark,
    /// The minute bell.
    Bell,
    Horde,
    Dawn,
}
