//! A forced ICE restart (`LAST_HARVEST_NET_TEST_RESTART_MS`) renegotiates a live connection without dropping it.

use matchbox_signaling::SignalingServer;
use last_harvest_net::{IceConfig, PeerSocket, PeerState, UNRELIABLE};
use std::time::{Duration, Instant};

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

#[test]
fn a_restarted_connection_keeps_carrying_packets() {
    // SAFETY: the only test in this binary, set before any thread starts.
    unsafe { std::env::set_var("LAST_HARVEST_NET_TEST_RESTART_MS", "500") };
    let port = signaling_server();
    let local = IceConfig::default();
    let url = format!("ws://127.0.0.1:{port}/restart");
    let mut a = PeerSocket::connect(&url, &local);
    while a.id().is_none() {
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_millis(50));
    let mut b = PeerSocket::connect(&url, &local);
    let deadline = Instant::now() + Duration::from_secs(10);
    while a.connected_peers().is_empty() || b.connected_peers().is_empty() {
        assert!(Instant::now() < deadline, "peers did not connect");
        a.update_peers();
        b.update_peers();
        std::thread::sleep(Duration::from_millis(10));
    }
    let (a_id, b_id) = (a.id().unwrap(), b.id().unwrap());

    // Past the restart, packets must still arrive both ways.
    let end = Instant::now() + Duration::from_secs(8);
    let mut last_heard = (Instant::now(), Instant::now());
    let mut worst = Duration::ZERO;
    while Instant::now() < end {
        a.send(UNRELIABLE, b_id, &[1]);
        b.send(UNRELIABLE, a_id, &[2]);
        std::thread::sleep(Duration::from_millis(16));
        if !b.receive(UNRELIABLE).is_empty() {
            last_heard.0 = Instant::now();
        }
        if !a.receive(UNRELIABLE).is_empty() {
            last_heard.1 = Instant::now();
        }
        worst = worst.max(last_heard.0.elapsed()).max(last_heard.1.elapsed());
        for (peer, state) in a.update_peers().into_iter().chain(b.update_peers()) {
            assert_ne!(state, PeerState::Disconnected, "{peer:?} dropped");
        }
    }
    // A restart closes the old candidates' sockets, so packets pause while the new pair is checked.
    assert!(worst < Duration::from_secs(5), "packets stopped for {worst:?}");
    assert!(last_heard.0.elapsed().max(last_heard.1.elapsed()) < Duration::from_millis(500), "packets did not resume");
}
