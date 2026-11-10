//! Settings and records kept between runs, as `key=value` lines.

use crate::Difficulty;
use crate::controls::Controls;
use crate::harvest::{RUN_KIT, Run, found_from_ids, found_ids, off_from_ids, off_ids, powerup_id, powerup_index};

pub const LEVELS: u8 = 10;
/// Runs the history keeps.
pub const HISTORY: usize = 10;
/// Most characters a run's powerups take in its `run=` line.
const KIT_TEXT_MAX: usize = 240;

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
    /// On a phone's screen, whether the on-screen controls show.
    pub touch: TouchControls,
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
    /// Most damage a second over a whole run.
    pub best_dps: u32,
    /// The last runs, newest first.
    pub history: [Option<Run>; HISTORY],
    /// Which pad button sends each game button.
    pub controls: Controls,
}

/// Auto shows the on-screen controls and hides them at gamepad or keyboard input until the next touch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TouchControls {
    #[default]
    Auto,
    On,
    Off,
}

impl TouchControls {
    const ALL: [TouchControls; 3] = [TouchControls::Auto, TouchControls::On, TouchControls::Off];

    pub fn key(self) -> &'static str {
        match self {
            TouchControls::Auto => "auto",
            TouchControls::On => "on",
            TouchControls::Off => "off",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            TouchControls::Auto => "Auto",
            TouchControls::On => "On",
            TouchControls::Off => "Off",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.key() == key)
    }

    /// `by` along Auto, On and Off, wrapping.
    pub fn step(self, by: i32) -> Self {
        Self::ALL[(self as i32 + by).rem_euclid(Self::ALL.len() as i32) as usize]
    }

    /// Whether the controls show on a screen that takes touches.
    pub fn shown(self) -> bool {
        self != TouchControls::Off
    }

    /// Whether gamepad or keyboard input hides them until the next touch.
    pub fn hide_at_other_input(self) -> bool {
        self == TouchControls::Auto
    }
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
        Self { volume: 6, brightness: [4, 4], rumble: true, fps: false, one_screen: false, touch_grenades: true, touch: TouchControls::Auto, best: 0, difficulty: Difficulty::default(), kills: 0, best_level: 0, nights: 0, dawns: 0, found: 0, enemies_off: 0, player: LookId::default(), map: LookId::default(), best_dps: 0, history: [None; HISTORY], controls: Controls::default() }
    }
}

impl Save {
    pub fn to_text(&self) -> String {
        let controls = if self.controls.is_default() { String::new() } else { format!("controls={}\n", self.controls.key()) };
        format!(
            "volume={}\nbrightness_top={}\nbrightness_bottom={}\nrumble={}\nfps={}\nlayout={}\ngrenades={}\ntouch={}\nbest={}\ndifficulty={}\nkills={}\nbest_level={}\nnights={}\ndawns={}\nfound={}\nenemies_off={}\nplayer={}\nmap={}\nbest_dps={}\n{controls}{}",
            self.volume,
            self.brightness[0],
            self.brightness[1],
            on_off(self.rumble),
            on_off(self.fps),
            if self.one_screen { "one" } else { "two" },
            if self.touch_grenades { "touch" } else { "pad" },
            self.touch.key(),
            self.best,
            self.difficulty.key(),
            self.kills,
            self.best_level,
            self.nights,
            self.dawns,
            found_ids(self.found),
            off_ids(self.enemies_off),
            self.player.as_str(),
            self.map.as_str(),
            self.best_dps,
            self.history.iter().flatten().map(|r| format!("run={}\n", run_text(r))).collect::<String>()
        )
    }

    /// Adds a finished run to the history and the best damage a second.
    pub fn record(&mut self, run: Run) {
        self.best_dps = self.best_dps.max(run.dps);
        self.history.rotate_right(1);
        self.history[0] = Some(run);
    }

    /// The run records and found powerups wiped, keeping the settings.
    pub fn without_progress(self) -> Self {
        Self { best: 0, kills: 0, best_level: 0, nights: 0, dawns: 0, found: 0, best_dps: 0, history: [None; HISTORY], ..self }
    }

