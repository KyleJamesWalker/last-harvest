//! Native peers: a matchbox-protocol signaling client and webrtc-rs connections, run on one tokio thread.

use crate::inbox::Inbox;
use crate::protocol::{self, Event, Signal};
use crate::{IceConfig, PeerId, PeerState, RELIABLE, UNRELIABLE};
use bytes::BytesMut;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;
use webrtc::data_channel::{DataChannel, DataChannelEvent, RTCDataChannelInit};
use webrtc::error::Error;
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfiguration, RTCConfigurationBuilder,
    RTCIceCandidateInit, RTCIceConnectionState, RTCIceServer, RTCIceTransportPolicy, RTCPeerConnectionIceEvent,
    RTCPeerConnectionState, RTCSessionDescription, SettingEngine, SettingEngineBuilder,
};

/// How often an open signaling connection sends `KeepAlive`, which a relay that resumes peers echoes.
const KEEP_ALIVE: Duration = Duration::from_secs(2);
/// A resumable signaling connection silent this long is lost, though TCP may not notice for minutes.
const SIGNALING_SILENCE: Duration = Duration::from_secs(6);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// How long a lost signaling connection tries to resume. The relay holds the place for 30 s.
const RESUME_WINDOW: Duration = Duration::from_secs(25);
/// ICE reports a silent connection disconnected after this, and failed after `ICE_FAILED`.
const ICE_DISCONNECTED: Duration = Duration::from_secs(3);
const ICE_FAILED: Duration = Duration::from_secs(30);
/// How often the offering side checks a connection, and repeats an ICE restart while it stays down.
const RESTART_CHECK: Duration = Duration::from_secs(1);
const RESTART_EVERY: Duration = Duration::from_secs(4);

/// Test hooks: `LAST_HARVEST_NET_TEST_RESTART_MS` makes the offering side restart ICE that long after connecting, and
/// `LAST_HARVEST_NET_TEST_DROP_SIGNAL_MS` drops the signaling connection, without a close, that long after it opened. Each
/// fires once.
fn test_delay(name: &str) -> Option<Duration> {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).map(Duration::from_millis)
}

/// Test conditions for the unreliable channel: `LAST_HARVEST_NET_LAG_MS` delays each packet, `LAST_HARVEST_NET_LOSS` drops that
/// percentage of them.
struct Impairment {
    lag: Duration,
    loss_percent: u32,
    seed: std::cell::Cell<u64>,
}

impl Impairment {
    fn from_env() -> Impairment {
        let var = |name: &str| std::env::var(name).ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
        Impairment { lag: Duration::from_millis(var("LAST_HARVEST_NET_LAG_MS")), loss_percent: var("LAST_HARVEST_NET_LOSS") as u32, seed: std::cell::Cell::new(0x9e37_79b9_7f4a_7c15) }
    }

    fn drop_next(&self) -> bool {
        if self.loss_percent == 0 {
            return false;
        }
        let mut x = self.seed.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.seed.set(x);
        (x % 100) < self.loss_percent as u64
    }
}

/// What the tokio thread hands the game thread.
#[derive(Default)]
struct Shared {
    id: Option<PeerId>,
    /// Peer state changes `update_peers` has not taken yet.
    changes: Vec<(PeerId, PeerState)>,
    inbox: Inbox,
    signaling_closed: bool,
}

type SharedRef = Arc<Mutex<Shared>>;

/// Locks `m` even if a panicking callback poisoned it; every holder leaves the data consistent.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What the game thread asks of the tokio thread.
enum Command {
    Send(usize, PeerId, Vec<u8>),
    Close,
}

/// The game thread's side of a room connection; its tokio thread runs until `close` or drop.
pub struct Socket {
    shared: SharedRef,
    commands: UnboundedSender<Command>,
}

