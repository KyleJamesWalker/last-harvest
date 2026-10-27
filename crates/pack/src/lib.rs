//! Data packs, ZIP files with a `.pak` extension and a `manifest.toml` at the root, and enemy cards.

pub mod card;
#[cfg(feature = "pak")]
mod pak;

#[cfg(feature = "pak")]
pub use pak::*;
