//! The browser side: web-sys signaling over a WebSocket and RTCPeerConnections, driven by browser callbacks.

use crate::inbox::Inbox;
use crate::protocol::{self, Event, Signal};
use crate::{IceConfig, PeerId, PeerState, RELIABLE, UNRELIABLE};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{
    MessageEvent, RtcConfiguration, RtcDataChannel, RtcDataChannelInit, RtcDataChannelState, RtcDataChannelType,
    RtcIceCandidateInit, RtcIceConnectionState, RtcIceGatheringState, RtcIceServer, RtcIceTransportPolicy,
    RtcOfferOptions, RtcPeerConnection, RtcPeerConnectionIceEvent, RtcPeerConnectionState, RtcSdpType,
    RtcSessionDescriptionInit, WebSocket,
};

const KEEP_ALIVE_MS: i32 = 2000;
/// A resumable signaling connection silent this long is lost.
const SILENCE_MS: f64 = 6000.0;
/// How long a lost signaling connection tries to resume. The relay holds the place for 30 s.
const RESUME_WINDOW_MS: f64 = 25000.0;
const RESTART_CHECK_MS: i32 = 1000;
const RESTART_EVERY_MS: f64 = 4000.0;
/// An offer or first answer goes out once ICE gathering completes, or after this.
const GATHER_TIMEOUT_MS: f64 = 5000.0;

type Callback = Closure<dyn FnMut(JsValue)>;
type StateRef = Rc<RefCell<State>>;

fn now() -> f64 {
    web_sys::window().and_then(|w| w.performance()).map_or(0.0, |p| p.now())
}

fn warn(message: &str) {
    web_sys::console::warn_1(&message.into());
}

fn error_text(e: &JsValue) -> String {
    js_sys::Reflect::get(e, &"message".into()).ok().and_then(|m| m.as_string()).unwrap_or_else(|| format!("{e:?}"))
}

fn callback(f: impl FnMut(JsValue) + 'static) -> Callback {
    Closure::new(f)
}

/// Runs `f` after the current browser callback returns; anything that drops a callback's own closure goes here.
fn defer(f: impl FnOnce() + 'static) {
    spawn_local(async move { f() });
}

fn set_timeout(ms: f64, f: impl FnOnce() + 'static) {
    if let Some(window) = web_sys::window() {
        let f = Closure::once_into_js(f);
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(f.unchecked_ref(), ms as i32);
    }
}

async fn sleep(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
        }
    });
    let _ = JsFuture::from(promise).await;
}

/// A repeating browser timer, cleared on drop.
struct Interval {
    id: i32,
    _callback: Closure<dyn FnMut()>,
}

impl Interval {
    fn new(ms: i32, f: impl FnMut() + 'static) -> Option<Interval> {
        let callback = Closure::<dyn FnMut()>::new(f);
        let id = web_sys::window()?
            .set_interval_with_callback_and_timeout_and_arguments_0(callback.as_ref().unchecked_ref(), ms)
            .ok()?;
        Some(Interval { id, _callback: callback })
    }
}

impl Drop for Interval {
    fn drop(&mut self) {
        if let Some(window) = web_sys::window() {
            window.clear_interval_with_handle(self.id);
        }
    }
}

struct Peer {
    /// Tells this connection apart from a later one to the same peer, whose late callbacks must not touch it.
    serial: u32,
    pc: RtcPeerConnection,
    channels: Vec<RtcDataChannel>,
    open: usize,
    connected: bool,
    offerer: bool,
    remote_set: bool,
    /// Local candidates held until the remote description is set, and remote ones held for the same.
    pending: Vec<String>,
    candidates: Vec<String>,
    restarted_at: Option<f64>,
    _handlers: Vec<Callback>,
}

impl Peer {
    fn close(&self) {
        for channel in &self.channels {
            channel.set_onopen(None);
            channel.set_onmessage(None);
            channel.set_onclose(None);
            channel.close();
        }
        self.pc.set_onicecandidate(None);
        self.pc.set_oniceconnectionstatechange(None);
        self.pc.set_onconnectionstatechange(None);
        self.pc.close();
    }
}

