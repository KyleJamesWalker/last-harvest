//! The player's comfort settings that change only how the game looks: screen shake, flashes and text size. The game
//! sets them each frame, so every scene draws with them and nothing a run or a replay records depends on them.

use std::cell::Cell;

use crate::save::{Save, TextSize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Comfort {
    pub shake: bool,
    pub flashes: bool,
    pub text: TextSize,
}

impl Default for Comfort {
    fn default() -> Self {
        Self {
            shake: true,
            flashes: true,
            text: TextSize::Normal,
        }
    }
}

impl Comfort {
    pub fn of(save: &Save) -> Self {
        Self {
            shake: save.shake,
            flashes: save.flashes,
            text: save.text,
        }
    }
}

/// Large text grows by this much, up to `LARGE_UP_TO`; text already that large keeps its size.
const LARGE: f32 = 1.3;
const LARGE_UP_TO: f32 = 40.0;

thread_local! {
    static NOW: Cell<Comfort> = Cell::new(Comfort::default());
}

pub(crate) fn set(c: Comfort) {
    NOW.set(c);
}

pub(crate) fn get() -> Comfort {
    NOW.get()
}

/// The size a label asked for at `size` draws at.
pub(crate) fn text(size: f32) -> f32 {
    match get().text {
        TextSize::Normal => size,
        TextSize::Large => size.max((size * LARGE).min(LARGE_UP_TO)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_text_grows_the_small_text_most_and_keeps_the_order_of_sizes() {
        assert_eq!(text(18.0), 18.0, "normal by default");
        set(Comfort {
            text: TextSize::Large,
            ..Comfort::default()
        });
        assert!((text(18.0) - 23.4).abs() < 1e-4);
        assert_eq!(text(38.0), 40.0);
        assert_eq!(text(64.0), 64.0, "titles keep their size");
        let sizes: Vec<f32> = (10..80).map(|s| text(s as f32)).collect();
        assert!(sizes.windows(2).all(|w| w[0] <= w[1]), "{sizes:?}");
        set(Comfort::default());
    }
}
