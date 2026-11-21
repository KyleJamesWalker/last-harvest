//! The title's Online pages: host or join a room, the lobby with its ready marks, and typing a room code or the
//! relay's address with a pad, a touch screen or a keyboard.

use last_harvest_lobby::{ALPHABET, Identity, ModPack, RoomCode, Settings, Start};

use crate::draw::{Align, Canvas};
use crate::harvest::{self, Harvest, replay};
use crate::input::{Buttons, Input};
use crate::menu::{Page, fit_label, wrap_label};
use crate::netplay::{Happening, Net, Packet, Request, Traffic};
use crate::{DIM, Difficulty, Game, HOT, SCREEN_W, Sfx, State, WHITE};

const RED: [f32; 4] = [1.0, 0.55, 0.5, 1.0];
const RELAY_KEYS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789.:-/";
const RELAY_MAX: usize = crate::save::RELAY_MAX;
/// The Join page's line naming the relay or nearby host it joins through.
const THROUGH: [f32; 4] = [40.0, 212.0, SCREEN_W - 80.0, 34.0];
const KEYS_TOP: f32 = 250.0;
/// The lobby's buttons: Leave, Ready, Start.
const BUTTONS: [[f32; 4]; 3] = [[40.0, 650.0, 290.0, 80.0], [367.0, 650.0, 290.0, 80.0], [694.0, 650.0, 290.0, 80.0]];

fn inside([x, y]: [f32; 2], [rx, ry, w, h]: [f32; 4]) -> bool {
    (rx..rx + w).contains(&x) && (ry..ry + h).contains(&y)
}

/// Text typed on the on-screen keys: the text and the key the pad has highlighted.
#[derive(Clone, Debug, Default)]
pub(crate) struct Entry {
    pub text: String,
    pub at: usize,
}

/// A row of the Online page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Host,
    HostLan,
    Join,
    Relay,
    Back,
}

impl Row {
    fn name(self) -> &'static str {
        match self {
            Row::Host => "Host a night",
            Row::HostLan => "Host on this network",
            Row::Join => "Join with a code",
            Row::Relay => "Relay",
            Row::Back => "Back",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
    Char(u8),
    Delete,
    Done,
}

/// Every key with its box: the characters in rows of `cols`, then Delete and Done.
fn keys(chars: &[u8], cols: usize) -> Vec<([f32; 4], Key)> {
    let w = (SCREEN_W - 80.0) / cols as f32;
    let h = if cols > 7 { 70.0 } else { 90.0 };
    let mut out: Vec<([f32; 4], Key)> = chars.iter().enumerate().map(|(i, &c)| ([40.0 + (i % cols) as f32 * w, KEYS_TOP + (i / cols) as f32 * h, w - 8.0, h - 8.0], Key::Char(c))).collect();
    let y = KEYS_TOP + chars.len().div_ceil(cols) as f32 * h + 8.0;
    out.push(([40.0, y, 460.0, 76.0], Key::Delete));
    out.push(([524.0, y, 460.0, 76.0], Key::Done));
    out
}

/// The key a d-pad step from key `at` lands on: left and right wrap within a row, up and down wrap through the rows,
/// and the last row is Delete then Done, under the left and right halves of the keys.
fn step_key(at: usize, dx: i32, dy: i32, chars: usize, cols: usize) -> usize {
    let rows = chars.div_ceil(cols);
    let row_len = |r: usize| if r == rows { 2 } else { (chars - r * cols).min(cols) };
    let (row, col) = if at < chars { (at / cols, at % cols) } else { (rows, at - chars) };
    let col = (col as i32 + dx).rem_euclid(row_len(row) as i32) as usize;
    let to = (row as i32 + dy).rem_euclid(rows as i32 + 1) as usize;
    let col = match (row == rows, to == rows) {
        _ if to == row => col,
        (false, true) => usize::from(col * 2 >= cols),
        (true, false) => if col == 0 { 0 } else { cols - 1 },
        _ => col,
    }
    .min(row_len(to) - 1);
    if to == rows { chars + col } else { to * cols + col }
}

/// What a press on the keys did.
enum Typed {
    Nothing,
    Done,
    Back,
}

impl Entry {
    fn input(&mut self, input: &Input, tapped: Option<[f32; 2]>, chars: &[u8], cols: usize, max: usize, sounds: &mut Vec<Sfx>) -> Typed {
        let all = keys(chars, cols);
        let n = all.len();
        let (dx, dy) = (input.pressed.contains(Buttons::RIGHT) as i32 - input.pressed.contains(Buttons::LEFT) as i32, input.pressed.contains(Buttons::DOWN) as i32 - input.pressed.contains(Buttons::UP) as i32);
        if dx != 0 || dy != 0 {
            self.at = step_key(self.at.min(n - 1), dx, dy, chars.len(), cols);
            sounds.push(Sfx::Tick(2));
            return Typed::Nothing;
        }
        let key = match tapped {
            Some(p) => all.iter().position(|(r, _)| inside(p, *r)).inspect(|&i| self.at = i).map(|i| all[i].1),
            None if input.pressed.contains(Buttons::A) => Some(all[self.at.min(n - 1)].1),
            None if input.pressed.contains(Buttons::START) => Some(Key::Done),
            None if input.pressed.contains(Buttons::B) && self.text.is_empty() => return Typed::Back,
            None if input.pressed.contains(Buttons::B) => Some(Key::Delete),
            None => None,
        };
        match key {
            Some(Key::Char(c)) if self.text.len() < max => {
                self.text.push(c as char);
                sounds.push(Sfx::Tick(6));
            }
            Some(Key::Char(_)) => sounds.push(Sfx::Miss),
            Some(Key::Delete) => {
                self.text.pop();
                sounds.push(Sfx::Tick(0));
            }
            Some(Key::Done) => return Typed::Done,
            None => {}
        }
        Typed::Nothing
    }

    fn typed(&mut self, text: &str, chars: &[u8], max: usize, upper: bool) -> bool {
        let mut done = false;
        for c in text.chars() {
            match c {
                '\u{8}' | '\u{7f}' => drop(self.text.pop()),
                '\r' | '\n' => done = true,
                c => {
                    let c = if upper { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() };
                    if c.is_ascii() && chars.contains(&(c as u8)) && self.text.len() < max {
                        self.text.push(c);
                    }
                }
            }
        }
        done
    }

