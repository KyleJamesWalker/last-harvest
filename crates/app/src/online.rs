//! Carries the game's online traffic over `last-harvest-net`: joins the room the game names once the relay's ICE
//! servers are in, and moves packets both ways each frame.

use last_harvest_game::Game;
use last_harvest_game::netplay::{RELIABLE, Request, UNRELIABLE};
use last_harvest_net::{IceConfig, IceFetch, PeerId, PeerSocket, PeerState};

/// Seconds to wait for the relay to give this device an id.
const JOIN_WAIT: f32 = 10.0;

/// Offers online play, with the relay and a room to host or join at launch from the environment.
#[cfg(not(target_arch = "wasm32"))]
pub fn set_up(game: &mut Game) {
    game.set_online(true);
    game.set_identity(crate::VERSION, 0);
    if let Ok(relay) = std::env::var("LAST_HARVEST_RELAY") {
        game.set_relay(&relay);
    }
    if let Ok(code) = std::env::var("LAST_HARVEST_HOST") {
        game.host_online(Some(code.as_str()).filter(|c| !c.is_empty()));
    } else if let Ok(code) = std::env::var("LAST_HARVEST_JOIN") {
        game.join_online(&code);
    }
}

#[derive(Default)]
pub struct Online {
    socket: Option<PeerSocket>,
    ice: Option<(IceFetch, String)>,
    joined: bool,
    waited: f32,
}

impl Online {
    /// Acts on the game's requests and hands it what arrived; before the game's update.
    pub fn pump(&mut self, game: &mut Game, dt: f32) {
        while let Some(r) = game.net_request() {
            match r {
                Request::Connect { room, ice } => {
                    self.close();
                    self.ice = Some((IceFetch::start(&ice), room));
                }
                Request::Close => self.close(),
            }
        }
        if let Some((fetch, room)) = &mut self.ice
            && let Some(got) = fetch.poll()
        {
            let config = got.unwrap_or_else(|e| {
                crate::log!("online: no ICE servers from the relay ({e}); peers on one network still connect");
                IceConfig::default()
            });
            crate::log!("online: joining with {} ICE server(s)", config.servers.len());
            self.socket = Some(PeerSocket::connect(room, &config));
            (self.ice, self.joined, self.waited) = (None, false, 0.0);
        }
        let Some(s) = &mut self.socket else { return };
        if !self.joined {
            if let Some(id) = s.id() {
                game.net_joined(id.0);
                self.joined = true;
            } else {
                self.waited += dt;
                if s.signaling_closed() || self.waited > JOIN_WAIT {
                    game.net_failed("no answer");
                    self.waited = f32::MIN;
                }
            }
        }
        for (peer, state) in s.update_peers() {
            game.net_peer(peer.0, state == PeerState::Connected);
        }
        for channel in [RELIABLE, UNRELIABLE] {
            for (peer, bytes) in s.receive(channel) {
                game.net_receive(channel, peer.0, bytes);
            }
        }
    }

    /// Sends what the game queued; after the game's update.
    pub fn send(&mut self, game: &mut Game) {
        let out = game.net_outgoing();
        if let Some(s) = &mut self.socket {
            for p in out {
                s.send(p.channel, PeerId(p.peer), &p.bytes);
            }
        }
    }

    fn close(&mut self) {
        if let Some(mut s) = self.socket.take() {
            s.close();
        }
        self.ice = None;
    }
}
