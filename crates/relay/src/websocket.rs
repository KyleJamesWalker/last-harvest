//! The relay's sockets: one thread accepts, and a thread per connection serves signaling, `/healthz` or `/ice`.

use crate::config::Config;
use crate::limits::{self, AddressLimits, AddressSlot};
use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// How long a new connection has to send its HTTP request head and finish the handshake.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
/// How long a socket thread waits for input before it sends what other threads queued, while traffic flows.
const POLL_ACTIVE: Duration = Duration::from_millis(4);
/// The same once a connection has been quiet for `ACTIVE_WINDOW`.
const POLL_IDLE: Duration = Duration::from_millis(50);
const ACTIVE_WINDOW: Duration = Duration::from_secs(2);
/// The largest HTTP request head the relay reads, in bytes.
const MAX_REQUEST_HEAD: usize = 8 * 1024;

/// Counts of the connections the relay serves, each under its own limit.
#[derive(Clone)]
struct Counts {
    connections: Arc<AtomicUsize>,
    signaling: Arc<AtomicUsize>,
    addresses: Arc<AddressLimits>,
    /// Signaling peers per client address.
    peers: Arc<AddressLimits>,
}

/// One place under a limit, given back on drop.
struct Slot(Arc<AtomicUsize>);

impl Slot {
    fn take(count: &Arc<AtomicUsize>, max: usize) -> Option<Slot> {
        count.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| (n < max).then_some(n + 1)).ok().map(|_| Slot(count.clone()))
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// A socket's read timeout: `POLL_ACTIVE` while traffic flows, `POLL_IDLE` once it stops.
pub struct Poll {
    last_active: Instant,
    timeout: Duration,
}

impl Poll {
    pub fn new(stream: &TcpStream) -> Poll {
        let _ = stream.set_read_timeout(Some(POLL_ACTIVE));
        Poll { last_active: Instant::now(), timeout: POLL_ACTIVE }
    }

    pub fn active(&mut self) {
        self.last_active = Instant::now();
    }

    /// Sets the timeout for the next read.
    pub fn update(&mut self, stream: &TcpStream) {
        let timeout = if self.last_active.elapsed() < ACTIVE_WINDOW { POLL_ACTIVE } else { POLL_IDLE };
        if timeout != self.timeout {
            let _ = stream.set_read_timeout(Some(timeout));
            self.timeout = timeout;
        }
    }
}

/// Binds `config.port` and serves it from background threads. Returns the port.
pub fn serve(config: &Config) -> io::Result<u16> {
    let listener = TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], config.port)))?;
    let port = listener.local_addr()?.port();
    let config = Arc::new(config.clone());
    let counts = Counts {
        connections: Default::default(),
        signaling: Default::default(),
        addresses: AddressLimits::new(config.max_per_address, config.connects_per_minute),
        peers: AddressLimits::new(config.max_peers_per_address, 0),
    };
    thread::spawn(move || {
        let mut full = false;
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let Some(slot) = Slot::take(&counts.connections, config.max_connections) else {
                if !full {
                    println!("[relay] refusing connections: {} open", config.max_connections);
                }
                full = true;
                continue;
            };
            full = false;
            // A connection from a possible proxy is counted once its X-Forwarded-For has arrived.
            let address = match stream.peer_addr().map(|a| a.ip()) {
                Ok(ip) if !limits::may_proxy(ip, config.trusted_proxies) => match counts.addresses.open(ip, Instant::now()) {
                    Ok(address) => Some(address),
                    Err(_) => continue,
                },
                Ok(_) => None,
                Err(_) => continue,
            };
            let (config, counts) = (config.clone(), counts.clone());
            let _ = thread::Builder::new().spawn(move || {
                let _slot = slot;
                handle_connection(stream, address, &config, &counts);
            });
        }
    });
    Ok(port)
}

