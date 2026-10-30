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
    /// Props strewn anywhere free: a sprite's frames, and one for each this many cells.
    pub scatter: Vec<(&'static [Art], usize)>,
}

/// A sprite's frames by name: the catalog's, else a player's prop's.
fn frames(name: &str) -> Option<&'static [Art]> {
    art::ALL.iter().find(|(n, _)| *n == name).map(|(_, f)| *f).or_else(|| super::super::props::prop(name).map(|p| p.frames))
}

/// Reads `farm.txt`; a scattered prop of a player's that isn't loaded is left out. Errors name the line.
pub fn parse(text: &str) -> Result<Farm, String> {
    let mut f = Farm { tufts: 0, grass: 0, corpses: 0, fields: 0, more_fields: 0, scatter: Vec::new() };
    let mut seen = [false; 5];
    let mut section = "";
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(s) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            if !matches!(s, "farm" | "scatter") {
                return Err(format!("{n}: [{s}] isn't a section; there's [farm] and [scatter]"));
            }
            section = if s == "farm" { "farm" } else { "scatter" };
            continue;
        }
        let (key, value) = line.split_once(char::is_whitespace).map_or((line, ""), |(k, v)| (k, v.trim()));
        let count = value.parse::<usize>().map_err(|_| format!("{n}: {key} takes a whole number"))?;
        match section {
            "farm" => {
                let at = ["tufts", "grass", "corpses", "fields", "more_fields"].iter().position(|k| *k == key).ok_or_else(|| format!("{n}: {key} isn't something [farm] sets"))?;
                *[&mut f.tufts, &mut f.grass, &mut f.corpses, &mut f.fields, &mut f.more_fields][at] = count;
                seen[at] = true;
            }
            "scatter" => {
                if count == 0 {
                    return Err(format!("{n}: {key} is one for each N cells, N from 1"));
                }
                match frames(key) {
                    Some(set) => f.scatter.push((set, count)),
                    None if key.starts_with(super::super::props::PREFIX) => {}
                    None => return Err(format!("{n}: no sprite called {key}")),
                }
            }
            _ => return Err(format!("{n}: a line before [farm] or [scatter]")),
        }
    }
    if let Some(k) = seen.iter().position(|s| !s) {
        return Err(format!("[farm] doesn't set {}", ["tufts", "grass", "corpses", "fields", "more_fields"][k]));
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
        assert_eq!((f.tufts, f.fields, f.scatter.len()), (10, 4, 10));
        assert_eq!(f.scatter[0], (&art::DEAD_TREE[..], 120));
        let base = "[farm]\ntufts 1\ngrass 1\ncorpses 1\nfields 1\nmore_fields 1\n[scatter]\n";
        assert!(parse(&format!("{base}PROP_NOT_LOADED 5")).unwrap().scatter.is_empty(), "a prop that isn't loaded is left out");
        assert!(parse(&format!("{base}NOPE 5")).unwrap_err().starts_with("8: no sprite"));
        assert!(parse(&format!("{base}BUSH 0")).unwrap_err().starts_with("8: BUSH"));
        assert!(parse("[farm]\ntufts 1").unwrap_err().contains("doesn't set grass"));
    }
}
