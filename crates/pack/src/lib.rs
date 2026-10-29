//! Data packs, ZIP files with a `.pak` extension and a `manifest.toml` at the root, and enemy cards.

pub mod card;
pub mod prop;
#[cfg(feature = "pak")]
mod pak;

#[cfg(feature = "pak")]
pub use pak::*;

/// Where a pack keeps hand-made maps and players' props.
pub const MAPS: &str = "maps/";
pub const PROPS: &str = "props/";
