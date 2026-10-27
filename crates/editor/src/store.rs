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

    /// Takes in a card's PNG from elsewhere, as the mod editor's Import does, and returns its front part's name.
    fn import_card(&self, _file: &str, _bytes: &[u8]) -> Result<String, String> {
        Err("cards are added to the enemies and players folders here".into())
    }

    /// The PNG as saved of the card the part `name` belongs to, for the mod editor's Export.
    fn card_bytes(&self, _name: &str) -> Option<Vec<u8>> {
        None
    }

    /// Every card kept here, by its file name, such as `ghoul.enemy.png`, for the mod editor's Export all.
    fn all_cards(&self) -> Vec<(String, Vec<u8>)> {
        Vec::new()
    }

    /// An enemy card's own death sound, as its file name such as `kill-ghoul.ogg` and its bytes, kept beside the card
    /// by the mod editor.
    fn card_sound(&self, _id: &str) -> Option<(String, Vec<u8>)> {
        None
    }

    fn set_card_sound(&self, _id: &str, _file: &str, _bytes: &[u8]) -> Result<(), String> {
        Err("a card's death sound goes in the sounds folder here".into())
    }

    /// Every card's own death sound by file name, for the mod editor's Export all.
    fn all_sounds(&self) -> Vec<(String, Vec<u8>)> {
        Vec::new()
    }

    /// Removes the card the part `name` belongs to for good, as the mod editor's Delete card does.
    fn delete_card(&self, _name: &str) -> Result<(), String> {
        Err("cards are removed from the enemies and players folders here".into())
    }

    /// The saved copy if there is one, else the packed frames.
    fn load(&self, name: &str) -> Option<Sprite> {
        Sprite::load_from(name, |file| self.read(name, file))
    }
}

/// Enemy cards as the editor sees them: one PNG per card, opened as five part sprites of three frames each.
pub mod cards {
    use last_harvest_pack::card;

    use crate::sprite::{self, CARD_PART_FRAMES, CardKind, Frame, Sprite};

    /// A card's text and its five rows of frames: walking front, side and back, then flash and hurt.
    pub fn rows(kind: CardKind, bytes: &[u8]) -> Option<(String, [Vec<Frame>; 5])> {
        let c = card::read_as(kind, "card", bytes).ok()?;
        let [w, h] = c.frame.map(|v| v as usize);
        let walk = match kind {
            CardKind::Enemy => last_harvest_game::card_fields(&c.text).map_or(3, |f| f.walk),
            CardKind::Player => CARD_PART_FRAMES,
        };
        let cols = walk.max(3);
        let cell = |col: usize, row: usize| {
            let px = (0..h).flat_map(|y| (0..w).map(move |x| ((row * h + y) * w * cols + col * w + x) * 4)).map(|i| [c.sheet[i], c.sheet[i + 1], c.sheet[i + 2], c.sheet[i + 3]]).collect();
            Frame { w, h, px }
        };
        let rows = std::array::from_fn(|row| (0..if row < 3 { walk } else { CARD_PART_FRAMES }).map(|col| cell(col, row)).collect());
        Some((c.text, rows))
    }

    /// A file of a card's part, as `Store::read` hands it out: a frame of the part's row, or the card's text.
    pub fn file(bytes: &[u8], name: &str, file: &str) -> Option<Vec<u8>> {
        let (text, rows) = rows(sprite::card_kind(name)?, bytes)?;
        if file == sprite::CARD_FILE {
            return Some(text.into_bytes());
        }
        let (_, row) = sprite::card_part(name)?;
        let k: usize = file.strip_suffix(".png")?.parse().ok()?;
        rows[row].get(k).map(sprite::encode_png)
    }

