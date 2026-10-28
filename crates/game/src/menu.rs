//! The title's bottom-screen menu and its pages; Settings also opens over a paused run.

use crate::draw::{Align, Canvas, Color, Frame};
use crate::harvest::{self, roster};
use crate::input::{Buttons, Input};
use crate::save::LEVELS;
use crate::{DIM, Difficulty, Game, HOT, SCREEN_H, SCREEN_W, Sfx, WHITE};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Page {
    #[default]
    Main,
    Settings,
    Enemies,
    Stats,
    Credits,
}

const MAIN: [(Page, &str); 5] = [(Page::Main, "Play"), (Page::Main, "Map"), (Page::Settings, "Settings"), (Page::Stats, "Stats"), (Page::Credits, "Credits")];
/// The row that picks the map runs start on rather than opening a page.
const MAP_ROW: usize = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Setting {
    TopBrightness,
    BottomBrightness,
    Volume,
    Rumble,
    Fps,
    Player,
    Enemies,
    Reset,
}

impl Setting {
    const ALL: [Setting; 8] = [Setting::TopBrightness, Setting::BottomBrightness, Setting::Volume, Setting::Rumble, Setting::Fps, Setting::Player, Setting::Enemies, Setting::Reset];

    fn name(self) -> &'static str {
        match self {
            Setting::TopBrightness => "Top brightness",
            Setting::BottomBrightness => "Bottom brightness",
            Setting::Volume => "Volume",
            Setting::Rumble => "Rumble",
            Setting::Fps => "Show FPS",
            Setting::Player => "Player",
            Setting::Enemies => "Enemies",
            Setting::Reset => "Reset progress",
        }
    }
}

/// The Play button, then the difficulty box under it, as x, y, width and height.
const PLAY: [f32; 4] = [212.0, 30.0, 600.0, 100.0];
pub(crate) const DIFFICULTY_Y: f32 = 150.0;
/// The Map, Settings, Stats and Credits rows, below the difficulty box.
const ROWS_TOP: f32 = 316.0;
const ROW_H: f32 = 80.0;
/// The settings rows; a run's settings leave out Player, Enemies and Reset.
const SET_TOP: f32 = 96.0;
const SET_H: f32 = 68.0;
/// Enemy rows that fit above the Back button; the list scrolls past that.
const ENEMY_ROWS: usize = 8;
/// The arrows either side of a level bar, as x centres.
const LESS_X: f32 = 600.0;
const MORE_X: f32 = 940.0;
const BACK: [f32; 4] = [362.0, 660.0, 300.0, 76.0];
/// Seconds a first press on reset waits for the second.
pub(crate) const RESET_CONFIRM: f32 = 3.0;
pub(crate) const RESET_SHOWN: f32 = 3.0;

fn inside([x, y]: [f32; 2], [rx, ry, w, h]: [f32; 4]) -> bool {
    (rx..rx + w).contains(&x) && (ry..ry + h).contains(&y)
}

/// Which way a tap steps the difficulty, if it lands on one of its arrows.
pub(crate) fn difficulty_arrow([x, y]: [f32; 2]) -> Option<i32> {
    if !(DIFFICULTY_Y - 10.0..DIFFICULTY_Y + 130.0).contains(&y) {
        return None;
    }
    let off = x - SCREEN_W / 2.0;
    (off.abs() > 160.0 && off.abs() < 320.0).then_some(off.signum() as i32)
}

fn main_row(i: usize) -> [f32; 4] {
    if i == 0 { PLAY } else { [212.0, ROWS_TOP + (i - 1) as f32 * ROW_H, 600.0, ROW_H - 16.0] }
}

fn setting_row(i: usize) -> [f32; 4] {
    [40.0, SET_TOP + i as f32 * SET_H, SCREEN_W - 80.0, SET_H - 12.0]
}

