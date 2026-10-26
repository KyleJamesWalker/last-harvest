//! The title's bottom-screen menu and its pages; Settings also opens over a paused run.

use crate::draw::{Align, Canvas, Frame};
use crate::harvest;
use crate::input::{Buttons, Input};
use crate::save::LEVELS;
use crate::{DIM, Difficulty, Game, HOT, SCREEN_H, SCREEN_W, Sfx, WHITE};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Page {
    #[default]
    Main,
    Settings,
    Stats,
    Credits,
}

const MAIN: [(Page, &str); 4] = [(Page::Main, "Play"), (Page::Settings, "Settings"), (Page::Stats, "Stats"), (Page::Credits, "Credits")];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Setting {
    TopBrightness,
    BottomBrightness,
    Volume,
    Rumble,
    Fps,
    Reset,
}

impl Setting {
    const ALL: [Setting; 6] = [Setting::TopBrightness, Setting::BottomBrightness, Setting::Volume, Setting::Rumble, Setting::Fps, Setting::Reset];

    fn name(self) -> &'static str {
        match self {
            Setting::TopBrightness => "Top brightness",
            Setting::BottomBrightness => "Bottom brightness",
            Setting::Volume => "Volume",
            Setting::Rumble => "Rumble",
            Setting::Fps => "Show FPS",
            Setting::Reset => "Reset progress",
        }
    }
}

/// The Play button, then the difficulty box under it, as x, y, width and height.
const PLAY: [f32; 4] = [212.0, 30.0, 600.0, 100.0];
pub(crate) const DIFFICULTY_Y: f32 = 150.0;
/// The Settings, Stats and Credits rows, below the difficulty box.
const ROWS_TOP: f32 = 330.0;
const ROW_H: f32 = 96.0;
/// The settings rows; a run's settings leave out Reset.
const SET_TOP: f32 = 100.0;
const SET_H: f32 = 86.0;
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

    pub(crate) fn draw_menu(&self, f: &mut Frame) {
        let c = &mut f.bottom;
        match self.page {
            Page::Main => self.draw_main(c),
            Page::Settings => self.draw_settings(c),
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
        ];
        c.label("Stats", SCREEN_W / 2.0, 24.0, 48.0, WHITE, Align::Center);
        for (i, (name, value)) in rows.iter().enumerate() {
            let y = 110.0 + i as f32 * 86.0;
            c.label(*name, 160.0, y, 40.0, DIM, Align::Left);
            c.label(value.clone(), SCREEN_W - 160.0, y, 40.0, HOT, Align::Right);
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

fn back_button(c: &mut Canvas) {
    let [x, y, w, h] = BACK;
    c.rect(x, y, w, h, [1.0, 1.0, 1.0, 0.08]);
    c.label("B: Back", x + w / 2.0, y + h / 2.0 - 22.0, 40.0, WHITE, Align::Center);
}
