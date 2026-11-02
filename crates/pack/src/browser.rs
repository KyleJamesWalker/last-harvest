//! How the card editor keeps its work in the browser's local storage, which the web game on the same site reads
//! back as a mod: a key per card, prop, map and changed data file, binary files as Base64.

use crate::card::Kind;
use crate::{ENEMY_CARDS, MAPS, PLAYER_CARDS, PROPS, prop};

pub const ENEMY_KEY: &str = "last-harvest-card:";
pub const PLAYER_KEY: &str = "last-harvest-player:";
pub const PROP_KEY: &str = "last-harvest-prop:";
pub const MAP_KEY: &str = "last-harvest-map:";
/// A data file the mod changes, by its file name.
pub const TABLE_KEY: &str = "last-harvest-table:";

/// The storage key of a card of `kind`.
pub fn card_key(kind: Kind, id: &str) -> String {
    match kind {
        Kind::Enemy => format!("{ENEMY_KEY}{id}"),
        Kind::Player => format!("{PLAYER_KEY}{id}"),
    }
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Bytes as Base64, for a store such as the browser's local storage that keeps only text.
pub fn to_text(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, &b)| n | (b as u32) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() { ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

pub fn from_text(text: &str) -> Option<Vec<u8>> {
    let digit = |c: u8| ALPHABET.iter().position(|&a| a == c).map(|v| v as u32);
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    for chunk in text.as_bytes().chunks(4) {
        let pad = chunk.iter().filter(|&&c| c == b'=').count();
        if pad > 2 {
            return None;
        }
        let n = chunk.iter().take(4 - pad).enumerate().try_fold(0u32, |n, (i, &c)| Some(n | digit(c)? << (18 - 6 * i)))?;
        out.extend((0..3 - pad).map(|i| (n >> (16 - 8 * i)) as u8));
    }
    Some(out)
}

/// The editor's work among local storage's `entries`, as the files Export all would pack, by their paths in a pack;
/// other keys are skipped, and an entry that won't decode is noted.
pub fn editor_files(entries: impl IntoIterator<Item = (String, String)>, problems: &mut Vec<String>) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    for (key, value) in entries {
        let binary = |dir: &str, id: &str, ext: &str| (format!("{dir}{id}{ext}"), from_text(&value));
        let (path, bytes) = if let Some(id) = key.strip_prefix(ENEMY_KEY) {
            binary(ENEMY_CARDS, id, Kind::Enemy.extension())
        } else if let Some(id) = key.strip_prefix(PLAYER_KEY) {
            binary(PLAYER_CARDS, id, Kind::Player.extension())
        } else if let Some(id) = key.strip_prefix(PROP_KEY) {
            binary(PROPS, id, prop::EXTENSION)
        } else if let Some(id) = key.strip_prefix(MAP_KEY) {
            (format!("{MAPS}{id}{}", last_harvest_game::MAP_EXTENSION), Some(value.into_bytes()))
        } else if let Some(file) = key.strip_prefix(TABLE_KEY) {
            (file.to_string(), Some(value.into_bytes()))
        } else {
            continue;
        };
        match bytes {
            Some(b) => files.push((path, b)),
            None => problems.push(format!("{path}: the card editor's copy is damaged")),
        }
    }
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_round_trips() {
        for n in 0..8 {
            let bytes: Vec<u8> = (0..n).map(|i| (i * 37 + 200) as u8).collect();
            assert_eq!(from_text(&to_text(&bytes)), Some(bytes));
        }
        assert_eq!(from_text("a==="), None);
    }

    #[test]
    fn the_editors_keys_become_pack_paths() {
        let entries = [
            (card_key(Kind::Enemy, "biter"), to_text(b"png")),
            (card_key(Kind::Player, "me"), to_text(b"me")),
            (format!("{PROP_KEY}crate"), to_text(b"prop")),
            (format!("{MAP_KEY}yard"), "map".to_string()),
            (format!("{TABLE_KEY}tuning.txt"), "t".to_string()),
            ("save.txt".to_string(), "not the editor's".to_string()),
            (card_key(Kind::Enemy, "bad"), "!!!!".to_string()),
        ];
        let mut problems = Vec::new();
        let files = editor_files(entries.map(|(k, v)| (k, v)), &mut problems);
        let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
        let map = format!("maps/yard{}", last_harvest_game::MAP_EXTENSION);
        assert_eq!(paths, ["enemies/biter.enemy.png", map.as_str(), "players/me.player.png", "props/crate.prop.png", "tuning.txt"]);
        assert_eq!(files[0].1, b"png");
        assert_eq!(problems, ["enemies/bad.enemy.png: the card editor's copy is damaged"]);
    }
}
