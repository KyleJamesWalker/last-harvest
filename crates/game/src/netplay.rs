//! Online co-op inside the game: the lobby, then the night kept in step over the platform's transport. The platform
//! joins the room `take_request` names, hands in what arrives and sends what `take_outgoing` returns; nothing here
//! touches a socket.

use std::collections::BTreeMap;

use last_harvest_lobby::{Client, ClientEvent, ClientState, EndReason, Hello, Host, HostConfig, HostEvent, Identity, Member, RoomCode, Start};

use crate::harvest::{Flow, Harvest, online::Extract};
use crate::netsync::input::{self, InputFrame, InputLog, InputMsg, InputQueue};
use crate::netsync::snapshot::SEND_EVERY;
use crate::netsync::{ClientLink, Event, HostLink, Mirror};
use crate::{Buttons, Input, Sfx};

pub const RELIABLE: usize = 0;
pub const UNRELIABLE: usize = 1;
/// Seconds a client waits for the host to answer before giving up.
const ANSWER_WAIT: f32 = 15.0;
/// Ticks the host keeps its connections open after closing the room, so the word reaches every client.
const CLOSE_TICKS: u32 = 30;
/// Unreliable packets held for the next step; older ones give way.
const UNRELIABLE_MOST: usize = 256;
/// Seconds of silence after which the host takes a client for gone.
const PEER_GONE: f32 = 10.0;

/// What the game asks of the platform's transport.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Join the signaling room at `room`, with the TURN servers the relay hands out at `ice`.
    Connect { room: String, ice: String },
    Close,
}

/// Bytes to or from a peer, on `RELIABLE` or `UNRELIABLE`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Packet {
    pub channel: usize,
    pub peer: u128,
    pub bytes: Vec<u8>,
}

/// The relay an empty setting means.
pub const DEFAULT_RELAY: &str = "wss://lh-relay.vps.pocketsquirrel.com";

pub const INSECURE_RELAY: &str = "This page is https, so it can only reach a secure (wss://) relay. For a LAN relay, play the desktop game or the web build served over http.";

/// The relay as a player types it, `host:port` or with a ws, wss, http or https scheme: its signaling base and its
/// ICE address. Empty means `DEFAULT_RELAY`. Without a scheme, a LAN or loopback address is `ws` and a domain name
/// `wss`. A page served over https (`secure_page`) can reach only `wss`, or `ws` on this machine.
pub fn relay_urls(relay: &str, secure_page: bool) -> Result<(String, String), String> {
    let r = relay.trim().trim_end_matches('/');
    let r = if r.is_empty() { DEFAULT_RELAY } else { r };
    let scheme = ["wss://", "https://", "ws://", "http://"].into_iter().find(|s| r.get(..s.len()).is_some_and(|p| p.eq_ignore_ascii_case(s)));
    let rest = &r[scheme.map_or(0, str::len)..];
    let host = relay_host(rest);
    let secure = match scheme {
        Some(s) => s.starts_with("wss") || s.starts_with("https"),
        None => !is_lan_host(host),
    };
    if secure_page && !secure && !is_loopback_host(host) {
        return Err(INSECURE_RELAY.into());
    }
    let (ws, http) = if secure { ("wss", "https") } else { ("ws", "http") };
    Ok((format!("{ws}://{rest}"), format!("{http}://{rest}/ice")))
}

/// The host in `host[:port][/path]`, without an IPv6 address's brackets.
fn relay_host(rest: &str) -> &str {
    let authority = rest.split('/').next().unwrap_or(rest);
    match authority.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(v6),
        None => authority.rsplit_once(':').map_or(authority, |(h, _)| h),
    }
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost") || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// An address that has no TLS certificate: an IP, `localhost`, a `.local` name or a name with no dot.
fn is_lan_host(host: &str) -> bool {
    host.parse::<std::net::IpAddr>().is_ok() || !host.contains('.') || host.to_ascii_lowercase().ends_with(".local")
}

/// Something the game acts on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Happening {
    Start(Start),
    /// A seat taken mid-run, and its card.
    Seat(usize, String),
    Unseat(Vec<usize>),
    End(String),
}

/// One client as the host sees it.
#[derive(Default)]
struct Remote {
    link: HostLink,
    queues: BTreeMap<usize, InputQueue>,
    /// Seconds since anything came from it.
    quiet: f32,
}