    /// The card with one part's frames and the card's text written in; a card that doesn't exist yet starts from
    /// the template. A part of a new size resizes the rest of the card to match, and a walking part with a new
    /// number of frames gives the other two walking parts as many, repeating their last frame or dropping the extra.
    pub fn with_part(existing: Option<&[u8]>, part: &Sprite) -> Result<Vec<u8>, String> {
        let (id, row) = sprite::card_part(&part.name).ok_or("not a card's part")?;
        let kind = sprite::card_kind(&part.name).ok_or("not a card's part")?;
        let text = part.card.as_deref().ok_or("a card needs its text")?;
        let card_id = match kind {
            CardKind::Enemy => last_harvest_game::card_fields(text)?.id,
            CardKind::Player => last_harvest_game::player_fields(text)?.id,
        };
        if card_id != id {
            return Err(format!("the card's id is [{card_id}], but {} needs [{id}]", part.name));
        }
        let first = part.frames.first().ok_or("a card's part has frames")?;
        let (w, h) = (first.w, first.h);
        let walking = row < 3 && kind == CardKind::Enemy;
        let fits = if walking { (1..=last_harvest_game::MAX_WALK).contains(&part.frames.len()) } else { part.frames.len() == CARD_PART_FRAMES };
        if !fits || part.frames.iter().any(|f| (f.w, f.h) != (w, h)) {
            return Err(format!("an enemy's walking part has 1 to {} frames and every other part {CARD_PART_FRAMES}, all one size", last_harvest_game::MAX_WALK));
        }
        if w > sprite::CARD_MAX_FRAME || h > sprite::CARD_MAX_FRAME {
            return Err(format!("a card's frames are at most {0}x{0} pixels", sprite::CARD_MAX_FRAME));
        }
        let mut rows = match existing {
            Some(bytes) => rows(kind, bytes).ok_or("the saved card can't be read, so saving would lose its other parts")?.1,
            None => {
                let t = sprite::card_template();
                std::array::from_fn(|r| t[r * CARD_PART_FRAMES..(r + 1) * CARD_PART_FRAMES].to_vec())
            }
        };
        for f in rows.iter_mut().flatten() {
            if (f.w, f.h) != (w, h) {
                *f = f.resized(w, h);
            }
        }
        let walk = if row < 3 { part.frames.len() } else { rows[0].len() };
        for r in rows.iter_mut().take(3) {
            let last = r.last().cloned().unwrap_or_else(|| Frame::blank(w, h));
            r.resize(walk, last);
        }
        rows[row] = part.frames.clone();
        let text = match kind {
            CardKind::Enemy => last_harvest_game::set_card_field(text, "walk", (walk != 3).then(|| walk.to_string()).as_deref()),
            CardKind::Player => text.to_string(),
        };
        let cols = walk.max(3);
        let mut sheet = vec![0u8; w * cols * h * 5 * 4];
        for (r, frames) in rows.iter().enumerate() {
            for (col, f) in frames.iter().enumerate() {
                for y in 0..h {
                    for x in 0..w {
                        let i = ((r * h + y) * w * cols + col * w + x) * 4;
                        sheet[i..i + 4].copy_from_slice(&f.px[y * w + x]);
                    }
                }
            }
        }
        card::write_as(kind, &text, (w * cols) as u32, (h * 5) as u32, &sheet)
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

    /// The part names of every card of `kind` among `ids`, each with a saved copy, sorted by card.
    pub fn listing(kind: CardKind, mut ids: Vec<String>) -> Vec<(String, bool)> {
        ids.sort();
        ids.iter().filter_map(|id| sprite::card_names_of(kind, id)).flatten().map(|n| (n, true)).collect()
    }

    /// A card's kind and id, read from a PNG and the name it came with: the name's ending when it has one, else
    /// whichever kind its text is.
    pub fn identify(file: &str, bytes: &[u8]) -> Result<(CardKind, String), String> {
        let kinds = match CardKind::of(file) {
            Some(k) => vec![k],
            None => vec![CardKind::Enemy, CardKind::Player],
        };
        let mut last = String::new();
        for kind in kinds {
            match card::read_as(kind, file, bytes) {
                Ok(c) => {
                    let id = match kind {
                        CardKind::Enemy => last_harvest_game::card_fields(&c.text)?.id,
                        CardKind::Player => last_harvest_game::player_fields(&c.text)?.id,
                    };
                    if c.frame[0] > 64 || c.frame[1] > 64 {
                        return Err("its frames are over 64 pixels, more than the game takes".into());
                    }
                    return Ok((kind, id));
                }
                Err(e) => last = e,
            }
        }
        Err(last)
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

    #[test]
    fn identify_tells_enemy_and_player_cards_apart() {
        use super::cards::identify;
        use crate::sprite::CardKind;
        use last_harvest_pack::card::{Kind, write_as};
        let sheet = vec![0u8; 24 * 40 * 4];
        let enemy = write_as(Kind::Enemy, "[bat]", 24, 40, &sheet).unwrap();
        let player = write_as(Kind::Player, "[jo]\nname Jo", 24, 40, &sheet).unwrap();
        assert_eq!(identify("x.png", &enemy), Ok((CardKind::Enemy, "bat".into())));
        assert_eq!(identify("download.png", &player), Ok((CardKind::Player, "jo".into())));
        assert!(identify("jo.enemy.png", &player).is_err(), "a name's ending is believed");
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use fs::FsStore;

#[cfg(not(target_arch = "wasm32"))]
mod fs {
    use std::path::PathBuf;

    use super::{Store, cards};
    use crate::history::{Commit, Repo};
    use crate::sprite::{self, CardKind, Sprite};

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
        /// Enemy cards keep to `cards`, and player cards to a `players` folder beside it.
        fn folder(&self, kind: CardKind) -> PathBuf {
            match kind {
                CardKind::Enemy => self.cards.clone(),
                CardKind::Player => self.cards.with_file_name("players"),
            }
        }

        fn card_path(&self, name: &str) -> Option<PathBuf> {
            let (kind, id) = (sprite::card_kind(name)?, sprite::card_id(name)?);
            Some(self.folder(kind).join(format!("{id}{}", kind.extension())))
        }

        fn ids(&self, kind: CardKind) -> Vec<String> {
            let found = std::fs::read_dir(self.folder(kind)).into_iter().flatten().flatten();
            found.filter_map(|e| e.file_name().into_string().ok()?.strip_suffix(kind.extension()).map(str::to_string)).collect()
        }
    }

    impl Store for FsStore {
        fn names(&self) -> Vec<(String, bool)> {
            let mut names = sprite::catalog(&self.dir);
            for kind in [CardKind::Enemy, CardKind::Player] {
                names.extend(cards::listing(kind, self.ids(kind)));
            }
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
            let dir = path.parent().unwrap_or(&self.cards);
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
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

        fn import_card(&self, file: &str, bytes: &[u8]) -> Result<String, String> {
            let (kind, id) = cards::identify(file, bytes)?;
            let names = sprite::card_names_of(kind, &id).ok_or_else(|| format!("its id [{id}] needs to be letters, digits and _"))?;
            let dir = self.folder(kind);
            std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            let path = dir.join(format!("{id}{}", kind.extension()));
            std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(names[0].clone())
        }

        fn delete_card(&self, name: &str) -> Result<(), String> {
            let path = self.card_path(name).ok_or("not a card")?;
            std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))
        }
    }
}