    fn draw(&self, c: &mut Canvas, chars: &[u8], cols: usize) {
        for (i, (r, key)) in keys(chars, cols).into_iter().enumerate() {
            let on = i == self.at;
            c.rect(r[0], r[1], r[2], r[3], if on { [1.0, 0.85, 0.3, 0.3] } else { [1.0, 1.0, 1.0, 0.08] });
            let (name, size) = match key {
                Key::Char(ch) => ((ch as char).to_string(), if cols > 7 { 40.0 } else { 52.0 }),
                Key::Delete => ("Delete".to_string(), 40.0),
                Key::Done => ("Done".to_string(), 40.0),
            };
            c.label(name, r[0] + r[2] / 2.0, r[1] + r[3] / 2.0 - size / 2.0, size, if on { HOT } else { WHITE }, Align::Center);
        }
    }
}

/// The title's online state: what this platform offers, what is typed, and the last word on how it went.
#[derive(Clone, Debug, Default)]
pub(crate) struct Online {
    pub available: bool,
    pub relay_env: Option<String>,
    pub version: String,
    pub pak: u64,
    pub message: String,
    pub row: usize,
    pub code: Entry,
    pub relay: Entry,
    pub relay_from: Page,
    pub fixed: Option<RoomCode>,
    pub secure_page: bool,
    /// The port this platform hosts its own relay on, for play on one network; none where it can't (the web).
    pub lan: Option<u16>,
    /// This device's address on its network, as the LAN lobby shows it.
    pub lan_address: Option<String>,
    /// The platform is to start the LAN relay before it connects.
    pub lan_wanted: bool,
    /// The room is hosted on this device's LAN relay.
    pub lan_session: bool,
    /// LAN hosts heard nearby, as `address:port`.
    pub nearby: Vec<String>,
    /// The nearby host the Join page joins through; none for the relay setting.
    pub through: Option<String>,
    /// The player picked `through`, so a host heard later does not take its place.
    pub through_picked: bool,
}

impl Game {
    pub fn set_online(&mut self, on: bool) {
        self.online.available = on;
    }

    pub(crate) fn online_available(&self) -> bool {
        self.online.available
    }

    pub fn lobby_ready(&self) -> bool {
        self.net.as_ref().is_some_and(|n| n.status().is_none())
    }

    pub fn lobby_size(&self) -> usize {
        self.net.as_ref().map_or(0, |n| n.roster().0.len())
    }

    pub fn lobby_all_ready(&self) -> bool {
        self.net.as_ref().is_some_and(|n| {
            let (list, _) = n.roster();
            list.len() > 1 && list.iter().all(|m| m.ready)
        })
    }

    pub fn wants_text(&self) -> bool {
        matches!(self.state, State::Title) && matches!(self.page, Page::Join | Page::Relay)
    }

    /// The page was served over https, so it can reach only a secure relay.
    pub fn set_secure_page(&mut self, on: bool) {
        self.online.secure_page = on;
    }

    /// Offers hosting on this network, on a relay of this device's own at `port`; the web build can't serve one.
    pub fn set_lan(&mut self, port: Option<u16>) {
        self.online.lan = port;
    }

    pub fn set_lan_address(&mut self, address: Option<String>) {
        self.online.lan_address = address;
    }

    /// The LAN hosts heard nearby, as `address:port`; the Join page offers the first until the player picks.
    pub fn set_nearby(&mut self, hosts: Vec<String>) {
        if !self.online.through_picked && self.online.through.is_none() {
            self.online.through = hosts.first().cloned();
        }
        self.online.nearby = hosts;
    }

    /// The port to start this device's LAN relay on, once, before the session's connect.
    pub fn take_lan_host(&mut self) -> Option<u16> {
        std::mem::take(&mut self.online.lan_wanted).then_some(self.online.lan).flatten()
    }

    fn online_rows(&self) -> Vec<Row> {
        let lan = self.online.lan.is_some().then_some(Row::HostLan);
        [Some(Row::Host), lan, Some(Row::Join), Some(Row::Relay), Some(Row::Back)].into_iter().flatten().collect()
    }

    pub fn set_relay(&mut self, relay: &str) {
        self.online.relay_env = Some(relay.to_string());
    }

    pub fn set_identity(&mut self, version: &str, pak: u64) {
        (self.online.version, self.online.pak) = (version.to_string(), pak);
    }

    fn relay(&self) -> String {
        self.online.relay_env.clone().unwrap_or_else(|| self.save.relay.as_str().to_string())
    }

    fn relay_urls(&mut self) -> Option<(String, String)> {
        match crate::netplay::relay_urls(&self.relay(), self.online.secure_page) {
            Ok(urls) => Some(urls),
            Err(why) => {
                self.online.message = why;
                self.sounds.push(Sfx::Miss);
                if self.page == Page::Main {
                    self.page = Page::Online;
                }
                None
            }
        }
    }

    #[doc(hidden)]
    pub fn identity_for_test(&self) -> Identity {
        self.identity()
    }

    fn identity(&self) -> Identity {
        let version = if self.online.version.is_empty() { "dev".to_string() } else { self.online.version.clone() };
        let pak = if self.online.pak != 0 { self.online.pak } else { crate::data::fingerprint().unwrap_or_default() };
        let mods = crate::data::load_report().mod_packs.iter().map(|(name, fingerprint)| ModPack { name: name.clone(), fingerprint: *fingerprint }).collect();
        Identity { game_version: version, pak, mods }
    }

    fn local_cards(&self) -> Vec<String> {
        let mine = self.save.player.as_str().to_string();
        let others: Vec<String> = crate::player_looks().iter().map(|l| l.id.clone()).filter(|id| *id != mine).collect();
        (0..self.players.clamp(1, last_harvest_lobby::MAX_LOCAL_PLAYERS)).map(|k| if k == 0 || others.is_empty() { mine.clone() } else { others[(k - 1) % others.len()].clone() }).collect()
    }

