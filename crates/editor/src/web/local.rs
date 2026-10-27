//! Enemy cards kept in the browser's local storage, for the mod editor: nothing leaves the page but what
//! Export hands over.

use web_sys::Storage;

use sprite_editor::history::Commit;
use sprite_editor::sprite::{self, CardKind, Sprite};
use sprite_editor::store::{Store, cards};

/// Enemy and player cards keep to their own keys.
fn key(kind: CardKind, id: &str) -> String {
    match kind {
        CardKind::Enemy => format!("last-harvest-card:{id}"),
        CardKind::Player => format!("last-harvest-player:{id}"),
    }
}

pub struct LocalStore {
    storage: Storage,
}

impl LocalStore {
    pub fn open() -> Result<Self, String> {
        let storage = web_sys::window().and_then(|w| w.local_storage().ok().flatten()).ok_or("this browser keeps no local storage")?;
        Ok(Self { storage })
    }

    fn get(&self, kind: CardKind, id: &str) -> Option<Vec<u8>> {
        cards::from_text(&self.storage.get_item(&key(kind, id)).ok()??)
    }

    fn put(&self, kind: CardKind, id: &str, bytes: &[u8]) -> Result<(), String> {
        self.storage.set_item(&key(kind, id), &cards::to_text(bytes)).map_err(|_| "the browser's storage for this page is full".into())
    }

    fn ids(&self, kind: CardKind) -> Vec<String> {
        let n = self.storage.length().unwrap_or(0);
        let prefix = key(kind, "");
        (0..n).filter_map(|i| self.storage.key(i).ok().flatten()?.strip_prefix(&prefix).map(str::to_string)).collect()
    }

    /// The kind and id of the card the part `name` belongs to.
    fn card_of(name: &str) -> Option<(CardKind, String)> {
        Some((sprite::card_kind(name)?, sprite::card_id(name)?))
    }
}

impl Store for LocalStore {
    fn names(&self) -> Vec<(String, bool)> {
        [CardKind::Enemy, CardKind::Player].into_iter().flat_map(|k| cards::listing(k, self.ids(k))).collect()
    }

    fn read(&self, name: &str, file: &str) -> Option<Vec<u8>> {
        let (kind, id) = Self::card_of(name)?;
        cards::file(&self.get(kind, &id)?, name, file)
    }

    fn save(&self, sprite: &Sprite) -> Result<(), String> {
        let (kind, id) = Self::card_of(&sprite.name).ok_or("the mod editor keeps cards only")?;
        let bytes = cards::with_part(self.get(kind, &id).as_deref(), sprite)?;
        self.put(kind, &id, &bytes)
    }

    fn stand(&self, _name: &str) -> Option<String> {
        Some("figure".into())
    }

    fn save_stand(&self, _name: &str, _how: &str) -> Result<(), String> {
        Ok(())
    }

    fn history(&self, _name: &str) -> Option<(Vec<Commit>, bool)> {
        None
    }

    fn version(&self, _id: &str, _name: &str, _file: &str) -> Option<Vec<u8>> {
        None
    }

    fn import_card(&self, file: &str, bytes: &[u8]) -> Result<String, String> {
        let (kind, id) = cards::identify(file, bytes)?;
        let names = sprite::card_names_of(kind, &id).ok_or_else(|| format!("its id [{id}] needs to be letters, digits and _ to open here"))?;
        self.put(kind, &id, bytes)?;
        Ok(names[0].clone())
    }

    fn card_bytes(&self, name: &str) -> Option<Vec<u8>> {
        let (kind, id) = Self::card_of(name)?;
        self.get(kind, &id)
    }

    fn delete_card(&self, name: &str) -> Result<(), String> {
        let (kind, id) = Self::card_of(name).ok_or("not a card")?;
        self.storage.remove_item(&key(kind, &id)).map_err(|_| "the browser wouldn't remove it".to_string())
    }
}
