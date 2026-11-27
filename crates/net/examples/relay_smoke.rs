//! Checks a deployed relay end to end: `/ice`, the origin check, TURN credentials over signaling, and two native peers
//! trading a packet on each channel, directly and then through TURN alone.
//! Usage: `cargo run -p last-harvest-net --example relay_smoke -- [relay] [allowed page origin]`.

use futures_util::StreamExt;
use last_harvest_net::{IceConfig, IceFetch, IceServer, PeerSocket, RELIABLE, UNRELIABLE};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

const DEFAULT_RELAY: &str = "wss://lh-relay.vps.pocketsquirrel.com";
const DEFAULT_ORIGIN: &str = "https://www.kylejameswalker.com";
const TIMEOUT: Duration = Duration::from_secs(20);

fn main() {
    let mut args = std::env::args().skip(1);
    let relay = args.next().unwrap_or_else(|| DEFAULT_RELAY.to_string());
    let origin = args.next().unwrap_or_else(|| DEFAULT_ORIGIN.to_string());
    let relay = relay.trim_end_matches('/');
    let (ws, http) = match relay.split_once("://") {
        Some(("wss" | "https", rest)) => (format!("wss://{rest}"), format!("https://{rest}")),
        Some(("ws" | "http", rest)) => (format!("ws://{rest}"), format!("http://{rest}")),
        _ => fail(&format!(
            "{relay}: give the relay as a ws, wss, http or https URL"
        )),
    };
    let tag = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
        % 0xff_ffff;
    let room = |what: &str| format!("{ws}/signal/last-harvest-smoke-{what}-{tag:06x}");

    let ice = fetch_ice(&format!("{http}/ice"));
    println!("ok   /ice answered with {} server(s)", ice.servers.len());

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a tokio runtime");
    let refused = runtime.block_on(probe(&room("evil"), Some("https://not-the-game.example")));
    match refused {
        Err(e) if e.contains("403") => println!("ok   a page from another origin is refused"),
        other => fail(&format!(
            "a page from another origin was not refused: {other:?}"
        )),
    }
    let turn = match runtime.block_on(probe(&room("origin"), Some(&origin))) {
        Ok(turn) => turn,
        Err(e) => fail(&format!("the page origin {origin} was refused: {e}")),
    };
    match &turn {
        Some(server) => println!(
            "ok   signaling from {origin} gave TURN credentials for {}",
            server.urls.join(", ")
        ),
        None => println!(
            "warn signaling from {origin} gave no TURN credentials: the relay has no TURN URL and secret"
        ),
    }

    exchange(&room("direct"), &IceConfig::default());
    println!("ok   two peers traded a packet on each channel");
    if let Some(server) = turn {
        exchange(
            &room("turn"),
            &IceConfig {
                servers: vec![server],
                relay_only: true,
            },
        );
        println!("ok   two peers traded a packet on each channel through TURN alone");
    }
    println!("all checks passed");
}

fn fail(message: &str) -> ! {
    eprintln!("FAIL {message}");
    std::process::exit(1);
}

fn fetch_ice(url: &str) -> IceConfig {
    let mut fetch = IceFetch::start(url);
    let end = Instant::now() + TIMEOUT;
    loop {
        match fetch.poll() {
            Some(Ok(ice)) => return ice,
            Some(Err(e)) => fail(&format!("{url}: {e}")),
            None if Instant::now() > end => fail(&format!("{url}: no answer")),
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

/// Joins `url` as a page from `origin` would and returns the TURN server the relay sends, if any.
async fn probe(url: &str, origin: Option<&str>) -> Result<Option<IceServer>, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut request = url.into_client_request().map_err(|e| e.to_string())?;
    if let Some(origin) = origin {
        request.headers_mut().insert(
            "origin",
            origin.parse().map_err(|_| format!("bad origin {origin}"))?,
        );
    }
    let (mut socket, _) = tokio::time::timeout(TIMEOUT, tokio_tungstenite::connect_async(request))
        .await
        .map_err(|_| "no handshake".to_string())?
        .map_err(|e| e.to_string())?;
    let mut assigned = false;
    let read = async {
        while let Some(Ok(message)) = socket.next().await {
            let Message::Text(text) = message else {
                continue;
            };
            let event: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
            assigned |= event.get("IdAssigned").is_some();
            if let Some(servers) = event.get("IceServers") {
                return last_harvest_net::parse_ice(&servers.to_string())
                    .map(|ice| ice.servers.into_iter().next());
            }
        }
        Err("signaling closed".to_string())
    };
    let turn = match tokio::time::timeout(Duration::from_secs(3), read).await {
        Ok(result) => result?,
        Err(_) if assigned => None,
        Err(_) => return Err("no IdAssigned".to_string()),
    };
    let _ = socket.close(None).await;
    Ok(turn)
}

/// Two peers join `url` and each sends the other a packet on both channels.
fn exchange(url: &str, ice: &IceConfig) {
    let end = Instant::now() + TIMEOUT;
    let mut a = PeerSocket::connect(url, ice);
    while a.id().is_none() {
        if a.signaling_closed() || Instant::now() > end {
            fail(&format!("{url}: no peer id"));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut b = PeerSocket::connect(url, ice);
    while a.connected_peers().is_empty() || b.connected_peers().is_empty() {
        if Instant::now() > end {
            fail(&format!("{url}: the peers did not connect"));
        }
        a.update_peers();
        b.update_peers();
        std::thread::sleep(Duration::from_millis(10));
    }
    let (a_id, b_id) = (a.id().unwrap(), b.id().unwrap());
    let mut got = [[false; 2]; 2];
    let mut resend = Instant::now();
    while got.iter().flatten().any(|g| !g) {
        if Instant::now() > end {
            fail(&format!(
                "{url}: packets missing, got {got:?} (a reliable, a unreliable; b reliable, b unreliable)"
            ));
        }
        if Instant::now() >= resend {
            for channel in [RELIABLE, UNRELIABLE] {
                a.send(channel, b_id, b"smoke from a");
                b.send(channel, a_id, b"smoke from b");
            }
            resend = Instant::now() + Duration::from_millis(250);
        }
        for channel in [RELIABLE, UNRELIABLE] {
            got[0][channel] |= a
                .receive(channel)
                .iter()
                .any(|(p, d)| *p == b_id && d == b"smoke from b");
            got[1][channel] |= b
                .receive(channel)
                .iter()
                .any(|(p, d)| *p == a_id && d == b"smoke from a");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    a.close();
    b.close();
}
