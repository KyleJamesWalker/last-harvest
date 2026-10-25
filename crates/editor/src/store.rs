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
    use std::path::PathBuf;

    use super::Store;
    use crate::history::{Commit, Repo};
    use crate::sprite::{self, Sprite};

    /// The sprites folder and the game's catalog on this machine.
    pub struct FsStore {
        pub dir: PathBuf,
        pub catalog: PathBuf,
    }

    impl Default for FsStore {
        fn default() -> Self {
            Self { dir: sprite::folder(), catalog: sprite::catalog_file() }
        }
    }

    impl Store for FsStore {
        fn names(&self) -> Vec<(String, bool)> {
            sprite::catalog(&self.dir)
        }

        fn read(&self, name: &str, file: &str) -> Option<Vec<u8>> {
            std::fs::read(self.dir.join(name).join(file)).ok()
        }

        fn save(&self, sprite: &Sprite) -> Result<(), String> {
            sprite.save(&self.dir)
        }

        fn stand(&self, name: &str) -> Option<String> {
            sprite::read_stand(&self.dir, &self.catalog, name)
        }

        fn save_stand(&self, name: &str, how: &str) -> Result<(), String> {
            sprite::save_stand(&self.dir, &self.catalog, name, how)
        }

        fn history(&self, name: &str) -> Option<(Vec<Commit>, bool)> {
            let repo = Repo::open(&self.dir)?;
            Some((repo.log(name), repo.changed(name)))
        }

        fn version(&self, id: &str, name: &str, file: &str) -> Option<Vec<u8>> {
            Repo::open(&self.dir)?.read(id, name, file)
        }
    }
}
