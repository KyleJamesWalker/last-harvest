//! The lobby's state machines, a host and its clients passing messages in memory.

use last_harvest_lobby::*;

const HOST: PeerKey = 1;

fn identity() -> Identity {
    Identity { game_version: "v0.2.0".into(), pak: fingerprint(b"game_data.pak"), mods: vec![ModPack { name: "owls".into(), fingerprint: 7 }] }
}

fn hello(cards: &[&str]) -> Hello {
    Hello { identity: identity(), cards: cards.iter().map(|c| c.to_string()).collect() }
}

fn settings() -> Settings {
    Settings { seed: 42, map: "farm".into(), difficulty: 1, unlocked: vec!["scythe".into(), "crows".into()], options: vec![("night".into(), "1".into())] }
}

struct Room {
    host: Host,
    clients: Vec<(PeerKey, Client)>,
    host_events: Vec<HostEvent>,
    client_events: Vec<(PeerKey, ClientEvent)>,
}

impl Room {
    fn new(seats: u8, drop_in: bool) -> Room {
        let config = HostConfig { identity: identity(), cards: vec!["farmer".into()], seats, drop_in };
        Room { host: Host::new(RoomCode::from_random(7), config, HOST), clients: Vec::new(), host_events: Vec::new(), client_events: Vec::new() }
    }

    /// A client connects to the host and every other client, as in a full-mesh signaling room.
    fn join(&mut self, peer: PeerKey, hello: Hello) -> &Client {
        let mut client = Client::new(hello);
        client.connected(HOST);
        for (other, c) in &mut self.clients {
            client.connected(*other);
            c.connected(peer);
        }
        self.clients.push((peer, client));
        self.pump();
        self.client(peer)
    }

    fn client(&self, peer: PeerKey) -> &Client {
        &self.clients.iter().find(|(p, _)| *p == peer).unwrap().1
    }

    fn client_mut(&mut self, peer: PeerKey) -> &mut Client {
        &mut self.clients.iter_mut().find(|(p, _)| *p == peer).unwrap().1
    }

    fn pump(&mut self) {
        loop {
            let mut moved = false;
            for (to, bytes) in self.host.take_outgoing() {
                moved = true;
                if let Some((_, client)) = self.clients.iter_mut().find(|(p, _)| *p == to) {
                    let events = client.receive(HOST, &bytes);
                    self.client_events.extend(events.into_iter().map(|e| (to, e)));
                }
            }
            let mut packets = Vec::new();
            for (from, client) in &mut self.clients {
                packets.extend(client.take_outgoing().into_iter().map(|(to, bytes)| (*from, to, bytes)));
            }
            for (from, to, bytes) in packets {
                moved = true;
                if to == HOST {
                    self.host_events.extend(self.host.receive(from, &bytes));
                } else if let Some((_, client)) = self.clients.iter_mut().find(|(p, _)| *p == to) {
                    assert!(client.receive(from, &bytes).is_empty(), "a client answered another client");
                }
            }
            if !moved {
                return;
            }
        }
    }

    /// The transport drops `peer` on every side.
    fn disconnect(&mut self, peer: PeerKey) {
        self.clients.retain(|(p, _)| *p != peer);
        self.host_events.extend(self.host.disconnected(peer));
        for (p, client) in &mut self.clients {
            let events = client.disconnected(peer);
            self.client_events.extend(events.into_iter().map(|e| (*p, e)));
        }
        self.pump();
    }

    fn ready_all(&mut self) {
        self.host.set_ready(true);
        for (_, client) in &mut self.clients {
            client.set_ready(true);
        }
        self.pump();
    }
}

fn refusal(room: &mut Room, peer: PeerKey, hello: Hello) -> Refusal {
    match room.join(peer, hello).state().clone() {
        ClientState::Refused(reason) => reason,
        other => panic!("not refused: {other:?}"),
    }
}

