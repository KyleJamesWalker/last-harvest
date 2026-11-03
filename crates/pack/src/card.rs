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
const MAX_SHEET: [u32; 2] = [MAX_WALK as u32 * MAX_FRAME * MAX_DETAIL, 6 * MAX_FRAME * MAX_DETAIL];
const MAX_DETAIL: u32 = 4;
/// How many of a card's pixels make one game pixel, when more than one. Its own chunk, so a game from before it reads
/// the card, drawn that many times bigger.
const DETAIL_KEYWORD: &str = "last-harvest-detail";
const ROWS_KEYWORD: &str = "last-harvest-rows";
/// Where a player card wears its ready grenade, as `worn::parse_card` reads it. Its own chunk, so a game from before
/// it reads the card all the same.
const WORN_KEYWORD: &str = "last-harvest-worn";
/// An enemy card's own death sound, an Ogg or WAV file whole: a private ancillary chunk, so a game from before it
/// reads the card all the same.
const SOUND_CHUNK: [u8; 4] = *b"lhSd";
/// The biggest death sound a card may carry. Checked before reading it, since a card may come from anyone.
pub const MAX_SOUND: usize = 512 * 1024;
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
    pub worn: Option<String>,
    pub detail: Option<String>,
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
    let latin1 = |key: &str| reader.info().uncompressed_latin1_text.iter().find(|t| t.keyword == key).map(|t| t.text.clone());
    let (rows, worn, detail) = (latin1(ROWS_KEYWORD), latin1(WORN_KEYWORD), latin1(DETAIL_KEYWORD));
    Ok(Decoded { text, w: frame.width, h: frame.height, sheet, rows, worn, detail })
}

/// Writes a PNG of an RGBA sheet `w` by `h` with `text` under `keyword`, and rows, worn and detail chunks when given.
#[allow(clippy::too_many_arguments)]
pub(crate) fn encode(keyword: &str, text: &str, w: u32, h: u32, rows: Option<&str>, worn: &str, detail: u8, sheet: &[u8]) -> Result<Vec<u8>, String> {
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
    if !worn.is_empty() {
        writer.write_text_chunk(&png::text_metadata::TEXtChunk::new(WORN_KEYWORD, worn)).map_err(|e| e.to_string())?;
    }
    if detail > 1 {
        writer.write_text_chunk(&png::text_metadata::TEXtChunk::new(DETAIL_KEYWORD, detail.to_string())).map_err(|e| e.to_string())?;
    }
    writer.write_image_data(sheet).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

/// Reads a card of `kind`; `name` is for messages.
pub fn read_as(kind: Kind, name: &str, bytes: &[u8]) -> Result<Card, String> {
    let fail = |e: &dyn std::fmt::Display| format!("{name}: {e}");
    let Decoded { text, w, h, sheet, rows, worn, detail } = decode(kind.keyword(), kind.noun(), name, bytes, MAX_SHEET)?;
    let detail = match detail.as_deref() {
        None => 1,
        Some(t) => t.parse::<u8>().ok().filter(|d| last_harvest_game::art::DETAILS.contains(d)).ok_or_else(|| fail(&format!("{DETAIL_KEYWORD} is {t:?}, but a card's detail is 1, 2 or 4")))?,
    };
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
    if frame.iter().any(|&side| side > MAX_FRAME * detail as u32) {
        let most = MAX_FRAME * detail as u32;
        return Err(fail(&format!("its frames are {}x{}, but the game draws them at most {MAX_FRAME} game pixels a side, {most} at detail {detail}", frame[0], frame[1])));
    }
    // A chunk this game can't read only costs the card its grenade's place, so a newer card still loads.
    let worn = worn.filter(|w| kind == Kind::Player && last_harvest_game::worn::parse_card(w).is_ok()).unwrap_or_default();
    Ok(Card { detail, worn, name: name.to_string(), text, frame, sheet })
}

/// Writes an enemy card from its text and a five-row RGBA sheet `w` by `h` pixels.
pub fn write(text: &str, w: u32, h: u32, sheet: &[u8]) -> Result<Vec<u8>, String> {
    write_as(Kind::Enemy, text, "", 1, w, h, 5, sheet)
}

/// Writes a card of `kind` from its text, its worn chunk (none when empty), its detail and an RGBA sheet `w` by `h`
/// pixels, `rows` (5, or 6 with a dead row) down.
#[allow(clippy::too_many_arguments)]
pub fn write_as(kind: Kind, text: &str, worn: &str, detail: u8, w: u32, h: u32, rows: u32, sheet: &[u8]) -> Result<Vec<u8>, String> {
    encode(kind.keyword(), text, w, h, (rows == 6).then_some("6"), worn, detail, sheet)
}

/// What a sound file is by its first bytes, `ogg` or `wav`; None for anything else.
pub fn sound_kind(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [b'O', b'g', b'g', b'S', ..] => Some("ogg"),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'A', b'V', b'E', ..] => Some("wav"),
        _ => None,
    }
}

