//! Settings and records kept between runs, as `key=value` lines.

use crate::Difficulty;
use crate::tuning::{PRESETS_MAX, PRESET_ID_MAX};
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
    /// On one screen, the small map in the corner of the view ahead.
    pub corner_map: bool,
    /// On a phone, holding the grenade button shows the map and a tap throws; else it aims in slowed time.
    pub touch_grenades: bool,
    /// On a phone's screen, whether the on-screen controls show.
    pub touch: TouchControls,
    pub difficulty: Difficulty,
    /// Each difficulty's records and unlocks, by its id; an empty id is a free slot.
    pub records: [(PresetId, Records); PRESETS_MAX],
    /// Zombie kinds turned off, kept by id.
    pub enemies_off: u64,
    /// The player's look, by id; the farmer's is empty.
    pub player: LookId,
    /// The hand-made map runs start on, by id; empty for a generated farm.
    pub map: LookId,
    /// The last runs, newest first.
    pub history: [Option<Run>; HISTORY],
    /// The how-to-play page has been shown, so the first Play no longer opens it.
    pub how_to_seen: bool,
    /// Which pad button sends each game button.
    pub controls: Controls,
    /// The online relay's address; empty for `netplay::DEFAULT_RELAY`.
    pub relay: Relay,
    /// The room code this device last hosted, and the one it last joined.
    pub hosted: Option<last_harvest_lobby::RoomCode>,
    pub joined: Option<last_harvest_lobby::RoomCode>,
    /// Mod packs turned off from the next start, by key.
    pub mods_off: ModsOff,
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

pub const RELAY_MAX: usize = 96;

/// The relay's address, kept inline so the save stays `Copy`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Relay {
    bytes: [u8; RELAY_MAX],
    len: u8,
}

impl Default for Relay {
    fn default() -> Self {
        Self { bytes: [0; RELAY_MAX], len: 0 }
    }
}

impl Relay {
    /// Printable ASCII with no spaces, cut to `RELAY_MAX`; anything else is left out.
    pub fn new(text: &str) -> Self {
        let mut out = Self::default();
        for b in text.bytes().filter(|b| b.is_ascii_graphic()).take(RELAY_MAX) {
            out.bytes[out.len as usize] = b;
            out.len += 1;
        }
        out
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }
}

pub const MODS_OFF_MAX: usize = 16;
pub const PACK_KEY_MAX: usize = 32;

/// Lowercase letters, digits and `_`; each run of anything else becomes one `-`.
pub fn pack_key(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars().flat_map(char::to_lowercase) {
        if out.len() >= PACK_KEY_MAX {
            break;
        }
        if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

/// The mod packs turned off, by key, kept inline so the save stays `Copy`.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct ModsOff {
    keys: [[u8; PACK_KEY_MAX]; MODS_OFF_MAX],
    lens: [u8; MODS_OFF_MAX],
}

impl std::fmt::Debug for ModsOff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.keys()).finish()
    }
}

impl ModsOff {
    pub fn parse(text: &str) -> Self {
        let mut out = Self::default();
        for key in text.split(',').map(str::trim).filter(|k| !k.is_empty() && pack_key(k) == *k) {
            out.push(key);
        }
        out
    }

    fn push(&mut self, key: &str) {
        let Some(at) = self.lens.iter().position(|&l| l == 0) else { return };
        if !self.is_off(key) {
            self.keys[at][..key.len()].copy_from_slice(key.as_bytes());
            self.lens[at] = key.len() as u8;
        }
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.keys.iter().zip(self.lens).take_while(|(_, l)| *l > 0).map(|(k, l)| std::str::from_utf8(&k[..l as usize]).unwrap_or(""))
    }

    pub fn is_off(&self, key: &str) -> bool {
        self.keys().any(|k| k == key)
    }