#[test]
fn clients_join_ready_up_and_start_the_same_run() {
    let mut room = Room::new(4, true);
    assert_eq!(room.join(10, hello(&["witch"])).state(), &ClientState::Lobby);
    assert_eq!(room.join(11, hello(&["farmer", "scarecrow"])).state(), &ClientState::Lobby);
    assert_eq!(room.client(10).seats(), [1]);
    assert_eq!(room.client(11).seats(), [2, 3]);
    assert_eq!(room.client(11).host(), Some(HOST));
    assert!(room.host_events.contains(&HostEvent::Joined { peer: 10, seats: vec![Seat { seat: 1, card: "witch".into() }] }));

    let roster = room.client(10).roster();
    assert_eq!(roster.iter().map(|m| m.peer).collect::<Vec<_>>(), [HOST, 10, 11]);
    assert!(roster[0].host && roster.iter().all(|m| !m.ready));

    assert_eq!(room.host.start(settings()), Err(StartError::NotReady(vec![HOST, 10, 11])));
    room.client_mut(10).set_ready(true);
    room.pump();
    assert!(room.client(11).roster()[1].ready, "the lobby list shows the ready mark");
    assert_eq!(room.host.start(settings()), Err(StartError::NotReady(vec![HOST, 11])));
    room.ready_all();

    let start = room.host.start(settings()).unwrap();
    room.pump();
    assert_eq!(start.seed, 42);
    assert_eq!(start.unlocked, ["scythe", "crows"]);
    assert_eq!(
        start.players.iter().map(|p| (p.seat, p.peer, p.card.as_str())).collect::<Vec<_>>(),
        [(0, HOST, "farmer"), (1, 10, "witch"), (2, 11, "farmer"), (3, 11, "scarecrow")]
    );
    for peer in [10, 11] {
        assert_eq!(room.client(peer).state(), &ClientState::Running);
        assert_eq!(room.client(peer).start(), Some(&start));
    }
    assert_eq!(room.host.start(settings()), Err(StartError::AlreadyRunning));
}

#[test]
fn a_card_change_reaches_the_lobby_list() {
    let mut room = Room::new(4, true);
    room.join(10, hello(&["witch"]));
    room.client_mut(10).set_cards(vec!["scarecrow".into()]);
    room.host.set_cards(&["witch".into()]);
    room.pump();
    let roster = room.client(10).roster();
    assert_eq!(roster[0].seats[0].card, "witch");
    assert_eq!(roster[1].seats[0].card, "scarecrow");
}

#[test]
fn mismatches_are_refused_with_a_reason() {
    let mut room = Room::new(4, true);
    let mut old = hello(&["witch"]);
    old.identity.game_version = "v0.1.9".into();
    let reason = refusal(&mut room, 10, old);
    assert_eq!(reason, Refusal::Version { host: "v0.2.0".into(), yours: "v0.1.9".into() });
    assert_eq!(reason.to_string(), "The host runs version v0.2.0, this game v0.1.9: update both to the same release.");

    let mut pak = hello(&["witch"]);
    pak.identity.pak ^= 1;
    assert_eq!(refusal(&mut room, 11, pak), Refusal::Pak);

    let mut mods = hello(&["witch"]);
    mods.identity.mods = vec![ModPack { name: "owls".into(), fingerprint: 8 }, ModPack { name: "bats".into(), fingerprint: 1 }];
    let reason = refusal(&mut room, 12, mods);
    assert_eq!(reason, Refusal::Mods { missing: vec![], extra: vec!["bats".into()], different: vec!["owls".into()] });
    assert_eq!(reason.to_string(), "The host's mods differ: remove bats; get the host's copy of owls.");

    let mut no_mods = hello(&["witch"]);
    no_mods.identity.mods.clear();
    assert_eq!(refusal(&mut room, 13, no_mods), Refusal::Mods { missing: vec!["owls".into()], extra: vec![], different: vec![] });

    assert!(matches!(refusal(&mut room, 14, hello(&[])), Refusal::BadHello(_)));
    assert!(matches!(refusal(&mut room, 15, hello(&["a", "b", "c"])), Refusal::BadHello(_)));
    assert_eq!(room.host.members().len(), 1, "nobody refused is seated");
}

#[test]
fn a_full_room_refuses_more_players_than_it_has_seats() {
    let mut room = Room::new(3, true);
    room.join(10, hello(&["witch"]));
    assert_eq!(refusal(&mut room, 11, hello(&["a", "b"])), Refusal::Full { free: 1, asked: 2 });
    assert_eq!(room.join(12, hello(&["a"])).seats(), [2]);
}

