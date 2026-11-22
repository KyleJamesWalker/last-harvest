//! Replay files: gzip around the game's own format, the lines that head each one, and the last few kept in a folder.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;

/// Replays a folder keeps; the oldest go first.
pub const KEEP: usize = 10;
pub const EXTENSION: &str = "lhr";

/// The lines heading each replay: this build, the data it read and the device it ran on.
pub fn info() -> Vec<(String, String)> {
    let report = last_harvest_game::data::load_report();
    let data = last_harvest_game::data::fingerprint().map_or_else(|| "built-in".to_string(), |d| format!("{d:016x}"));
    vec![
        ("game".into(), crate::VERSION.into()),
        ("data".into(), data),
        ("pack".into(), report.pack_version.clone().unwrap_or_else(|| "none".into())),
        ("mods".into(), report.mods.join(",")),
        ("device".into(), format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)),
    ]
}

pub fn pack(raw: &[u8]) -> Vec<u8> {
    let mut gz = GzEncoder::new(Vec::new(), Compression::best());
    gz.write_all(raw).expect("writing to memory");
    gz.finish().expect("writing to memory")
}

/// A replay's own bytes, from a file gzipped or not.
pub fn unpack(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if !bytes.starts_with(&[0x1f, 0x8b]) {
        return Ok(bytes.to_vec());
    }
    let mut raw = Vec::new();
    GzDecoder::new(bytes).read_to_end(&mut raw).map_err(|e| format!("replay: {e}"))?;
    Ok(raw)
}

/// Writes `raw` into `dir` as `night-<unix seconds>-<n>.lhr`, then drops all but the newest `KEEP`.
pub fn keep(dir: &Path, raw: &[u8], unix: u64) -> std::io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = (0..100).map(|n| dir.join(format!("night-{unix:010}-{n:02}.{EXTENSION}"))).find(|p| !p.exists()).unwrap_or_else(|| dir.join(format!("night-{unix:010}-99.{EXTENSION}")));
    let tmp = path.with_extension("tmp");
    let mut f = fs::File::create(&tmp)?;
    f.write_all(&pack(raw))?;
    f.sync_all()?;
    fs::rename(&tmp, &path)?;
    let mut all: Vec<PathBuf> = fs::read_dir(dir)?.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| e == EXTENSION)).collect();
    all.sort();
    for old in all.iter().take(all.len().saturating_sub(KEEP)) {
        fs::remove_file(old)?;
    }
    Ok(path)
}

/// Each replay in `dir`, newest first, as its file name and its own bytes; one that won't read is left out.
pub fn saved(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut all: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| e == EXTENSION)).collect();
    all.sort();
    all.into_iter().rev().filter_map(|p| Some((p.file_name()?.to_string_lossy().into_owned(), unpack(&fs::read(&p).ok()?).ok()?))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_packed_replay_unpacks_and_a_plain_one_passes_through() {
        let raw = b"LHRP and then some".repeat(50);
        let packed = pack(&raw);
        assert!(packed.len() < raw.len());
        assert_eq!(unpack(&packed).unwrap(), raw);
        assert_eq!(unpack(&raw).unwrap(), raw);
    }

    #[test]
    fn the_folder_keeps_only_the_newest_ten() {
        let dir = std::env::temp_dir().join(format!("lh-replays-{}", std::process::id()));
        fs::remove_dir_all(&dir).ok();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("save.txt"), "not a replay").unwrap();
        let paths: Vec<PathBuf> = (0..13).map(|i| keep(&dir, &[i as u8; 8], 1_700_000_000 + i / 2).unwrap()).collect();
        let mut left: Vec<PathBuf> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|e| e == EXTENSION)).collect();
        left.sort();
        assert_eq!(left, paths[3..], "the three oldest went");
        assert_eq!(unpack(&fs::read(&paths[12]).unwrap()).unwrap(), [12; 8]);
        assert!(dir.join("save.txt").exists(), "nothing else is touched");
        let listed = saved(&dir);
        assert_eq!(listed.len(), KEEP);
        assert_eq!((listed[0].0.as_str(), listed[0].1.as_slice()), (paths[12].file_name().unwrap().to_str().unwrap(), [12u8; 8].as_slice()), "newest first, unpacked");
        assert!(saved(&dir.join("none")).is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }
}