/// Routes one TCP connection: signaling, `/healthz` or `/ice`.
fn handle_connection(
    mut stream: TcpStream,
    address: Option<AddressSlot>,
    config: &Config,
    counts: &Counts,
) {
    let _ = stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT));
    let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
    let _ = stream.set_nodelay(true);

    let Some((head, head_len)) = peek_request_head(&stream) else { return };
    let peer = stream.peer_addr().map_or(IpAddr::V4(Ipv4Addr::LOCALHOST), |a| a.ip());
    let client = if limits::may_proxy(peer, config.trusted_proxies) { limits::forwarded_for(&head).unwrap_or(peer) } else { peer };
    let _address = match address {
        Some(address) => address,
        None => {
            match counts.addresses.open(client, Instant::now()) {
                Ok(address) => address,
                Err(_) => {
                    refuse(&mut stream, head_len, "429 Too Many Requests", "too many connections\n");
                    return;
                }
            }
        }
    };
    let (path, query, upgrade) = parse_request_head(&head);
    if let Some(room) = path.strip_prefix("/signal/").filter(|r| upgrade && !r.is_empty()) {
        let Some(_slot) = Slot::take(&counts.signaling, config.max_clients) else {
            println!("[relay] refused a signaling client: relay full");
            refuse(&mut stream, head_len, "503 Service Unavailable", "relay full\n");
            return;
        };
        let Ok(_peer) = counts.peers.open(client, Instant::now()) else {
            refuse(&mut stream, head_len, "429 Too Many Requests", "too many signaling peers\n");
            return;
        };
        crate::signal::run(stream, room.to_string(), crate::signal::parse_resume(&query), config);
        return;
    }
    let mut consumed = vec![0; head_len];
    let _ = stream.read_exact(&mut consumed);
    match path.as_str() {
        "/healthz" => respond(&mut stream, "200 OK", "text/plain", "ok\n"),
        "/ice" => ice(&mut stream, &head, config),
        "/" if upgrade => respond(&mut stream, "404 Not Found", "text/plain", "last-harvest-relay: signaling is on /signal/<room>\n"),
        "/" => respond(&mut stream, "426 Upgrade Required", "text/plain", "last-harvest-relay: connect with a WebSocket to /signal/<room>\n"),
        _ => respond(&mut stream, "404 Not Found", "text/plain", "not found\n"),
    }
}