    pub fn host_online(&mut self, code: Option<&str>) {
        let Some(relay) = self.relay_urls() else { return };
        self.online.lan_session = false;
        self.host_through(code, relay);
    }

    /// Hosts on this device's own relay, for players on its network: no internet and no ICE servers.
    pub fn host_lan(&mut self, code: Option<&str>) {
        let Some(port) = self.online.lan else {
            self.online.message = "Hosting on this network isn't available here.".into();
            self.sounds.push(Sfx::Miss);
            return;
        };
        (self.online.lan_wanted, self.online.lan_session) = (true, true);
        self.host_through(code, (format!("ws://127.0.0.1:{port}"), String::new()));
    }

    fn host_through(&mut self, code: Option<&str>, relay: (String, String)) {
        let code = code.and_then(|c| RoomCode::parse(c).ok()).or(self.online.fixed).unwrap_or_else(|| RoomCode::from_random(self.rng.next_u32() as u64 * 0x9E37 + self.rng.next_u32() as u64));
        self.online.fixed = Some(code);
        self.net = Some(Box::new(Net::host(code, self.identity(), self.local_cards(), relay)));
        self.online.message.clear();
        self.page = Page::Lobby;
    }

    pub fn join_online(&mut self, code: &str) {
        match RoomCode::parse(code) {
            Ok(code) => {
                let relay = match self.online.through.clone() {
                    Some(host) => (format!("ws://{host}"), String::new()),
                    None => {
                        let Some(relay) = self.relay_urls() else { return };
                        relay
                    }
                };
                self.online.lan_session = false;
                self.net = Some(Box::new(Net::join(code, self.identity(), self.local_cards(), relay)));
                self.online.message.clear();
                self.page = Page::Lobby;
            }
            Err(e) => {
                self.online.message = e.to_string();
                self.sounds.push(Sfx::Miss);
                if self.page == Page::Main {
                    self.page = Page::Online;
                }
            }
        }
    }

    pub fn net_request(&mut self) -> Option<Request> {
        let r = self.net.as_mut()?.take_request();
        if r == Some(Request::Close) {
            self.pump_net(0.0);
            self.net = None;
        }
        r
    }

    pub fn net_joined(&mut self, id: u128) {
        if let Some(n) = self.net.as_mut() {
            n.joined(id);
        }
    }

    pub fn net_failed(&mut self, why: &str) {
        if let Some(n) = self.net.as_mut() {
            n.failed(why);
        }
    }

    pub fn net_peer(&mut self, peer: u128, up: bool) {
        if let Some(n) = self.net.as_mut() {
            n.peer(peer, up);
        }
    }

    pub fn net_receive(&mut self, channel: usize, peer: u128, bytes: Vec<u8>) {
        if let Some(n) = self.net.as_mut() {
            n.receive(Packet { channel, peer, bytes });
        }
    }

    pub fn net_outgoing(&mut self) -> Vec<Packet> {
        self.net.as_mut().map(|n| n.take_outgoing()).unwrap_or_default()
    }

    pub fn net_traffic(&self) -> Option<Traffic> {
        self.net.as_ref().map(|n| n.traffic)
    }

    pub fn typed(&mut self, text: &str) {
        match self.page {
            Page::Join => {
                if self.online.code.typed(text, ALPHABET, last_harvest_lobby::CODE_LEN, true) {
                    let code = self.online.code.text.clone();
                    self.join_online(&code);
                }
            }
            Page::Relay => {
                if self.online.relay.typed(text, RELAY_KEYS, RELAY_MAX, false) {
                    self.close_relay();
                }
            }
            _ => {}
        }
    }

    pub fn zombie_spots(&self) -> Vec<(u32, [f32; 2])> {
        match &self.state {
            State::Harvest(h) => h.zombie_spots(),
            _ => Vec::new(),
        }
    }

    pub fn player_spots(&self) -> Vec<[f32; 2]> {
        match &self.state {
            State::Harvest(h) => h.player_spots(),
            _ => Vec::new(),
        }
    }

    pub(crate) fn pump_net(&mut self, dt: f32) {
        let Some(net) = self.net.as_mut() else { return };
        if !net.running {
            net.idle(dt);
        }
        for h in net.take_happenings() {
            match h {
                Happening::Start(start) if !self.in_run() || !self.online_run() => self.start_online(start),
                Happening::Start(_) => {}
                Happening::Seat(seat, card) => {
                    if let State::Harvest(h) = &mut self.state {
                        if h.is_host() {
                            h.seat(seat, &card);
                        } else if h.is_client() {
                            h.client_seat(seat, &card);
                        }
                    }
                }
                Happening::Unseat(seats) => {
                    if let State::Harvest(h) = &mut self.state
                        && h.is_host()
                    {
                        seats.into_iter().for_each(|s| h.unseat(s));
                    }
                }
                Happening::End(why) => {
                    if self.online_run() {
                        self.keep_replays();
                        self.state = State::Title;
                    }
                    if matches!(self.page, Page::Lobby | Page::Main) || self.online_run() {
                        self.page = Page::Online;
                    }
                    self.online.message = why;
                }
            }
        }
    }

    /// Keeps an online run's connection alive while this device is away, its lid shut.
    pub(crate) fn online_away(&mut self, dt: f32) {
        if !self.online_run() {
            return;
        }
        self.pump_net(dt);
        let mut state = std::mem::replace(&mut self.state, State::Title);
        if let (State::Harvest(h), Some(net)) = (&mut state, self.net.as_mut().filter(|n| n.running)) {
            net.step_away(h, dt, &mut Vec::new());
        }
        self.state = state;
    }

    /// BACK leaves online play, with a word to the room: the host closes it, a client leaves it.
    pub(crate) fn leave_online(&mut self, in_run: bool) {
        let Some(net) = self.net.as_mut().filter(|n| !n.leaving()) else { return };
        net.leave();
        self.online.message = if !in_run { String::new() } else if net.is_host() { "You closed the room.".into() } else { "You left the night.".into() };
        self.page = Page::Online;
    }