    /// With the list full, turning one off drops the first key `gone` accepts; false when there is none.
    pub fn set(&mut self, key: &str, off: bool, gone: impl Fn(&str) -> bool) -> bool {
        let mut kept: Vec<String> = self.keys().filter(|k| *k != key).map(String::from).collect();
        if off {
            if kept.len() >= MODS_OFF_MAX {
                let Some(at) = kept.iter().position(|k| gone(k)) else { return false };
                kept.remove(at);
            }
            kept.push(pack_key(key));
        }
        *self = Self::parse(&kept.join(","));
        true
    }

    pub fn to_text(&self) -> String {
        self.keys().collect::<Vec<_>>().join(",")
    }
}

/// The save's `mods_off` keys, read without the game's data, which isn't installed yet when the app needs them.
pub fn mods_off_in(save: &str) -> Vec<String> {
    let line = save.lines().filter_map(|l| l.split_once('=')).rfind(|(k, _)| k.trim() == "mods_off");
    line.map(|(_, v)| ModsOff::parse(v.trim()).keys().map(String::from).collect()).unwrap_or_default()
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

/// A difficulty's id, kept inline so the save stays `Copy`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PresetId {
    bytes: [u8; PRESET_ID_MAX],
    len: u8,
}

impl PresetId {
    /// An id as `tuning.txt` can have it, else empty.
    pub fn new(id: &str) -> Self {
        let mut out = Self::default();
        if crate::tuning::valid_preset_id(id) {
            out.bytes[..id.len()].copy_from_slice(id.as_bytes());
            out.len = id.len() as u8;
        }
        out
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }
}

/// What one difficulty's nights have earned: its records, and the counts and finds that unlock powerups.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Records {
    /// Longest night survived, in seconds.
    pub best: u32,
    /// Zombies killed over every run, highest level reached, nights played, nights survived and nights with a level breaker.
    pub kills: u32,
    pub best_level: u32,
    pub nights: u32,
    pub dawns: u32,
    pub breakers: u32,
    pub breaker_dawns: u32,
    /// Powerups taken from a find card, kept by id.
    pub found: u64,
    /// Most damage a second over a whole run.
    pub best_dps: u32,
}

impl Records {
    fn to_text(self) -> String {
        format!("best:{} kills:{} level:{} nights:{} dawns:{} breakers:{} breaker_dawns:{} dps:{} found:{}", self.best, self.kills, self.best_level, self.nights, self.dawns, self.breakers, self.breaker_dawns, self.best_dps, found_ids(self.found))
    }

    /// Words it doesn't know, and numbers that don't read, are left at 0.
    fn from_text(text: &str) -> Self {
        let mut r = Self::default();
        for (key, value) in text.split_whitespace().filter_map(|w| w.split_once(':')) {
            let n = || value.parse().unwrap_or(0);
            match key {
                "best" => r.best = n(),
                "kills" => r.kills = n(),
                "level" => r.best_level = n(),
                "nights" => r.nights = n(),
                "dawns" => r.dawns = n(),
                "breakers" => r.breakers = n(),
                "breaker_dawns" => r.breaker_dawns = n(),
                "dps" => r.best_dps = n(),
                "found" => r.found = found_from_ids(value),
                _ => {}
            }
        }
        r
    }
}

impl Default for Save {
    fn default() -> Self {
        Self { volume: 6, brightness: [4, 4], rumble: true, fps: false, one_screen: false, corner_map: true, touch_grenades: true, touch: TouchControls::Auto, difficulty: Difficulty::default(), records: [(PresetId::default(), Records::default()); PRESETS_MAX], enemies_off: 0, player: LookId::default(), map: LookId::default(), history: [None; HISTORY], how_to_seen: false, controls: Controls::default(), relay: Relay::default(), hosted: None, joined: None, mods_off: ModsOff::default() }
    }
}

