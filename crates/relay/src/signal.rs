//! Signaling on `/signal/<room>`: the matchbox protocol (`IdAssigned`, `NewPeer`, `PeerLeft`, `Signal`) as
//! `matchbox_server` speaks it, so a relay alone can pair a room's WebRTC peers, e.g. on a LAN without internet.
//! A peer also gets a `ResumeToken`: when its connection ends without a close, it keeps its place and ID for
//! `RESUME_GRACE`, and `/signal/<room>?resume=<id>&token=<token>` takes them back without a `PeerLeft`. The relay
//! echoes `KeepAlive`.

use crate::config::Config;
use crate::websocket::Poll;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::TcpStream;
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::http::StatusCode;
use tungstenite::protocol::WebSocketConfig;
use tungstenite::Message;

/// A signaling peer silent this long leaves its room.
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);
/// Messages queued for one peer; more are discarded.
const OUTBOX_LEN: usize = 256;
/// The largest signaling message, in bytes: an SDP with every ICE candidate gathered is a few KiB.
const MAX_MESSAGE_LEN: usize = 16 * 1024;
/// A game has at most four players, so at most four devices.
const MAX_ROOM_PEERS: usize = 4;
/// How long a disconnected peer keeps its place for a resume.
const RESUME_GRACE: Duration = Duration::from_secs(30);

/// A room's peer: the queue of messages to its connection, none while it is discarded, and the secret that resumes it.
struct Member {
    tx: Option<SyncSender<String>>,
    token: String,
    /// Set when a resume takes the place over, so the earlier connection's session ends without leaving.
    replaced: Arc<AtomicBool>,
}

/// Each signaling room's peers, by peer ID.
type Rooms = HashMap<String, HashMap<String, Member>>;

fn rooms() -> &'static Mutex<Rooms> {
    static ROOMS: OnceLock<Mutex<Rooms>> = OnceLock::new();
    ROOMS.get_or_init(Default::default)
}

fn random_bytes() -> [u8; 16] {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("the OS random source");
    b
}

fn new_token() -> String {
    random_bytes().iter().map(|x| format!("{x:02x}")).collect()
}

/// A random UUID v4, the form matchbox peer IDs take.
fn new_peer_id() -> String {
    let mut b = random_bytes();
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32])
}

/// Queues a message to one peer of a room; it is discarded if the peer is gone or its queue is full.
fn send_to(room: &str, peer: &str, text: String) {
    if let Some(tx) = rooms().lock().unwrap().get(room).and_then(|r| r.get(peer)).and_then(|m| m.tx.as_ref()) {
        let _ = tx.try_send(text);
    }
}

/// The `resume` and `token` of a `/signal` request's query string.
pub fn parse_resume(query: &str) -> Option<(String, String)> {
    let mut resume = None;
    let mut token = None;
    for pair in query.split('&') {
        match pair.split_once('=') {
            Some(("resume", v)) => resume = Some(v.to_string()),
            Some(("token", v)) => token = Some(v.to_string()),
            _ => {}
        }
    }
    Some((resume?, token?))
}

/// How a peer joins its room: a new peer is told to the others, a resumed one is not.
enum Join {
    New(Vec<SyncSender<String>>),
    Resumed,
}

/// Adds `me` to `room`, or puts a resumed peer's new connection in its place. None when the room is full or the
/// resume does not match a peer.
fn join(room: &str, me: &str, resume: bool, token: &str, tx: SyncSender<String>, replaced: Arc<AtomicBool>) -> Option<Join> {
    let mut rooms = rooms().lock().unwrap();
    if resume {
        let member = rooms.get_mut(room)?.get_mut(me).filter(|m| m.token == token)?;
        member.replaced.store(true, Ordering::Release);
        *member = Member { tx: Some(tx), token: member.token.clone(), replaced };
        return Some(Join::Resumed);
    }
    let peers = rooms.entry(room.to_string()).or_default();
    if peers.len() >= MAX_ROOM_PEERS {
        return None;
    }
    let others = peers.values().filter_map(|m| m.tx.clone()).collect();
    peers.insert(me.to_string(), Member { tx: Some(tx), token: token.to_string(), replaced });
    Some(Join::New(others))
}

/// Removes `me` from `room` and tells the others, unless a resume replaced this connection.
fn leave(room: &str, me: &str, replaced: &AtomicBool) {
    let remaining: Vec<SyncSender<String>> = {
        let mut rooms = rooms().lock().unwrap();
        if replaced.load(Ordering::Acquire) {
            return;
        }
        let Some(peers) = rooms.get_mut(room) else { return };
        peers.remove(me);
        let remaining = peers.values().filter_map(|m| m.tx.clone()).collect();
        if peers.is_empty() {
            rooms.remove(room);
        }
        remaining
    };
    for other in remaining {
        let _ = other.try_send(json!({ "PeerLeft": me }).to_string());
    }
}

