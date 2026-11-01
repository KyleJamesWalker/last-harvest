//! Cards: a PNG of a figure's frames, five rows down, with its text in a compressed `iTXt` chunk, so one image file
//! shares the art and the numbers together. The rows walk front, side and back, then flash and hurt with one frame
//! per facing; a `last-harvest-rows` chunk of 6 adds a dead row. An enemy card's text is its `enemies.txt` section, whose `walk` sets how many frames its walking rows
//! have (3 when left out), and the sheet is as wide as its longest row; a player card's is its id, name and `about`,
//! and it always walks three frames.

use last_harvest_game::data::Card;
use last_harvest_game::{CARD_MAX_FRAME as MAX_FRAME, MAX_WALK};

pub const KEYWORD: &str = "last-harvest-enemy";
/// The biggest sheet a card can have: eight walking frames across and six rows down, of 64-pixel frames. Checked
/// before decoding, since a card may come from anyone.
const MAX_SHEET: [u32; 2] = [MAX_WALK as u32 * MAX_FRAME, 6 * MAX_FRAME];
const ROWS_KEYWORD: &str = "last-harvest-rows";
/// What an enemy card's file name ends in: still a PNG, so it previews anywhere, but named as a card.
pub const EXTENSION: &str = ".enemy.png";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Enemy,
    Player,
}

impl Kind {
    pub fn keyword(self) -> &'static str {
        match self {
            Kind::Enemy => KEYWORD,
            Kind::Player => "last-harvest-player",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Kind::Enemy => EXTENSION,
            Kind::Player => ".player.png",
        }
    }

    /// The kind a file name says a card is.
    pub fn of(name: &str) -> Option<Kind> {
        [Kind::Enemy, Kind::Player].into_iter().find(|k| name.ends_with(k.extension()))
    }

    fn noun(self) -> &'static str {
        match self {
            Kind::Enemy => "an enemy card",
            Kind::Player => "a player card",
        }
    }
}

/// Reads an enemy card; `name` is for messages.
pub fn read(name: &str, bytes: &[u8]) -> Result<Card, String> {
    read_as(Kind::Enemy, name, bytes)
}

/// A card-like PNG as read: its text, its size, its RGBA pixels, and its rows chunk if it has one.
pub(crate) struct Decoded {
    pub text: String,
    pub w: u32,
    pub h: u32,
    pub sheet: Vec<u8>,
    pub rows: Option<String>,
}

/// Reads a PNG no bigger than `max` with its text under `keyword`; `noun` and `name` are for messages. The size is
/// checked before decoding, since the file may come from anyone.
pub(crate) fn decode(keyword: &str, noun: &str, name: &str, bytes: &[u8], max: [u32; 2]) -> Result<Decoded, String> {
    let fail = |e: &dyn std::fmt::Display| format!("{name}: {e}");
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16 | png::Transformations::ALPHA);
    let mut reader = decoder.read_info().map_err(|e| fail(&e))?;
    let (w, h) = (reader.info().width, reader.info().height);
    if w > max[0] || h > max[1] {
        return Err(fail(&format!("{w}x{h} is bigger than {noun} can be, {}x{}", max[0], max[1])));
    }
    let mut buf = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf).map_err(|e| fail(&e))?;
    buf.truncate(frame.buffer_size());
    let sheet = match frame.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::GrayscaleAlpha => buf.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        other => return Err(fail(&format!("unexpected color type {other:?}"))),
    };
    reader.finish().map_err(|e| fail(&e))?;
    let text = reader.info().utf8_text.iter().find(|t| t.keyword == keyword).ok_or_else(|| fail(&format!("no {keyword} text, so not {noun}")))?;
    let mut text = text.clone();
    text.decompress_text().map_err(|e| fail(&e))?;
    let text = text.get_text().map_err(|e| fail(&e))?;
    let rows = reader.info().uncompressed_latin1_text.iter().find(|t| t.keyword == ROWS_KEYWORD).map(|t| t.text.clone());
    Ok(Decoded { text, w: frame.width, h: frame.height, sheet, rows })
}

