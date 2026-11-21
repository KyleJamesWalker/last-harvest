//! A local full-mesh matchbox signaling server for tests: `cargo run -p last-harvest-net --example signal [port]`.
//! Prints `listening <port>` once bound.

use matchbox_signaling::SignalingServer;

#[tokio::main]
async fn main() {
    let port = std::env::args()
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(0);
    let mut server = SignalingServer::full_mesh_builder(([127, 0, 0, 1], port)).build();
    let addr = server.bind().expect("bind the signaling port");
    println!("listening {}", addr.port());
    server.serve().await.expect("signaling server");
}
