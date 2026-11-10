//! How the random farm lays itself out, read from `farm.txt`.

use std::sync::OnceLock;

use super::Fence;
use crate::art::{self, Art};

#[derive(Clone, Debug)]
pub struct Farm {
    pub tufts: usize,
    pub grass: usize,
    pub corpses: usize,
    pub fields: usize,
    pub more_fields: usize,
    /// The ground codes the farm is laid on, a tufted cell gets, and a grass patch paints.
    pub ground: u8,
    pub tufted: u8,
    pub patch: u8,
    /// The props ringing the map's edge in turn, each its first frame.
    pub border: Vec<&'static [Art]>,
    /// The frames a corpse on the ground picks from.
    pub corpse: &'static [Art],
    pub field: Fields,
    pub road: Road,
    pub station: Station,
    pub windmill: Windmill,
    pub graveyard: Graveyard,
    /// Props strewn anywhere free: a sprite's frames, and one for each this many cells.
    pub scatter: Vec<(&'static [Art], usize)>,
    /// The buildings near the middle, in the order they go down.
    pub buildings: Vec<Building>,
}

/// What a field is and how it lies.
#[derive(Clone, Debug)]
pub struct Fields {
    /// The ground every field is laid on.
    pub soil: u8,
    /// Ground codes a field is laid bare with, and ground codes whose crop it grows, each with its weight.
    pub plain: Vec<(u8, usize)>,
    pub crops: Vec<(u8, usize)>,
    /// The least and most tiles across, and down.
    pub width: [usize; 2],
    pub height: [usize; 2],
    /// One field in this many is fenced; 0 for none.
    pub fenced: usize,
    pub(super) fence: Fence,
    /// What stands in one bare field in this many; 0 for none.
    pub scarecrow: (&'static [Art], usize),
}

impl Fields {
    /// Every kind of field, bare ones first, each with its weight and whether it grows a crop.
    pub fn kinds(&self) -> impl Iterator<Item = (u8, usize, bool)> + '_ {
        self.plain.iter().map(|&(c, w)| (c, w, false)).chain(self.crops.iter().map(|&(c, w)| (c, w, true)))
    }
}

/// The road down the map and what stands on and beside it.
#[derive(Clone, Debug)]
pub struct Road {
    pub ground: u8,
    /// Tries at a sign on its shoulders, and the sprites a sign picks from.
    pub signs: usize,
    pub sign: Vec<&'static [Art]>,
    pub barrier: &'static [Art],
    /// The least and most wrecks on it, and the frames a wreck picks from.
    pub wrecks: [usize; 2],
    pub wreck: &'static [Art],
}

/// The gas station by the road, its pumps, and the truck and car parked near.
#[derive(Clone, Debug)]
pub struct Station {
    pub station: &'static [Art],
    pub pump: &'static [Art],
    pub car: &'static [Art],
    pub truck: &'static [Art],
    pub driver: &'static [Art],
}

#[derive(Clone, Debug)]
pub struct Windmill {
    pub mill: &'static [Art],
    pub fan: &'static [Art],
}

#[derive(Clone, Debug)]
pub struct Graveyard {
    pub tombstone: &'static [Art],
    pub(super) fence: Fence,
}

/// A building the farm puts near the middle, with a path from its door to the road.
#[derive(Clone, Debug)]
pub struct Building {
    pub name: String,
    /// The sprites it picks one of at random.
    pub sprites: Vec<&'static [Art]>,
    /// How far from the middle it may stand, in tiles, and how many tiles stay clear around it.
    pub within: usize,
    pub margin: usize,
    /// Tiles in from its left to its door.
    pub door: usize,
    /// What goes down around it, in order.
    pub near: Vec<Near>,
}

#[derive(Clone, Debug)]
pub enum Near {
    /// A sprite's frames within this many tiles of the building's middle, or of the step outside its door.
    Prop { frames: &'static [Art], within: usize, door: bool },
    /// The windmill, its fan turning.
    Windmill,
    /// Tombstones in a picket fence.
    Graveyard,
}

/// A sprite's frames by name: the catalog's, else a player's prop's.
fn frames(name: &str) -> Option<&'static [Art]> {
    art::ALL.iter().find(|(n, _)| *n == name).map(|(_, f)| *f).or_else(|| super::super::props::prop(name).map(|p| p.frames))
}

/// The most tiles a building's `within`, `margin`, `door` or `near` may name: wider than the largest map.
const MAX_TILES: usize = 200;

/// The most `fields` or `more_fields`: each is a search across the map at every run start.
const MAX_FIELDS: usize = 64;

const FARM_KEYS: [&str; 5] = ["tufts", "grass", "corpses", "fields", "more_fields"];

/// The most tiles a field is across or down, wreck on the road, sign tried, or weight of a kind of field.
const MAX_FIELD: usize = 40;
const MAX_WRECKS: usize = 64;
const MAX_SIGNS: usize = 64;
const MAX_WEIGHT: usize = 1000;

/// The sections that aren't buildings.
const SECTIONS: [&str; 7] = ["farm", "scatter", "fields", "road", "gas_station", "windmill", "graveyard"];

/// One of the game's own sprites, which the farm lays when `farm.txt` doesn't say.
fn own(name: &str) -> &'static [Art] {
    art::ALL.iter().find(|(n, _)| *n == name).map(|(_, f)| *f).unwrap_or_else(|| panic!("no sprite called {name} in art.txt"))
}