/// A PNG's chunks after its signature, each whole: length, type, data and CRC.
fn chunks(png: &[u8]) -> Result<Vec<&[u8]>, String> {
    let (mut out, mut at) = (Vec::new(), png.get(..8).filter(|s| *s == b"\x89PNG\r\n\x1a\n").ok_or("not a PNG")?.len());
    while at < png.len() {
        let len = png.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize).ok_or("the PNG is cut short")?;
        let whole = png.get(at..at.saturating_add(len).saturating_add(12)).ok_or("the PNG is cut short")?;
        out.push(whole);
        at += whole.len();
    }
    Ok(out)
}

/// A card's own death sound, as its kind (`ogg` or `wav`) and bytes; None when it has none. One too big, damaged
/// or not a sound is refused unread.
pub fn sound(png: &[u8]) -> Result<Option<(&'static str, &[u8])>, String> {
    let Some(whole) = chunks(png)?.into_iter().find(|c| c[4..8] == SOUND_CHUNK) else { return Ok(None) };
    let data = &whole[8..whole.len() - 4];
    if data.len() > MAX_SOUND {
        return Err(format!("its death sound is {} KB, but a card's may be at most {} KB", data.len() / 1024, MAX_SOUND / 1024));
    }
    let (body, crc) = whole[4..].split_at(whole.len() - 8);
    if crc32fast::hash(body).to_be_bytes() != crc {
        return Err("its death sound is damaged".into());
    }
    sound_kind(data).map(|k| Some((k, data))).ok_or_else(|| "its death sound isn't an Ogg or WAV".into())
}

