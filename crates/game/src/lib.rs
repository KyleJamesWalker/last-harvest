pub mod art;
mod draw;
mod font;
mod gallery;
mod hardware;
mod harvest;
mod input;
mod osd;
mod rng;
mod save;
pub mod scene3d;
mod sfx;
mod system;

pub use draw::{Align, Canvas, Color, Frame, Shape};
pub use input::{Buttons, Input, InputTracker, MAX_TOUCHES, RawInput};
pub use save::Save;
pub use sfx::Sfx;
pub use system::{Battery, Outputs, System};

use rng::Rng;
use save::LEVELS;

pub const SCREEN_W: f32 = 1024.0;
pub const SCREEN_H: f32 = 768.0;

/// Kill ticks closer together than this blur into one buzz.
const TICK_GAP: f32 = 0.15;
const VOLUME_SHOWN: f32 = 1.2;
const WHITE: Color = [1.0, 1.0, 1.0, 1.0];
const HOT: Color = [1.0, 0.85, 0.2, 1.0];
const DIM: Color = [1.0, 1.0, 1.0, 0.55];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scene {
    Title,
    /// The game: survive the night on a generated farm.
    Harvest,
    /// Debug walk around a generated farm with no zombies.
    Farm,
    /// Debug frame-rate benchmark: the same frozen swarm every run.
    Bench,
    /// Debug view of every sprite in the art catalog.
    Gallery,
    /// Debug check of every hardware feature, and where screen brightness is set.
    Hardware,
}

impl Scene {
    pub fn from_name(name: &str) -> Option<Scene> {
        Some(match name {
            "title" | "menu" => Scene::Title,
            "harvest" | "play" => Scene::Harvest,
            "farm" => Scene::Farm,
            "bench" => Scene::Bench,
            "art" => Scene::Gallery,
            "hardware" => Scene::Hardware,
            _ => return None,
        })
    }
}

enum State {
    Title,
    Harvest(Box<harvest::Harvest>),
    Gallery(gallery::Gallery),
    Hardware(hardware::Hardware),
}

pub struct Game {
    state: State,
    rng: Rng,
    paused: bool,
    input: Input,
    system: System,
    show_perf: bool,
    rumble: f32,
    /// Motor pulses as (start, end) on `clock`.
    pulses: Vec<(f32, f32)>,
    clock: f32,
    last_tick: f32,
    sounds: Vec<Sfx>,
    save: Save,
    volume_shown: f32,
    leds: [bool; 2],
}

impl Game {
    pub fn new(seed: u64) -> Self {
        Self {
            state: State::Title,
            rng: Rng::new(seed),
            paused: false,
            input: Input::default(),
            system: System::default(),
            show_perf: false,
            rumble: 0.0,
            pulses: Vec::new(),
            clock: 0.0,
            last_tick: f32::MIN,
            sounds: Vec::new(),
            save: Save::default(),
            volume_shown: 0.0,
            leds: [false; 2],
        }
    }

    pub fn load(&mut self, save: Save) {
        self.save = save;
    }

    pub fn save(&self) -> Save {
        self.save
    }

    /// One line describing the current scene, for logs.
    pub fn summary(&self) -> String {
        match &self.state {
            State::Title => "title".into(),
            State::Harvest(h) => h.summary(),
            State::Gallery(_) => "art".into(),
            State::Hardware(_) => "hardware".into(),
        }
    }

    pub fn set_system(&mut self, system: System) {
        self.system = system;
    }

    pub fn open(&mut self, scene: Scene) {
        let seed = self.rng.next_u32() as u64 + 1;
        self.state = match scene {
            Scene::Title => State::Title,
            Scene::Harvest => {
                self.sounds.push(Sfx::Start);
                State::Harvest(Box::new(harvest::Harvest::new(seed)))
            }
            Scene::Farm => State::Harvest(Box::new(harvest::Harvest::calm(seed))),
            Scene::Bench => State::Harvest(Box::new(harvest::Harvest::bench())),
            Scene::Gallery => State::Gallery(gallery::Gallery::default()),
            Scene::Hardware => State::Hardware(hardware::Hardware::default()),
        };
    }

    /// Whether the vibration motor should be running this frame.
    pub fn rumble(&self) -> bool {
        self.rumble > 0.0 || self.pulses.iter().any(|&(a, b)| (a..b).contains(&self.clock))
    }

    fn buzz(&mut self, r: harvest::Rumble) {
        use harvest::Rumble;
        let now = self.clock;
        let pattern: &[(f32, f32)] = match r {
            Rumble::Tick if now - self.last_tick < TICK_GAP => &[],
            Rumble::Tick => {
                self.last_tick = now;
                &[(0.0, 0.05)]
            }
            Rumble::Hit => &[(0.0, 0.25)],
            Rumble::Blast => &[(0.0, 0.4)],
            Rumble::Fanfare => &[(0.0, 0.08), (0.14, 0.22), (0.3, 0.5)],
        };
        self.pulses.extend(pattern.iter().map(|&(a, b)| (now + a, now + b)));
    }

