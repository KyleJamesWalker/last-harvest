//! Native peers in one room on an in-process signaling server.

use last_harvest_net::{IceConfig, PeerId, PeerSocket, RELIABLE, UNRELIABLE};
use matchbox_signaling::SignalingServer;
use std::time::{Duration, Instant};

type Packets = Vec<(PeerId, Vec<u8>)>;

fn signaling_server() -> u16 {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async move {
            let mut server = SignalingServer::full_mesh_builder(([127, 0, 0, 1], 0)).build();
            tx.send(server.bind().unwrap().port()).unwrap();
            server.serve().await.unwrap();
        });
    });
    rx.recv().unwrap()
}

fn connect_all(n: usize, room: &str) -> Vec<PeerSocket> {
    let port = signaling_server();
    let deadline = Instant::now() + Duration::from_secs(30);
    // matchbox_signaling's full-mesh builder loses NewPeer for two peers that join at once.
    let mut sockets: Vec<PeerSocket> = Vec::new();
    for _ in 0..n {
        let mut socket = PeerSocket::connect(
            &format!("ws://127.0.0.1:{port}/{room}"),
            &IceConfig::default(),
        );
        while socket.id().is_none() {
            assert!(
                Instant::now() < deadline,
                "no peer id from the signaling server"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(50));
        sockets.push(socket);
    }
    while sockets.iter().any(|s| s.connected_peers().len() < n - 1) {
        assert!(Instant::now() < deadline, "peers did not connect");
        for socket in &mut sockets {
            socket.update_peers();
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    sockets
}

#[test]
fn two_peers_trade_a_message_on_each_channel() {
    let mut sockets = connect_all(2, "loopback");
    let (a, b) = (sockets[0].id().unwrap(), sockets[1].id().unwrap());
    assert_eq!(sockets[0].connected_peers(), [b]);
    assert_eq!(sockets[1].connected_peers(), [a]);
    sockets[0].send(RELIABLE, b, b"reliable from a");
    sockets[0].send(UNRELIABLE, b, b"unreliable from a");
    sockets[1].send(RELIABLE, a, b"reliable from b");
    sockets[1].send(UNRELIABLE, a, b"unreliable from b");

    let mut got: [[Packets; 2]; 2] = Default::default();
    let deadline = Instant::now() + Duration::from_secs(10);
    while got.iter().flatten().any(Vec::is_empty) {
        assert!(Instant::now() < deadline, "messages missing: {got:?}");
        for (socket, got) in sockets.iter_mut().zip(&mut got) {
            for channel in [RELIABLE, UNRELIABLE] {
                got[channel].extend(socket.receive(channel));
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(got[1][RELIABLE], [(a, b"reliable from a".to_vec())]);
    assert_eq!(got[1][UNRELIABLE], [(a, b"unreliable from a".to_vec())]);
    assert_eq!(got[0][RELIABLE], [(b, b"reliable from b".to_vec())]);
    assert_eq!(got[0][UNRELIABLE], [(b, b"unreliable from b".to_vec())]);
}

#[test]
fn channels_reach_every_peer() {
    let mut sockets = connect_all(3, "channels");
    let ids: Vec<PeerId> = sockets.iter_mut().map(|s| s.id().unwrap()).collect();
    for (i, socket) in sockets.iter_mut().enumerate() {
        for &peer in &ids {
            if peer != ids[i] {
                socket.send(RELIABLE, peer, &[i as u8; 3]);
                socket.send(UNRELIABLE, peer, &[i as u8 + 10]);
            }
        }
    }
    let mut got: Vec<Vec<(usize, PeerId, Vec<u8>)>> = vec![Vec::new(); 3];
    let deadline = Instant::now() + Duration::from_secs(10);
    while got.iter().any(|g| g.len() < 4) {
        assert!(Instant::now() < deadline, "messages missing: {got:?}");
        for (i, socket) in sockets.iter_mut().enumerate() {
            for channel in [RELIABLE, UNRELIABLE] {
                got[i].extend(
                    socket
                        .receive(channel)
                        .into_iter()
                        .map(|(p, d)| (channel, p, d)),
                );
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    for (i, messages) in got.iter().enumerate() {
        for (channel, from, data) in messages {
            let sender = ids.iter().position(|id| id == from).unwrap();
            assert_ne!(sender, i);
            let expected = if *channel == RELIABLE {
                vec![sender as u8; 3]
            } else {
                vec![sender as u8 + 10]
            };
            assert_eq!(data, &expected);
        }
    }

    sockets.pop().unwrap().close();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let left: Vec<bool> = sockets
            .iter_mut()
            .map(|s| {
                s.update_peers();
                s.connected_peers().len() == 1
            })
            .collect();
        if left.iter().all(|&l| l) {
            break;
        }
        assert!(Instant::now() < deadline, "closed peer still connected");
        std::thread::sleep(Duration::from_millis(10));
    }
}
