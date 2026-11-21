//! The online co-op lobby, as messages on the host's reliable channel. The host runs the game: a client says hello
//! with its version, data pack and mods; the host seats it or refuses it with a reason, keeps the roster with
//! ready marks, starts the run with what the simulation needs, seats a player who drops in mid-run, and closes the
//! room when it leaves. No sockets: the caller feeds in what arrives and sends what `take_outgoing` returns.

mod client;
mod code;
mod codec;
mod host;

pub use client::{Client, ClientEvent, ClientState, EndReason};
pub use code::{CodeError, RoomCode, ALPHABET, CODE_LEN};
pub use codec::{decode, encode, is_lobby_message, DecodeError, Message};
pub use host::{Host, HostConfig, HostEvent, StartError};

/// The lobby protocol's version; every message carries it.
pub const PROTOCOL: u8 = 1;
/// Players in one game, over every device.
pub const MAX_SEATS: u8 = 4;
/// Players on one device: split-screen on a client.
pub const MAX_LOCAL_PLAYERS: usize = 2;

/// A peer as the transport names it: `last_harvest_net::PeerId.0`.
pub type PeerKey = u128;

/// FNV-1a, 64-bit: a data pack's or a mod pack's fingerprint.
pub fn fingerprint(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3))
}

/// A mod pack by its name and fingerprint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModPack {
    pub name: String,
    pub fingerprint: u64,
}

/// What must match between the host and a client for their simulations to agree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub game_version: String,
    pub pak: u64,
    pub mods: Vec<ModPack>,
}

/// A client's first message to the room.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hello {
    pub identity: Identity,
    /// One player card id per local player.
    pub cards: Vec<String>,
}

/// One player's place in the game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seat {
    pub seat: u8,
    pub card: String,
}

/// A device in the room and the players it seats.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    pub peer: PeerKey,
    pub seats: Vec<Seat>,
    pub ready: bool,
    pub host: bool,
}

/// What every simulation needs to start the same run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Start {
    pub seed: u64,
    pub map: String,
    pub difficulty: u8,
    /// Every seated player, by seat.
    pub players: Vec<Player>,
    /// The host's unlocked powerup pool.
    pub unlocked: Vec<String>,
    /// Other settings the simulation reads, as name and value.
    pub options: Vec<(String, String)>,
}

/// A seated player and the device that plays it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Player {
    pub seat: u8,
    pub peer: PeerKey,
    pub card: String,
}

/// The host's choices for a run: `Host::start` adds the players.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub seed: u64,
    pub map: String,
    pub difficulty: u8,
    pub unlocked: Vec<String>,
    pub options: Vec<(String, String)>,
}

/// Why the host turned a client away.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    Protocol { host: u8, yours: u8 },
    Version { host: String, yours: String },
    Pak,
    Mods { missing: Vec<String>, extra: Vec<String>, different: Vec<String> },
    Full { free: u8, asked: u8 },
    Running,
    BadHello(String),
    /// A refusal from another protocol version, by its text.
    Other(String),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let list = |names: &[String]| names.join(", ");
        match self {
            Refusal::Protocol { host, yours } => write!(f, "The host speaks lobby protocol {host}, this game {yours}: update both to the same release."),
            Refusal::Version { host, yours } => write!(f, "The host runs version {host}, this game {yours}: update both to the same release."),
            Refusal::Pak => write!(f, "The host's game data differs from this game's: use the same release's game_data.pak, and the same cards, maps and farms."),
            Refusal::Mods { missing, extra, different } => {
                let mut parts = Vec::new();
                if !missing.is_empty() {
                    parts.push(format!("install {}", list(missing)));
                }
                if !extra.is_empty() {
                    parts.push(format!("remove {}", list(extra)));
                }
                if !different.is_empty() {
                    parts.push(format!("get the host's copy of {}", list(different)));
                }
                write!(f, "The host's mods differ: {}.", parts.join("; "))
            }
            Refusal::Full { free, asked } => write!(f, "The room has {free} free seat(s); this game asked for {asked}."),
            Refusal::Running => write!(f, "The run has started and the host takes no one mid-run."),
            Refusal::BadHello(why) => write!(f, "The host could not read this game's hello: {why}."),
            Refusal::Other(text) => f.write_str(text),
        }
    }
}