impl Socket {
    pub fn connect(room_url: &str, ice: &IceConfig) -> Socket {
        let shared = SharedRef::default();
        let (commands, rx) = unbounded_channel();
        let url = room_url.to_string();
        let ice = ice.clone();
        let task_shared = shared.clone();
        std::thread::Builder::new()
            .name("last-harvest-net".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("tokio runtime");
                runtime.block_on(run(url, ice, task_shared.clone(), rx));
                lock(&task_shared).signaling_closed = true;
            })
            .expect("spawn the peer socket thread");
        Socket { shared, commands }
    }

    pub fn id(&mut self) -> Option<PeerId> {
        lock(&self.shared).id
    }

    pub fn update_peers(&mut self) -> Vec<(PeerId, PeerState)> {
        std::mem::take(&mut lock(&self.shared).changes)
    }

    pub fn signaling_closed(&self) -> bool {
        lock(&self.shared).signaling_closed
    }

    pub fn send(&mut self, channel: usize, peer: PeerId, data: &[u8]) {
        let _ = self
            .commands
            .send(Command::Send(channel, peer, data.to_vec()));
    }

    pub fn receive(&mut self, channel: usize) -> Vec<(PeerId, Vec<u8>)> {
        lock(&self.shared).inbox.take(channel)
    }

    pub fn close(&mut self) {
        let _ = self.commands.send(Command::Close);
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        self.close();
    }
}

/// A peer to drop: any connection to it, or only the connection with this serial, which a late callback from an
/// earlier connection to the same peer must not close.
type Gone = (PeerId, Option<u64>);

/// Numbers every connection made.
static NEXT_SERIAL: AtomicU64 = AtomicU64::new(0);

struct Peer {
    serial: u64,
    connection: Arc<dyn PeerConnection>,
    channels: Vec<Arc<dyn DataChannel>>,
    signals: UnboundedSender<Signal>,
    /// Both data channels are open.
    connected: Arc<AtomicBool>,
}

/// What a peer's callbacks need: where packets and state changes go, and how to reach the signaling server.
#[derive(Clone)]
struct Context {
    /// The game's own ICE servers.
    base: IceConfig,
    /// What new connections use: `base` and the relay's latest servers.
    ice: Arc<Mutex<RTCConfiguration>>,
    shared: SharedRef,
    outgoing: UnboundedSender<String>,
    gone: UnboundedSender<Gone>,
}

impl Context {
    fn new(base: IceConfig, shared: SharedRef, outgoing: UnboundedSender<String>, gone: UnboundedSender<Gone>) -> Context {
        let ice = Arc::new(Mutex::new(rtc_configuration(&base)));
        Context { base, ice, shared, outgoing, gone }
    }

    fn signal(&self, receiver: PeerId, signal: Signal) {
        let _ = self.outgoing.send(protocol::signal_message(receiver, &signal));
    }
}

fn rtc_configuration(ice: &IceConfig) -> RTCConfiguration {
    let ice_servers = ice
        .servers
        .iter()
        .map(|s| RTCIceServer {
            urls: s.urls.clone(),
            username: s.username.clone(),
            credential: s.credential.clone(),
        })
        .collect();
    let ice_transport_policy = if ice.relay_only {
        RTCIceTransportPolicy::Relay
    } else {
        RTCIceTransportPolicy::All
    };
    RTCConfigurationBuilder::new()
        .with_ice_servers(ice_servers)
        .with_ice_transport_policy(ice_transport_policy)
        .build()
}

fn setting_engine() -> SettingEngine {
    SettingEngineBuilder::new().with_ice_timeouts(Some(ICE_DISCONNECTED), Some(ICE_FAILED), None).build()
}

