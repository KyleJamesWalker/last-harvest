//! Where sprites are read from and saved to: the sprites folder on this machine, or a server's over HTTP.

use crate::history::Commit;
use crate::sprite::Sprite;

pub trait Store {
    /// Every sprite to choose from, with whether each has a saved copy.
    fn names(&self) -> Vec<(String, bool)>;
    /// A file in a sprite's folder.
    fn read(&self, name: &str, file: &str) -> Option<Vec<u8>>;
    fn save(&self, sprite: &Sprite) -> Result<(), String>;
    /// How a sprite stands, as `sprite::read_stand` finds it.
    fn stand(&self, name: &str) -> Option<String>;
    fn save_stand(&self, name: &str, how: &str) -> Result<(), String>;
    /// The commits that touched a sprite, newest first, and whether its files differ from the last;
    /// None when the sprites aren't in a git repository.
    fn history(&self, name: &str) -> Option<(Vec<Commit>, bool)>;
    /// A file of a sprite's folder as it was at a commit.
    fn version(&self, id: &str, name: &str, file: &str) -> Option<Vec<u8>>;

    /// Takes in a card's PNG from elsewhere, as the mod editor's Import does, and returns its id.
    fn import_card(&self, _bytes: &[u8]) -> Result<String, String> {
        Err("cards are added to the enemies folder here".into())
    }

    /// A card's PNG as saved, for the mod editor's Export.
    fn card_bytes(&self, _id: &str) -> Option<Vec<u8>> {
        None
    }

    /// The saved copy if there is one, else the packed frames.
    fn load(&self, name: &str) -> Option<Sprite> {
        Sprite::load_from(name, |file| self.read(name, file))
    }
}

/// Enemy cards as the editor sees them: one PNG per card, opened as five part sprites of three frames each.
pub mod cards {
    use last_harvest_pack::card;

    use crate::sprite::{self, CARD_FRAMES, CARD_PART_FRAMES, Frame, Sprite};

    /// A card's text and its 15 frames in sheet order.
    pub fn frames(bytes: &[u8]) -> Option<(String, Vec<Frame>)> {
        let c = card::read("card", bytes).ok()?;
        let [w, h] = c.frame.map(|v| v as usize);
        let frames = (0..CARD_FRAMES)
            .map(|k| {
                let (col, row) = (k % 3, k / 3);
                let px = (0..h).flat_map(|y| (0..w).map(move |x| ((row * h + y) * w * 3 + col * w + x) * 4)).map(|i| [c.sheet[i], c.sheet[i + 1], c.sheet[i + 2], c.sheet[i + 3]]).collect();
                Frame { w, h, px }
            })
            .collect();
        Some((c.text, frames))
    }

    /// A file of a card's part, as `Store::read` hands it out: a frame of the part's row, or the card's text.
    pub fn file(bytes: &[u8], name: &str, file: &str) -> Option<Vec<u8>> {
        let (text, frames) = frames(bytes)?;
        if file == sprite::CARD_FILE {
            return Some(text.into_bytes());
        }
        let (_, row) = sprite::card_part(name)?;
        let k: usize = file.strip_suffix(".png")?.parse().ok().filter(|&k| k < CARD_PART_FRAMES)?;
        frames.get(row * CARD_PART_FRAMES + k).map(sprite::encode_png)
    }