impl Game {
    pub(crate) fn settings_list(&self) -> &'static [Setting] {
        if self.settings_in_run { &Setting::ALL[..5] } else { &Setting::ALL }
    }

    /// The title screen: Up and Down choose a row, Left and Right the difficulty, A or a tap opens.
    pub(crate) fn title(&mut self, input: &Input, dt: f32) {
        self.reset_armed = (self.reset_armed - dt).max(0.0);
        self.reset_shown = (self.reset_shown - dt).max(0.0);
        let tapped = input.touch.filter(|_| input.touch_started);
        let back = input.pressed.contains(Buttons::B) || tapped.is_some_and(|p| inside(p, BACK));
        match self.page {
            Page::Main => self.main_page(input, tapped),
            Page::Settings => {
                if self.settings(input, tapped) {
                    self.page = Page::Main;
                }
            }
            Page::Enemies => {
                if self.enemies_page(input, tapped) {
                    self.page = Page::Settings;
                }
            }
            Page::Stats | Page::Credits if back => {
                self.page = Page::Main;
                self.sounds.push(Sfx::Tick(0));
            }
            Page::Stats | Page::Credits => {}
        }
    }

    fn main_page(&mut self, input: &Input, tapped: Option<[f32; 2]>) {
        let step = input.pressed.contains(Buttons::DOWN) as i32 - input.pressed.contains(Buttons::UP) as i32;
        if step != 0 {
            self.row = (self.row as i32 + step).rem_euclid(MAIN.len() as i32) as usize;
            self.sounds.push(Sfx::Tick(self.row as u8 * 3));
            return;
        }
        let by = input.pressed.contains(Buttons::RIGHT) as i32 - input.pressed.contains(Buttons::LEFT) as i32;
        if by != 0 && self.row == MAP_ROW {
            return self.step_map(by);
        }
        let arrow = tapped.and_then(difficulty_arrow);
        if (by != 0 && self.row == 0) || arrow.is_some() {
            let next = self.save.difficulty.step(arrow.unwrap_or(by));
            if next != self.save.difficulty {
                self.save.difficulty = next;
                self.sounds.push(Sfx::Tick(next as u8 * 4));
            }
            return;
        }
        let chosen = match tapped {
            Some(p) => (0..MAIN.len()).find(|&i| inside(p, main_row(i))),
            None => input.pressed.any(Buttons::START | Buttons::A).then_some(self.row),
        };
        match chosen {
            Some(0) => self.open(crate::Scene::Harvest),
            Some(MAP_ROW) => {
                self.row = MAP_ROW;
                self.step_map(1);
            }
            Some(i) => {
                self.row = i;
                self.page = MAIN[i].0;
                if self.page == Page::Settings {
                    self.open_settings(false);
                }
                self.sounds.push(Sfx::Tick(9));
            }
            None => {}
        }
    }

    /// Moves the map runs start on `by` along Random and the hand-made maps.
    fn step_map(&mut self, by: i32) {
        let maps = crate::data::maps();
        let at = maps.iter().position(|m| m.id == self.save.map.as_str()).map_or(0, |i| i + 1) as i32;
        let next = (at + by).rem_euclid(maps.len() as i32 + 1) as usize;
        self.save.map = crate::save::LookId::new(next.checked_sub(1).map_or("", |i| maps[i].id.as_str()));
        self.sounds.push(Sfx::Tick(next as u8 * 3));
    }

    pub(crate) fn open_settings(&mut self, in_run: bool) {
        self.settings_in_run = in_run;
        self.setting = 0;
        self.reset_armed = 0.0;
    }

    /// Up and Down choose, Left and Right (or the arrows) change, A toggles; returns true once B leaves.
    pub(crate) fn settings(&mut self, input: &Input, tapped: Option<[f32; 2]>) -> bool {
        let list = self.settings_list();
        if input.pressed.any(Buttons::B | Buttons::START) || tapped.is_some_and(|p| inside(p, BACK)) {
            self.reset_armed = 0.0;
            self.sounds.push(Sfx::Tick(0));
            return true;
        }
        let step = input.pressed.contains(Buttons::DOWN) as i32 - input.pressed.contains(Buttons::UP) as i32;
        if step != 0 {
            self.setting = (self.setting as i32 + step).rem_euclid(list.len() as i32) as usize;
            self.reset_armed = 0.0;
            self.sounds.push(Sfx::Tick(self.setting as u8 * 2));
            return false;
        }
        let mut by = input.pressed.contains(Buttons::RIGHT) as i32 - input.pressed.contains(Buttons::LEFT) as i32;
        let mut press = input.pressed.contains(Buttons::A);
        if let Some(p) = tapped {
            let Some(i) = (0..list.len()).find(|&i| inside(p, setting_row(i))) else { return false };
            self.setting = i;
            match p[0] {
                x if (x - LESS_X).abs() < 60.0 => by = -1,
                x if (x - MORE_X).abs() < 60.0 => by = 1,
                _ => press = true,
            }
        }
        let level = |v: u8, lo: u8| v.saturating_add_signed(by as i8).clamp(lo, LEVELS);
        match list[self.setting] {
            Setting::TopBrightness if by != 0 => self.save.brightness[0] = level(self.save.brightness[0], 1),
            Setting::BottomBrightness if by != 0 => self.save.brightness[1] = level(self.save.brightness[1], 1),
            Setting::Volume if by != 0 => {
                self.save.volume = level(self.save.volume, 0);
                self.sounds.push(Sfx::Tick(self.save.volume));
                return false;
            }
            Setting::Rumble if by != 0 || press => {
                self.save.rumble = !self.save.rumble;
                if self.save.rumble {
                    self.rumble = 0.2;
                }
            }
            Setting::Fps if by != 0 || press => self.save.fps = !self.save.fps,
            Setting::Player if by != 0 || press => {
                let looks = crate::player_looks();
                let at = looks.iter().position(|l| l.id == self.save.player.as_str()).unwrap_or(0) as i32;
                let next = (at + if by == 0 { 1 } else { by }).rem_euclid(looks.len() as i32) as usize;
                self.save.player = crate::save::LookId::new(&looks[next].id);
            }
            Setting::Enemies if by > 0 || press => {
                self.page = Page::Enemies;
                self.enemy = 0;
                self.sounds.push(Sfx::Tick(9));
                return false;
            }
            Setting::Reset if press && self.reset_armed > 0.0 => {
                self.save = self.save.without_progress();
                self.reset_armed = 0.0;
                self.reset_shown = RESET_SHOWN;
                self.sounds.push(Sfx::Turn);
                return false;
            }
            Setting::Reset if press => self.reset_armed = RESET_CONFIRM,
            _ => return false,
        }
        self.sounds.push(Sfx::Tick(6));
        false
    }

    /// Up and Down choose a kind, A, Left, Right or a tap turns it on or off; returns true once B leaves.
    fn enemies_page(&mut self, input: &Input, tapped: Option<[f32; 2]>) -> bool {
        let count = roster().enemies.len();
        if input.pressed.any(Buttons::B | Buttons::START) || tapped.is_some_and(|p| inside(p, BACK)) {
            self.sounds.push(Sfx::Tick(0));
            return true;
        }
        let step = input.pressed.contains(Buttons::DOWN) as i32 - input.pressed.contains(Buttons::UP) as i32;
        if step != 0 {
            self.enemy = (self.enemy as i32 + step).rem_euclid(count as i32) as usize;
            self.sounds.push(Sfx::Tick(self.enemy as u8 * 2));
            return false;
        }
        let first = self.enemy.saturating_sub(ENEMY_ROWS - 1);
        let toggle = match tapped {
            Some(p) => (first..count.min(first + ENEMY_ROWS)).find(|&i| inside(p, setting_row(i - first))).inspect(|&i| self.enemy = i).is_some(),
            None => input.pressed.any(Buttons::A | Buttons::LEFT | Buttons::RIGHT),
        };
        if toggle {
            let flipped = self.save.enemies_off ^ 1 << self.enemy;
            if roster().all_off(flipped) {
                self.sounds.push(Sfx::Miss);
            } else {
                self.save.enemies_off = flipped;
                self.sounds.push(Sfx::Tick(6));
            }
        }
        false
    }

    pub(crate) fn draw_menu(&self, f: &mut Frame) {
        let c = &mut f.bottom;
        match self.page {
            Page::Main => self.draw_main(c),
            Page::Settings => self.draw_settings(c),
            Page::Enemies => self.draw_enemies(c),
            Page::Stats => self.draw_stats(c),
            Page::Credits => draw_credits(c),
        }
    }

    fn draw_main(&self, c: &mut Canvas) {
        let pulse = 0.55 + 0.45 * (self.clock * 3.0).sin().abs();
        for (i, (_, name)) in MAIN.iter().enumerate() {
            let [x, y, w, h] = main_row(i);
            let on = i == self.row;
            c.rect(x, y, w, h, if on { [1.0, 0.85, 0.3, 0.3] } else { [1.0, 1.0, 1.0, 0.08] });
            let color = if on { [HOT[0], HOT[1], HOT[2], if i == 0 { pulse } else { 1.0 }] } else { WHITE };
            if i == MAP_ROW {
                let maps = crate::data::maps();
                let name = maps.iter().find(|m| m.id == self.save.map.as_str()).map_or("Random farm", |m| m.name.as_str());
                let arrow = if maps.is_empty() { [1.0, 1.0, 1.0, 0.15] } else if on { HOT } else { DIM };
                c.label("<", x + 40.0, y + h / 2.0 - 26.0, 48.0, arrow, Align::Center);
                c.label(">", x + w - 40.0, y + h / 2.0 - 26.0, 48.0, arrow, Align::Center);
                fit_label(c, &format!("Map: {name}"), x + w / 2.0, y + h / 2.0 - 22.0, 44.0, w - 120.0, color, Align::Center);
                continue;
            }
            c.label(*name, x + w / 2.0, y + h / 2.0 - 26.0, 52.0, color, Align::Center);
        }
        let d = self.save.difficulty;
        let (y, mid) = (DIFFICULTY_Y, SCREEN_W / 2.0);
        c.rect(mid - 300.0, y - 10.0, 600.0, 150.0, [1.0, 1.0, 1.0, 0.06]);
        let arrow = |on: bool| if on { HOT } else { [1.0, 1.0, 1.0, 0.15] };
        c.label("<", mid - 250.0, y + 6.0, 56.0, arrow(d != Difficulty::Easy), Align::Center);
        c.label(">", mid + 250.0, y + 6.0, 56.0, arrow(d != Difficulty::Hard), Align::Center);
        c.label(d.name(), mid, y + 6.0, 52.0, HOT, Align::Center);
        c.label(d.blurb(), mid, y + 80.0, 26.0, DIM, Align::Center);
        c.label("D-pad: walk and strafe.   Y / A: turn.   B: turn around.   R1: screenshot.", SCREEN_W / 2.0, SCREEN_H - 84.0, 24.0, DIM, Align::Center);
        c.label("Hold X: aim at barrels.   Tap the map: throw.   Left / Right on Play: difficulty.", SCREEN_W / 2.0, SCREEN_H - 50.0, 24.0, DIM, Align::Center);
    }

    pub(crate) fn draw_settings(&self, c: &mut Canvas) {
        c.rect(0.0, 0.0, SCREEN_W, SCREEN_H, [0.12, 0.12, 0.15, 1.0]);
        c.label("Settings", SCREEN_W / 2.0, 24.0, 48.0, WHITE, Align::Center);
        for (i, &s) in self.settings_list().iter().enumerate() {
            let [x, y, w, h] = setting_row(i);
            let on = i == self.setting;
            c.rect(x, y, w, h, if on { [1.0, 0.85, 0.3, 0.25] } else { [1.0, 1.0, 1.0, 0.06] });
            let mid = y + h / 2.0;
            let armed = s == Setting::Reset && self.reset_armed > 0.0;
            let red = [1.0, 0.45, 0.4, 1.0];
            let name = if armed { "Press A again to erase" } else { s.name() };
            c.label(name, x + 30.0, mid - 20.0, 38.0, if armed { red } else if on { HOT } else { WHITE }, Align::Left);
            let (value, lo) = match s {
                Setting::TopBrightness => (self.save.brightness[0], 1),
                Setting::BottomBrightness => (self.save.brightness[1], 1),
                Setting::Volume => (self.save.volume, 0),
                Setting::Rumble | Setting::Fps => {
                    let v = if s == Setting::Rumble { self.save.rumble } else { self.save.fps };
                    c.label(if v { "On" } else { "Off" }, (LESS_X + MORE_X) / 2.0, mid - 20.0, 38.0, if v { HOT } else { DIM }, Align::Center);
                    continue;
                }
                Setting::Player => {
                    let looks = crate::player_looks();
                    let name = looks.iter().find(|l| l.id == self.save.player.as_str()).map_or("Farmer", |l| l.name.as_str());
                    let arrow = if looks.len() > 1 { HOT } else { [1.0, 1.0, 1.0, 0.15] };
                    c.label("<", LESS_X, mid - 24.0, 44.0, arrow, Align::Center);
                    c.label(">", MORE_X, mid - 24.0, 44.0, arrow, Align::Center);
                    fit_label(c, name, (LESS_X + MORE_X) / 2.0, mid - 20.0, 38.0, MORE_X - LESS_X - 80.0, if on { HOT } else { WHITE }, Align::Center);
                    continue;
                }
                Setting::Enemies => {
                    let r = roster();
                    let on = (0..r.enemies.len()).filter(|i| self.save.enemies_off & 1 << i == 0).count();
                    c.label(format!("{on} of {} on   >", r.enemies.len()), x + w - 30.0, mid - 20.0, 38.0, if on < r.enemies.len() { HOT } else { DIM }, Align::Right);
                    continue;
                }
                Setting::Reset => {
                    let hint = if armed { "Clears records, kills and found powerups" } else { "Settings stay" };
                    c.label(hint, x + w - 30.0, mid - 14.0, 24.0, if armed { red } else { DIM }, Align::Right);
                    continue;
                }
            };
            let arrow = |live: bool| if live { HOT } else { [1.0, 1.0, 1.0, 0.15] };
            c.label("<", LESS_X, mid - 24.0, 44.0, arrow(value > lo), Align::Center);
            c.label(">", MORE_X, mid - 24.0, 44.0, arrow(value < LEVELS), Align::Center);
            let (left, span) = (LESS_X + 40.0, MORE_X - LESS_X - 80.0);
            let cell = span / LEVELS as f32;
            for l in 0..LEVELS {
                let lit = l < value;
                c.rect(left + l as f32 * cell + 3.0, mid - 14.0, cell - 6.0, 28.0, if lit { HOT } else { [1.0, 1.0, 1.0, 0.12] });
            }
        }
        back_button(c);
    }

    fn draw_enemies(&self, c: &mut Canvas) {
        let r = roster();
        c.rect(0.0, 0.0, SCREEN_W, SCREEN_H, [0.12, 0.12, 0.15, 1.0]);
        c.label("Enemies", SCREEN_W / 2.0, 24.0, 48.0, WHITE, Align::Center);
        let first = self.enemy.saturating_sub(ENEMY_ROWS - 1);
        for (row, i) in (first..r.enemies.len().min(first + ENEMY_ROWS)).enumerate() {
            let [x, y, w, h] = setting_row(row);
            let (sel, on) = (i == self.enemy, self.save.enemies_off & 1 << i == 0);
            c.rect(x, y, w, h, if sel { [1.0, 0.85, 0.3, 0.25] } else { [1.0, 1.0, 1.0, 0.06] });
            let [cr, cg, cb, _] = self.mix_shade(i);
            c.rect(x, y, 10.0, h, [cr, cg, cb, if on { 1.0 } else { 0.25 }]);
            let e = &r.enemies[i];
            let icon = e.look.walk[0][0];
            let scale = (h - 16.0) / icon.h();
            c.art(icon, x + 20.0, y + 8.0, scale, false, if on { WHITE } else { [1.0, 1.0, 1.0, 0.3] });
            let left = x + 40.0 + icon.w() * scale;
            fit_label(c, &e.name, left, y + h / 2.0 - 20.0, 38.0, (LESS_X + MORE_X) / 2.0 - 60.0 - left, if sel { HOT } else if on { WHITE } else { DIM }, Align::Left);
            c.label(if on { "On" } else { "Off" }, (LESS_X + MORE_X) / 2.0, y + h / 2.0 - 20.0, 38.0, if on { HOT } else { DIM }, Align::Center);
            if e.from_card {
                c.label("Card", x + w - 24.0, y + h / 2.0 - 14.0, 26.0, DIM, Align::Right);
            }
        }
        back_button(c);
    }

    /// The player's chosen look walking on the top screen, turning through its facings, under its name.
    pub(crate) fn draw_player(&self, c: &mut Canvas) {
        let looks = crate::player_looks();
        let Some(p) = looks.iter().find(|l| l.id == self.save.player.as_str()).or(looks.first()) else { return };
        c.rect(0.0, 560.0, SCREEN_W, 208.0, [0.1, 0.08, 0.07, 1.0]);
        let (facing, flip) = [(0, false), (1, false), (2, false), (1, true)][(self.clock / 1.5) as usize % 4];
        let walk = &p.look.walk[facing];
        let frame = walk[(self.clock * 8.0) as usize % walk.len()];
        let scale = 300.0 / frame.h().max(frame.w());
        c.art(frame, SCREEN_W / 2.0 - frame.w() * scale / 2.0, 600.0 - frame.h() * scale, scale, flip, WHITE);
        fit_label(c, &p.name, SCREEN_W / 2.0, 60.0, 64.0, SCREEN_W - 80.0, HOT, Align::Center);
        let note = if looks.len() > 1 { format!("{} of {}: Left and Right to change", looks.iter().position(|l| l.id == p.id).unwrap_or(0) + 1, looks.len()) } else { "Add a .player.png to data/players for more".to_string() };
        c.label(note, SCREEN_W / 2.0, 150.0, 28.0, DIM, Align::Center);
        let d = self.save.difficulty;
        let start = p.start_names(d);
        let kit = if start.is_empty() { "no weapons".to_string() } else { start.join(", ") };
        fit_label(c, &format!("{} starts with {kit}", d.name()), SCREEN_W / 2.0, 196.0, 28.0, SCREEN_W - 80.0, WHITE, Align::Center);
    }

    /// Kind `i`'s colour in the spawn mix, and on its row of the list: `HOT` when highlighted.
    fn mix_shade(&self, i: usize) -> Color {
        const SHADES: [[f32; 4]; 4] = [[0.55, 0.62, 0.78, 1.0], [0.5, 0.75, 0.55, 1.0], [0.78, 0.5, 0.62, 1.0], [0.62, 0.55, 0.85, 1.0]];
        if i == self.enemy { HOT } else { SHADES[i % SHADES.len()] }
    }

    /// How a night's spawns split between the kinds left on, dusk to dawn across the foot of the top screen.
    fn draw_spawn_mix(&self, c: &mut Canvas) {
        const COLUMNS: usize = 60;
        let r = roster();
        let [x, y, w, h] = [40.0, 664.0, SCREEN_W - 80.0, 64.0];
        c.label("Spawn mix through the night", x, 628.0, 26.0, DIM, Align::Left);
        let col = w / COLUMNS as f32;
        for k in 0..COLUMNS {
            let minutes = (k as f32 + 0.5) / COLUMNS as f32 * crate::harvest::NIGHT_MINUTES;
            let mut top = y + h;
            for (i, share) in r.shares(minutes, self.save.enemies_off).into_iter().enumerate() {
                let tall = share * h;
                if tall > 0.0 {
                    c.rect(x + k as f32 * col, top - tall, col + 0.5, tall, self.mix_shade(i));
                }
                top -= tall;
            }
        }
        c.label("dusk", x, y + h + 4.0, 22.0, DIM, Align::Left);
        c.label("dawn", x + w, y + h + 4.0, 22.0, DIM, Align::Right);
    }

    /// The highlighted kind walking on the top screen, turning through its facings, beside its numbers.
    pub(crate) fn draw_enemy(&self, c: &mut Canvas) {
        let r = roster();
        let Some(e) = r.enemies.get(self.enemy) else { return };
        let on = self.save.enemies_off & 1 << self.enemy == 0;
        c.rect(0.0, 560.0, SCREEN_W, 208.0, [0.1, 0.08, 0.07, 1.0]);
        let turn = (self.clock / 1.5) as usize % 4;
        let (facing, flip) = [(0, false), (1, false), (2, false), (1, true)][turn];
        let walk = &e.look.walk[facing];
        let frame = walk[(self.clock * 8.0 * e.speed / 20.0) as usize % walk.len()];
        let scale = 300.0 / frame.h().max(frame.w());
        c.art(frame, 280.0 - frame.w() * scale / 2.0, 600.0 - frame.h() * scale, scale, flip, if on { WHITE } else { [0.35, 0.35, 0.4, 1.0] });
        if !on {
            c.label("OFF", 280.0, 400.0, 64.0, [1.0, 0.45, 0.4, 1.0], Align::Center);
        }
        self.draw_spawn_mix(c);
        fit_label(c, &e.name, 560.0, 60.0, 64.0, SCREEN_W - 560.0 - 24.0, HOT, Align::Left);
        let minutes = |m: f32| if m == 1.0 { "1 minute".to_string() } else { format!("{m} minutes") };
        let arrives = match (e.chance >= 1.0 && e.per_minute == 0.0, e.after > 0.0) {
            (true, false) => "Whenever no other kind does".to_string(),
            (true, true) => format!("After {}", minutes(e.after)),
            (false, after) => {
                let percent = |p: f32| (p * 1000.0).round() / 10.0;
                let grows = if e.per_minute > 0.0 { format!(" +{}%/min", percent(e.per_minute)) } else { String::new() };
                let from = if after { format!(" after {} min", e.after) } else { String::new() };
                format!("Chance {}%{grows}{from}", percent(e.chance))
            }
        };
        let rows = [
            ("Health", e.hp.to_string()),
            ("Speed", e.speed.to_string()),
            ("Bite", e.damage.to_string()),
            ("Money", e.xp.to_string()),
            ("Weight", e.mass.to_string()),
        ];
        for (i, (name, value)) in rows.iter().enumerate() {
            let y = 160.0 + i as f32 * 56.0;
            c.label(*name, 560.0, y, 36.0, DIM, Align::Left);
            c.label(value.clone(), 900.0, y, 36.0, WHITE, Align::Right);
        }
        c.label(arrives, 560.0, 450.0, 28.0, DIM, Align::Left);
        let traits = [(e.heavy, "Staggers on a hard hit"), (r.leader == self.enemy, "Leads hordes"), (r.harvester == self.enemy, "Rises at dawn as the Harvester"), (e.from_card, "From an enemy card")];
        for (i, (_, line)) in traits.iter().filter(|t| t.0).enumerate() {
            c.label(*line, 560.0, 500.0 + i as f32 * 36.0, 28.0, HOT, Align::Left);
        }
    }

    fn draw_stats(&self, c: &mut Canvas) {
        let s = &self.save;
        let (open, all, _) = harvest::unlocked(&self.progress());
        let rows = [
            ("Longest night", format!("{}:{:02}", s.best / 60, s.best % 60)),
            ("Highest level", s.best_level.to_string()),
            ("Zombies killed", s.kills.to_string()),
            ("Nights played", s.nights.to_string()),
            ("Nights survived", s.dawns.to_string()),
            ("Powerups unlocked", format!("{open} of {all}")),
            ("Cards loaded", crate::data::load_report().cards.to_string()),
            ("Mod packs", crate::data::load_report().mods.len().to_string()),
        ];
        c.label("Stats", SCREEN_W / 2.0, 24.0, 48.0, WHITE, Align::Center);
        for (i, (name, value)) in rows.iter().enumerate() {
            let y = 104.0 + i as f32 * 66.0;
            c.label(*name, 160.0, y, 36.0, DIM, Align::Left);
            c.label(value.clone(), SCREEN_W - 160.0, y, 36.0, HOT, Align::Right);
        }
        let mods = &crate::data::load_report().mods;
        if !mods.is_empty() {
            let line: String = mods.iter().map(|m| m.trim_end_matches(".pak")).collect::<Vec<_>>().join(", ").chars().take(60).collect();
            c.label(line, SCREEN_W - 160.0, 104.0 + rows.len() as f32 * 66.0 - 14.0, 24.0, DIM, Align::Right);
        }
        back_button(c);
    }
}

