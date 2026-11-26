//! Play's page: each seated player's card, the map and the difficulty, then Start.

use crate::draw::{Align, Canvas};
use crate::fmath as fm;
use crate::input::{Buttons, Input};
use crate::menu::{Page, fit_label, fitted, inside, wrap_label};
use crate::pad;
use crate::save::LookId;
use crate::{DIM, Game, HOT, SCREEN_W, Sfx, WHITE};

/// The rows Up and Down move between; the page opens on Start.
const PLAYER_ROW: usize = 0;
const MAP_ROW: usize = 1;
const DIFFICULTY_ROW: usize = 2;
const START_ROW: usize = 3;
const ROWS: usize = 4;

/// The seats' cards share this box, a row each.
const CARDS: [f32; 4] = [40.0, 76.0, SCREEN_W - 80.0, 304.0];
const MAP: [f32; 4] = [40.0, 392.0, SCREEN_W - 80.0, 72.0];
const DIFFICULTY: [f32; 4] = [40.0, 476.0, SCREEN_W - 80.0, 136.0];
const BACK: [f32; 4] = [40.0, 626.0, 280.0, 96.0];
const START: [f32; 4] = [340.0, 626.0, SCREEN_W - 380.0, 96.0];
/// How far in from a row's ends a tap steps back or on.
const ARROW_REACH: f32 = 140.0;
const OFF: [f32; 4] = [1.0, 1.0, 1.0, 0.15];

fn seat_row(seat: usize, seats: usize) -> [f32; 4] {
    let [x, y, w, h] = CARDS;
    let each = h / seats.max(1) as f32;
    [x, y + seat as f32 * each, w, each - 8.0]
}

/// Which way a tap on row `r` steps it: back near its left end, on near its right, else none.
fn arrow([x, _]: [f32; 2], [rx, _, w, _]: [f32; 4]) -> i32 {
    if x < rx + ARROW_REACH { -1 } else if x > rx + w - ARROW_REACH { 1 } else { 0 }
}

/// A frame of a card's walk, turning through its facings as Settings showed it.
fn walking(look: &crate::PlayerLook, t: f32) -> (crate::art::Art, bool) {
    let (facing, flip) = [(0, false), (1, false), (2, false), (1, true)][(t / 1.5) as usize % 4];
    let walk = &look.look.walk[facing];
    (walk[(t * 8.0) as usize % walk.len()], flip)
}

impl Game {
    /// The card `seat` plays as: player 1's from the save, the others' as picked here, or as a run dresses them.
    pub(crate) fn seat_card(&self, seat: usize) -> String {
        if seat == 0 {
            return self.save.player.as_str().to_string();
        }
        let looks = crate::player_looks();
        let mine = looks.iter().find(|l| l.id == self.save.player.as_str()).or(looks.first()).map_or("", |l| l.id.as_str()).to_string();
        if let Some(id) = self.seat_looks.get(seat).copied().flatten() {
            return id.as_str().to_string();
        }
        let others: Vec<&str> = crate::player_looks().iter().map(|l| l.id.as_str()).filter(|id| *id != mine).collect();
        if others.is_empty() { mine } else { others[(seat - 1) % others.len()].to_string() }
    }

    /// The first `n` seats' cards, player 1's first.
    pub(crate) fn seat_cards(&self, n: usize) -> Vec<String> {
        (0..n).map(|s| self.seat_card(s)).collect()
    }

    fn step_card(&mut self, seat: usize, by: i32) {
        let looks = crate::player_looks();
        if looks.len() < 2 {
            self.sounds.push(Sfx::Miss);
            return;
        }
        let at = looks.iter().position(|l| l.id == self.seat_card(seat)).unwrap_or(0) as i32;
        let next = (at + by).rem_euclid(looks.len() as i32) as usize;
        let id = LookId::new(&looks[next].id);
        match seat {
            0 => self.save.player = id,
            _ => self.seat_looks[seat] = Some(id),
        }
        self.sounds.push(Sfx::Tick(next as u8 * 3));
    }

    fn step_difficulty(&mut self, by: i32) {
        let next = self.save.difficulty.step(by);
        if next != self.save.difficulty {
            self.save.difficulty = next;
            self.sounds.push(Sfx::Tick(next.index() * 2));
        }
    }

    /// Play opens this page on Start, so a second A plays the night as last set.
    pub(crate) fn open_setup(&mut self) {
        self.page = Page::Setup;
        self.setup_row = START_ROW;
        self.sounds.push(Sfx::Tick(9));
    }

    /// Starts the night, after the how-to page on a save that hasn't seen it.
    fn begin_night(&mut self) {
        if self.how_to_due {
            self.page = Page::HowTo;
            self.how_to_then_play = true;
            self.sounds.push(Sfx::Tick(9));
            return;
        }
        self.page = Page::Main;
        self.open(crate::Scene::Harvest);
    }

