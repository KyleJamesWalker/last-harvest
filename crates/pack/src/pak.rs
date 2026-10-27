//! Reading and writing packs.

use std::collections::HashMap;
use std::io::{Read, Seek, Write};
use std::path::Path;

use last_harvest_game::art;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

pub const FORMAT: u32 = 1;
/// The most a pack's file, and a whole pack, can hold once unpacked.
const MAX_FILE: u64 = 32 << 20;
const MAX_PACK: u64 = 128 << 20;
pub const MANIFEST: &str = "manifest.toml";
pub const ATLAS: &str = "art/atlas.rgba";
pub const FONT: &str = "fonts/AtkinsonHyperlegible-Regular.ttf";
pub const POWERUPS: &str = "powerups.txt";
pub const ENEMIES: &str = "enemies.txt";
pub const SOUNDS: &str = "sounds/";
pub const ENEMY_CARDS: &str = "enemies/";
pub const PLAYER_CARDS: &str = "players/";

/// The manifest of a mod pack named `id`.
pub fn mod_manifest(id: &str) -> String {
    format!("id = \"{id}\"\nversion = \"1\"\nformat = {FORMAT}\n")
}

/// The manifest of the base pack built for this binary.
pub fn manifest() -> String {
    format!(
        "id = \"game_data\"\nversion = \"{}\"\nformat = {FORMAT}\n\n[atlas]\nwidth = {}\nheight = {}\nhash = \"{:016x}\"\n",
        env!("CARGO_PKG_VERSION"),
        art::ATLAS_W,
        art::ATLAS_H,
        art::ART_HASH,
    )
}

/// Files already compressed, or read in place, that deflate would only slow down.
fn stored(path: &str) -> bool {
    [".rgba", ".ogg", ".png", ".wav"].iter().any(|ext| path.ends_with(ext))
}

/// Writes `files` in the order given, with fixed timestamps so the same files give the same bytes.
pub fn write(out: impl Write + Seek, files: &[(String, Vec<u8>)]) -> zip::result::ZipResult<()> {
    let mut zip = ZipWriter::new(out);
    for (path, bytes) in files {
        let method = if stored(path) { CompressionMethod::Stored } else { CompressionMethod::Deflated };
        let options = SimpleFileOptions::default().compression_method(method).last_modified_time(DateTime::default()).unix_permissions(0o644);
        zip.start_file(path, options)?;
        zip.write_all(bytes)?;
    }
    zip.finish()?;
    Ok(())
}

/// A pack read into memory, its files by path.
pub struct Pak {
    files: HashMap<String, Vec<u8>>,
}

impl Pak {
    pub fn open(path: &Path) -> Result<Self, String> {
        Self::read(std::fs::File::open(path).map_err(|e| e.to_string())?)
    }

    /// Reads every file and checks the manifest: its format, and that an atlas was built for this binary's sprite layout.
    pub fn read(r: impl Read + Seek) -> Result<Self, String> {
        let mut zip = ZipArchive::new(r).map_err(|e| e.to_string())?;
        let mut files = HashMap::new();
        let mut total = 0u64;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
            if entry.is_dir() {
                continue;
            }
            total += entry.size();
            if entry.size() > MAX_FILE || total > MAX_PACK {
                return Err(format!("{} makes the pack bigger than the game reads: {} MB a file, {} MB in all", entry.name(), MAX_FILE >> 20, MAX_PACK >> 20));
            }
            let mut bytes = Vec::with_capacity(entry.size() as usize);
            // A pack may come from anyone: read no more than it declares, whatever its data holds.
            (&mut entry).take(MAX_FILE).read_to_end(&mut bytes).map_err(|e| format!("{}: {e}", entry.name()))?;
            files.insert(entry.name().to_string(), bytes);
        }
        let text = files.get(MANIFEST).ok_or("no manifest.toml")?;
        let text = std::str::from_utf8(text).map_err(|_| "manifest.toml is not UTF-8")?;
        let format = value(text, "", "format").and_then(|v| v.parse::<u32>().ok()).ok_or("manifest.toml has no format")?;
        if format != FORMAT {
            return Err(format!("format {format}, but this build reads format {FORMAT}"));
        }
        if files.contains_key(ATLAS) && value(text, "atlas", "hash") != Some(&format!("{:016x}", art::ART_HASH)) {
            return Err("its atlas was built for a different build of the game".into());
        }
        Ok(Self { files })
    }

    /// No files, for mods to lay over when there's no base pack.
    pub fn empty() -> Self {
        Self { files: HashMap::new() }
    }

    /// Lays a mod over this pack: each of its files replaces the one at the same path, all but its manifest.
    pub fn overlay(&mut self, mut top: Pak) {
        top.files.remove(MANIFEST);
        self.files.extend(top.files);
    }

    pub fn take(&mut self, path: &str) -> Option<Vec<u8>> {
        self.files.remove(path)
    }

    /// Takes every file directly under `dir`, by its name within it, sorted by name.
    pub fn take_dir(&mut self, dir: &str) -> Vec<(String, Vec<u8>)> {
        let names: Vec<String> = self.files.keys().filter(|p| p.strip_prefix(dir).is_some_and(|n| !n.is_empty() && !n.contains('/'))).cloned().collect();
        let mut taken: Vec<(String, Vec<u8>)> = names.into_iter().map(|p| (p[dir.len()..].to_string(), self.files.remove(&p).unwrap())).collect();
        taken.sort_by(|a, b| a.0.cmp(&b.0));
        taken
    }
}

