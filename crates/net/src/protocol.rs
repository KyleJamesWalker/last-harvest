//! The matchbox signaling protocol: server events, relayed WebRTC handshake messages, and a relay's resume URL.

use crate::PeerId;
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

/// `url` asking the relay to give `id` its place back.
pub(crate) fn resume_url(url: &str, id: PeerId, token: &str) -> String {
    let join = if url.contains('?') { '&' } else { '?' };
    let mut escaped = String::new();
    for b in token.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            escaped.push(b as char);
        } else {
            escaped.push_str(&format!("%{b:02X}"));
        }
    }
    format!("{url}{join}resume={}&token={escaped}", uuid_string(id))
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
    fn a_resume_url_carries_the_id_and_an_escaped_token() {
        let a = parse_uuid(A).unwrap();
        assert_eq!(resume_url("ws://h/room", a, "a b+c"), format!("ws://h/room?resume={A}&token=a%20b%2Bc"));
        assert_eq!(resume_url("ws://h/room?x=1", a, "t"), format!("ws://h/room?x=1&resume={A}&token=t"));
    }
}
