//! Enemy cards kept in the browser's local storage, for the mod editor: nothing leaves the page but what
//! Export hands over.

use last_harvest_pack::card;
use web_sys::Storage;

use sprite_editor::history::Commit;
use sprite_editor::sprite::{self, Sprite};
use sprite_editor::store::{Store, cards};

const KEY: &str = "last-harvest-card:";

pub struct LocalStore {
    storage: Storage,
}

impl LocalStore {
    pub fn open() -> Result<Self, String> {
        let storage = web_sys::window().and_then(|w| w.local_storage().ok().flatten()).ok_or("this browser keeps no local storage")?;
        Ok(Self { storage })
    }

    fn get(&self, id: &str) -> Option<Vec<u8>> {
        cards::from_text(&self.storage.get_item(&format!("{KEY}{id}")).ok()??)
    }

    fn put(&self, id: &str, bytes: &[u8]) -> Result<(), String> {
        self.storage.set_item(&format!("{KEY}{id}"), &cards::to_text(bytes)).map_err(|_| "the browser's storage for this page is full".into())
    }

    fn ids(&self) -> Vec<String> {
        let n = self.storage.length().unwrap_or(0);
        (0..n).filter_map(|i| self.storage.key(i).ok().flatten()?.strip_prefix(KEY).map(str::to_string)).collect()
    }
}

impl Store for LocalStore {
    fn names(&self) -> Vec<(String, bool)> {
        cards::listing(self.ids())
    }

    fn read(&self, name: &str, file: &str) -> Option<Vec<u8>> {
        cards::file(&self.get(&sprite::card_id(name)?)?, name, file)
    }

    fn save(&self, sprite: &Sprite) -> Result<(), String> {
        let id = sprite::card_id(&sprite.name).ok_or("the mod editor keeps enemy cards only")?;
        let bytes = cards::with_part(self.get(&id).as_deref(), sprite)?;
        self.put(&id, &bytes)
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

    fn import_card(&self, bytes: &[u8]) -> Result<String, String> {
        let c = card::read("card", bytes)?;
        let id = last_harvest_game::card_fields(&c.text)?.id;
        if sprite::card_names(&id).is_none() {
            return Err(format!("its id [{id}] needs to be letters, digits and _ to open here"));
        }
        if c.frame[0] > 64 || c.frame[1] > 64 {
            return Err("its frames are over 64 pixels, more than the game takes".into());
        }
        self.put(&id, bytes)?;
        Ok(id)
    }

    fn card_bytes(&self, id: &str) -> Option<Vec<u8>> {
        self.get(id)
    }
}
