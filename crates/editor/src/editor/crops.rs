//! Crops: the three sprites a `ground.txt` crop line stacks down a run of field, named for where each stands, and
//! the open one stacked with the others in the previews as a map lays them.

use last_harvest_game::art::Art;

use super::{Editor, Frame};
use crate::table::Table;

/// Where each of a crop line's sprites stands in a north-south run.
const PLACES: [&str; 3] = ["north end", "middle", "south end"];

/// A ground code whose crop line names the open sprite.
pub(super) struct Crop {
    /// The ground code's section, such as `c`.
    pub code: String,
    /// Its `name` line, else its code.
    pub name: String,
    /// The crop line's sprites: top, middle and bottom.
    pub pieces: [String; 3],
    /// The others' saved frames, in the same order; none for the open sprite's own places.
    pub frames: [Vec<Frame>; 3],
}

impl Crop {
    /// The first ground code in a `ground.txt` whose crop line names `sprite`, without its frames.
    pub(super) fn find(ground: &str, sprite: &str) -> Option<Self> {
        let t = Table::new("ground.txt", ground);
        t.sections().into_iter().find_map(|code| {
            let entries = t.entries(&code);
            let line = entries.iter().find(|e| e.key == "crop")?;
            let pieces: [String; 3] = line.value.split_whitespace().map(str::to_string).collect::<Vec<_>>().try_into().ok()?;
            pieces.contains(&sprite.to_string()).then_some(())?;
            let name = entries.iter().find(|e| e.key == "name").map_or(code.clone(), |e| e.value.clone());
            Some(Self { code, name, pieces, frames: Default::default() })
        })
    }

    /// How many pictures of a place go to the previews: every frame of the top, which picks between them, and the
    /// first of the others, which a map lays alone.
    fn packed(&self, place: usize) -> usize {
        if place == 0 { self.frames[0].len() } else { self.frames[place].len().min(1) }
    }

    /// The line naming where `sprite` stands, such as "Crop: north end of a Corn run (ground c)".
    pub(super) fn role(&self, sprite: &str) -> String {
        let places: Vec<&str> = (0..3).filter(|&i| self.pieces[i] == sprite).map(|i| PLACES[i]).collect();
        format!("Crop: {} of a {} run (ground {})", places.join(" and "), self.name, self.code)
    }
}

impl Editor {
    /// Finds the crop the open sprite is part of in the store's `ground.txt`, and reads the other pieces as saved.
    pub(super) fn load_crop(&mut self) {
        let found = self.store.read_table("ground.txt").and_then(|t| Crop::find(&t, &self.sprite.name)).filter(|_| !self.none_open());
        self.crop = found.map(|mut c| {
            c.frames = c.pieces.clone().map(|p| if p == self.sprite.name { Vec::new() } else { self.store.load(&p).map(|s| s.frames).unwrap_or_default() });
            c
        });
    }

    /// The line naming where the open sprite stands in its crop, if it is part of one.
    pub fn crop_role(&self) -> Option<String> {
        self.crop.as_ref().map(|c| c.role(&self.sprite.name))
    }

    /// The other pieces' pictures to pack for the previews after the sprite's own.
    pub(super) fn crop_surfaces(&self) -> Vec<Frame> {
        let Some(c) = &self.crop else { return Vec::new() };
        (0..3).flat_map(|i| c.frames[i].iter().take(c.packed(i)).cloned()).collect()
    }

    /// The crop as the previews stand it: the top's frames, the middle and the bottom, from the open sprite's live
    /// frames and the others as packed; None until they are.
    pub fn crop_preview(&self) -> Option<(Vec<Art>, Art, Art)> {
        let c = self.crop.as_ref().filter(|_| self.map.is_none() && self.data.is_none())?;
        let own = self.live.get(..self.sprite.frames.len())?;
        let mut at = self.crop_at?;
        let mut arts: Vec<Vec<Art>> = Vec::new();
        for i in 0..3 {
            if c.pieces[i] == self.sprite.name {
                arts.push(if i == 0 { own.to_vec() } else { own.get(..1)?.to_vec() });
                continue;
            }
            let n = c.packed(i);
            arts.push(self.live.get(at..at + n)?.to_vec());
            at += n;
        }
        let [top, middle, bottom] = <[Vec<Art>; 3]>::try_from(arts).ok()?;
        Some((top.clone(), *middle.first()?, *bottom.first()?)).filter(|_| !top.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GROUND: &str = "[.]\nname Dirt\nart DIRT\n\n[w]\nname Wheat\nart SOIL\ncell field\ncrop PROP_WHEAT_TOP PROP_WHEAT PROP_WHEAT_BOTTOM\n";

    #[test]
    fn a_crop_line_names_each_pieces_place_in_its_run() {
        let c = Crop::find(GROUND, "PROP_WHEAT").unwrap();
        assert_eq!((c.code.as_str(), c.name.as_str()), ("w", "Wheat"));
        assert_eq!(c.role("PROP_WHEAT_TOP"), "Crop: north end of a Wheat run (ground w)");
        assert_eq!(c.role("PROP_WHEAT"), "Crop: middle of a Wheat run (ground w)");
        assert_eq!(c.role("PROP_WHEAT_BOTTOM"), "Crop: south end of a Wheat run (ground w)");
        assert!(Crop::find(GROUND, "SOIL").is_none(), "a ground's tile isn't a crop");
        assert_eq!(Crop::find("[x]\ncrop A A B\n", "A").unwrap().role("A"), "Crop: north end and middle of a x run (ground x)");
    }
}
