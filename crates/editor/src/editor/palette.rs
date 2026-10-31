//! The palette the editors draw from: the game's own, or one loaded from Lospec by name or as a `.hex` file.

use last_harvest_game::paint;

use super::Editor;
use crate::sprite;

/// The most colors the palette panel shows.
pub(super) const SHOWN: usize = 90;

impl Editor {
    /// Loads the Lospec palette called `name`, or goes back to the game's own when it's blank.
    pub(super) fn load_lospec(&mut self, name: &str) {
        if name.is_empty() {
            self.palette = sprite::palette();
            self.palette_name = None;
            return self.say("Back to the game's palette");
        }
        if !paint::lospec_name(name) {
            return self.say("A Lospec palette's name is its address's last part, such as endesga-32");
        }
        match self.store.lospec(name) {
            Ok(text) => self.use_palette(name, &text),
            Err(e) => self.say(e),
        }
    }

    /// Draws from the colors in a palette file's `text`, as Lospec's `.hex` files have them, under `name`.
    pub fn use_palette(&mut self, name: &str, text: &str) {
        match paint::parse_palette(text) {
            Ok(colors) => {
                let n = colors.len();
                self.palette = colors.into_iter().map(|[r, g, b]| [r, g, b, 255]).collect();
                self.palette_name = Some(name.to_string());
                let cut = if n > SHOWN { format!("; the first {SHOWN} show") } else { String::new() };
                self.say(format!("Drawing from {name}: {n} colors{cut}. The game's own art keeps to crates/game/palette.txt"));
            }
            Err(e) => self.say(format!("{name} isn't a palette: {e}")),
        }
    }

    /// What the palette panel's heading says.
    pub(super) fn palette_label(&self) -> String {
        match &self.palette_name {
            Some(name) => name.chars().take(14).collect(),
            None => "Game palette".into(),
        }
    }
}