#[derive(Default)]
struct State {
    url: String,
    /// The game's own ICE servers, and what new connections use: those and the relay's latest.
    base_ice: IceConfig,
    ice: IceConfig,
    socket: Option<WebSocket>,
    socket_handlers: Vec<Callback>,
    keep_alive: Option<Interval>,
    restart_check: Option<Interval>,
    /// Counts signaling connections, so callbacks from a replaced one do nothing.
    connection: u32,
    next_serial: u32,
    token: Option<String>,
    id: Option<PeerId>,
    lost_at: Option<f64>,
    retry_ms: f64,
    heard_at: f64,
    /// Signals queued while the connection resumes.
    outbox: Vec<String>,
    failed: bool,
    closed: bool,
    peers: HashMap<PeerId, Peer>,
    changes: Vec<(PeerId, PeerState)>,
    inbox: Inbox,
}

impl State {
    fn signal(&mut self, receiver: PeerId, signal: Signal) {
        let text = protocol::signal_message(receiver, &signal);
        match &self.socket {
            Some(socket) if socket.ready_state() == WebSocket::OPEN => {
                let _ = socket.send_with_str(&text);
            }
            _ if self.lost_at.is_some() => self.outbox.push(text),
            _ => {}
        }
    }

    fn clear_socket(&mut self) {
        if let Some(socket) = self.socket.take() {
            socket.set_onopen(None);
            socket.set_onmessage(None);
            socket.set_onclose(None);
        }
        self.socket_handlers.clear();
        self.keep_alive = None;
    }

    fn peer(&mut self, id: PeerId, serial: u32) -> Option<&mut Peer> {
        self.peers.get_mut(&id).filter(|p| p.serial == serial)
    }
}

/// Closes the connection to `id`, or only connection `serial` when given, and reports `Disconnected`.
fn drop_peer(state: &StateRef, id: PeerId, serial: Option<u32>) {
    let mut s = state.borrow_mut();
    if serial.is_some_and(|serial| s.peers.get(&id).is_some_and(|p| p.serial != serial)) {
        return;
    }
    let Some(peer) = s.peers.remove(&id) else { return };
    s.changes.push((id, PeerState::Disconnected));
    drop(s);
    peer.close();
}

/// One signaling connection. Lost without a close, it resumes the same peer ID while the relay allows.
fn open_signaling(state: &StateRef, target: &str, resuming: bool) {
    let socket = match WebSocket::new(target) {
        Ok(socket) => socket,
        Err(e) => {
            warn(&format!("[net] signaling: {}", error_text(&e)));
            lost(state, resuming, false);
            return;
        }
    };
    let mut s = state.borrow_mut();
    s.clear_socket();
    s.connection += 1;
    let connection = s.connection;
    let heard_any = Rc::new(Cell::new(false));
    let current = move |s: &State| s.connection == connection && !s.closed;

    let weak = Rc::downgrade(state);
    let on_open = callback(move |_| {
        let Some(state) = weak.upgrade() else { return };
        let mut s = state.borrow_mut();
        if !current(&s) {
            return;
        }
        s.heard_at = now();
        let outbox = std::mem::take(&mut s.outbox);
        if let Some(socket) = &s.socket {
            for text in outbox {
                let _ = socket.send_with_str(&text);
            }
        }
    });
    let (weak, heard) = (Rc::downgrade(state), heard_any.clone());
    let on_message = callback(move |e| {
        let Some(state) = weak.upgrade() else { return };
        let Some(text) = e.unchecked_into::<MessageEvent>().data().as_string() else { return };
        let mut s = state.borrow_mut();
        if !current(&s) {
            return;
        }
        s.heard_at = now();
        let Some(event) = protocol::parse_event(&text) else { return };
        if !heard.get() && resuming {
            web_sys::console::log_1(&"[net] signaling resumed".into());
        }
        heard.set(true);
        s.lost_at = None;
        s.retry_ms = 250.0;
        drop(s);
        on_event(&state, event);
    });
    let (weak, heard) = (Rc::downgrade(state), heard_any.clone());
    let on_close = callback(move |_| {
        let Some(state) = weak.upgrade() else { return };
        if current(&state.borrow()) {
            let heard = heard.get();
            defer(move || lost(&state, resuming, heard));
        }
    });
    socket.set_onopen(Some(on_open.as_ref().unchecked_ref()));
    socket.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    socket.set_onclose(Some(on_close.as_ref().unchecked_ref()));
    s.socket_handlers = vec![on_open, on_message, on_close];
    s.socket = Some(socket);
    s.heard_at = now();

    let weak = Rc::downgrade(state);
    s.keep_alive = Interval::new(KEEP_ALIVE_MS, move || {
        let Some(state) = weak.upgrade() else { return };
        let s = state.borrow();
        if !current(&s) {
            return;
        }
        if s.token.is_some() && now() - s.heard_at >= SILENCE_MS {
            warn("[net] signaling went silent");
            // Abandoned, not closed: a close frame would make the relay give the place up.
            if let Some(socket) = &s.socket {
                socket.set_onclose(None);
                socket.set_onmessage(None);
            }
            drop(s);
            let heard = heard_any.get();
            defer(move || lost(&state, resuming, heard));
            return;
        }
        if let Some(socket) = s.socket.as_ref().filter(|s| s.ready_state() == WebSocket::OPEN) {
            let _ = socket.send_with_str(protocol::KEEP_ALIVE);
        }
    });
}

