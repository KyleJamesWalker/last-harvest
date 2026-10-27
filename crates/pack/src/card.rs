//! Enemy cards: a PNG of the enemy's frames, three across and five down, with its `enemies.txt` section in a
//! compressed `iTXt` chunk, so one image file shares the art and the numbers together.

use last_harvest_game::data::Card;

pub const KEYWORD: &str = "last-harvest-enemy";

/// Reads a card; `name` is for messages.
pub fn read(name: &str, bytes: &[u8]) -> Result<Card, String> {
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
    let text = reader.info().utf8_text.iter().find(|t| t.keyword == KEYWORD).ok_or_else(|| fail(&format!("no {KEYWORD} text, so not an enemy card")))?;
    let mut text = text.clone();
    text.decompress_text().map_err(|e| fail(&e))?;
    let text = text.get_text().map_err(|e| fail(&e))?;
    let (w, h) = (frame.width, frame.height);
    if w % 3 != 0 || h % 5 != 0 {
        return Err(fail(&format!("{w}x{h} doesn't split into three frames across and five down")));
    }
    Ok(Card { name: name.to_string(), text, frame: [w / 3, h / 5], sheet })
}

/// Writes a card from its text and an RGBA sheet `w` by `h` pixels.
pub fn write(text: &str, w: u32, h: u32, sheet: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    let mut chunk = png::text_metadata::ITXtChunk::new(KEYWORD, text);
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
        let card = read("ghoul.png", &write(text, w, h, &sheet).unwrap()).unwrap();
        assert_eq!(card.text, text);
        assert_eq!(card.frame, [12, 20]);
        assert_eq!(card.sheet, sheet);
    }

    #[test]
    fn plain_images_and_odd_sizes_are_not_cards() {
        let mut plain = Vec::new();
        let mut encoder = png::Encoder::new(&mut plain, 3, 5);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&[0; 60]).unwrap();
        assert!(read("x.png", &plain).err().unwrap().contains("not an enemy card"));
        let odd = write("[a]", 4, 5, &[0; 80]).unwrap();
        assert!(read("y.png", &odd).err().unwrap().contains("three frames across"));
    }
}
