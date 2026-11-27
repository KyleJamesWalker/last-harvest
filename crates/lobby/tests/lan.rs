//! Hosting on this network: the game's LAN relay (`Config::lan_host`) and two `last-harvest-net` peers with no ICE
//! servers, through create and join, and the one room it holds.

use last_harvest_lobby::*;
use last_harvest_net::{IceConfig, PeerSocket, PeerState, RELIABLE};
use std::time::{Duration, Instant};

enum Side {
    Host(Host),
    Client(Client),
}

struct Peer {
    socket: PeerSocket,
    side: Side,
}

impl Peer {
    /// Moves one round of traffic: peer changes, packets in, packets out.
    fn step(&mut self) {
        for (peer, state) in self.socket.update_peers() {
            match (&mut self.side, state) {
                (Side::Client(client), PeerState::Connected) => client.connected(peer.0),
                (Side::Client(client), PeerState::Disconnected) => {
                    drop(client.disconnected(peer.0))
                }
                (Side::Host(host), PeerState::Disconnected) => drop(host.disconnected(peer.0)),
                (Side::Host(_), PeerState::Connected) => {}
            }
        }
        for (peer, bytes) in self.socket.receive(RELIABLE) {
            match &mut self.side {
                Side::Host(host) => drop(host.receive(peer.0, &bytes)),
                Side::Client(client) => drop(client.receive(peer.0, &bytes)),
            }
        }
        let outgoing = match &mut self.side {
            Side::Host(host) => host.take_outgoing(),
            Side::Client(client) => client.take_outgoing(),
        };
        for (to, bytes) in outgoing {
            self.socket
                .send(RELIABLE, last_harvest_net::PeerId(to), &bytes);
        }
    }

    fn client(&self) -> &Client {
        match &self.side {
            Side::Client(client) => client,
            Side::Host(_) => panic!("the host"),
        }
    }

    fn host_mut(&mut self) -> &mut Host {
        match &mut self.side {
            Side::Host(host) => host,
            Side::Client(_) => panic!("a client"),
        }
    }
}

fn run_until(peers: &mut [Peer], what: &str, mut done: impl FnMut(&mut [Peer]) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !done(peers) {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        for peer in peers.iter_mut() {
            peer.step();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn connect(url: &str) -> PeerSocket {
    let mut socket = PeerSocket::connect(url, &IceConfig::default());
    let deadline = Instant::now() + Duration::from_secs(10);
    while socket.id().is_none() {
        assert!(Instant::now() < deadline, "no peer id from the relay");
        std::thread::sleep(Duration::from_millis(10));
    }
    socket
}

#[test]
fn a_lan_host_and_a_client_meet_through_the_lan_relay_which_holds_one_room() {
    let port = last_harvest_relay::start(&last_harvest_relay::Config::lan_host(0)).unwrap();
    let base = format!("ws://127.0.0.1:{port}");
    let code = RoomCode::from_random(0x1a7);
    let url = code.room_url(&base);
    let identity = Identity {
        game_version: "test".into(),
        pak: fingerprint(b"pak"),
        mods: Vec::new(),
    };

    let mut host_socket = connect(&url);
    let me = host_socket.id().unwrap().0;
    let config = HostConfig {
        identity: identity.clone(),
        cards: vec!["farmer".into()],
        seats: 4,
        drop_in: true,
    };
    let hello = Hello {
        identity,
        cards: vec!["witch".into()],
    };
    let mut peers = vec![
        Peer {
            side: Side::Host(Host::new(code, config, me)),
            socket: host_socket,
        },
        Peer {
            socket: connect(&url),
            side: Side::Client(Client::new(hello)),
        },
    ];
    run_until(&mut peers, "the client in the lobby", |p| {
        p[1].client().state() == &ClientState::Lobby && p[1].client().roster().len() == 2
    });
    assert_eq!(peers[1].client().host(), Some(me));

    let other = RoomCode::from_random(0xbad).room_url(&base);
    let mut stray = PeerSocket::connect(&other, &IceConfig::default());
    let deadline = Instant::now() + Duration::from_secs(10);
    while !stray.signaling_closed() {
        assert!(Instant::now() < deadline, "a second room was let in");
        let _ = stray.id();
        std::thread::sleep(Duration::from_millis(10));
    }
    stray.close();

    peers[0].host_mut().close();
    run_until(&mut peers, "the room to close", |p| {
        p[1].client().state() == &ClientState::Ended(EndReason::HostLeft)
    });
    for peer in &mut peers {
        peer.socket.close();
    }
}