/// TURN credentials for a page the relay allows, or a native game.
fn ice(stream: &mut TcpStream, head: &str, config: &Config) {
    let origin = limits::header(head, "origin");
    if !config.origin_allowed(origin) {
        respond(stream, "403 Forbidden", "text/plain", "origin not allowed\n");
        return;
    }
    let body = crate::turn::ice_servers(config, crate::turn::now_unix());
    let allow = origin.map(|o| format!("Access-Control-Allow-Origin: {o}\r\nVary: Origin\r\n")).unwrap_or_default();
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nCache-Control: no-store\r\n{allow}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn refuse(stream: &mut TcpStream, head_len: usize, status: &str, body: &str) {
    let mut consumed = vec![0; head_len];
    let _ = stream.read_exact(&mut consumed);
    respond(stream, status, "text/plain", body);
}

/// One lobby client's WebSocket, until it closes, falls behind or goes quiet.
/// The HTTP request head and its length in bytes, left unread in the socket.
fn peek_request_head(stream: &TcpStream) -> Option<(String, usize)> {
    let mut buf = vec![0; MAX_REQUEST_HEAD];
    let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
    loop {
        let n = stream.peek(&mut buf).ok()?;
        if n == 0 {
            return None;
        }
        if let Some(end) = buf[..n].windows(4).position(|w| w == b"\r\n\r\n") {
            return Some((String::from_utf8_lossy(&buf[..end]).into_owned(), end + 4));
        }
        if n == buf.len() || Instant::now() >= deadline {
            return None;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// The path, and whether the request asks for a WebSocket.
fn parse_request_head(head: &str) -> (String, String, bool) {
    let mut lines = head.lines();
    let target = lines.next().and_then(|l| l.split_whitespace().nth(1)).unwrap_or("");
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let (path, query) = (path.to_string(), query.to_string());
    let upgrade = lines.any(|l| {
        let (name, value) = l.split_once(':').unwrap_or((l, ""));
        name.trim().eq_ignore_ascii_case("upgrade") && value.to_ascii_lowercase().contains("websocket")
    });
    (path, query, upgrade)
}

fn respond(stream: &mut TcpStream, status: &str, content_type: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

/// True when `127.0.0.1:<port>` answers `GET /healthz` with 200.
pub fn healthcheck(port: u16) -> bool {
    let Ok(mut stream) = TcpStream::connect_timeout(&SocketAddr::from(([127, 0, 0, 1], port)), Duration::from_secs(3)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    if stream.write_all(b"GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").is_err() {
        return false;
    }
    let mut response = String::new();
    let _ = stream.read_to_string(&mut response);
    response.starts_with("HTTP/1.1 200")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::ErrorKind;
    use tungstenite::stream::MaybeTlsStream;
    use tungstenite::{Message, WebSocket};

    fn relay(config: impl FnOnce(&mut Config)) -> u16 {
        let mut c = Config::embedded(0);
        config(&mut c);
        serve(&c).unwrap()
    }

    fn open(port: u16) -> TcpStream {
        let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.set_read_timeout(Some(Duration::from_millis(500))).unwrap();
        stream
    }

    /// Whether the relay closed `stream` rather than waiting for its request.
    fn closed(stream: &mut TcpStream) -> bool {
        match stream.read(&mut [0; 64]) {
            Ok(0) => true,
            Ok(_) => false,
            Err(e) => !matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut),
        }
    }

    /// The value of a signaling message's `key`, skipping other messages.
    fn read_field(ws: &mut WebSocket<MaybeTlsStream<TcpStream>>, key: &str) -> Option<String> {
        loop {
            let value: serde_json::Value = serde_json::from_str(&read_text(ws)?).ok()?;
            if let Some(field) = value.get(key) {
                return Some(field.as_str().map_or_else(|| field.to_string(), str::to_string));
            }
        }
    }

    /// Every message until the read times out.
    fn drain(ws: &mut WebSocket<MaybeTlsStream<TcpStream>>) -> Vec<String> {
        std::iter::from_fn(|| read_text(ws)).collect()
    }

    fn signal(port: u16, room: &str) -> Result<WebSocket<MaybeTlsStream<TcpStream>>, String> {
        signal_from(port, room, None)
    }

    /// A signaling peer whose request says, as a proxy would, that it comes from `forwarded`.
    fn signal_from(port: u16, room: &str, forwarded: Option<&str>) -> Result<WebSocket<MaybeTlsStream<TcpStream>>, String> {
        use tungstenite::client::IntoClientRequest;
        let stream = open(port);
        stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut request = format!("ws://127.0.0.1:{port}/signal/{room}").into_client_request().unwrap();
        if let Some(address) = forwarded {
            request.headers_mut().insert("x-forwarded-for", address.parse().unwrap());
        }
        tungstenite::client::client(request, MaybeTlsStream::Plain(stream)).map(|(ws, _)| ws).map_err(|e| e.to_string())
    }

    fn read_text(ws: &mut WebSocket<MaybeTlsStream<TcpStream>>) -> Option<String> {
        match ws.read() {
            Ok(Message::Text(text)) => Some(text.to_string()),
            _ => None,
        }
    }

    #[test]
    fn connections_over_the_limit_are_closed_at_once() {
        let port = relay(|c| c.max_connections = 2);
        let mut held = [open(port), open(port)];
        let mut extra = open(port);
        assert!(closed(&mut extra));
        assert!(!closed(&mut held[1]), "the relay waits for an open connection's request");

        drop(held);
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let mut retry = open(port);
            if !closed(&mut retry) {
                break;
            }
            assert!(Instant::now() < deadline, "closed connections free their places");
        }
    }

    #[test]
    fn a_forwarded_address_holds_at_most_max_per_address() {
        let port = relay(|c| c.max_per_address = 2);
        let _held: Vec<_> = (0..2).map(|i| signal_from(port, &format!("addr-{i}"), Some("203.0.113.9")).unwrap()).collect();
        assert!(signal_from(port, "addr-x", Some("203.0.113.9")).unwrap_err().contains("429"));
        assert!(signal_from(port, "addr-y", Some("198.51.100.1")).is_ok(), "other addresses are not affected");
        let _loopback: Vec<_> = (0..3).map(|i| signal(port, &format!("addr-lo-{i}")).unwrap()).collect();
    }

    #[test]
    fn an_address_holds_at_most_max_peers_per_address() {
        let port = relay(|c| c.max_peers_per_address = 2);
        let mut held: Vec<_> = (0..2).map(|i| signal_from(port, &format!("peers-{i}"), Some("2001:db8:7:1::1")).unwrap()).collect();
        assert!(signal_from(port, "peers-x", Some("2001:db8:7:2::9")).unwrap_err().contains("429"), "the same /56");
        assert!(signal_from(port, "peers-y", Some("198.51.100.2")).is_ok(), "other addresses are not affected");
        assert_eq!(get(port, "/healthz", None).lines().next(), Some("HTTP/1.1 200 OK"), "other requests are not affected");

        held[0].close(None).unwrap();
        let _ = held[0].flush();
        let deadline = Instant::now() + Duration::from_secs(3);
        while signal_from(port, "peers-z", Some("2001:db8:7:1::1")).is_err() {
            assert!(Instant::now() < deadline, "a closed peer frees its place");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn a_peer_alone_in_its_room_is_closed() {
        let port = relay(|c| c.lonely_timeout = Duration::from_millis(300));
        let mut alone = signal(port, "lonely-a").unwrap();
        let mut pair: Vec<_> = (0..2).map(|_| signal(port, "lonely-b").unwrap()).collect();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match alone.read() {
                Ok(Message::Close(_)) | Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => break,
                Err(tungstenite::Error::Io(ref e)) if !matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => break,
                _ => assert!(Instant::now() < deadline, "the relay kept a lonely peer"),
            }
        }
        for peer in &mut pair {
            peer.send(Message::Text("\"KeepAlive\"".into())).unwrap();
            assert!(drain(peer).iter().any(|t| t.contains("KeepAlive")), "a peer with company stays");
        }
    }

    #[test]
    fn signaling_peers_count_against_max_clients() {
        let port = relay(|c| c.max_clients = 1);
        let mut first = signal(port, "limit-a").unwrap();
        assert!(read_text(&mut first).unwrap().contains("IdAssigned"));
        assert!(signal(port, "limit-b").unwrap_err().contains("503"));
    }

    #[test]
    fn signaling_rooms_hold_four_peers() {
        let port = relay(|_| {});
        let mut peers: Vec<_> = (0..4).map(|_| signal(port, "four").unwrap()).collect();
        for peer in &mut peers {
            assert!(read_text(peer).unwrap().contains("IdAssigned"));
        }
        let mut fifth = signal(port, "four").unwrap();
        assert_eq!(read_text(&mut fifth), None);
    }

    #[test]
    fn signaling_closes_on_an_oversize_message() {
        let port = relay(|_| {});
        let mut peer = signal(port, "oversize").unwrap();
        assert!(read_text(&mut peer).unwrap().contains("IdAssigned"));
        let _ = peer.send(Message::Text("x".repeat(64 * 1024).into()));
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if let Err(error) = peer.read() {
                assert!(!matches!(error, tungstenite::Error::Io(ref e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)), "{error}");
                return;
            }
        }
        panic!("the relay kept the connection");
    }

    #[test]
    fn a_dropped_signaling_peer_resumes_its_place() {
        let port = relay(|_| {});
        let mut first = signal(port, "resume").unwrap();
        let id = read_field(&mut first, "IdAssigned").unwrap();
        let token = read_field(&mut first, "ResumeToken").unwrap();
        let mut second = signal(port, "resume").unwrap();
        let second_id = read_field(&mut second, "IdAssigned").unwrap();
        assert_eq!(read_field(&mut first, "NewPeer").unwrap(), second_id);

        // A connection lost without a close frame keeps its place.
        drop(first);
        assert!(!drain(&mut second).iter().any(|t| t.contains("PeerLeft")), "no PeerLeft while the place is held");

        assert_eq!(read_text(&mut signal(port, &format!("resume?resume={id}&token=wrong")).unwrap()), None);
        let mut resumed = signal(port, &format!("resume?resume={id}&token={token}")).unwrap();
        assert_eq!(read_field(&mut resumed, "IdAssigned").unwrap(), id);
        assert!(!drain(&mut second).iter().any(|t| t.contains("NewPeer")), "a resume is not a new peer");
        let hello = serde_json::json!({ "Signal": { "receiver": id, "data": "hello" } });
        second.send(Message::Text(hello.to_string().into())).unwrap();
        assert_eq!(read_field(&mut resumed, "Signal").unwrap(), serde_json::json!({ "sender": second_id, "data": "hello" }).to_string());

        // A close frame leaves at once.
        resumed.close(None).unwrap();
        let _ = resumed.flush();
        assert_eq!(read_field(&mut second, "PeerLeft").unwrap(), id);
    }

    #[test]
    fn a_held_place_expires() {
        let port = relay(|_| {});
        let mut first = signal(port, "expire").unwrap();
        let id = read_field(&mut first, "IdAssigned").unwrap();
        let token = read_field(&mut first, "ResumeToken").unwrap();
        let mut second = signal(port, "expire").unwrap();
        read_field(&mut second, "IdAssigned").unwrap();
        crate::signal::hold_for_test("expire", &id, Duration::from_millis(200));
        assert_eq!(read_field(&mut second, "PeerLeft").unwrap(), id);
        assert_eq!(read_text(&mut signal(port, &format!("expire?resume={id}&token={token}")).unwrap()), None);
    }

    fn get(port: u16, path: &str, origin: Option<&str>) -> String {
        let mut stream = open(port);
        stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let origin = origin.map(|o| format!("Origin: {o}\r\n")).unwrap_or_default();
        write!(stream, "GET {path} HTTP/1.1\r\nHost: x\r\n{origin}Connection: close\r\n\r\n").unwrap();
        let mut response = String::new();
        let _ = stream.read_to_string(&mut response);
        response
    }

    #[test]
    fn healthz_and_ice_answer() {
        let port = relay(|c| {
            c.allowed_origins = vec!["https://game.example.com".into()];
            c.turn_url = Some("turn:relay.example.com:3478".into());
            c.turn_secret = Some("00ff".into());
        });
        assert!(healthcheck(port));
        let native = get(port, "/ice", None);
        assert!(native.starts_with("HTTP/1.1 200") && native.contains("turn:relay.example.com:3478"), "{native}");
        let page = get(port, "/ice", Some("https://game.example.com"));
        assert!(page.contains("Access-Control-Allow-Origin: https://game.example.com"), "{page}");
        assert!(get(port, "/ice", Some("https://evil.example")).starts_with("HTTP/1.1 403"));
        assert!(get(port, "/nope", None).starts_with("HTTP/1.1 404"));
    }

    #[test]
    fn request_heads() {
        assert_eq!(parse_request_head("GET /healthz HTTP/1.1\r\nHost: x"), ("/healthz".to_string(), String::new(), false));
        assert_eq!(
            parse_request_head("GET /?a=b HTTP/1.1\r\nHost: x\r\nConnection: Upgrade\r\nUpgrade: WebSocket"),
            ("/".to_string(), "a=b".to_string(), true)
        );
        assert_eq!(parse_request_head(""), (String::new(), String::new(), false));
    }
}