    /// Up and Down choose a row, Left and Right change it, A or Start on Start plays, B goes back. Each other seated
    /// player's Left and Right change their own card.
    pub(crate) fn setup_page(&mut self, input: &Input, tapped: Option<[f32; 2]>) {
        let controls = self.save.controls;
        let theirs: Vec<i32> = self.others.iter().take(self.players.saturating_sub(1)).map(|i| {
            let pressed = controls.apply(i.pressed);
            pressed.contains(Buttons::RIGHT) as i32 - pressed.contains(Buttons::LEFT) as i32
        }).collect();
        for (k, by) in theirs.into_iter().enumerate().filter(|(_, by)| *by != 0) {
            self.step_card(k + 1, by);
        }
        if input.pressed.contains(Buttons::B) || tapped.is_some_and(|p| inside(p, BACK)) {
            self.page = Page::Main;
            self.sounds.push(Sfx::Tick(0));
            return;
        }
        if let Some(p) = tapped {
            return self.setup_tap(p);
        }
        if input.pressed.contains(Buttons::START) {
            return self.begin_night();
        }
        let step = input.pressed.contains(Buttons::DOWN) as i32 - input.pressed.contains(Buttons::UP) as i32;
        if step != 0 {
            self.setup_row = (self.setup_row as i32 + step).rem_euclid(ROWS as i32) as usize;
            self.sounds.push(Sfx::Tick(self.setup_row as u8 * 3));
            return;
        }
        let by = input.pressed.contains(Buttons::RIGHT) as i32 - input.pressed.contains(Buttons::LEFT) as i32;
        let a = input.pressed.contains(Buttons::A);
        match self.setup_row {
            PLAYER_ROW if by != 0 || a => self.step_card(0, if by == 0 { 1 } else { by }),
            MAP_ROW if by != 0 || a => self.step_map(if by == 0 { 1 } else { by }),
            DIFFICULTY_ROW if by != 0 => self.step_difficulty(by),
            START_ROW if a => self.begin_night(),
            _ => {}
        }
    }

    fn setup_tap(&mut self, p: [f32; 2]) {
        if inside(p, START) {
            self.setup_row = START_ROW;
            return self.begin_night();
        }
        let seats = self.players.max(1);
        if let Some(seat) = (0..seats).find(|&s| inside(p, seat_row(s, seats))) {
            if seat == 0 {
                self.setup_row = PLAYER_ROW;
            }
            let by = arrow(p, seat_row(seat, seats));
            return self.step_card(seat, if by == 0 { 1 } else { by });
        }
        if inside(p, MAP) {
            self.setup_row = MAP_ROW;
            let by = arrow(p, MAP);
            return self.step_map(if by == 0 { 1 } else { by });
        }
        if inside(p, DIFFICULTY) {
            self.setup_row = DIFFICULTY_ROW;
            match arrow(p, DIFFICULTY) {
                0 => self.sounds.push(Sfx::Tick(6)),
                by => self.step_difficulty(by),
            }
        }
    }

    /// The bottom screen, whole on its own for a one-screen device that shows nothing else.
    pub(crate) fn draw_setup(&self, c: &mut Canvas) {
        c.label("Tonight", SCREEN_W / 2.0, 16.0, 44.0, WHITE, Align::Center);
        let seats = self.players.max(1);
        for seat in 0..seats {
            self.draw_seat(c, seat, seats);
        }
        let shade = |on: bool| if on { [1.0, 0.85, 0.3, 0.25] } else { [1.0, 1.0, 1.0, 0.06] };
        let arrows = |c: &mut Canvas, [x, y, w, h]: [f32; 4], live: [bool; 2], on: bool| {
            let color = |l: bool| if !l { OFF } else if on { HOT } else { DIM };
            c.label("<", x + 40.0, y + h / 2.0 - 28.0, 56.0, color(live[0]), Align::Center);
            c.label(">", x + w - 40.0, y + h / 2.0 - 28.0, 56.0, color(live[1]), Align::Center);
        };
        let [x, y, w, h] = MAP;
        let on = self.setup_row == MAP_ROW;
        c.rect(x, y, w, h, shade(on));
        let maps = crate::data::maps();
        let name = maps.iter().find(|m| m.id == self.save.map.as_str()).map_or("Random farm", |m| m.name.as_str());
        arrows(c, MAP, [!maps.is_empty(); 2], on);
        fit_label(c, &format!("Map: {name}"), x + w / 2.0, y + h / 2.0 - 22.0, 44.0, w - 2.0 * ARROW_REACH, if on { HOT } else { WHITE }, Align::Center);
        let [x, y, w, h] = DIFFICULTY;
        let on = self.setup_row == DIFFICULTY_ROW;
        let d = self.save.difficulty;
        c.rect(x, y, w, h, shade(on));
        arrows(c, [x, y, w, 72.0], [!d.is_first(), !d.is_last()], on);
        fit_label(c, d.name(), x + w / 2.0, y + 12.0, 48.0, w - 2.0 * ARROW_REACH, HOT, Align::Center);
        wrap_label(c, d.blurb(), x + w / 2.0, y + 70.0, 26.0, w - 2.0 * ARROW_REACH, 2, if on { WHITE } else { DIM }, Align::Center);
        let [x, y, w, h] = BACK;
        c.rect(x, y, w, h, [1.0, 1.0, 1.0, 0.08]);
        c.label(pad::hint("{B}: Back"), x + w / 2.0, y + h / 2.0 - 20.0, 40.0, WHITE, Align::Center);
        let [x, y, w, h] = START;
        let on = self.setup_row == START_ROW;
        let pulse = 0.55 + 0.45 * fm::sin(self.clock * 3.0).abs();
        c.rect(x, y, w, h, if on { [1.0, 0.85, 0.3, 0.3] } else { [1.0, 1.0, 1.0, 0.08] });
        c.label("Start", x + w / 2.0, y + h / 2.0 - 26.0, 52.0, if on { [HOT[0], HOT[1], HOT[2], pulse] } else { WHITE }, Align::Center);
        let hint = if seats > 1 { "Up / Down: choose.   Left / Right: change.   Each player's Left / Right: their card." } else { "Up / Down: choose.   Left / Right: change.   {A} on Start: play." };
        c.hint(hint, SCREEN_W / 2.0, 732.0, 24.0, DIM, Align::Center);
    }

