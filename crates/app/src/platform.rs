//! The platform backend this build plugs in, and the one place the game's asks reach it.

use std::path::PathBuf;

use crate::metrics::Telemetry;
use last_harvest_game::{Ask, Game};
pub use last_harvest_platform::*;

/// The backend, and the outbox that holds play stats for it.
pub struct Hooks {
    pub backend: Box<dyn Platform>,
    pub metrics: Telemetry,
}

impl Hooks {
    /// `store` is the folder the metrics outbox and crash reports live in; none keeps them in memory.
    pub fn new(backend: Box<dyn Platform>, store: Option<PathBuf>) -> Self {
        Self {
            backend,
            metrics: Telemetry::new(store),
        }
    }

    /// Keeps what hasn't been sent; every shell calls it as it exits.
    pub fn exiting(&mut self) {
        self.metrics.exiting();
    }
}

/// The public build's backend, which does nothing.
pub fn backend(store: Option<PathBuf>) -> Hooks {
    Hooks::new(Box::new(NoOp), store)
}

/// Hands this frame's asks to the backend and the outbox; every shell calls it once a frame, `dt` seconds long.
pub fn serve(game: &mut Game, hooks: &mut Hooks, dt: f32) {
    let platform = hooks.backend.as_mut();
    platform.tick();
    game.set_metrics_offered(platform.collects());
    let asks = game.take_asks();
    hooks
        .metrics
        .frame(game.save().metrics, &asks, platform, dt);
    hand(asks, platform);
}

pub fn hand(asks: impl IntoIterator<Item = Ask>, platform: &mut dyn Platform) {
    for ask in asks {
        match ask {
            Ask::Unlock(id) => platform.unlock(id),
            Ask::RunEnded(_) => {}
        }
    }
}