impl Default for Farm {
    /// Today's farm, but for the counts `[farm]` must set and the buildings and scatter it lists.
    fn default() -> Self {
        let rail = Fence { top: own("RAIL_TOP"), side: own("RAIL_SIDE"), bottom: own("RAIL_BOTTOM"), gate_post: None };
        let picket = Fence { top: own("PICKET_TOP"), side: own("PICKET_SIDE"), bottom: own("PICKET_BOTTOM"), gate_post: Some(own("SKULL_FENCE")) };
        Farm {
            tufts: 0,
            grass: 0,
            corpses: 0,
            fields: 0,
            more_fields: 0,
            ground: b'.',
            tufted: b',',
            patch: b'g',
            border: vec![own("DEAD_TREE"), own("BUSH"), own("BUSH")],
            corpse: own("CORPSE"),
            field: Fields { soil: b's', plain: vec![(b's', 1), (b'p', 1)], crops: vec![(b'c', 2)], width: [5, 11], height: [4, 8], fenced: 2, fence: rail, scarecrow: (own("SCARECROW"), 2) },
            road: Road { ground: b'v', signs: 4, sign: vec![own("SIGN"), own("SIGN_LOW")], barrier: own("BARRIER"), wrecks: [3, 5], wreck: own("WRECK_V") },
            station: Station { station: own("GAS_STATION"), pump: own("GAS_PUMP"), car: own("CAR_V"), truck: own("TRUCK"), driver: own("TRUCK_DRIVER") },
            windmill: Windmill { mill: own("WINDMILL"), fan: own("WINDMILL_FAN") },
            graveyard: Graveyard { tombstone: own("TOMBSTONE"), fence: picket },
            scatter: Vec::new(),
            buildings: Vec::new(),
        }
    }
}

/// A ground code, one character.
fn code(n: usize, key: &str, word: &str) -> Result<u8, String> {
    match word.as_bytes() {
        [c] if c.is_ascii_graphic() => Ok(*c),
        _ => Err(format!("{n}: {key} is a ground code from ground.txt, one character")),
    }
}

/// A sprite's frames by name; none for a player's prop that isn't loaded.
fn sprite(n: usize, name: &str) -> Result<Option<&'static [Art]>, String> {
    match frames(name).filter(|f| !f.is_empty()) {
        Some(set) => Ok(Some(set)),
        None if name.starts_with(super::super::props::PREFIX) => Ok(None),
        None => Err(format!("{n}: no sprite called {name}")),
    }
}

