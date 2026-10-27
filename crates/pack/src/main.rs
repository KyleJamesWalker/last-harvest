//! Builds `game_data.pak`: the data the game loads, shipped beside the binary.
//! Usage: `cargo run -p last-harvest-pack [-- <out.pak>]`, writing `target/game_data.pak` by default.

use std::path::{Path, PathBuf};

use last_harvest_game::art;
use last_harvest_pack::{ATLAS, ENEMIES, FONT, MANIFEST, POWERUPS, SOUNDS, manifest, write};

/// Every file in the base pack, by its path inside the pack, sorted so the same inputs give the same bytes.
fn files(root: &Path) -> std::io::Result<Vec<(String, Vec<u8>)>> {
    let mut files = vec![
        (MANIFEST.to_string(), manifest().into_bytes()),
        (ATLAS.to_string(), art::pixels().to_vec()),
        (FONT.to_string(), std::fs::read(root.join("assets").join(FONT))?),
        (POWERUPS.to_string(), std::fs::read(root.join("crates/game").join(POWERUPS))?),
        (ENEMIES.to_string(), std::fs::read(root.join("crates/game").join(ENEMIES))?),
    ];
    for entry in std::fs::read_dir(root.join("assets/private/sounds"))? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
        if name.ends_with(".ogg") || name.ends_with(".wav") {
            files.push((format!("{SOUNDS}{name}"), std::fs::read(&path)?));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
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