fn lost(state: &StateRef, resuming: bool, heard_any: bool) {
    let mut s = state.borrow_mut();
    s.clear_socket();
    if s.closed {
        return;
    }
    let (Some(id), Some(token)) = (s.id, s.token.clone()) else {
        s.failed = true;
        return;
    };
    if resuming && !heard_any {
        s.failed = true;
        return;
    }
    let now = now();
    if now - *s.lost_at.get_or_insert(now) >= RESUME_WINDOW_MS {
        warn("[net] signaling did not come back");
        s.failed = true;
        return;
    }
    let target = protocol::resume_url(&s.url, id, &token);
    let delay = s.retry_ms;
    s.retry_ms = (s.retry_ms * 2.0).min(2000.0);
    let connection = s.connection;
    let weak = Rc::downgrade(state);
    set_timeout(delay, move || {
        let Some(state) = weak.upgrade() else { return };
        let current = { let s = state.borrow(); s.connection == connection && !s.closed };
        if current {
            open_signaling(&state, &target, true);
        }
    });
}

fn on_event(state: &StateRef, event: Event) {
    match event {
        Event::IdAssigned(id) => state.borrow_mut().id = Some(id),
        Event::ResumeToken(token) => state.borrow_mut().token = Some(token),
        Event::IceServers(servers) => {
            let mut s = state.borrow_mut();
            s.ice = crate::ice::with_relay_servers(&s.base_ice, &servers);
            let config = rtc_configuration(&s.ice);
            for peer in s.peers.values() {
                if let Err(e) = peer.pc.set_configuration_with_configuration(&config) {
                    warn(&format!("[net] new ICE servers: {}", error_text(&e)));
                }
            }
        }
        Event::NewPeer(id) => spawn_local(offer(state.clone(), id)),
        Event::PeerLeft(id) => drop_peer(state, id, None),
        Event::Signal(sender, signal) => spawn_local(on_signal(state.clone(), sender, signal)),
    }
}

fn rtc_configuration(ice: &IceConfig) -> RtcConfiguration {
    let servers = js_sys::Array::new();
    for server in &ice.servers {
        let urls = js_sys::Array::new();
        for url in &server.urls {
            urls.push(&url.into());
        }
        let ice_server = RtcIceServer::new();
        ice_server.set_urls(&urls);
        if !server.username.is_empty() {
            ice_server.set_username(&server.username);
        }
        if !server.credential.is_empty() {
            ice_server.set_credential(&server.credential);
        }
        servers.push(&ice_server);
    }
    let config = RtcConfiguration::new();
    config.set_ice_servers(&servers);
    config.set_ice_transport_policy(if ice.relay_only { RtcIceTransportPolicy::Relay } else { RtcIceTransportPolicy::All });
    config
}