pub(crate) struct Hosting {
    pub lobby: Option<Host>,
    config: HostConfig,
    peers: BTreeMap<u128, Remote>,
    extract: Extract,
    tick: u32,
    played: Vec<u32>,
}

pub(crate) struct Joining {
    pub lobby: Client,
    link: ClientLink,
    mirror: Mirror,
    logs: Vec<InputLog>,
    tick: u32,
    waited: f32,
    /// Seconds since anything came from the host during the night.
    quiet: f32,
}

pub(crate) enum Side {
    Host(Box<Hosting>),
    Client(Box<Joining>),
}

/// Bytes each way since online play began.
#[derive(Clone, Copy, Debug, Default)]
pub struct Traffic {
    pub down: u64,
    pub up: u64,
    pub secs: f32,
}

pub(crate) struct Net {
    pub code: RoomCode,
    pub side: Side,
    me: Option<u128>,
    unreliable: Vec<Packet>,
    outbox: Vec<Packet>,
    request: Option<Request>,
    happenings: Vec<Happening>,
    closing: Option<u32>,
    pub running: bool,
    pub traffic: Traffic,
}

impl Net {
    fn new(code: RoomCode, side: Side, (base, ice): (String, String)) -> Self {
        let request = Some(Request::Connect { room: code.room_url(&base), ice });
        Net { code, side, me: None, unreliable: Vec::new(), outbox: Vec::new(), request, happenings: Vec::new(), closing: None, running: false, traffic: Traffic::default() }
    }

    /// Opens room `code` for players to join, `cards` being this device's players.
    pub fn host(code: RoomCode, identity: Identity, cards: Vec<String>, relay: (String, String)) -> Self {
        let config = HostConfig { identity, cards, seats: last_harvest_lobby::MAX_SEATS, drop_in: true };
        let hosting = Hosting { lobby: None, config, peers: BTreeMap::new(), extract: Extract::default(), tick: 0, played: Vec::new() };
        Self::new(code, Side::Host(Box::new(hosting)), relay)
    }

    /// Joins room `code` with this device's players' `cards`.
    pub fn join(code: RoomCode, identity: Identity, cards: Vec<String>, relay: (String, String)) -> Self {
        let joining = Joining { lobby: Client::new(Hello { identity, cards }), link: ClientLink::default(), mirror: Mirror::default(), logs: Vec::new(), tick: 0, waited: 0.0, quiet: 0.0 };
        Self::new(code, Side::Client(Box::new(joining)), relay)
    }

    pub fn is_host(&self) -> bool {
        matches!(self.side, Side::Host(_))
    }

    pub fn take_request(&mut self) -> Option<Request> {
        self.request.take()
    }

    pub fn take_outgoing(&mut self) -> Vec<Packet> {
        let out = std::mem::take(&mut self.outbox);
        self.traffic.up += out.iter().map(|p| p.bytes.len() as u64).sum::<u64>();
        out
    }

    pub fn take_happenings(&mut self) -> Vec<Happening> {
        std::mem::take(&mut self.happenings)
    }

    /// The relay gave this device its peer id.
    pub fn joined(&mut self, me: u128) {
        self.me = Some(me);
        if let Side::Host(h) = &mut self.side
            && h.lobby.is_none()
        {
            h.lobby = Some(Host::new(self.code, h.config.clone(), me));
        }
    }

    /// The transport could not reach the relay, or lost it before the room filled.
    pub fn failed(&mut self, why: &str) {
        if self.me.is_none() {
            self.end(format!("Couldn't reach the relay: {why}"));
        }
    }

    pub fn peer(&mut self, peer: u128, up: bool) {
        let mut happened = Vec::new();
        match &mut self.side {
            Side::Host(h) => {
                if !up {
                    let events = h.lobby.as_mut().map(|l| l.disconnected(peer)).unwrap_or_default();
                    happened.extend(host_events(h, events));
                }
            }
            Side::Client(c) if up => c.lobby.connected(peer),
            Side::Client(c) => happened.extend(client_events(c.lobby.disconnected(peer))),
        }
        self.after(happened);
    }

    pub fn receive(&mut self, packet: Packet) {
        self.traffic.down += packet.bytes.len() as u64;
        if packet.channel == UNRELIABLE {
            if self.running {
                if self.unreliable.len() >= UNRELIABLE_MOST {
                    self.unreliable.remove(0);
                }
                self.unreliable.push(packet);
            }
            return;
        }
        let happened = match &mut self.side {
            Side::Host(h) => {
                let events = h.lobby.as_mut().map(|l| l.receive(packet.peer, &packet.bytes)).unwrap_or_default();
                host_events(h, events)
            }
            Side::Client(c) => client_events(c.lobby.receive(packet.peer, &packet.bytes)),
        };
        self.after(happened);
    }