    /// Unknown keys and bad values fall back to the defaults, so an old or damaged file still loads.
    pub fn parse(text: &str) -> Self {
        let mut save = Self::default();
        let mut runs = 0;
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
                "touch" => save.touch = TouchControls::from_key(value).unwrap_or(save.touch),
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
                "best_dps" => save.best_dps = value.parse().unwrap_or(save.best_dps),
                "controls" => save.controls = Controls::from_key(value).unwrap_or_default(),
                "run" if runs < HISTORY => {
                    if let Some(run) = run_from_text(value) {
                        save.history[runs] = Some(run);
                        runs += 1;
                    }
                }
                _ => {}
            }
        }
        save
    }
}

/// A run as `difficulty,won|lost,level,seconds,damage a second,kills,powerups`, the powerups as `id:level` with
/// spaces between.
fn run_text(r: &Run) -> String {
    let mut kit = String::new();
    for (id, level) in r.kit.iter().flatten().filter_map(|&(d, l)| Some((powerup_id(d as usize)?, l))) {
        let one = format!("{id}:{level}");
        if kit.len() + one.len() + 1 > KIT_TEXT_MAX {
            break;
        }
        if !kit.is_empty() {
            kit.push(' ');
        }
        kit.push_str(&one);
    }
    format!("{},{},{},{},{},{},{kit}", r.difficulty.key(), if r.won { "won" } else { "lost" }, r.level, r.secs, r.dps, r.kills)
}

/// The powerups a `run=` line names that the book still has, at most `RUN_KIT`.
fn kit_from_text(text: &str) -> [Option<(u16, u8)>; RUN_KIT] {
    let mut kit = [None; RUN_KIT];
    let held = text.split_whitespace().filter_map(|w| {
        let (id, level) = w.split_once(':')?;
        Some((powerup_index(id)? as u16, level.parse::<u8>().ok()?))
    });
    for (slot, held) in kit.iter_mut().zip(held) {
        *slot = Some(held);
    }
    kit
}

fn run_from_text(text: &str) -> Option<Run> {
    let f: Vec<&str> = text.split(',').map(str::trim).collect();
    let (difficulty, result, level, secs, dps, kills, kit) = match f[..] {
        [a, b, c, d, e, g] => (a, b, c, d, e, g, ""),
        [a, b, c, d, e, g, k] => (a, b, c, d, e, g, k),
        _ => return None,
    };
    let won = match result {
        "won" => true,
        "lost" => false,
        _ => return None,
    };
    Some(Run { difficulty: Difficulty::from_key(difficulty)?, won, level: level.parse().ok()?, secs: secs.parse().ok()?, dps: dps.parse().ok()?, kills: kills.parse().ok()?, kit: kit_from_text(kit) })
}

