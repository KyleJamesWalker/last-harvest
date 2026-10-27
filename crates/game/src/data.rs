//! Data read at startup: from a data pack the app installs, else the copies built in with the `embed` feature.

use std::sync::OnceLock;

use crate::art::{ATLAS_H, ATLAS_W};

/// Files a data pack supplies; any it leaves out fall back to the built-in copy.
#[derive(Default)]
pub struct Data {
    pub atlas: Option<Vec<u8>>,
    pub font: Option<Vec<u8>>,
    pub powerups: Option<String>,
    pub enemies: Option<String>,
}

static DATA: OnceLock<Data> = OnceLock::new();

#[cfg(feature = "embed")]
mod built_in {
    pub const ATLAS: Option<&[u8]> = Some(include_bytes!(concat!(env!("OUT_DIR"), "/art.rgba")));
    pub const FONT: Option<&[u8]> = Some(include_bytes!("../../../assets/fonts/AtkinsonHyperlegible-Regular.ttf"));
    pub const POWERUPS: Option<&str> = Some(include_str!("../powerups.txt"));
    pub const ENEMIES: Option<&str> = Some(include_str!("../enemies.txt"));
}

#[cfg(not(feature = "embed"))]
mod built_in {
    pub const ATLAS: Option<&[u8]> = None;
    pub const FONT: Option<&[u8]> = None;
    pub const POWERUPS: Option<&str> = None;
    pub const ENEMIES: Option<&str> = None;
}

/// Installs a pack's data. It must come before anything reads the data, which then can't change.
pub fn install(data: Data) -> Result<(), String> {
    if let Some(atlas) = &data.atlas
        && atlas.len() != (ATLAS_W * ATLAS_H * 4) as usize
    {
        return Err(format!("the atlas is {} bytes, not {ATLAS_W}x{ATLAS_H} RGBA", atlas.len()));
    }
    DATA.set(data).map_err(|_| "the game data was already read".to_string())
}

fn data() -> &'static Data {
    DATA.get_or_init(Data::default)
}

fn missing(what: &str) -> ! {
    panic!("no {what}: this build reads its data from game_data.pak beside the binary")
}

/// The sprite texture, `ATLAS_W` by `ATLAS_H` RGBA.
pub fn atlas() -> &'static [u8] {
    data().atlas.as_deref().or(built_in::ATLAS).unwrap_or_else(|| missing("sprite atlas"))
}

pub fn font() -> &'static [u8] {
    data().font.as_deref().or(built_in::FONT).unwrap_or_else(|| missing("font"))
}

pub fn powerups() -> &'static str {
    data().powerups.as_deref().or(built_in::POWERUPS).unwrap_or_else(|| missing("powerups.txt"))
}

pub fn enemies() -> &'static str {
    data().enemies.as_deref().or(built_in::ENEMIES).unwrap_or_else(|| missing("enemies.txt"))
}
