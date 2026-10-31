//! Players' own props: sprites a map can place by `PROP_<ID>`, from a `.prop.png` whose text says how they stand.
//!
//! ```text
//! [rusty_tank]
//! frames 2        # frames across the sheet, 1 to MAX_FRAMES; 1 when left out
//! fps 4           # how fast they animate; still when left out
//! open 1          # top rows a figure walks through
//! stand prop 8    # prop N, fold N, slab N, figure, ground, carve or turned; prop by its size when left out
//! views 12        # how tall its south and east views are, drawn under its frames, for carve and turned
//! footprint yes   # its outline from above for carving follows its frames; its own outline when left out
//! roof yes        # a carving's top takes its colors from its roof, after any footprint; from its frames when left out
//! north yes       # its north and west sides take their colors from views of their own, after the south and east
//! west yes        # views; from those when left out
//! clear 5A6E3C    # colors the 3D view leaves out, where the 2D view keeps them, without a `#`
//! cover no        # whether it stops shots; yes when left out
//! hp 40           # hit points before zombies or shots break it, up to 1000; it doesn't break when left out
//! broken yes      # the last frame shows once it breaks, and the rest animate; it goes when left out
//! paint 91765F    # colors that are paint, light to dark, and a coat line for each coat they come in,
//! coat 4A6FA5     # as `crate::paint` says
//! ```
//!
//! The sheet holds its frames side by side along the top, then the footprint and the roof, and under them the south,
//! east, north and west views.

use std::sync::RwLock;

use super::map::MAX_HP;
use crate::art::{ATLAS_W, Art, Stand};
use crate::data::Card;
use crate::paint::{Paint, hex};

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
    /// A stand made from the views under its frames, which `stand_by` turns into `stand` once they are packed.
    pub by_views: Option<ByViews>,
    /// How tall the south and east views are; 0 for none.
    pub views: u32,
    pub footprint: bool,
    pub roof: bool,
    pub north: bool,
    pub west: bool,
    /// Colors the 3D view leaves out of its frames.
    pub clear: Vec<[u8; 3]>,
    pub cover: bool,
    pub hp: f32,
    pub broken: bool,
    /// Its paint colors and the coats each frame comes in.
    pub paint: Paint,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ByViews {
    Carve,
    Turned { depth: Option<f32> },
}

impl PropFields {
    /// Sets the stand a prop makes from its views once they are packed: `views` are the pictures after its frames, in
    /// the sheet's order.
    pub fn stand_by(&mut self, views: &[Art]) {
        let mut next = views.iter().copied();
        let mut take = |on: bool| if on { next.next() } else { None };
        let (footprint, roof) = (take(self.footprint), take(self.roof));
        let (Some(south), Some(east)) = (take(true), take(true)) else { return };
        let (north, west) = (take(self.north), take(self.west));
        match self.by_views {
            Some(ByViews::Carve) => self.stand = Some(Stand::Carve { footprint, roof, south, east, north, west }),
            Some(ByViews::Turned { depth }) => self.stand = Some(Stand::Turned { east, depth }),
            None => {}
        }
    }

    /// The pictures along the top of the sheet: the frames, then the footprint and roof when carving.
    pub fn cells(&self) -> u32 {
        self.frames + (self.views > 0) as u32 * (self.footprint as u32 + self.roof as u32)
    }

    /// Each frame's size in a sheet `w` by `h`, laid out as the module says, or None when it can't be.
    pub fn frame_in(&self, w: u32, h: u32) -> Option<[u32; 2]> {
        if self.views == 0 {
            return w.is_multiple_of(self.frames).then(|| [w / self.frames, h]);
        }
        let fh = h.checked_sub(self.views).filter(|&v| v > 0)?;
        let cells = self.cells();
        let by_views = w.saturating_sub((1 + self.west as u32) * fh) / (1 + self.north as u32);
        [w / cells, by_views].into_iter().find(|&fw| fw > 0 && w == self.sheet_w(fw, fh)).map(|fw| [fw, fh])
    }

    /// How wide the sheet is for frames `fw` by `fh`.
    pub fn sheet_w(&self, fw: u32, fh: u32) -> u32 {
        let views = if self.views > 0 { (1 + self.north as u32) * fw + (1 + self.west as u32) * fh } else { 0 };
        (self.cells() * fw).max(views)
    }
}

