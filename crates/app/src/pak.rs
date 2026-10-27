//! Finds `game_data.pak`, hands its data to the game, and gathers the sound files.

use std::path::PathBuf;

use last_harvest_game::data::{self, Data};
use last_harvest_pack::{self as pack, Pak};

use crate::samples::{self, Files};

const PAK: &str = "game_data.pak";

/// `LAST_HARVEST_PAK`, else `game_data.pak` beside the binary.
fn find() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("LAST_HARVEST_PAK") {
        return Some(path.into());
    }
    let beside = std::env::current_exe().ok()?.parent()?.join(PAK);
    beside.is_file().then_some(beside)
}

/// Installs the pack's data in the game, which must happen before the game starts, and returns the sound files:
/// a sounds folder named by `LAST_HARVEST_SOUNDS`, else the pack's, else a sounds folder found nearby.
pub fn load() -> Result<Files, String> {
    let mut sounds = Files::new();
    match find() {
        Some(path) => {
            let fail = |e: String| format!("{}: {e}", path.display());
            let mut pak = Pak::open(&path).map_err(fail)?;
            let powerups = pak.take(pack::POWERUPS).map(String::from_utf8).transpose().map_err(|_| fail("powerups.txt is not UTF-8".into()))?;
            data::install(Data { atlas: pak.take(pack::ATLAS), font: pak.take(pack::FONT), powerups }).map_err(fail)?;
            sounds = pak.take_dir(pack::SOUNDS);
            eprintln!("data: {} with {} sounds", path.display(), sounds.len());
        }
        None if !cfg!(feature = "embed") => return Err(format!("no {PAK} beside the binary")),
        None => eprintln!("data: built in"),
    }
    if (sounds.is_empty() || std::env::var_os("LAST_HARVEST_SOUNDS").is_some())
        && let Some(dir) = samples::folder()
    {
        sounds = samples::read_folder(&dir);
        eprintln!("audio: sounds from {}", dir.display());
    }
    Ok(sounds)
}
