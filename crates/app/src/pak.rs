//! Finds `game_data.pak` and any enemy cards, hands their data to the game, and gathers the sound files.

use std::path::PathBuf;
use std::sync::Mutex;

use last_harvest_game::art;
use last_harvest_game::data::{self, Card, Data, MapFile};
use last_harvest_pack::{self as pack, Pak, card};

use crate::log;
use crate::samples::{self, Files};

/// The game's data, beside the binary: the pack and the enemy cards. What the game writes stays out of it.
const DATA: &str = "data";
const PAK: &str = "data/game_data.pak";

/// The weapon section the range tries, laid over the powerups the game installs.
static TRIED: Mutex<Option<String>> = Mutex::new(None);

/// The `effects.txt` sections the range tries, such as its weapon's shot style, laid over the effects installed.
static TRIED_EFFECTS: Mutex<Option<String>> = Mutex::new(None);

/// Lays a weapon's `powerups.txt` section over the powerups the next install hands the game.
pub fn try_section(section: &str) {
    if let Ok(mut t) = TRIED.lock() {
        *t = Some(section.to_string());
    }
}

/// Lays what a range setup tries over the next install: its weapon's section and its `effects.txt` sections.
pub fn try_setup(setup: &last_harvest_game::RangeSetup) {
    if let Some(section) = &setup.section {
        try_section(section);
    }
    if let (Some(effects), Ok(mut t)) = (&setup.effects, TRIED_EFFECTS.lock()) {
        *t = Some(effects.clone());
    }
}

/// What `LAST_HARVEST_RANGE` asks the range to try: a file holding a setup, or the setup itself, such as `rifle:3`.
/// Its section, if it has one, is laid over the powerups the game installs.
#[cfg(not(target_arch = "wasm32"))]
pub fn range_setup() -> Option<last_harvest_game::RangeSetup> {
    let value = std::env::var("LAST_HARVEST_RANGE").ok()?;
    let text = std::fs::read_to_string(&value).unwrap_or(value);
    match last_harvest_game::RangeSetup::read(&text) {
        Ok(setup) => {
            try_setup(&setup);
            log!("range: {} at level {}", setup.weapon, setup.level);
            Some(setup)
        }
        Err(e) => {
            log!("range: LAST_HARVEST_RANGE: {e}");
            None
        }
    }
}

fn data_folder() -> Option<PathBuf> {
    Some(crate::home()?.join(DATA))
}

/// `LAST_HARVEST_PAK`, else `data/game_data.pak` beside the binary.
fn find() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("LAST_HARVEST_PAK") {
        return Some(path.into());
    }
    let pak = data_folder()?.join("game_data.pak");
    pak.is_file().then_some(pak)
}

/// `LAST_HARVEST_ENEMIES`, else `data/enemies` beside the binary.
fn cards_folder() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("LAST_HARVEST_ENEMIES") {
        return Some(dir.into());
    }
    Some(data_folder()?.join("enemies"))
}

/// `LAST_HARVEST_PLAYERS`, else `data/players` beside the binary.
fn players_folder() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("LAST_HARVEST_PLAYERS") {
        return Some(dir.into());
    }
    Some(data_folder()?.join("players"))
}

/// `LAST_HARVEST_MAPS`, else `data/maps` beside the binary.
fn maps_folder() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("LAST_HARVEST_MAPS") {
        return Some(dir.into());
    }
    Some(data_folder()?.join("maps"))
}

/// `LAST_HARVEST_PROPS`, else `data/props` beside the binary.
fn props_folder() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("LAST_HARVEST_PROPS") {
        return Some(dir.into());
    }
    Some(data_folder()?.join("props"))
}

/// The props among `files`, by name, noting any that don't read.
fn read_props(files: Vec<(String, Vec<u8>)>, problems: &mut Vec<String>) -> Vec<Card> {
    let props = files.into_iter().filter(|(name, _)| name.ends_with(pack::prop::EXTENSION));
    props.filter_map(|(name, bytes)| pack::prop::read(&name, &bytes).map_err(|e| problems.push(e)).ok()).collect()
}

/// The map files among `files`, by name, noting any that aren't text.
fn read_maps(files: Vec<(String, Vec<u8>)>, problems: &mut Vec<String>) -> Vec<MapFile> {
    let ext = last_harvest_game::MAP_EXTENSION;
    let maps = files.into_iter().filter(|(name, _)| name.ends_with(ext));
    maps.filter_map(|(file, bytes)| match String::from_utf8(bytes) {
        Ok(text) => Some(MapFile { file, text }),
        Err(_) => {
            problems.push(format!("{file}: not text"));
            None
        }
    })
    .collect()
}

/// Each enemy card's own death sound by card name, as `kill-<id>.ogg` or `.wav` and its bytes, in the order read.
type OwnSounds = Vec<(String, Option<(String, Vec<u8>)>)>;