/// The tokio thread's loop: the signaling connection, the peers it announces, and the game thread's commands.
async fn run(
    url: String,
    ice: IceConfig,
    shared: SharedRef,
    mut commands: UnboundedReceiver<Command>,
) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let (outgoing, outgoing_rx) = unbounded_channel::<String>();
    let (incoming, mut incoming_rx) = unbounded_channel::<Event>();
    let (close_signaling, close_rx) = oneshot::channel();
    tokio::spawn(signaling(url, outgoing_rx, incoming, close_rx));
    let (gone, mut gone_rx) = unbounded_channel::<Gone>();
    let context = Context::new(ice, shared.clone(), outgoing, gone);
    let mut peers: HashMap<PeerId, Peer> = HashMap::new();
    let impairment = Impairment::from_env();
    let mut signaling_open = true;
    let mut close_signaling = Some(close_signaling);

    loop {
        tokio::select! {
            event = incoming_rx.recv(), if signaling_open => match event {
                Some(event) => on_event(&context, &mut peers, event).await,
                None => {
                    signaling_open = false;
                    lock(&shared).signaling_closed = true;
                }
            },
            Some((peer, serial)) = gone_rx.recv() => drop_peer(&shared, &mut peers, peer, serial).await,
            command = commands.recv() => match command {
                Some(Command::Send(channel, peer, data)) => {
                    let open = peers.get(&peer).filter(|p| p.connected.load(Ordering::Acquire));
                    let index = channel;
                    if let Some(channel) = open.and_then(|p| p.channels.get(channel)) {
                        if index != UNRELIABLE {
                            let _ = channel.send(BytesMut::from(&data[..])).await;
                        } else if !impairment.drop_next() {
                            if impairment.lag.is_zero() {
                                let _ = channel.send(BytesMut::from(&data[..])).await;
                            } else {
                                let (channel, lag) = (channel.clone(), impairment.lag);
                                tokio::spawn(async move {
                                    tokio::time::sleep(lag).await;
                                    let _ = channel.send(BytesMut::from(&data[..])).await;
                                });
                            }
                        }
                    }
                }
                Some(Command::Close) | None => {
                    for peer in peers.keys().copied().collect::<Vec<_>>() {
                        drop_peer(&shared, &mut peers, peer, None).await;
                    }
                    if let Some(close) = close_signaling.take() {
                        let (done, finished) = oneshot::channel();
                        if close.send(done).is_ok() {
                            let _ = finished.await;
                        }
                    }
                    return;
                }
            },
        }
    }
}

/// The signaling connection: sends `outgoing` and passes on what the server sends. A connection lost without a close
/// resumes the same peer ID for `RESUME_WINDOW` when the server handed out a `ResumeToken`; otherwise, or after that,
/// `incoming` closes. Messages queued while it reconnects go out after.
async fn signaling(
    url: String,
    mut outgoing: UnboundedReceiver<String>,
    incoming: UnboundedSender<Event>,
    mut close: oneshot::Receiver<oneshot::Sender<()>>,
) {
    let mut id: Option<PeerId> = None;
    let mut token: Option<String> = None;
    let mut lost_at: Option<Instant> = None;
    let mut retry = Duration::from_millis(250);
    let mut test_drop = test_delay("LAST_HARVEST_NET_TEST_DROP_SIGNAL_MS");
    loop {
        let resume = match (&id, &token, lost_at) {
            (Some(id), Some(token), Some(_)) => Some(protocol::resume_message(*id, token)),
            _ => None,
        };
        let target = if resume.is_some() { protocol::resume_url(&url) } else { url.clone() };
        let attempt = tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(target.as_str()));
        let stream = tokio::select! {
            result = attempt => match result {
                Ok(Ok((stream, _))) => Some(stream),
                Ok(Err(e)) => {
                    eprintln!("[net] signaling {url}: {e}");
                    None
                }
                Err(_) => {
                    eprintln!("[net] signaling {url}: timed out");
                    None
                }
            },
            Ok(done) = &mut close => {
                let _ = done.send(());
                return;
            }
        };
        let resuming = lost_at.is_some();
        let mut heard_any = false;
        if let Some(stream) = stream {
            let (mut ws_tx, mut ws_rx) = stream.split();
            if let Some(resume) = resume {
                let _ = ws_tx.send(Message::Text(resume.into())).await;
            }
            let mut keep_alive = tokio::time::interval(KEEP_ALIVE);
            let mut heard = Instant::now();
            let opened = Instant::now();
            loop {
                let drop_at = test_drop.map(|d| opened + d);
                tokio::select! {
                    message = ws_rx.next() => match message {
                        Some(Ok(Message::Text(text))) => {
                            heard = Instant::now();
                            let Some(event) = protocol::parse_event(&text) else { continue };
                            if !heard_any && resuming {
                                eprintln!("[net] signaling resumed");
                            }
                            heard_any = true;
                            lost_at = None;
                            retry = Duration::from_millis(250);
                            match &event {
                                Event::ResumeToken(t) => token = Some(t.clone()),
                                Event::IdAssigned(i) => id = Some(*i),
                                _ => {}
                            }
                            if incoming.send(event).is_err() {
                                return;
                            }
                        }
                        Some(Ok(_)) => heard = Instant::now(),
                        _ => break,
                    },
                    Some(text) = outgoing.recv() => {
                        if ws_tx.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    _ = keep_alive.tick() => {
                        if token.is_some() && heard.elapsed() >= SIGNALING_SILENCE {
                            eprintln!("[net] signaling went silent");
                            break;
                        }
                        let _ = ws_tx.send(Message::Text(protocol::KEEP_ALIVE.into())).await;
                    }
                    _ = tokio::time::sleep_until(drop_at.unwrap_or(opened).into()), if drop_at.is_some() => {
                        eprintln!("[net] test: dropping the signaling connection");
                        test_drop = None;
                        break;
                    }
                    Ok(done) = &mut close => {
                        let _ = ws_tx.close().await;
                        let _ = done.send(());
                        return;
                    }
                }
            }
        }
        // A refused resume closes at once; so does a server that never sent a token.
        if token.is_none() || id.is_none() || (resuming && !heard_any) {
            return;
        }
        let since = *lost_at.get_or_insert_with(Instant::now);
        if since.elapsed() >= RESUME_WINDOW {
            eprintln!("[net] signaling did not come back");
            return;
        }
        tokio::select! {
            _ = tokio::time::sleep(retry) => {}
            Ok(done) = &mut close => {
                let _ = done.send(());
                return;
            }
        }
        retry = (retry * 2).min(Duration::from_secs(2));
    }
}

