//! The title's Online pages: host or join a room, the lobby with its ready marks, and typing a room code or the
//! relay's address with a pad, a touch screen or a keyboard.

use last_harvest_lobby::{ALPHABET, Identity, RoomCode, Settings, Start};

use crate::draw::{Align, Canvas};
use crate::harvest::{self, Harvest};
use crate::input::{Buttons, Input};
use crate::menu::{Page, fit_label, wrap_label};
use crate::netplay::{Happening, Net, Packet, Request, Traffic};
use crate::{DIM, Difficulty, Game, HOT, SCREEN_W, Sfx, State, WHITE};

const RED: [f32; 4] = [1.0, 0.55, 0.5, 1.0];
const RELAY_KEYS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789.:-/";
const RELAY_MAX: usize = crate::save::RELAY_MAX;
const ONLINE_ROWS: [&str; 4] = ["Host a night", "Join with a code", "Relay", "Back"];
const ROW_TOP: f32 = 150.0;
const ROW_H: f32 = 96.0;
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
            let rows = chars.len().div_ceil(cols) as i32;
            let (mut col, mut row) = if self.at < chars.len() { ((self.at % cols) as i32, (self.at / cols) as i32) } else { (if self.at == n - 1 { cols as i32 - 1 } else { 0 }, rows) };
            col = (col + dx).rem_euclid(cols as i32);
            row = (row + dy).rem_euclid(rows + 1);
            self.at = if row == rows { if col * 2 < cols as i32 { n - 2 } else { n - 1 } } else { ((row * cols as i32 + col) as usize).min(chars.len() - 1) };
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

    pub fn set_relay(&mut self, relay: &str) {
        self.online.relay_env = Some(relay.to_string());
    }

    pub fn set_identity(&mut self, version: &str, pak: u64) {
        (self.online.version, self.online.pak) = (version.to_string(), pak);
    }

    fn relay(&self) -> String {
        self.online.relay_env.clone().unwrap_or_else(|| self.save.relay.as_str().to_string())
    }

    fn identity(&self) -> Identity {
        let version = if self.online.version.is_empty() { "dev".to_string() } else { self.online.version.clone() };
        let pak = if self.online.pak != 0 { self.online.pak } else { last_harvest_lobby::fingerprint(crate::data::load_report().versions().as_bytes()) };
        Identity { game_version: version, pak, mods: Vec::new() }
    }

    fn local_cards(&self) -> Vec<String> {
        let mine = self.save.player.as_str().to_string();
        let others: Vec<String> = crate::player_looks().iter().map(|l| l.id.clone()).filter(|id| *id != mine).collect();
        (0..self.players.clamp(1, last_harvest_lobby::MAX_LOCAL_PLAYERS)).map(|k| if k == 0 || others.is_empty() { mine.clone() } else { others[(k - 1) % others.len()].clone() }).collect()
    }

    pub fn host_online(&mut self, code: Option<&str>) {
        let code = code.and_then(|c| RoomCode::parse(c).ok()).or(self.online.fixed).unwrap_or_else(|| RoomCode::from_random(self.rng.next_u32() as u64 * 0x9E37 + self.rng.next_u32() as u64));
        self.online.fixed = Some(code);
        self.net = Some(Box::new(Net::host(code, self.identity(), self.local_cards(), &self.relay())));
        self.online.message.clear();
        self.page = Page::Lobby;
    }

    pub fn join_online(&mut self, code: &str) {
        match RoomCode::parse(code) {
            Ok(code) => {
                self.net = Some(Box::new(Net::join(code, self.identity(), self.local_cards(), &self.relay())));
                self.online.message.clear();
                self.page = Page::Lobby;
            }
            Err(e) => {
                self.online.message = e.to_string();
                self.sounds.push(Sfx::Miss);
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
        let mut h = Harvest::on(start.seed, &start.map).at(difficulty).single_screen(self.one_screen()).with_players(n);
        let looks: Vec<(usize, String)> = seats.iter().map(|&s| (s, card(s))).collect();
        h = if host { h.with_progress(self.progress()).dressed(&looks).hosting(mine.len()) } else { h.client_of(&mine, &seats).dressed(&looks).with_views(mine.len()) };
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
        let rows = ONLINE_ROWS.len();
        let step = input.pressed.contains(Buttons::DOWN) as i32 - input.pressed.contains(Buttons::UP) as i32;
        if step != 0 {
            self.online.row = (self.online.row as i32 + step).rem_euclid(rows as i32) as usize;
            self.sounds.push(Sfx::Tick(self.online.row as u8 * 3));
            return;
        }
        let chosen = match tapped {
            Some(p) => (0..rows).find(|&i| inside(p, online_row(i))),
            None if back => Some(rows - 1),
            None => input.pressed.any(Buttons::A | Buttons::START).then_some(self.online.row),
        };
        match chosen {
            Some(0 | 1) if !self.online.available => {
                self.online.message = "Online play isn't available on this device yet.".into();
                self.sounds.push(Sfx::Miss);
            }
            Some(0) => {
                self.sounds.push(Sfx::Tick(9));
                self.host_online(None);
            }
            Some(1) => {
                self.page = Page::Join;
                self.online.code = Entry::default();
                self.sounds.push(Sfx::Tick(9));
            }
            Some(2) => self.open_relay(Page::Online),
            Some(_) => {
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
        self.page = self.online.relay_from;
        self.sounds.push(Sfx::Tick(6));
    }

    pub(crate) fn join_page(&mut self, input: &Input, tapped: Option<[f32; 2]>) {
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
        for (i, name) in ONLINE_ROWS.iter().enumerate() {
            let [x, y, w, h] = online_row(i);
            let on = i == self.online.row;
            let off = i < 2 && !self.online.available;
            c.rect(x, y, w, h, if on { [1.0, 0.85, 0.3, 0.25] } else { [1.0, 1.0, 1.0, 0.06] });
            let color = if off { DIM } else if on { HOT } else { WHITE };
            if i == 2 {
                let relay = self.relay();
                let shown = if relay.trim().is_empty() { "this machine".to_string() } else { relay };
                c.label("Relay", x + 30.0, y + h / 2.0 - 22.0, 44.0, color, Align::Left);
                fit_label(c, &shown, x + w - 30.0, y + h / 2.0 - 18.0, 36.0, w - 260.0, if on { HOT } else { DIM }, Align::Right);
                continue;
            }
            c.label(*name, x + w / 2.0, y + h / 2.0 - 24.0, 48.0, color, Align::Center);
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
        let shown = if text.is_empty() { "Empty: a relay on this machine" } else { text.as_str() };
        fit_label(c, shown, SCREEN_W / 2.0, 120.0, 44.0, SCREEN_W - 120.0, if text.is_empty() { DIM } else { HOT }, Align::Center);
        if self.online.relay_env.is_some() {
            c.label("LAST_HARVEST_RELAY is set and wins over this", SCREEN_W / 2.0, 200.0, 26.0, DIM, Align::Center);
        }
        self.online.relay.draw(c, RELAY_KEYS, 10);
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

fn online_row(i: usize) -> [f32; 4] {
    [112.0, ROW_TOP + i as f32 * ROW_H, 800.0, ROW_H - 16.0]
}