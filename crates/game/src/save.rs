//! Settings and records kept between runs, as `key=value` lines.

use crate::Difficulty;
use crate::harvest::{found_from_ids, found_ids, off_from_ids, off_ids};

pub const LEVELS: u8 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Save {
    pub volume: u8,
    /// Per-screen brightness in 1..=LEVELS, top first.
    pub brightness: [u8; 2],
    pub rumble: bool,
    /// The frame-rate overlay, which Select also toggles.
    pub fps: bool,
    /// Longest night survived, in seconds.
    pub best: u32,
    pub difficulty: Difficulty,
    /// Zombies killed over every run, highest level reached, nights played and nights survived, which unlock powerups.
    pub kills: u32,
    pub best_level: u32,
    pub nights: u32,
    pub dawns: u32,
    /// Powerups taken from a find card, kept by id.
    pub found: u64,
    /// Zombie kinds turned off, kept by id.
    pub enemies_off: u64,
}

impl Default for Save {
    fn default() -> Self {
        Self { volume: 6, brightness: [4, 4], rumble: true, fps: false, best: 0, difficulty: Difficulty::Normal, kills: 0, best_level: 0, nights: 0, dawns: 0, found: 0, enemies_off: 0 }
    }
}

impl Save {
    pub fn to_text(&self) -> String {
        format!(
            "volume={}\nbrightness_top={}\nbrightness_bottom={}\nrumble={}\nfps={}\nbest={}\ndifficulty={}\nkills={}\nbest_level={}\nnights={}\ndawns={}\nfound={}\nenemies_off={}\n",
            self.volume,
            self.brightness[0],
            self.brightness[1],
            on_off(self.rumble),
            on_off(self.fps),
            self.best,
            self.difficulty.key(),
            self.kills,
            self.best_level,
            self.nights,
            self.dawns,
            found_ids(self.found),
            off_ids(self.enemies_off)
        )
    }

    /// The run records and found powerups wiped, keeping the settings.
    pub fn without_progress(self) -> Self {
        Self { best: 0, kills: 0, best_level: 0, nights: 0, dawns: 0, found: 0, ..self }
    }

    /// Unknown keys and bad values fall back to the defaults, so an old or damaged file still loads.
    pub fn parse(text: &str) -> Self {
        let mut save = Self::default();
        for (key, value) in text.lines().filter_map(|l| l.split_once('=')) {
            let value = value.trim();
            match key.trim() {
                "volume" => save.volume = value.parse().unwrap_or(save.volume).min(LEVELS),
                "brightness_top" => save.brightness[0] = value.parse().unwrap_or(save.brightness[0]).clamp(1, LEVELS),
                "brightness_bottom" => save.brightness[1] = value.parse().unwrap_or(save.brightness[1]).clamp(1, LEVELS),
                "rumble" => save.rumble = value == "on",
                "fps" => save.fps = value == "on",
                "best" => save.best = value.parse().unwrap_or(save.best),
                "difficulty" => save.difficulty = Difficulty::from_key(value).unwrap_or(save.difficulty),
                "kills" => save.kills = value.parse().unwrap_or(save.kills),
                "best_level" => save.best_level = value.parse().unwrap_or(save.best_level),
                "nights" => save.nights = value.parse().unwrap_or(save.nights),
                "dawns" => save.dawns = value.parse().unwrap_or(save.dawns),
                "found" => save.found = found_from_ids(value),
                "enemies_off" => save.enemies_off = off_from_ids(value),
                _ => {}
            }
        }
        save
    }
}

fn on_off(b: bool) -> &'static str {
    if b { "on" } else { "off" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let save = Save { volume: 3, brightness: [7, 2], rumble: false, fps: true, best: 41, difficulty: Difficulty::Hard, kills: 900, best_level: 12, nights: 7, dawns: 1, found: 0b1010_0000_0000, enemies_off: 0b10 };
        assert_eq!(Save::parse(&save.to_text()), save);
    }

    #[test]
    fn damaged_files_fall_back_per_field() {
        let save = Save::parse("volume=99\nbrightness_top=0\nbest=oops\njunk\nbrightness_bottom=8");
        assert_eq!(save, Save { volume: LEVELS, brightness: [1, 8], ..Save::default() });
    }
}