/// Handles one signaling server message.
async fn on_event(context: &Context, peers: &mut HashMap<PeerId, Peer>, event: Event) {
    match event {
        Event::IdAssigned(id) => lock(&context.shared).id = Some(id),
        Event::ResumeToken(_) => {}
        Event::IceServers(servers) => *lock(&context.ice) = rtc_configuration(&crate::ice::with_relay_servers(&context.base, &servers)),
        Event::NewPeer(id) => match new_peer(context, id, true).await {
            Some(peer) => {
                peers.insert(id, peer);
            }
            None => lock(&context.shared).changes.push((id, PeerState::Disconnected)),
        },
        Event::PeerLeft(id) => {
            let _ = context.gone.send((id, None));
        }
        Event::Signal(sender, signal) => {
            if matches!(signal, Signal::Offer(_)) && !peers.contains_key(&sender) {
                let Some(peer) = new_peer(context, sender, false).await else {
                    lock(&context.shared).changes.push((sender, PeerState::Disconnected));
                    return;
                };
                peers.insert(sender, peer);
            }
            if let Some(peer) = peers.get(&sender) {
                let _ = peer.signals.send(signal);
            }
        }
    }
}

/// A peer connection's callbacks: they run on its driver task, so each only hands its event on.
struct Events {
    context: Context,
    id: PeerId,
    serial: u64,
    remote_set: Arc<AtomicBool>,
    pending: Arc<Mutex<Vec<String>>>,
    ice_state: Arc<Mutex<RTCIceConnectionState>>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Events {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        let json = event.candidate.to_json().ok().and_then(|c| serde_json::to_string(&c).ok());
        if let Some(json) = json {
            if self.remote_set.load(Ordering::Acquire) {
                self.context.signal(self.id, Signal::IceCandidate(json));
            } else {
                lock(&self.pending).push(json);
            }
        }
    }

    async fn on_ice_connection_state_change(&self, state: RTCIceConnectionState) {
        *lock(&self.ice_state) = state;
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        if matches!(state, RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed) {
            let _ = self.context.gone.send((self.id, Some(self.serial)));
        }
    }
}

