//! Enemy cards kept in the browser's local storage, for the mod editor: nothing leaves the page but what
//! Export hands over.

use web_sys::Storage;

use sprite_editor::history::Commit;
use sprite_editor::sprite::{self, CardKind, Sprite};
use sprite_editor::store::{Store, cards, props};

/// Enemy and player cards keep to their own keys.
fn key(kind: CardKind, id: &str) -> String {
    match kind {
        CardKind::Enemy => format!("last-harvest-card:{id}"),
        CardKind::Player => format!("last-harvest-player:{id}"),
    }
}

/// An enemy card's own death sound, kept as its file name and Base64 bytes.
fn sound_key(id: &str) -> String {
    format!("last-harvest-sound:{id}")
}

/// Maps keep to keys of their own, by id.
const MAP_KEY: &str = "last-harvest-map:";
const PROP_KEY: &str = "last-harvest-prop:";
/// A data file the mod changes, by its file name.
const TABLE_KEY: &str = "last-harvest-table:";

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

    /// The ids kept under keys starting `prefix`, sorted.
    fn keyed(&self, prefix: &str) -> Vec<String> {
        let n = self.storage.length().unwrap_or(0);
        let mut ids: Vec<String> = (0..n).filter_map(|i| self.storage.key(i).ok().flatten()?.strip_prefix(prefix).map(str::to_string)).collect();
        ids.sort();
        ids
    }

    /// The kind and id of the card the part `name` belongs to.
    fn card_of(name: &str) -> Option<(CardKind, String)> {
        Some((sprite::card_kind(name)?, sprite::card_id(name)?))
    }
}

impl Store for LocalStore {
    fn names(&self) -> Vec<(String, bool)> {
        let mut names: Vec<(String, bool)> = [CardKind::Enemy, CardKind::Player].into_iter().flat_map(|k| cards::listing(k, self.ids(k))).collect();
        names.extend(self.keyed(PROP_KEY).iter().map(|id| (props::name(id), true)));
        names
    }

    fn read(&self, name: &str, file: &str) -> Option<Vec<u8>> {
        if let Some(id) = props::id(name) {
            return props::file(&self.prop_bytes(&id)?, file);
        }
        let (kind, id) = Self::card_of(name)?;
        cards::file(&self.get(kind, &id)?, name, file)
    }

    fn save(&self, sprite: &Sprite) -> Result<(), String> {
        if let Some(id) = props::id(&sprite.name) {
            return self.put_prop(&id, &props::from_sprite(sprite)?);
        }
        let (kind, id) = Self::card_of(&sprite.name).ok_or("the mod editor keeps cards, props and maps only")?;
        let bytes = cards::with_part(self.get(kind, &id).as_deref(), sprite)?;
        self.put(kind, &id, &bytes)
    }

    fn stand(&self, name: &str) -> Option<String> {
        match props::id(name) {
            Some(id) => props::stand(&self.prop_bytes(&id)?),
            None => Some("figure".into()),
        }
    }

    fn save_stand(&self, name: &str, how: &str) -> Result<(), String> {
        let Some(id) = props::id(name) else { return Ok(()) };
        let bytes = self.prop_bytes(&id).ok_or("save the prop first")?;
        self.put_prop(&id, &props::with_stand(&bytes, how)?)
    }

    fn prop_bytes(&self, id: &str) -> Option<Vec<u8>> {
        cards::from_text(&self.storage.get_item(&format!("{PROP_KEY}{id}")).ok()??)
    }

    fn put_prop(&self, id: &str, bytes: &[u8]) -> Result<(), String> {
        last_harvest_pack::prop::read(id, bytes)?;
        self.storage.set_item(&format!("{PROP_KEY}{id}"), &cards::to_text(bytes)).map_err(|_| "the browser's storage for this page is full".into())
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

    fn all_cards(&self) -> Vec<(String, Vec<u8>)> {
        let mut out = Vec::new();
        for kind in [CardKind::Enemy, CardKind::Player] {
            let mut ids = self.ids(kind);
            ids.sort();
            out.extend(ids.into_iter().filter_map(|id| Some((format!("{id}{}", kind.extension()), self.get(kind, &id)?))));
        }
        out
    }

    fn card_sound(&self, id: &str) -> Option<(String, Vec<u8>)> {
        let text = self.storage.get_item(&sound_key(id)).ok()??;
        let (file, bytes) = text.split_once(':')?;
        Some((file.to_string(), cards::from_text(bytes)?))
    }

    fn set_card_sound(&self, id: &str, file: &str, bytes: &[u8]) -> Result<(), String> {
        self.storage.set_item(&sound_key(id), &format!("{file}:{}", cards::to_text(bytes))).map_err(|_| "the browser's storage for this page is full".into())
    }

    fn remove_card_sound(&self, id: &str) -> Result<(), String> {
        self.storage.remove_item(&sound_key(id)).map_err(|_| "the browser wouldn't remove it".to_string())
    }

    fn all_sounds(&self) -> Vec<(String, Vec<u8>)> {
        let mut ids = self.ids(CardKind::Enemy);
        ids.sort();
        ids.iter().filter_map(|id| self.card_sound(id)).collect()
    }

    fn delete_card(&self, name: &str) -> Result<(), String> {
        let (kind, id) = Self::card_of(name).ok_or("not a card")?;
        self.storage.remove_item(&key(kind, &id)).map_err(|_| "the browser wouldn't remove it".to_string())?;
        if kind == CardKind::Enemy {
            let _ = self.storage.remove_item(&sound_key(&id));
        }
        Ok(())
    }

    fn map_ids(&self) -> Vec<String> {
        self.keyed(MAP_KEY)
    }

    fn read_map(&self, id: &str) -> Option<String> {
        self.storage.get_item(&format!("{MAP_KEY}{id}")).ok()?
    }

    fn save_map(&self, id: &str, text: &str) -> Result<(), String> {
        self.storage.set_item(&format!("{MAP_KEY}{id}"), text).map_err(|_| "the browser's storage for this page is full".into())
    }

    fn read_table(&self, file: &str) -> Option<String> {
        let built_in = last_harvest_game::built_in_table(file)?;
        Some(self.storage.get_item(&format!("{TABLE_KEY}{file}")).ok().flatten().unwrap_or_else(|| built_in.to_string()))
    }

    /// Keeps a data file the mod changes; one the same as the game's own is dropped, so Export leaves it out.
    fn save_table(&self, file: &str, text: &str) -> Result<(), String> {
        last_harvest_game::check_table(file, text)?;
        let key = format!("{TABLE_KEY}{file}");
        if last_harvest_game::built_in_table(file) == Some(text) {
            let _ = self.storage.remove_item(&key);
            return Ok(());
        }
        self.storage.set_item(&key, text).map_err(|_| "the browser's storage for this page is full".into())
    }
}