/// Reads a prop's text; errors name the line.
pub fn prop_fields(text: &str) -> Result<PropFields, String> {
    let mut f = PropFields { id: String::new(), frames: 1, fps: 0.0, open: 0, stand: None, by_views: None, views: 0, footprint: false, roof: false, north: false, west: false, clear: Vec::new(), cover: true, hp: 0.0, broken: false, paint: Paint::default() };
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
            "stand" if value == "carve" => f.by_views = Some(ByViews::Carve),
            "stand" if value.starts_with("turned") => f.by_views = Some(ByViews::Turned { depth: turned_depth(value).ok_or_else(|| format!("{n}: turned takes a depth from 1 to 64 pixels, or none"))? }),
            "stand" => f.stand = Some(parse_stand(value).ok_or_else(|| format!("{n}: a stand is prop N, fold N, slab N, slab N float N, figure, ground, carve or turned"))?),
            "views" => f.views = (number()? as u32).min(MAX_FRAME),
            "footprint" => f.footprint = yes_no(value).ok_or_else(|| format!("{n}: footprint is yes or no"))?,
            "roof" => f.roof = yes_no(value).ok_or_else(|| format!("{n}: roof is yes or no"))?,
            "north" => f.north = yes_no(value).ok_or_else(|| format!("{n}: north is yes or no"))?,
            "west" => f.west = yes_no(value).ok_or_else(|| format!("{n}: west is yes or no"))?,
            "cover" => f.cover = yes_no(value).ok_or_else(|| format!("{n}: cover is yes or no"))?,
            "hp" => f.hp = Some(number()?).filter(|v| *v <= MAX_HP).ok_or_else(|| format!("{n}: hp is a number from 0 to {MAX_HP}"))?,
            "broken" => f.broken = yes_no(value).ok_or_else(|| format!("{n}: broken is yes or no"))?,
            "clear" if !value.is_empty() => f.clear = value.split_whitespace().map(hex).collect::<Option<_>>().ok_or_else(|| format!("{n}: clear lists colors such as 5A6E3C"))?,
            "clear" => return Err(format!("{n}: clear lists colors such as 5A6E3C, without a #")),
            "paint" | "coat" => f.paint.read(key, value).unwrap_or(Ok(())).map_err(|e| format!("{n}: {e}"))?,
            _ => return Err(format!("{n}: {key} isn't something a prop says")),
        }
    }
    if f.id.is_empty() {
        return Err("no [id] line".into());
    }
    if f.broken && f.frames < 2 {
        return Err("a broken frame is the last of 2 or more frames".into());
    }
    if f.by_views.is_some() && f.views == 0 {
        return Err("a carved or turned prop has views under its frames, and a views line saying how tall".into());
    }
    f.paint.check()?;
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
    match (f.by_views, f.stand) {
        (Some(ByViews::Carve), _) => s += "stand carve\n",
        (Some(ByViews::Turned { depth: None }), _) => s += "stand turned\n",
        (Some(ByViews::Turned { depth: Some(d) }), _) => s += &format!("stand turned {d}\n"),
        (None, Some(stand)) => s += &format!("stand {}\n", stand_text(stand)),
        (None, None) => {}
    }
    if f.views > 0 {
        s += &format!("views {}\n", f.views);
    }
    if f.views > 0 && f.footprint {
        s += "footprint yes\n";
    }
    for (on, key) in [(f.roof, "roof"), (f.north, "north"), (f.west, "west")] {
        if f.views > 0 && on {
            s += &format!("{key} yes\n");
        }
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
    if !f.clear.is_empty() {
        s += &format!("clear {}\n", f.clear.iter().map(|[r, g, b]| format!("{r:02X}{g:02X}{b:02X}")).collect::<Vec<_>>().join(" "));
    }
    s + &f.paint.text()
}

/// The depth `turned` or `turned N` gives: Some(None) for none said, None when N isn't a depth.
pub fn turned_depth(text: &str) -> Option<Option<f32>> {
    match text.strip_prefix("turned")?.trim() {
        "" => Some(None),
        n => n.parse::<f32>().ok().filter(|d| (1.0..=64.0).contains(d)).map(Some),
    }
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
        "fold" => Stand::Fold { rows: size()? },
        "slab" => {
            let lift = match words.get(2..4) {
                Some(["float", n]) => n.parse::<f32>().ok().filter(|v| v.is_finite() && (0.0..=256.0).contains(v))?,
                None if words.len() == 2 => 0.0,
                _ => return None,
            };
            Stand::Slab { height: size()?, lift }
        }
        "figure" => Stand::Figure,
        "ground" => Stand::Ground,
        _ => return None,
    })
}