    /// The card with one part's frames and the card's text written in; a card that doesn't exist yet starts from
    /// the template. A part of a new size resizes the rest of the card to match.
    pub fn with_part(existing: Option<&[u8]>, part: &Sprite) -> Result<Vec<u8>, String> {
        let (id, row) = sprite::card_part(&part.name).ok_or("not a card's part")?;
        let text = part.card.as_deref().ok_or("a card needs its enemy text")?;
        let fields = last_harvest_game::card_fields(text)?;
        if fields.id != id {
            return Err(format!("the card's id is [{}], but {} needs [{id}]", fields.id, part.name));
        }
        let first = part.frames.first().ok_or("a card's part has frames")?;
        let (w, h) = (first.w, first.h);
        if part.frames.len() != CARD_PART_FRAMES || part.frames.iter().any(|f| (f.w, f.h) != (w, h)) {
            return Err(format!("each part of a card has {CARD_PART_FRAMES} frames, all one size"));
        }
        let mut frames = existing.and_then(frames).map_or_else(sprite::card_template, |(_, frames)| frames);
        for f in &mut frames {
            if (f.w, f.h) != (w, h) {
                *f = f.resized(w, h);
            }
        }
        frames[row * CARD_PART_FRAMES..(row + 1) * CARD_PART_FRAMES].clone_from_slice(&part.frames);
        let mut sheet = vec![0u8; w * 3 * h * 5 * 4];
        for (k, f) in frames.iter().enumerate() {
            let (col, row) = (k % 3, k / 3);
            for y in 0..h {
                for x in 0..w {
                    let i = ((row * h + y) * w * 3 + col * w + x) * 4;
                    sheet[i..i + 4].copy_from_slice(&f.px[y * w + x]);
                }
            }
        }
        card::write(text, (w * 3) as u32, (h * 5) as u32, &sheet)
    }

    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    /// A card as Base64, for a store such as the browser's local storage that keeps only text.
    pub fn to_text(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for chunk in bytes.chunks(3) {
            let n = chunk.iter().enumerate().fold(0u32, |n, (i, &b)| n | (b as u32) << (16 - 8 * i));
            for i in 0..4 {
                out.push(if i <= chunk.len() { ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
            }
        }
        out
    }

    pub fn from_text(text: &str) -> Option<Vec<u8>> {
        let digit = |c: u8| ALPHABET.iter().position(|&a| a == c).map(|v| v as u32);
        let mut out = Vec::with_capacity(text.len() / 4 * 3);
        for chunk in text.as_bytes().chunks(4) {
            let pad = chunk.iter().filter(|&&c| c == b'=').count();
            if pad > 2 {
                return None;
            }
            let n = chunk.iter().take(4 - pad).enumerate().try_fold(0u32, |n, (i, &c)| Some(n | digit(c)? << (18 - 6 * i)))?;
            out.extend((0..3 - pad).map(|i| (n >> (16 - 8 * i)) as u8));
        }
        Some(out)
    }

    /// The part names of every card among `ids`, each with a saved copy, sorted by card.
    pub fn listing(mut ids: Vec<String>) -> Vec<(String, bool)> {
        ids.sort();
        ids.iter().filter_map(|id| sprite::card_names(id)).flatten().map(|n| (n, true)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::cards::{from_text, to_text};

    #[test]
    fn base64_round_trips_every_length() {
        for n in 0..10 {
            let bytes: Vec<u8> = (0..n).map(|i| (i * 37 + 200) as u8).collect();
            assert_eq!(from_text(&to_text(&bytes)), Some(bytes));
        }
        assert_eq!(to_text(b"Man"), "TWFu");
        assert_eq!(to_text(b"Ma"), "TWE=");
        assert_eq!(from_text("TQ=="), Some(b"M".to_vec()));
        assert_eq!(from_text("T!=="), None);
        assert_eq!(from_text("===="), None);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use fs::FsStore;

#[cfg(not(target_arch = "wasm32"))]
mod fs {
    use std::path::PathBuf;

    use last_harvest_pack::card;

    use super::{Store, cards};
    use crate::history::{Commit, Repo};
    use crate::sprite::{self, Sprite};

    /// The sprites folder, the game's catalog and the enemy cards folder on this machine.
    pub struct FsStore {
        pub dir: PathBuf,
        pub catalog: PathBuf,
        pub cards: PathBuf,
    }

    impl Default for FsStore {
        fn default() -> Self {
            let dir = sprite::folder();
            Self { cards: dir.with_file_name("enemies"), dir, catalog: sprite::catalog_file() }
        }
    }

    impl FsStore {
        fn card_path(&self, name: &str) -> Option<PathBuf> {
            sprite::card_id(name).map(|id| self.cards.join(format!("{id}{}", card::EXTENSION)))
        }
    }

    impl Store for FsStore {
        fn names(&self) -> Vec<(String, bool)> {
            let mut names = sprite::catalog(&self.dir);
            let ids = std::fs::read_dir(&self.cards)
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|e| e.file_name().into_string().ok()?.strip_suffix(card::EXTENSION).map(str::to_string))
                .collect();
            names.extend(cards::listing(ids));
            names
        }

        fn read(&self, name: &str, file: &str) -> Option<Vec<u8>> {
            match self.card_path(name) {
                Some(path) => cards::file(&std::fs::read(path).ok()?, name, file),
                None => std::fs::read(self.dir.join(name).join(file)).ok(),
            }
        }

        fn save(&self, sprite: &Sprite) -> Result<(), String> {
            let Some(path) = self.card_path(&sprite.name) else { return sprite.save(&self.dir) };
            let bytes = cards::with_part(std::fs::read(&path).ok().as_deref(), sprite)?;
            std::fs::create_dir_all(&self.cards).map_err(|e| format!("{}: {e}", self.cards.display()))?;
            std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))
        }

        fn stand(&self, name: &str) -> Option<String> {
            if sprite::card_id(name).is_some() {
                return Some("figure".into());
            }
            sprite::read_stand(&self.dir, &self.catalog, name)
        }

        fn save_stand(&self, name: &str, how: &str) -> Result<(), String> {
            if sprite::card_id(name).is_some() {
                return Ok(());
            }
            sprite::save_stand(&self.dir, &self.catalog, name, how)
        }

        fn history(&self, name: &str) -> Option<(Vec<Commit>, bool)> {
            sprite::card_id(name).is_none().then_some(())?;
            let repo = Repo::open(&self.dir)?;
            Some((repo.log(name), repo.changed(name)))
        }

        fn version(&self, id: &str, name: &str, file: &str) -> Option<Vec<u8>> {
            Repo::open(&self.dir)?.read(id, name, file)
        }
    }
}
