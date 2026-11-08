//! Crops: the three sprites a `ground.txt` crop line stacks down a run of field, named for where each stands, and
//! the open one stacked with the others in the previews as a map lays them.

use last_harvest_game::art::Art;
use last_harvest_game::{CropShow, CropSway};

use super::{Editor, Frame};
use crate::table::Table;

/// Where each of a crop line's sprites stands in a north-south run.
const PLACES: [&str; 3] = ["north end", "middle", "south end"];

/// A ground code whose crop names the open sprite, on the map or in 3D.
pub(super) struct Crop {
    /// The ground code's section, such as `c`.
    pub code: String,
    /// Its `name` line, else its code.
    pub name: String,
    /// The crop line's sprites, top, middle and bottom, as the map draws them.
    pub flat: [String; 3],
    /// Its `crop_3d` line's sprites, else the crop line's, as they stand in 3D.
    pub standing: [String; 3],
    pub sway: CropSway,
    /// Cards each cell's crop stands one above another in 3D.
    pub tall: u8,
    /// Every other sprite of the crop with its saved frames, as many as the previews need, in packing order.
    pub others: Vec<(String, Vec<Frame>)>,
}

fn three(value: &str) -> Option<[String; 3]> {
    value.split_whitespace().map(str::to_string).collect::<Vec<_>>().try_into().ok()
}

impl Crop {
    /// The first ground code in a `ground.txt` whose crop names `sprite`, without the others' frames.
    pub(super) fn find(ground: &str, sprite: &str) -> Option<Self> {
        let t = Table::new("ground.txt", ground);
        t.sections().into_iter().find_map(|code| {
            let entries = t.entries(&code);
            let value = |key: &str| entries.iter().find(|e| e.key == key).map(|e| e.value.clone());
            let flat = three(&value("crop")?)?;
            let standing = value("crop_3d").and_then(|v| three(&v)).unwrap_or_else(|| flat.clone());
            flat.iter().chain(&standing).any(|p| p == sprite).then_some(())?;
            let sway = value("sway").and_then(|v| {
                let n: Vec<f32> = v.split_whitespace().filter_map(|w| w.parse().ok()).collect();
                (n.len() == 2).then(|| CropSway { degrees: n[0], seconds: n[1] })
            });
            let name = value("name").unwrap_or_else(|| code.clone());
            let tall = value("tall").and_then(|v| v.parse().ok()).unwrap_or(1);
            Some(Self { code, name, flat, standing, sway: sway.unwrap_or_default(), tall, others: Vec::new() })
        })
    }

    /// The crop's sprites but `sprite`, each once, with whether a top names it: a top packs every frame, which a
    /// map picks between, and the others their first.
    fn other_names(&self, sprite: &str) -> Vec<(String, bool)> {
        let mut out: Vec<(String, bool)> = Vec::new();
        for (i, p) in self.flat.iter().chain(&self.standing).enumerate().filter(|(_, p)| *p != sprite) {
            match out.iter_mut().find(|(n, _)| n == p) {
                Some((_, top)) => *top |= i % 3 == 0,
                None => out.push((p.clone(), i % 3 == 0)),
            }
        }
        out
    }

    /// The line naming where `sprite` stands, such as "Crop: north end of a Corn run (ground c)", saying where it
    /// stands only on the map or only in 3D.
    pub(super) fn role(&self, sprite: &str) -> String {
        let places = |pieces: &[String; 3]| (0..3).filter(|&i| pieces[i] == sprite).map(|i| PLACES[i]).collect::<Vec<_>>().join(" and ");
        let (flat, standing) = (places(&self.flat), places(&self.standing));
        let run = format!("of a {} run", self.name);
        let said = match (flat.is_empty(), standing.is_empty()) {
            _ if flat == standing => format!("{flat} {run}"),
            (true, _) => format!("{standing} {run} in 3D only"),
            (_, true) => format!("{flat} {run} on the map only"),
            _ => format!("{flat} {run}; in 3D, {standing}"),
        };
        format!("Crop: {said} (ground {})", self.code)
    }
}

impl Editor {
    /// Finds the crop the open sprite is part of in the store's `ground.txt`, and reads the other pieces as saved.
    pub(super) fn load_crop(&mut self) {
        let found = self.store.read_table("ground.txt").and_then(|t| Crop::find(&t, &self.sprite.name)).filter(|_| !self.none_open());
        self.crop = found.map(|mut c| {
            c.others = c.other_names(&self.sprite.name).into_iter().map(|(p, top)| {
                let frames = self.store.load(&p).map(|s| s.frames).unwrap_or_default();
                let n = if top { frames.len() } else { frames.len().min(1) };
                (p, frames.into_iter().take(n).collect())
            }).collect();
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
        c.others.iter().flat_map(|(_, f)| f.iter().cloned()).collect()
    }

    /// The crop as the previews stand it, on the map and in 3D, from the open sprite's live frames and the others
    /// as packed; None until they are.
    pub fn crop_preview(&self) -> Option<CropShow> {
        let c = self.crop.as_ref().filter(|_| self.map.is_none() && self.data.is_none())?;
        let own = self.live.get(..self.sprite.frames.len())?;
        let mut at = self.crop_at?;
        let mut arts: Vec<(&str, &[Art])> = vec![(&self.sprite.name, own)];
        for (name, frames) in &c.others {
            arts.push((name, self.live.get(at..at + frames.len())?));
            at += frames.len();
        }
        let of = |name: &String| arts.iter().find(|(n, _)| n == name).map(|(_, a)| *a).filter(|a| !a.is_empty());
        let pieces = |p: &[String; 3]| Some((of(&p[0])?.to_vec(), of(&p[1])?[0], of(&p[2])?[0]));
        Some(CropShow { flat: pieces(&c.flat)?, standing: pieces(&c.standing)?, sway: c.sway, tall: c.tall })
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

    #[test]
    fn a_crop_3d_line_says_where_a_piece_stands_only_on_the_map_or_only_in_3d() {
        let ground = "[c]\nname Corn\ncrop TOP CANOPY STALK\ncrop_3d TOP STALK STALK\nsway 4 3.5\ntall 3\n[s]\ncrop A B C\ncrop_3d A B D\n";
        let c = Crop::find(ground, "CANOPY").unwrap();
        assert_eq!(c.role("CANOPY"), "Crop: middle of a Corn run on the map only (ground c)");
        assert_eq!(c.role("STALK"), "Crop: south end of a Corn run; in 3D, middle and south end (ground c)");
        assert_eq!(c.role("TOP"), "Crop: north end of a Corn run (ground c)");
        assert_eq!((c.sway, c.tall), (CropSway { degrees: 4.0, seconds: 3.5 }, 3));
        assert_eq!(c.other_names("CANOPY"), [("TOP".to_string(), true), ("STALK".to_string(), false)]);
        let d = Crop::find(ground, "D").unwrap();
        assert_eq!((d.role("D"), d.sway, d.tall), ("Crop: south end of a s run in 3D only (ground s)".to_string(), CropSway::default(), 1));
    }
}