fn stand_text(s: Stand) -> String {
    match s {
        Stand::Prop { depth } => format!("prop {depth}"),
        Stand::Fold { rows } => format!("fold {rows}"),
        Stand::Slab { height, lift: 0.0 } => format!("slab {height}"),
        Stand::Slab { height, lift } => format!("slab {height} float {lift}"),
        Stand::Figure => "figure".into(),
        Stand::Ground => "ground".into(),
        Stand::Carve { .. } => "carve".into(),
        Stand::Turned { depth: None, .. } => "turned".into(),
        Stand::Turned { depth: Some(d), .. } => format!("turned {d}"),
    }
}

/// A prop as a map places it: its frames, how fast they turn, its walk-through rows, how it stands, whether it
/// stops shots, its hit points, and the frame that shows once it breaks. A prop with coats has a run of frames, a
/// stand and a broken frame for each.
#[derive(Clone, Debug)]
pub struct Named {
    pub name: String,
    pub frames: &'static [Art],
    pub fps: f32,
    pub open: usize,
    pub stands: Vec<Option<Stand>>,
    pub cover: bool,
    pub hp: f32,
    pub broken: Option<&'static [Art]>,
}

impl Named {
    pub fn coats(&self) -> usize {
        self.stands.len().max(1)
    }

    /// Coat `k`'s frames.
    pub fn coat(&self, k: usize) -> &'static [Art] {
        let per = self.frames.len() / self.coats();
        &self.frames[(k * per).min(self.frames.len() - per)..][..per]
    }

    /// The coat one of its frames is in.
    fn coat_of(&self, art: Art) -> usize {
        let at = |all: &[Art]| all.iter().position(|a| *a == art).map(|i| i * self.coats() / all.len());
        at(self.frames).or_else(|| self.broken.and_then(at)).unwrap_or(0)
    }

    /// The frame shown once the prop showing `art` breaks.
    pub fn broken_for(&self, art: Art) -> Option<&'static [Art]> {
        let k = self.coat_of(art);
        self.broken.map(|b| &b[k.min(b.len() - 1)..][..1])
    }
}

/// One coat of a prop as packed: its frames, and how it stands.
pub type Coat = (Vec<Art>, Option<Stand>);

/// Props from packs, set once the game draws them in, and the editor's, set as it packs them.
static NAMED: RwLock<Vec<Named>> = RwLock::new(Vec::new());

/// Sets the props maps can place by name, each with its coats. Leaks each call's frames: call it only when they
/// change.
pub fn set_props(props: Vec<(PropFields, Vec<Coat>)>) {
    let named = props
        .into_iter()
        .filter(|(_, coats)| coats.iter().all(|(arts, _)| !arts.is_empty()) && !coats.is_empty())
        .map(|(f, coats)| named(&f, coats))
        .collect();
    *NAMED.write().unwrap_or_else(|e| e.into_inner()) = named;
}

fn named(f: &PropFields, coats: Vec<Coat>) -> Named {
    let (mut frames, mut broken, mut stands) = (Vec::new(), Vec::new(), Vec::new());
    for (mut arts, stand) in coats {
        if f.broken && arts.len() > 1 {
            broken.extend(arts.pop());
        }
        frames.extend(arts);
        stands.push(stand);
    }
    let broken: &'static [Art] = broken.leak();
    Named { name: format!("{PREFIX}{}", f.id.to_ascii_uppercase()), frames: frames.leak(), fps: f.fps, open: f.open, stands, cover: f.cover, hp: f.hp, broken: (!broken.is_empty()).then_some(broken) }
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
    named_of(art).and_then(|n| n.stands[n.coat_of(art)])
}

/// The prop an added sprite is a frame of.
pub(crate) fn named_of(art: Art) -> Option<Named> {
    NAMED.read().ok()?.iter().find(|n| n.frames.contains(&art) || n.broken.is_some_and(|b| b.contains(&art))).cloned()
}

/// Loads props into a texture of their own, so maps that place them can be checked away from the game, as the
/// packer does; returns a line for each left out. Once a run: the sprites it adds can't be added again.
pub fn load_for_checks(props: &[Card]) -> Vec<String> {
    let mut atlas = vec![0u8; (ATLAS_W * crate::art::ATLAS_H * 4) as usize];
    let (notes, rects, named, _) = pack(props, &mut atlas, &mut super::enemies::start(), crate::art::RECTS.len());
    crate::art::set_added(rects);
    set_props(named);
    notes
}