#[test]
fn another_protocol_version_is_refused_readably() {
    let mut room = Room::new(4, true);
    let mut bytes = encode(&Message::Hello(hello(&["witch"])));
    bytes[2] = PROTOCOL + 1;
    assert_eq!(room.host.receive(10, &bytes), vec![HostEvent::Refused { peer: 10, reason: Refusal::Protocol { host: PROTOCOL, yours: PROTOCOL + 1 } }]);

    let mut client = Client::new(hello(&["witch"]));
    let mut from_newer_host = encode(&Message::Welcome { room: "AC3JK".into(), seats: vec![1], running: false });
    from_newer_host[2] = PROTOCOL + 1;
    let events = client.receive(HOST, &from_newer_host);
    assert_eq!(events, vec![ClientEvent::Refused(Refusal::Protocol { host: PROTOCOL + 1, yours: PROTOCOL })]);
    assert!(room.host.receive(10, b"not the lobby").is_empty());
}

#[test]
fn a_player_drops_in_mid_run_and_gets_a_seat() {
    let mut room = Room::new(4, true);
    room.join(10, hello(&["witch"]));
    room.ready_all();
    room.host.start(settings()).unwrap();
    room.pump();

    let late = room.join(11, hello(&["scarecrow", "farmer"]));
    assert_eq!(late.state(), &ClientState::Running);
    assert_eq!(late.seats(), [2, 3]);
    assert_eq!(late.start().unwrap().seed, 42);
    assert_eq!(late.start().unwrap().players.len(), 4);
    assert_eq!(room.host.running().unwrap().players.len(), 4);
    assert!(room.host_events.iter().any(|e| matches!(e, HostEvent::DropIn { peer: 11, players } if players.len() == 2)));
    assert!(room.client_events.iter().any(|(p, e)| *p == 10 && matches!(e, ClientEvent::Joined(m) if m.peer == 11)));
    assert_eq!(room.client(10).start().unwrap().players.len(), 4);
}

#[test]
fn a_room_without_drop_in_refuses_mid_run() {
    let mut room = Room::new(4, false);
    room.ready_all();
    room.host.start(settings()).unwrap();
    assert_eq!(refusal(&mut room, 10, hello(&["witch"])), Refusal::Running);
}

#[test]
fn a_client_that_leaves_frees_its_seats() {
    let mut room = Room::new(4, true);
    room.join(10, hello(&["witch"]));
    room.join(11, hello(&["a", "b"]));
    room.disconnect(10);
    assert!(room.host_events.contains(&HostEvent::Left { peer: 10, seats: vec![1] }));
    assert_eq!(room.client(11).roster().len(), 2);
    assert_eq!(room.join(12, hello(&["c"])).seats(), [1]);

    room.ready_all();
    room.host.start(settings()).unwrap();
    room.pump();
    let events = room.client_mut(11).leave();
    assert_eq!(events, vec![ClientEvent::Ended(EndReason::Left)]);
    room.pump();
    assert!(room.host_events.contains(&HostEvent::Left { peer: 11, seats: vec![2, 3] }));
    assert!(room.client_events.contains(&(12, ClientEvent::Left { peer: 11, seats: vec![2, 3] })));
    assert_eq!(room.host.running().unwrap().players.iter().map(|p| p.seat).collect::<Vec<_>>(), [0, 1]);
}

#[test]
fn the_host_leaving_ends_the_room_for_every_client() {
    let mut room = Room::new(4, true);
    room.join(10, hello(&["witch"]));
    room.join(11, hello(&["farmer"]));
    room.host.close();
    room.pump();
    for peer in [10, 11] {
        assert_eq!(room.client(peer).state(), &ClientState::Ended(EndReason::HostLeft));
    }
    assert!(room.host.is_closed());
    assert_eq!(room.host.start(settings()), Err(StartError::Closed));
}

#[test]
fn losing_the_host_ends_the_room() {
    let mut room = Room::new(4, true);
    room.join(10, hello(&["witch"]));
    room.ready_all();
    room.host.start(settings()).unwrap();
    room.pump();
    let events = room.client_mut(10).disconnected(HOST);
    assert_eq!(events, vec![ClientEvent::Ended(EndReason::HostLost)]);
    assert!(room.client_mut(10).disconnected(HOST).is_empty(), "it ends once");
}

#[test]
fn a_second_host_in_the_room_is_noticed() {
    let mut room = Room::new(4, true);
    let welcome = encode(&Message::Welcome { room: "AC3JK".into(), seats: vec![0], running: false });
    assert_eq!(room.host.receive(99, &welcome), vec![HostEvent::CodeTaken { peer: 99 }]);
}
