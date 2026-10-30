//! How the random farm lays itself out, read from `farm.txt`.

use std::sync::OnceLock;

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
    /// Props strewn anywhere free: a sprite's frames, and one for each this many cells.
    pub scatter: Vec<(&'static [Art], usize)>,
    /// The buildings near the middle, in the order they go down.
    pub buildings: Vec<Building>,
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

const FARM_KEYS: [&str; 5] = ["tufts", "grass", "corpses", "fields", "more_fields"];

/// Reads `farm.txt`; a prop of a player's that isn't loaded is left out. Errors name the line.
pub fn parse(text: &str) -> Result<Farm, String> {
    let mut f = Farm { tufts: 0, grass: 0, corpses: 0, fields: 0, more_fields: 0, ground: b'.', tufted: b',', patch: b'g', scatter: Vec::new(), buildings: Vec::new() };
    let mut seen = [false; FARM_KEYS.len()];
    let mut section = "";
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(s) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = match s {
                "farm" => "farm",
                "scatter" => "scatter",
                _ if s.is_empty() || !s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') => {
                    return Err(format!("{n}: [{s}] isn't [farm], [scatter] or a building named in lowercase letters, digits and _"));
                }
                _ if f.buildings.iter().any(|b| b.name == s) => return Err(format!("{n}: [{s}] appears twice")),
                _ => {
                    f.buildings.push(Building { name: s.to_string(), sprites: Vec::new(), within: 0, margin: 0, door: 0, near: Vec::new() });
                    "building"
                }
            };
            continue;
        }
        let (key, value) = line.split_once(char::is_whitespace).map_or((line, ""), |(k, v)| (k, v.trim()));
        let count = || value.parse::<usize>().map_err(|_| format!("{n}: {key} takes a whole number"));
        match section {
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
                *[&mut f.tufts, &mut f.grass, &mut f.corpses, &mut f.fields, &mut f.more_fields][at] = count()?;
                seen[at] = true;
            }
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
                    "within" => b.within = count()?,
                    "margin" => b.margin = count()?,
                    "door" => b.door = count()?,
                    "near" => match value.split_whitespace().collect::<Vec<_>>()[..] {
                        ["windmill"] => b.near.push(Near::Windmill),
                        ["graveyard"] => b.near.push(Near::Graveyard),
                        [name, within, ref rest @ ..] if rest.is_empty() || rest == ["door"] => {
                            let within = within.parse::<usize>().ok().filter(|w| *w > 0).ok_or_else(|| format!("{n}: near takes a sprite and how many tiles from 1"))?;
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
    Ok(f)
}

/// How the random farm lays itself out, read the first time it's asked for.
pub fn farm() -> &'static Farm {
    static F: OnceLock<Farm> = OnceLock::new();
    F.get_or_init(|| parse(crate::data::farm()).unwrap_or_else(|e| panic!("farm.txt:{e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_farm_reads_and_mistakes_name_their_line() {
        let f = farm();
        assert_eq!((f.tufts, f.fields, f.scatter.len(), f.ground), (10, 4, 10, b'.'));
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
    }
}