    /// Leaves online play as BACK does, for a shell about to close.
    pub fn close_online(&mut self) {
        let run = self.online_run();
        self.leave_online(run);
    }

    pub(crate) fn online_run(&self) -> bool {
        matches!(&self.state, State::Harvest(h) if h.is_host() || h.is_client())
    }

    fn start_online(&mut self, start: Start) {
        let Some(net) = self.net.as_mut() else { return };
        let mine = net.my_seats(&start);
        let host = net.is_host();
        if let Some(why) = start_refused(&start, &mine) {
            net.leave();
            self.online.message = why.into();
            self.page = Page::Online;
            return;
        }
        let card = |seat: usize| start.players.iter().find(|p| p.seat as usize == seat).map_or("", |p| p.card.as_str()).to_string();
        let look = start.options.iter().find(|o| o.0 == "look").map_or_else(|| card(0), |o| o.1.clone());
        harvest::use_farm(None);
        harvest::use_player_farm(&look);
        let seats: Vec<usize> = start.players.iter().map(|p| p.seat as usize).collect();
        let n = seats.iter().max().map_or(1, |m| m + 1);
        let difficulty = Difficulty::ALL.get(start.difficulty as usize).copied().unwrap_or_default();
        let looks: Vec<(usize, String)> = seats.iter().map(|&s| (s, card(s))).collect();
        let h = if host {
            let kind = replay::Kind::Host { looks, local: mine.len() };
            let setup = replay::Setup { map: start.map.clone(), difficulty, progress: self.progress(), look: look.clone(), farm: Some(look), one_screen: self.one_screen(), players: n, kind, ..replay::Setup::night(start.seed) };
            let mut h = setup.build();
            if let Some(info) = &self.replay_info {
                h.record(info.clone());
            }
            h
        } else {
            Harvest::on(start.seed, &start.map).at(difficulty).single_screen(self.one_screen()).with_players(n).client_of(&mine, &seats).dressed(&looks).with_views(mine.len())
        };
        self.keep_replays();
        self.sounds.push(Sfx::Start);
        self.state = State::Harvest(Box::new(h));
        self.page = Page::Main;
    }

    pub(crate) fn step_online(&mut self, h: &mut Harvest, input: &Input, others: &[Input], dt: f32) -> Option<harvest::Flow> {
        let net = self.net.as_mut().filter(|n| n.running && (h.is_host() || h.is_client()))?;
        let flow = net.step(h, input, others, dt, &mut self.sounds);
        if flow.quit {
            net.leave();
            self.online.message = if net.is_host() { "You closed the room.".into() } else { "You left the night.".into() };
            self.page = Page::Online;
        }
        Some(flow)
    }

    pub(crate) fn hold_online(&mut self, h: &Harvest) {
        if let Some(net) = self.net.as_mut().filter(|n| n.running) {
            net.hold(h);
        }
    }

    pub(crate) fn online_page(&mut self, input: &Input, tapped: Option<[f32; 2]>) {
        let back = input.pressed.contains(Buttons::B);
        let all = self.online_rows();
        let rows = all.len();
        self.online.row = self.online.row.min(rows - 1);
        let step = input.pressed.contains(Buttons::DOWN) as i32 - input.pressed.contains(Buttons::UP) as i32;
        if step != 0 {
            self.online.row = (self.online.row as i32 + step).rem_euclid(rows as i32) as usize;
            self.sounds.push(Sfx::Tick(self.online.row as u8 * 3));
            return;
        }
        let chosen = match tapped {
            Some(p) => (0..rows).find(|&i| inside(p, online_row(i, rows))),
            None if back => Some(rows - 1),
            None => input.pressed.any(Buttons::A | Buttons::START).then_some(self.online.row),
        };
        match chosen.map(|i| all[i]) {
            Some(Row::Host | Row::HostLan | Row::Join) if !self.online.available => {
                self.online.message = "Online play isn't available on this device yet.".into();
                self.sounds.push(Sfx::Miss);
            }
            Some(Row::Host) => {
                self.sounds.push(Sfx::Tick(9));
                self.host_online(None);
            }
            Some(Row::HostLan) => {
                self.sounds.push(Sfx::Tick(9));
                self.host_lan(None);
            }
            Some(Row::Join) => {
                self.page = Page::Join;
                self.online.code = Entry::default();
                (self.online.through, self.online.through_picked) = (self.online.nearby.first().cloned(), false);
                self.sounds.push(Sfx::Tick(9));
            }
            Some(Row::Relay) => self.open_relay(Page::Online),
            Some(Row::Back) => {
                self.page = Page::Main;
                self.sounds.push(Sfx::Tick(0));
            }
            None => {}
        }
    }

    pub(crate) fn open_relay(&mut self, from: Page) {
        self.online.relay = Entry { text: self.save.relay.as_str().to_string(), at: 0 };
        self.online.relay_from = from;
        self.page = Page::Relay;
        self.sounds.push(Sfx::Tick(9));
    }

    fn close_relay(&mut self) {
        self.save.relay = crate::save::Relay::new(&self.online.relay.text);
        self.online.message = crate::netplay::relay_urls(self.save.relay.as_str(), self.online.secure_page).err().unwrap_or_default();
        self.page = self.online.relay_from;
        self.sounds.push(Sfx::Tick(6));
    }

    pub(crate) fn join_page(&mut self, input: &Input, tapped: Option<[f32; 2]>) {
        if !self.online.nearby.is_empty() && (input.pressed.contains(Buttons::Y) || tapped.is_some_and(|p| inside(p, THROUGH))) {
            self.next_through();
            return;
        }
        match self.online.code.input(input, tapped, ALPHABET, 7, last_harvest_lobby::CODE_LEN, &mut self.sounds) {
            Typed::Done => {
                let code = self.online.code.text.clone();
                self.join_online(&code);
            }
            Typed::Back => {
                self.page = Page::Online;
                self.sounds.push(Sfx::Tick(0));
            }
            Typed::Nothing => {}
        }
    }

