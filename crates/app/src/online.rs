//! Carries the game's online traffic over `last-harvest-net`: joins the room the game names once the relay's ICE
//! servers are in, and moves packets both ways each frame.

use last_harvest_game::Game;
use last_harvest_game::netplay::{RELIABLE, Request, UNRELIABLE};
use last_harvest_net::{IceConfig, IceFetch, PeerId, PeerSocket, PeerState};

/// Seconds to wait for the relay to give this device an id.
const JOIN_WAIT: f32 = 10.0;
/// Seconds between looks at the LAN hosts heard nearby.
#[cfg(not(target_arch = "wasm32"))]
const NEARBY_EVERY: f32 = 0.5;

/// Offers online play, with the relay and a room to host or join at launch from the environment.
#[cfg(not(target_arch = "wasm32"))]
pub fn set_up(game: &mut Game) {
    game.set_online(true);
    game.set_identity(crate::VERSION, 0);
    if let Ok(relay) = std::env::var("LAST_HARVEST_RELAY") {
        game.set_relay(&relay);
    }
    game.set_lan(Some(last_harvest_relay::lan::HOST_PORT));
    last_harvest_relay::lan::listen();
    if let Ok(code) = std::env::var("LAST_HARVEST_LAN_HOST") {
        game.host_lan(Some(code.as_str()).filter(|c| !c.is_empty()));
    } else if let Ok(code) = std::env::var("LAST_HARVEST_HOST") {
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
    #[cfg(not(target_arch = "wasm32"))]
    nearby_in: f32,
}

impl Online {
    /// Acts on the game's requests and hands it what arrived; before the game's update.
    pub fn pump(&mut self, game: &mut Game, dt: f32) {
        #[cfg(not(target_arch = "wasm32"))]
        self.lan(game, dt);
        while let Some(r) = game.net_request() {
            match r {
                Request::Connect { room, ice } if ice.is_empty() => {
                    self.close();
                    crate::log!("online: joining on this network, with no ICE servers");
                    self.socket = Some(PeerSocket::connect(&room, &IceConfig::default()));
                    (self.joined, self.waited) = (false, 0.0);
                }
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

    /// Starts this device's LAN relay when the game hosts on its network, and hands it the hosts heard nearby.
    #[cfg(not(target_arch = "wasm32"))]
    fn lan(&mut self, game: &mut Game, dt: f32) {
        use last_harvest_relay::lan;
        if let Some(port) = game.take_lan_host() {
            match lan::host(port) {
                Ok(()) => {
                    let address = lan::local_address().map(|a| a.to_string());
                    crate::log!("online: hosting on this network: others join ws://{}:{port}", address.as_deref().unwrap_or("<this device>"));
                    game.set_lan_address(address);
                }
                Err(e) => {
                    crate::log!("online: can't host on this network: {e}");
                    game.net_failed(&e);
                }
            }
        }
        self.nearby_in -= dt;
        if self.nearby_in <= 0.0 {
            self.nearby_in = NEARBY_EVERY;
            game.set_nearby(lan::nearby());
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
            #[cfg(not(target_arch = "wasm32"))]
            std::thread::spawn(move || drop(s));
        }
        self.ice = None;
    }
}
