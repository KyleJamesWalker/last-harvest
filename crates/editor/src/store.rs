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

    /// The saved copy if there is one, else the packed frames.
    fn load(&self, name: &str) -> Option<Sprite> {
        Sprite::load_from(name, |file| self.read(name, file))
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use fs::FsStore;

#[cfg(not(target_arch = "wasm32"))]
mod fs {
    use std::path::{Path, PathBuf};

    use last_harvest_pack::card;

    use super::Store;
    use crate::history::{Commit, Repo};
    use crate::sprite::{self, CARD_FRAMES, CARD_PART_FRAMES, Frame, Sprite};

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
            sprite::card_id(name).map(|id| self.cards.join(format!("{id}.png")))
        }

        /// A card's text and its frames in sheet order.
        fn card(&self, path: &Path) -> Option<(String, Vec<Frame>)> {
            let c = card::read(&path.display().to_string(), &std::fs::read(path).ok()?).ok()?;
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

        /// Writes one part's frames into its row of the card, and the card's text; a card not yet on disk starts
        /// from the template. A part of a new size resizes the rest of the card to match.
        fn save_card(&self, path: &Path, sprite: &Sprite) -> Result<(), String> {
            let (id, row) = sprite::card_part(&sprite.name).ok_or("not a card's part")?;
            let text = sprite.card.as_deref().ok_or("a card needs its enemy text")?;
            let fields = last_harvest_game::card_fields(text)?;
            if fields.id != id {
                return Err(format!("the card's id is [{}], but {} needs [{id}]", fields.id, sprite.name));
            }
            let first = sprite.frames.first().ok_or("a card's part has frames")?;
            let (w, h) = (first.w, first.h);
            if sprite.frames.len() != CARD_PART_FRAMES || sprite.frames.iter().any(|f| (f.w, f.h) != (w, h)) {
                return Err(format!("each part of a card has {CARD_PART_FRAMES} frames, all one size"));
            }
            let mut frames = self.card(path).map_or_else(sprite::card_template, |(_, frames)| frames);
            for f in &mut frames {
                if (f.w, f.h) != (w, h) {
                    *f = f.resized(w, h);
                }
            }
            frames[row * CARD_PART_FRAMES..(row + 1) * CARD_PART_FRAMES].clone_from_slice(&sprite.frames);
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
            let bytes = card::write(text, (w * 3) as u32, (h * 5) as u32, &sheet)?;
            std::fs::create_dir_all(&self.cards).map_err(|e| format!("{}: {e}", self.cards.display()))?;
            std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
        }
    }

    impl Store for FsStore {
        fn names(&self) -> Vec<(String, bool)> {
            let mut names = sprite::catalog(&self.dir);
            let mut cards: Vec<String> = std::fs::read_dir(&self.cards)
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|e| e.file_name().into_string().ok()?.strip_suffix(".png").map(str::to_string))
                .collect();
            cards.sort();
            names.extend(cards.iter().filter_map(|id| sprite::card_names(id)).flatten().map(|n| (n, true)));
            names
        }

        fn read(&self, name: &str, file: &str) -> Option<Vec<u8>> {
            let Some(path) = self.card_path(name) else { return std::fs::read(self.dir.join(name).join(file)).ok() };
            let (text, frames) = self.card(&path)?;
            if file == sprite::CARD_FILE {
                return Some(text.into_bytes());
            }
            let (_, row) = sprite::card_part(name)?;
            let k: usize = file.strip_suffix(".png")?.parse().ok().filter(|&k| k < CARD_PART_FRAMES)?;
            frames.get(row * CARD_PART_FRAMES + k).map(sprite::encode_png)
        }

        fn save(&self, sprite: &Sprite) -> Result<(), String> {
            match self.card_path(&sprite.name) {
                Some(path) => self.save_card(&path, sprite),
                None => sprite.save(&self.dir),
            }
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
