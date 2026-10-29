//! Props: a PNG of a sprite's frames side by side, one row high, with its text in a compressed `iTXt` chunk, as
//! `last_harvest_game::prop_fields` reads it.

use last_harvest_game::data::Card;
use last_harvest_game::{PROP_MAX_FRAME, PROP_MAX_FRAMES};

pub const KEYWORD: &str = "last-harvest-prop";
pub const EXTENSION: &str = ".prop.png";

/// Reads a prop; `name` is for messages.
pub fn read(name: &str, bytes: &[u8]) -> Result<Card, String> {
    let fail = |e: &dyn std::fmt::Display| format!("{name}: {e}");
    let max = [PROP_MAX_FRAMES * PROP_MAX_FRAME, PROP_MAX_FRAME];
    let d = crate::card::decode(KEYWORD, "a prop", name, bytes, max)?;
    let fields = last_harvest_game::prop_fields(&d.text).map_err(|e| fail(&e))?;
    if d.w % fields.frames != 0 || d.w / fields.frames > PROP_MAX_FRAME {
        return Err(fail(&format!("{} pixels across doesn't split into {} frames of at most {PROP_MAX_FRAME}", d.w, fields.frames)));
    }
    Ok(Card { name: name.to_string(), text: d.text, frame: [d.w / fields.frames, d.h], sheet: d.sheet })
}

/// Writes a prop from its text and an RGBA sheet `w` by `h` pixels.
pub fn write(text: &str, w: u32, h: u32, sheet: &[u8]) -> Result<Vec<u8>, String> {
    crate::card::encode(KEYWORD, text, w, h, None, sheet)
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
        let card = crate::card::write("[a]", 3, 5, &[0; 60]).unwrap();
        assert!(read("z.prop.png", &card).err().unwrap().contains("not a prop"), "a card isn't a prop");
    }
}