/// A connection to `id` with negotiated channels 0 and 1; it closes any other connection to `id`.
fn new_peer(state: &StateRef, id: PeerId, offerer: bool) -> Option<(u32, RtcPeerConnection)> {
    let mut s = state.borrow_mut();
    let pc = RtcPeerConnection::new_with_configuration(&rtc_configuration(&s.ice)).ok()?;
    s.next_serial += 1;
    let serial = s.next_serial;
    let mut handlers = Vec::new();

    // Firefox drops the connection without a handler here (matchbox issue 36).
    let on_ice_state = callback(|_| {});
    pc.set_oniceconnectionstatechange(Some(on_ice_state.as_ref().unchecked_ref()));
    handlers.push(on_ice_state);

    let weak = Rc::downgrade(state);
    let on_candidate = callback(move |e| {
        let Some(state) = weak.upgrade() else { return };
        let Some(candidate) = e.unchecked_into::<RtcPeerConnectionIceEvent>().candidate() else { return };
        let Some(json) = js_sys::JSON::stringify(&candidate.to_json()).ok().and_then(|j| j.as_string()) else { return };
        let mut s = state.borrow_mut();
        let Some(peer) = s.peer(id, serial) else { return };
        if peer.remote_set {
            s.signal(id, Signal::IceCandidate(json));
        } else {
            peer.pending.push(json);
        }
    });
    pc.set_onicecandidate(Some(on_candidate.as_ref().unchecked_ref()));
    handlers.push(on_candidate);

    let weak = Rc::downgrade(state);
    let pc_for_state = pc.clone();
    let on_state = callback(move |_| {
        if pc_for_state.connection_state() == RtcPeerConnectionState::Failed
            && let Some(state) = weak.upgrade()
        {
            defer(move || drop_peer(&state, id, Some(serial)));
        }
    });
    pc.set_onconnectionstatechange(Some(on_state.as_ref().unchecked_ref()));
    handlers.push(on_state);

    let mut channels = Vec::new();
    for (index, ordered, max_retransmits) in [(RELIABLE, true, None), (UNRELIABLE, false, Some(0))] {
        let init = RtcDataChannelInit::new();
        init.set_ordered(ordered);
        init.set_negotiated(true);
        init.set_id(index as u16);
        if let Some(max) = max_retransmits {
            init.set_max_retransmits(max);
        }
        let channel = pc.create_data_channel_with_data_channel_dict(&format!("matchbox_socket_{index}"), &init);
        channel.set_binary_type(RtcDataChannelType::Arraybuffer);

        let weak = Rc::downgrade(state);
        let on_open = callback(move |_| {
            let Some(state) = weak.upgrade() else { return };
            let mut s = state.borrow_mut();
            let Some(peer) = s.peer(id, serial) else { return };
            peer.open += 1;
            if peer.open == 2 {
                peer.connected = true;
                s.changes.push((id, PeerState::Connected));
            }
        });
        let weak = Rc::downgrade(state);
        let on_message = callback(move |e| {
            let Some(state) = weak.upgrade() else { return };
            let Ok(buffer) = e.unchecked_into::<MessageEvent>().data().dyn_into::<js_sys::ArrayBuffer>() else { return };
            let mut s = state.borrow_mut();
            if s.peer(id, serial).is_none() {
                return;
            }
            if !s.inbox.deliver(index, id, js_sys::Uint8Array::new(&buffer).to_vec()) {
                warn(&format!("[net] peer {:032x} overflowed the reliable inbox; dropping it", id.0));
                drop(s);
                defer(move || drop_peer(&state, id, Some(serial)));
            }
        });
        let weak = Rc::downgrade(state);
        let on_close = callback(move |_| {
            if let Some(state) = weak.upgrade() {
                defer(move || drop_peer(&state, id, Some(serial)));
            }
        });
        channel.set_onopen(Some(on_open.as_ref().unchecked_ref()));
        channel.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        channel.set_onclose(Some(on_close.as_ref().unchecked_ref()));
        handlers.extend([on_open, on_message, on_close]);
        channels.push(channel);
    }

    let peer = Peer {
        serial,
        pc: pc.clone(),
        channels,
        open: 0,
        connected: false,
        offerer,
        remote_set: false,
        pending: Vec::new(),
        candidates: Vec::new(),
        restarted_at: None,
        _handlers: handlers,
    };
    let earlier = s.peers.insert(id, peer);
    drop(s);
    if let Some(earlier) = earlier {
        earlier.close();
    }
    Some((serial, pc))
}

async fn gathered(pc: &RtcPeerConnection) {
    let start = now();
    while pc.ice_gathering_state() != RtcIceGatheringState::Complete && now() - start < GATHER_TIMEOUT_MS {
        sleep(50).await;
    }
}

