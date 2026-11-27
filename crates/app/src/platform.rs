//! The platform backend this build plugs in, and the one place the game's asks reach it.

use last_harvest_game::{Ask, Game};
pub use last_harvest_platform::*;

/// The public build's backend, which does nothing.
pub fn backend() -> Box<dyn Platform> {
    Box::new(NoOp)
}

/// Hands what the game asked since the last frame to `platform`; every shell calls it once a frame.
pub fn serve(game: &mut Game, platform: &mut dyn Platform) {
    platform.tick();
    hand(game.take_asks(), platform);
}

pub fn hand(asks: impl IntoIterator<Item = Ask>, platform: &mut dyn Platform) {
    for ask in asks {
        match ask {
            Ask::Unlock(id) => platform.unlock(id),
        }
    }
}