    pub fn outputs(&self) -> Outputs {
        let level = |l: u8| l as f32 / LEVELS as f32;
        Outputs {
            rumble: self.rumble(),
            volume: self.volume(),
            dread: match &self.state {
                State::Harvest(h) if !self.paused => h.dread(),
                _ => 0.0,
            },
            brightness: self.save.brightness.map(level),
            screens_on: !self.paused,
            work_led: self.leds[0],
            low_led: self.leds[1],
        }
    }

    /// Drains the queued sound cues.
    pub fn take_sounds(&mut self) -> Vec<Sfx> {
        std::mem::take(&mut self.sounds)
    }

    /// Master volume in 0..=1, set with the volume keys.
    pub fn volume(&self) -> f32 {
        self.save.volume as f32 / LEVELS as f32
    }

    pub fn update(&mut self, input: &Input, dt: f32) {
        self.paused = input.lid_closed;
        self.input = *input;
        self.rumble = (self.rumble - dt).max(0.0);
        self.clock += dt;
        let now = self.clock;
        self.pulses.retain(|&(_, end)| end > now);
        self.volume_shown = (self.volume_shown - dt).max(0.0);
        let step = input.pressed.contains(Buttons::VOL_UP) as i8 - input.pressed.contains(Buttons::VOL_DOWN) as i8;
        if step != 0 {
            self.save.volume = self.save.volume.saturating_add_signed(step).min(LEVELS);
            self.volume_shown = VOLUME_SHOWN;
            self.sounds.push(Sfx::Tick(self.save.volume));
        }
        if input.pressed.contains(Buttons::SELECT) && !matches!(self.state, State::Hardware(_)) {
            self.show_perf = !self.show_perf;
        }
        if self.paused {
            self.rumble = 0.0;
            self.pulses.clear();
            return;
        }
        if input.pressed.any(Buttons::BACK) {
            self.state = State::Title;
            return;
        }
        self.state = match std::mem::replace(&mut self.state, State::Title) {
            State::Title => {
                if input.pressed.any(Buttons::START | Buttons::A) || input.touch_started {
                    self.open(Scene::Harvest);
                    return;
                }
                State::Title
            }
            State::Harvest(mut h) => {
                let flow = h.update(input, dt, &mut self.sounds);
                if let Some(r) = flow.rumble {
                    self.buzz(r);
                }
                if let Some(secs) = flow.survived {
                    self.save.best = self.save.best.max(secs as u32);
                }
                if flow.quit { State::Title } else { State::Harvest(h) }
            }
            State::Gallery(mut g) => {
                g.update(input, dt);
                State::Gallery(g)
            }
            State::Hardware(mut hw) => {
                let controls = hardware::Controls {
                    sounds: &mut self.sounds,
                    rumble: &mut self.rumble,
                    brightness: &mut self.save.brightness,
                    leds: &mut self.leds,
                };
                hw.update(input, dt, controls);
                State::Hardware(hw)
            }
        };
    }

    pub fn draw(&self) -> Frame {
        let mut f = Frame::default();
        f.top.clear = [0.08, 0.08, 0.1, 1.0];
        f.bottom.clear = [0.12, 0.12, 0.15, 1.0];
        match &self.state {
            State::Title => self.draw_title(&mut f),
            State::Harvest(h) => h.draw(&mut f),
            State::Gallery(g) => g.draw(&mut f),
            State::Hardware(hw) => {
                hw.draw(&mut f, &self.system, self.save.brightness, self.leds);
                osd::draw(&mut f.top, &self.input, self.rumble(), self.system.headphones);
                osd::touches(&mut f.bottom, &self.input);
            }
        }
        if self.paused {
            f.top.text_centered(360.0, 16.0, WHITE, "PAUSED");
        }
        if self.volume_shown > 0.0 {
            f.top.text_centered(40.0, 8.0, WHITE, &format!("VOL {}", self.save.volume));
        }
        if self.show_perf {
            system::draw_perf(&mut f.top, &self.system);
        } else if matches!(self.state, State::Title) {
            system::draw_battery(&mut f.top, &self.system, 16.0);
        }
        f
    }