    fn after(&mut self, happened: Vec<Happening>) {
        for h in happened {
            match h {
                Happening::Start(_) => self.running = true,
                Happening::End(_) => self.request = Some(Request::Close),
                _ => {}
            }
            self.happenings.push(h);
        }
        self.flush();
    }

    /// Sends what the lobby has queued.
    fn flush(&mut self) {
        let out = match &mut self.side {
            Side::Host(h) => h.lobby.as_mut().map(Host::take_outgoing).unwrap_or_default(),
            Side::Client(c) => c.lobby.take_outgoing(),
        };
        self.outbox.extend(out.into_iter().map(|(peer, bytes)| Packet { channel: RELIABLE, peer, bytes }));
    }

    fn end(&mut self, why: String) {
        if !self.happenings.iter().any(|h| matches!(h, Happening::End(_))) {
            self.happenings.push(Happening::End(why));
        }
        self.request = Some(Request::Close);
        self.running = false;
    }

    /// A client gives up on a room nobody answers in, and a closing host lets go once its last word is out.
    pub fn idle(&mut self, dt: f32) {
        self.traffic.secs += dt;
        if let Some(left) = &mut self.closing {
            *left = left.saturating_sub(1);
            if *left == 0 {
                self.closing = None;
                self.request = Some(Request::Close);
            }
        }
        if let Side::Client(c) = &mut self.side
            && *c.lobby.state() == ClientState::Joining
            && self.me.is_some()
        {
            c.waited += dt;
            if c.waited > ANSWER_WAIT {
                let code = self.code;
                self.end(format!("No host answered in room {}.", code.as_str()));
            }
        }
    }

    pub fn set_ready(&mut self, ready: bool) {
        match &mut self.side {
            Side::Host(h) => {
                if let Some(l) = h.lobby.as_mut() {
                    l.set_ready(ready);
                }
            }
            Side::Client(c) => c.lobby.set_ready(ready),
        }
        self.flush();
    }

    /// The lobby list, the host first, and whether this device is ready.
    pub fn roster(&self) -> (Vec<Member>, bool) {
        let (list, me) = match &self.side {
            Side::Host(h) => (h.lobby.as_ref().map(|l| l.members().to_vec()).unwrap_or_default(), self.me),
            Side::Client(c) => (c.lobby.roster().to_vec(), self.me),
        };
        let ready = list.iter().any(|m| Some(m.peer) == me && m.ready);
        (list, ready)
    }

