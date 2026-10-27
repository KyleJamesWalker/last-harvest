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
    pub cards: Vec<Card>,
    /// Player cards, the same sheet as an enemy's with three walking frames, and an id and name for text.
    pub players: Vec<Card>,
}

/// An enemy card: one `enemies.txt` section and a sheet of `frame`-sized cells, three across and five down.
/// The rows walk front, side and back, then flash and hurt with one frame per facing.
pub struct Card {
    /// Where it came from, for messages.
    pub name: String,
    pub text: String,
    pub frame: [u32; 2],
    /// RGBA, `frame[0] * 3` wide and `frame[1] * 5` tall.
    pub sheet: Vec<u8>,
}

static DATA: OnceLock<Data> = OnceLock::new();

/// The font and the tables are the repository's own, so every build carries them; only the tileset's atlas
/// waits on the `embed` feature.
mod built_in {
    #[cfg(feature = "embed")]
    pub const ATLAS: Option<&[u8]> = Some(include_bytes!(concat!(env!("OUT_DIR"), "/art.rgba")));
    #[cfg(not(feature = "embed"))]
    pub const ATLAS: Option<&[u8]> = None;
    pub const FONT: Option<&[u8]> = Some(include_bytes!("../../../assets/fonts/AtkinsonHyperlegible-Regular.ttf"));
    pub const POWERUPS: Option<&str> = Some(include_str!("../powerups.txt"));
    pub const ENEMIES: Option<&str> = Some(include_str!("../enemies.txt"));
}

/// Installs a pack's data and draws its cards' sprites into the atlas. It must come before anything reads the
/// data, which then can't change. Returns a line for each card left out, and why.
pub fn install(mut data: Data) -> Result<Vec<String>, String> {
    if let Some(atlas) = &data.atlas
        && atlas.len() != (ATLAS_W * ATLAS_H * 4) as usize
    {
        return Err(format!("the atlas is {} bytes, not {ATLAS_W}x{ATLAS_H} RGBA", atlas.len()));
    }
    if DATA.get().is_some() {
        return Err("the game data was already read".into());
    }
    let mut notes = Vec::new();
    let cards = std::mem::take(&mut data.cards);
    let players = std::mem::take(&mut data.players);
    if !cards.is_empty() || !players.is_empty() {
        match data.atlas.take().or(built_in::ATLAS.map(<[u8]>::to_vec)) {
            Some(mut atlas) => {
                let base = data.enemies.as_deref().or(built_in::ENEMIES).unwrap_or_else(|| missing("enemies.txt"));
                notes = crate::harvest::install_cards(&cards, &players, base, &mut atlas);
                data.atlas = Some(atlas);
            }
            None => notes = cards.iter().chain(&players).map(|c| format!("{}: no sprite atlas to draw it into", c.name)).collect(),
        }
    }
    DATA.set(data).map_err(|_| "the game data was already read".to_string())?;
    Ok(notes)
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