/// Writes a PNG of an RGBA sheet `w` by `h` with `text` under `keyword`, and a rows chunk when given.
pub(crate) fn encode(keyword: &str, text: &str, w: u32, h: u32, rows: Option<&str>, sheet: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    let mut chunk = png::text_metadata::ITXtChunk::new(keyword, text);
    chunk.compress_text().map_err(|e| e.to_string())?;
    writer.write_text_chunk(&chunk).map_err(|e| e.to_string())?;
    if let Some(rows) = rows {
        writer.write_text_chunk(&png::text_metadata::TEXtChunk::new(ROWS_KEYWORD, rows)).map_err(|e| e.to_string())?;
    }
    writer.write_image_data(sheet).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

/// Reads a card of `kind`; `name` is for messages.
pub fn read_as(kind: Kind, name: &str, bytes: &[u8]) -> Result<Card, String> {
    let fail = |e: &dyn std::fmt::Display| format!("{name}: {e}");
    let Decoded { text, w, h, sheet, rows } = decode(kind.keyword(), kind.noun(), name, bytes, MAX_SHEET)?;
    let rows = match rows.as_deref() {
        None => 5,
        Some("6") => 6,
        Some(t) => return Err(fail(&format!("{ROWS_KEYWORD} is {t:?}, but a card has 5 or 6 rows"))),
    };
    let cols = match kind {
        Kind::Enemy => last_harvest_game::card_fields(&text).map_or(3, |f| f.walk).max(3) as u32,
        Kind::Player => 3,
    };
    if w % cols != 0 || h % rows != 0 {
        return Err(fail(&format!("{w}x{h} doesn't split into {cols} frames across and {rows} down")));
    }
    let frame = [w / cols, h / rows];
    if frame.iter().any(|&side| side > MAX_FRAME) {
        return Err(fail(&format!("its frames are {}x{}, but the game draws them at most {MAX_FRAME} pixels a side", frame[0], frame[1])));
    }
    Ok(Card { name: name.to_string(), text, frame, sheet })
}

/// Writes an enemy card from its text and a five-row RGBA sheet `w` by `h` pixels.
pub fn write(text: &str, w: u32, h: u32, sheet: &[u8]) -> Result<Vec<u8>, String> {
    write_as(Kind::Enemy, text, w, h, 5, sheet)
}

/// Writes a card of `kind` from its text and an RGBA sheet `w` by `h` pixels, `rows` (5, or 6 with a dead row) down.
pub fn write_as(kind: Kind, text: &str, w: u32, h: u32, rows: u32, sheet: &[u8]) -> Result<Vec<u8>, String> {
    encode(kind.keyword(), text, w, h, (rows == 6).then_some("6"), sheet)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_card_round_trips_its_text_and_frames() {
        let (w, h) = (3 * 12, 5 * 20);
        let sheet: Vec<u8> = (0..w * h).flat_map(|i| [(i % 256) as u8, 7, 9, if i % 3 == 0 { 0 } else { 255 }]).collect();
        let text = "[ghoul]\nname Ghoul — fast\nabout Quick, and it never stops.\nhp 40\n";
        let card = read("ghoul.enemy.png", &write(text, w, h, &sheet).unwrap()).unwrap();
        assert_eq!(card.text, text);
        assert_eq!(last_harvest_game::card_fields(&card.text).unwrap().about, "Quick, and it never stops.");
        assert_eq!(card.frame, [12, 20]);
        assert_eq!(card.sheet, sheet);
    }

    #[test]
    fn a_player_card_keeps_its_own_keyword() {
        let sheet = vec![9u8; 3 * 8 * 5 * 8 * 4];
        let bytes = write_as(Kind::Player, "[jo]\nname Jo\nabout Up early.", 24, 40, 5, &sheet).unwrap();
        let card = read_as(Kind::Player, "jo.player.png", &bytes).unwrap();
        assert_eq!((card.text.as_str(), card.frame), ("[jo]\nname Jo\nabout Up early.", [8, 8]));
        assert!(read("jo.player.png", &bytes).err().unwrap().contains("not an enemy card"), "an enemy card is a different kind");
        assert_eq!(Kind::of("jo.player.png"), Some(Kind::Player));
        assert_eq!(Kind::of("ghoul.enemy.png"), Some(Kind::Enemy));
        assert_eq!(Kind::of("ghoul.png"), None);
    }

    #[test]
    fn a_rows_chunk_adds_a_dead_row() {
        let sheet = vec![9u8; 3 * 8 * 6 * 8 * 4];
        let bytes = write_as(Kind::Enemy, "[a]", 24, 48, 6, &sheet).unwrap();
        assert_eq!(read("a.enemy.png", &bytes).unwrap().frame, [8, 8]);
        let five = write("[a]", 24, 48, &sheet).unwrap();
        assert!(read("b.enemy.png", &five).is_err(), "48 tall with no chunk is five rows, which doesn't split");
    }

    #[test]
    fn a_sheet_too_big_for_a_card_is_refused_before_decoding() {
        let mut big = Vec::new();
        let mut encoder = png::Encoder::new(&mut big, 4096, 4);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&vec![0; 4096 * 4 * 4]).unwrap();
        assert!(read("big.png", &big).err().unwrap().contains("bigger than an enemy card"));
    }

    #[test]
    fn plain_images_and_odd_sizes_are_not_cards() {
        let mut plain = Vec::new();
        let mut encoder = png::Encoder::new(&mut plain, 3, 5);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&[0; 60]).unwrap();
        assert!(read("x.png", &plain).err().unwrap().contains("not an enemy card"));
        let odd = write("[a]", 4, 5, &[0; 80]).unwrap();
        assert!(read("y.png", &odd).err().unwrap().contains("3 frames across"));
        let long = write("[a]\nwalk 5", 5 * 4, 5 * 4, &[0; 5 * 4 * 5 * 4 * 4]).unwrap();
        assert_eq!(read("z.png", &long).ok().map(|c| c.frame), Some([4, 4]), "a five-frame walk makes the sheet five across");
        let wide = write("[a]", 3 * 100, 5 * 10, &vec![0; 3 * 100 * 5 * 10 * 4]).unwrap();
        assert!(read("w.png", &wide).err().unwrap().contains("at most 64"), "three 100-pixel frames fit the sheet but not the game");
    }
}