impl Save {
    pub fn to_text(&self) -> String {
        let mut controls = if self.controls.is_default() { String::new() } else { format!("controls={}\n", self.controls.key()) };
        if self.relay.len > 0 {
            controls += &format!("relay={}\n", self.relay.as_str());
        }
        if !self.mods_off.to_text().is_empty() {
            controls += &format!("mods_off={}\n", self.mods_off.to_text());
        }
        for (key, code) in [("hosted", self.hosted), ("joined", self.joined)] {
            if let Some(code) = code {
                controls += &format!("{key}={code}\n");
            }
        }
        format!(
            "volume={}\nbrightness_top={}\nbrightness_bottom={}\nrumble={}\nfps={}\nlayout={}\ncorner_map={}\ngrenades={}\ntouch={}\ndifficulty={}\nenemies_off={}\nplayer={}\nmap={}\nhow_to={}\n{controls}{}{}",
            self.volume,
            self.brightness[0],
            self.brightness[1],
            on_off(self.rumble),
            on_off(self.fps),
            if self.one_screen { "one" } else { "two" },
            on_off(self.corner_map),
            if self.touch_grenades { "touch" } else { "pad" },
            self.touch.key(),
            self.difficulty.key(),
            off_ids(self.enemies_off),
            self.player.as_str(),
            self.map.as_str(),
            if self.how_to_seen { "seen" } else { "due" },
            self.records.iter().filter(|(id, _)| id.len > 0).map(|(id, r)| format!("record.{}={}\n", id.as_str(), r.to_text())).collect::<String>(),
            self.history.iter().flatten().map(|r| format!("run={}\n", run_text(r))).collect::<String>()
        )
    }

    /// Adds a finished run to the history and the best damage a second.
    pub fn record(&mut self, run: Run) {
        if let Some(r) = self.records_mut(run.difficulty) {
            r.best_dps = r.best_dps.max(run.dps);
        }
        self.history.rotate_right(1);
        self.history[0] = Some(run);
    }

    /// The run records and found powerups wiped, keeping the settings.
    pub fn without_progress(self) -> Self {
        Self { records: Save::default().records, history: [None; HISTORY], ..self }
    }

    /// What `d`'s nights have earned so far.
    pub fn records(&self, d: Difficulty) -> Records {
        self.records.iter().find(|(id, _)| id.as_str() == d.key()).map(|(_, r)| *r).unwrap_or_default()
    }

    /// `d`'s records to change: its own slot, else a free one, else one whose difficulty isn't loaded.
    pub fn records_mut(&mut self, d: Difficulty) -> Option<&mut Records> {
        self.records_for(d.key(), |id| Difficulty::from_key(id).is_some())
    }

    fn records_for(&mut self, key: &str, loaded: impl Fn(&str) -> bool) -> Option<&mut Records> {
        let at = self.records.iter().position(|(id, _)| id.as_str() == key).or_else(|| self.records.iter().position(|(id, _)| id.len == 0)).or_else(|| self.records.iter().position(|(id, _)| !loaded(id.as_str())))?;
        if self.records[at].0.as_str() != key {
            self.records[at] = (PresetId::new(key), Records::default());
        }
        Some(&mut self.records[at].1)
    }

