//! Players' own props: sprites a map can place by `PROP_<ID>`, from a `.prop.png` whose text says how they stand.
//!
//! ```text
//! [rusty_tank]
//! frames 2        # frames across the sheet, 1 to MAX_FRAMES; 1 when left out
//! fps 4           # how fast they animate; still when left out
//! open 1          # top rows a figure walks through
//! stand prop 8    # prop N, slab N, figure or ground; prop by its size when left out
//! cover no        # whether it stops shots; yes when left out
//! hp 40           # hit points before zombies or shots break it, up to 1000; it doesn't break when left out
//! broken yes      # the last frame shows once it breaks, and the rest animate; it goes when left out
//! ```

use std::sync::RwLock;

use super::map::MAX_HP;
use crate::art::{ATLAS_W, Art, Stand};
use crate::data::Card;

/// What a prop's sprite name starts with, so it never clashes with the game's own.
pub const PREFIX: &str = "PROP_";
pub const MAX_FRAMES: u32 = 16;
/// The biggest frame a prop can have, as big as the game's biggest building.
pub const MAX_FRAME: u32 = 256;

#[derive(Clone, Debug, PartialEq)]
pub struct PropFields {
    pub id: String,
    pub frames: u32,
    pub fps: f32,
    pub open: usize,
    pub stand: Option<Stand>,
    pub cover: bool,
    pub hp: f32,
    pub broken: bool,
}

/// Reads a prop's text; errors name the line.
pub fn prop_fields(text: &str) -> Result<PropFields, String> {
    let mut f = PropFields { id: String::new(), frames: 1, fps: 0.0, open: 0, stand: None, cover: true, hp: 0.0, broken: false };
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(id) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            if !f.id.is_empty() {
                return Err(format!("{n}: a prop has one [id]"));
            }
            if !super::valid_map_id(id) {
                return Err(format!("{n}: a prop's id is 1 to {} lowercase letters, digits and _", crate::PLAYER_ID_MAX));
            }
            f.id = id.to_string();
            continue;
        }
        let (key, value) = line.split_once(char::is_whitespace).map_or((line, ""), |(k, v)| (k, v.trim()));
        let number = || value.parse::<f32>().ok().filter(|v| v.is_finite() && *v >= 0.0).ok_or_else(|| format!("{n}: {key} is a number"));
        match key {
            "frames" => f.frames = (number()? as u32).clamp(1, MAX_FRAMES),
            "fps" => f.fps = number()?.min(60.0),
            "open" => f.open = (number()? as usize).min(64),
            "stand" => f.stand = Some(parse_stand(value).ok_or_else(|| format!("{n}: a stand is prop N, slab N, figure or ground"))?),
            "cover" => f.cover = yes_no(value).ok_or_else(|| format!("{n}: cover is yes or no"))?,
            "hp" => f.hp = Some(number()?).filter(|v| *v <= MAX_HP).ok_or_else(|| format!("{n}: hp is a number from 0 to {MAX_HP}"))?,
            "broken" => f.broken = yes_no(value).ok_or_else(|| format!("{n}: broken is yes or no"))?,
            _ => return Err(format!("{n}: {key} isn't something a prop says")),
        }
    }
    if f.id.is_empty() {
        return Err("no [id] line".into());
    }
    if f.broken && f.frames < 2 {
        return Err("a broken frame is the last of 2 or more frames".into());
    }
    Ok(f)
}

/// Writes a prop's text from its fields, leaving out what it would take anyway.
pub fn prop_text(f: &PropFields) -> String {
    let mut s = format!("[{}]\n", f.id);
    if f.frames > 1 {
        s += &format!("frames {}\n", f.frames);
    }
    if f.fps > 0.0 {
        s += &format!("fps {}\n", f.fps);
    }
    if f.open > 0 {
        s += &format!("open {}\n", f.open);
    }
    if let Some(stand) = f.stand {
        s += &format!("stand {}\n", stand_text(stand));
    }
    if !f.cover {
        s += "cover no\n";
    }
    if f.hp > 0.0 {
        s += &format!("hp {}\n", f.hp);
    }
    if f.broken {
        s += "broken yes\n";
    }
    s
}

fn yes_no(word: &str) -> Option<bool> {
    match word {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    }
}

fn parse_stand(text: &str) -> Option<Stand> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let size = || words.get(1)?.parse::<f32>().ok().filter(|v| v.is_finite() && (0.0..=256.0).contains(v));
    Some(match *words.first()? {
        "prop" => Stand::Prop { depth: size()? },
        "slab" => Stand::Slab { height: size()? },
        "figure" => Stand::Figure,
        "ground" => Stand::Ground,
        _ => return None,
    })
}

