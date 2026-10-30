//! Data read at startup: from a data pack the app installs, else the copies built in with the `embed` feature.

use std::sync::{Mutex, OnceLock};

use crate::art::{ATLAS_H, ATLAS_W};

/// Files a data pack supplies; any it leaves out fall back to the built-in copy.
#[derive(Default)]
pub struct Data {
    pub atlas: Option<Vec<u8>>,
    pub font: Option<Vec<u8>>,
    pub powerups: Option<String>,
    pub enemies: Option<String>,
    /// `props.txt`: the game's own props a map can place, and how they stand in the way.
    pub props_table: Option<String>,
    /// A pack's `tuning.txt`, setting over the game's own numbers.
    pub tuning: Option<String>,
    /// `effects.txt`: what each effect, swing and animal plays.
    pub effects: Option<String>,
    pub cards: Vec<Card>,
    /// Player cards, the same sheet as an enemy's with three walking frames, and an id and name for text.
    pub players: Vec<Card>,
    /// Players' own props for maps, a sheet of frames one row high with an id and how they stand for text.
    pub props: Vec<Card>,
    /// Hand-made maps, as found; install keeps the ones that read.
    pub maps: Vec<MapFile>,
}

/// A hand-made map's file name and text, as found.
pub struct MapFile {
    pub file: String,
    pub text: String,
}

/// A map that reads: its id from the file name, the name it gives itself, and its text, read again each run so
/// its scatter differs.
pub struct LoadedMap {
    pub id: String,
    pub name: String,
    pub text: String,
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
static MAPS: OnceLock<Vec<LoadedMap>> = OnceLock::new();

/// What loaded beside the game's own data, and what couldn't, for players who can't read the log.
#[derive(Default)]
pub struct LoadReport {
    pub cards: usize,
    pub mods: Vec<String>,
    /// A line for each card or mod left out, and why.
    pub problems: Vec<String>,
    /// The game's release, and the one its `game_data.pak` was built for.
    pub version: String,
    pub pack_version: Option<String>,
    /// Where to put which `game_data.pak`, when there is none.
    pub missing_pack: Option<String>,
}

impl LoadReport {
    /// The game's release, and the pack's when it differs.
    pub fn versions(&self) -> String {
        match &self.pack_version {
            Some(p) if *p != self.version => format!("{}, data {p}", self.version),
            _ => self.version.clone(),
        }
    }
}

static REPORT: OnceLock<LoadReport> = OnceLock::new();

/// Keeps the app's account of what it loaded; the first one stays.
pub fn report(r: LoadReport) {
    REPORT.set(r).ok();
}

pub fn load_report() -> &'static LoadReport {
    REPORT.get_or_init(LoadReport::default)
}

/// Problems found after the data loaded, such as a mod's sound that won't play.
static LATER: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Adds a problem found after the report was made.
pub fn note(problem: String) {
    LATER.lock().unwrap_or_else(|e| e.into_inner()).push(problem);
}

/// How many things couldn't load, and the first of them, if any.
pub fn problems() -> Option<(usize, String)> {
    let later = LATER.lock().unwrap_or_else(|e| e.into_inner());
    let mut all = load_report().problems.iter().chain(later.iter());
    let first = all.next()?.clone();
    Some((all.count() + 1, first))
}

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
    pub const PROPS: Option<&str> = Some(include_str!("../props.txt"));
    pub const EFFECTS: Option<&str> = Some(include_str!("../effects.txt"));
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
    let props = std::mem::take(&mut data.props);
    if !cards.is_empty() || !players.is_empty() || !props.is_empty() {
        match data.atlas.take().or(built_in::ATLAS.map(<[u8]>::to_vec)) {
            Some(mut atlas) => {
                let base = data.enemies.as_deref().or(built_in::ENEMIES).unwrap_or_else(|| missing("enemies.txt"));
                notes = crate::harvest::install_cards(&cards, &players, &props, base, &mut atlas);
                data.atlas = Some(atlas);
            }
            None => notes = cards.iter().chain(&players).chain(&props).map(|c| format!("{}: no sprite atlas to draw it into", c.name)).collect(),
        }
    }
    let mut maps: Vec<LoadedMap> = Vec::new();
    for file in std::mem::take(&mut data.maps) {
        match crate::harvest::check_map(&file.file, file.text) {
            Ok(map) => {
                maps.retain(|m| m.id != map.id);
                maps.push(map);
            }
            Err(e) => {
                eprintln!("left out: {e}");
                note(e);
            }
        }
    }
    DATA.set(data).map_err(|_| "the game data was already read".to_string())?;
    MAPS.set(maps).ok();
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

/// The font built into the game, for when a pack's won't do.
pub fn built_in_font() -> &'static [u8] {
    built_in::FONT.unwrap_or_else(|| missing("font"))
}

/// The hand-made maps that read, in the order found; a later one replaces an earlier with the same id.
pub fn maps() -> &'static [LoadedMap] {
    MAPS.get().map_or(&[], Vec::as_slice)
}

pub fn powerups() -> &'static str {
    data().powerups.as_deref().or(built_in::POWERUPS).unwrap_or_else(|| missing("powerups.txt"))
}

/// The game's own props, as `props.txt` lists them.
pub fn props() -> &'static str {
    data().props_table.as_deref().or(built_in::PROPS).unwrap_or_else(|| missing("props.txt"))
}

/// What each effect, swing and animal plays, as `effects.txt` says.
pub fn effects() -> &'static str {
    data().effects.as_deref().or(built_in::EFFECTS).unwrap_or_else(|| missing("effects.txt"))
}

/// A pack's `tuning.txt`, if it has one; the game's own numbers are built in.
pub fn tuning() -> Option<&'static str> {
    data().tuning.as_deref()
}

pub fn enemies() -> &'static str {
    data().enemies.as_deref().or(built_in::ENEMIES).unwrap_or_else(|| missing("enemies.txt"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_name_the_pack_only_when_it_differs() {
        let report = |pack: Option<&str>| LoadReport { version: "v0.2.0".into(), pack_version: pack.map(String::from), ..Default::default() };
        assert_eq!(report(Some("v0.2.0")).versions(), "v0.2.0");
        assert_eq!(report(None).versions(), "v0.2.0");
        assert_eq!(report(Some("v0.1.0")).versions(), "v0.2.0, data v0.1.0");
    }
}