fn draw_credits(c: &mut Canvas) {
    let credits = [
        ("Programming", "Kyle James Walker"),
        ("Zombie Apocalypse Tileset", "Ittai Manero"),
        ("Template", "twinbit-ds"),
        ("Atkinson Hyperlegible font", "Braille Institute of America"),
    ];
    c.label("Credits", SCREEN_W / 2.0, 24.0, 48.0, WHITE, Align::Center);
    for (i, (role, name)) in credits.iter().enumerate() {
        let y = 116.0 + i as f32 * 132.0;
        c.label(*role, SCREEN_W / 2.0, y, 30.0, DIM, Align::Center);
        c.label(*name, SCREEN_W / 2.0, y + 40.0, 46.0, WHITE, Align::Center);
    }
    back_button(c);
}

/// Draws `text` within `room` pixels: smaller, down to two thirds of `size`, then cut short, keeping its middle where
/// a label of `size` at `y` would have it. Widths are estimated, since a label isn't measured until it's drawn.
#[allow(clippy::too_many_arguments)]
fn fit_label(c: &mut Canvas, text: &str, x: f32, y: f32, size: f32, room: f32, color: Color, align: Align) {
    let (text, fits) = fitted(text, size, room, size * 2.0 / 3.0);
    c.label(text, x, y + (size - fits) / 2.0, fits, color, align);
}