/// Reads the cards of `kind` among `files`, by name, noting any that aren't, and adds each enemy card's own death
/// sound, or None, to `own`. A sound that won't read is noted and the card loads without it.
fn read_cards(kind: card::Kind, files: Vec<(String, Vec<u8>)>, problems: &mut Vec<String>, own: &mut OwnSounds) -> Vec<Card> {
    let mut cards = Vec::new();
    for (name, bytes) in files.into_iter().filter(|(name, _)| name.ends_with(kind.extension())) {
        match card::read_as(kind, &name, &bytes) {
            Ok(c) if kind == card::Kind::Enemy => {
                let id = last_harvest_game::card_fields(&c.text).map(|f| f.id).unwrap_or_default();
                let sound = card::sound(&bytes).map_err(|e| problems.push(format!("{name}: {e}"))).ok().flatten();
                own.push((name, sound.map(|(ext, s)| (format!("kill-{id}.{ext}"), s.to_vec()))));
                cards.push(c);
            }
            Ok(c) => cards.push(c),
            Err(e) => problems.push(e),
        }
    }
    cards
}

/// `LAST_HARVEST_MODS`, else `data/mods` beside the binary.
fn mods_folder() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("LAST_HARVEST_MODS") {
        return Some(dir.into());
    }
    Some(data_folder()?.join("mods"))
}

/// A mod pack as the game lays it over its data, or why it can't be: it doesn't read, or its atlas or tables would
/// stop the game.
pub fn read_mod(pak: Result<Pak, String>) -> Result<Pak, String> {
    let pak = pak?;
    pak.check_mod()?;
    Ok(pak)
}

/// The mod packs in the mods folder, in name order, by file name; one that `read_mod` refuses is left out and noted.
fn read_mods(problems: &mut Vec<String>) -> Vec<(String, Pak)> {
    let Some(dir) = mods_folder() else { return Vec::new() };
    let mut mods = Vec::new();
    for (name, bytes) in samples::read_files(&dir, &[".pak"]) {
        match read_mod(Pak::read(std::io::Cursor::new(bytes))) {
            Ok(p) => mods.push((name, p)),
            Err(e) => problems.push(format!("{name}: {e}")),
        }
    }
    mods
}

/// Installs the pack's data, with any mods laid over it, and the enemy cards in the game, which must happen before
/// the game starts, and returns the sound files: a sounds folder named by `LAST_HARVEST_SOUNDS`, else the pack's,
/// else a sounds folder found nearby, with the mods' sounds over either.
pub fn load() -> Result<Files, String> {
    let mut missing_pack = None;
    let mut problems = Vec::new();
    let folder = || missing(data_folder().map_or_else(|| DATA.into(), |d| d.display().to_string()));
    let base = match find().map(|path| (Pak::open(&path), path)) {
        Some((Ok(pak), path)) => {
            log!("data: {} ({})", path.display(), pak.version().unwrap_or("no version"));
            Some(pak)
        }
        // A pack that won't open is a missing one: an update that swapped the binary but not its pack leaves one.
        Some((Err(e), path)) => {
            log!("data: {}: {e}", path.display());
            problems.push(format!("game_data.pak: {e}"));
            if cfg!(feature = "embed") {
                log!("data: built in");
            } else {
                missing_pack = Some(folder());
            }
            None
        }
        None if !cfg!(feature = "embed") => {
            let message = folder();
            log!("data: no {PAK} beside the binary. {message}");
            missing_pack = Some(message);
            None
        }
        None => {
            log!("data: built in");
            None
        }
    };
    install_noting(base, missing_pack, Vec::new(), problems)
}

/// `load` with the pack already open, or none for the built-in data. A web page has no data folder beside it, so
/// it fetches the pack itself and finds no mods or cards.
pub fn install(base: Option<Pak>, missing_pack: Option<String>) -> Result<Files, String> {
    install_noting(base, missing_pack, Vec::new(), Vec::new())
}

/// `install` with `mods` laid over the mods folder's, in order, by name, and `problems` already found reading them:
/// a web page keeps its mods in the browser.
pub fn install_with(base: Pak, mods: Vec<(String, Pak)>, problems: Vec<String>) -> Result<Files, String> {
    install_noting(Some(base), None, mods, problems)
}

/// Drops each of the base pack's tables that the game would refuse, noting why, so the built-in one stands in.
fn drop_refused_tables(pak: &mut Pak, problems: &mut Vec<String>) {
    for (file, e) in pak.table_problems() {
        pak.take(file);
        problems.push(format!("game_data.pak: {e}; using the game's own {file}"));
    }
}

