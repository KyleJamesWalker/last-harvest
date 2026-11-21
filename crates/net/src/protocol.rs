//! The matchbox signaling protocol: server events, relayed WebRTC handshake messages, and how a relay resumes a peer.

use crate::{IceServer, PeerId};
use serde_json::{json, Value};

/// Sent every few seconds; a relay that resumes peers echoes it.
pub(crate) const KEEP_ALIVE: &str = "\"KeepAlive\"";

/// A WebRTC handshake message. An ICE candidate is the JSON of an `RTCIceCandidateInit`, or `null`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Signal {
    Offer(String),
    Answer(String),
    IceCandidate(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    IdAssigned(PeerId),
    ResumeToken(String),
    /// The relay's ICE servers with fresh TURN credentials, sent after joining and again before they expire.
    IceServers(Vec<IceServer>),
    NewPeer(PeerId),
    PeerLeft(PeerId),
    Signal(PeerId, Signal),
}

pub(crate) fn uuid_string(peer: PeerId) -> String {
    uuid::Uuid::from_u128(peer.0).hyphenated().to_string()
}

pub(crate) fn parse_uuid(text: &str) -> Option<PeerId> {
    uuid::Uuid::parse_str(text).ok().map(|u| PeerId(u.as_u128()))
}

/// One message from the signaling server; None for anything else, such as an echoed keep-alive.
pub(crate) fn parse_event(text: &str) -> Option<Event> {
    let event: Value = serde_json::from_str(text).ok()?;
    let id = |key: &str| event.get(key).and_then(Value::as_str).and_then(parse_uuid);
    if let Some(peer) = id("IdAssigned") {
        return Some(Event::IdAssigned(peer));
    }
    if let Some(token) = event.get("ResumeToken").and_then(Value::as_str) {
        return Some(Event::ResumeToken(token.to_string()));
    }
    if let Some(servers) = event.get("IceServers") {
        return crate::ice::parse_ice_value(servers).ok().map(|ice| Event::IceServers(ice.servers));
    }
    if let Some(peer) = id("NewPeer") {
        return Some(Event::NewPeer(peer));
    }
    if let Some(peer) = id("PeerLeft") {
        return Some(Event::PeerLeft(peer));
    }
    let signal = event.get("Signal")?;
    let sender = signal.get("sender").and_then(Value::as_str).and_then(parse_uuid)?;
    let data = signal.get("data")?;
    let text = |key: &str| data.get(key).and_then(Value::as_str).map(str::to_string);
    let signal = if let Some(sdp) = text("Offer") {
        Signal::Offer(sdp)
    } else if let Some(sdp) = text("Answer") {
        Signal::Answer(sdp)
    } else {
        Signal::IceCandidate(text("IceCandidate")?)
    };
    Some(Event::Signal(sender, signal))
}

/// The message that relays `signal` to `receiver`.
pub(crate) fn signal_message(receiver: PeerId, signal: &Signal) -> String {
    let data = match signal {
        Signal::Offer(sdp) => json!({ "Offer": sdp }),
        Signal::Answer(sdp) => json!({ "Answer": sdp }),
        Signal::IceCandidate(candidate) => json!({ "IceCandidate": candidate }),
    };
    json!({ "Signal": { "receiver": uuid_string(receiver), "data": data } }).to_string()
}

/// `url` asking the relay for a place back; the connection's first message must be `resume_message`.
pub(crate) fn resume_url(url: &str) -> String {
    let join = if url.contains('?') { '&' } else { '?' };
    format!("{url}{join}resume")
}

/// Names the place to resume, so its token never appears in a URL.
pub(crate) fn resume_message(id: PeerId, token: &str) -> String {
    json!({ "Resume": { "id": uuid_string(id), "token": token } }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "67e55044-10b1-426f-9247-bb680e5fe0c8";

    #[test]
    fn server_events_parse() {
        let a = parse_uuid(A).unwrap();
        assert_eq!(uuid_string(a), A);
        assert_eq!(parse_event(&format!(r#"{{"IdAssigned":"{A}"}}"#)), Some(Event::IdAssigned(a)));
        assert_eq!(parse_event(r#"{"ResumeToken":"t0k"}"#), Some(Event::ResumeToken("t0k".into())));
        assert_eq!(parse_event(&format!(r#"{{"NewPeer":"{A}"}}"#)), Some(Event::NewPeer(a)));
        assert_eq!(parse_event(&format!(r#"{{"PeerLeft":"{A}"}}"#)), Some(Event::PeerLeft(a)));
        let turn = IceServer { urls: vec!["turn:t.example.com:3478".into()], username: "1:lh-ab".into(), credential: "x".into() };
        let ice = r#"{"IceServers":{"iceServers":[{"urls":["turn:t.example.com:3478"],"username":"1:lh-ab","credential":"x"}]}}"#;
        assert_eq!(parse_event(ice), Some(Event::IceServers(vec![turn])));
        assert_eq!(parse_event(r#"{"IceServers":{}}"#), None);
        assert_eq!(parse_event(KEEP_ALIVE), None);
        assert_eq!(parse_event(r#"{"NewPeer":"not a uuid"}"#), None);
        assert_eq!(parse_event("not json"), None);
    }

    #[test]
    fn a_relayed_signal_reads_back_as_the_receiver_gets_it() {
        let (a, b) = (parse_uuid(A).unwrap(), PeerId(0xbeef));
        for signal in [Signal::Offer("v=0 offer".into()), Signal::Answer("v=0 answer".into()), Signal::IceCandidate("null".into())] {
            let sent: Value = serde_json::from_str(&signal_message(b, &signal)).unwrap();
            assert_eq!(sent["Signal"]["receiver"], uuid_string(b));
            // The server swaps the receiver for the sender and passes `data` through untouched.
            let relayed = json!({ "Signal": { "sender": A, "data": sent["Signal"]["data"] } }).to_string();
            assert_eq!(parse_event(&relayed), Some(Event::Signal(a, signal)));
        }
    }

    #[test]
    fn a_resume_keeps_its_token_out_of_the_url() {
        let a = parse_uuid(A).unwrap();
        assert_eq!(resume_url("ws://h/room"), "ws://h/room?resume");
        assert_eq!(resume_url("ws://h/room?x=1"), "ws://h/room?x=1&resume");
        let message: Value = serde_json::from_str(&resume_message(a, "a b+c")).unwrap();
        assert_eq!(message, json!({ "Resume": { "id": A, "token": "a b+c" } }));
    }
}
