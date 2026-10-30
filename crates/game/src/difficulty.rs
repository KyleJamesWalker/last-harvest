//! How hard a run is: how hard zombies hit, how long a hit protects, and how often health turns up.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Difficulty {
    #[default]
    Easy,
    Normal,
    Hard,
}

impl Difficulty {
    pub const ALL: [Difficulty; 3] = [Difficulty::Easy, Difficulty::Normal, Difficulty::Hard];

    pub fn name(self) -> &'static str {
        match self {
            Difficulty::Easy => "Easy",
            Difficulty::Normal => "Normal",
            Difficulty::Hard => "Hard",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Difficulty::Easy => "Gentle bites and plenty of health, for a first night",
            Difficulty::Normal => "Bites hurt and health is scarce",
            Difficulty::Hard => "Every bite counts; health almost never turns up",
        }
    }

    /// The one `by` steps along, stopping at the ends.
    pub fn step(self, by: i32) -> Self {
        let i = Self::ALL.iter().position(|&d| d == self).unwrap_or(1) as i32;
        Self::ALL[(i + by).clamp(0, Self::ALL.len() as i32 - 1) as usize]
    }

    /// Multiplies the damage a zombie deals.
    pub fn damage(self) -> f32 {
        let t = crate::tuning::tuning();
        [t.easy_damage, t.normal_damage, t.hard_damage][self as usize]
    }

    /// Seconds a hit leaves the player safe from the next.
    pub fn hurt_time(self) -> f32 {
        let t = crate::tuning::tuning();
        [t.easy_hurt_time, t.normal_hurt_time, t.hard_hurt_time][self as usize]
    }

    /// One kill in this many drops health instead of money.
    pub fn health_drop(self) -> usize {
        let t = crate::tuning::tuning();
        [t.easy_health_drop, t.normal_health_drop, t.hard_health_drop][self as usize].max(1.0) as usize
    }

    pub fn heal(self) -> f32 {
        let t = crate::tuning::tuning();
        [t.easy_heal, t.normal_heal, t.hard_heal][self as usize]
    }

    pub fn key(self) -> &'static str {
        ["easy", "normal", "hard"][self as usize]
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.key() == key)
    }
}