    /// The next of the nearby hosts, then the relay setting.
    fn next_through(&mut self) {
        let nearby = &self.online.nearby;
        let at = self.online.through.as_ref().and_then(|t| nearby.iter().position(|h| h == t));
        self.online.through = match at {
            Some(i) => nearby.get(i + 1).cloned(),
            None => nearby.first().cloned(),
        };
        self.online.through_picked = true;
        self.sounds.push(Sfx::Tick(6));
    }

    pub(crate) fn relay_page(&mut self, input: &Input, tapped: Option<[f32; 2]>) {
        match self.online.relay.input(input, tapped, RELAY_KEYS, 10, RELAY_MAX, &mut self.sounds) {
            Typed::Done => self.close_relay(),
            Typed::Back => {
                self.page = self.online.relay_from;
                self.sounds.push(Sfx::Tick(0));
            }
            Typed::Nothing => {}
        }
    }

    pub(crate) fn lobby_page(&mut self, input: &Input, tapped: Option<[f32; 2]>) {
        let Some(net) = self.net.as_mut() else {
            self.page = Page::Online;
            return;
        };
        let button = tapped.and_then(|p| BUTTONS.iter().position(|r| inside(p, *r)));
        let (_, ready) = net.roster();
        if input.pressed.contains(Buttons::B) || button == Some(0) {
            net.leave();
            self.page = Page::Online;
            self.online.message.clear();
            self.sounds.push(Sfx::Tick(0));
        } else if input.pressed.contains(Buttons::A) || button == Some(1) {
            net.set_ready(!ready);
            self.sounds.push(Sfx::Tick(if ready { 0 } else { 9 }));
        } else if net.is_host() && (input.pressed.any(Buttons::START | Buttons::X) || button == Some(2)) {
            let settings = Settings { seed: self.rng.next_u32() as u64 + 1, map: self.save.map.as_str().to_string(), difficulty: self.save.difficulty as u8, unlocked: Vec::new(), options: vec![("look".into(), self.save.player.as_str().to_string())] };
            match net.start(settings) {
                Ok(start) => self.start_online(start),
                Err(why) => {
                    self.online.message = why;
                    self.sounds.push(Sfx::Miss);
                }
            }
        }
    }

    pub(crate) fn draw_online(&self, c: &mut Canvas) {
        c.rect(0.0, 0.0, SCREEN_W, 768.0, [0.12, 0.12, 0.15, 1.0]);
        c.label("Online", SCREEN_W / 2.0, 24.0, 56.0, WHITE, Align::Center);
        let all = self.online_rows();
        for (i, row) in all.iter().enumerate() {
            let [x, y, w, h] = online_row(i, all.len());
            let on = i == self.online.row;
            let off = matches!(row, Row::Host | Row::HostLan | Row::Join) && !self.online.available;
            c.rect(x, y, w, h, if on { [1.0, 0.85, 0.3, 0.25] } else { [1.0, 1.0, 1.0, 0.06] });
            let color = if off { DIM } else if on { HOT } else { WHITE };
            let side = match row {
                Row::Relay => {
                    let relay = self.relay();
                    Some(if relay.trim().is_empty() { crate::netplay::DEFAULT_RELAY.trim_start_matches("wss://").to_string() } else { relay })
                }
                Row::Join => self.online.nearby.first().map(|h| format!("Nearby: {}", h.rsplit_once(':').map_or(h.as_str(), |(a, _)| a))),
                _ => None,
            };
            match side {
                Some(shown) => {
                    c.label(row.name(), x + 30.0, y + h / 2.0 - 22.0, 44.0, color, Align::Left);
                    let left = if *row == Row::Join { 420.0 } else { 260.0 };
                    fit_label(c, &shown, x + w - 30.0, y + h / 2.0 - 18.0, 36.0, w - left, if on { HOT } else { DIM }, Align::Right);
                }
                None => c.label(row.name(), x + w / 2.0, y + h / 2.0 - 24.0, 48.0, color, Align::Center),
            }
        }
        self.draw_message(c, 560.0);
    }

    fn draw_message(&self, c: &mut Canvas, y: f32) {
        if !self.online.message.is_empty() {
            wrap_label(c, &self.online.message, SCREEN_W / 2.0, y, 30.0, SCREEN_W - 80.0, 3, RED, Align::Center);
        }
    }

    pub(crate) fn draw_join(&self, c: &mut Canvas) {
        c.rect(0.0, 0.0, SCREEN_W, 768.0, [0.12, 0.12, 0.15, 1.0]);
        c.label("Join with a code", SCREEN_W / 2.0, 20.0, 48.0, WHITE, Align::Center);
        let n = last_harvest_lobby::CODE_LEN;
        for i in 0..n {
            let x = SCREEN_W / 2.0 + (i as f32 - n as f32 / 2.0) * 110.0 + 5.0;
            c.rect(x, 90.0, 100.0, 120.0, [1.0, 1.0, 1.0, 0.1]);
            if let Some(ch) = self.online.code.text.chars().nth(i) {
                c.label(ch.to_string(), x + 50.0, 110.0, 80.0, HOT, Align::Center);
            }
        }
        let (through, color) = match &self.online.through {
            Some(host) => (format!("Through nearby host {host}"), HOT),
            None => {
                let relay = self.relay();
                (format!("Through relay {}", if relay.trim().is_empty() { crate::netplay::DEFAULT_RELAY.trim_start_matches("wss://") } else { relay.as_str() }), DIM)
            }
        };
        let [x, y, w, _] = THROUGH;
        if self.online.nearby.is_empty() {
            fit_label(c, &through, x + w / 2.0, y, 26.0, w, color, Align::Center);
        } else {
            c.hint(&format!("{through}   {{Y}}: change"), x + w / 2.0, y, 26.0, color, Align::Center);
        }
        self.online.code.draw(c, ALPHABET, 7);
        if self.online.message.is_empty() {
            c.hint("{A}: type.   {B}: delete.   {START}: join.", SCREEN_W / 2.0, 720.0, 26.0, DIM, Align::Center);
        } else {
            self.draw_message(c, 700.0);
        }
    }

