//! A native peer whose signaling connection drops without a close resumes its place on last-harvest-relay.

use last_harvest_net::{IceConfig, PeerSocket, PeerState};
use std::time::{Duration, Instant};

#[test]
fn a_dropped_signaling_connection_resumes_its_place() {
    // SAFETY: the only test in this binary, so no other thread reads the environment yet.
    unsafe { std::env::set_var("LAST_HARVEST_NET_TEST_DROP_SIGNAL_MS", "1500") };
    let port = last_harvest_relay::start(&last_harvest_relay::Config::embedded(0)).unwrap();
    let url = format!("ws://127.0.0.1:{port}/signal/resume-e2e");
    let mut sockets = [
        PeerSocket::connect(&url, &IceConfig::default()),
        PeerSocket::connect(&url, &IceConfig::default()),
    ];

    let deadline = Instant::now() + Duration::from_secs(15);
    let mut ids = [None, None];
    let mut connected = [false, false];
    let mut lost = false;
    while Instant::now() < deadline {
        for (i, socket) in sockets.iter_mut().enumerate() {
            let id = socket.id();
            assert!(
                ids[i].is_none() || id == ids[i],
                "a resume keeps the peer ID"
            );
            ids[i] = id.or(ids[i]);
            for (_, state) in socket.update_peers() {
                connected[i] |= state == PeerState::Connected;
                lost |= state == PeerState::Disconnected;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(connected[0] && connected[1], "the peers connected");
    assert!(!lost, "no peer left");
    assert!(
        !sockets[0].signaling_closed() && !sockets[1].signaling_closed(),
        "both signaling connections came back"
    );
}