/// `text` and a size, from `size` down to `least`, that fit `room` pixels, cut short with an ellipsis if even `least`
/// doesn't.
pub(crate) fn fitted(text: &str, size: f32, room: f32, least: f32) -> (String, f32) {
    const EM: f32 = 0.56;
    let n = text.chars().count();
    let fits = size.min(room / (n.max(1) as f32 * EM)).max(least);
    let max = (room / (fits * EM) + 1e-3) as usize;
    let text = if n <= max { text.to_string() } else { text.chars().take(max.saturating_sub(3)).collect::<String>().trim_end().to_string() + "..." };
    (text, fits)
}

fn back_button(c: &mut Canvas) {
    let [x, y, w, h] = BACK;
    c.rect(x, y, w, h, [1.0, 1.0, 1.0, 0.08]);
    c.label("B: Back", x + w / 2.0, y + h / 2.0 - 22.0, 40.0, WHITE, Align::Center);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_shrinks_to_fit_then_cuts_short() {
        assert_eq!(fitted("Big", 64.0, 440.0, 40.0), ("Big".to_string(), 64.0), "a short name keeps its size");
        let (text, size) = fitted("Ghoul of the North", 64.0, 440.0, 40.0);
        assert!((40.0..64.0).contains(&size) && text == "Ghoul of the North");
        for n in 20..=40 {
            let name = "x".repeat(n);
            let (text, size) = fitted(&name, 64.0, 944.0, 64.0 * 2.0 / 3.0);
            assert!(size > 64.0 * 2.0 / 3.0 || text == name || text.ends_with("..."));
            if size > 64.0 * 2.0 / 3.0 {
                assert_eq!(text, name, "shrunk to fit, it isn't also cut");
            }
        }
        let (text, size) = fitted("Ghoul of the Northern Fields and Far Beyond", 64.0, 440.0, 40.0);
        assert_eq!(size, 40.0);
        assert!(text.ends_with("...") && text.chars().count() as f32 * 0.56 * size <= 440.0, "{text}");
    }
}
