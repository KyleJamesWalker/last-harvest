//! The ground a map paints, one code a cell, read from `ground.txt`.

use std::sync::OnceLock;

use super::Cell;
use crate::art::{self, Art};

/// How a run of the same ground lays its art's first, middle and last frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Joins {
    Across,
    Down,
}

/// One kind of ground: its code, its paint button's label, its tile frames, and what the cell is.
#[derive(Clone, Debug)]
pub struct Ground {
    pub code: u8,
    pub name: String,
    pub art: &'static [Art],
    pub joins: Option<Joins>,
    pub(super) cell: Cell,
    /// A crop standing on it: its north end's frames, its middle, and its south end.
    pub crop: Option<(&'static [Art], Art, Art)>,
}

impl Ground {
    /// The tile at (x, y), picked between the art's frames by where it is, as every run picks the same.
    pub(super) fn tile(&self, x: usize, y: usize, before: bool, after: bool) -> Art {
        match self.joins {
            Some(_) => self.art[if !before { 0 } else if !after { 2 } else { 1 }.min(self.art.len() - 1)],
            None => self.art[(x * 7 + y * 13) % self.art.len()],
        }
    }
}

fn frames(name: &str) -> Option<&'static [Art]> {
    art::ALL.iter().find(|(n, _)| *n == name).map(|(_, f)| *f)
}

/// Reads `ground.txt`; errors name the line.
pub fn parse(text: &str) -> Result<Vec<Ground>, String> {
    let mut out: Vec<Ground> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(code) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let &[c] = code.as_bytes() else { return Err(format!("{n}: a ground's code is one character, not [{code}]")) };
            if !c.is_ascii_graphic() || c == b'#' || c == b'[' {
                return Err(format!("{n}: [{code}] can't be a ground's code"));
            }
            if out.iter().any(|g| g.code == c) {
                return Err(format!("{n}: [{code}] appears twice"));
            }
            out.push(Ground { code: c, name: code.to_string(), art: &art::DIRT, joins: None, cell: Cell::Open, crop: None });
            continue;
        }
        let line = line.split(" #").next().unwrap_or(line).trim();
        let Some(g) = out.last_mut() else { return Err(format!("{n}: a line before any [code]")) };
        let (key, value) = line.split_once(char::is_whitespace).map_or((line, ""), |(k, v)| (k, v.trim()));
        let art_of = |name: &str| frames(name).ok_or_else(|| format!("{n}: no sprite called {name}"));
        match key {
            "name" => g.name = value.to_string(),
            "art" => g.art = art_of(value)?,
            "joins" => {
                g.joins = Some(match value {
                    "across" => Joins::Across,
                    "down" => Joins::Down,
                    _ => return Err(format!("{n}: joins is across or down")),
                })
            }
            "cell" => {
                g.cell = match value {
                    "open" => Cell::Open,
                    "road" => Cell::Road,
                    "path" => Cell::Path,
                    "field" => Cell::Taken,
                    _ => return Err(format!("{n}: cell is open, road, path or field")),
                }
            }
            "crop" => {
                let words: Vec<&str> = value.split_whitespace().collect();
                let &[top, middle, bottom] = &words[..] else { return Err(format!("{n}: crop is TOP MIDDLE BOTTOM, three sprites")) };
                g.crop = Some((art_of(top)?, art_of(middle)?[0], art_of(bottom)?[0]));
            }
            _ => return Err(format!("{n}: {key} isn't something ground says")),
        }
    }
    if out.is_empty() {
        return Err("no ground".into());
    }
    if out.iter().any(|g| g.cell == Cell::Path && g.art.len() < 16) {
        return Err("a path's art has 16 frames, one for each way its neighbours join".into());
    }
    Ok(out)
}

/// The ground a map can paint, read the first time it's asked for.
pub fn ground() -> &'static [Ground] {
    static G: OnceLock<Vec<Ground>> = OnceLock::new();
    G.get_or_init(|| parse(crate::data::ground()).unwrap_or_else(|e| panic!("ground.txt:{e}")))
}

/// The ground a code paints, else the first kind.
pub fn of(code: u8) -> &'static Ground {
    let g = ground();
    g.iter().find(|k| k.code == code).unwrap_or(&g[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ground_reads_and_mistakes_name_their_line() {
        let g = ground();
        assert_eq!(g.iter().map(|k| k.code as char).collect::<String>(), ".,gspcvh=");
        assert_eq!((of(b'v').joins, of(b'c').crop.map(|c| c.1)), (Some(Joins::Across), Some(art::CORN)));
        assert_eq!(of(b'?').code, b'.', "an unknown code lays the first ground");
        assert!(parse("[ab]\n").unwrap_err().starts_with("1: a ground's code"));
        assert!(parse("[x]\nart NOPE").unwrap_err().starts_with("2: no sprite"));
        assert!(parse("[x]\ncell lava").unwrap_err().starts_with("2: cell is"));
        assert!(parse("[x]\nart DIRT\ncell path").unwrap_err().contains("16 frames"));
    }
}
