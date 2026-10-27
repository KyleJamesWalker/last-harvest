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

/// Reads the cards among `files`, by name, logging any that aren't.
fn read_cards(files: Vec<(String, Vec<u8>)>) -> Vec<Card> {
    files.into_iter().filter(|(name, _)| name.ends_with(card::EXTENSION)).filter_map(|(name, bytes)| card::read(&name, &bytes).inspect_err(|e| eprintln!("enemies: {e}")).ok()).collect()
}

/// Installs the pack's data and the enemy cards in the game, which must happen before the game starts, and returns
/// the sound files: a sounds folder named by `LAST_HARVEST_SOUNDS`, else the pack's, else a sounds folder found nearby.
pub fn load() -> Result<Files, String> {
    let mut sounds = Files::new();
    let mut data = Data::default();
    match find() {
        Some(path) => {
            let fail = |e: String| format!("{}: {e}", path.display());
            let mut pak = Pak::open(&path).map_err(fail)?;
            let mut text = |file: &str| pak.take(file).map(String::from_utf8).transpose().map_err(|_| fail(format!("{file} is not UTF-8")));
            let (powerups, enemies) = (text(pack::POWERUPS)?, text(pack::ENEMIES)?);
            data = Data { atlas: pak.take(pack::ATLAS), font: pak.take(pack::FONT), powerups, enemies, cards: read_cards(pak.take_dir(pack::ENEMY_CARDS)) };
            sounds = pak.take_dir(pack::SOUNDS);
            eprintln!("data: {} with {} sounds", path.display(), sounds.len());
        }
        None if !cfg!(feature = "embed") => return Err(format!("no {PAK} beside the binary")),
        None => eprintln!("data: built in"),
    }
    if let Some(dir) = cards_folder() {
        data.cards.extend(read_cards(samples::read_files(&dir, &[card::EXTENSION])));
    }
    let cards: Vec<String> = data.cards.iter().map(|c| c.name.clone()).collect();
    for note in data::install(data)? {
        eprintln!("enemies: {note}");
    }
    if !cards.is_empty() {
        eprintln!("enemies: cards {cards:?}");
    }
    if (sounds.is_empty() || std::env::var_os("LAST_HARVEST_SOUNDS").is_some())
        && let Some(dir) = samples::folder()
    {
        sounds = samples::read_folder(&dir);
        eprintln!("audio: sounds from {}", dir.display());
    }
    Ok(sounds)
}