/// A connection to `id` with negotiated channels 0 and 1. The peer already in the room is the `offerer`.
async fn new_peer(context: &Context, id: PeerId, offerer: bool) -> Option<Peer> {
    let configuration = lock(&context.ice).clone();
    let serial = NEXT_SERIAL.fetch_add(1, Ordering::Relaxed);
    let remote_set = Arc::new(AtomicBool::new(false));
    let pending = Arc::new(Mutex::new(Vec::<String>::new()));
    let ice_state = Arc::new(Mutex::new(RTCIceConnectionState::New));
    let events = Events {
        context: context.clone(),
        id,
        serial,
        remote_set: remote_set.clone(),
        pending: pending.clone(),
        ice_state: ice_state.clone(),
    };
    let connection = PeerConnectionBuilder::new()
        .with_configuration(configuration)
        .with_setting_engine(setting_engine())
        .with_handler(Arc::new(events))
        .with_udp_addrs(vec!["0.0.0.0:0", "[::]:0"])
        .build()
        .await;
    let connection: Arc<dyn PeerConnection> = match connection {
        Ok(connection) => Arc::new(connection),
        Err(e) => {
            eprintln!("[net] peer connection to {:032x}: {e}", id.0);
            return None;
        }
    };
    let connected = Arc::new(AtomicBool::new(false));
    let opened = Arc::new(AtomicUsize::new(0));
    let mut channels = Vec::new();
    for (index, ordered, max_retransmits) in [(RELIABLE, true, None), (UNRELIABLE, false, Some(0))] {
        let init = RTCDataChannelInit {
            ordered,
            max_retransmits,
            negotiated: Some(index as u16),
            ..Default::default()
        };
        let channel = connection.create_data_channel(&format!("matchbox_socket_{index}"), Some(init)).await.ok()?;
        let (opened, connected, shared, gone) =
            (opened.clone(), connected.clone(), context.shared.clone(), context.gone.clone());
        let events = channel.clone();
        tokio::spawn(async move {
            while let Some(event) = events.poll().await {
                match event {
                    DataChannelEvent::OnOpen => {
                        if opened.fetch_add(1, Ordering::AcqRel) + 1 == 2 {
                            connected.store(true, Ordering::Release);
                            lock(&shared).changes.push((id, PeerState::Connected));
                        }
                    }
                    DataChannelEvent::OnMessage(message) => {
                        if !lock(&shared).inbox.deliver(index, id, message.data.to_vec()) {
                            eprintln!("[net] peer {:032x} overflowed the reliable inbox; dropping it", id.0);
                            let _ = gone.send((id, Some(serial)));
                        }
                    }
                    DataChannelEvent::OnClose => break,
                    _ => {}
                }
            }
            let _ = gone.send((id, Some(serial)));
        });
        channels.push(channel);
    }

    let (signals, signals_rx) = unbounded_channel();
    let handshake = Handshake {
        connection: connection.clone(),
        context: context.clone(),
        id,
        serial,
        offerer,
        remote_set,
        pending,
        ice_state,
        connected: connected.clone(),
    };
    tokio::spawn(handshake.run(signals_rx));
    Some(Peer {
        serial,
        connection,
        channels,
        signals,
        connected,
    })
}

/// One peer's offer, answer and ICE candidates; candidates wait on both sides until the remote description is set.
/// Once connected, the offering side restarts ICE while the connection is down, and a failed renegotiation step is
/// only logged.
struct Handshake {
    connection: Arc<dyn PeerConnection>,
    context: Context,
    id: PeerId,
    serial: u64,
    offerer: bool,
    remote_set: Arc<AtomicBool>,
    pending: Arc<Mutex<Vec<String>>>,
    ice_state: Arc<Mutex<RTCIceConnectionState>>,
    connected: Arc<AtomicBool>,
}

impl Handshake {
    async fn run(self, mut signals: UnboundedReceiver<Signal>) {
        if let Err(e) = self.exchange(&mut signals).await {
            eprintln!("[net] handshake with {:032x}: {e}", self.id.0);
            let _ = self.context.gone.send((self.id, Some(self.serial)));
        }
    }

