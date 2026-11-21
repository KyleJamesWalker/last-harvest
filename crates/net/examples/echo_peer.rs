//! A native peer to test other peers against: greets every peer on both channels and echoes each packet back on its
//! channel with `echo:` in front. Usage: `echo_peer <room url> <seconds>`; `LAST_HARVEST_NET_TURN_*` force a TURN relay.

use last_harvest_net::{IceConfig, IceServer, PeerSocket, PeerState, RELIABLE, UNRELIABLE};
use std::time::{Duration, Instant};

fn main() {
    let mut args = std::env::args().skip(1);
    let url = args.next().expect("room url");
    let seconds: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(60);
    let ice = match std::env::var("LAST_HARVEST_NET_TURN_URL") {
        Ok(turn) => IceConfig {
            servers: vec![IceServer {
                urls: vec![turn],
                username: std::env::var("LAST_HARVEST_NET_TURN_USERNAME").unwrap_or_default(),
                credential: std::env::var("LAST_HARVEST_NET_TURN_CREDENTIAL").unwrap_or_default(),
            }],
            relay_only: true,
        },
        Err(_) => IceConfig::default(),
    };
    let mut socket = PeerSocket::connect(&url, &ice);
    let end = Instant::now() + Duration::from_secs(seconds);
    let mut greet = Instant::now();
    while Instant::now() < end && !socket.signaling_closed() {
        for (peer, state) in socket.update_peers() {
            println!(
                "{:032x} {}",
                peer.0,
                if state == PeerState::Connected {
                    "connected"
                } else {
                    "left"
                }
            );
        }
        if Instant::now() >= greet {
            for peer in socket.connected_peers().to_vec() {
                for channel in [RELIABLE, UNRELIABLE] {
                    socket.send(channel, peer, format!("native-hello/{channel}").as_bytes());
                }
            }
            greet = Instant::now() + Duration::from_millis(500);
        }
        for channel in [RELIABLE, UNRELIABLE] {
            for (peer, data) in socket.receive(channel) {
                socket.send(channel, peer, &[b"echo:".as_slice(), &data].concat());
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    println!("closed");
}