fn local_sdp(pc: &RtcPeerConnection) -> Result<String, JsValue> {
    pc.local_description().map(|d| d.sdp()).ok_or_else(|| "no local description".into())
}

async fn set_remote(pc: &RtcPeerConnection, kind: RtcSdpType, sdp: &str) -> Result<(), JsValue> {
    let description = RtcSessionDescriptionInit::new(kind);
    description.set_sdp(sdp);
    JsFuture::from(pc.set_remote_description(&description)).await.map(drop)
}

async fn set_local(pc: &RtcPeerConnection, description: JsValue) -> Result<(), JsValue> {
    JsFuture::from(pc.set_local_description(description.unchecked_ref())).await.map(drop)
}

async fn add_candidate(pc: &RtcPeerConnection, json: &str) {
    let Ok(candidate) = js_sys::JSON::parse(json) else { return };
    if candidate.is_null() {
        return;
    }
    let candidate: RtcIceCandidateInit = candidate.unchecked_into();
    if let Err(e) = JsFuture::from(pc.add_ice_candidate_with_opt_rtc_ice_candidate_init(Some(&candidate))).await {
        warn(&format!("[net] ice candidate: {}", error_text(&e)));
    }
}

async fn offer(state: StateRef, id: PeerId) {
    let Some((serial, pc)) = new_peer(&state, id, true) else {
        state.borrow_mut().changes.push((id, PeerState::Disconnected));
        return;
    };
    let sdp = async {
        set_local(&pc, JsFuture::from(pc.create_offer()).await?).await?;
        gathered(&pc).await;
        local_sdp(&pc)
    };
    match sdp.await {
        Ok(sdp) => {
            let mut s = state.borrow_mut();
            if s.peer(id, serial).is_some() {
                s.signal(id, Signal::Offer(sdp));
            }
        }
        Err(e) => {
            warn(&format!("[net] offer: {}", error_text(&e)));
            drop_peer(&state, id, Some(serial));
        }
    }
}

/// Marks the remote description set, then sends and applies the candidates held for it.
async fn remote_set(state: &StateRef, id: PeerId, serial: u32) {
    let (pc, candidates) = {
        let mut s = state.borrow_mut();
        let Some(peer) = s.peer(id, serial) else { return };
        peer.remote_set = true;
        let pending = std::mem::take(&mut peer.pending);
        let candidates = std::mem::take(&mut peer.candidates);
        let pc = peer.pc.clone();
        for json in pending {
            s.signal(id, Signal::IceCandidate(json));
        }
        (pc, candidates)
    };
    for json in candidates {
        add_candidate(&pc, &json).await;
    }
}

async fn on_signal(state: StateRef, id: PeerId, signal: Signal) {
    let existing = state.borrow().peers.get(&id).map(|p| (p.serial, p.pc.clone(), p.connected, p.remote_set));
    let (serial, connected, result) = match (signal, existing) {
        (Signal::Offer(sdp), existing) => {
            let (serial, pc, connected) = match existing {
                Some((serial, pc, connected, _)) => (serial, pc, connected),
                None => match new_peer(&state, id, false) {
                    Some((serial, pc)) => (serial, pc, false),
                    None => {
                        state.borrow_mut().changes.push((id, PeerState::Disconnected));
                        return;
                    }
                },
            };
            let answer = async {
                set_remote(&pc, RtcSdpType::Offer, &sdp).await?;
                remote_set(&state, id, serial).await;
                set_local(&pc, JsFuture::from(pc.create_answer()).await?).await?;
                // A renegotiation answers at once; its candidates trickle.
                if !connected {
                    gathered(&pc).await;
                }
                local_sdp(&pc)
            };
            let result = answer.await.map(|sdp| {
                let mut s = state.borrow_mut();
                if s.peer(id, serial).is_some() {
                    s.signal(id, Signal::Answer(sdp));
                }
            });
            (serial, connected, result)
        }
        (Signal::Answer(sdp), Some((serial, pc, connected, _))) => {
            let result = set_remote(&pc, RtcSdpType::Answer, &sdp).await;
            if result.is_ok() {
                remote_set(&state, id, serial).await;
            }
            (serial, connected, result)
        }
        (Signal::IceCandidate(json), Some((_, pc, _, true))) => {
            add_candidate(&pc, &json).await;
            return;
        }
        (Signal::IceCandidate(json), Some((serial, ..))) => {
            if let Some(peer) = state.borrow_mut().peer(id, serial) {
                peer.candidates.push(json);
            }
            return;
        }
        _ => return,
    };
    if let Err(e) = result {
        if connected {
            warn(&format!("[net] renegotiation: {}", error_text(&e)));
        } else {
            warn(&format!("[net] handshake: {}", error_text(&e)));
            drop_peer(&state, id, Some(serial));
        }
    }
}