/// The value of `key` under `[section]`, or at the top for "", from the flat `key = value` lines manifests use.
fn value<'a>(text: &'a str, section: &str, key: &str) -> Option<&'a str> {
    let mut current = "";
    for line in text.lines().map(|l| l.split('#').next().unwrap().trim()) {
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            current = name.trim();
        } else if current == section
            && let Some((k, v)) = line.split_once('=')
            && k.trim() == key
        {
            return Some(v.trim().trim_matches('"'));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn pack(files: &[(&str, &[u8])]) -> Vec<u8> {
        let files: Vec<(String, Vec<u8>)> = files.iter().map(|(p, b)| (p.to_string(), b.to_vec())).collect();
        let mut buf = Cursor::new(Vec::new());
        write(&mut buf, &files).unwrap();
        buf.into_inner()
    }

    #[test]
    fn media_is_stored_and_text_is_deflated() {
        let bytes = pack(&[(MANIFEST, manifest().as_bytes()), (ATLAS, &[0; 64]), ("sounds/shot.ogg", &[1; 64])]);
        let mut zip = ZipArchive::new(Cursor::new(bytes)).unwrap();
        assert_eq!(zip.by_name(ATLAS).unwrap().compression(), CompressionMethod::Stored);
        assert_eq!(zip.by_name("sounds/shot.ogg").unwrap().compression(), CompressionMethod::Stored);
        assert_eq!(zip.by_name(MANIFEST).unwrap().compression(), CompressionMethod::Deflated);
    }

    #[test]
    fn the_same_files_give_the_same_bytes() {
        let files = [(POWERUPS, b"grenades 1".as_slice())];
        assert_eq!(pack(&files), pack(&files));
    }

    #[test]
    fn reads_back_what_was_written() {
        let bytes = pack(&[(MANIFEST, manifest().as_bytes()), (ATLAS, &[7; 16]), ("sounds/shot.ogg", b"a"), ("sounds/shot-2.ogg", b"b"), ("sounds/deep/x.ogg", b"c")]);
        let mut pak = Pak::read(Cursor::new(bytes)).unwrap();
        assert_eq!(pak.take(ATLAS), Some(vec![7; 16]));
        assert_eq!(pak.take(ATLAS), None);
        let sounds = pak.take_dir(SOUNDS);
        assert_eq!(sounds, [("shot-2.ogg".to_string(), b"b".to_vec()), ("shot.ogg".to_string(), b"a".to_vec())]);
    }

    #[test]
    fn rejects_an_atlas_from_another_build() {
        let stale = manifest().replace(&format!("{:016x}", art::ART_HASH), "0000000000000000");
        let err = Pak::read(Cursor::new(pack(&[(MANIFEST, stale.as_bytes()), (ATLAS, &[0; 4])]))).err().unwrap();
        assert!(err.contains("different build"), "{err}");
        assert!(Pak::read(Cursor::new(pack(&[(MANIFEST, stale.as_bytes())]))).is_ok(), "a pack without an atlas doesn't care");
    }

    #[test]
    fn rejects_other_formats_and_missing_manifests() {
        let newer = manifest().replace(&format!("format = {FORMAT}"), "format = 99");
        assert!(Pak::read(Cursor::new(pack(&[(MANIFEST, newer.as_bytes())]))).err().unwrap().contains("format 99"));
        assert!(Pak::read(Cursor::new(pack(&[(POWERUPS, b"x")]))).is_err());
    }

    #[test]
    fn a_mod_replaces_files_at_the_same_path_and_adds_the_rest() {
        let base = pack(&[(MANIFEST, manifest().as_bytes()), ("sounds/shot.ogg", b"base"), (POWERUPS, b"base")]);
        let top = pack(&[(MANIFEST, mod_manifest("loud").as_bytes()), ("sounds/shot.ogg", b"mod"), ("sounds/kill-ghoul.ogg", b"new")]);
        let mut pak = Pak::read(Cursor::new(base)).unwrap();
        pak.overlay(Pak::read(Cursor::new(top)).unwrap());
        assert_eq!(pak.take(POWERUPS), Some(b"base".to_vec()));
        assert_eq!(pak.take_dir(SOUNDS), [("kill-ghoul.ogg".to_string(), b"new".to_vec()), ("shot.ogg".to_string(), b"mod".to_vec())]);
        assert!(pak.take(MANIFEST).unwrap().starts_with(b"id = \"game_data\""), "the base keeps its own manifest");
    }

    #[test]
    fn a_file_bigger_than_the_game_reads_turns_the_pack_away() {
        let zeros = vec![0u8; (MAX_FILE + 1) as usize];
        let bytes = pack(&[(MANIFEST, manifest().as_bytes()), (POWERUPS, &zeros)]);
        assert!(bytes.len() < 1 << 20, "it packs small");
        assert!(Pak::read(Cursor::new(bytes)).err().unwrap().contains("bigger than the game reads"));
    }

    #[test]
    fn manifest_values_come_from_their_section() {
        let text = "format = 1 # comment\n[atlas]\nhash = \"ab\"\n[other]\nhash = \"cd\"\n";
        assert_eq!(value(text, "", "format"), Some("1"));
        assert_eq!(value(text, "atlas", "hash"), Some("ab"));
        assert_eq!(value(text, "", "hash"), None);
    }
}
