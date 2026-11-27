//! The platform backend this build plugs in, and the one place the game's asks reach it.

use std::cell::Cell;

use last_harvest_game::{Ask, Buttons, Game};
pub use last_harvest_platform::*;

/// Frames between asks for the button labels, which change only when the player's pad or bindings do.
const GLYPH_EVERY: u32 = 30;

thread_local! {
    static FRAME: Cell<u32> = const { Cell::new(0) };
}

/// The public build's backend, which does nothing.
pub fn backend() -> Box<dyn Platform> {
    Box::new(NoOp)
}

/// Hands what the game asked since the last frame to `platform`; every shell calls it once a frame.
pub fn serve(game: &mut Game, platform: &mut dyn Platform) {
    platform.tick();
    let frame = FRAME.get();
    FRAME.set(frame.wrapping_add(1));
    if frame.is_multiple_of(GLYPH_EVERY) {
        glyphs(game, platform);
    }
    hand(game.take_asks(), platform);
}

/// Hands the game the platform's label for each of its buttons.
pub fn glyphs(game: &mut Game, platform: &dyn Platform) {
    const BUTTONS: [(Button, Buttons); 10] = [
        (Button::A, Buttons::A),
        (Button::B, Buttons::B),
        (Button::X, Buttons::X),
        (Button::Y, Buttons::Y),
        (Button::L1, Buttons::L1),
        (Button::R1, Buttons::R1),
        (Button::L2, Buttons::L2),
        (Button::R2, Buttons::R2),
        (Button::Select, Buttons::SELECT),
        (Button::Start, Buttons::START),
    ];
    let labels: Vec<(Buttons, String)> = BUTTONS
        .iter()
        .filter_map(|&(b, game)| platform.glyph(b).map(|l| (game, l)))
        .collect();
    game.set_button_labels(labels.iter().map(|(b, l)| (*b, l.as_str())));
}

pub fn hand(asks: impl IntoIterator<Item = Ask>, platform: &mut dyn Platform) {
    for ask in asks {
        match ask {
            Ask::Unlock(id) => platform.unlock(id),
        }
    }
}
