//! WebRTC transport: every peer joins a matchbox signaling room and opens a reliable and an unreliable data channel
//! to every other peer. Native (webrtc-rs) and browser (web-sys) peers share rooms.

mod inbox;
mod protocol;

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
use native as imp;

/// Ordered and retransmitted.
pub const RELIABLE: usize = 0;
/// Unordered, never retransmitted.
pub const UNRELIABLE: usize = 1;

/// A STUN or TURN server, as in the browser's `RTCIceServer`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IceServer {
    pub urls: Vec<String>,
    pub username: String,
    pub credential: String,
}

/// How peers look for a route to each other. The default has no servers, so it only connects peers on one network.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IceConfig {
    pub servers: Vec<IceServer>,
    /// Use TURN relays only, so other players never learn this player's address.
    pub relay_only: bool,
}

/// A peer's signaling UUID as a number; ordering it orders players the same way on every peer.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct PeerId(pub u128);

/// A change in a peer's connection: `Connected` once both channels are open.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PeerState {
    Connected,
    Disconnected,
}

/// A room connection, polled once per frame. Nothing blocks.
pub struct PeerSocket {
    inner: imp::Socket,
    /// Open peers, in the order they connected.
    connected: Vec<PeerId>,
}

impl PeerSocket {
    /// `room_url` is `ws[s]://<signaling server>/<room>`.
    pub fn connect(room_url: &str, ice: &IceConfig) -> PeerSocket {
        PeerSocket {
            inner: imp::Socket::connect(room_url, ice),
            connected: Vec::new(),
        }
    }

    /// None until the signaling server has assigned one.
    pub fn id(&mut self) -> Option<PeerId> {
        self.inner.id()
    }

    /// Peers that connected or left; each change is returned once.
    pub fn update_peers(&mut self) -> Vec<(PeerId, PeerState)> {
        let changes = self.inner.update_peers();
        for &(peer, state) in &changes {
            self.connected.retain(|&p| p != peer);
            if state == PeerState::Connected {
                self.connected.push(peer);
            }
        }
        changes
    }

    /// The peers whose connections are open, as of the last `update_peers`.
    pub fn connected_peers(&self) -> &[PeerId] {
        &self.connected
    }

    /// True once the signaling connection failed or closed; established peers stay connected.
    pub fn signaling_closed(&self) -> bool {
        self.inner.signaling_closed()
    }

    /// Queues `data` on `channel` to `peer`; a peer that is not connected never gets it.
    pub fn send(&mut self, channel: usize, peer: PeerId, data: &[u8]) {
        if channel <= UNRELIABLE {
            self.inner.send(channel, peer, data);
        }
    }

    /// The packets waiting on `channel`, oldest first; each is returned once.
    pub fn receive(&mut self, channel: usize) -> Vec<(PeerId, Vec<u8>)> {
        if channel > UNRELIABLE {
            return Vec::new();
        }
        self.inner.receive(channel)
    }

    /// Closes every peer connection and the signaling connection.
    pub fn close(&mut self) {
        self.inner.close();
        self.connected.clear();
    }
}