/// A line for each prop left out, the rects drawn, and the props drawn with their coats.
type Packed = (Vec<String>, Vec<[u16; 4]>, Vec<(PropFields, Vec<Coat>)>, Vec<(Art, Art)>);

/// Draws each prop's frames into the texture's free rows from `cursor` on, numbered from `first`.
pub(super) fn pack(cards: &[Card], atlas: &mut [u8], cursor: &mut [u16; 3], first: usize) -> Packed {
    let (mut notes, mut rects, mut kept, mut solids) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for card in cards {
        let fields = match prop_fields(&card.text) {
            Ok(f) => f,
            Err(e) => {
                notes.push(format!("{}: {e}", card.name));
                continue;
            }
        };
        let [fw, fh] = card.frame;
        let (sw, vh) = (fields.sheet_w(fw, fh), fields.views);
        if !(1..=MAX_FRAME).contains(&fw) || !(1..=MAX_FRAME).contains(&fh) || card.sheet.len() != (sw * (fh + vh) * 4) as usize {
            notes.push(format!("{}: frames are 1 to {MAX_FRAME} pixels, {} across", card.name, fields.frames));
            continue;
        }
        if kept.iter().any(|(f, _): &(PropFields, Vec<Coat>)| f.id == fields.id) {
            notes.push(format!("{}: another prop is [{}] too", card.name, fields.id));
            continue;
        }
        // Each picture as its top-left in the sheet and its size: the frames, footprint and roof, then the views, then
        // the frames again for the 3D view, without the colors it clears; all of them again for each coat.
        let mut pictures: Vec<[u32; 4]> = (0..fields.cells()).map(|k| [k * fw, 0, fw, fh]).collect();
        if vh > 0 {
            pictures.extend([[0, fh, fw, vh], [fw, fh, fh, vh]]);
            let x = fw + fh;
            pictures.extend(fields.north.then_some([x, fh, fw, vh]));
            pictures.extend(fields.west.then_some([x + fields.north as u32 * fw, fh, fh, vh]));
        }
        let copies = if fields.clear.is_empty() { 0 } else { fields.frames as usize };
        let plain = pictures.len();
        pictures.extend((0..copies as u32).map(|k| [k * fw, 0, fw, fh]));
        let group = pictures.len();
        let pictures: Vec<[u32; 4]> = (0..fields.paint.copies()).flat_map(|_| pictures.iter().copied()).collect();
        let mut try_cursor = *cursor;
        let Some(spots) = pictures.iter().map(|p| super::enemies::place(&mut try_cursor, [p[2] as u16, p[3] as u16])).collect::<Option<Vec<_>>>() else {
            notes.push(format!("{}: no room left in the sprite texture", card.name));
            continue;
        };
        *cursor = try_cursor;
        let mut arts = Vec::new();
        for (k, ([sx, sy, w, h], [x, y])) in pictures.into_iter().zip(spots).enumerate() {
            let mut picture = vec![0u8; (w * h * 4) as usize];
            for py in 0..h {
                for px in 0..w {
                    let s = (((sy + py) * sw + sx + px) * 4) as usize;
                    let p = &card.sheet[s..s + 4];
                    let cleared = k % group >= plain && fields.clear.contains(&[p[0], p[1], p[2]]);
                    let d = ((py * w + px) * 4) as usize;
                    picture[d..d + 4].copy_from_slice(&if p[3] >= 128 && !cleared { [p[0], p[1], p[2], 255] } else { [0; 4] });
                }
            }
            fields.paint.repaint(k / group, &mut picture);
            for py in 0..h {
                let d = (((y as u32 + py) * ATLAS_W + x as u32) * 4) as usize;
                atlas[d..d + (w * 4) as usize].copy_from_slice(&picture[(py * w * 4) as usize..][..(w * 4) as usize]);
            }
            arts.push(Art((first + rects.len()) as u16));
            rects.push([x, y, w as u16, h as u16]);
        }
        let coats = arts
            .chunks(group)
            .map(|coat| {
                let (arts, in_3d) = coat.split_at(plain);
                solids.extend(arts.iter().copied().zip(in_3d.iter().copied()));
                let (frames, views) = arts.split_at(fields.frames as usize);
                let mut f = fields.clone();
                f.stand_by(views);
                (frames.to_vec(), f.stand)
            })
            .collect();
        kept.push((fields, coats));
    }
    (notes, rects, kept, solids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prop_packs_a_3d_copy_without_the_colors_it_clears() {
        let fields = prop_fields("[ash]\nframes 2\nclear FFFFFF").unwrap();
        assert_eq!(prop_fields(&prop_text(&fields)).unwrap().clear, [[255, 255, 255]]);
        assert!(prop_fields("[ash]\nclear #FFFFFF").unwrap_err().contains("without a #"), "a # starts a comment");
        let card = Card { name: "ash.prop.png".into(), text: prop_text(&fields), frame: [2, 2], sheet: vec![255; 2 * 2 * 2 * 4] };
        let mut atlas = vec![0u8; (ATLAS_W * crate::art::ATLAS_H * 4) as usize];
        let (notes, rects, kept, solids) = pack(&[card], &mut atlas, &mut super::super::enemies::start(), 0);
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!((kept[0].1[0].0.len(), solids.len()), (2, 2), "each frame has its copy");
        let alpha = |a: Art| atlas[((rects[a.0 as usize][1] as u32 * ATLAS_W + rects[a.0 as usize][0] as u32) * 4 + 3) as usize];
        assert_eq!((alpha(solids[0].0), alpha(solids[0].1)), (255, 0), "white stays in 2D and goes in 3D");
    }

    #[test]
    fn a_prop_packs_its_frames_and_views_once_a_coat() {
        let fields = prop_fields("[car]\nframes 2\nbroken yes\nstand carve\nviews 2\npaint 102030\ncoat 405060\ncoat 708090").unwrap();
        assert_eq!(prop_fields(&prop_text(&fields)).unwrap(), fields);
        let sheet = [0x10, 0x20, 0x30, 255].repeat((fields.sheet_w(2, 2) * 4) as usize);
        let card = Card { name: "car.prop.png".into(), text: prop_text(&fields), frame: [2, 2], sheet };
        let mut atlas = vec![0u8; (ATLAS_W * crate::art::ATLAS_H * 4) as usize];
        let (notes, rects, kept, _) = pack(&[card], &mut atlas, &mut super::super::enemies::start(), 0);
        assert!(notes.is_empty(), "{notes:?}");
        let color = |a: Art| {
            let i = ((rects[a.0 as usize][1] as u32 * ATLAS_W + rects[a.0 as usize][0] as u32) * 4) as usize;
            [atlas[i], atlas[i + 1], atlas[i + 2]]
        };
        let coats = &kept[0].1;
        assert_eq!((coats.len(), color(coats[0].0[0]), color(coats[1].0[1])), (2, [0x40, 0x50, 0x60], [0x70, 0x80, 0x90]));
        let Some(Stand::Carve { south, .. }) = coats[1].1 else { panic!("{:?}", coats[1].1) };
        assert_eq!(color(south), [0x70, 0x80, 0x90], "each coat carves from views in its own color");
        let n = named(&fields, coats.clone());
        assert_eq!((n.frames.len(), n.coat(1), n.broken_for(n.coat(1)[0])), (2, &[coats[1].0[0]][..], Some(&[coats[1].0[1]][..])), "a coat breaks into its own broken frame");
    }

    #[test]
    fn a_carved_prop_packs_its_roof_after_any_footprint_and_its_north_and_west_after_its_views() {
        let a = |k: u16| Some(Art(k));
        for (text, footprint, roof, north, west) in [
            ("roof yes", None, a(1), None, None),
            ("footprint yes\nroof yes", a(1), a(2), None, None),
            ("roof yes\nnorth yes\nwest yes", None, a(1), a(4), a(5)),
            ("west yes", None, None, None, a(3)),
        ] {
            let fields = prop_fields(&format!("[stack]\nstand carve\nviews 2\n{text}")).unwrap();
            assert_eq!(prop_fields(&prop_text(&fields)).unwrap(), fields);
            assert_eq!(fields.frame_in(fields.sheet_w(4, 3), 5), Some([4, 3]), "{text}");
            let sheet = vec![255; (fields.sheet_w(4, 3) * 5 * 4) as usize];
            let card = Card { name: "stack.prop.png".into(), text: prop_text(&fields), frame: [4, 3], sheet };
            let mut atlas = vec![0u8; (ATLAS_W * crate::art::ATLAS_H * 4) as usize];
            let (notes, rects, kept, _) = pack(&[card], &mut atlas, &mut super::super::enemies::start(), 0);
            assert!(notes.is_empty(), "{notes:?}");
            let views = fields.cells() as u16;
            assert_eq!(kept[0].1[0].1, Some(Stand::Carve { footprint, roof, south: Art(views), east: Art(views + 1), north, west }), "{text}");
            let size = |a: Option<Art>| a.map(|a| [rects[a.0 as usize][2], rects[a.0 as usize][3]]);
            assert_eq!((size(north), size(west)), (north.map(|_| [4, 2]), west.map(|_| [3, 2])), "{text}");
        }
    }

    #[test]
    fn a_turned_prop_packs_its_east_view_to_stand_up() {
        let (fw, fh, vh) = (4u32, 3u32, 5u32);
        let fields = prop_fields("[post]\nstand turned\nviews 5").unwrap();
        let sw = fields.sheet_w(fw, fh);
        let card = Card { name: "post.prop.png".into(), text: prop_text(&fields), frame: [fw, fh], sheet: vec![255; (sw * (fh + vh) * 4) as usize] };
        let mut atlas = vec![0u8; (ATLAS_W * crate::art::ATLAS_H * 4) as usize];
        let (notes, rects, kept, _) = pack(&[card], &mut atlas, &mut super::super::enemies::start(), 0);
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(rects.iter().map(|r| [r[2], r[3]]).collect::<Vec<_>>(), [[4, 3], [4, 5], [3, 5]], "the frame, then the south and east views");
        assert_eq!(kept[0].1, [(vec![Art(0)], Some(Stand::Turned { east: Art(2), depth: None }))]);
        let thin = prop_fields("[post]\nstand turned 2\nviews 5").unwrap();
        assert_eq!((thin.by_views, prop_fields(&prop_text(&thin)).unwrap()), (Some(ByViews::Turned { depth: Some(2.0) }), thin.clone()));
        assert!(prop_fields("[post]\nstand turned 0\nviews 5").unwrap_err().contains("depth"));
    }

    #[test]
    fn a_prop_reads_and_writes_its_text() {
        let f = prop_fields("[rusty_tank]\nframes 2\nfps 4\nopen 1\nstand slab 6\ncover no\nhp 40\nbroken yes\n").unwrap();
        assert_eq!(f, PropFields { id: "rusty_tank".into(), frames: 2, fps: 4.0, open: 1, stand: Some(Stand::Slab { height: 6.0, lift: 0.0 }), by_views: None, views: 0, footprint: false, roof: false, north: false, west: false, clear: Vec::new(), cover: false, hp: 40.0, broken: true, paint: Paint::default() });
        assert_eq!(prop_fields(&prop_text(&f)).unwrap(), f);
        assert_eq!(prop_text(&prop_fields("[a]").unwrap()), "[a]\n", "defaults stay out");
        assert!(prop_fields("frames 2").unwrap_err().contains("[id]"));
        assert!(prop_fields("[A]").unwrap_err().contains("lowercase"));
        assert!(prop_fields("[a]\nwings 2").unwrap_err().starts_with("2: wings"));
        assert!(prop_fields("[a]\nstand carve").unwrap_err().contains("views under its frames"));
        let carved = prop_fields("[a]\nframes 2\nstand carve\nviews 12\nfootprint yes").unwrap();
        assert_eq!((carved.by_views, carved.views, carved.footprint), (Some(ByViews::Carve), 12, true));
        assert_eq!(prop_fields(&prop_text(&carved)).unwrap(), carved);
        assert!(prop_fields("[a]\nstand sideways").unwrap_err().contains("a stand is"));
        let folded = prop_fields("[a]\nstand fold 20").unwrap();
        assert_eq!((folded.stand, prop_fields(&prop_text(&folded)).unwrap().stand), (Some(Stand::Fold { rows: 20.0 }), Some(Stand::Fold { rows: 20.0 })));
        let floating = prop_fields("[a]\nstand slab 1 float 6").unwrap();
        assert_eq!(floating.stand, Some(Stand::Slab { height: 1.0, lift: 6.0 }));
        assert_eq!(prop_fields(&prop_text(&floating)).unwrap(), floating);
        assert!(prop_fields("[a]\nstand slab 1 hover 6").unwrap_err().contains("a stand is"));
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
        let (notes, rects, kept, _) = pack(&cards, &mut atlas, &mut super::super::enemies::start(), 100);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].1, [(vec![Art(100), Art(101)], None)]);
        assert_eq!(rects.len(), 2);
        assert!(notes[0].contains("another prop is [a]") && notes[1].contains("3 across"), "{notes:?}");
    }
}