    pub(crate) fn draw_relay(&self, c: &mut Canvas) {
        c.rect(0.0, 0.0, SCREEN_W, 768.0, [0.12, 0.12, 0.15, 1.0]);
        c.label("Relay", SCREEN_W / 2.0, 20.0, 48.0, WHITE, Align::Center);
        let text = &self.online.relay.text;
        c.rect(40.0, 90.0, SCREEN_W - 80.0, 100.0, [1.0, 1.0, 1.0, 0.1]);
        let default = format!("Empty: {}", crate::netplay::DEFAULT_RELAY.trim_start_matches("wss://"));
        let shown = if text.is_empty() { default.as_str() } else { text.as_str() };
        fit_label(c, shown, SCREEN_W / 2.0, 120.0, 44.0, SCREEN_W - 120.0, if text.is_empty() { DIM } else { HOT }, Align::Center);
        if self.online.relay_env.is_some() {
            c.label("LAST_HARVEST_RELAY is set and wins over this", SCREEN_W / 2.0, 200.0, 26.0, DIM, Align::Center);
        }
        self.online.relay.draw(c, RELAY_KEYS, 10);
        self.draw_message(c, 700.0);
    }

    pub(crate) fn draw_lobby(&self, c: &mut Canvas) {
        c.rect(0.0, 0.0, SCREEN_W, 768.0, [0.12, 0.12, 0.15, 1.0]);
        let Some(net) = self.net.as_ref() else { return };
        c.label(format!("Room {}", net.code.as_str()), SCREEN_W / 2.0, 20.0, 64.0, HOT, Align::Center);
        let (members, ready) = net.roster();
        if let Some(status) = net.status() {
            c.label(status, SCREEN_W / 2.0, 110.0, 34.0, DIM, Align::Center);
        } else if net.is_host() {
            c.label("Share the code: players join it from Online", SCREEN_W / 2.0, 110.0, 30.0, DIM, Align::Center);
        }
        if self.online.lan_session && net.is_host() {
            let address = self.online.lan_address.as_deref().unwrap_or("this device's address");
            let port = self.online.lan.unwrap_or_default();
            fit_label(c, &format!("On this network: Join lists this host, or set the relay to {address}:{port}"), SCREEN_W / 2.0, 142.0, 24.0, SCREEN_W - 80.0, DIM, Align::Center);
        }
        let looks = crate::player_looks();
        let mut y = 170.0;
        for m in &members {
            for s in &m.seats {
                let name = looks.iter().find(|l| l.id == s.card).map_or("Farmer", |l| l.name.as_str());
                c.rect(40.0, y, SCREEN_W - 80.0, 92.0, [1.0, 1.0, 1.0, 0.07]);
                c.label(format!("P{}", s.seat + 1), 70.0, y + 24.0, 44.0, HOT, Align::Left);
                let who = if m.host { format!("{name}  (host)") } else { name.to_string() };
                fit_label(c, &who, 180.0, y + 26.0, 40.0, 520.0, WHITE, Align::Left);
                let (mark, color) = if m.ready { ("Ready", HOT) } else { ("Not ready", DIM) };
                c.label(mark, SCREEN_W - 70.0, y + 26.0, 40.0, color, Align::Right);
                y += 104.0;
            }
        }
        let all = !members.is_empty() && members.iter().all(|m| m.ready);
        let labels = ["Leave", if ready { "Not ready" } else { "Ready" }, "Start"];
        for (i, r) in BUTTONS.iter().enumerate() {
            if i == 2 && !net.is_host() {
                continue;
            }
            let lit = i == 1 || (i == 2 && all);
            c.rect(r[0], r[1], r[2], r[3], if lit { [1.0, 0.85, 0.3, 0.25] } else { [1.0, 1.0, 1.0, 0.08] });
            c.label(labels[i], r[0] + r[2] / 2.0, r[1] + 18.0, 42.0, if lit { HOT } else { WHITE }, Align::Center);
        }
        if self.online.message.is_empty() {
            let hint = if net.is_host() { "{A}: ready.   {START}: start once all are ready.   {B}: close the room." } else { "{A}: ready.   {B}: leave." };
            c.hint(hint, SCREEN_W / 2.0, 610.0, 26.0, DIM, Align::Center);
        } else {
            self.draw_message(c, 590.0);
        }
    }
}

/// Why a start can't be played here: no players, a seat past the room's, or none of them this device's.
fn start_refused(start: &Start, mine: &[usize]) -> Option<&'static str> {
    if start.players.is_empty() || start.players.iter().any(|p| p.seat >= last_harvest_lobby::MAX_SEATS) {
        Some("The host's start named no seats this game can play.")
    } else if mine.is_empty() {
        Some("The host started without a seat for this game.")
    } else {
        None
    }
}