fn install_noting(base: Option<Pak>, missing_pack: Option<String>, more: Vec<(String, Pak)>, mut problems: Vec<String>) -> Result<Files, String> {
    let pack_version = base.as_ref().and_then(Pak::version).map(String::from);
    let had_base = base.is_some();
    let mut pak = base.unwrap_or_else(Pak::empty);
    drop_refused_tables(&mut pak, &mut problems);
    let mut mods = read_mods(&mut problems);
    mods.extend(more);
    let mod_names: Vec<String> = mods.iter().map(|(n, _)| n.clone()).collect();
    if !mods.is_empty() {
        log!("mods: {:?}", mods.iter().map(|(n, _)| n).collect::<Vec<_>>());
    }
    for (_, m) in mods {
        pak.overlay(m);
    }
    let mut text = |file: &str| pak.take(file).map(String::from_utf8).transpose().map_err(|_| format!("{file} is not UTF-8"));
    let (powerups, enemies, props_table, tuning, effects) = (text(pack::POWERUPS)?, text(pack::ENEMIES)?, text(pack::PROPS_TABLE)?, text(pack::TUNING)?, text(pack::EFFECTS)?);
    let (ground, farm) = (text(pack::GROUND)?, text(pack::FARM)?);
    let powerups = match TRIED.lock().ok().and_then(|t| t.clone()) {
        Some(section) => match last_harvest_game::powerups_with(powerups.as_deref().or(last_harvest_game::built_in_table("powerups.txt")).unwrap_or_default(), &section) {
            Ok(text) => Some(text),
            Err(e) => {
                problems.push(format!("the weapon tried: {e}"));
                powerups
            }
        },
        None => powerups,
    };
    let effects = match TRIED_EFFECTS.lock().ok().and_then(|t| t.clone()) {
        Some(sections) => match last_harvest_game::effects_with(effects.as_deref().or(last_harvest_game::built_in_table("effects.txt")).unwrap_or_default(), &sections) {
            Ok(text) => Some(text),
            Err(e) => {
                problems.push(format!("the shot tried: {e}"));
                effects
            }
        },
        None => effects,
    };
    let (enemy, player) = (card::Kind::Enemy, card::Kind::Player);
    let mut own = OwnSounds::new();
    let cards = read_cards(enemy, pak.take_dir(pack::ENEMY_CARDS), &mut problems, &mut own);
    let players = read_cards(player, pak.take_dir(pack::PLAYER_CARDS), &mut problems, &mut Vec::new());
    let maps = read_maps(pak.take_dir(pack::MAPS), &mut problems);
    let props = read_props(pak.take_dir(pack::PROPS), &mut problems);
    let mut data = Data { atlas: pak.take(pack::ATLAS), font: pak.take(pack::FONT), powerups, enemies, props_table, tuning, effects, ground, farm, cards, players, props, maps };
    if missing_pack.is_some() && data.atlas.is_none() {
        data.atlas = Some(vec![0; (art::ATLAS_W * art::ATLAS_H * 4) as usize]);
    }
    if let Some(dir) = props_folder() {
        data.props.extend(read_props(samples::read_files(&dir, &[pack::prop::EXTENSION]), &mut problems));
    }
    keep_last_of_each_name(&mut data.props);
    if let Some(dir) = maps_folder() {
        data.maps.extend(read_maps(samples::read_files(&dir, &[last_harvest_game::MAP_EXTENSION]), &mut problems));
    }
    if let Some(dir) = cards_folder() {
        data.cards.extend(read_cards(enemy, samples::read_files(&dir, &[enemy.extension()]), &mut problems, &mut own));
    }
    if let Some(dir) = players_folder() {
        data.players.extend(read_cards(player, samples::read_files(&dir, &[player.extension()]), &mut problems, &mut Vec::new()));
    }
    keep_last_of_each_name(&mut data.cards);
    keep_last_of_each_name(&mut data.players);
    let cards: Vec<String> = data.cards.iter().chain(&data.players).map(|c| c.name.clone()).collect();
    let left_out = data::install(data)?;
    if !cards.is_empty() {
        log!("cards: {cards:?}");
    }
    let loaded = cards.iter().filter(|c| !left_out.iter().any(|l| l.strip_prefix(c.as_str()).is_some_and(|r| r.starts_with(": ")))).count();
    problems.extend(left_out);
    for problem in &problems {
        log!("left out: {problem}");
    }
    data::report(data::LoadReport { cards: loaded, mods: mod_names, problems, version: crate::VERSION.into(), pack_version, missing_pack });
    let mut sounds = pak.take_dir(pack::SOUNDS);
    let forced = std::env::var_os("LAST_HARVEST_SOUNDS").is_some();
    if (forced || !had_base)
        && let Some(dir) = samples::folder()
    {
        log!("audio: sounds from {}", dir.display());
        let folder = samples::read_folder(&dir);
        sounds = if forced { folder } else { overlay_sounds(folder, sounds) };
    }
    let sounds = with_own_sounds(sounds, own);
    log!("audio: {} sound files", sounds.len());
    Ok(sounds)
}

