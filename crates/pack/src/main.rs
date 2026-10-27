//! Builds `game_data.pak`: a ZIP of the data the game loads, shipped beside the binary.
//! Usage: `cargo run -p last-harvest-pack [-- <out.pak>]`, writing `target/game_data.pak` by default.

use std::io::{Seek, Write};
use std::path::{Path, PathBuf};

use last_harvest_game::art;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

const FORMAT: u32 = 1;

/// Files already compressed, or read in place, that deflate would only slow down.
fn stored(path: &str) -> bool {
    [".rgba", ".ogg", ".png", ".wav"].iter().any(|ext| path.ends_with(ext))
}

fn manifest() -> String {
    format!(
        "id = \"game_data\"\nversion = \"{}\"\nformat = {FORMAT}\n\n[atlas]\nwidth = {}\nheight = {}\nhash = \"{:016x}\"\n",
        env!("CARGO_PKG_VERSION"),
        art::ATLAS_W,
        art::ATLAS_H,
        art::ART_HASH,
    )
}

/// Every file in the base pack, by its path inside the pack, sorted so the same inputs give the same bytes.
fn files(root: &Path) -> std::io::Result<Vec<(String, Vec<u8>)>> {
    let mut files = vec![
        ("manifest.toml".to_string(), manifest().into_bytes()),
        ("art/atlas.rgba".to_string(), art::PIXELS.to_vec()),
        ("fonts/AtkinsonHyperlegible-Regular.ttf".to_string(), std::fs::read(root.join("assets/fonts/AtkinsonHyperlegible-Regular.ttf"))?),
        ("powerups.txt".to_string(), std::fs::read(root.join("crates/game/powerups.txt"))?),
    ];
    for entry in std::fs::read_dir(root.join("assets/private/sounds"))? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
        if name.ends_with(".ogg") || name.ends_with(".wav") {
            files.push((format!("sounds/{name}"), std::fs::read(&path)?));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

fn write(out: impl Write + Seek, files: &[(String, Vec<u8>)]) -> zip::result::ZipResult<()> {
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap();
    let out = std::env::args_os().nth(1).map(PathBuf::from).unwrap_or_else(|| root.join("target/game_data.pak"));
    let files = files(root)?;
    write(std::fs::File::create(&out)?, &files)?;
    let size = std::fs::metadata(&out)?.len();
    println!("{}: {} files, {:.1} MB", out.display(), files.len(), size as f64 / 1e6);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read};

    #[test]
    fn media_is_stored_and_text_is_deflated() {
        let files = vec![
            ("manifest.toml".to_string(), manifest().into_bytes()),
            ("art/atlas.rgba".to_string(), vec![0; 64]),
            ("sounds/shot.ogg".to_string(), vec![1; 64]),
        ];
        let mut buf = Cursor::new(Vec::new());
        write(&mut buf, &files).unwrap();
        let mut zip = zip::ZipArchive::new(buf).unwrap();
        assert_eq!(zip.by_name("art/atlas.rgba").unwrap().compression(), CompressionMethod::Stored);
        assert_eq!(zip.by_name("sounds/shot.ogg").unwrap().compression(), CompressionMethod::Stored);
        let mut text = String::new();
        let mut entry = zip.by_name("manifest.toml").unwrap();
        assert_eq!(entry.compression(), CompressionMethod::Deflated);
        entry.read_to_string(&mut text).unwrap();
        assert!(text.contains(&format!("hash = \"{:016x}\"", art::ART_HASH)));
    }

    #[test]
    fn the_same_files_give_the_same_bytes() {
        let files = vec![("powerups.txt".to_string(), b"grenades 1".to_vec())];
        let pack = || {
            let mut buf = Cursor::new(Vec::new());
            write(&mut buf, &files).unwrap();
            buf.into_inner()
        };
        assert_eq!(pack(), pack());
    }
}
