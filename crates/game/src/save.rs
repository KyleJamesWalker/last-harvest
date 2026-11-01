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
    /// Runs play on the top screen alone.
    pub one_screen: bool,
    /// On a phone, holding the grenade button shows the map and a tap throws; else it aims in slowed time.
    pub touch_grenades: bool,
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
    /// The player's look, by id; the farmer's is empty.
    pub player: LookId,
    /// The hand-made map runs start on, by id; empty for a generated farm.
    pub map: LookId,
}

/// A player look's id, kept inline so the save stays `Copy`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LookId {
    bytes: [u8; crate::PLAYER_ID_MAX],
    len: u8,
}

impl LookId {
    /// An id as a player card can have it; anything longer or with other characters is the farmer's.
    pub fn new(id: &str) -> Self {
        let fits = id.len() <= crate::PLAYER_ID_MAX && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
        let mut out = Self::default();
        if fits {
            out.bytes[..id.len()].copy_from_slice(id.as_bytes());
            out.len = id.len() as u8;
        }
        out
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }
}

impl Default for Save {
    fn default() -> Self {
        Self { volume: 6, brightness: [4, 4], rumble: true, fps: false, one_screen: false, touch_grenades: true, best: 0, difficulty: Difficulty::default(), kills: 0, best_level: 0, nights: 0, dawns: 0, found: 0, enemies_off: 0, player: LookId::default(), map: LookId::default() }
    }
}

impl Save {
    pub fn to_text(&self) -> String {
        format!(
            "volume={}\nbrightness_top={}\nbrightness_bottom={}\nrumble={}\nfps={}\nlayout={}\ngrenades={}\nbest={}\ndifficulty={}\nkills={}\nbest_level={}\nnights={}\ndawns={}\nfound={}\nenemies_off={}\nplayer={}\nmap={}\n",
            self.volume,
            self.brightness[0],
            self.brightness[1],
            on_off(self.rumble),
            on_off(self.fps),
            if self.one_screen { "one" } else { "two" },
            if self.touch_grenades { "touch" } else { "pad" },
            self.best,
            self.difficulty.key(),
            self.kills,
            self.best_level,
            self.nights,
            self.dawns,
            found_ids(self.found),
            off_ids(self.enemies_off),
            self.player.as_str(),
            self.map.as_str()
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
                "layout" => save.one_screen = value == "one",
                "grenades" => save.touch_grenades = value != "pad",
                "best" => save.best = value.parse().unwrap_or(save.best),
                "difficulty" => save.difficulty = Difficulty::from_key(value).unwrap_or(save.difficulty),
                "kills" => save.kills = value.parse().unwrap_or(save.kills),
                "best_level" => save.best_level = value.parse().unwrap_or(save.best_level),
                "nights" => save.nights = value.parse().unwrap_or(save.nights),
                "dawns" => save.dawns = value.parse().unwrap_or(save.dawns),
                "found" => save.found = found_from_ids(value),
                "enemies_off" => save.enemies_off = off_from_ids(value),
                "player" => save.player = LookId::new(value),
                "map" => save.map = LookId::new(value),
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
        let save = Save { volume: 3, brightness: [7, 2], rumble: false, fps: true, one_screen: true, touch_grenades: false, best: 41, difficulty: Difficulty::Hard, kills: 900, best_level: 12, nights: 7, dawns: 1, found: 0b1010_0000_0000, enemies_off: 0b10, player: LookId::new("jo"), map: LookId::new("old_farm") };
        assert_eq!(Save::parse(&save.to_text()), save);
    }

    #[test]
    fn ids_the_tables_no_longer_have_drop_and_ones_they_gained_start_off() {
        let (rifle, crows) = (found_from_ids("rifle"), found_from_ids("crows"));
        assert!(rifle.count_ones() == 1 && crows.count_ones() == 1 && rifle != crows);
        let kinds = off_ids(u64::MAX);
        let kind = kinds.rsplit(',').next().unwrap();
        let save = Save::parse(&format!("found=rifle,laser_from_v9, crows\nenemies_off=dragon,{kind}"));
        assert_eq!(save.found, rifle | crows, "an unknown powerup is dropped, and every other starts unfound");
        assert_eq!(save.enemies_off, off_from_ids(kind), "an unknown kind is dropped, and every other starts on");
        assert_eq!(save.enemies_off.count_ones(), 1);
        let text = save.to_text();
        assert!(!text.contains("laser_from_v9") && !text.contains("dragon"), "the next save forgets them: {text}");
        assert_eq!(Save::parse(&text), save);
    }

    #[test]
    fn damaged_files_fall_back_per_field() {
        let save = Save::parse("volume=99\nbrightness_top=0\nbest=oops\njunk\nbrightness_bottom=8");
        assert_eq!(save, Save { volume: LEVELS, brightness: [1, 8], ..Save::default() });
    }
}