/// A fence style: its top row's three frames, its sides' two, its bottom row's three, and a gate post's, if any.
fn fence(n: usize, key: &str, words: &[&str], into: &mut Fence) -> Result<(), String> {
    let bad = || format!("{n}: {key} is TOP SIDE BOTTOM and a GATE post if any: sprites of 3, 2 and 3 frames");
    let ([top, side, bottom], gate) = match *words {
        [a, b, c] => ([a, b, c], None),
        [a, b, c, d] => ([a, b, c], Some(d)),
        _ => return Err(bad()),
    };
    let (top, side, bottom) = (sprite(n, top)?, sprite(n, side)?, sprite(n, bottom)?);
    let gate = gate.map(|g| sprite(n, g)).transpose()?;
    if [top.map(<[Art]>::len), side.map(<[Art]>::len), bottom.map(<[Art]>::len)].iter().zip([3, 2, 3]).any(|(l, want)| l.is_some_and(|l| l != want)) {
        return Err(bad());
    }
    if let (Some(top), Some(side), Some(bottom)) = (top, side, bottom)
        && gate.is_none_or(|g| g.is_some())
    {
        *into = Fence { top, side, bottom, gate_post: gate.flatten() };
    }
    Ok(())
}

/// `K` whole numbers, each up to `most`.
fn numbers<const K: usize>(n: usize, key: &str, words: &[&str], most: usize, what: &str) -> Result<[usize; K], String> {
    let bad = || format!("{n}: {key} is {what}, up to {most}");
    let got: Vec<usize> = words.iter().map(|w| w.parse::<usize>().ok().filter(|v| *v <= most)).collect::<Option<_>>().ok_or_else(bad)?;
    got.try_into().map_err(|_| bad())
}

/// A least and most, from `least` up.
fn range(n: usize, key: &str, words: &[&str], least: usize, most: usize) -> Result<[usize; 2], String> {
    let [lo, hi] = numbers::<2>(n, key, words, most, "LEAST MOST")?;
    if lo < least || lo > hi {
        return Err(format!("{n}: {key} is LEAST MOST, from {least} to {most}, the least first"));
    }
    Ok([lo, hi])
}

