//! The host's side: it seats clients, keeps the roster, starts the run and closes the room.

use crate::codec::{decode, encode, DecodeError, Message};
use crate::{Hello, Identity, Member, PeerKey, Player, Refusal, RoomCode, Seat, Settings, Start, MAX_LOCAL_PLAYERS, MAX_SEATS, PROTOCOL};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostConfig {
    pub identity: Identity,
    /// The host's own player cards, one per local player.
    pub cards: Vec<String>,
    /// Seats over every device, at most `MAX_SEATS`.
    pub seats: u8,
    /// Seat players who join a run in progress.
    pub drop_in: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostEvent {
    /// A client was seated in the lobby.
    Joined { peer: PeerKey, seats: Vec<Seat> },
    Refused { peer: PeerKey, reason: Refusal },
    /// A ready mark or a card changed.
    Changed,
    /// A client joined the run in progress: spawn these players.
    DropIn { peer: PeerKey, players: Vec<Player> },
    /// A client left; in a run, its players leave with it.
    Left { peer: PeerKey, seats: Vec<u8> },
    /// Another host answered in this room: pick a new code.
    CodeTaken { peer: PeerKey },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartError {
    NotReady(Vec<PeerKey>),
    AlreadyRunning,
    Closed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Phase {
    Lobby,
    Running(Start),
    Closed,
}

pub struct Host {
    code: RoomCode,
    config: HostConfig,
    /// The host first.
    members: Vec<Member>,
    phase: Phase,
    outgoing: Vec<(PeerKey, Vec<u8>)>,
}

impl Host {
    /// `me` is the host's own peer, as the transport assigned it.
    pub fn new(code: RoomCode, mut config: HostConfig, me: PeerKey) -> Host {
        config.seats = config.seats.clamp(1, MAX_SEATS);
        config.cards.truncate(MAX_LOCAL_PLAYERS.min(config.seats as usize));
        let seats = config.cards.iter().enumerate().map(|(i, card)| Seat { seat: i as u8, card: card.clone() }).collect();
        let host = Member { peer: me, seats, ready: false, host: true };
        Host { code, config, members: vec![host], phase: Phase::Lobby, outgoing: Vec::new() }
    }

    pub fn code(&self) -> RoomCode {
        self.code
    }

    /// Every device in the room, the host first.
    pub fn members(&self) -> &[Member] {
        &self.members
    }

    pub fn running(&self) -> Option<&Start> {
        match &self.phase {
            Phase::Running(start) => Some(start),
            _ => None,
        }
    }

    pub fn is_closed(&self) -> bool {
        self.phase == Phase::Closed
    }

    /// Messages to send on the reliable channel, oldest first.
    pub fn take_outgoing(&mut self) -> Vec<(PeerKey, Vec<u8>)> {
        std::mem::take(&mut self.outgoing)
    }

    pub fn set_ready(&mut self, ready: bool) {
        if self.phase == Phase::Lobby && self.members[0].ready != ready {
            self.members[0].ready = ready;
            self.send_roster();
        }
    }

    pub fn set_cards(&mut self, cards: &[String]) {
        if self.phase == Phase::Lobby && set_cards(&mut self.members[0], cards) {
            self.send_roster();
        }
    }

    /// The devices that are not ready, the host included.
    pub fn not_ready(&self) -> Vec<PeerKey> {
        self.members.iter().filter(|m| !m.ready).map(|m| m.peer).collect()
    }

    /// Starts the run once everyone is ready, and tells every client.
    pub fn start(&mut self, settings: Settings) -> Result<Start, StartError> {
        match self.phase {
            Phase::Lobby => {}
            Phase::Running(_) => return Err(StartError::AlreadyRunning),
            Phase::Closed => return Err(StartError::Closed),
        }
        let not_ready = self.not_ready();
        if !not_ready.is_empty() {
            return Err(StartError::NotReady(not_ready));
        }
        let start = Start {
            seed: settings.seed,
            map: settings.map,
            difficulty: settings.difficulty,
            players: self.players(),
            unlocked: settings.unlocked,
            options: settings.options,
        };
        self.broadcast(&Message::Start(start.clone()), None);
        self.phase = Phase::Running(start.clone());
        Ok(start)
    }

    /// Ends the room: every client is told, and nothing more is sent.
    pub fn close(&mut self) {
        if self.phase != Phase::Closed {
            self.broadcast(&Message::Closed, None);
            self.phase = Phase::Closed;
        }
    }

    /// The transport lost `peer`.
    pub fn disconnected(&mut self, peer: PeerKey) -> Vec<HostEvent> {
        self.remove(peer)
    }

    /// A reliable-channel packet from `peer`. Anything that is not a lobby message is ignored.
    pub fn receive(&mut self, peer: PeerKey, bytes: &[u8]) -> Vec<HostEvent> {
        if self.phase == Phase::Closed || peer == self.members[0].peer {
            return Vec::new();
        }
        let message = match decode(bytes) {
            Ok(message) => message,
            Err(DecodeError::Protocol(yours)) => return self.refuse(peer, Refusal::Protocol { host: PROTOCOL, yours }),
            Err(DecodeError::NotLobby) => return Vec::new(),
            Err(error) if !self.is_member(peer) => return self.refuse(peer, Refusal::BadHello(format!("{error:?}"))),
            Err(_) => return Vec::new(),
        };
        match message {
            Message::Hello(hello) if !self.is_member(peer) => self.hello(peer, hello),
            Message::Ready(ready) if self.phase == Phase::Lobby => self.update(peer, |m| std::mem::replace(&mut m.ready, ready) != ready),
            Message::Cards(cards) if self.phase == Phase::Lobby => self.update(peer, |m| set_cards(m, &cards)),
            Message::Leave => self.remove(peer),
            Message::Welcome { .. } | Message::Roster(_) | Message::Start(_) | Message::Closed => vec![HostEvent::CodeTaken { peer }],
            _ => Vec::new(),
        }
    }

    fn is_member(&self, peer: PeerKey) -> bool {
        self.members.iter().any(|m| m.peer == peer)
    }

    fn hello(&mut self, peer: PeerKey, hello: Hello) -> Vec<HostEvent> {
        if let Some(refusal) = self.check(&hello) {
            return self.refuse(peer, refusal);
        }
        let taken: Vec<u8> = self.members.iter().flat_map(|m| m.seats.iter().map(|s| s.seat)).collect();
        let seats: Vec<Seat> = (0..self.config.seats)
            .filter(|s| !taken.contains(s))
            .zip(hello.cards)
            .map(|(seat, card)| Seat { seat, card })
            .collect();
        let member = Member { peer, seats: seats.clone(), ready: false, host: false };
        let seat_ids = seats.iter().map(|s| s.seat).collect();
        self.members.push(member.clone());
        let running = matches!(self.phase, Phase::Running(_));
        self.send(peer, &Message::Welcome { room: self.code.to_string(), seats: seat_ids, running });
        if let Phase::Running(start) = &mut self.phase {
            let players: Vec<Player> = seats.iter().map(|s| Player { seat: s.seat, peer, card: s.card.clone() }).collect();
            start.players.extend(players.iter().cloned());
            start.players.sort_by_key(|p| p.seat);
            let start = start.clone();
            self.send(peer, &Message::Start(start));
            self.broadcast(&Message::Joined(member), Some(peer));
            return vec![HostEvent::DropIn { peer, players }];
        }
        self.send_roster();
        vec![HostEvent::Joined { peer, seats }]
    }

    fn check(&self, hello: &Hello) -> Option<Refusal> {
        let (host, yours) = (&self.config.identity, &hello.identity);
        if hello.cards.is_empty() || hello.cards.len() > MAX_LOCAL_PLAYERS {
            return Some(Refusal::BadHello(format!("{} local players; a device seats 1 to {MAX_LOCAL_PLAYERS}", hello.cards.len())));
        }
        if host.game_version != yours.game_version {
            return Some(Refusal::Version { host: host.game_version.clone(), yours: yours.game_version.clone() });
        }
        if host.pak != yours.pak {
            return Some(Refusal::Pak);
        }
        let find = |mods: &[crate::ModPack], name: &str| mods.iter().find(|m| m.name == name).map(|m| m.fingerprint);
        let missing: Vec<String> = host.mods.iter().filter(|m| find(&yours.mods, &m.name).is_none()).map(|m| m.name.clone()).collect();
        let extra: Vec<String> = yours.mods.iter().filter(|m| find(&host.mods, &m.name).is_none()).map(|m| m.name.clone()).collect();
        let different: Vec<String> =
            host.mods.iter().filter(|m| find(&yours.mods, &m.name).is_some_and(|f| f != m.fingerprint)).map(|m| m.name.clone()).collect();
        if !(missing.is_empty() && extra.is_empty() && different.is_empty()) {
            return Some(Refusal::Mods { missing, extra, different });
        }
        if matches!(self.phase, Phase::Running(_)) && !self.config.drop_in {
            return Some(Refusal::Running);
        }
        let used: usize = self.members.iter().map(|m| m.seats.len()).sum();
        let free = self.config.seats as usize - used;
        if hello.cards.len() > free {
            return Some(Refusal::Full { free: free as u8, asked: hello.cards.len() as u8 });
        }
        None
    }

    fn refuse(&mut self, peer: PeerKey, reason: Refusal) -> Vec<HostEvent> {
        self.send(peer, &Message::Refuse(reason.clone()));
        vec![HostEvent::Refused { peer, reason }]
    }

    fn update(&mut self, peer: PeerKey, change: impl FnOnce(&mut Member) -> bool) -> Vec<HostEvent> {
        let Some(member) = self.members.iter_mut().find(|m| m.peer == peer) else { return Vec::new() };
        if !change(member) {
            return Vec::new();
        }
        self.send_roster();
        vec![HostEvent::Changed]
    }

    fn remove(&mut self, peer: PeerKey) -> Vec<HostEvent> {
        let Some(index) = self.members.iter().position(|m| m.peer == peer).filter(|&i| i > 0) else { return Vec::new() };
        let member = self.members.remove(index);
        let seats: Vec<u8> = member.seats.iter().map(|s| s.seat).collect();
        match &mut self.phase {
            Phase::Lobby => self.send_roster(),
            Phase::Running(start) => {
                start.players.retain(|p| p.peer != peer);
                self.broadcast(&Message::Left { peer, seats: seats.clone() }, None);
            }
            Phase::Closed => {}
        }
        vec![HostEvent::Left { peer, seats }]
    }

    fn players(&self) -> Vec<Player> {
        let mut players: Vec<Player> =
            self.members.iter().flat_map(|m| m.seats.iter().map(|s| Player { seat: s.seat, peer: m.peer, card: s.card.clone() })).collect();
        players.sort_by_key(|p| p.seat);
        players
    }

    fn send_roster(&mut self) {
        self.broadcast(&Message::Roster(self.members.clone()), None);
    }

    fn send(&mut self, peer: PeerKey, message: &Message) {
        self.outgoing.push((peer, encode(message)));
    }

    fn broadcast(&mut self, message: &Message, except: Option<PeerKey>) {
        let bytes = encode(message);
        for member in &self.members[1..] {
            if Some(member.peer) != except {
                self.outgoing.push((member.peer, bytes.clone()));
            }
        }
    }
}

/// Puts new cards on a member's seats, one per seat; false when nothing changed.
fn set_cards(member: &mut Member, cards: &[String]) -> bool {
    let mut changed = false;
    for (seat, card) in member.seats.iter_mut().zip(cards) {
        if seat.card != *card {
            seat.card = card.clone();
            changed = true;
        }
    }
    changed
}