    async fn exchange(&self, signals: &mut UnboundedReceiver<Signal>) -> Result<(), Error> {
        let (connection, context, id) = (&self.connection, &self.context, self.id);
        let mut early_candidates = Vec::new();
        if self.offerer {
            let offer = connection.create_offer(None).await?;
            let sdp = offer.sdp.clone();
            connection.set_local_description(offer).await?;
            context.signal(id, Signal::Offer(sdp));
        }
        let mut check = tokio::time::interval(RESTART_CHECK);
        check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut last_restart: Option<Instant> = None;
        let mut test_restart = test_delay("LAST_HARVEST_NET_TEST_RESTART_MS");
        let mut connected_at: Option<Instant> = None;
        loop {
            let signal = tokio::select! {
                signal = signals.recv() => match signal {
                    Some(signal) => signal,
                    None => return Ok(()),
                },
                _ = check.tick(), if self.offerer => {
                    if !self.connected.load(Ordering::Acquire) {
                        continue;
                    }
                    let connected_at = *connected_at.get_or_insert_with(Instant::now);
                    let forced = test_restart.is_some_and(|d| connected_at.elapsed() >= d);
                    // A restart whose answer was lost waits in Checking, so anything short of connected retries.
                    let down = !matches!(*lock(&self.ice_state), RTCIceConnectionState::Connected | RTCIceConnectionState::Completed);
                    if forced || (down && last_restart.is_none_or(|t| t.elapsed() >= RESTART_EVERY)) {
                        test_restart = test_restart.filter(|_| !forced);
                        last_restart = Some(Instant::now());
                        eprintln!("[net] restarting ICE with {:032x}", id.0);
                        if let Err(e) = self.restart().await {
                            eprintln!("[net] ICE restart with {:032x}: {e}", id.0);
                        }
                    }
                    continue;
                }
            };
            match self.apply(signal, &mut early_candidates).await {
                Err(e) if self.connected.load(Ordering::Acquire) => eprintln!("[net] renegotiation with {:032x}: {e}", id.0),
                result => result?,
            }
        }
    }

    /// A new offer with fresh ICE credentials; the answer comes back through `apply`.
    async fn restart(&self) -> Result<(), Error> {
        self.connection.restart_ice().await?;
        let offer = self.connection.create_offer(None).await?;
        let sdp = offer.sdp.clone();
        self.connection.set_local_description(offer).await?;
        self.context.signal(self.id, Signal::Offer(sdp));
        Ok(())
    }

    /// One offer, answer or ICE candidate from the peer.
    async fn apply(&self, signal: Signal, early_candidates: &mut Vec<RTCIceCandidateInit>) -> Result<(), Error> {
        let (connection, context, id) = (&self.connection, &self.context, self.id);
        let remote = match signal {
            Signal::Offer(sdp) if !self.offerer => RTCSessionDescription::offer(sdp)?,
            Signal::Answer(sdp) if self.offerer => RTCSessionDescription::answer(sdp)?,
            Signal::IceCandidate(json) => {
                match serde_json::from_str::<RTCIceCandidateInit>(&json) {
                    Ok(candidate) if self.remote_set.load(Ordering::Acquire) => connection.add_ice_candidate(candidate).await?,
                    Ok(candidate) => early_candidates.push(candidate),
                    Err(_) => {}
                }
                return Ok(());
            }
            _ => return Ok(()),
        };
        connection.set_remote_description(remote).await?;
        self.remote_set.store(true, Ordering::Release);
        for json in std::mem::take(&mut *lock(&self.pending)) {
            context.signal(id, Signal::IceCandidate(json));
        }
        for candidate in early_candidates.drain(..) {
            connection.add_ice_candidate(candidate).await?;
        }
        if !self.offerer {
            let answer = connection.create_answer(None).await?;
            let sdp = answer.sdp.clone();
            connection.set_local_description(answer).await?;
            context.signal(id, Signal::Answer(sdp));
        }
        Ok(())
    }
}