/// Reads `farm.txt`; a prop of a player's that isn't loaded is left out. Errors name the line.
pub fn parse(text: &str) -> Result<Farm, String> {
    let mut f = Farm::default();
    let mut seen = [false; FARM_KEYS.len()];
    let mut section = "";
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(s) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = match SECTIONS.iter().find(|k| **k == s) {
                Some(k) => k,
                None if s.is_empty() || !s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') => {
                    return Err(format!("{n}: [{s}] isn't [farm], [scatter], [fields], [road], [gas_station], [windmill], [graveyard] or a building named in lowercase letters, digits and _"));
                }
                None if f.buildings.iter().any(|b| b.name == s) => return Err(format!("{n}: [{s}] appears twice")),
                None => {
                    f.buildings.push(Building { name: s.to_string(), sprites: Vec::new(), within: 0, margin: 0, door: 0, near: Vec::new() });
                    "building"
                }
            };
            continue;
        }
        let (key, value) = line.split_once(char::is_whitespace).map_or((line, ""), |(k, v)| (k, v.trim()));
        let count = || value.parse::<usize>().map_err(|_| format!("{n}: {key} takes a whole number"));
        let words: Vec<&str> = value.split_whitespace().collect();
        let one = |into: &mut &'static [Art]| -> Result<(), String> {
            let [name] = words[..] else { return Err(format!("{n}: {key} is one sprite")) };
            if let Some(set) = sprite(n, name)? {
                *into = set;
            }
            Ok(())
        };
        let weights = |into: &mut Vec<(u8, usize)>| -> Result<(), String> {
            let bad = || format!("{n}: {key} is ground codes from ground.txt, each with its weight, 1 to {MAX_WEIGHT}");
            if !words.len().is_multiple_of(2) {
                return Err(bad());
            }
            *into = words.chunks(2).map(|p| Ok((code(n, key, p[0])?, p[1].parse::<usize>().ok().filter(|w| (1..=MAX_WEIGHT).contains(w)).ok_or_else(bad)?))).collect::<Result<_, String>>()?;
            Ok(())
        };
        match section {
            "farm" if key == "border" => {
                let sets: Vec<Option<&'static [Art]>> = words.iter().map(|w| sprite(n, w)).collect::<Result<_, _>>()?;
                if sets.is_empty() {
                    return Err(format!("{n}: border is the sprites ringing the map, at least one"));
                }
                f.border = sets.into_iter().flatten().collect();
                if f.border.is_empty() {
                    f.border = Farm::default().border;
                }
            }
            "farm" if key == "corpse" => one(&mut f.corpse)?,
            "farm" if matches!(key, "ground" | "tufted" | "patch") => {
                let [code] = value.as_bytes() else { return Err(format!("{n}: {key} is a ground code from ground.txt, one character")) };
                *match key {
                    "ground" => &mut f.ground,
                    "tufted" => &mut f.tufted,
                    _ => &mut f.patch,
                } = *code;
            }
            "farm" => {
                let at = FARM_KEYS.iter().position(|k| *k == key).ok_or_else(|| format!("{n}: {key} isn't something [farm] sets"))?;
                let v = count()?;
                if matches!(key, "fields" | "more_fields") && v > MAX_FIELDS {
                    return Err(format!("{n}: {key} is at most {MAX_FIELDS}"));
                }
                *[&mut f.tufts, &mut f.grass, &mut f.corpses, &mut f.fields, &mut f.more_fields][at] = v;
                seen[at] = true;
            }
            "fields" => match key {
                "soil" => {
                    let [c] = words[..] else { return Err(format!("{n}: soil is a ground code from ground.txt, one character")) };
                    f.field.soil = code(n, key, c)?;
                }
                "plain" => weights(&mut f.field.plain)?,
                "crops" => weights(&mut f.field.crops)?,
                "width" => f.field.width = range(n, key, &words, 2, MAX_FIELD)?,
                "height" => f.field.height = range(n, key, &words, 2, MAX_FIELD)?,
                "fenced" => f.field.fenced = count()?,
                "fence" => fence(n, key, &words, &mut f.field.fence)?,
                "scarecrow" => {
                    let [name, every] = words[..] else { return Err(format!("{n}: scarecrow is a sprite and N, for one bare field in N; 0 for none")) };
                    let every = every.parse().map_err(|_| format!("{n}: scarecrow is a sprite and N, for one bare field in N; 0 for none"))?;
                    f.field.scarecrow = (sprite(n, name)?.unwrap_or(f.field.scarecrow.0), every);
                }
                _ => return Err(format!("{n}: {key} isn't something [fields] sets; there's soil, plain, crops, width, height, fenced, fence and scarecrow")),
            },
            "road" => match key {
                "ground" => {
                    let [c] = words[..] else { return Err(format!("{n}: ground is a ground code from ground.txt, one character")) };
                    f.road.ground = code(n, key, c)?;
                }
                "signs" => {
                    let Some((tries, names)) = words.split_first() else { return Err(format!("{n}: signs is N tries and the sprites a sign picks from")) };
                    let [tries] = numbers::<1>(n, key, &[tries], MAX_SIGNS, "N tries and the sprites a sign picks from")?;
                    let sets: Vec<Option<&'static [Art]>> = names.iter().map(|w| sprite(n, w)).collect::<Result<_, _>>()?;
                    if tries > 0 && sets.is_empty() {
                        return Err(format!("{n}: signs names the sprites a sign picks from"));
                    }
                    f.road.sign = sets.into_iter().flatten().collect();
                    f.road.signs = if f.road.sign.is_empty() { 0 } else { tries };
                }
                "barrier" => one(&mut f.road.barrier)?,
                "wrecks" => {
                    let [lo, hi, name] = words[..] else { return Err(format!("{n}: wrecks is LEAST MOST and a sprite")) };
                    f.road.wrecks = range(n, key, &[lo, hi], 0, MAX_WRECKS)?;
                    if let Some(set) = sprite(n, name)? {
                        f.road.wreck = set;
                    }
                }
                _ => return Err(format!("{n}: {key} isn't something [road] sets; there's ground, signs, barrier and wrecks")),
            },
            "gas_station" => match key {
                "station" => one(&mut f.station.station)?,
                "pump" => one(&mut f.station.pump)?,
                "car" => one(&mut f.station.car)?,
                "truck" => one(&mut f.station.truck)?,
                "driver" => one(&mut f.station.driver)?,
                _ => return Err(format!("{n}: {key} isn't something [gas_station] sets; there's station, pump, car, truck and driver")),
            },
            "windmill" => match key {
                "mill" => one(&mut f.windmill.mill)?,
                "fan" => one(&mut f.windmill.fan)?,
                _ => return Err(format!("{n}: {key} isn't something [windmill] sets; there's mill and fan")),
            },
            "graveyard" => match key {
                "tombstone" => one(&mut f.graveyard.tombstone)?,
                "fence" => fence(n, key, &words, &mut f.graveyard.fence)?,
                _ => return Err(format!("{n}: {key} isn't something [graveyard] sets; there's tombstone and fence")),
            },
            "scatter" => {
                let count = count()?;
                if count == 0 {
                    return Err(format!("{n}: {key} is one for each N cells, N from 1"));
                }
                match frames(key) {
                    Some(set) => f.scatter.push((set, count)),
                    None if key.starts_with(super::super::props::PREFIX) => {}
                    None => return Err(format!("{n}: no sprite called {key}")),
                }
            }
            "building" => {
                let b = f.buildings.last_mut().expect("a building's section");
                match key {
                    "sprites" => {
                        for name in value.split_whitespace() {
                            match frames(name) {
                                Some(set) => b.sprites.push(set),
                                None if name.starts_with(super::super::props::PREFIX) => {}
                                None => return Err(format!("{n}: no sprite called {name}")),
                            }
                        }
                    }
                    "within" | "margin" | "door" => {
                        let v = count()?;
                        if v > MAX_TILES {
                            return Err(format!("{n}: {key} is at most {MAX_TILES} tiles"));
                        }
                        *match key {
                            "within" => &mut b.within,
                            "margin" => &mut b.margin,
                            _ => &mut b.door,
                        } = v;
                    }
                    "near" => match value.split_whitespace().collect::<Vec<_>>()[..] {
                        ["windmill"] => b.near.push(Near::Windmill),
                        ["graveyard"] => b.near.push(Near::Graveyard),
                        [name, within, ref rest @ ..] if rest.is_empty() || rest == ["door"] => {
                            let within = within.parse::<usize>().ok().filter(|w| (1..=MAX_TILES).contains(w)).ok_or_else(|| format!("{n}: near takes a sprite and how many tiles, 1 to {MAX_TILES}"))?;
                            match frames(name) {
                                Some(frames) => b.near.push(Near::Prop { frames, within, door: !rest.is_empty() }),
                                None if name.starts_with(super::super::props::PREFIX) => {}
                                None => return Err(format!("{n}: no sprite called {name}")),
                            }
                        }
                        _ => return Err(format!("{n}: near is windmill, graveyard, or a sprite and how many tiles, then door to count from it")),
                    },
                    _ => return Err(format!("{n}: {key} isn't something a building sets; there's sprites, within, margin, door and near")),
                }
            }
            _ => return Err(format!("{n}: a line before [farm], [scatter] or a building")),
        }
    }
    if let Some(k) = seen.iter().position(|s| !s) {
        return Err(format!("[farm] doesn't set {}", FARM_KEYS[k]));
    }
    if let Some(b) = f.buildings.iter().find(|b| b.within == 0) {
        return Err(format!("[{}] needs within, from 1", b.name));
    }
    if f.field.kinds().next().is_none() {
        return Err("[fields] needs a plain or crops code".into());
    }
    Ok(f)
}