/// Row `i` of `n` on the Online page; five rows sit closer to clear the message under them.
fn online_row(i: usize, n: usize) -> [f32; 4] {
    let (top, h) = if n > 4 { (110.0, 88.0) } else { (150.0, 96.0) };
    [112.0, top + i as f32 * h, 800.0, h - 16.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_relay_is_the_default_one_and_the_typed_relay_is_saved() {
        let a = Input { pressed: Buttons::A, ..Input::default() };
        let mut g = Game::new(1);
        g.set_online(true);
        g.page = Page::Online;
        g.online_page(&a, None);
        assert_eq!(g.page, Page::Lobby, "hosting with no relay set uses the default");
        let Some(crate::netplay::Request::Connect { room, ice }) = g.net_request() else { panic!("no connect") };
        assert!(room.starts_with("wss://lh-relay.vps.pocketsquirrel.com/signal/"), "{room}");
        assert_eq!(ice, "https://lh-relay.vps.pocketsquirrel.com/ice");
        g.lobby_page(&Input { pressed: Buttons::B, ..Input::default() }, None);
        g.online.row = 2;
        g.online_page(&a, None);
        assert_eq!(g.page, Page::Relay);
        g.typed("192.168.1.5:8097\n");
        assert_eq!(g.page, Page::Online);
        assert_eq!(g.save().relay.as_str(), "192.168.1.5:8097");
        assert!(g.save().to_text().contains("relay=192.168.1.5:8097"), "and it goes in save.txt");
        assert_eq!(crate::Save::parse(&g.save().to_text()).relay.as_str(), "192.168.1.5:8097", "and reads back after a restart");
        g.online.row = 0;
        g.online_page(&a, None);
        assert_eq!(g.page, Page::Lobby, "with one set, Host opens the lobby");
        let Some(crate::netplay::Request::Connect { room, .. }) = g.net_request() else { panic!("no connect") };
        assert!(room.starts_with("ws://192.168.1.5:8097/signal/"), "{room}");
    }

    fn shown_text(g: &Game) -> Vec<String> {
        let frame = g.draw();
        [frame.top, frame.bottom]
            .iter()
            .flat_map(|c| c.shapes.iter())
            .filter_map(|s| match s {
                crate::draw::Shape::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn host_on_this_network_shows_only_where_the_platform_can_serve_a_relay() {
        let a = Input { pressed: Buttons::A, ..Input::default() };
        let mut g = Game::new(1);
        g.set_online(true);
        g.page = Page::Online;
        assert_eq!(g.online_rows(), [Row::Host, Row::Join, Row::Relay, Row::Back], "the web build never calls set_lan");
        assert!(!shown_text(&g).iter().any(|t| t == "Host on this network"));

        g.set_lan(Some(12525));
        g.set_lan_address(Some("192.168.1.5".into()));
        assert_eq!(g.online_rows(), [Row::Host, Row::HostLan, Row::Join, Row::Relay, Row::Back]);
        assert!(shown_text(&g).iter().any(|t| t == "Host on this network"));
        g.online.row = 1;
        g.online_page(&a, None);
        assert_eq!(g.page, Page::Lobby);
        assert_eq!(g.take_lan_host(), Some(12525), "the platform starts its relay");
        assert_eq!(g.take_lan_host(), None, "once");
        let Some(Request::Connect { room, ice }) = g.net_request() else { panic!("no connect") };
        assert!(room.starts_with("ws://127.0.0.1:12525/signal/"), "{room}");
        assert!(ice.is_empty(), "no ICE servers to fetch: {ice}");
        assert!(g.save().relay.as_str().is_empty(), "the relay setting is left alone");
        assert!(shown_text(&g).iter().any(|t| t.contains("192.168.1.5:12525")), "the lobby shows where others find it");

        g.lobby_page(&Input { pressed: Buttons::B, ..Input::default() }, None);
        g.online.row = 0;
        g.online_page(&a, None);
        assert!(!shown_text(&g).iter().any(|t| t.contains("192.168.1.5:12525")), "a room on the public relay shows no address");
    }

    #[test]
    fn a_nearby_host_is_joined_through_for_that_session_only() {
        let a = Input { pressed: Buttons::A, ..Input::default() };
        let mut g = Game::new(1);
        g.set_online(true);
        g.page = Page::Online;
        g.set_nearby(vec!["192.168.1.7:12525".into()]);
        assert!(shown_text(&g).iter().any(|t| t == "Nearby: 192.168.1.7"));
        g.online.row = 1;
        g.online_page(&a, None);
        assert_eq!((g.page, g.online.through.as_deref()), (Page::Join, Some("192.168.1.7:12525")));
        assert!(shown_text(&g).iter().any(|t| t.contains("Through nearby host 192.168.1.7:12525")));
        g.typed("ACDEF\n");
        assert_eq!(g.page, Page::Lobby);
        let Some(Request::Connect { room, ice }) = g.net_request() else { panic!("no connect") };
        assert!(room.starts_with("ws://192.168.1.7:12525/signal/"), "{room}");
        assert!(ice.is_empty());
        assert!(g.save().relay.as_str().is_empty(), "the saved relay is not overwritten");

        g.lobby_page(&Input { pressed: Buttons::B, ..Input::default() }, None);
        let _ = g.net_request();
        g.online_page(&a, None);
        g.join_page(&Input { pressed: Buttons::Y, ..Input::default() }, None);
        assert_eq!(g.online.through, None, "{{Y}} moves on to the relay setting");
        g.set_nearby(vec!["192.168.1.7:12525".into()]);
        assert_eq!(g.online.through, None, "and a beacon does not undo the pick");
        g.typed("ACDEF\n");
        let Some(Request::Connect { room, .. }) = g.net_request() else { panic!("no connect") };
        assert!(room.starts_with("wss://lh-relay.vps.pocketsquirrel.com/signal/"), "{room}");
    }

    #[test]
    fn a_relay_without_a_scheme_is_ws_on_a_lan_and_wss_by_name() {
        use crate::netplay::relay_urls;
        let urls = |r: &str| relay_urls(r, false).unwrap();
        assert_eq!(urls(""), ("wss://lh-relay.vps.pocketsquirrel.com".into(), "https://lh-relay.vps.pocketsquirrel.com/ice".into()));
        assert_eq!(urls("192.168.1.5:8097").0, "ws://192.168.1.5:8097");
        assert_eq!(urls("localhost:8080/").1, "http://localhost:8080/ice");
        assert_eq!(urls("rgds:8097").0, "ws://rgds:8097", "a name with no dot is on the LAN");
        assert_eq!(urls("mac.local:8097").0, "ws://mac.local:8097");
        assert_eq!(urls("[::1]:8097").0, "ws://[::1]:8097");
        assert_eq!(urls("relay.example.com").0, "wss://relay.example.com");
        assert_eq!(urls("relay.example.com:8443").1, "https://relay.example.com:8443/ice");
        assert_eq!(urls("WS://relay.example.com").0, "ws://relay.example.com", "a typed scheme wins, in any case");
        assert_eq!(urls("https://relay.example.com").0, "wss://relay.example.com");
    }

    #[test]
    fn an_https_page_refuses_a_plain_relay_but_this_machines() {
        use crate::netplay::relay_urls;
        assert_eq!(relay_urls("", true).unwrap().0, "wss://lh-relay.vps.pocketsquirrel.com");
        assert_eq!(relay_urls("relay.example.com", true).unwrap().0, "wss://relay.example.com");
        assert_eq!(relay_urls("localhost:8101", true).unwrap().0, "ws://localhost:8101", "browsers let a secure page reach loopback");
        assert_eq!(relay_urls("ws://127.0.0.1:8101", true).unwrap().0, "ws://127.0.0.1:8101");
        for r in ["192.168.1.5:8097", "ws://relay.example.com", "http://192.168.1.5:8097", "rgds:8097"] {
            assert_eq!(relay_urls(r, true), Err(crate::netplay::INSECURE_RELAY.to_string()), "{r}");
        }
        let mut g = Game::new(1);
        g.set_online(true);
        g.set_secure_page(true);
        g.set_relay("192.168.1.5:8097");
        g.join_online("ACDEF");
        assert_eq!(g.page, Page::Online, "a join at launch shows why it can't");
        assert!(g.online.message.contains("secure"), "{}", g.online.message);
        assert!(g.net.is_none());
    }

    /// Taps the shown menu screen of a phone at `at`, in that screen's own pixels.
    fn tap(g: &mut Game, size: [f32; 2], [x, y]: [f32; 2]) {
        let phone = g.draw().phone.expect("a phone frame");
        let [rx, ry, rw, rh] = phone.bottom.or(phone.top).expect("a screen");
        let mut touches = [None; crate::MAX_TOUCHES];
        touches[0] = Some([rx + x / SCREEN_W * rw, ry + y / crate::SCREEN_H * rh]);
        assert!(touches[0].is_some_and(|[x, y]| x < size[0] && y < size[1]));
        g.update(&Input { touches, ..Input::default() }, 0.016);
        g.update(&Input::default(), 0.016);
    }

    fn middle([x, y, w, h]: [f32; 4]) -> [f32; 2] {
        [x + w / 2.0, y + h / 2.0]
    }

    fn type_by_touch(g: &mut Game, size: [f32; 2], text: &str, chars: &[u8], cols: usize) {
        let all = keys(chars, cols);
        for ch in text.bytes() {
            let (r, _) = all.iter().find(|(_, k)| matches!(k, Key::Char(c) if *c == ch)).expect("a key for each character");
            tap(g, size, middle(*r));
        }
        let (done, _) = all.iter().find(|(_, k)| matches!(k, Key::Done)).unwrap();
        tap(g, size, middle(*done));
    }

    #[test]
    fn close_online_lets_go_of_the_room_as_back_does() {
        let mut g = Game::new(1);
        g.set_online(true);
        g.set_relay("127.0.0.1:9");
        g.host_online(None);
        g.close_online();
        assert_eq!(g.page, Page::Online);
        let mut closed = false;
        for _ in 0..600 {
            g.update(&Input::default(), 0.016);
            while let Some(r) = g.net_request() {
                closed |= r == Request::Close;
            }
        }
        assert!(closed && g.net_traffic().is_none(), "the shell is asked to close the socket");
    }

    #[test]
    fn a_touch_only_phone_reaches_online_types_the_relay_and_joins_a_room_by_taps() {
        let size = [2400.0, 1080.0];
        let mut g = Game::new(1);
        g.set_phone(Some(size));
        g.set_touch(true);
        g.set_online(true);
        let phone = g.draw().phone.unwrap();
        let (rect, canvas) = if phone.bottom.is_some() { (phone.bottom, g.draw().bottom) } else { (phone.top, g.draw().top) };
        let [rx, ry, rw, rh] = rect.unwrap();
        let at = canvas.shapes.iter().find_map(|s| match s {
            crate::draw::Shape::Text { text, x, y, size, .. } if text == "Online" => Some([*x, y + size / 2.0]),
            _ => None,
        });
        let [x, y] = at.expect("Online on the title menu");
        let mut touches = [None; crate::MAX_TOUCHES];
        touches[0] = Some([rx + x / SCREEN_W * rw, ry + y / crate::SCREEN_H * rh]);
        g.update(&Input { touches, ..Input::default() }, 0.016);
        g.update(&Input::default(), 0.016);
        assert_eq!(g.page, Page::Online, "a tap on Online opens it");

        tap(&mut g, size, middle(online_row(2, 4)));
        assert_eq!(g.page, Page::Relay, "a tap on Relay opens its keys");
        type_by_touch(&mut g, size, "10.0.0.2:8097", RELAY_KEYS, 10);
        assert_eq!((g.page, g.save().relay.as_str()), (Page::Online, "10.0.0.2:8097"));

        tap(&mut g, size, middle(online_row(1, 4)));
        assert_eq!(g.page, Page::Join);
        let code = RoomCode::from_random(7);
        type_by_touch(&mut g, size, code.as_str(), ALPHABET, 7);
        assert_eq!(g.page, Page::Lobby, "Done joins room {}", code.as_str());
        assert!(g.net_traffic().is_some());
        tap(&mut g, size, middle(BUTTONS[0]));
        assert_eq!(g.page, Page::Online, "a tap on Leave leaves the lobby");
    }

    #[test]
    fn every_key_on_both_keyboards_is_reached_from_its_neighbours_in_all_four_directions() {
        for (chars, cols) in [(&ALPHABET[..], 7), (RELAY_KEYS, 10)] {
            let (n, len) = (keys(chars, cols).len(), chars.len());
            assert_eq!(step_key(n - 2, 1, 0, len, cols), n - 1, "Right from Delete is Done");
            assert_eq!(step_key(n - 1, 1, 0, len, cols), n - 2, "and wraps back to Delete");
            assert_eq!(step_key(n - 1, -1, 0, len, cols), n - 2, "Left from Done is Delete");
            for at in 0..n {
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let to = step_key(at, dx, dy, len, cols);
                    assert!(to < n, "{at} steps ({dx}, {dy}) off the keys to {to}");
                    if dx != 0 {
                        assert_eq!(step_key(to, -dx, 0, len, cols), at, "left and right undo each other");
                    }
                }
            }
            let mut seen = vec![false; n];
            let mut todo = vec![0];
            while let Some(at) = todo.pop() {
                if !std::mem::replace(&mut seen[at], true) {
                    todo.extend([(1, 0), (-1, 0), (0, 1), (0, -1)].map(|(dx, dy)| step_key(at, dx, dy, len, cols)));
                }
            }
            assert!(seen.iter().all(|&s| s), "every key is reached: {seen:?}");
        }
    }
}