    /// Unknown keys and bad values fall back to the defaults, so an old or damaged file still loads; a file from
    /// before `how_to` counts it seen once a night has been played.
    pub fn parse(text: &str) -> Self {
        let mut save = Self::default();
        let mut runs = 0;
        let mut how_to = None;
        let (mut legacy, mut any_record) = (Records::default(), false);
        for (key, value) in text.lines().filter_map(|l| l.split_once('=')) {
            let value = value.trim();
            match key.trim() {
                "volume" => save.volume = value.parse().unwrap_or(save.volume).min(LEVELS),
                "brightness_top" => save.brightness[0] = value.parse().unwrap_or(save.brightness[0]).clamp(1, LEVELS),
                "brightness_bottom" => save.brightness[1] = value.parse().unwrap_or(save.brightness[1]).clamp(1, LEVELS),
                "rumble" => save.rumble = value == "on",
                "fps" => save.fps = value == "on",
                "layout" => save.one_screen = value == "one",
                "corner_map" => save.corner_map = value != "off",
                "grenades" => save.touch_grenades = value != "pad",
                "touch" => save.touch = TouchControls::from_key(value).unwrap_or(save.touch),
                "best" => legacy.best = value.parse().unwrap_or(legacy.best),
                "difficulty" => save.difficulty = Difficulty::from_key(value).unwrap_or(save.difficulty),
                "kills" => legacy.kills = value.parse().unwrap_or(legacy.kills),
                "best_level" => legacy.best_level = value.parse().unwrap_or(legacy.best_level),
                "nights" => legacy.nights = value.parse().unwrap_or(legacy.nights),
                "dawns" => legacy.dawns = value.parse().unwrap_or(legacy.dawns),
                "breakers" => legacy.breakers = value.parse().unwrap_or(legacy.breakers),
                "breaker_dawns" => legacy.breaker_dawns = value.parse().unwrap_or(legacy.breaker_dawns),
                "found" => legacy.found = found_from_ids(value),
                "enemies_off" => save.enemies_off = off_from_ids(value),
                "player" => save.player = LookId::new(value),
                "map" => save.map = LookId::new(value),
                "best_dps" => legacy.best_dps = value.parse().unwrap_or(legacy.best_dps),
                key if key.starts_with("record.") => {
                    let id = &key["record.".len()..];
                    if crate::tuning::valid_preset_id(id)
                        && let Some(r) = save.records_for(id, |_| true)
                    {
                        *r = Records::from_text(value);
                        any_record = true;
                    }
                }
                "how_to" => how_to = Some(value != "due"),
                "controls" => save.controls = Controls::from_key(value).unwrap_or_default(),
                "relay" => save.relay = Relay::new(value),
                "mods_off" => save.mods_off = ModsOff::parse(value),
                "hosted" => save.hosted = last_harvest_lobby::RoomCode::parse(value).ok(),
                "joined" => save.joined = last_harvest_lobby::RoomCode::parse(value).ok(),
                "run" if runs < HISTORY => {
                    if let Some(run) = run_from_text(value) {
                        save.history[runs] = Some(run);
                        runs += 1;
                    }
                }
                _ => {}
            }
        }
        // A save from before difficulties had their own records earned them on every one it could play.
        if !any_record && legacy != Records::default() {
            for key in ["easy", "normal", "hard"] {
                if let Some(r) = save.records_for(key, |_| true) {
                    *r = legacy;
                }
            }
        }
        save.how_to_seen = how_to.unwrap_or(save.records.iter().any(|(_, r)| r.nights > 0));
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
    Some(Run { difficulty: Difficulty::from_key(difficulty)?, won, level: level.parse().ok()?, secs: secs.parse().ok()?, dps: dps.parse().ok()?, kills: kills.parse().ok()?, kit: kit_from_text(kit), broke: false })
}

fn on_off(b: bool) -> &'static str {
    if b { "on" } else { "off" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let mut save = Save { volume: 3, brightness: [7, 2], rumble: false, fps: true, one_screen: true, corner_map: false, touch_grenades: false, touch: TouchControls::Off, difficulty: Difficulty::HARD, enemies_off: 0b10, player: LookId::new("jo"), map: LookId::new("old_farm"), history: [None; HISTORY], how_to_seen: true, controls: Controls::default(), relay: Relay::new("wss://relay.example.com"), hosted: last_harvest_lobby::RoomCode::parse("ACDEF").ok(), joined: last_harvest_lobby::RoomCode::parse("FEDCA").ok(), ..Save::default() };
        assert!(save.hosted.is_some() && save.joined.is_some());
        *save.records_mut(Difficulty::HARD).unwrap() = Records { best: 41, kills: 900, best_level: 12, nights: 7, dawns: 1, breakers: 3, breaker_dawns: 1, found: 0b1010_0000_0000, best_dps: 0 };
        *save.records_mut(Difficulty::DEAD).unwrap() = Records { kills: 5, nights: 1, best_dps: 70, ..Records::default() };
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
        let save = Save::parse(&format!("record.normal=found:rifle,laser_from_v9,crows\nenemies_off=dragon,{kind}"));
        assert_eq!(save.records(Difficulty::NORMAL).found, rifle | crows, "an unknown powerup is dropped, and every other starts unfound");
        assert_eq!(save.enemies_off, off_from_ids(kind), "an unknown kind is dropped, and every other starts on");
        assert_eq!(save.enemies_off.count_ones(), 1);
        let text = save.to_text();
        assert!(!text.contains("laser_from_v9") && !text.contains("dragon"), "the next save forgets them: {text}");
        assert_eq!(Save::parse(&text), save);
    }