    /// Words for the lobby page while it waits on the transport or the host.
    pub fn status(&self) -> Option<&'static str> {
        match &self.side {
            _ if self.me.is_none() => Some("Reaching the relay..."),
            Side::Client(c) if *c.lobby.state() == ClientState::Joining => Some("Looking for the host..."),
            _ => None,
        }
    }

    /// The host starts the night once everyone is ready: the start every device builds its run from.
    pub fn start(&mut self, settings: last_harvest_lobby::Settings) -> Result<Start, String> {
        let Side::Host(h) = &mut self.side else { return Err("Only the host starts the night.".into()) };
        let Some(lobby) = h.lobby.as_mut() else { return Err("Still reaching the relay.".into()) };
        let start = lobby.start(settings).map_err(|e| match e {
            last_harvest_lobby::StartError::NotReady(_) => "Waiting for everyone to be ready.".to_string(),
            last_harvest_lobby::StartError::AlreadyRunning => "The night is already on.".to_string(),
            last_harvest_lobby::StartError::Closed => "The room is closed.".to_string(),
        })?;
        for p in &start.players {
            if Some(p.peer) != self.me {
                h.peers.entry(p.peer).or_default().queues.insert(p.seat as usize, InputQueue::default());
            }
        }
        self.running = true;
        self.flush();
        Ok(start)
    }

    /// This device's seats.
    pub fn my_seats(&self, start: &Start) -> Vec<usize> {
        start.players.iter().filter(|p| Some(p.peer) == self.me).map(|p| p.seat as usize).collect()
    }

    /// Leaves online play: the host closes the room, a client leaves it.
    pub fn leave(&mut self) {
        match &mut self.side {
            Side::Host(h) => {
                if let Some(l) = h.lobby.as_mut() {
                    l.close();
                }
            }
            Side::Client(c) => drop(c.lobby.leave()),
        }
        self.flush();
        self.running = false;
        self.unreliable.clear();
        self.closing = Some(CLOSE_TICKS);
    }

    /// Whether this device is already letting go of the room.
    pub fn leaving(&self) -> bool {
        self.closing.is_some() || !self.running && matches!(self.request, Some(Request::Close))
    }

    /// One tick of an online run: the host plays everyone's input and sends snapshots, a client draws them.
    pub fn step(&mut self, h: &mut Harvest, input: &Input, others: &[Input], dt: f32, sounds: &mut Vec<Sfx>) -> Flow {
        self.step_as(h, input, others, dt, sounds, false)
    }

    /// A step while this device is away, its lid shut: a host holds the night under its own pause, a client keeps
    /// acking but sends no input, so the host stops waiting on it.
    pub fn step_away(&mut self, h: &mut Harvest, dt: f32, sounds: &mut Vec<Sfx>) {
        if self.is_host() {
            h.hold_away();
        }
        self.step_as(h, &Input::default(), &[], dt, sounds, true);
    }

    fn step_as(&mut self, h: &mut Harvest, input: &Input, others: &[Input], dt: f32, sounds: &mut Vec<Sfx>, away: bool) -> Flow {
        self.traffic.secs += dt;
        match &mut self.side {
            Side::Host(s) => {
                let (flow, gone) = host_step(s, &mut self.unreliable, &mut self.outbox, h, input, dt, sounds);
                let events: Vec<HostEvent> = gone.into_iter().flat_map(|peer| s.lobby.as_mut().map(|l| l.disconnected(peer)).unwrap_or_default()).collect();
                let happened = host_events(s, events);
                self.after(happened);
                flow
            }
            Side::Client(c) => {
                let flow = client_step(c, &mut self.unreliable, &mut self.outbox, h, input, others, dt, sounds, away);
                if c.quiet > PEER_GONE {
                    self.end("Lost the connection to the host.".into());
                }
                flow
            }
        }
    }

    /// Keeps clients drawing while the host's run is held under Settings.
    pub fn hold(&mut self, h: &Harvest) {
        if let Side::Host(s) = &mut self.side {
            s.tick += 1;
            if s.tick.is_multiple_of(SEND_EVERY) {
                send_world(s, &mut self.outbox, h);
            }
        }
    }
}

fn host_events(h: &mut Hosting, events: Vec<HostEvent>) -> Vec<Happening> {
    let mut out = Vec::new();
    for e in events {
        match e {
            HostEvent::DropIn { peer, players } => {
                let r = h.peers.entry(peer).or_default();
                for p in players {
                    r.queues.insert(p.seat as usize, InputQueue::default());
                    out.push(Happening::Seat(p.seat as usize, p.card));
                }
            }
            HostEvent::Left { peer, seats } => {
                h.peers.remove(&peer);
                if h.lobby.as_ref().is_some_and(|l| l.running().is_some()) {
                    out.push(Happening::Unseat(seats.into_iter().map(usize::from).collect()));
                }
            }
            HostEvent::Joined { .. } | HostEvent::Refused { .. } | HostEvent::Changed | HostEvent::CodeTaken { .. } => {}
        }
    }
    out
}

fn client_events(events: Vec<ClientEvent>) -> Vec<Happening> {
    let mut out = Vec::new();
    for e in events {
        match e {
            ClientEvent::Started(start) => out.push(Happening::Start(start)),
            ClientEvent::Joined(m) => out.extend(m.seats.into_iter().map(|s| Happening::Seat(s.seat as usize, s.card))),
            ClientEvent::Left { seats, .. } => out.push(Happening::Unseat(seats.into_iter().map(usize::from).collect())),
            ClientEvent::Refused(r) => out.push(Happening::End(r.to_string())),
            ClientEvent::Ended(EndReason::HostLeft) => out.push(Happening::End("The host ended the night.".into())),
            ClientEvent::Ended(EndReason::HostLost) => out.push(Happening::End("Lost the connection to the host.".into())),
            ClientEvent::Ended(EndReason::Left) => out.push(Happening::End(String::new())),
            ClientEvent::Welcomed { .. } | ClientEvent::Roster => {}
        }
    }
    out
}