/// Keeps a disconnected peer's place for `grace`, then removes it unless it resumed.
fn hold(room: String, me: String, replaced: Arc<AtomicBool>, grace: Duration) {
    {
        let mut rooms = rooms().lock().unwrap();
        if replaced.load(Ordering::Acquire) {
            return;
        }
        if let Some(member) = rooms.get_mut(&room).and_then(|r| r.get_mut(&me)) {
            member.tx = None;
        }
    }
    std::thread::spawn(move || {
        std::thread::sleep(grace);
        leave(&room, &me, &replaced);
    });
}

/// One peer's signaling session in `room`, on its connection's thread, until it closes or goes quiet.
pub fn run(stream: TcpStream, room: String, resume: Option<(String, String)>, config: &Config) {
    #[allow(clippy::result_large_err)] // tungstenite's handshake callback signature
    let check_origin = |request: &Request, response: Response| -> Result<Response, ErrorResponse> {
        let origin = request.headers().get("origin").and_then(|v| v.to_str().ok());
        if config.origin_allowed(origin) {
            return Ok(response);
        }
        let mut refusal = ErrorResponse::new(Some("origin not allowed\n".to_string()));
        *refusal.status_mut() = StatusCode::FORBIDDEN;
        Err(refusal)
    };
    let ws_config = WebSocketConfig::default().max_message_size(Some(MAX_MESSAGE_LEN)).max_frame_size(Some(MAX_MESSAGE_LEN));
    let Ok(mut ws) = tungstenite::accept_hdr_with_config(stream, check_origin, Some(ws_config)) else { return };
    let mut poll = Poll::new(ws.get_ref());

    let resuming = resume.is_some();
    let (me, token) = resume.unwrap_or_else(|| (new_peer_id(), new_token()));
    let (tx, outgoing) = sync_channel::<String>(OUTBOX_LEN);
    let _ = tx.try_send(json!({ "IdAssigned": me }).to_string());
    let _ = tx.try_send(json!({ "ResumeToken": token }).to_string());
    let replaced = Arc::new(AtomicBool::new(false));
    let own = tx.clone();
    // Reading the room's peers and adding this one under one lock: two peers joining at once still pair up.
    match join(&room, &me, resuming, &token, tx, replaced.clone()) {
        Some(Join::New(others)) => {
            for other in others {
                let _ = other.try_send(json!({ "NewPeer": me }).to_string());
            }
        }
        Some(Join::Resumed) => println!("[relay] signaling peer resumed"),
        None => {
            println!("[relay] refused a signaling peer: {}", if resuming { "no place to resume" } else { "room full" });
            let _ = ws.close(None);
            let _ = ws.flush();
            return;
        }
    }

    let mut last_heard = Instant::now();
    let mut closed = false;
    'session: loop {
        if replaced.load(Ordering::Acquire) {
            return;
        }
        loop {
            match ws.read() {
                Ok(Message::Text(text)) => {
                    last_heard = Instant::now();
                    poll.active();
                    let Ok(request) = serde_json::from_str::<Value>(&text) else { continue };
                    // Echoed, so a client learns within seconds that a connection has died.
                    if request == "KeepAlive" {
                        let _ = own.try_send(text.to_string());
                        continue;
                    }
                    if let Some(signal) = request.get("Signal") {
                        let (Some(receiver), Some(data)) = (signal.get("receiver").and_then(Value::as_str), signal.get("data")) else { continue };
                        send_to(&room, receiver, json!({ "Signal": { "sender": me, "data": data } }).to_string());
                    }
                }
                Ok(Message::Close(_)) => {
                    closed = true;
                    break 'session;
                }
                Ok(_) => last_heard = Instant::now(),
                Err(tungstenite::Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => break,
                Err(_) => break 'session,
            }
        }
        let mut wrote = false;
        while let Ok(text) = outgoing.try_recv() {
            if ws.write(Message::Text(text.into())).is_err() {
                break 'session;
            }
            wrote = true;
        }
        if wrote {
            poll.active();
            if ws.flush().is_err() {
                break;
            }
        }
        if last_heard.elapsed() >= IDLE_TIMEOUT {
            break;
        }
        poll.update(ws.get_ref());
    }

    if closed {
        leave(&room, &me, &replaced);
    } else {
        hold(room, me, replaced, RESUME_GRACE);
    }
}

/// Holds `me`'s place in `room` for `grace` as if its connection had been lost, and ends the session holding it.
#[cfg(test)]
pub fn hold_for_test(room: &str, me: &str, grace: Duration) {
    let replaced = Arc::new(AtomicBool::new(false));
    if let Some(member) = rooms().lock().unwrap().get_mut(room).and_then(|r| r.get_mut(me)) {
        member.replaced.store(true, Ordering::Release);
        member.replaced = replaced.clone();
    }
    hold(room.to_string(), me.to_string(), replaced, grace);
}