/// Checks the ground codes a farm names against `ground`: each is there, and each of its crops is a field with a
/// crop.
pub fn check_codes(f: &Farm, ground: &[super::ground::Ground]) -> Result<(), String> {
    let plain = [f.ground, f.tufted, f.patch, f.field.soil, f.road.ground].into_iter().chain(f.field.plain.iter().map(|p| p.0));
    if let Some(c) = plain.into_iter().find(|c| !ground.iter().any(|g| g.code == *c)) {
        return Err(format!("no ground code {} in ground.txt", c as char));
    }
    match f.field.crops.iter().find(|(c, _)| !ground.iter().any(|g| g.code == *c && g.cell == super::Cell::Taken && g.crop.is_some())) {
        Some((c, _)) => Err(format!("crops {}: not a ground code with cell field and a crop line in ground.txt", *c as char)),
        None => Ok(()),
    }
}

/// How the random farm lays itself out: the farm in use's, else `farm.txt`, read the first time it's asked for.
pub fn farm() -> &'static Farm {
    if let Some(f) = super::farms::active_farm() {
        return f;
    }
    static F: OnceLock<Farm> = OnceLock::new();
    F.get_or_init(|| parse(crate::data::farm()).unwrap_or_else(|e| panic!("farm.txt:{e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_farm_reads_and_mistakes_name_their_line() {
        let f = farm();
        assert_eq!((f.tufts, f.fields, f.scatter.len(), f.ground), (10, 4, 11, b'.'));
        assert_eq!(f.scatter[0], (&art::DEAD_TREE[..], 120));
        assert_eq!(f.buildings.iter().map(|b| (b.name.as_str(), b.sprites.len(), b.near.len())).collect::<Vec<_>>(), [("house", 1, 2), ("barn", 2, 4)]);
        let base = "[farm]\ntufts 1\ngrass 1\ncorpses 1\nfields 1\nmore_fields 1\n[scatter]\n";
        assert!(parse(&format!("{base}PROP_NOT_LOADED 5")).unwrap().scatter.is_empty(), "a prop that isn't loaded is left out");
        assert!(parse(&format!("{base}NOPE 5")).unwrap_err().starts_with("8: no sprite"));
        assert!(parse(&format!("{base}BUSH 0")).unwrap_err().starts_with("8: BUSH"));
        assert!(parse("[farm]\ntufts 1").unwrap_err().contains("doesn't set grass"));
        let shed = parse(&format!("{base}[shed]\nsprites TANK PROP_GONE\nwithin 5\nnear SIGN 3 door\nnear graveyard")).unwrap();
        assert!(matches!(shed.buildings[0].near[..], [Near::Prop { within: 3, door: true, .. }, Near::Graveyard]));
        assert_eq!(shed.buildings[0].sprites.len(), 1);
        assert_eq!(parse(&format!("{base}[shed]\nsprites TANK")).unwrap_err(), "[shed] needs within, from 1");
        assert!(parse(&format!("{base}[shed]\nwithin 5\nnear SIGN")).unwrap_err().starts_with("10: near is"));
        assert!(parse(&format!("{base}[Shed]")).unwrap_err().starts_with("8: [Shed]"));
        assert!(parse("[farm]\nground ..").unwrap_err().starts_with("2: ground is a ground code"));
        assert!(parse(&format!("{base}[shed]\nmargin 18446744073709551615")).unwrap_err().starts_with("9: margin"));
        assert!(parse(&format!("{base}[shed]\nwithin 5\nnear SIGN 201")).unwrap_err().starts_with("10: near takes"));
        for key in ["fields", "more_fields"] {
            let huge = base.replace(&format!("\n{key} 1"), &format!("\n{key} 4000000000"));
            assert!(crate::harvest::check_table("farm.txt", &huge).unwrap_err().contains(&format!("{key} is at most 64")), "{key}");
            parse(&base.replace(&format!("\n{key} 1"), &format!("\n{key} 64"))).unwrap();
        }
    }

    #[test]
    fn what_the_farm_lays_reads_or_keeps_todays_and_mistakes_name_their_line() {
        let base = "[farm]\ntufts 1\ngrass 1\ncorpses 1\nfields 1\nmore_fields 1\n";
        let today = parse(base).unwrap();
        let own = crate::harvest::built_in_table("farm.txt").unwrap();
        let said = parse(&format!("{base}{}", &own[own.find("border").unwrap()..own.find("[scatter]").unwrap()])).unwrap();
        assert_eq!(format!("{today:?}"), format!("{said:?}"), "the game's own file says what's kept when left out");
        let f = parse(&format!("{base}[fields]\nplain\ncrops c 2 k 1\n[windmill]\nfan PROP_GONE")).unwrap();
        assert_eq!((f.field.plain, f.field.crops), (vec![], vec![(b'c', 2), (b'k', 1)]));
        assert_eq!(f.windmill.fan, today.windmill.fan, "a prop that isn't loaded keeps the sprite");
        for (text, error) in [
            ("[fields]\ncrops c", "8: crops is ground codes"),
            ("[fields]\ncrops cc 1", "8: crops is a ground code"),
            ("[fields]\ncrops c 0", "8: crops is ground codes"),
            ("[fields]\nwidth 9 3", "8: width is LEAST MOST"),
            ("[fields]\nheight 1 3", "8: height is LEAST MOST"),
            ("[fields]\nfence RAIL_TOP RAIL_TOP RAIL_BOTTOM", "8: fence is TOP SIDE BOTTOM"),
            ("[fields]\nscarecrow SCARECROW", "8: scarecrow is a sprite"),
            ("[fields]\nplain\ncrops", "[fields] needs a plain or crops code"),
            ("[road]\nwrecks 1 2 NOPE", "8: no sprite called NOPE"),
            ("[road]\nsigns 4", "8: signs names"),
            ("[gas_station]\npump GAS_PUMP CAR_V", "8: pump is one sprite"),
            ("[windmill]\nsail WINDMILL", "8: sail isn't something [windmill] sets"),
            ("border", "7: border is"),
        ] {
            assert!(parse(&format!("{base}{text}")).unwrap_err().starts_with(error), "{text}: {:?}", parse(&format!("{base}{text}")).unwrap_err());
        }
    }

    #[test]
    fn a_farms_codes_are_ground_and_its_crops_grow() {
        let ground = super::super::ground::parse(&format!("{}\n[k]\nname Cane\nart SOIL\ncell field\ncrop SIGN BARRIER TOMBSTONE\n", crate::harvest::built_in_table("ground.txt").unwrap())).unwrap();
        let farm = |more: &str| parse(&format!("[farm]\ntufts 1\ngrass 1\ncorpses 1\nfields 1\nmore_fields 1\n{more}")).unwrap();
        assert_eq!(check_codes(&farm(""), &ground), Ok(()));
        assert_eq!(check_codes(&farm("[fields]\ncrops k 1"), &ground), Ok(()));
        assert_eq!(check_codes(&farm("[fields]\ncrops p 1"), &ground), Err("crops p: not a ground code with cell field and a crop line in ground.txt".into()));
        assert_eq!(check_codes(&farm("[fields]\ncrops Z 1"), &ground).unwrap_err(), "crops Z: not a ground code with cell field and a crop line in ground.txt");
        assert_eq!(check_codes(&farm("[road]\nground Z"), &ground), Err("no ground code Z in ground.txt".into()));
        assert_eq!(check_codes(&farm("[fields]\nplain Z 1"), &ground), Err("no ground code Z in ground.txt".into()));
    }
}
