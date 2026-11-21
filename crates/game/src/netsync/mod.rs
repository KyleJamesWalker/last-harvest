//! Online co-op state sync: the host runs the night and sends each client compact snapshots of it, and each client
//! sends back its players' input. Nothing here touches a socket; `crates/net` carries the bytes.

pub mod events;
pub mod input;
pub mod link;
pub mod mirror;
pub mod pipe;
pub mod snapshot;
pub mod wire;

#[cfg(test)]
mod tests;

pub use events::Event;
pub use input::{InputFrame, InputLog, InputMsg, InputQueue};
pub use link::{ClientLink, HostLink};
pub use mirror::Mirror;
pub use pipe::Pipe;
pub use snapshot::{Part, PlayerState, Spawn, Spawned, Thing, World, ZombieState};