fn stand_text(s: Stand) -> String {
    match s {
        Stand::Prop { depth } => format!("prop {depth}"),
        Stand::Slab { height } => format!("slab {height}"),
        Stand::Figure => "figure".into(),
        Stand::Ground | Stand::Carve { .. } => "ground".into(),
    }
}

/// A prop as a map places it: its frames, how fast they turn, its walk-through rows, how it stands, whether it
/// stops shots, its hit points, and the frame that shows once it breaks.
#[derive(Clone, Debug)]
pub struct Named {
    pub name: String,
    pub frames: &'static [Art],
    pub fps: f32,
    pub open: usize,
    pub stand: Option<Stand>,
    pub cover: bool,
    pub hp: f32,
    pub broken: Option<&'static [Art]>,
}

/// Props from packs, set once the game draws them in, and the editor's, set as it packs them.
static NAMED: RwLock<Vec<Named>> = RwLock::new(Vec::new());

/// Sets the props maps can place by name. Leaks each call's frames: call it only when they change.
pub fn set_props(props: Vec<(PropFields, Vec<Art>)>) {
    let named = props
        .into_iter()
        .map(|(f, arts)| {
            let all: &'static [Art] = arts.leak();
            let (frames, broken) = if f.broken && all.len() > 1 { all.split_at(all.len() - 1) } else { (all, &[][..]) };
            Named { name: format!("{PREFIX}{}", f.id.to_ascii_uppercase()), frames, fps: f.fps, open: f.open, stand: f.stand, cover: f.cover, hp: f.hp, broken: (!broken.is_empty()).then_some(broken) }
        })
        .collect();
    *NAMED.write().unwrap_or_else(|e| e.into_inner()) = named;
}

/// A prop by its sprite name, such as `PROP_RUSTY_TANK`.
pub fn prop(name: &str) -> Option<Named> {
    NAMED.read().unwrap_or_else(|e| e.into_inner()).iter().find(|n| n.name == name).cloned()
}