    #[test]
    fn mods_off_round_trip_keep_unknown_keys_and_read_without_the_data() {
        assert!(!Save::default().to_text().contains("mods_off"), "nothing off writes nothing");
        let mut save = Save::default();
        assert!(save.mods_off.set("thanksgiving", true, |_| false));
        assert!(save.mods_off.set("my-cards", true, |_| false));
        let text = save.to_text();
        assert!(text.contains("mods_off=thanksgiving,my-cards\n"), "{text}");
        assert_eq!(Save::parse(&text), save);
        assert_eq!(mods_off_in(&text), ["thanksgiving", "my-cards"]);
        assert_eq!(Save::parse("mods_off=gone, Bad Key,ok").mods_off.to_text(), "gone,ok", "an unknown pack stays off; a bad key drops");
        assert!(save.mods_off.set("thanksgiving", false, |_| false));
        assert_eq!(save.mods_off.to_text(), "my-cards");
        assert!(mods_off_in("volume=3").is_empty());
    }

    #[test]
    fn a_full_list_drops_a_pack_no_longer_found_before_refusing() {
        let mut off = ModsOff::default();
        for i in 0..MODS_OFF_MAX {
            assert!(off.set(&format!("p{i}"), true, |_| false));
        }
        assert!(!off.set("new", true, |_| false), "no room and nothing gone");
        assert!(off.set("new", true, |k| k == "p3"));
        assert!(off.is_off("new") && !off.is_off("p3"));
    }

    #[test]
    fn a_pack_key_is_its_name_cut_to_letters_digits_and_dashes() {
        assert_eq!(pack_key("Thanksgiving"), "thanksgiving");
        assert_eq!(pack_key("  Night of the Living Dead! "), "night-of-the-living-dead");
        assert_eq!(pack_key("my_cards.pak"), "my_cards-pak");
        assert_eq!(pack_key("!!!"), "");
        assert!(pack_key(&"a".repeat(80)).len() <= PACK_KEY_MAX);
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
        Run { secs, kills: 7, level: 3, won: secs >= 600, difficulty: Difficulty::HARD, dps, kit: [None; RUN_KIT], broke: false }
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
        assert_eq!(save.records(Difficulty::HARD).best_dps, 50);
        assert_eq!(save.history[0], Some(run(100 + HISTORY as u32 + 1, 50 - HISTORY as u32 - 1)), "newest first");
        assert_eq!(save.history[HISTORY - 1], Some(run(102, 48)), "the oldest two dropped");
        save.record(run(600, 90));
        assert_eq!(Save::parse(&save.to_text()), save);
        assert_eq!(save.without_progress().history, [None; HISTORY]);
        assert_eq!(save.without_progress().records(Difficulty::HARD).best_dps, 0);
    }

    #[test]
    fn a_save_from_before_the_history_loads_with_none() {
        let old = "volume=3\nbest=320\ndifficulty=hard\nkills=40\nbest_level=6\nnights=4\ndawns=0\nfound=\nplayer=\nmap=\n";
        let save = Save::parse(old);
        let hard = save.records(Difficulty::HARD);
        assert_eq!((save.volume, hard.best, hard.best_level), (3, 320, 6));
        assert_eq!((hard.best_dps, save.history), (0, [None; HISTORY]));
    }

