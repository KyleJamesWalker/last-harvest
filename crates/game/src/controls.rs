//! Which pad button sends each game button. The platforms report buttons where the RG DS has them; the player can
//! move any of the face, shoulder, Select and Start buttons to another, and the game reads them through this table.

use crate::Buttons;

/// The buttons a player can move, in the order a save lists them.
pub const BINDABLE: [Buttons; 10] = [Buttons::A, Buttons::B, Buttons::X, Buttons::Y, Buttons::L1, Buttons::R1, Buttons::L2, Buttons::R2, Buttons::SELECT, Buttons::START];
const KEYS: [&str; 10] = ["a", "b", "x", "y", "l1", "r1", "l2", "r2", "select", "start"];

/// Each game button's job, in the order the Controls page lists them.
pub const ACTIONS: [(Buttons, &str); 10] = [
    (Buttons::Y, "Turn left, favor"),
    (Buttons::A, "Turn right, choose"),
    (Buttons::B, "Turn around, back"),
    (Buttons::X, "Pick, aim at barrels"),
    (Buttons::L1, "Map, banish"),
    (Buttons::L2, "Aim a grenade"),
    (Buttons::R1, "Throw ahead, reroll"),
    (Buttons::R2, "Screenshot"),
    (Buttons::START, "Pause"),
    (Buttons::SELECT, "Frame rate"),
];

fn index(b: Buttons) -> Option<usize> {
    BINDABLE.iter().position(|&x| x == b)
}

fn all() -> Buttons {
    BINDABLE.into_iter().fold(Buttons::NONE, |a, b| a | b)
}

/// For each game button in `BINDABLE` order, the index of the pad button that sends it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Controls([u8; 10]);

impl Default for Controls {
    fn default() -> Self {
        Self([0, 1, 2, 3, 4, 5, 6, 7, 8, 9])
    }
}

impl Controls {
    /// The pad button that sends game button `game`; a button that can't move sends itself.
    pub fn physical(self, game: Buttons) -> Buttons {
        index(game).map_or(game, |i| BINDABLE[self.0[i] as usize])
    }

    /// The game buttons that the pad buttons `pad` send.
    pub fn apply(self, pad: Buttons) -> Buttons {
        let mut out = pad & !all();
        for (i, &game) in BINDABLE.iter().enumerate() {
            if pad.contains(BINDABLE[self.0[i] as usize]) {
                out |= game;
            }
        }
        out
    }

    /// Makes pad button `pad` send game button `game`. The game button that `pad` sent before takes `game`'s old pad
    /// button, and is returned.
    pub fn bind(&mut self, game: Buttons, pad: Buttons) -> Option<Buttons> {
        let (g, p) = (index(game)?, index(pad)? as u8);
        let had = self.0.iter().position(|&x| x == p)?;
        if had == g {
            return None;
        }
        self.0.swap(g, had);
        Some(BINDABLE[had])
    }

    pub fn is_default(self) -> bool {
        self == Self::default()
    }

    /// The pad button for each game button in `BINDABLE` order, as a save writes it.
    pub fn key(self) -> String {
        self.0.iter().map(|&i| KEYS[i as usize]).collect::<Vec<_>>().join(" ")
    }

    /// A `key`, if it names every pad button once.
    pub fn from_key(text: &str) -> Option<Self> {
        let mut out = [0u8; 10];
        let mut words = text.split_whitespace();
        for slot in &mut out {
            let word = words.next()?;
            *slot = KEYS.iter().position(|&k| k == word)? as u8;
        }
        let mut seen = [false; 10];
        for &i in &out {
            if std::mem::replace(&mut seen[i as usize], true) {
                return None;
            }
        }
        words.next().is_none().then_some(Self(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_sends_each_button_as_itself() {
        let c = Controls::default();
        for b in BINDABLE {
            assert_eq!(c.physical(b), b);
            assert_eq!(c.apply(b), b);
        }
        assert_eq!(c.apply(Buttons::UP | Buttons::HOME), Buttons::UP | Buttons::HOME, "the d-pad and system keys never move");
    }

    #[test]
    fn binding_a_taken_button_swaps_the_two() {
        let mut c = Controls::default();
        assert_eq!(c.bind(Buttons::X, Buttons::R1), Some(Buttons::R1), "R1 had its own job");
        assert_eq!(c.physical(Buttons::X), Buttons::R1);
        assert_eq!(c.physical(Buttons::R1), Buttons::X, "and takes X's old button");
        assert_eq!(c.apply(Buttons::R1 | Buttons::UP), Buttons::X | Buttons::UP);
        assert_eq!(c.apply(Buttons::X), Buttons::R1);
        assert_eq!(c.bind(Buttons::X, Buttons::R1), None, "already there");
        assert_eq!(c.bind(Buttons::UP, Buttons::R1), None, "the d-pad can't move");
        assert!(!c.is_default());
    }

    #[test]
    fn a_save_writes_and_reads_the_table() {
        let mut c = Controls::default();
        c.bind(Buttons::A, Buttons::B);
        c.bind(Buttons::START, Buttons::L2);
        assert_eq!(c.key(), "b a x y l1 r1 start r2 select l2");
        assert_eq!(Controls::from_key(&c.key()), Some(c));
        for bad in ["", "a b x y l1 r1 l2 r2 select", "a a x y l1 r1 l2 r2 select start", "a b x y l1 r1 l2 r2 select start up", "a b x y l1 r1 l2 r2 select home"] {
            assert_eq!(Controls::from_key(bad), None, "{bad:?}");
        }
    }
}