/// Every prop maps can place by name, in the order set.
pub fn props() -> Vec<Named> {
    NAMED.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// How an added sprite stands, if a prop set it.
pub(crate) fn stand_of(art: Art) -> Option<Stand> {
    named_of(art).and_then(|n| n.stand)
}

/// The prop an added sprite is a frame of.
pub(crate) fn named_of(art: Art) -> Option<Named> {
    NAMED.read().ok()?.iter().find(|n| n.frames.contains(&art) || n.broken.is_some_and(|b| b.contains(&art))).cloned()
}

/// Loads props into a texture of their own, so maps that place them can be checked away from the game, as the
/// packer does; returns a line for each left out. Once a run: the sprites it adds can't be added again.
pub fn load_for_checks(props: &[Card]) -> Vec<String> {
    let mut atlas = vec![0u8; (ATLAS_W * crate::art::ATLAS_H * 4) as usize];
    let (notes, rects, named) = pack(props, &mut atlas, &mut super::enemies::start(), crate::art::RECTS.len());
    crate::art::set_added(rects);
    set_props(named);
    notes
}

/// A line for each prop left out, the rects drawn, and the props drawn with their arts.
type Packed = (Vec<String>, Vec<[u16; 4]>, Vec<(PropFields, Vec<Art>)>);

/// Draws each prop's frames into the texture's free rows from `cursor` on, numbered from `first`.
pub(super) fn pack(cards: &[Card], atlas: &mut [u8], cursor: &mut [u16; 3], first: usize) -> Packed {
    let (mut notes, mut rects, mut kept) = (Vec::new(), Vec::new(), Vec::new());
    for card in cards {
        let fields = match prop_fields(&card.text) {
            Ok(f) => f,
            Err(e) => {
                notes.push(format!("{}: {e}", card.name));
                continue;
            }
        };
        let [fw, fh] = card.frame;
        if !(1..=MAX_FRAME).contains(&fw) || !(1..=MAX_FRAME).contains(&fh) || card.sheet.len() != (fw * fields.frames * fh * 4) as usize {
            notes.push(format!("{}: frames are 1 to {MAX_FRAME} pixels, {} across", card.name, fields.frames));
            continue;
        }
        if kept.iter().any(|(f, _): &(PropFields, Vec<Art>)| f.id == fields.id) {
            notes.push(format!("{}: another prop is [{}] too", card.name, fields.id));
            continue;
        }
        let mut try_cursor = *cursor;
        let Some(spots) = (0..fields.frames).map(|_| super::enemies::place(&mut try_cursor, [fw as u16, fh as u16])).collect::<Option<Vec<_>>>() else {
            notes.push(format!("{}: no room left in the sprite texture", card.name));
            continue;
        };
        *cursor = try_cursor;
        let mut arts = Vec::new();
        for (col, [x, y]) in spots.into_iter().enumerate() {
            for py in 0..fh {
                for px in 0..fw {
                    let s = ((py * fw * fields.frames + col as u32 * fw + px) * 4) as usize;
                    let d = (((y as u32 + py) * ATLAS_W + x as u32 + px) * 4) as usize;
                    let p = &card.sheet[s..s + 4];
                    atlas[d..d + 4].copy_from_slice(&if p[3] >= 128 { [p[0], p[1], p[2], 255] } else { [0; 4] });
                }
            }
            arts.push(Art((first + rects.len()) as u16));
            rects.push([x, y, fw as u16, fh as u16]);
        }
        kept.push((fields, arts));
    }
    (notes, rects, kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prop_reads_and_writes_its_text() {
        let f = prop_fields("[rusty_tank]\nframes 2\nfps 4\nopen 1\nstand slab 6\ncover no\nhp 40\nbroken yes\n").unwrap();
        assert_eq!(f, PropFields { id: "rusty_tank".into(), frames: 2, fps: 4.0, open: 1, stand: Some(Stand::Slab { height: 6.0 }), cover: false, hp: 40.0, broken: true });
        assert_eq!(prop_fields(&prop_text(&f)).unwrap(), f);
        assert_eq!(prop_text(&prop_fields("[a]").unwrap()), "[a]\n", "defaults stay out");
        assert!(prop_fields("frames 2").unwrap_err().contains("[id]"));
        assert!(prop_fields("[A]").unwrap_err().contains("lowercase"));
        assert!(prop_fields("[a]\nwings 2").unwrap_err().starts_with("2: wings"));
        assert!(prop_fields("[a]\nstand carve").unwrap_err().contains("a stand is"));
        assert!(prop_fields("[a]\nfps nan").is_err());
        assert!(prop_fields("[a]\ncover maybe").unwrap_err().contains("cover is yes or no"));
        assert!(prop_fields("[a]").unwrap().cover, "props stop shots unless they say");
        for bad in ["hp -1", "hp 1001", "hp nan", "hp inf", "hp", "broken 2"] {
            assert!(prop_fields(&format!("[a]\nframes 2\n{bad}")).unwrap_err().starts_with("3: "), "{bad}");
        }
        assert!(prop_fields("[a]\nbroken yes").unwrap_err().contains("2 or more frames"));
    }

    #[test]
    fn mangled_prop_texts_are_refused_without_panicking() {
        let good = "[rusty_tank]\nframes 3\nfps 4\nopen 1\nstand slab 6\ncover no\nhp 40\nbroken yes\n";
        prop_fields(good).unwrap();
        let pieces = ["99999", "1001", "0", "-1", "nan", "inf", "hp", "cover", "broken", "yes", "no", "frames", "stand", "[", "]", "\n", " ", "#"];
        let mut rng = crate::rng::Rng::new(7);
        for _ in 0..4000 {
            let mut text = good.to_string();
            for _ in 0..1 + rng.below(3) {
                let at = rng.below(text.len());
                match rng.below(3) {
                    0 => text.insert_str(at, pieces[rng.below(pieces.len())]),
                    1 => text.replace_range(at..(at + 1 + rng.below(6)).min(text.len()), ""),
                    _ => text.insert(at, (b' ' + rng.below(95) as u8) as char),
                }
            }
            if let Ok(f) = prop_fields(&text) {
                assert!(f.hp.is_finite() && (0.0..=MAX_HP).contains(&f.hp) && (!f.broken || f.frames > 1), "{text:?}");
                assert_eq!(prop_fields(&prop_text(&f)).unwrap(), f, "{text:?}");
            }
        }
    }

    #[test]
    fn props_draw_into_free_rows_and_refuse_bad_sheets() {
        let mut atlas = vec![0u8; (crate::art::ATLAS_W * crate::art::ATLAS_H * 4) as usize];
        let card = |name: &str, text: &str, frame: [u32; 2], n: u32| Card { name: name.into(), text: text.into(), frame, sheet: vec![255; (frame[0] * n * frame[1] * 4) as usize] };
        let cards = [card("a.prop.png", "[a]\nframes 2", [20, 10], 2), card("b.prop.png", "[a]", [8, 8], 1), card("c.prop.png", "[c]\nframes 3", [8, 8], 2)];
        let (notes, rects, kept) = pack(&cards, &mut atlas, &mut super::super::enemies::start(), 100);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].1, vec![Art(100), Art(101)]);
        assert_eq!(rects.len(), 2);
        assert!(notes[0].contains("another prop is [a]") && notes[1].contains("3 across"), "{notes:?}");
    }
}