/// Which `game_data.pak` to copy into `folder`: the one from this game's release.
fn missing(folder: String) -> String {
    match crate::VERSION {
        v if v.starts_with("dev") => format!("Copy in a game_data.pak built with this game: {folder}"),
        v => format!("Copy game_data.pak from release {v} into {folder}"),
    }
}

/// Drops each card that a later one of the same file name replaces, so a loose card stands in for a pack's.
fn keep_last_of_each_name(cards: &mut Vec<Card>) {
    let mut seen = std::collections::HashSet::new();
    cards.reverse();
    cards.retain(|c| seen.insert(c.name.clone()));
    cards.reverse();
}

/// `base` with each of `top`'s files in place of the one of the same name.
fn overlay_sounds(base: Files, top: Files) -> Files {
    let mut out: Files = base.into_iter().filter(|(n, _)| !top.iter().any(|(t, _)| t == n)).collect();
    out.extend(top);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// `sounds` with each card's own death sound, the last read under each card name, where no file has its cue.
fn with_own_sounds(mut sounds: Files, own: OwnSounds) -> Files {
    let last: std::collections::HashMap<String, Option<(String, Vec<u8>)>> = own.into_iter().collect();
    let stem = |file: &str| file.rsplit_once('.').map_or(file, |(s, _)| s).to_string();
    for (file, bytes) in last.into_values().flatten() {
        if !sounds.iter().any(|(n, _)| stem(n) == stem(&file)) {
            sounds.push((file, bytes));
        }
    }
    sounds.sort_by(|a, b| a.0.cmp(&b.0));
    sounds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_card_that_wont_read_is_noted_and_the_rest_load() {
        let good = card::write("[ghoul]\nname Ghoul", 3, 5, &[0; 3 * 5 * 4]).unwrap();
        let files = vec![("bad.enemy.png".to_string(), b"nope".to_vec()), ("ghoul.enemy.png".to_string(), good), ("notes.txt".to_string(), vec![])];
        let mut problems = Vec::new();
        let cards = read_cards(card::Kind::Enemy, files, &mut problems, &mut Vec::new());
        assert_eq!(cards.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["ghoul.enemy.png"]);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].starts_with("bad.enemy.png: "), "{problems:?}");
    }

    #[test]
    fn a_loose_card_stands_in_for_a_packs_of_the_same_name() {
        let card = |name: &str, text: &str| Card { detail: 1, worn: String::new(), name: name.into(), text: text.into(), frame: [1, 1], sheet: vec![] };
        let mut cards = vec![card("ghoul.enemy.png", "pack"), card("bat.enemy.png", "pack"), card("ghoul.enemy.png", "loose")];
        keep_last_of_each_name(&mut cards);
        assert_eq!(cards.iter().map(|c| (c.name.as_str(), c.text.as_str())).collect::<Vec<_>>(), [("bat.enemy.png", "pack"), ("ghoul.enemy.png", "loose")]);
    }

    #[test]
    fn a_cards_own_sound_fills_in_only_where_no_file_has_its_cue() {
        let (ogg, wav) = (b"OggS".to_vec(), b"RIFF\0\0\0\0WAVE".to_vec());
        let sound = |file: &str, b: &[u8]| Some((file.to_string(), b.to_vec()));
        let own = vec![
            ("bat.enemy.png".to_string(), sound("kill-bat.ogg", &ogg)),
            ("ghoul.enemy.png".to_string(), sound("kill-ghoul.wav", &wav)),
            ("old.enemy.png".to_string(), sound("kill-old.ogg", &ogg)),
            ("old.enemy.png".to_string(), None),
        ];
        let files = vec![("kill-ghoul.ogg".to_string(), b"pack".to_vec())];
        let names: Vec<String> = with_own_sounds(files, own).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["kill-bat.ogg", "kill-ghoul.ogg"], "the pack's ghoul wins, and a card replaced by one without a sound brings none");
    }

    #[test]
    fn a_mods_sounds_replace_the_folders_by_name() {
        let base = vec![("shot.ogg".to_string(), b"a".to_vec()), ("boom.ogg".to_string(), b"b".to_vec())];
        let top = vec![("shot.ogg".to_string(), b"mod".to_vec()), ("kill-ghoul.ogg".to_string(), b"new".to_vec())];
        let names: Vec<(String, Vec<u8>)> = overlay_sounds(base, top);
        assert_eq!(names, [("boom.ogg".into(), b"b".to_vec()), ("kill-ghoul.ogg".into(), b"new".to_vec()), ("shot.ogg".into(), b"mod".to_vec())]);
    }
}
