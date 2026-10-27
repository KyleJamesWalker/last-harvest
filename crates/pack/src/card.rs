//! Cards: a PNG of a figure's frames, five rows down, with its text in a compressed `iTXt` chunk, so one image file
//! shares the art and the numbers together. The rows walk front, side and back, then flash and hurt with one frame
//! per facing. An enemy card's text is its `enemies.txt` section, whose `walk` sets how many frames its walking rows
//! have (3 when left out), and the sheet is as wide as its longest row; a player card's is its id and name, and it
//! always walks three frames.

use last_harvest_game::data::Card;

pub const KEYWORD: &str = "last-harvest-enemy";
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

/// Reads a card of `kind`; `name` is for messages.
pub fn read_as(kind: Kind, name: &str, bytes: &[u8]) -> Result<Card, String> {
    let fail = |e: &dyn std::fmt::Display| format!("{name}: {e}");
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16 | png::Transformations::ALPHA);
    let mut reader = decoder.read_info().map_err(|e| fail(&e))?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf).map_err(|e| fail(&e))?;
    buf.truncate(frame.buffer_size());
    let sheet = match frame.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::GrayscaleAlpha => buf.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        other => return Err(fail(&format!("unexpected color type {other:?}"))),
    };
    reader.finish().map_err(|e| fail(&e))?;
    let keyword = kind.keyword();
    let text = reader.info().utf8_text.iter().find(|t| t.keyword == keyword).ok_or_else(|| fail(&format!("no {keyword} text, so not {}", kind.noun())))?;
    let mut text = text.clone();
    text.decompress_text().map_err(|e| fail(&e))?;
    let text = text.get_text().map_err(|e| fail(&e))?;
    let (w, h) = (frame.width, frame.height);
    let cols = match kind {
        Kind::Enemy => last_harvest_game::card_fields(&text).map_or(3, |f| f.walk).max(3) as u32,
        Kind::Player => 3,
    };
    if w % cols != 0 || h % 5 != 0 {
        return Err(fail(&format!("{w}x{h} doesn't split into {cols} frames across and five down")));
    }
    Ok(Card { name: name.to_string(), text, frame: [w / cols, h / 5], sheet })
}

/// Writes an enemy card from its text and an RGBA sheet `w` by `h` pixels.
pub fn write(text: &str, w: u32, h: u32, sheet: &[u8]) -> Result<Vec<u8>, String> {
    write_as(Kind::Enemy, text, w, h, sheet)
}

/// Writes a card of `kind` from its text and an RGBA sheet `w` by `h` pixels.
pub fn write_as(kind: Kind, text: &str, w: u32, h: u32, sheet: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    let mut chunk = png::text_metadata::ITXtChunk::new(kind.keyword(), text);
    chunk.compress_text().map_err(|e| e.to_string())?;
    writer.write_text_chunk(&chunk).map_err(|e| e.to_string())?;
    writer.write_image_data(sheet).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_card_round_trips_its_text_and_frames() {
        let (w, h) = (3 * 12, 5 * 20);
        let sheet: Vec<u8> = (0..w * h).flat_map(|i| [(i % 256) as u8, 7, 9, if i % 3 == 0 { 0 } else { 255 }]).collect();
        let text = "[ghoul]\nname Ghoul — fast\nhp 40\n";
        let card = read("ghoul.enemy.png", &write(text, w, h, &sheet).unwrap()).unwrap();
        assert_eq!(card.text, text);
        assert_eq!(card.frame, [12, 20]);
        assert_eq!(card.sheet, sheet);
    }

    #[test]
    fn a_player_card_keeps_its_own_keyword() {
        let sheet = vec![9u8; 3 * 8 * 5 * 8 * 4];
        let bytes = write_as(Kind::Player, "[jo]\nname Jo", 24, 40, &sheet).unwrap();
        let card = read_as(Kind::Player, "jo.player.png", &bytes).unwrap();
        assert_eq!((card.text.as_str(), card.frame), ("[jo]\nname Jo", [8, 8]));
        assert!(read("jo.player.png", &bytes).err().unwrap().contains("not an enemy card"), "an enemy card is a different kind");
        assert_eq!(Kind::of("jo.player.png"), Some(Kind::Player));
        assert_eq!(Kind::of("ghoul.enemy.png"), Some(Kind::Enemy));
        assert_eq!(Kind::of("ghoul.png"), None);
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
    }
}
