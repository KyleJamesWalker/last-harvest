//! The save kept in the platform's cloud: fetched once at launch and merged into the game's, then stored on each write.

use last_harvest_game::{Game, Save};
use last_harvest_platform::CloudSaves;

use crate::log;

/// The save's name in the cloud, whatever the local file is called.
pub const NAME: &str = "save.txt";

#[derive(Debug, Default)]
pub struct CloudSave {
    asked: bool,
    answered: bool,
}

impl CloudSave {
    /// Asks for the cloud's copy; call once at launch.
    pub fn fetch(&mut self, cloud: &mut dyn CloudSaves) {
        cloud.fetch(NAME);
        self.asked = true;
    }

    /// Once the cloud's copy arrives, merges it into `game` and stores the result; true on that frame only.
    pub fn poll(&mut self, game: &mut Game, cloud: &mut dyn CloudSaves) -> bool {
        if !self.asked || self.answered {
            return false;
        }
        while let Some(file) = cloud.poll_cloud() {
            if file.name != NAME {
                continue;
            }
            self.answered = true;
            match &file.bytes {
                Some(bytes) => {
                    log!("cloud save: merging {} bytes", bytes.len());
                    game.merge_save(&Save::parse(&String::from_utf8_lossy(bytes)));
                }
                None => log!("cloud save: none yet"),
            }
            let text = game.save().to_text();
            if file.bytes.as_deref() != Some(text.as_bytes()) {
                cloud.store(NAME, text.as_bytes());
            }
            return true;
        }
        false
    }

    /// Stores a save just written, but not before the launch fetch answers: until then the cloud may hold progress
    /// this device lacks.
    pub fn store(&self, bytes: &[u8], cloud: &mut dyn CloudSaves) {
        if self.answered {
            cloud.store(NAME, bytes);
        }
    }
}