    #[test]
    fn an_old_saves_records_and_unlocks_carry_to_easy_normal_and_hard() {
        let old = "difficulty=normal\nbest=320\nkills=40\nbest_level=6\nnights=4\ndawns=1\nbreakers=2\nbreaker_dawns=1\nfound=rifle\nbest_dps=55\n";
        let save = Save::parse(old);
        let want = Records { best: 320, kills: 40, best_level: 6, nights: 4, dawns: 1, breakers: 2, breaker_dawns: 1, found: found_from_ids("rifle"), best_dps: 55 };
        for d in [Difficulty::EASY, Difficulty::NORMAL, Difficulty::HARD] {
            assert_eq!(save.records(d), want, "{}", d.key());
        }
        for d in [Difficulty::STROLL, Difficulty::BRUTAL, Difficulty::DEAD] {
            assert_eq!(save.records(d), Records::default(), "{} starts fresh", d.key());
        }
        assert!(save.how_to_seen);
        let again = Save::parse(&save.to_text());
        assert_eq!(again, save, "the next save keeps them per difficulty");
        assert!(!save.to_text().contains("\nkills="), "{}", save.to_text());
        assert_eq!(Save::parse(&format!("{old}record.hard=kills:3\n")).records(Difficulty::EASY), Records::default(), "a save with its own records reads only those");
    }

    #[test]
    fn a_difficulty_a_mod_no_longer_adds_keeps_its_records() {
        let save = Save::parse("record.nightmare=kills:12 nights:2\nrecord.Bad-Id=kills:9\n");
        assert!(save.to_text().contains("record.nightmare=best:0 kills:12 level:0 nights:2"), "{}", save.to_text());
        assert!(!save.to_text().contains("Bad-Id"));
    }

    #[test]
    fn a_damaged_history_line_is_dropped_and_the_rest_kept() {
        let save = Save::parse("run=hard,lost,3,100,40,7\nrun=nightmare,lost,3,100,40,7\nrun=easy,maybe,1,1,1,1\nrun=easy,won,3\nrun=normal,won,9,600,80,12\n");
        assert_eq!(save.history[0], Some(run(100, 40)));
        assert_eq!(save.history[1], Some(Run { difficulty: Difficulty::NORMAL, won: true, level: 9, secs: 600, dps: 80, kills: 12, kit: [None; RUN_KIT], ..Run::default() }), "a line from before powerups loads with none");
        assert_eq!(save.history[2], None);
    }

    #[test]
    fn how_to_play_round_trips_and_an_old_save_counts_it_seen_once_a_night_was_played() {
        for seen in [false, true] {
            let save = Save { how_to_seen: seen, ..Save::default() };
            assert_eq!(Save::parse(&save.to_text()).how_to_seen, seen);
        }
        assert!(!Save::default().how_to_seen && !Save::parse("").how_to_seen, "a new player hasn't seen it");
        assert!(Save::parse("volume=3\nnights=4\n").how_to_seen, "an old save that has played");
        assert!(!Save::parse("volume=3\nnights=0\n").how_to_seen, "an old save that never played");
        assert!(Save::parse("nights=0\nhow_to=seen\n").how_to_seen);
        assert!(!Save::parse("nights=4\nhow_to=due\n").how_to_seen);
        assert!(Save { how_to_seen: true, ..Save::default() }.without_progress().how_to_seen, "a reset keeps it");
    }

    #[test]
    fn damaged_files_fall_back_per_field() {
        let save = Save::parse("volume=99\nbrightness_top=0\nbest=oops\njunk\nbrightness_bottom=8");
        assert_eq!(save, Save { volume: LEVELS, brightness: [1, 8], ..Save::default() });
    }
}
