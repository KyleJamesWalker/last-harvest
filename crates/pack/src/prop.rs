//! Props: a PNG of a sprite's frames side by side, with its text in a compressed `iTXt` chunk, as
//! `last_harvest_game::prop_fields` reads it. A carved or turned prop also holds its views, laid out as the
//! game's `PropFields::frame_in` says. A `last-harvest-detail` chunk of 2 or 4 draws the whole sheet that many of its
//! pixels to a game pixel, as on a card.

use last_harvest_game::data::Card;
use last_harvest_game::{PROP_MAX_FRAME, PROP_MAX_FRAMES};

pub const KEYWORD: &str = "last-harvest-prop";
pub const EXTENSION: &str = ".prop.png";

/// Reads a prop; `name` is for messages.
pub fn read(name: &str, bytes: &[u8]) -> Result<Card, String> {
    let fail = |e: &dyn std::fmt::Display| format!("{name}: {e}");
    let max = [(PROP_MAX_FRAMES + 2) * PROP_MAX_FRAME, 2 * PROP_MAX_FRAME];
    let d = crate::card::decode(KEYWORD, "a prop", name, bytes, max)?;
    let detail = crate::card::detail_of(d.detail.as_deref()).map_err(|e| fail(&e))?;
    let fields = last_harvest_game::prop_fields(&d.text).map_err(|e| fail(&e))?;
    let frame = fields.frame_in(d.w, d.h).filter(|f| f.iter().all(|&side| side <= PROP_MAX_FRAME));
    let Some(frame) = frame else {
        let views = if fields.views > 0 { format!(" over views {} tall", fields.views) } else { String::new() };
        return Err(fail(&format!("{}x{} doesn't split into {} frames of at most {PROP_MAX_FRAME}{views}", d.w, d.h, fields.frames)));
    };
    Ok(Card { detail, worn: String::new(), name: name.to_string(), text: d.text, frame, sheet: d.sheet })
}

/// Writes a prop from its text and an RGBA sheet `w` by `h` pixels.
pub fn write(text: &str, w: u32, h: u32, sheet: &[u8]) -> Result<Vec<u8>, String> {
    write_at(text, 1, w, h, sheet)
}

/// Writes a prop as `write` does, `detail` of its pixels to a game pixel.
pub fn write_at(text: &str, detail: u8, w: u32, h: u32, sheet: &[u8]) -> Result<Vec<u8>, String> {
    crate::card::encode(KEYWORD, text, w, h, None, "", detail, sheet)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prop_round_trips_and_checks_its_sheet() {
        let sheet = vec![7u8; 2 * 10 * 6 * 4];
        let p = read("tank.prop.png", &write("[tank]\nframes 2", 20, 6, &sheet).unwrap()).unwrap();
        assert_eq!((p.frame, p.text.as_str()), ([10, 6], "[tank]\nframes 2"));
        assert!(read("x.prop.png", &write("[x]\nframes 3", 20, 6, &sheet).unwrap()).err().unwrap().contains("3 frames"));
        assert!(read("y.prop.png", &write("frames 2", 20, 6, &sheet).unwrap()).err().unwrap().contains("[id]"));
        assert!(read("h.prop.png", &write("[h]\nframes 2\nhp 1e9", 20, 6, &sheet).unwrap()).err().unwrap().contains("hp is a number from 0 to 1000"));
        assert!(read("b.prop.png", &write("[b]\nbroken yes", 20, 6, &sheet).unwrap()).err().unwrap().contains("2 or more frames"));
        let carved = read("c.prop.png", &write("[c]\nframes 2\nstand carve\nviews 4\nfootprint yes", 30, 10, &vec![0; 30 * 10 * 4]).unwrap()).unwrap();
        assert_eq!(carved.frame, [10, 6], "three cells across over views 4 tall");
        let tall = read("t.prop.png", &write("[t]\nstand turned\nviews 3", 12, 11, &vec![0; 12 * 11 * 4]).unwrap()).unwrap();
        assert_eq!(tall.frame, [4, 8], "views wider than the frames set the width");
        let card = crate::card::write("[a]", 3, 5, &[0; 60]).unwrap();
        assert!(read("z.prop.png", &card).err().unwrap().contains("not a prop"), "a card isn't a prop");
    }

    #[test]
    fn a_prop_round_trips_its_detail() {
        let sheet = vec![7u8; 2 * 8 * 6 * 4];
        let p = read("d.prop.png", &write_at("[d]\nframes 2", 2, 16, 6, &sheet).unwrap()).unwrap();
        assert_eq!((p.detail, p.frame), (2, [8, 6]));
        assert_eq!(read("e.prop.png", &write("[e]", 16, 6, &sheet[..16 * 6 * 4]).unwrap()).unwrap().detail, 1);
        assert!(read("f.prop.png", &write_at("[f]", 3, 16, 6, &sheet[..16 * 6 * 4]).unwrap()).err().unwrap().contains("1, 2 or 4"));
    }

    #[test]
    fn the_widest_carved_prop_reads() {
        let (fw, cells) = (PROP_MAX_FRAME, PROP_MAX_FRAMES + 2);
        let text = format!("[wide]\nframes {PROP_MAX_FRAMES}\nstand carve\nviews 2\nfootprint yes\nroof yes");
        let p = read("w.prop.png", &write(&text, cells * fw, 3, &vec![0; (cells * fw * 3 * 4) as usize]).unwrap()).unwrap();
        assert_eq!(p.frame, [fw, 1]);
    }
}
