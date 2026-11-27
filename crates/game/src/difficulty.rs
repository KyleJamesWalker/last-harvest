//! How hard a run is: one of `tuning.txt`'s difficulties, and the kit tier whose starting weapons and grenades it
//! plays with.

use crate::tuning::{Preset, rules};

/// The three tiers `powerups.txt` and player cards give starting weapons and grenades for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tier {
    #[default]
    Easy,
    Normal,
    Hard,
}

impl Tier {
    pub const ALL: [Tier; 3] = [Tier::Easy, Tier::Normal, Tier::Hard];

    pub fn key(self) -> &'static str {
        ["easy", "normal", "hard"][self as usize]
    }

    pub fn name(self) -> &'static str {
        ["Easy", "Normal", "Hard"][self as usize]
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.key() == key)
    }
}

/// A difficulty, by its place in `tuning.txt`'s list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Difficulty(u8);

impl Default for Difficulty {
    fn default() -> Self {
        Self::EASY
    }
}

impl Difficulty {
    /// The game's own come first, in this order, and a mod can only add after them.
    pub const STROLL: Difficulty = Difficulty(0);
    pub const EASY: Difficulty = Difficulty(1);
    pub const NORMAL: Difficulty = Difficulty(2);
    pub const HARD: Difficulty = Difficulty(3);
    pub const BRUTAL: Difficulty = Difficulty(4);
    pub const DEAD: Difficulty = Difficulty(5);

    fn preset(self) -> &'static Preset {
        let all = &rules().presets;
        all.get(self.0 as usize)
            .unwrap_or(&all[Self::NORMAL.0 as usize])
    }

    /// Every difficulty, in order.
    pub fn all() -> impl Iterator<Item = Difficulty> {
        (0..rules().presets.len() as u8).map(Difficulty)
    }

    pub fn index(self) -> u8 {
        self.0
    }

    /// The difficulty at `i`, if there is one.
    pub fn from_index(i: u8) -> Option<Self> {
        ((i as usize) < rules().presets.len()).then_some(Difficulty(i))
    }

    pub fn name(self) -> &'static str {
        &self.preset().name
    }

    pub fn blurb(self) -> &'static str {
        &self.preset().blurb
    }

    pub fn key(self) -> &'static str {
        &self.preset().id
    }

    pub fn from_key(key: &str) -> Option<Self> {
        rules()
            .presets
            .iter()
            .position(|p| p.id == key)
            .map(|i| Difficulty(i as u8))
    }

    /// Whose starting weapons and grenades it plays with.
    pub fn tier(self) -> Tier {
        self.preset().kit
    }

    /// The one `by` steps along, stopping at the ends.
    pub fn step(self, by: i32) -> Self {
        let last = rules().presets.len() as i32 - 1;
        Difficulty((self.0 as i32 + by).clamp(0, last) as u8)
    }

    pub fn is_first(self) -> bool {
        self.0 == 0
    }

    pub fn is_last(self) -> bool {
        self.0 as usize + 1 >= rules().presets.len()
    }

    /// Makes this thread's `tuning()` this difficulty's numbers.
    pub fn play(self) {
        crate::tuning::play_at(Some(self.0 as usize));
    }

    /// Multiplies the damage a zombie deals.
    pub fn damage(self) -> f32 {
        self.preset().damage
    }

    /// Seconds a hit leaves the player safe from the next.
    pub fn hurt_time(self) -> f32 {
        self.preset().hurt_time
    }

    /// One kill in this many drops health; the rest drop hearts.
    pub fn health_drop(self) -> usize {
        self.preset().health_drop.max(1.0) as usize
    }

    pub fn heal(self) -> f32 {
        self.preset().heal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_games_own_difficulties_come_in_order() {
        let ids: Vec<&str> = Difficulty::all().map(Difficulty::key).collect();
        assert_eq!(ids, ["stroll", "easy", "normal", "hard", "brutal", "dead"]);
        for (d, tier) in [
            (Difficulty::EASY, Tier::Easy),
            (Difficulty::NORMAL, Tier::Normal),
            (Difficulty::HARD, Tier::Hard),
        ] {
            assert_eq!((d.key(), d.tier()), (tier.key(), tier));
        }
        assert!(Difficulty::STROLL.is_first() && Difficulty::DEAD.is_last());
        assert_eq!(Difficulty::STROLL.step(-1), Difficulty::STROLL);
        assert_eq!(Difficulty::DEAD.step(1), Difficulty::DEAD);
        assert_eq!(Difficulty::from_key("brutal"), Some(Difficulty::BRUTAL));
        assert_eq!(Difficulty::from_index(9), None);
    }
}