/// A card's PNG carrying `sound` as its own death sound in place of any it had, or none.
pub fn with_sound(png: &[u8], sound: Option<&[u8]>) -> Result<Vec<u8>, String> {
    if let Some(s) = sound {
        sound_kind(s).ok_or("a death sound is an Ogg or WAV")?;
        if s.len() > MAX_SOUND {
            return Err(format!("a card's death sound may be at most {} KB", MAX_SOUND / 1024));
        }
    }
    let mut out = png[..8].to_vec();
    for whole in chunks(png)? {
        if &whole[4..8] == b"IEND"
            && let Some(s) = sound
        {
            let body = [&SOUND_CHUNK[..], s].concat();
            out.extend((s.len() as u32).to_be_bytes());
            out.extend(&body);
            out.extend(crc32fast::hash(&body).to_be_bytes());
        }
        if whole[4..8] != SOUND_CHUNK {
            out.extend(whole);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An Ogg of `n` bytes, as far as its first bytes say.
    fn ogg(n: usize) -> Vec<u8> {
        [b"OggS".as_slice(), &vec![7; n - 4]].concat()
    }

    #[test]
    fn a_card_carries_its_own_death_sound_and_an_older_reader_never_minds() {
        let sheet = vec![9u8; 3 * 8 * 5 * 8 * 4];
        let plain = write("[ghoul]\nsound kill-ghoul", 24, 40, &sheet).unwrap();
        assert_eq!(sound(&plain), Ok(None));
        let loud = with_sound(&plain, Some(&ogg(MAX_SOUND))).unwrap();
        assert_eq!(sound(&loud), Ok(Some(("ogg", &ogg(MAX_SOUND)[..]))));
        let card = read("ghoul.enemy.png", &loud).unwrap();
        assert_eq!((card.text, card.sheet), ("[ghoul]\nsound kill-ghoul".to_string(), sheet), "a reader that knows nothing of the chunk reads the card as ever");
        let wav = [b"RIFF\0\0\0\0WAVE".as_slice(), &[1; 20]].concat();
        let swapped = with_sound(&loud, Some(&wav)).unwrap();
        assert_eq!(sound(&swapped), Ok(Some(("wav", &wav[..]))), "a new sound replaces the old");
        assert_eq!(with_sound(&swapped, None).unwrap(), plain, "and none leaves the card as it was");
        assert!(with_sound(&plain, Some(b"ID3 mp3")).is_err());
        assert!(with_sound(&plain, Some(&ogg(MAX_SOUND + 1))).is_err());
    }

    #[test]
    fn a_death_sound_too_big_or_damaged_is_refused_unread() {
        let plain = write("[a]", 3, 5, &[0; 3 * 5 * 4]).unwrap();
        let big = ogg(MAX_SOUND + 1);
        let body = [b"lhSd".as_slice(), &big].concat();
        let iend = plain.len() - 12;
        let sneaked = [&plain[..iend], &(big.len() as u32).to_be_bytes(), &body, &[0; 4], &plain[iend..]].concat();
        assert!(sound(&sneaked).err().unwrap().contains("at most 512 KB"), "the size is refused before the CRC is even checked");
        let mut damaged = with_sound(&plain, Some(&ogg(64))).unwrap();
        damaged[iend + 20] ^= 1;
        assert_eq!(sound(&damaged), Err("its death sound is damaged".into()));
        assert!(sound(&plain[..plain.len() - 3]).is_err(), "a PNG cut short");
    }

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
        let bytes = write_as(Kind::Player, "[jo]\nname Jo\nabout Up early.", "", 1, 24, 40, 5, &sheet).unwrap();
        let card = read_as(Kind::Player, "jo.player.png", &bytes).unwrap();
        assert_eq!((card.text.as_str(), card.frame), ("[jo]\nname Jo\nabout Up early.", [8, 8]));
        assert!(read("jo.player.png", &bytes).err().unwrap().contains("not an enemy card"), "an enemy card is a different kind");
        assert_eq!(Kind::of("jo.player.png"), Some(Kind::Player));
        assert_eq!(Kind::of("ghoul.enemy.png"), Some(Kind::Enemy));
        assert_eq!(Kind::of("ghoul.png"), None);
    }

    #[test]
    fn a_player_card_keeps_where_it_wears_its_grenade() {
        let sheet = vec![9u8; 3 * 8 * 5 * 8 * 4];
        let card = |kind, worn| read_as(kind, "jo", &write_as(kind, "[jo]", worn, 1, 24, 40, 5, &sheet).unwrap()).unwrap().worn;
        assert_eq!(card(Kind::Player, "front 5 6\nback hide\n"), "front 5 6\nback hide\n");
        assert_eq!(card(Kind::Player, ""), "");
        assert_eq!(card(Kind::Player, "wings 1 1"), "", "a chunk it can't read leaves the card, without it");
        assert_eq!(card(Kind::Enemy, "front 5 6"), "", "only a player wears one");
    }

    #[test]
    fn a_detail_chunk_lets_frames_run_that_many_times_past_the_limit() {
        let side = 4 * MAX_FRAME;
        let sheet = vec![9u8; (3 * side * 5 * side * 4) as usize];
        let fine = read_as(Kind::Player, "jo", &write_as(Kind::Player, "[jo]", "", 4, 3 * side, 5 * side, 5, &sheet).unwrap()).unwrap();
        assert_eq!((fine.detail, fine.frame), (4, [side, side]));
        let plain = read_as(Kind::Player, "jo", &write_as(Kind::Player, "[jo]", "", 1, 3 * side, 5 * side, 5, &sheet).unwrap());
        assert!(plain.err().unwrap().contains("at most 64 game pixels"));
        let small = vec![9u8; 3 * 8 * 5 * 8 * 4];
        let plain = read_as(Kind::Player, "jo", &write_as(Kind::Player, "[jo]", "", 1, 24, 40, 5, &small).unwrap()).unwrap();
        assert_eq!(plain.detail, 1, "no chunk at detail 1");
        let odd = encode(Kind::Player.keyword(), "[jo]", 24, 40, None, "", 3, &small).unwrap();
        assert!(read_as(Kind::Player, "jo", &odd).err().unwrap().contains("detail is 1, 2 or 4"));
    }

    #[test]
    fn a_rows_chunk_adds_a_dead_row() {
        let sheet = vec![9u8; 3 * 8 * 6 * 8 * 4];
        let bytes = write_as(Kind::Enemy, "[a]", "", 1, 24, 48, 6, &sheet).unwrap();
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
