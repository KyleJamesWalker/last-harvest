//! The lobby over the real transport: a local relay and three `last-harvest-net` peers in one process.

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

    fn client_mut(&mut self) -> &mut Client {
        match &mut self.side {
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
fn three_peers_create_join_ready_and_start_through_the_relay() {
    let port = last_harvest_relay::start(&last_harvest_relay::Config::embedded(0)).unwrap();
    let code = RoomCode::from_random(0x5eed);
    let url = code.room_url(&format!("ws://127.0.0.1:{port}"));
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
    let mut peers = vec![Peer {
        side: Side::Host(Host::new(code, config, me)),
        socket: host_socket,
    }];
    for cards in [
        vec!["witch".to_string()],
        vec!["scarecrow".to_string(), "farmer".to_string()],
    ] {
        let hello = Hello {
            identity: identity.clone(),
            cards,
        };
        peers.push(Peer {
            socket: connect(&url),
            side: Side::Client(Client::new(hello)),
        });
    }

    run_until(&mut peers, "both clients in the lobby", |p| {
        p[1..]
            .iter()
            .all(|c| c.client().state() == &ClientState::Lobby && c.client().roster().len() == 3)
    });
    assert_eq!(peers[1].client().host(), Some(me));
    let mut seats: Vec<u8> = peers[1..]
        .iter()
        .flat_map(|c| c.client().seats().to_vec())
        .collect();
    seats.sort();
    assert_eq!(seats, [1, 2, 3]);

    peers[0].host_mut().set_ready(true);
    for client in &mut peers[1..] {
        client.client_mut().set_ready(true);
    }
    run_until(&mut peers, "every ready mark", |p| {
        p[1..]
            .iter()
            .all(|c| c.client().roster().iter().all(|m| m.ready))
    });

    let settings = Settings {
        seed: 77,
        map: "farm".into(),
        difficulty: 2,
        unlocked: vec!["scythe".into()],
        options: Vec::new(),
    };
    let start = peers[0].host_mut().start(settings).unwrap();
    assert_eq!(start.players.len(), 4);
    run_until(&mut peers, "the start", |p| {
        p[1..]
            .iter()
            .all(|c| c.client().state() == &ClientState::Running)
    });
    for client in &peers[1..] {
        assert_eq!(client.client().start(), Some(&start));
    }

    peers[0].host_mut().close();
    run_until(&mut peers, "the room to close", |p| {
        p[1..]
            .iter()
            .all(|c| c.client().state() == &ClientState::Ended(EndReason::HostLeft))
    });
    for peer in &mut peers {
        peer.socket.close();
    }
}