    /// Seat `seat`'s row: its card walking, its name and about line, and the arrows that change it.
    fn draw_seat(&self, c: &mut Canvas, seat: usize, seats: usize) {
        let looks = crate::player_looks();
        let id = self.seat_card(seat);
        let Some(p) = looks.iter().find(|l| l.id == id).or(looks.first()) else { return };
        let [x, y, w, h] = seat_row(seat, seats);
        let on = seat == 0 && self.setup_row == PLAYER_ROW;
        c.rect(x, y, w, h, if on { [1.0, 0.85, 0.3, 0.25] } else { [1.0, 1.0, 1.0, 0.06] });
        let arrow = if looks.len() < 2 { OFF } else if on || seat > 0 { HOT } else { DIM };
        let size = (h * 0.5).min(56.0);
        c.label("<", x + 40.0, y + h / 2.0 - size / 2.0, size, arrow, Align::Center);
        c.label(">", x + w - 40.0, y + h / 2.0 - size / 2.0, size, arrow, Align::Center);
        let mut left = x + 80.0;
        if seats > 1 {
            c.label(format!("P{}", seat + 1), left, y + h / 2.0 - 18.0, 36.0, HOT, Align::Left);
            left += 70.0;
        }
        let (frame, flip) = walking(p, self.clock + seat as f32 * 0.4);
        let tall = h - 16.0;
        let scale = tall / frame.h().max(frame.w());
        let box_w = tall.min(200.0);
        c.art(frame, left + box_w / 2.0 - frame.w() * scale / 2.0, y + h - 8.0 - frame.h() * scale, scale, flip, WHITE);
        let text_x = left + box_w + 30.0;
        let room = x + w - ARROW_REACH + 40.0 - text_x;
        let name_color = if on { HOT } else { WHITE };
        if seats == 1 {
            fit_label(c, &p.name, text_x, y + 20.0, 52.0, room, name_color, Align::Left);
            wrap_label(c, &p.about, text_x, y + 86.0, 28.0, room, 3, WHITE, Align::Left);
            let d = self.save.difficulty;
            let start = p.start_names(d.tier());
            let kit = if start.is_empty() { "no weapons".to_string() } else { start.join(", ") };
            fit_label(c, &format!("Starts with {kit}"), text_x, y + h - 44.0, 26.0, room, DIM, Align::Left);
        } else {
            let name = (h * 0.4).min(44.0);
            fit_label(c, &p.name, text_x, y + 6.0, name, room, name_color, Align::Left);
            if h > 120.0 {
                wrap_label(c, &p.about, text_x, y + 12.0 + name, 26.0, room, 2, DIM, Align::Left);
            } else {
                c.label(fitted(&p.about, 24.0, room, 24.0).0, text_x, y + 10.0 + name, 24.0, DIM, Align::Left);
            }
        }
    }

    /// The top screen: player 1's card large, as Settings showed it, or each seat's side by side.
    pub(crate) fn draw_setup_top(&self, c: &mut Canvas) {
        let seats = self.players.max(1);
        if seats == 1 {
            return self.draw_player(c);
        }
        let looks = crate::player_looks();
        c.rect(0.0, 560.0, SCREEN_W, 208.0, [0.1, 0.08, 0.07, 1.0]);
        let col = SCREEN_W / seats as f32;
        for seat in 0..seats {
            let id = self.seat_card(seat);
            let Some(p) = looks.iter().find(|l| l.id == id).or(looks.first()) else { continue };
            let mid = col * (seat as f32 + 0.5);
            c.label(format!("P{}", seat + 1), mid, 40.0, 48.0, HOT, Align::Center);
            fit_label(c, &p.name, mid, 100.0, 40.0, col - 24.0, WHITE, Align::Center);
            wrap_label(c, &p.about, mid, 156.0, 24.0, col - 24.0, 3, DIM, Align::Center);
            let (frame, flip) = walking(p, self.clock + seat as f32 * 0.4);
            let scale = (col - 40.0).min(300.0) / frame.h().max(frame.w());
            c.art(frame, mid - frame.w() * scale / 2.0, 600.0 - frame.h() * scale, scale, flip, WHITE);
        }
    }
}