/// The offering side restarts ICE on a connected peer whose connection is down; a restart whose answer was lost
/// waits in `checking`, so anything short of connected retries.
fn check_peers(state: &StateRef) {
    let now = now();
    let mut restarts = Vec::new();
    for (&id, peer) in state.borrow_mut().peers.iter_mut() {
        if !peer.offerer || !peer.connected {
            continue;
        }
        if matches!(peer.pc.ice_connection_state(), RtcIceConnectionState::Connected | RtcIceConnectionState::Completed) {
            continue;
        }
        if peer.restarted_at.is_some_and(|t| now - t < RESTART_EVERY_MS) {
            continue;
        }
        peer.restarted_at = Some(now);
        restarts.push((id, peer.serial, peer.pc.clone()));
    }
    for (id, serial, pc) in restarts {
        web_sys::console::log_1(&format!("[net] restarting ICE with {:032x}", id.0).into());
        let state = state.clone();
        spawn_local(async move {
            let options = RtcOfferOptions::new();
            options.set_ice_restart(true);
            let sdp = async {
                set_local(&pc, JsFuture::from(pc.create_offer_with_rtc_offer_options(&options)).await?).await?;
                local_sdp(&pc)
            };
            match sdp.await {
                Ok(sdp) => {
                    let mut s = state.borrow_mut();
                    if s.peer(id, serial).is_some() {
                        s.signal(id, Signal::Offer(sdp));
                    }
                }
                Err(e) => warn(&format!("[net] ICE restart: {}", error_text(&e))),
            }
        });
    }
}

/// The game's side of a room connection; closing or dropping it closes every connection it opened.
pub struct Socket {
    state: StateRef,
}

impl Socket {
    pub fn connect(room_url: &str, ice: &IceConfig) -> Socket {
        let state = Rc::new(RefCell::new(State {
            url: room_url.to_string(),
            base_ice: ice.clone(),
            ice: ice.clone(),
            retry_ms: 250.0,
            ..Default::default()
        }));
        open_signaling(&state, room_url, false);
        let weak: Weak<RefCell<State>> = Rc::downgrade(&state);
        let check = Interval::new(RESTART_CHECK_MS, move || {
            if let Some(state) = weak.upgrade() {
                check_peers(&state);
            }
        });
        state.borrow_mut().restart_check = check;
        Socket { state }
    }

    pub fn id(&mut self) -> Option<PeerId> {
        self.state.borrow().id
    }

    pub fn update_peers(&mut self) -> Vec<(PeerId, PeerState)> {
        std::mem::take(&mut self.state.borrow_mut().changes)
    }

    pub fn signaling_closed(&self) -> bool {
        let s = self.state.borrow();
        s.failed || s.closed
    }

    pub fn send(&mut self, channel: usize, peer: PeerId, data: &[u8]) {
        let s = self.state.borrow();
        let open = s.peers.get(&peer).filter(|p| p.connected).and_then(|p| p.channels.get(channel));
        if let Some(channel) = open.filter(|c| c.ready_state() == RtcDataChannelState::Open) {
            let _ = channel.send_with_u8_array(data);
        }
    }

    pub fn receive(&mut self, channel: usize) -> Vec<(PeerId, Vec<u8>)> {
        self.state.borrow_mut().inbox.take(channel)
    }

    pub fn close(&mut self) {
        let mut s = self.state.borrow_mut();
        if s.closed {
            return;
        }
        s.closed = true;
        if let Some(socket) = &s.socket {
            let _ = socket.close();
        }
        s.clear_socket();
        s.restart_check = None;
        let peers: Vec<Peer> = s.peers.drain().map(|(_, p)| p).collect();
        drop(s);
        for peer in peers {
            peer.close();
        }
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        self.close();
    }
}
