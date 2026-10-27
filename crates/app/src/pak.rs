//! Finds `game_data.pak` and any enemy cards, hands their data to the game, and gathers the sound files.

use std::path::PathBuf;

use last_harvest_game::data::{self, Card, Data};
use last_harvest_pack::{self as pack, Pak, card};

use crate::samples::{self, Files};

/// The game's data, beside the binary: the pack and the enemy cards. What the game writes stays out of it.
const DATA: &str = "data";
const PAK: &str = "data/game_data.pak";

fn data_folder() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join(DATA))
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

/// Reads the cards of `kind` among `files`, by name, noting any that aren't.
fn read_cards(kind: card::Kind, files: Vec<(String, Vec<u8>)>, problems: &mut Vec<String>) -> Vec<Card> {
    let mut cards = Vec::new();
    for (name, bytes) in files.into_iter().filter(|(name, _)| name.ends_with(kind.extension())) {
        match card::read_as(kind, &name, &bytes) {
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

/// The mod packs in the mods folder, in name order, by file name; one that can't be read, or whose atlas or tables
/// would stop the game, is left out and noted.
fn read_mods(problems: &mut Vec<String>) -> Vec<(String, Pak)> {
    let Some(dir) = mods_folder() else { return Vec::new() };
    let read = |bytes: Vec<u8>| -> Result<Pak, String> {
        let pak = Pak::read(std::io::Cursor::new(bytes))?;
        pak.check_mod()?;
        Ok(pak)
    };
    let mut mods = Vec::new();
    for (name, bytes) in samples::read_files(&dir, &[".pak"]) {
        match read(bytes) {
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
    let base = match find() {
        Some(path) => Some(Pak::open(&path).map_err(|e| format!("{}: {e}", path.display()))?).inspect(|_| eprintln!("data: {}", path.display())),
        None if !cfg!(feature = "embed") => return Err(format!("no {PAK} beside the binary")),
        None => {
            eprintln!("data: built in");
            None
        }
    };
    let had_base = base.is_some();
    let mut pak = base.unwrap_or_else(Pak::empty);
    let mut problems = Vec::new();
    let mods = read_mods(&mut problems);
    let mod_names: Vec<String> = mods.iter().map(|(n, _)| n.clone()).collect();
    if !mods.is_empty() {
        eprintln!("mods: {:?}", mods.iter().map(|(n, _)| n).collect::<Vec<_>>());
    }
    for (_, m) in mods {
        pak.overlay(m);
    }
    let mut text = |file: &str| pak.take(file).map(String::from_utf8).transpose().map_err(|_| format!("{file} is not UTF-8"));
    let (powerups, enemies) = (text(pack::POWERUPS)?, text(pack::ENEMIES)?);
    let (enemy, player) = (card::Kind::Enemy, card::Kind::Player);
    let cards = read_cards(enemy, pak.take_dir(pack::ENEMY_CARDS), &mut problems);
    let players = read_cards(player, pak.take_dir(pack::PLAYER_CARDS), &mut problems);
    let mut data = Data { atlas: pak.take(pack::ATLAS), font: pak.take(pack::FONT), powerups, enemies, cards, players };
    if let Some(dir) = cards_folder() {
        data.cards.extend(read_cards(enemy, samples::read_files(&dir, &[enemy.extension()]), &mut problems));
    }
    if let Some(dir) = players_folder() {
        data.players.extend(read_cards(player, samples::read_files(&dir, &[player.extension()]), &mut problems));
    }
    keep_last_of_each_name(&mut data.cards);
    keep_last_of_each_name(&mut data.players);
    let cards: Vec<String> = data.cards.iter().chain(&data.players).map(|c| c.name.clone()).collect();
    let left_out = data::install(data)?;
    if !cards.is_empty() {
        eprintln!("cards: {cards:?}");
    }
    let loaded = cards.len().saturating_sub(left_out.len());
    problems.extend(left_out);
    for problem in &problems {
        eprintln!("left out: {problem}");
    }
    data::report(data::LoadReport { cards: loaded, mods: mod_names, problems });
    let mut sounds = pak.take_dir(pack::SOUNDS);
    let forced = std::env::var_os("LAST_HARVEST_SOUNDS").is_some();
    if (forced || !had_base)
        && let Some(dir) = samples::folder()
    {
        eprintln!("audio: sounds from {}", dir.display());
        let folder = samples::read_folder(&dir);
        sounds = if forced { folder } else { overlay_sounds(folder, sounds) };
    }
    eprintln!("audio: {} sound files", sounds.len());
    Ok(sounds)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_card_that_wont_read_is_noted_and_the_rest_load() {
        let good = card::write("[ghoul]\nname Ghoul", 3, 5, &[0; 3 * 5 * 4]).unwrap();
        let files = vec![("bad.enemy.png".to_string(), b"nope".to_vec()), ("ghoul.enemy.png".to_string(), good), ("notes.txt".to_string(), vec![])];
        let mut problems = Vec::new();
        let cards = read_cards(card::Kind::Enemy, files, &mut problems);
        assert_eq!(cards.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["ghoul.enemy.png"]);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].starts_with("bad.enemy.png: "), "{problems:?}");
    }

    #[test]
    fn a_loose_card_stands_in_for_a_packs_of_the_same_name() {
        let card = |name: &str, text: &str| Card { name: name.into(), text: text.into(), frame: [1, 1], sheet: vec![] };
        let mut cards = vec![card("ghoul.enemy.png", "pack"), card("bat.enemy.png", "pack"), card("ghoul.enemy.png", "loose")];
        keep_last_of_each_name(&mut cards);
        assert_eq!(cards.iter().map(|c| (c.name.as_str(), c.text.as_str())).collect::<Vec<_>>(), [("bat.enemy.png", "pack"), ("ghoul.enemy.png", "loose")]);
    }

    #[test]
    fn a_mods_sounds_replace_the_folders_by_name() {
        let base = vec![("shot.ogg".to_string(), b"a".to_vec()), ("boom.ogg".to_string(), b"b".to_vec())];
        let top = vec![("shot.ogg".to_string(), b"mod".to_vec()), ("kill-ghoul.ogg".to_string(), b"new".to_vec())];
        let names: Vec<(String, Vec<u8>)> = overlay_sounds(base, top);
        assert_eq!(names, [("boom.ogg".into(), b"b".to_vec()), ("kill-ghoul.ogg".into(), b"new".to_vec()), ("shot.ogg".into(), b"mod".to_vec())]);
    }
}