fn on_off(b: bool) -> &'static str {
    if b { "on" } else { "off" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let save = Save { volume: 3, brightness: [7, 2], rumble: false, fps: true, one_screen: true, touch_grenades: false, touch: TouchControls::Off, best: 41, difficulty: Difficulty::Hard, kills: 900, best_level: 12, nights: 7, dawns: 1, found: 0b1010_0000_0000, enemies_off: 0b10, player: LookId::new("jo"), map: LookId::new("old_farm"), best_dps: 0, history: [None; HISTORY], controls: Controls::default() };
        assert_eq!(Save::parse(&save.to_text()), save);
    }

    #[test]
    fn the_controls_round_trip_and_an_old_or_odd_save_keeps_the_default() {
        let mut save = Save::default();
        assert!(!save.to_text().contains("controls="), "the default writes nothing");
        save.controls.bind(crate::Buttons::X, crate::Buttons::R1);
        assert!(save.to_text().contains("controls=a b r1 y l1 x l2 r2 select start\n"), "{}", save.to_text());
        assert_eq!(Save::parse(&save.to_text()), save);
        assert_eq!(Save::parse("volume=3\nlayout=one").controls, Controls::default(), "a save from before the setting");
        assert_eq!(Save::parse("controls=a a x y l1 r1 l2 r2 select start").controls, Controls::default(), "a button twice");
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
    fn the_touch_controls_round_trip_and_an_old_or_odd_save_keeps_auto() {
        for touch in TouchControls::ALL {
            let save = Save { touch, ..Save::default() };
            assert_eq!(Save::parse(&save.to_text()).touch, touch);
        }
        assert_eq!(Save::parse("volume=3\nlayout=one").touch, TouchControls::Auto, "a save from before the setting");
        assert_eq!(Save::parse("touch=sometimes").touch, TouchControls::Auto);
        assert_eq!(TouchControls::Auto.step(-1), TouchControls::Off);
        assert_eq!(TouchControls::Off.step(1), TouchControls::Auto);
    }

    fn run(secs: u32, dps: u32) -> Run {
        Run { secs, kills: 7, level: 3, won: secs >= 600, difficulty: Difficulty::Hard, dps, kit: [None; RUN_KIT] }
    }

    fn holding(pairs: &[(&str, u8)]) -> [Option<(u16, u8)>; RUN_KIT] {
        let mut kit = [None; RUN_KIT];
        for (slot, &(id, level)) in kit.iter_mut().zip(pairs) {
            *slot = Some((powerup_index(id).unwrap() as u16, level));
        }
        kit
    }

    #[test]
    fn a_runs_powerups_round_trip_by_id() {
        let mut save = Save::default();
        let kit = holding(&[("pistol", 3), ("knife", 1), ("crows", 5), ("hollow_points", 2)]);
        save.record(Run { kit, ..run(300, 40) });
        let text = save.to_text();
        assert!(text.contains("run=hard,lost,3,300,40,7,pistol:3 knife:1 crows:5 hollow_points:2\n"), "{text}");
        assert_eq!(Save::parse(&text).history[0].unwrap().kit, kit);
    }

    #[test]
    fn a_powerup_the_book_no_longer_has_is_skipped_and_the_run_kept() {
        let save = Save::parse("run=hard,lost,3,300,40,7,pistol:3 laser_from_v9:4 knife:x crows:5\n");
        assert_eq!(save.history[0].unwrap().kit, holding(&[("pistol", 3), ("crows", 5)]));
    }

    #[test]
    fn a_full_kit_keeps_its_line_short() {
        let ids: Vec<&str> = (0..40).filter_map(powerup_id).collect();
        let pairs: Vec<(&str, u8)> = ids.iter().map(|&id| (id, 9)).collect();
        let kit = holding(&pairs);
        assert_eq!(kit.iter().flatten().count(), RUN_KIT, "at most RUN_KIT kept");
        let mut save = Save::default();
        save.record(Run { kit, ..run(300, 40) });
        let line = save.to_text().lines().find(|l| l.starts_with("run=")).unwrap().to_string();
        assert!(line.len() <= KIT_TEXT_MAX + 60, "{line}");
    }

    #[test]
    fn the_history_keeps_the_last_runs_newest_first_and_the_best_damage_a_second() {
        let mut save = Save::default();
        for k in 0..HISTORY as u32 + 2 {
            save.record(run(100 + k, 50 - k));
        }
        assert_eq!(save.best_dps, 50);
        assert_eq!(save.history[0], Some(run(100 + HISTORY as u32 + 1, 50 - HISTORY as u32 - 1)), "newest first");
        assert_eq!(save.history[HISTORY - 1], Some(run(102, 48)), "the oldest two dropped");
        save.record(run(600, 90));
        assert_eq!(Save::parse(&save.to_text()), save);
        assert_eq!(save.without_progress().history, [None; HISTORY]);
        assert_eq!(save.without_progress().best_dps, 0);
    }

    #[test]
    fn a_save_from_before_the_history_loads_with_none() {
        let old = "volume=3\nbest=320\ndifficulty=hard\nkills=40\nbest_level=6\nnights=4\ndawns=0\nfound=\nplayer=\nmap=\n";
        let save = Save::parse(old);
        assert_eq!((save.volume, save.best, save.best_level), (3, 320, 6));
        assert_eq!((save.best_dps, save.history), (0, [None; HISTORY]));
    }

    #[test]
    fn a_damaged_history_line_is_dropped_and_the_rest_kept() {
        let save = Save::parse("run=hard,lost,3,100,40,7\nrun=nightmare,lost,3,100,40,7\nrun=easy,maybe,1,1,1,1\nrun=easy,won,3\nrun=normal,won,9,600,80,12\n");
        assert_eq!(save.history[0], Some(run(100, 40)));
        assert_eq!(save.history[1], Some(Run { difficulty: Difficulty::Normal, won: true, level: 9, secs: 600, dps: 80, kills: 12, kit: [None; RUN_KIT] }), "a line from before powerups loads with none");
        assert_eq!(save.history[2], None);
    }

    #[test]
    fn damaged_files_fall_back_per_field() {
        let save = Save::parse("volume=99\nbrightness_top=0\nbest=oops\njunk\nbrightness_bottom=8");
        assert_eq!(save, Save { volume: LEVELS, brightness: [1, 8], ..Save::default() });
    }
}
