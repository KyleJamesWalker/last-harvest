//! A client's side: it says hello to every peer in the room, and the one that answers is the host.

use crate::codec::{decode, encode, DecodeError, Message};
use crate::{Hello, Member, PeerKey, Refusal, Start, PROTOCOL};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EndReason {
    /// The host closed the room.
    HostLeft,
    /// The transport lost the host.
    HostLost,
    /// This client left.
    Left,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientState {
    Joining,
    Lobby,
    Running,
    Refused(Refusal),
    Ended(EndReason),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientEvent {
    Welcomed { seats: Vec<u8> },
    Refused(Refusal),
    /// The lobby list or its ready marks changed.
    Roster,
    /// The run starts, or this client joined one in progress.
    Started(Start),
    /// Another device joined the run in progress: spawn its players.
    Joined(Member),
    /// A device left the run: its players leave with it.
    Left { peer: PeerKey, seats: Vec<u8> },
    Ended(EndReason),
}

pub struct Client {
    hello: Hello,
    state: ClientState,
    host: Option<PeerKey>,
    seats: Vec<u8>,
    roster: Vec<Member>,
    start: Option<Start>,
    greeted: Vec<PeerKey>,
    outgoing: Vec<(PeerKey, Vec<u8>)>,
}

impl Client {
    pub fn new(hello: Hello) -> Client {
        Client {
            hello,
            state: ClientState::Joining,
            host: None,
            seats: Vec::new(),
            roster: Vec::new(),
            start: None,
            greeted: Vec::new(),
            outgoing: Vec::new(),
        }
    }

    pub fn state(&self) -> &ClientState {
        &self.state
    }

    pub fn host(&self) -> Option<PeerKey> {
        self.host
    }

    /// This device's seats, one per local player.
    pub fn seats(&self) -> &[u8] {
        &self.seats
    }

    /// The lobby list, the host first.
    pub fn roster(&self) -> &[Member] {
        &self.roster
    }

    pub fn start(&self) -> Option<&Start> {
        self.start.as_ref()
    }

    /// Messages to send on the reliable channel, oldest first.
    pub fn take_outgoing(&mut self) -> Vec<(PeerKey, Vec<u8>)> {
        std::mem::take(&mut self.outgoing)
    }

    /// The transport opened a connection to `peer`, which may be the host.
    pub fn connected(&mut self, peer: PeerKey) {
        if self.state == ClientState::Joining && !self.greeted.contains(&peer) {
            self.greeted.push(peer);
            self.outgoing.push((peer, encode(&Message::Hello(self.hello.clone()))));
        }
    }

    /// The transport lost `peer`.
    pub fn disconnected(&mut self, peer: PeerKey) -> Vec<ClientEvent> {
        self.greeted.retain(|&p| p != peer);
        if self.host == Some(peer) && self.is_seated() {
            return self.end(EndReason::HostLost);
        }
        Vec::new()
    }

    pub fn set_ready(&mut self, ready: bool) {
        if self.state == ClientState::Lobby {
            self.send_host(&Message::Ready(ready));
        }
    }

    pub fn set_cards(&mut self, cards: Vec<String>) {
        if self.state == ClientState::Lobby {
            self.hello.cards = cards.clone();
            self.send_host(&Message::Cards(cards));
        }
    }

    pub fn leave(&mut self) -> Vec<ClientEvent> {
        if !self.is_seated() {
            return Vec::new();
        }
        self.send_host(&Message::Leave);
        self.end(EndReason::Left)
    }

    /// A reliable-channel packet from `peer`. Anything that is not a lobby message from the host is ignored.
    pub fn receive(&mut self, peer: PeerKey, bytes: &[u8]) -> Vec<ClientEvent> {
        if self.host.is_some_and(|host| host != peer) {
            return Vec::new();
        }
        let message = match decode(bytes) {
            Ok(message) => message,
            Err(DecodeError::Protocol(host)) if self.state == ClientState::Joining => {
                Message::Refuse(Refusal::Protocol { host, yours: PROTOCOL })
            }
            Err(_) => return Vec::new(),
        };
        match (&self.state, message) {
            (ClientState::Joining, Message::Welcome { seats, running, .. }) => {
                self.host = Some(peer);
                self.seats = seats.clone();
                if !running {
                    self.state = ClientState::Lobby;
                }
                vec![ClientEvent::Welcomed { seats }]
            }
            (ClientState::Joining, Message::Refuse(reason)) => {
                self.state = ClientState::Refused(reason.clone());
                vec![ClientEvent::Refused(reason)]
            }
            (ClientState::Lobby, Message::Roster(members)) => {
                self.roster = members;
                vec![ClientEvent::Roster]
            }
            (ClientState::Lobby | ClientState::Joining, Message::Start(start)) if self.host.is_some() => {
                self.state = ClientState::Running;
                self.start = Some(start.clone());
                vec![ClientEvent::Started(start)]
            }
            (ClientState::Running, Message::Joined(member)) => {
                if let Some(start) = &mut self.start {
                    start.players.extend(member.seats.iter().map(|s| crate::Player { seat: s.seat, peer: member.peer, card: s.card.clone() }));
                    start.players.sort_by_key(|p| p.seat);
                }
                vec![ClientEvent::Joined(member)]
            }
            (ClientState::Running, Message::Left { peer, seats }) => {
                if let Some(start) = &mut self.start {
                    start.players.retain(|p| p.peer != peer);
                }
                vec![ClientEvent::Left { peer, seats }]
            }
            (ClientState::Lobby | ClientState::Running, Message::Closed) => self.end(EndReason::HostLeft),
            _ => Vec::new(),
        }
    }

    fn is_seated(&self) -> bool {
        self.host.is_some() && matches!(self.state, ClientState::Joining | ClientState::Lobby | ClientState::Running)
    }

    fn end(&mut self, reason: EndReason) -> Vec<ClientEvent> {
        self.state = ClientState::Ended(reason.clone());
        vec![ClientEvent::Ended(reason)]
    }

    fn send_host(&mut self, message: &Message) {
        if let Some(host) = self.host {
            self.outgoing.push((host, encode(message)));
        }
    }
}