    fn draw_title(&self, f: &mut Frame) {
        let t = self.clock;
        f.top.clear = [0.06, 0.07, 0.11, 1.0];
        // A line of the dead shambling along the horizon, and fireflies drifting over the field.
        f.top.circle(840.0, 120.0, 90.0, [0.8, 0.8, 0.7, 0.06]);
        f.top.circle(840.0, 120.0, 48.0, [0.85, 0.85, 0.75, 1.0]);
        f.top.circle(822.0, 108.0, 10.0, [0.75, 0.75, 0.66, 1.0]);
        f.top.circle(856.0, 136.0, 7.0, [0.75, 0.75, 0.66, 1.0]);
        f.top.rect(0.0, 600.0, SCREEN_W, 168.0, [0.1, 0.08, 0.07, 1.0]);
        let walkers = [&art::SKINNY_SIDE, &art::KID_SIDE, &art::SKINNY_SIDE, &art::BIG_SIDE, &art::SKINNY_SIDE];
        for (i, frames) in walkers.iter().enumerate() {
            let span = SCREEN_W + 400.0;
            let x = (i as f32 * span / walkers.len() as f32 + t * 22.0) % span - 200.0;
            let a = frames[(t * 5.0 + i as f32) as usize % frames.len()];
            f.top.art(a, x, 600.0 - a.h() * 8.0 + 8.0, 8.0, false, [0.02, 0.02, 0.03, 1.0]);
        }
        for i in 0..30 {
            let seed = i as f32 * 7.31;
            let x = (seed.sin() * 9000.0).fract().abs() * SCREEN_W + (t * 0.3 + seed).sin() * 30.0;
            let y = (seed.cos() * 7000.0).fract().abs() * 560.0 + (t * 0.4 + seed * 2.0).cos() * 20.0;
            let glow = ((t * 1.7 + seed).sin() * 0.5 + 0.5).powi(3);
            f.top.circle(x, y, 3.0 + 3.0 * glow, [0.75, 1.0, 0.45, 0.2 + 0.7 * glow]);
        }
        let flicker = 0.85 + 0.15 * ((t * 13.0).sin() * (t * 7.3).sin()).abs();
        f.top.text_centered(248.0, 20.0, [0.0, 0.0, 0.0, 0.7], "LAST HARVEST");
        f.top.text_centered(240.0 + (t * 1.2).sin() * 4.0, 20.0, [flicker, flicker * 0.92, flicker * 0.8, 1.0], "LAST HARVEST");
        f.top.label("Survive the night", SCREEN_W / 2.0, 400.0, 44.0, DIM, Align::Center);
        if self.save.best > 0 {
            let line = format!("Longest night: {}:{:02}", self.save.best / 60, self.save.best % 60);
            f.top.label(line, SCREEN_W / 2.0, 480.0, 34.0, HOT, Align::Center);
        }
        let pulse = 0.55 + 0.45 * (t * 3.0).sin().abs();
        f.bottom.label("Press A or tap to start", SCREEN_W / 2.0, 300.0, 56.0, [1.0, 1.0, 1.0, pulse], Align::Center);
        let hint = "D-pad: walk and strafe.  Y / A: turn.  B: turn around.  Tap the map: throw.";
        f.bottom.label(hint, SCREEN_W / 2.0, SCREEN_H - 60.0, 24.0, DIM, Align::Center);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(game: &mut Game, b: Buttons) {
        game.update(&Input { pressed: b, ..Default::default() }, 0.016);
    }

    #[test]
    fn a_or_a_tap_on_the_title_starts_a_run_and_back_returns() {
        let mut game = Game::new(1);
        press(&mut game, Buttons::A);
        assert!(matches!(game.state, State::Harvest(_)));
        press(&mut game, Buttons::BACK);
        assert!(matches!(game.state, State::Title));
        game.update(&Input { touch: Some([512.0, 400.0]), touch_started: true, ..Default::default() }, 0.016);
        assert!(matches!(game.state, State::Harvest(_)));
    }

    #[test]
    fn a_finished_run_records_the_longest_night() {
        let mut game = Game::new(1);
        press(&mut game, Buttons::A);
        let mut t = InputTracker::default();
        // Tapping A every other frame picks each level-up so the run plays out.
        for i in 0..60 * 60 * 10 {
            let held = if i % 2 == 0 { Buttons::A } else { Buttons::NONE };
            game.update(&t.frame(RawInput { held, ..Default::default() }), 1.0 / 60.0);
            if game.save().best > 0 {
                break;
            }
        }
        assert!(game.save().best > 0);
    }

    #[test]
    fn volume_keys_clamp_beep_and_persist() {
        let mut game = Game::new(1);
        for _ in 0..20 {
            press(&mut game, Buttons::VOL_UP);
        }
        assert_eq!(game.volume(), 1.0);
        assert_eq!(game.save().volume, LEVELS);
        assert!(game.take_sounds().iter().all(|s| matches!(s, Sfx::Tick(_))));
    }

    #[test]
    fn closing_the_lid_freezes_the_game_and_turns_the_screens_off() {
        let mut game = Game::new(1);
        let input = Input { lid_closed: true, pressed: Buttons::START, ..Default::default() };
        game.update(&input, 0.016);
        assert!(matches!(game.state, State::Title));
        assert!(!game.outputs().screens_on);
    }

    #[test]
    fn every_scene_opens_by_name() {
        let mut game = Game::new(1);
        for name in ["title", "harvest", "farm", "bench", "art", "hardware"] {
            game.open(Scene::from_name(name).unwrap());
        }
        assert!(matches!(game.state, State::Hardware(_)));
    }
}