/// Closes the connection to `id`, or only connection `serial` when given, and reports `Disconnected`, also for a
/// handshake that never connected.
async fn drop_peer(shared: &SharedRef, peers: &mut HashMap<PeerId, Peer>, id: PeerId, serial: Option<u64>) {
    if serial.is_some_and(|s| peers.get(&id).is_some_and(|p| p.serial != s)) {
        return;
    }
    let Some(peer) = peers.remove(&id) else {
        return;
    };
    lock(shared).changes.push((id, PeerState::Disconnected));
    let _ = peer.connection.close().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_late_report_from_an_earlier_connection_keeps_the_current_one() {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        runtime.block_on(async {
            let shared = SharedRef::default();
            let (outgoing, _outgoing_rx) = unbounded_channel();
            let (gone, _gone_rx) = unbounded_channel();
            let context = Context::new(IceConfig::default(), shared.clone(), outgoing, gone);
            let id = PeerId(9);
            let earlier = new_peer(&context, id, false).await.unwrap();
            let current = new_peer(&context, id, false).await.unwrap();
            let (earlier, current_serial) = (earlier.serial, current.serial);
            let mut peers = HashMap::from([(id, current)]);

            drop_peer(&shared, &mut peers, id, Some(earlier)).await;
            assert!(peers.contains_key(&id));
            drop_peer(&shared, &mut peers, id, Some(current_serial)).await;
            assert!(peers.is_empty());
        });
    }

    #[test]
    fn the_relays_ice_servers_reach_new_connections() {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        runtime.block_on(async {
            let (outgoing, _outgoing_rx) = unbounded_channel();
            let (gone, _gone_rx) = unbounded_channel();
            let stun = crate::IceServer { urls: vec!["stun:s.example.com:3478".into()], ..Default::default() };
            let context = Context::new(IceConfig { servers: vec![stun], relay_only: false }, SharedRef::default(), outgoing, gone);
            let mut peers = HashMap::new();
            let turn = |name: &str| crate::IceServer { urls: vec!["turn:t.example.com:3478".into()], username: format!("1:{name}"), credential: "x".into() };
            on_event(&context, &mut peers, Event::IceServers(vec![turn("lh-old")])).await;
            on_event(&context, &mut peers, Event::IceServers(vec![turn("lh-new")])).await;
            let peer = new_peer(&context, PeerId(3), false).await.unwrap();
            let servers = peer.connection.get_configuration().await.ice_servers().to_vec();
            let users: Vec<_> = servers.iter().map(|s| (s.urls[0].as_str(), s.username.as_str())).collect();
            assert_eq!(users, [("stun:s.example.com:3478", ""), ("turn:t.example.com:3478", "1:lh-new")]);
        });
    }

    #[test]
    fn a_poisoned_lock_still_serves_the_game_thread() {
        let shared = SharedRef::default();
        let held = shared.clone();
        let _ = std::thread::spawn(move || {
            let _guard = lock(&held);
            panic!("a webrtc callback panics");
        })
        .join();
        assert!(shared.is_poisoned());
        lock(&shared).id = Some(PeerId(7));
        lock(&shared).changes.push((PeerId(7), PeerState::Connected));
        assert!(lock(&shared).inbox.deliver(RELIABLE, PeerId(7), vec![1]));

        let (commands, _rx) = unbounded_channel();
        let mut socket = Socket { shared, commands };
        assert_eq!(socket.id(), Some(PeerId(7)));
        assert_eq!(socket.update_peers(), vec![(PeerId(7), PeerState::Connected)]);
        assert_eq!(socket.receive(RELIABLE), vec![(PeerId(7), vec![1])]);
        assert!(!socket.signaling_closed());
    }

    #[test]
    fn a_failed_handshake_reports_disconnected() {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        runtime.block_on(async {
            let shared = SharedRef::default();
            let (outgoing, _outgoing_rx) = unbounded_channel();
            let (gone, mut gone_rx) = unbounded_channel();
            let context = Context::new(IceConfig::default(), shared.clone(), outgoing, gone);
            let id = PeerId(9);
            let peer = new_peer(&context, id, true).await.unwrap();
            let serial = peer.serial;
            peer.signals.send(Signal::Answer("not an SDP answer".into())).unwrap();
            let mut peers = HashMap::from([(id, peer)]);

            let failed = tokio::time::timeout(Duration::from_secs(10), gone_rx.recv()).await;
            assert_eq!(failed.unwrap(), Some((id, Some(serial))));
            drop_peer(&shared, &mut peers, id, None).await;
            assert_eq!(std::mem::take(&mut lock(&shared).changes), vec![(id, PeerState::Disconnected)]);
        });
    }
}