fn send_world(s: &mut Hosting, outbox: &mut Vec<Packet>, h: &Harvest) {
    let world = s.extract.world(h, s.tick, &s.played);
    for (&peer, r) in &mut s.peers {
        for bytes in r.link.snapshot(world.clone()) {
            outbox.push(Packet { channel: UNRELIABLE, peer, bytes });
        }
    }
}

/// One host tick, and the clients that have gone quiet for good.
fn host_step(s: &mut Hosting, unreliable: &mut Vec<Packet>, outbox: &mut Vec<Packet>, h: &mut Harvest, input: &Input, dt: f32, sounds: &mut Vec<Sfx>) -> (Flow, Vec<u128>) {
    for r in s.peers.values_mut() {
        r.quiet += dt;
    }
    for p in unreliable.drain(..) {
        let (Some(r), Some(m)) = (s.peers.get_mut(&p.peer), InputMsg::decode(&p.bytes)) else { continue };
        r.quiet = 0.0;
        r.link.ack(m.ack_snapshot, m.ack_event);
        for seat in &m.seats {
            if let Some(q) = r.queues.get_mut(&(seat.seat as usize)) {
                q.receive(seat);
            }
        }
    }
    s.played.resize(s.played.len().max(h.player_count()), 0);
    for r in s.peers.values_mut() {
        for (&seat, q) in &mut r.queues {
            let (inp, at) = q.play();
            h.steer(seat, inp);
            h.set_away(seat, q.quiet() > input::AWAY_TICKS);
            if let Some(slot) = s.played.get_mut(seat) {
                *slot = at.unwrap_or(0);
            }
        }
    }
    let before = sounds.len();
    let flow = h.update(input, dt, sounds);
    s.tick += 1;
    let fresh = s.extract.record(h, s.tick, &sounds[before..]);
    for r in s.peers.values_mut() {
        fresh.hand(&mut r.link);
    }
    if s.tick.is_multiple_of(SEND_EVERY) {
        send_world(s, outbox, h);
    }
    let gone = s.peers.iter().filter(|(_, r)| r.quiet > PEER_GONE).map(|(&p, _)| p).collect();
    (flow, gone)
}

#[allow(clippy::too_many_arguments)]
fn client_step(c: &mut Joining, unreliable: &mut Vec<Packet>, outbox: &mut Vec<Packet>, h: &mut Harvest, input: &Input, others: &[Input], dt: f32, sounds: &mut Vec<Sfx>, away: bool) -> Flow {
    let host = c.lobby.host();
    c.quiet += dt;
    for p in unreliable.drain(..).filter(|p| Some(p.peer) == host) {
        c.quiet = 0.0;
        if let Some(part) = c.link.receive(&p.bytes) {
            c.mirror.apply(&part);
            let fresh = c.link.fresh(&part.spawns);
            h.client_spawned(&fresh);
            c.mirror.spawn(fresh);
        }
    }
    for ev in c.link.events() {
        match ev {
            Event::Stop { id } => c.mirror.stop(id),
            ev => h.client_event(ev, sounds),
        }
    }
    c.tick += 1;
    let seats = h.local_seats();
    c.logs.resize_with(seats.len(), InputLog::default);
    let mine: Vec<Input> = std::iter::once(*input).chain(others.iter().copied()).take(seats.len()).collect();
    c.mirror.frozen = h.held();
    let turn = crate::tuning::tuning().turn_speed;
    for (v, inp) in mine.iter().enumerate() {
        c.mirror.play_locally(seats[v]);
        c.mirror.predict(seats[v], c.tick, inp, turn);
        if !away {
            c.logs[v].push(c.tick, InputFrame::of(inp));
        }
    }
    if let Some(host) = host
        && c.tick.is_multiple_of(input::SEND_EVERY)
    {
        let (s, e) = c.link.ack();
        let seats = if away { Vec::new() } else { c.logs.iter().zip(&seats).map(|(l, &seat)| l.seat(seat as u8)).collect() };
        let msg = InputMsg { ack_snapshot: s, ack_event: e, seats };
        outbox.push(Packet { channel: UNRELIABLE, peer: host, bytes: msg.encode() });
    }
    c.mirror.advance(dt);
    h.client_frame(&c.mirror, dt, sounds);
    let mut flow = Flow::default();
    let pressed = mine.iter().fold(Buttons::NONE, |b, i| b | i.pressed);
    if h.client_wants_out(pressed) {
        flow.quit = true;
    }
    flow
}
