//! The game, free of any platform. Each frame `App::tick` (crates/app/src/main.rs; `frame` in `web.rs`) calls
//! `Game::update`, which dispatches on `State` to the title, gallery, hardware page or a run's `Harvest::update`,
//! and the app renders what `Game::draw` returns.

pub mod alpha;
pub mod art;
pub mod data;
pub mod paint;
mod difficulty;
mod draw;
pub mod fmath;
mod font;
mod gallery;
mod hardware;
mod icon;
mod harvest;
mod input;
mod menu;
pub mod netplay;
pub mod netsync;
mod online_menu;
#[cfg(test)]
mod netplay_tests;
mod osd;
mod controls;
pub mod pad;
pub mod photo;
mod rng;
mod save;
pub mod scene3d;
mod sfx;
mod system;
mod watch;
pub mod touch;
pub mod tuning;
pub mod worn;

pub use difficulty::{Difficulty, Tier};
pub use draw::{Align, Canvas, Color, Frame, Shape};
pub use art::Stand;
pub use harvest::showcase::{CropShow, Figure, Showcase};
pub use harvest::{SwingShow, swing_shown};
pub use harvest::{NamedProp, load_props_for_checks, PROP_MAX_FRAME, PROP_MAX_FRAMES, PROP_PREFIX, PropByViews, PropFields, named_props, turned_depth, prop_fields, prop_text, set_props};
pub use harvest::{RangeSetup, effects_with, powerups_with, sandbox};
pub use harvest::online;
pub use harvest::replay;
pub use harvest::{DEFAULT_SOUND_DELAY, WeaponLevels, sound_group, sound_groups, valid_sound_group, powerup_zones, unlock_conditions, weapon_behaviours, weapon_aims, weapon_levels, weapon_patterns, weapon_sound_names, weapon_stats, weapon_targets};
pub use harvest::{SHOT_SPRITES, Shot, shot_named, shot_sprite_names};
pub use harvest::{TABLES, built_in_table, check_own_table, check_table, default_prop_depth, MAP_EXTENSION, map_ground, set_map_ground, CropSway, MAP_MAX, MAP_MIN, map_props, parse_props, MapSource, PROP_MAX_HP, PropKind, PropLine, PropTough, check_map, check_map_on_any_farm, valid_map_id, FARM_DIR, FARM_GROUND_END, FARM_ID_MAX, farm_file, farm_ids, farm_of_file, set_farm, table_kind, use_farm, use_farm_text, valid_farm_id};
pub use harvest::{ABOUT_MAX, CARD_FLAGS, CARD_MAX_FRAME, card_frame_most, CARD_NUMBERS, MAX_WALK, PLAYER_BASE_KEYS, PLAYER_ID_MAX, PlayerFields, PlayerLook, default_start, flash_pixel, frame_limits, hurt_pixel, player_fields, player_looks, set_player_farm, set_player_start, use_player_farm, player_powerups, weapon_slots, weapons, CardFields, night_minutes, SpawnMix, card_fields, check_tables, kill_sound, kill_sounds, kill_weight, set_card_field, spawn_mix};
pub use input::{Buttons, Input, InputTracker, MAX_TOUCHES, RawInput};
pub use pad::PadFamily;
pub use controls::Controls;
pub use save::{ModsOff, Save, TouchControls, mods_off_in, pack_key};
pub use sfx::Sfx;
pub use system::{Battery, Outputs, System};

use crate::fmath as fm;
use rng::Rng;
use menu::Page;
use save::LEVELS;

pub const SCREEN_W: f32 = 1024.0;
pub const SCREEN_H: f32 = 768.0;
/// The most players who share one screen, each on their own pad.
pub const SEATS: usize = 4;

/// Kill ticks closer together than this blur into one buzz.
const TICK_GAP: f32 = 0.15;
const VOLUME_SHOWN: f32 = 1.2;
/// The most kinds walking the title's horizon besides the filler between them.
const MAX_TITLE_KINDS: usize = 5;
const WHITE: Color = [1.0, 1.0, 1.0, 1.0];
const HOT: Color = [1.0, 0.85, 0.2, 1.0];
const DIM: Color = [1.0, 1.0, 1.0, 0.55];

/// What the app saves after a draw that asked for a screenshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capture {
    /// Both screens, as drawn.
    Screens,
    /// Photo mode's top screen alone, with its middle square beside it.
    Photo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scene {
    Title,
    /// The game: survive the night on a generated farm.
    Harvest,
    /// Debug: the game on the top screen alone, with the map on L1 and grenades aimed on L2.
    Single,
    /// Debug walk around a generated farm with no zombies.
    Farm,
    /// Debug frame-rate benchmark: the same frozen swarm every run, on a farm this many tiles wide and tall.
    Bench([usize; 2]),
    /// The benchmark's horde standing in a field of the corn ground's crop, past the view's reach on every side.
    BenchCorn([usize; 2]),
    /// Debug view of every sprite in the art catalog.
    Gallery,
    /// Debug check of every hardware feature, and where screen brightness is set.
    Hardware,
    /// Debug close-up of one figure, in 3D and flat, for making icons.
    Icon,
    /// The weapon range: one weapon, as `Game::set_range` gives it, against a ring of zombies.
    Range,
}

impl Scene {
    pub fn from_name(name: &str) -> Option<Scene> {
        Some(match name {
            "title" | "menu" => Scene::Title,
            "harvest" | "play" => Scene::Harvest,
            "single" => Scene::Single,
            "farm" => Scene::Farm,
            "bench" => Scene::Bench(harvest::MAP_SIZE),
            "bench-corn" => Scene::BenchCorn(harvest::MAP_SIZE),
            _ if name.starts_with("bench-corn-") => {
                let (w, h) = name["bench-corn-".len()..].split_once('x')?;
                Scene::BenchCorn([w.parse().ok()?, h.parse().ok()?])
            }
            _ if name.starts_with("bench-") => {
                let (w, h) = name["bench-".len()..].split_once('x')?;
                Scene::Bench([w.parse().ok()?, h.parse().ok()?])
            }
            "art" => Scene::Gallery,
            "hardware" => Scene::Hardware,
            "icon" => Scene::Icon,
            "range" => Scene::Range,
            _ => return None,
        })
    }
}

enum State {
    Title,
    Harvest(Box<harvest::Harvest>),
    Gallery(gallery::Gallery),
    Hardware(hardware::Hardware),
    Icon(Box<icon::Icon>),
    /// A saved replay played back, from the Replays page.
    Watch(Box<watch::Watching>),
}

pub struct Game {
    state: State,
    rng: Rng,
    paused: bool,
    input: Input,
    system: System,
    rumble: f32,
    /// Each seat's motor pulses as (start, end) on `clock`.
    pulses: [Vec<(f32, f32)>; SEATS],
    clock: f32,
    last_tick: [f32; SEATS],
    sounds: Vec<Sfx>,
    save: Save,
    volume_shown: f32,
    leds: [bool; 2],
    /// Every powerup on offer, whatever the save says, for trying them out.
    unlock_all: bool,
    /// Powerups every run starts with, as `Harvest::with_kit` reads them.
    kit: String,
    /// Seconds into the night every run starts at, for trying out late-night play.
    start_t: f32,
    /// A bench scene just opened and hasn't reported its flow-field timing yet.
    bench_due: bool,
    /// A thrown grenade is never spent, for trying out aiming.
    endless: bool,
    /// Scripted players every run starts with beside the local one.
    bots: usize,
    /// What the range scene tries.
    range: RangeSetup,
    /// The `single` scene is open, which plays on one screen whatever Settings says.
    single_scene: bool,
    /// A phone's window, in pixels: one canvas, on-screen controls, runs on one screen.
    phone: Option<[f32; 2]>,
    /// The platform sets the screens' backlights, so Settings offers their brightness.
    backlights: bool,
    /// The phone's screen takes touches, so Settings offers the on-screen controls and touch grenades.
    touch_ui: bool,
    /// The main menu offers Quit, where the platform can close the game.
    can_quit: bool,
    quit_asked: bool,
    touchpad: touch::Touchpad,
    /// Seconds left to confirm wiping the records and found powerups.
    reset_armed: f32,
    /// Seconds left of the title's word that progress was erased.
    reset_shown: f32,
    page: Page,
    /// The highlighted row on the title's main page and on the settings page.
    row: usize,
    /// The History page on show.
    history_page: usize,
    setting: usize,
    /// The highlighted kind on the Enemies page.
    enemy: usize,
    /// Settings is open over a paused run, rather than from the title.
    settings_in_run: bool,
    /// R2 asked for a screenshot, which the app takes after the next draw.
    shot: bool,
    /// Seconds left of the white flash that says a screenshot was taken.
    shot_flash: f32,
    /// The platform saves screenshots, so photo mode can take one.
    captures: bool,
    /// A button or stick has been used since launch, so Off can't strand a touch-only player.
    pad_seen: bool,
    /// The game folder's name, empty until one is chosen; None where the platform has no folder to choose.
    folder: Option<String>,
    /// Settings asked the platform to choose the game folder.
    folder_asked: bool,
    /// The loaded save hasn't seen the how-to page, so the first Play opens it.
    how_to_due: bool,
    /// The how-to page on show was opened by Play, so leaving it starts the run.
    how_to_then_play: bool,
    /// The Controls page, open over Settings.
    rebind: Option<menu::Rebind>,
    /// The pad buttons pressed this frame before the controls table turned them into game buttons.
    raw_pressed: Buttons,
    /// The platform has a keyboard, so the Controls page shows each job's key.
    keyboard: bool,
    /// Players sharing the screen, as the platform seated them at the title.
    players: usize,
    /// The fewest views a run shows, for trying split-screen with scripted players in the views past the seated ones.
    split: usize,
    /// Each view's size in pixels while a run is split, player 1's first.
    view_sizes: Vec<[f32; 2]>,
    /// What players 2 onward press this frame, by seat.
    others: Vec<Input>,
    net: Option<Box<netplay::Net>>,
    online: online_menu::Online,
    /// Every night is recorded, each replay headed by these lines; none records nothing.
    replay_info: Option<Vec<(String, String)>>,
    replays: Vec<Vec<u8>>,
    /// The replays the platform kept, newest first, for the Replays page.
    saved_replays: Vec<watch::Saved>,
    replay_row: usize,
    /// The Replays page opened and wants the platform's replays.
    replays_asked: bool,
}

/// Where the title's menu sits on the top screen when one screen shows both, and its scale.
const TITLE_MENU: ([f32; 2], f32) = ([(SCREEN_W - SCREEN_W * 0.75) / 2.0, 170.0], 0.75);

/// A touch on the top screen as one on the title's menu placed over it, if it lands there.
fn title_menu_touch([x, y]: [f32; 2]) -> Option<[f32; 2]> {
    let ([ax, ay], k) = TITLE_MENU;
    let p = [(x - ax) / k, (y - ay) / k];
    ((0.0..SCREEN_W).contains(&p[0]) && (0.0..SCREEN_H).contains(&p[1])).then_some(p)
}

impl Game {
    pub fn new(seed: u64) -> Self {
        Self {
            state: State::Title,
            rng: Rng::new(seed),
            paused: false,
            input: Input::default(),
            system: System::default(),
            rumble: 0.0,
            pulses: Default::default(),
            clock: 0.0,
            last_tick: [f32::MIN; SEATS],
            sounds: Vec::new(),
            save: Save::default(),
            volume_shown: 0.0,
            leds: [false; 2],
            unlock_all: false,
            kit: String::new(),
            start_t: 0.0,
            bench_due: false,
            endless: false,
            bots: 0,
            range: RangeSetup::default(),
            single_scene: false,
            phone: None,
            backlights: true,
            touch_ui: true,
            can_quit: false,
            quit_asked: false,
            touchpad: touch::Touchpad::default(),
            reset_armed: 0.0,
            reset_shown: 0.0,
            page: Page::Main,
            row: 0,
            history_page: 0,
            setting: 0,
            enemy: 0,
            settings_in_run: false,
            shot: false,
            shot_flash: 0.0,
            captures: false,
            pad_seen: false,
            folder: None,
            folder_asked: false,
            how_to_due: false,
            how_to_then_play: false,
            rebind: None,
            raw_pressed: Buttons::NONE,
            keyboard: false,
            players: 1,
            split: 1,
            view_sizes: Vec::new(),
            others: Vec::new(),
            net: None,
            online: online_menu::Online::default(),
            replay_info: None,
            replays: Vec::new(),
            saved_replays: Vec::new(),
            replay_row: 0,
            replays_asked: false,
        }
    }

    /// What R2 or photo mode asked to capture, if anything; the app saves the frame it just drew.
    pub fn take_screenshot(&mut self) -> Option<Capture> {
        if !std::mem::take(&mut self.shot) {
            return None;
        }
        self.shot_flash = SHOT_FLASH;
        Some(if self.in_photo() { Capture::Photo } else { Capture::Screens })
    }

    /// The platform saves what `take_screenshot` asks for, so photo mode offers to take photos.
    pub fn set_captures(&mut self, on: bool) {
        self.captures = on;
    }

    /// The platform lets the player choose a game folder, this one so far, or "" for none yet: Settings shows it.
    pub fn set_folder(&mut self, name: &str) {
        self.folder = Some(name.to_owned());
    }

    /// Settings' Game folder was pressed since the last call: the platform shows its folder picker.
    pub fn take_folder_request(&mut self) -> bool {
        std::mem::take(&mut self.folder_asked)
    }

    fn in_photo(&self) -> bool {
        match &self.state {
            State::Harvest(h) => h.in_photo(),
            State::Watch(w) => w.in_photo(),
            _ => false,
        }
    }

    /// This frame is a photo mode capture, drawn with no guides or overlays.
    fn clean(&self) -> bool {
        self.shot && self.captures && self.in_photo()
    }

    pub fn load(&mut self, save: Save) {
        self.how_to_due = !save.how_to_seen;
        self.save = save;
        pad::set_controls(save.controls);
    }

    pub fn save(&self) -> Save {
        self.save
    }

    pub fn unlock_everything(&mut self) {
        self.unlock_all = true;
    }

    /// Starts every run carrying these powerups: `id` or `id:level`, comma separated.
    pub fn start_with(&mut self, kit: &str) {
        self.kit = kit.to_string();
    }

    /// Starts every run `secs` seconds into the night.
    pub fn start_clock(&mut self, secs: f32) {
        self.start_t = secs.max(0.0);
    }

    pub fn endless_grenades(&mut self, on: bool) {
        self.endless = on;
    }

    /// Starts every run with `n` scripted players beside the local one.
    pub fn add_bots(&mut self, n: usize) {
        self.bots = n;
    }

    /// What the range scene tries: the platform installs a setup's section into the data before the game reads it.
    pub fn set_range(&mut self, setup: RangeSetup) {
        self.range = setup;
    }

    fn progress(&self) -> harvest::Progress {
        self.progress_at(self.save.difficulty)
    }

    /// What `d`'s nights have unlocked.
    fn progress_at(&self, d: Difficulty) -> harvest::Progress {
        let s = self.save.records(d);
        harvest::Progress { kills: s.kills, best: s.best, best_level: s.best_level, nights: s.nights, dawns: s.dawns, breakers: s.breakers, breaker_dawns: s.breaker_dawns, found: s.found, everything: self.unlock_all, hidden: harvest::hidden_powerups(self.save.player.as_str()) }
    }

    /// One line describing the current scene, for logs.
    pub fn summary(&self) -> String {
        match &self.state {
            State::Title => "title".into(),
            State::Harvest(h) => match self.net_traffic() {
                Some(t) if t.secs > 0.0 => format!("{} net down={:.0}B/s up={:.0}B/s", h.summary(), t.down as f32 / t.secs, t.up as f32 / t.secs),
                _ => h.summary(),
            },
            State::Gallery(_) => "art".into(),
            State::Hardware(_) => "hardware".into(),
            State::Icon(_) => "icon".into(),
            State::Watch(_) => "replay".into(),
        }
    }

    pub fn set_system(&mut self, system: System) {
        self.system = system;
    }

    /// The labels on the pad or keyboard that last sent input, which every hint that names a button uses.
    pub fn set_pad_family(&mut self, family: PadFamily) {
        pad::set_active(family);
        self.keyboard |= family == PadFamily::Keyboard;
    }

    /// The platform maps a pad's face buttons by the names printed on them, not by position, so hints name them so.
    pub fn set_buttons_by_name(&mut self, on: bool) {
        pad::set_by_name(on);
    }

    /// The platform has a keyboard, before any key is pressed.
    pub fn set_keyboard(&mut self, on: bool) {
        self.keyboard = on;
    }

    /// The pad button a keyboard's Esc stands for, so it pauses a run or backs out of a menu and never quits.
    pub fn escape(&self) -> Buttons {
        self.save.controls.physical(match &self.state {
            State::Harvest(h) if self.page != Page::Settings => h.escape(),
            _ => Buttons::B,
        })
    }

    /// A bench scene's flow-field timing, once, the first time it's asked for after the scene opens; `time` runs
    /// the work it's given and returns the seconds it took.
    pub fn bench_report(&mut self, time: impl FnOnce(&mut dyn FnMut()) -> f64) -> Option<String> {
        match &self.state {
            State::Harvest(h) if std::mem::take(&mut self.bench_due) => Some(h.bench_report(time)),
            _ => None,
        }
    }

    /// What the debug pipe's `state` prints, in a harvest.
    pub fn debug_state(&self) -> Option<String> {
        match &self.state {
            State::Harvest(h) => Some(h.debug_state()),
            _ => None,
        }
    }

    pub fn open(&mut self, scene: Scene) {
        self.keep_replays();
        self.bench_due = matches!(scene, Scene::Bench(_) | Scene::BenchCorn(_));
        let seed = self.rng.next_u32() as u64 + 1;
        self.single_scene = scene == Scene::Single;
        harvest::use_farm(None);
        self.state = match scene {
            Scene::Title => {
                self.page = Page::Main;
                State::Title
            }
            Scene::Harvest | Scene::Single => {
                self.sounds.push(Sfx::Start);
                let player = self.save.player.as_str().to_string();
                let setup = replay::Setup { map: self.save.map.as_str().to_string(), difficulty: self.save.difficulty, progress: self.progress(), enemies_off: self.save.enemies_off, look: player.clone(), farm: Some(player), kit: self.kit.clone(), start_t: self.start_t, one_screen: self.one_screen(), endless: self.endless, players: self.players, bots: self.bots, views: self.players.max(self.split), ..replay::Setup::night(seed) };
                let mut h = setup.build();
                if let Some(info) = &self.replay_info {
                    h.record(info.clone());
                }
                State::Harvest(Box::new(h))
            }
            Scene::Farm => State::Harvest(Box::new(harvest::Harvest::calm(seed))),
            Scene::Bench(size) => State::Harvest(Box::new(harvest::Harvest::bench(size))),
            Scene::BenchCorn(size) => State::Harvest(Box::new(harvest::Harvest::bench_corn(size))),
            Scene::Range => State::Harvest(Box::new(harvest::Harvest::range(seed, &self.range).single_screen(self.one_screen()))),
            Scene::Gallery => State::Gallery(gallery::Gallery::default()),
            Scene::Hardware => State::Hardware(hardware::Hardware::default()),
            Scene::Icon => State::Icon(Box::default()),
        };
    }

    /// Runs play on the top screen alone.
    fn one_screen(&self) -> bool {
        self.single_scene || self.save.one_screen || self.phone.is_some()
    }

    /// Plays as on a phone in a window `size` pixels across and down, whose touches come in its pixels.
    pub fn set_phone(&mut self, size: Option<[f32; 2]>) {
        self.phone = size;
    }

    pub fn set_backlights(&mut self, on: bool) {
        self.backlights = on;
    }

    /// Offers Quit in the main menu; the platform closes the game once `quit_asked` says so.
    pub fn set_quit(&mut self, on: bool) {
        self.can_quit = on;
    }

    pub fn quit_asked(&self) -> bool {
        self.quit_asked
    }

    /// How many players the platform has seated; the title names those who joined.
    pub fn set_players(&mut self, n: usize) {
        self.players = n.max(1);
    }

    /// Every run shows at least `n` views, the ones past the seated players driven by `LAST_HARVEST_BOTS`'s players.
    pub fn set_split(&mut self, n: usize) {
        self.split = n.max(1);
    }

    /// How many views the run on show has, one per local player; 1 outside a run.
    pub fn views(&self) -> usize {
        match &self.state {
            State::Harvest(h) => h.views(),
            _ => 1,
        }
    }

    /// The size in pixels of each view the window splits into, player 1's first, so each lays its HUD out to fit.
    pub fn set_view_sizes(&mut self, sizes: Vec<[f32; 2]>) {
        self.view_sizes = sizes;
    }

    /// The view whose player works the pause menu, its Settings and photo mode: 0 outside a co-op run.
    pub fn menu_view(&self) -> usize {
        match &self.state {
            State::Harvest(h) if h.views() > 1 => h.menu_player(),
            _ => 0,
        }
    }

    /// Runs a step with each seated player's input, player 1's first.
    pub fn update_all(&mut self, inputs: &[Input], dt: f32) {
        let Some((first, others)) = inputs.split_first() else { return };
        self.others = others.to_vec();
        self.update(first, dt);
    }

    pub fn record_replays(&mut self, info: Vec<(String, String)>) {
        self.replay_info = Some(info);
    }

    pub fn take_replays(&mut self) -> Vec<Vec<u8>> {
        if let State::Harvest(h) = &mut self.state {
            self.replays.extend(h.take_replays());
        }
        std::mem::take(&mut self.replays)
    }

    /// Ends the recording of the night in play where it stands, then takes every replay.
    pub fn end_replays(&mut self) -> Vec<Vec<u8>> {
        self.keep_replays();
        self.take_replays()
    }

    /// True once each time the Replays page opens: the platform then hands over its replays with `set_saved_replays`.
    pub fn take_replays_request(&mut self) -> bool {
        std::mem::take(&mut self.replays_asked)
    }

    /// The replays the platform keeps, newest first, as file names and bytes after any gzip.
    pub fn set_saved_replays(&mut self, saved: Vec<(String, Vec<u8>)>) {
        self.saved_replays = saved.into_iter().map(|(name, raw)| watch::Saved::new(name, raw)).collect();
        self.replay_row = self.replay_row.min(self.saved_replays.len().saturating_sub(1));
    }

    /// Plays saved replay `i` on the run's screens; one that won't read stays on the list with why.
    pub(crate) fn watch_replay(&mut self, i: usize) {
        let Some(opened) = self.saved_replays.get(i).map(watch::Saved::open) else { return };
        match opened {
            Ok((r, warn)) => {
                self.keep_replays();
                harvest::use_farm(None);
                self.sounds.push(Sfx::Start);
                self.state = State::Watch(Box::new(watch::Watching::new(r, warn)));
            }
            Err(e) => {
                self.saved_replays[i].line = Err(e);
                self.sounds.push(Sfx::Miss);
            }
        }
    }

    /// Where a replay's controls go: along the foot of the top screen when it's the only one, else atop the bottom.
    fn watch_strip_top(&self) -> Option<f32> {
        self.phone.is_some().then_some(SCREEN_H - watch::STRIP_H)
    }

    pub(crate) fn keep_replays(&mut self) {
        if let State::Harvest(h) = &mut self.state {
            self.replays.extend(h.stop_recording());
        }
    }

    /// The title's main page is open, where a pad joins or leaves.
    pub fn at_title(&self) -> bool {
        matches!(self.state, State::Title) && self.page == Page::Main
    }

    /// A run is on, a night or a debug scene that plays like one.
    pub fn in_run(&self) -> bool {
        matches!(self.state, State::Harvest(_))
    }

    /// The title's main page on one screen without touch: the top screen with the menu over it, smaller.
    pub(crate) fn title_on_one(&self) -> bool {
        self.phone.is_some() && !self.has_touch() && matches!(self.state, State::Title) && self.page == Page::Main
    }

    /// Plays as on a phone without touch: no on-screen controls, grenades aimed with L2, whatever Settings says.
    pub fn set_touch(&mut self, on: bool) {
        self.touch_ui = on;
    }

    pub(crate) fn touch_screen(&self) -> bool {
        self.touch_ui
    }

    /// The on-screen controls are laid out, as the screen takes touches and Settings leaves them on.
    pub(crate) fn has_touch(&self) -> bool {
        self.touch_ui && self.save.touch.shown()
    }

    pub(crate) fn has_backlights(&self) -> bool {
        self.backlights
    }

    pub(crate) fn is_phone(&self) -> bool {
        self.phone.is_some()
    }

    fn phone_layout(&self, size: [f32; 2]) -> touch::Layout {
        if matches!(self.state, State::Watch(_)) {
            let photo = self.in_photo() && self.has_touch();
            return touch::layout(size, true, photo, photo);
        }
        touch::layout(size, matches!(self.state, State::Harvest(_)) || self.title_on_one(), self.has_touch(), self.in_photo())
    }

    /// The input with the on-screen controls' buttons, stick and screen touch in place of raw fingers.
    fn through_touchpad(&mut self, input: &Input, size: [f32; 2]) -> Input {
        self.pad_seen |= input.held != Buttons::NONE || input.pressed != Buttons::NONE || input.stick != [0.0; 2] || input.look != 0.0;
        if self.save.touch == TouchControls::Off && !self.pad_seen && self.touch_screen() && input.touches.iter().any(Option::is_some) {
            self.save.touch = TouchControls::Auto;
        }
        let layout = self.phone_layout(size);
        let hides = self.save.touch.hide_at_other_input();
        let other = hides && (input.held != Buttons::NONE || input.stick != [0.0; 2] || input.look != 0.0);
        self.touchpad.hidden &= hides;
        let mut pad = self.touchpad.update(&input.touches, &layout, other);
        // The Grenade key is R1, which rerolls the level-up cards; there only the Reroll key rerolls.
        if matches!(&self.state, State::Harvest(h) if h.choosing()) {
            pad.held.set(Buttons::R1, false);
            pad.pressed.set(Buttons::R1, false);
        }
        let stick = [0, 1].map(|i| (input.stick[i] + pad.stick[i]).clamp(-1.0, 1.0));
        let look = (input.look + pad.look).clamp(-1.0, 1.0);
        let touch = if self.title_on_one() { pad.touch.and_then(title_menu_touch) } else { pad.touch };
        let mut touches = [None; MAX_TOUCHES];
        touches[0] = touch;
        Input { held: input.held | pad.held, pressed: input.pressed | pad.pressed, stick, look, touch, touches, touch_started: pad.touch_started && touch.is_some(), ..*input }
    }

    /// Whether the vibration motor should be running this frame.
    pub fn rumble(&self) -> bool {
        self.rumble > 0.0 || self.pulsing(0)
    }

    /// Whether the pads of the player in `seat`, from 0, should be buzzing this frame, as Settings allows.
    pub fn rumble_for(&self, seat: usize) -> bool {
        let on = if seat == 0 { self.rumble() } else { self.pulsing(seat) };
        on && (self.save.rumble || matches!(self.state, State::Hardware(_)))
    }

    fn pulsing(&self, seat: usize) -> bool {
        self.pulses.get(seat).is_some_and(|p| p.iter().any(|&(a, b)| (a..b).contains(&self.clock)))
    }

    fn buzz(&mut self, seat: usize, r: harvest::Rumble) {
        use harvest::Rumble;
        let now = self.clock;
        let pattern: &[(f32, f32)] = match r {
            Rumble::Tick if now - self.last_tick[seat] < TICK_GAP => &[],
            Rumble::Tick => {
                self.last_tick[seat] = now;
                &[(0.0, 0.05)]
            }
            Rumble::Hit => &[(0.0, 0.25)],
            Rumble::Blast => &[(0.0, 0.4)],
            Rumble::Fanfare => &[(0.0, 0.08), (0.14, 0.22), (0.3, 0.5)],
        };
        self.pulses[seat].extend(pattern.iter().map(|&(a, b)| (now + a, now + b)));
    }

    pub fn outputs(&self) -> Outputs {
        let level = |l: u8| l as f32 / LEVELS as f32;
        Outputs {
            rumble: self.rumble_for(0),
            volume: self.volume(),
            dread: match &self.state {
                State::Harvest(h) if !self.paused => h.dread(),
                State::Watch(w) if !self.paused => w.dread(),
                _ => 0.0,
            },
            brightness: self.save.brightness.map(level),
            screens_on: [!self.paused, !self.paused && !matches!(&self.state, State::Harvest(h) if h.one_screen())],
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
        if let State::Harvest(h) = &mut self.state {
            h.hold_still();
        }
        pad::set_controls(self.save.controls);
        self.raw_pressed = input.pressed;
        let controls = if matches!(self.state, State::Hardware(_)) { Controls::default() } else { self.save.controls };
        let mapped = Input { held: controls.apply(input.held), pressed: controls.apply(input.pressed), ..*input };
        let input = &mapped;
        let rebinding = self.rebind.as_mut().map(|r| {
            r.tick(dt);
            r.waiting()
        }) == Some(true);
        let padded;
        let input = match self.phone {
            Some(size) => {
                padded = self.through_touchpad(input, size);
                &padded
            }
            None => input,
        };
        self.paused = input.lid_closed;
        self.input = *input;
        self.rumble = (self.rumble - dt).max(0.0);
        self.clock += dt;
        let now = self.clock;
        for p in &mut self.pulses {
            p.retain(|&(_, end)| end > now);
        }
        self.volume_shown = (self.volume_shown - dt).max(0.0);
        let step = input.pressed.contains(Buttons::VOL_UP) as i8 - input.pressed.contains(Buttons::VOL_DOWN) as i8;
        if step != 0 {
            self.save.volume = self.save.volume.saturating_add_signed(step).min(LEVELS);
            self.volume_shown = VOLUME_SHOWN;
            self.sounds.push(Sfx::Tick(self.save.volume));
        }
        self.shot_flash = (self.shot_flash - dt).max(0.0);
        // The icon scene keeps R2 for its barricade, so its screenshot stays on R1.
        let shot_key = if matches!(self.state, State::Icon(_)) { Buttons::R1 } else { Buttons::R2 };
        if input.pressed.contains(shot_key) && self.captures && !matches!(self.state, State::Hardware(_)) && !self.in_photo() && !rebinding {
            self.shot = true;
            self.sounds.push(Sfx::Tick(LEVELS));
        }
        if input.pressed.contains(Buttons::SELECT) && !matches!(self.state, State::Hardware(_)) && !rebinding {
            self.save.fps = !self.save.fps;
        }
        if self.paused {
            self.rumble = 0.0;
            self.pulses.iter_mut().for_each(Vec::clear);
            self.online_away(dt);
            return;
        }
        if input.pressed.any(Buttons::BACK) {
            let online = self.online_run();
            self.open(Scene::Title);
            self.leave_online(online);
            return;
        }
        self.pump_net(dt);
        self.state = match std::mem::replace(&mut self.state, State::Title) {
            State::Title => {
                self.title(input, dt);
                return;
            }
            State::Harvest(mut h) if self.page == Page::Settings => {
                let owner = h.menu_player().checked_sub(1).and_then(|o| self.others.get(o)).filter(|_| h.views() > 1);
                let controls = self.save.controls;
                let theirs = owner.map(|i| Input { held: controls.apply(i.held), pressed: controls.apply(i.pressed), ..*i });
                let input = theirs.as_ref().unwrap_or(input);
                if self.settings(input, input.touch.filter(|_| input.touch_started)) {
                    self.page = Page::Main;
                }
                self.hold_online(&h);
                h.set_one_screen(self.one_screen());
                h.set_corner_map(self.save.corner_map);
                State::Harvest(h)
            }
            State::Harvest(mut h) => {
                h.set_corner_map(self.save.corner_map);
                if self.phone.is_some() {
                    h.set_one_screen(true);
                    h.set_touch_aim(self.save.touch_grenades && self.has_touch());
                }
                h.set_touch_keys(self.phone.is_some() && self.has_touch() && !self.touchpad.hidden);
                h.set_saves_photos(self.captures);
                h.set_taps(self.phone.is_none() || self.touch_screen());
                let controls = self.save.controls;
                let others: Vec<Input> = self.others.iter().map(|i| Input { held: controls.apply(i.held), pressed: controls.apply(i.pressed), ..*i }).collect();
                h.set_inputs(&others);
                let flow = match self.step_online(&mut h, input, &others, dt) {
                    Some(flow) => flow,
                    None => h.update(input, dt, &mut self.sounds),
                };
                if flow.capture && self.captures {
                    self.shot = true;
                    self.sounds.push(Sfx::Tick(LEVELS));
                } else if flow.capture {
                    h.toggle_guides();
                }
                if flow.settings {
                    self.page = Page::Settings;
                    self.open_settings(true);
                }
                for (seat, r) in flow.rumble.into_iter().enumerate().take(h.views()) {
                    if let Some(r) = r {
                        self.buzz(seat, r);
                    }
                }
                if let Some(r) = self.save.records_mut(h.difficulty()) {
                    r.found |= flow.found;
                }
                if let Some(run) = flow.ended {
                    let p = self.progress_at(run.difficulty).after(run);
                    if let Some(r) = self.save.records_mut(run.difficulty) {
                        (r.kills, r.best, r.best_level, r.nights, r.dawns, r.breakers, r.breaker_dawns) = (p.kills, p.best, p.best_level, p.nights, p.dawns, p.breakers, p.breaker_dawns);
                    }
                    self.save.record(run);
                }
                if flow.quit {
                    self.replays.extend(h.stop_recording());
                    State::Title
                } else {
                    State::Harvest(h)
                }
            }
            State::Gallery(mut g) => {
                g.update(input, dt);
                State::Gallery(g)
            }
            State::Icon(mut i) => {
                i.update(input, dt);
                State::Icon(i)
            }
            State::Watch(mut w) => {
                let top = self.watch_strip_top().unwrap_or(0.0);
                let touch_keys = self.phone.is_some() && self.has_touch() && !self.touchpad.hidden;
                w.set_device(replay::Device { one_screen: self.one_screen(), touch_keys, saves_photos: self.captures, taps: self.phone.is_none() || self.touch_screen() });
                let going = w.update(input, input.touch.filter(|_| input.touch_started), top, dt, &mut self.sounds);
                let capture = w.take_capture();
                if capture && self.captures {
                    self.shot = true;
                    self.sounds.push(Sfx::Tick(LEVELS));
                } else if capture {
                    w.toggle_guides();
                }
                if going {
                    State::Watch(w)
                } else {
                    self.page = Page::Replays;
                    State::Title
                }
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

    /// Draws with what moved in the last step `alpha` of the way from where it was, for a display faster than the step.
    pub fn draw_at(&mut self, alpha: f32) -> Frame {
        let clean = self.clean();
        if let State::Watch(w) = &mut self.state {
            let mut night = Frame::default();
            w.draw(alpha, &mut night, clean);
            let over = self.draw();
            for (to, from) in [(&mut night.top, &over.top), (&mut night.bottom, &over.bottom)] {
                to.clip(None);
                to.anchor([0.5, 0.5]);
                to.place(from, [0.0, 0.0], 1.0);
            }
            night.phone = over.phone;
            return night;
        }
        let undo = match &mut self.state {
            State::Harvest(h) => h.blend(alpha),
            _ => None,
        };
        let fitted = |v: usize| self.view_sizes.get(v).map(|&size| fit(size));
        if let State::Harvest(h) = &mut self.state
            && h.views() > 1
        {
            h.set_canvas(fitted(0));
        }
        let mut frame = self.draw();
        let clean = self.clean();
        if let State::Harvest(h) = &mut self.state
            && h.views() > 1
        {
            frame.views = (1..h.views()).map(|v| {
                let mut f = Frame::default();
                h.set_canvas(fitted(v));
                h.draw_view(v, &mut f, clean);
                f.top
            }).collect();
            if h.followed(0) != 0 {
                let mut f = Frame::default();
                h.set_canvas(fitted(0));
                h.draw_view(0, &mut f, clean);
                frame.top = f.top;
            }
            h.set_canvas(None);
            let owner = h.menu_player();
            if self.page == Page::Settings
                && let Some(c) = std::iter::once(&mut frame.top).chain(&mut frame.views).nth(owner)
            {
                c.anchor([0.5, 0.5]);
                self.draw_settings(c);
            }
            for (canvas, &size) in std::iter::once(&mut frame.top).chain(&mut frame.views).zip(&self.view_sizes) {
                canvas.refit(fit(size));
            }
        }
        if let (State::Harvest(h), Some(undo)) = (&mut self.state, undo) {
            h.unblend(undo);
        }
        frame
    }

    pub fn draw(&self) -> Frame {
        let mut f = Frame::default();
        f.top.clear = [0.08, 0.08, 0.1, 1.0];
        f.bottom.clear = [0.12, 0.12, 0.15, 1.0];
        match &self.state {
            State::Title => self.draw_title(&mut f),
            State::Harvest(h) => {
                h.draw(&mut f, self.clean());
                if self.page == Page::Settings && h.views() == 1 {
                    self.draw_settings(if h.one_screen() { &mut f.top } else { &mut f.bottom });
                }
            }
            State::Gallery(g) => g.draw(&mut f),
            State::Icon(i) => i.draw(&mut f),
            State::Watch(w) if w.in_photo() => {}
            State::Watch(w) => {
                let hints = !self.is_phone() || !self.has_touch() || self.touchpad.hidden;
                w.draw_banner(&mut f.top);
                match self.watch_strip_top() {
                    Some(top) => w.draw_strip(&mut f.top, top, hints),
                    None => w.draw_strip(&mut f.bottom, 0.0, hints),
                }
            }
            State::Hardware(hw) => {
                hw.draw(&mut f, &self.system, self.save.brightness, self.leds);
                osd::draw(&mut f.top, &self.input, self.rumble(), self.system.headphones);
                osd::touches(&mut f.bottom, &self.input);
            }
        }
        if self.paused {
            f.top.text_centered(360.0, 16.0, WHITE, "PAUSED");
        }
        let clean = self.clean();
        if self.volume_shown > 0.0 && !clean {
            f.top.text_centered(40.0, 8.0, WHITE, &format!("VOL {}", self.save.volume));
        }
        if self.shot_flash > 0.0 && !clean {
            let a = self.shot_flash / SHOT_FLASH * 0.7;
            f.top.rect(0.0, 0.0, SCREEN_W, SCREEN_H, [1.0, 1.0, 1.0, a]);
            f.bottom.rect(0.0, 0.0, SCREEN_W, SCREEN_H, [1.0, 1.0, 1.0, a]);
        }
        if self.save.fps && !clean {
            let top = if matches!(self.state, State::Harvest(_)) { PERF_UNDER_KILLS } else { 8.0 };
            system::draw_perf(&mut f.top, &self.system, top);
        } else if matches!(self.state, State::Title) {
            system::draw_battery(&mut f.top, &self.system, 16.0);
        }
        if self.title_on_one() {
            let menu = std::mem::take(&mut f.bottom);
            let (at, k) = TITLE_MENU;
            f.top.rect(at[0] + 190.0 * k, at[1], (SCREEN_W - 380.0) * k, SCREEN_H * k, [0.08, 0.08, 0.1, 0.45]);
            f.top.place(&menu, at, k);
        }
        if let Some(size) = self.phone {
            let layout = self.phone_layout(size);
            let pad = if clean { Canvas { clear: [0.0; 4], size: Some(size), ..Default::default() } } else { self.touchpad.draw(&layout) };
            f.phone = Some(touch::Phone { size, top: layout.top, bottom: layout.bottom, pad });
        }
        f
    }

    /// The title's line of the dead, side on: each kind turned on once, the last kind on between them.
    fn title_walkers(&self) -> Vec<&'static [art::Art]> {
        let r = harvest::roster();
        let on: Vec<usize> = (0..r.enemies.len()).filter(|&i| self.save.enemies_off & 1 << i == 0).collect();
        let Some(&filler) = on.last() else { return Vec::new() };
        let side = |i: usize| r.enemies[i].look.walk[1].as_slice();
        let mut line = vec![side(filler)];
        for &i in on.iter().rev().skip(1).take(MAX_TITLE_KINDS) {
            line.extend([side(i), side(filler)]);
        }
        line
    }

    /// The title's top screen, whatever page is open, with the version in its corner.
    fn draw_title(&self, f: &mut Frame) {
        self.draw_title_top(f);
        let report = data::load_report();
        if !report.version.is_empty() {
            f.top.label(format!("Version {}", report.versions()), 16.0, 12.0, 24.0, DIM, Align::Left);
        }
        if let Some(joined) = joined(self.players) {
            f.top.label(format!("{joined} joined. Start again on a joined pad to leave"), SCREEN_W / 2.0, 736.0, 24.0, HOT, Align::Center);
        }
    }

    fn draw_title_top(&self, f: &mut Frame) {
        let t = self.clock;
        f.top.clear = [0.06, 0.07, 0.11, 1.0];
        if self.page == Page::Enemies {
            self.draw_enemy(&mut f.top);
            self.draw_menu(f);
            return;
        }
        if self.page == Page::Settings && self.settings_list().get(self.setting) == Some(&menu::Setting::Player) {
            self.draw_player(&mut f.top);
            self.draw_menu(f);
            return;
        }
        // On one screen the logo takes the top and a bigger moon sits under it, beside the menu.
        let one = self.title_on_one();
        let ([mx, my], mk, logo_y) = if one { ([800.0, 100.0], 2.0, 52.0) } else { ([840.0, 120.0], 1.0, 240.0) };
        // Behind the logo the moon is dimmed, so the light letters stay readable over it.
        let lit = |c: [f32; 3], a: f32| if one { [c[0] * 0.5, c[1] * 0.5, c[2] * 0.55, a] } else { [c[0], c[1], c[2], a] };
        f.top.circle(mx, my, 90.0 * mk, lit([0.8, 0.8, 0.7], 0.06));
        f.top.circle(mx, my, 48.0 * mk, lit([0.85, 0.85, 0.75], 1.0));
        f.top.circle(mx - 18.0 * mk, my - 12.0 * mk, 10.0 * mk, lit([0.75, 0.75, 0.66], 1.0));
        f.top.circle(mx + 16.0 * mk, my + 16.0 * mk, 7.0 * mk, lit([0.75, 0.75, 0.66], 1.0));
        f.top.rect(0.0, 600.0, SCREEN_W, 168.0, [0.1, 0.08, 0.07, 1.0]);
        let walkers = self.title_walkers();
        for (i, frames) in walkers.iter().enumerate() {
            let span = SCREEN_W + 400.0;
            let x = (i as f32 * span / walkers.len() as f32 + t * 22.0) % span - 200.0;
            let a = frames[(t * 5.0 + i as f32) as usize % frames.len()];
            // Side frames face left, as in the 3D view: flipped, they walk the way they move.
            f.top.art(a, x, 600.0 - a.h() * 8.0 + 8.0, 8.0, true, [0.02, 0.02, 0.03, 1.0]);
        }
        for i in 0..30 {
            let seed = i as f32 * 7.31;
            let x = (fm::sin(seed) * 9000.0).fract().abs() * SCREEN_W + fm::sin(t * 0.3 + seed) * 30.0;
            let y = (fm::cos(seed) * 7000.0).fract().abs() * 560.0 + fm::cos(t * 0.4 + seed * 2.0) * 20.0;
            let glow = fm::powi(fm::sin(t * 1.7 + seed) * 0.5 + 0.5, 3);
            f.top.circle(x, y, 3.0 + 3.0 * glow, [0.75, 1.0, 0.45, 0.2 + 0.7 * glow]);
        }
        let flicker = 0.85 + 0.15 * (fm::sin(t * 13.0) * fm::sin(t * 7.3)).abs();
        let bob = if one { 0.0 } else { fm::sin(t * 1.2) * 4.0 };
        f.top.text_centered(logo_y + 8.0 + bob, 20.0, [0.0, 0.0, 0.0, 0.7], "LAST HARVEST");
        if one {
            let x = (SCREEN_W - crate::font::width("LAST HARVEST") * 20.0) / 2.0;
            for (dx, dy) in [(-6.0, 0.0), (6.0, 0.0), (0.0, -6.0), (0.0, 6.0)] {
                f.top.text(x + dx, logo_y + dy, 20.0, [0.04, 0.04, 0.07, 1.0], "LAST HARVEST");
            }
        }
        f.top.text_centered(logo_y + bob, 20.0, [flicker, flicker * 0.92, flicker * 0.8, 1.0], "LAST HARVEST");
        // On one screen the menu sits under the logo, so the records stay on the Stats page and warnings go between.
        let warn_y = if one { 160.0 } else { 696.0 };
        if !one {
            self.draw_title_records(f);
        }
        if let Some(missing) = &data::load_report().missing_pack {
            let red = [1.0, 0.55, 0.5, 1.0];
            f.top.label("No game data: the game can't draw without game_data.pak.", SCREEN_W / 2.0, warn_y, 24.0, red, Align::Center);
            f.top.label(menu::fitted(missing, 22.0, SCREEN_W - 40.0, 22.0).0, SCREEN_W / 2.0, warn_y + 32.0, 22.0, red, Align::Center);
        } else if let Some((count, first)) = data::problems() {
            let more = if count > 1 { format!("{count} files couldn't load:") } else { "A file couldn't load:".to_string() };
            let red = [1.0, 0.55, 0.5, 1.0];
            f.top.label(more, SCREEN_W / 2.0, warn_y, 24.0, red, Align::Center);
            f.top.label(menu::fitted(&first, 22.0, SCREEN_W - 40.0, 22.0).0, SCREEN_W / 2.0, warn_y + 32.0, 22.0, red, Align::Center);
        }
        self.draw_menu(f);
    }

    /// The title's tagline and records under the logo.
    fn draw_title_records(&self, f: &mut Frame) {
        f.top.label("Survive the night", SCREEN_W / 2.0, 400.0, 44.0, DIM, Align::Center);
        let best = self.save.records(self.save.difficulty).best;
        if best > 0 {
            let line = format!("Longest night: {}:{:02}", best / 60, best % 60);
            f.top.label(line, SCREEN_W / 2.0, 480.0, 34.0, HOT, Align::Center);
        }
        let (open, all, waiting) = harvest::unlocked(&self.progress());
        let line = match waiting {
            0 => format!("Powerups unlocked: {open} of {all}"),
            1 => format!("Powerups unlocked: {open} of {all}.  1 waiting to be found"),
            n => format!("Powerups unlocked: {open} of {all}.  {n} waiting to be found"),
        };
        f.top.label(line, SCREEN_W / 2.0, 530.0, 28.0, DIM, Align::Center);
        if self.reset_shown > 0.0 {
            let a = self.reset_shown.min(1.0);
            f.top.label("Progress erased: records, kills and found powerups", SCREEN_W / 2.0, 480.0, 34.0, [1.0, 0.45, 0.4, a], Align::Center);
        }
        if let Some((name, how)) = harvest::next_unlock(&self.progress()) {
            f.top.label(format!("Next to find: {name}. {how}"), SCREEN_W / 2.0, 566.0, 24.0, DIM, Align::Center);
        }
    }
}

/// The size a 1024x768 drawing is laid out to for a view `[w, h]` pixels across and down: as wide or as tall as needed.
fn fit([w, h]: [f32; 2]) -> [f32; 2] {
    let aspect = w / h.max(1.0);
    if aspect >= SCREEN_W / SCREEN_H { [SCREEN_H * aspect, SCREEN_H] } else { [SCREEN_W, SCREEN_W / aspect.max(0.01)] }
}

/// Players 2 and up, named for the title: "P2", "P2 and P3", "P2, P3 and P4".
fn joined(players: usize) -> Option<String> {
    let names: Vec<String> = (2..=players).map(|n| format!("P{n}")).collect();
    let (last, rest) = names.split_last()?;
    Some(if rest.is_empty() { last.clone() } else { format!("{} and {last}", rest.join(", ")) })
}

/// Where the frame-rate overlay starts during a run, below the kill count.
const PERF_UNDER_KILLS: f32 = 72.0;
/// Seconds the screenshot flash lasts.
const SHOT_FLASH: f32 = 0.25;

#[cfg(test)]
mod tests {
    use super::*;
    use menu::{DIFFICULTY_Y, RESET_CONFIRM};

    #[test]
    fn a_run_with_two_seated_shows_two_views_each_laid_out_to_its_size() {
        let mut game = Game::new(4);
        game.set_players(2);
        game.open(Scene::Harvest);
        assert_eq!(game.views(), 2);
        game.set_view_sizes(vec![[640.0, 720.0], [640.0, 720.0]]);
        let f = game.draw_at(0.0);
        assert_eq!(f.views.len(), 1, "player 2's view");
        assert_eq!((f.top.size, f.views[0].size), (Some([1024.0, 1152.0]), Some([1024.0, 1152.0])));
        let mut solo = Game::new(4);
        solo.open(Scene::Harvest);
        assert!(solo.views() == 1 && solo.draw_at(0.0).views.is_empty() && solo.draw_at(0.0).top.size.is_none(), "one player draws as ever");
    }

    #[test]
    fn settings_from_player_twos_pause_are_theirs_to_work_and_show_in_their_view() {
        let mut game = Game::new(4);
        game.set_phone(Some([1280.0, 720.0]));
        game.set_players(2);
        game.open(Scene::Harvest);
        game.set_view_sizes(vec![[640.0, 720.0], [640.0, 720.0]]);
        let both = |game: &mut Game, one: Buttons, two: Buttons| {
            game.update_all(&[Input { pressed: one, held: one, ..Default::default() }, Input { pressed: two, held: two, ..Default::default() }], 0.016);
        };
        for b in [Buttons::START, Buttons::DOWN, Buttons::A] {
            both(&mut game, Buttons::NONE, b);
        }
        assert_eq!((game.page, game.menu_view()), (Page::Settings, 1));
        both(&mut game, Buttons::DOWN, Buttons::NONE);
        assert_eq!(game.setting, 0, "player 1's pad leaves player 2's Settings alone");
        both(&mut game, Buttons::NONE, Buttons::DOWN);
        assert_eq!(game.setting, 1);
        let f = game.draw_at(0.0);
        let says = |c: &Canvas| c.shapes.iter().any(|s| matches!(s, Shape::Text { text, .. } if text == "Settings"));
        assert!(says(&f.views[0]) && !says(&f.top), "in player 2's view only");
        both(&mut game, Buttons::NONE, Buttons::B);
        assert_eq!(game.page, Page::Main);
    }

    #[test]
    fn each_seat_buzzes_on_its_own_and_rumble_off_stills_them_all() {
        let mut game = Game::new(4);
        game.buzz(1, harvest::Rumble::Hit);
        assert!(game.rumble_for(1) && !game.rumble_for(0) && !game.outputs().rumble, "player 2's pad alone");
        game.update(&Input::default(), 0.3);
        assert!(!game.rumble_for(1), "the pulse ends");
        game.buzz(0, harvest::Rumble::Blast);
        assert!(game.rumble_for(0) && game.outputs().rumble && !game.rumble_for(1));
        game.save.rumble = false;
        assert!(!game.rumble_for(0) && !game.outputs().rumble);
    }

    #[test]
    fn a_tall_views_map_fills_it_from_top_to_bottom() {
        let mut game = Game::new(4);
        game.set_phone(Some([1280.0, 720.0]));
        game.set_players(2);
        game.open(Scene::Harvest);
        game.set_view_sizes(vec![[640.0, 720.0], [640.0, 720.0]]);
        for _ in 0..2 {
            game.update(&Input { held: Buttons::L1, ..Default::default() }, 0.016);
        }
        let f = game.draw_at(0.0);
        let tiles: Vec<f32> = f.top.shapes.iter().filter_map(|s| if let Shape::Art { y, .. } = s { Some(*y) } else { None }).collect();
        let (top, bottom) = (tiles.iter().copied().fold(f32::MAX, f32::min), tiles.iter().copied().fold(0.0, f32::max));
        assert_eq!(f.top.size, Some([1024.0, 1152.0]));
        assert!(top <= 0.0 && bottom >= 1152.0 - 64.0, "the map reaches the view's edges: {top}..{bottom}");
    }

    #[test]
    fn a_split_past_the_seated_players_fills_with_scripted_ones() {
        let mut game = Game::new(4);
        game.add_bots(1);
        game.set_split(2);
        game.open(Scene::Harvest);
        assert_eq!(game.views(), 2);
    }

    #[test]
    fn the_title_names_the_players_who_joined() {
        assert_eq!([1, 2, 3, 4].map(joined), [None, Some("P2".into()), Some("P2 and P3".into()), Some("P2, P3 and P4".into())]);
        let mut game = Game::new(1);
        let says = |game: &Game| game.draw().top.shapes.iter().any(|s| matches!(s, Shape::Text { text, .. } if text.contains("joined")));
        assert!(!says(&game) && game.at_title() && !game.in_run());
        game.set_players(3);
        assert!(says(&game));
    }

    fn press(game: &mut Game, b: Buttons) {
        game.update(&Input { pressed: b, ..Default::default() }, 0.016);
    }

    #[test]
    fn on_a_phone_a_run_has_no_map_key_and_photo_mode_has_change_for_r2() {
        let size = [1080.0, 2400.0];
        let mut game = Game::new(1);
        game.set_phone(Some(size));
        press(&mut game, Buttons::A);
        assert!(matches!(game.state, State::Harvest(_)));
        assert_eq!(game.phone_layout(size).key(Buttons::R2), None, "the paused whole map is gone");
        for b in [Buttons::START, Buttons::DOWN, Buttons::DOWN, Buttons::A] {
            press(&mut game, b);
        }
        assert!(game.in_photo() && game.phone_layout(size).key(Buttons::R2).is_some(), "photo mode still changes its setting");
    }

    #[test]
    fn on_a_phone_the_map_key_flips_the_map_up_and_a_tap_on_it_throws() {
        let size = [2400.0, 1080.0];
        let mut game = Game::new(1);
        game.set_phone(Some(size));
        press(&mut game, Buttons::A);
        assert!(matches!(game.state, State::Harvest(_)));
        let frame = game.draw();
        let phone = frame.phone.expect("a phone frame");
        assert!(phone.top.is_some() && phone.bottom.is_none(), "a run shows one screen");
        let layout = touch::layout(size, true, true, false);
        let [kx, ky, ..] = layout.key(Buttons::L2).unwrap();
        let [px, py, pw, ph] = layout.top.unwrap();
        let key = Some([kx + 4.0, ky + 4.0]);
        let mut touches = [None; MAX_TOUCHES];
        touches[0] = key;
        game.update(&Input { touches, ..Default::default() }, 0.016);
        game.update(&Input::default(), 0.016);
        game.take_sounds();
        touches[0] = Some([px + pw * 0.6, py + ph * 0.4]);
        game.update(&Input { touches, ..Default::default() }, 0.016);
        let flying = |g: &Game| match &g.state {
            State::Harvest(h) => h.grenades_in_the_air(),
            _ => 0,
        };
        assert_eq!(flying(&game), 1, "a tap on the map the key put up throws");
        game.update(&Input::default(), 0.016);
        touches[0] = key;
        game.update(&Input { touches, ..Default::default() }, 0.016);
        game.update(&Input::default(), 0.016);
        touches[0] = Some([px + pw * 0.6, py + ph * 0.4]);
        game.update(&Input { touches, ..Default::default() }, 0.016);
        assert_eq!(flying(&game), 1, "a tap on the 3D view does not");
    }

    #[test]
    fn on_a_phone_holding_the_grenade_key_throws_ahead_and_longer_throws_further() {
        let size = [2400.0, 1080.0];
        let layout = touch::layout(size, true, true, false);
        let [gx, gy, gw, gh] = layout.key(Buttons::R1).expect("a Grenade key for quick throws");
        let hold_at = Some([gx + gw / 2.0, gy + gh / 2.0]);
        let throw_after = |frames: usize| {
            let mut game = Game::new(1);
            game.set_phone(Some(size));
            press(&mut game, Buttons::A);
            let mut touches = [None; MAX_TOUCHES];
            touches[0] = hold_at;
            for _ in 0..frames {
                game.update(&Input { touches, ..Default::default() }, 0.016);
            }
            game.update(&Input::default(), 0.016);
            match &game.state {
                State::Harvest(h) => h.last_thrown_distance().expect("a thrown grenade"),
                _ => panic!("not in a run"),
            }
        };
        let short = throw_after(1);
        let long = throw_after(45);
        assert!(long > short + 30.0, "holding longer throws further: short={short}, long={long}");
    }

    #[test]
    fn on_a_phone_the_grenade_key_does_not_reroll_the_level_up_cards() {
        let size = [2400.0, 1080.0];
        let mut game = Game::new(1);
        game.set_phone(Some(size));
        press(&mut game, Buttons::A);
        let State::Harvest(h) = &mut game.state else { panic!("not in a run") };
        h.deal_cards_with_rerolls(1);
        let [gx, gy, gw, gh] = touch::layout(size, true, true, false).key(Buttons::R1).expect("a Grenade key");
        let mut touches = [None; MAX_TOUCHES];
        touches[0] = Some([gx + gw / 2.0, gy + gh / 2.0]);
        game.update(&Input { touches, ..Default::default() }, 0.016);
        game.update(&Input::default(), 0.016);
        let State::Harvest(h) = &game.state else { panic!("not in a run") };
        assert!(h.choosing() && h.rerolls_left() == 1, "a tap on Grenade spent a reroll");
        press(&mut game, Buttons::R1);
        let State::Harvest(h) = &game.state else { panic!("not in a run") };
        assert_eq!(h.rerolls_left(), 0, "a gamepad's R1 still rerolls");
    }

    #[test]
    fn a_phone_sets_the_grenade_style_where_others_set_the_screens() {
        let mut game = Game::new(1);
        assert!(game.settings_list().contains(&menu::Setting::Screens) && !game.settings_list().contains(&menu::Setting::Grenades));
        game.set_phone(Some([2400.0, 1080.0]));
        let list = game.settings_list();
        assert!(list.contains(&menu::Setting::Grenades) && !list.contains(&menu::Setting::Screens));
        game.set_phone(None);
        game.set_backlights(false);
        let list = game.settings_list();
        assert!(!list.contains(&menu::Setting::TopBrightness) && !list.contains(&menu::Setting::BottomBrightness), "brightness only where the platform sets the backlights");
        assert!(game.save.touch_grenades, "touch by default");
        let save = Save { touch_grenades: false, ..game.save };
        assert!(!Save::parse(&save.to_text()).touch_grenades);
    }

    #[test]
    fn settings_turn_the_on_screen_controls_auto_on_or_off() {
        let size = [2400.0, 1080.0];
        let mut game = Game::new(1);
        game.set_phone(Some(size));
        let at = game.settings_list().iter().position(|&s| s == menu::Setting::Touch).expect("a phone offers the controls");
        settings_row(&mut game, at);
        assert_eq!(game.save.touch, TouchControls::Auto, "Auto by default");
        press(&mut game, Buttons::RIGHT);
        assert_eq!(game.save.touch, TouchControls::On);
        press(&mut game, Buttons::A);
        assert_eq!(game.save.touch, TouchControls::Off);
        assert!(!game.has_touch() && !game.settings_list().contains(&menu::Setting::Grenades), "Off leaves out the grenade style");
        assert_eq!(Save::parse(&game.save.to_text()).touch, TouchControls::Off);
        press(&mut game, Buttons::B);
        let phone = game.draw().phone.unwrap();
        assert!(phone.top.is_some() && phone.bottom.is_none(), "the title as without touch");
        for _ in 0..3 {
            press(&mut game, Buttons::UP);
        }
        press(&mut game, Buttons::A);
        let State::Harvest(_) = &game.state else { panic!("a run starts") };
        assert_eq!(game.phone_layout(size).key(Buttons::L2), None, "a run without the controls");

        let walk = Input { held: Buttons::RIGHT, ..Default::default() };
        for (touch, hidden) in [(TouchControls::Auto, true), (TouchControls::On, false)] {
            game.load(Save { touch, ..game.save });
            game.touchpad.hidden = false;
            game.update(&walk, 0.016);
            assert!(game.phone_layout(size).key(Buttons::L2).is_some());
            assert_eq!(game.touchpad.hidden, hidden, "{touch:?}: the pad hides the controls only in Auto");
        }
        game.touchpad.hidden = true;
        game.update(&walk, 0.016);
        assert!(!game.touchpad.hidden, "On brings them back");

        game.set_touch(false);
        game.load(Save { touch: TouchControls::On, ..game.save });
        assert!(!game.has_touch(), "a screen without touch has no controls whatever Settings says");
        game.page = Page::Settings;
        game.open_settings(true);
        assert!(!game.settings_list().contains(&menu::Setting::Touch), "nor the row");
    }

    fn finger(game: &mut Game, at: [f32; 2]) {
        let mut touches = [None; MAX_TOUCHES];
        touches[0] = Some(at);
        game.update(&Input { touches, ..Default::default() }, 0.016);
        game.update(&Input::default(), 0.016);
    }

    #[test]
    fn off_comes_back_to_auto_at_a_touch_until_a_button_or_stick_is_used() {
        let mut game = Game::new(1);
        game.set_phone(Some([720.0, 480.0]));
        game.load(Save { touch: TouchControls::Off, ..Save::default() });
        finger(&mut game, [10.0, 10.0]);
        assert_eq!(game.save().touch, TouchControls::Auto, "a touch-only player is never stranded");

        let mut game = Game::new(1);
        game.set_phone(Some([720.0, 480.0]));
        press(&mut game, Buttons::DOWN);
        game.load(Save { touch: TouchControls::Off, ..Save::default() });
        finger(&mut game, [10.0, 10.0]);
        assert_eq!(game.save().touch, TouchControls::Off, "a pad player's Off stays");
    }

    #[test]
    fn a_tap_on_the_title_menu_over_the_picture_lands_where_it_is_drawn() {
        let size = [720.0, 480.0];
        let mut game = Game::new(1);
        game.set_phone(Some(size));
        press(&mut game, Buttons::DOWN);
        press(&mut game, Buttons::UP);
        game.load(Save { touch: TouchControls::Off, ..Save::default() });
        assert!(game.title_on_one());
        let ([ax, ay], k) = TITLE_MENU;
        let settings = [SCREEN_W / 2.0, menu::ROWS_TOP + 2.0 * menu::ROW_H + 20.0];
        let on_top = [ax + settings[0] * k, ay + settings[1] * k];
        let [px, py, pw, ph] = game.phone_layout(size).top.unwrap();
        finger(&mut game, [px + on_top[0] / SCREEN_W * pw, py + on_top[1] / SCREEN_H * ph]);
        assert_eq!(game.page, Page::Settings);
        assert_eq!(title_menu_touch([ax - 1.0, ay]), None, "a tap beside the menu misses it");
    }

    fn recorded_night(ticks: usize) -> Vec<u8> {
        recorded_night_with(ticks, 0)
    }

    fn recorded_night_with(ticks: usize, bots: usize) -> Vec<u8> {
        let mut game = Game::new(11);
        game.add_bots(bots);
        game.record_replays(vec![("game".into(), "test".into())]);
        game.open(Scene::Harvest);
        for i in 0..ticks {
            let held = if i % 200 < 100 { Buttons::UP } else { Buttons::LEFT | Buttons::UP };
            game.update(&Input { held, ..Default::default() }, 1.0 / 60.0);
        }
        game.open(Scene::Title);
        game.take_replays().pop().expect("a replay")
    }

    #[test]
    fn stats_opens_the_replays_and_a_replay_plays_to_its_end_without_touching_the_save() {
        let raw = recorded_night(600);
        let mut game = Game::new(3);
        for _ in 0..4 {
            press(&mut game, Buttons::DOWN);
        }
        press(&mut game, Buttons::A);
        assert_eq!(game.page, Page::Stats);
        press(&mut game, Buttons::X);
        assert_eq!(game.page, Page::Replays);
        assert!(game.take_replays_request() && !game.take_replays_request(), "asked once");
        game.set_saved_replays(vec![("night-1795237918-00.lhr".into(), raw)]);
        assert!(game.saved_replays[0].line.as_ref().is_ok_and(|l| l.starts_with("2026-11-21 05:11")), "{:?}", game.saved_replays[0].line);
        game.record_replays(vec![("game".into(), "test".into())]);
        let save = game.save().to_text();
        press(&mut game, Buttons::A);
        assert!(matches!(game.state, State::Watch(_)));
        press(&mut game, Buttons::RIGHT);
        press(&mut game, Buttons::RIGHT);
        for _ in 0..200 {
            game.update(&Input::default(), 1.0 / 60.0);
            game.draw_at(0.5);
        }
        let State::Watch(w) = &game.state else { panic!("still watching") };
        assert!(w.ended() && w.drift().is_none(), "600 ticks at 4x in 200 frames, matching");
        assert_eq!(game.save().to_text(), save, "the save is untouched");
        assert!(game.take_replays().is_empty(), "watching records nothing");
        press(&mut game, Buttons::B);
        assert!(matches!(game.state, State::Title) && game.page == Page::Replays);
    }

    #[test]
    fn a_replay_that_drifts_stops_where_it_drifted() {
        let mut raw = recorded_night(300);
        let at = raw.len() - 300 + 120;
        raw[at] ^= 0xff;
        let mut game = Game::new(3);
        game.set_saved_replays(vec![("night-1795237918-00.lhr".into(), raw)]);
        game.page = Page::Replays;
        press(&mut game, Buttons::A);
        for _ in 0..300 {
            game.update(&Input::default(), 1.0 / 60.0);
        }
        let State::Watch(w) = &game.state else { panic!("watching") };
        assert_eq!(w.drift().map(|d| d.tick), Some(120));
        assert!(w.paused() && !w.ended(), "it stops at the drift");
        let f = game.draw_at(1.0);
        assert!(f.bottom.shapes.iter().chain(&f.top.shapes).any(|s| matches!(s, Shape::Text { text, .. } if text.starts_with("Drifted"))), "and says where");
    }

    #[test]
    fn a_replay_with_more_players_follows_the_one_chosen_and_still_matches() {
        let raw = recorded_night_with(400, 1);
        let mut game = Game::new(3);
        game.set_saved_replays(vec![("night-1795237918-00.lhr".into(), raw)]);
        game.page = Page::Replays;
        press(&mut game, Buttons::A);
        press(&mut game, Buttons::R1);
        for _ in 0..450 {
            game.update(&Input::default(), 1.0 / 60.0);
            game.draw_at(0.5);
        }
        let f = game.draw_at(1.0);
        assert!(f.top.shapes.iter().any(|s| matches!(s, Shape::Text { text, .. } if text.contains("P2"))), "names whose view it is");
        let State::Watch(w) = &game.state else { panic!("watching") };
        assert!(w.ended() && w.drift().is_none(), "drawing as P2 changes nothing");
    }

    #[test]
    fn a_tap_on_the_replay_strip_works_its_buttons() {
        let raw = recorded_night(120);
        let mut game = Game::new(3);
        game.set_saved_replays(vec![("night-1795237918-00.lhr".into(), raw)]);
        game.page = Page::Replays;
        press(&mut game, Buttons::A);
        let tap = |game: &mut Game, x: f32, y: f32| game.update(&Input { touch: Some([x, y]), touch_started: true, ..Default::default() }, 1.0 / 60.0);
        tap(&mut game, 430.0, 90.0);
        let State::Watch(w) = &game.state else { panic!("watching") };
        assert!(w.paused(), "the third button pauses");
        tap(&mut game, SCREEN_W - 100.0, 90.0);
        assert!(game.in_photo(), "the last opens photo mode");
        press(&mut game, Buttons::B);
        assert!(!game.in_photo() && matches!(game.state, State::Watch(_)), "B leaves photo mode, not the replay");
        tap(&mut game, 100.0, 90.0);
        assert!(matches!(game.state, State::Title), "the first leaves");
    }

    fn watching(raw: Vec<u8>) -> Game {
        let mut game = Game::new(3);
        game.set_saved_replays(vec![("night-1795237918-00.lhr".into(), raw)]);
        game.page = Page::Replays;
        press(&mut game, Buttons::A);
        assert!(matches!(game.state, State::Watch(_)));
        game
    }

    fn watched(game: &Game) -> &watch::Watching {
        let State::Watch(w) = &game.state else { panic!("watching") };
        w
    }

    fn hold_for(game: &mut Game, held: Buttons, frames: usize) {
        for _ in 0..frames {
            game.update(&Input { held, ..Default::default() }, 1.0 / 60.0);
            game.draw_at(1.0);
        }
    }

    /// Opens photo mode on the replay where it stands, walks, poses, drops props and changes the time, then leaves.
    fn photo_on_the_replay(game: &mut Game) {
        game.set_captures(true);
        press(game, Buttons::X);
        assert!(game.in_photo() && watched(game).paused(), "X pauses and opens photo mode");
        let f = game.draw_at(1.0);
        assert!(f.top.shapes.iter().chain(&f.bottom.shapes).any(|s| matches!(s, Shape::Text { text, .. } if text == "PHOTO MODE")), "with its guides");
        press(game, Buttons::R2);
        assert!(game.take_screenshot().is_none(), "R2 changes the setting instead of taking a screenshot");
        for _ in 0..6 {
            press(game, Buttons::L2);
        }
        hold_for(game, Buttons::UP, 90);
        hold_for(game, Buttons::RIGHT, 20);
        press(game, Buttons::L2);
        for _ in 0..3 {
            press(game, Buttons::L2);
        }
        press(game, Buttons::R2);
        hold_for(game, Buttons::LEFT | Buttons::UP, 10);
        press(game, Buttons::L2);
        press(game, Buttons::R2);
        press(game, Buttons::R2);
        press(game, Buttons::R1);
        assert_eq!(game.take_screenshot(), Some(Capture::Photo), "R1 takes a photo");
        press(game, Buttons::B);
        assert!(!game.in_photo() && watched(game).paused(), "B goes back to the paused replay");
    }

    #[test]
    fn photo_mode_on_a_paused_replay_leaves_it_exactly_where_it_was_and_it_plays_on_matching() {
        let mut game = watching(recorded_night(600));
        game.set_captures(true);
        hold_for(&mut game, Buttons::NONE, 200);
        press(&mut game, Buttons::A);
        let at = watched(&game).at();
        assert!(at.0 > 150, "{at:?}");
        photo_on_the_replay(&mut game);
        assert_eq!(watched(&game).at(), at, "the same tick, and nothing the replay checks moved");
        press(&mut game, Buttons::A);
        hold_for(&mut game, Buttons::NONE, 500);
        assert!(watched(&game).ended() && watched(&game).drift().is_none(), "it plays to its end matching the recording");
    }

    #[test]
    fn photo_mode_on_a_replay_paused_in_its_own_pause_menu_leaves_that_menu_open() {
        let mut game = Game::new(11);
        game.record_replays(vec![("game".into(), "test".into())]);
        game.open(Scene::Harvest);
        for i in 0..400 {
            let pressed = match i {
                100 => Buttons::START,
                160 => Buttons::A,
                _ => Buttons::NONE,
            };
            game.update(&Input { held: Buttons::UP, pressed, ..Default::default() }, 1.0 / 60.0);
        }
        game.open(Scene::Title);
        let mut game = watching(game.take_replays().pop().expect("a replay"));
        hold_for(&mut game, Buttons::NONE, 130);
        press(&mut game, Buttons::A);
        let at = watched(&game).at();
        assert!(at.2.is_some(), "the recording is in its pause menu: {at:?}");
        photo_on_the_replay(&mut game);
        assert_eq!(watched(&game).at(), at);
        press(&mut game, Buttons::A);
        hold_for(&mut game, Buttons::NONE, 400);
        assert!(watched(&game).ended() && watched(&game).drift().is_none());
    }

    #[test]
    fn photo_mode_on_a_replay_follows_the_player_on_show_and_still_matches() {
        let mut game = watching(recorded_night_with(400, 1));
        press(&mut game, Buttons::R1);
        hold_for(&mut game, Buttons::NONE, 150);
        let at = watched(&game).at();
        photo_on_the_replay(&mut game);
        assert_eq!(watched(&game).at(), at);
        press(&mut game, Buttons::A);
        hold_for(&mut game, Buttons::NONE, 400);
        assert!(watched(&game).ended() && watched(&game).drift().is_none());
    }

    #[test]
    fn a_night_recorded_walking_in_photo_mode_replays_and_photo_mode_over_it_puts_its_own_back() {
        let mut game = Game::new(11);
        game.record_replays(vec![("game".into(), "test".into())]);
        game.open(Scene::Harvest);
        let opens = [(60, Buttons::START), (61, Buttons::DOWN), (62, Buttons::DOWN), (63, Buttons::A)];
        let presses: Vec<(usize, Buttons)> = opens.into_iter().chain((64..70).map(|i| (i, Buttons::L2))).chain([(220, Buttons::B), (221, Buttons::B)]).collect();
        for i in 0..300 {
            let pressed = presses.iter().find(|p| p.0 == i).map_or(Buttons::NONE, |p| p.1);
            let held = if (80..200).contains(&i) { Buttons::UP | Buttons::RIGHT } else { Buttons::NONE };
            game.update(&Input { held, pressed, ..Default::default() }, 1.0 / 60.0);
        }
        assert!(!game.in_photo());
        game.open(Scene::Title);
        let mut game = watching(game.take_replays().pop().expect("a replay"));
        hold_for(&mut game, Buttons::NONE, 150);
        press(&mut game, Buttons::A);
        let says_photo = |game: &mut Game| game.draw_at(1.0).top.shapes.iter().chain(&game.draw_at(1.0).bottom.shapes).any(|s| matches!(s, Shape::Text { text, .. } if text == "PHOTO MODE"));
        assert!(says_photo(&mut game), "the recording is walking in its own photo mode");
        let at = watched(&game).at();
        photo_on_the_replay(&mut game);
        assert_eq!(watched(&game).at(), at);
        assert!(says_photo(&mut game), "and its own photo mode is back");
        press(&mut game, Buttons::A);
        hold_for(&mut game, Buttons::NONE, 200);
        assert!(watched(&game).ended() && watched(&game).drift().is_none(), "the walk and the photo mode over it replay matching");
    }

    #[test]
    fn paused_left_and_right_step_a_replay_a_tick_back_and_forth() {
        let mut game = watching(recorded_night(300));
        hold_for(&mut game, Buttons::NONE, 100);
        press(&mut game, Buttons::A);
        let at = watched(&game).at();
        press(&mut game, Buttons::RIGHT);
        let next = watched(&game).at();
        assert_eq!(next.0, at.0 + 1, "Right steps a tick");
        press(&mut game, Buttons::LEFT);
        assert_eq!(watched(&game).at(), at, "Left steps back to exactly where it was");
        press(&mut game, Buttons::LEFT);
        assert_eq!(watched(&game).at().0, at.0 - 1);
        assert!(watched(&game).paused() && watched(&game).drift().is_none());
        press(&mut game, Buttons::RIGHT);
        press(&mut game, Buttons::RIGHT);
        assert_eq!(watched(&game).at(), next, "and forward again to the same night");
    }

    #[test]
    fn on_a_phone_photo_mode_on_a_replay_has_the_photo_keys() {
        let size = [2400.0, 1080.0];
        let mut game = watching(recorded_night(120));
        game.set_phone(Some(size));
        game.set_touch(true);
        game.save.touch = TouchControls::On;
        assert_eq!(game.phone_layout(size).key(Buttons::R2), None, "watching has only its strip");
        press(&mut game, Buttons::X);
        assert!(game.in_photo() && game.phone_layout(size).key(Buttons::R2).is_some(), "photo mode has Change");
    }

    #[test]
    fn quit_shows_last_in_the_main_menu_only_where_the_game_can_quit() {
        let mut game = Game::new(1);
        for _ in 0..7 {
            press(&mut game, Buttons::DOWN);
        }
        assert_eq!(game.row, 0, "seven rows wrap back to Play");
        game.set_quit(true);
        for _ in 0..7 {
            press(&mut game, Buttons::DOWN);
        }
        press(&mut game, Buttons::A);
        assert!(game.quit_asked());
    }

    #[test]
    fn one_screen_without_touch_shows_the_title_with_the_menu_over_it() {
        let mut game = Game::new(1);
        game.set_phone(Some([720.0, 480.0]));
        game.set_touch(false);
        let f = game.draw();
        let phone = f.phone.unwrap();
        assert!(phone.top.is_some() && phone.bottom.is_none(), "the title's screen, not the menu's");
        assert!(f.top.shapes.iter().any(|s| matches!(s, Shape::Text { text, .. } if text == "Play")), "with the menu on it");
        for _ in 0..3 {
            press(&mut game, Buttons::DOWN);
        }
        press(&mut game, Buttons::A);
        let phone = game.draw().phone.unwrap();
        assert!(phone.top.is_none() && phone.bottom.is_some(), "Settings shows full size");
    }

    #[test]
    fn at_every_handheld_shape_one_screen_fills_the_window_at_4_3() {
        let handhelds = [[640.0, 480.0], [720.0, 720.0], [720.0, 480.0], [960.0, 544.0], [480.0, 640.0]];
        let shapes = (4..=25).map(|tenths| [1000.0 * tenths as f32 / 10.0, 1000.0]);
        for size in handhelds.into_iter().chain(shapes) {
            let mut game = Game::new(1);
            game.set_phone(Some(size));
            game.set_touch(false);
            let check = |game: &mut Game, held: Buttons, what: &str| {
                game.update(&Input { held, ..Default::default() }, 0.016);
                let phone = game.draw().phone.unwrap();
                let shown: Vec<_> = [phone.top, phone.bottom].into_iter().flatten().collect();
                assert_eq!(shown.len(), 1, "{size:?} {what}: one screen");
                let [x, y, w, h] = shown[0];
                assert!((w / h - SCREEN_W / SCREEN_H).abs() < 1e-3, "{size:?} {what}: {w}x{h} is stretched");
                assert!(x >= -0.01 && y >= -0.01 && x + w <= size[0] + 0.01 && y + h <= size[1] + 0.01, "{size:?} {what}: clipped");
                assert!((w - size[0]).abs() < 0.01 || (h - size[1]).abs() < 0.01, "{size:?} {what}: smaller than the window allows");
            };
            check(&mut game, Buttons::NONE, "title");
            settings_row(&mut game, 0);
            check(&mut game, Buttons::NONE, "settings");
            press(&mut game, Buttons::B);
            for _ in 0..3 {
                press(&mut game, Buttons::UP);
            }
            press(&mut game, Buttons::A);
            assert!(matches!(game.state, State::Harvest(_)), "{size:?}: a run");
            check(&mut game, Buttons::NONE, "run");
            for (held, what) in [(Buttons::L1, "map"), (Buttons::L2, "grenade aim"), (Buttons::R1, "quick throw")] {
                check(&mut game, held, what);
                check(&mut game, Buttons::NONE, what);
            }
            press(&mut game, Buttons::START);
            check(&mut game, Buttons::NONE, "pause");
        }
    }

    #[test]
    fn the_pause_menu_offers_taps_only_where_the_screen_takes_them() {
        for (touch, taps) in [(false, false), (true, true)] {
            let mut game = Game::new(1);
            game.set_phone(Some([640.0, 480.0]));
            game.set_touch(touch);
            press(&mut game, Buttons::A);
            press(&mut game, Buttons::START);
            let f = game.draw();
            let shown = f.top.shapes.iter().chain(&f.bottom.shapes).any(|s| matches!(s, Shape::Text { text, .. } if text.contains("tap")));
            assert_eq!(shown, taps, "touch {touch}");
        }
    }

    #[test]
    fn the_hints_name_the_buttons_of_the_pad_last_used() {
        let mut game = Game::new(1);
        game.set_phone(Some([640.0, 480.0]));
        game.set_touch(false);
        let texts = |game: &Game| {
            let f = game.draw();
            f.top.shapes.iter().chain(&f.bottom.shapes).filter_map(|s| if let Shape::Text { text, .. } = s { Some(text.clone()) } else { None }).collect::<Vec<_>>().join("\n")
        };
        assert!(texts(&game).contains("L1: map.") && texts(&game).contains("B: turn around."), "{}", texts(&game));
        for (family, map, turn) in [(PadFamily::Xbox, "LB: map.", "A: turn around."), (PadFamily::PlayStation, "L1: map.", "Cross: turn around."), (PadFamily::Keyboard, "1: map.", "Z: turn around.")] {
            game.set_pad_family(family);
            let shown = texts(&game);
            assert!(shown.contains(map) && shown.contains(turn), "{family:?}: {shown}");
        }
        game.set_pad_family(PadFamily::Nintendo);
    }

    #[test]
    fn the_title_over_the_picture_names_the_pad_keys_in_type_a_handheld_can_read() {
        let size = [640.0, 480.0];
        let mut game = Game::new(1);
        game.set_phone(Some(size));
        game.set_touch(false);
        let f = game.draw();
        let k = size[1] / SCREEN_H;
        let texts: Vec<_> = f.top.shapes.iter().filter_map(|s| if let Shape::Text { text, size, .. } = s { Some((text.clone(), *size)) } else { None }).collect();
        let help: Vec<_> = texts.iter().filter(|(t, ..)| t.contains("turn around") || t.contains("L2")).collect();
        assert_eq!(help.len(), 2, "two lines of keys: {texts:?}");
        assert!(texts.iter().all(|(t, ..)| !t.contains("Tap")), "no taps without touch");
        for (text, px) in help {
            assert!(px * k >= 12.0, "{text}: {} window pixels high", px * k);
        }
    }

    #[test]
    fn a_or_a_tap_on_the_title_starts_a_run_and_back_returns() {
        let mut game = Game::new(1);
        press(&mut game, Buttons::A);
        assert!(matches!(game.state, State::Harvest(_)));
        press(&mut game, Buttons::BACK);
        assert!(matches!(game.state, State::Title));
        game.update(&Input { touch: Some([512.0, menu::ROWS_TOP + 2.0 * menu::ROW_H + 20.0]), touch_started: true, ..Default::default() }, 0.016);
        assert_eq!(game.page, Page::Settings, "a tap on a row opens it");
        press(&mut game, Buttons::B);
        game.update(&Input { touch: Some([512.0, 60.0]), touch_started: true, ..Default::default() }, 0.016);
        assert!(matches!(game.state, State::Harvest(_)));
    }

    #[test]
    fn a_finished_run_records_the_longest_night() {
        let mut game = Game::new(1);
        press(&mut game, Buttons::A);
        let mut t = InputTracker::default();
        // Tapping A and X every other frame starts the run and picks each level-up, so the run plays out.
        for i in 0..60 * 60 * 10 {
            let held = if i % 2 == 0 { Buttons::A | Buttons::X } else { Buttons::NONE };
            game.update(&t.frame(RawInput { held, ..Default::default() }), 1.0 / 60.0);
            if game.save().records(game.save().difficulty).best > 0 {
                break;
            }
        }
        assert!(game.save().records(Difficulty::EASY).best > 0, "the first run is on Easy");
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
        assert_eq!(game.outputs().screens_on, [false; 2]);
    }

    #[test]
    fn every_scene_opens_by_name() {
        let mut game = Game::new(1);
        for name in ["title", "harvest", "single", "farm", "bench", "bench-corn", "art", "icon", "range", "hardware"] {
            game.open(Scene::from_name(name).unwrap());
        }
        assert!(matches!(game.state, State::Hardware(_)));
    }

    #[test]
    fn the_range_scene_tries_the_weapon_it_was_given() {
        let mut game = Game::new(1);
        game.set_range(RangeSetup::read("shotgun:2").unwrap());
        game.open(Scene::from_name("range").unwrap());
        let State::Harvest(h) = &game.state else { panic!("the range is a run") };
        assert!(h.summary().starts_with("range shotgun 2"), "{}", h.summary());
    }

    /// Opens Settings, then its Controls page, from the title.
    fn open_controls(game: &mut Game) {
        settings_row(game, 0);
        let row = game.settings_list().iter().position(|&s| s == menu::Setting::Controls).expect("Settings lists Controls");
        for _ in 0..row {
            press(game, Buttons::DOWN);
        }
        press(game, Buttons::A);
        assert!(game.rebind.is_some(), "A opens Controls");
    }

    fn note(game: &Game) -> String {
        let f = game.draw();
        f.bottom.shapes.iter().filter_map(|s| if let Shape::Text { text, .. } = s { Some(text.clone()) } else { None }).collect::<Vec<_>>().join("\n")
    }

    #[test]
    fn a_job_moves_to_the_button_pressed_and_swaps_with_the_one_there() {
        let mut game = Game::new(1);
        open_controls(&mut game);
        for _ in 0..3 {
            press(&mut game, Buttons::DOWN);
        }
        assert_eq!(controls::ACTIONS[3].0, Buttons::X);
        press(&mut game, Buttons::A);
        assert!(note(&game).contains("Press the button for Pick, aim at barrels"), "{}", note(&game));
        press(&mut game, Buttons::R1);
        let c = game.save().controls;
        assert_eq!((c.physical(Buttons::X), c.physical(Buttons::R1)), (Buttons::R1, Buttons::X));
        assert!(note(&game).contains("Swapped with Throw ahead, reroll"), "{}", note(&game));
        assert_eq!(Save::parse(&game.save().to_text()).controls, c, "the save keeps it");

        for _ in 0..3 {
            press(&mut game, Buttons::UP);
        }
        press(&mut game, Buttons::A);
        press(&mut game, Buttons::R2);
        assert_eq!(game.take_screenshot(), None, "a button pressed to bind does nothing else");
        assert_eq!(game.save().controls.physical(Buttons::Y), Buttons::R2);
    }

    #[test]
    fn the_moved_button_does_the_job_and_default_puts_them_back() {
        let mut game = Game::new(1);
        open_controls(&mut game);
        press(&mut game, Buttons::DOWN);
        press(&mut game, Buttons::A);
        press(&mut game, Buttons::B);
        assert_eq!(game.save().controls.physical(Buttons::A), Buttons::B, "B chooses now");
        assert_eq!(game.escape(), Buttons::A, "and Esc backs out on the button B moved to");
        press(&mut game, Buttons::B);
        assert!(game.rebind.as_ref().is_some_and(|r| r.waiting()), "the pad's B chose the row instead of leaving");
        press(&mut game, Buttons::B);
        press(&mut game, Buttons::UP);
        press(&mut game, Buttons::UP);
        press(&mut game, Buttons::B);
        assert!(game.save().controls.is_default(), "the Default row");
        press(&mut game, Buttons::B);
        assert!(game.rebind.is_none() && game.page == Page::Settings, "B leaves Controls for Settings");
    }

    #[test]
    fn a_tap_waits_for_a_button_and_another_tap_gives_up() {
        let mut game = Game::new(1);
        open_controls(&mut game);
        let [x, y, w, h] = menu::control_row(2);
        let at = [x + w / 2.0, y + h / 2.0];
        let tap = |game: &mut Game| game.update(&Input { touch: Some(at), touch_started: true, ..Default::default() }, 0.016);
        tap(&mut game);
        assert!(game.rebind.as_ref().is_some_and(|r| r.waiting()));
        tap(&mut game);
        assert!(game.rebind.as_ref().is_some_and(|r| !r.waiting()) && game.save().controls.is_default());
    }

    /// Opens Settings from the title and moves to its `row`.
    fn settings_row(game: &mut Game, row: usize) {
        for _ in 0..3 {
            press(game, Buttons::DOWN);
        }
        press(game, Buttons::A);
        assert_eq!(game.page, Page::Settings);
        for _ in 0..row {
            press(game, Buttons::DOWN);
        }
    }

    #[test]
    fn reset_takes_two_presses_and_keeps_the_settings() {
        let mut game = Game::new(1);
        let kept = Save { volume: 2, difficulty: Difficulty::HARD, ..Save::default() };
        let mut played = kept;
        *played.records_mut(Difficulty::HARD).unwrap() = crate::save::Records { kills: 900, best: 300, best_level: 9, nights: 4, dawns: 1, found: 0b110, ..Default::default() };
        game.load(played);
        settings_row(&mut game, 9);
        press(&mut game, Buttons::A);
        assert_eq!(game.save().records(Difficulty::HARD).kills, 900, "one press only asks");
        game.update(&Input::default(), RESET_CONFIRM + 0.1);
        press(&mut game, Buttons::A);
        assert_eq!(game.save().records(Difficulty::HARD).kills, 900, "the question times out");
        press(&mut game, Buttons::A);
        assert_eq!(game.save(), kept);
        assert!(game.reset_shown > 0.0, "the title says so");
    }

    #[test]
    fn settings_change_brightness_rumble_and_fps_and_b_goes_back() {
        let mut game = Game::new(1);
        settings_row(&mut game, 0);
        press(&mut game, Buttons::RIGHT);
        assert_eq!(game.save().brightness, [5, 4]);
        for _ in 0..20 {
            press(&mut game, Buttons::LEFT);
        }
        assert_eq!(game.save().brightness[0], 1, "never dark");
        press(&mut game, Buttons::DOWN);
        press(&mut game, Buttons::DOWN);
        press(&mut game, Buttons::DOWN);
        press(&mut game, Buttons::A);
        assert!(!game.save().rumble);
        game.rumble = 1.0;
        assert!(!game.outputs().rumble, "rumble off keeps the motor still");
        press(&mut game, Buttons::DOWN);
        press(&mut game, Buttons::A);
        assert!(game.save().fps);
        press(&mut game, Buttons::B);
        assert_eq!(game.page, Page::Main);
        assert!(matches!(game.state, State::Title));
    }

    #[test]
    fn the_corner_map_row_shows_on_one_screen_and_its_choice_persists() {
        let mut game = Game::new(1);
        assert!(!game.settings_list().contains(&menu::Setting::CornerMap), "two screens have no corner map");
        game.save.one_screen = true;
        game.page = Page::Settings;
        game.setting = game.settings_list().iter().position(|&s| s == menu::Setting::CornerMap).expect("the row");
        press(&mut game, Buttons::A);
        assert!(!game.save.corner_map);
        assert!(!crate::save::Save::parse(&game.save.to_text()).corner_map);
        assert!(crate::save::Save::parse("layout=one\n").corner_map, "on in an old save");
    }

    #[test]
    fn screens_set_to_one_mid_run_darken_the_bottom_screen_and_persist() {
        let mut game = Game::new(1);
        press(&mut game, Buttons::A);
        assert_eq!(game.outputs().screens_on, [true; 2]);
        press(&mut game, Buttons::START);
        press(&mut game, Buttons::DOWN);
        press(&mut game, Buttons::A);
        for _ in 0..5 {
            press(&mut game, Buttons::DOWN);
        }
        assert_eq!(game.settings_list()[game.setting], menu::Setting::Screens);
        press(&mut game, Buttons::A);
        assert!(game.save.one_screen);
        assert!(matches!(&game.state, State::Harvest(h) if h.one_screen()));
        assert_eq!(game.outputs().screens_on, [true, false]);
        assert!(crate::save::Save::parse(&game.save.to_text()).one_screen);
    }

    #[test]
    fn the_pause_menu_opens_settings_over_the_run_without_reset() {
        let mut game = Game::new(1);
        press(&mut game, Buttons::A);
        press(&mut game, Buttons::START);
        press(&mut game, Buttons::DOWN);
        press(&mut game, Buttons::A);
        assert_eq!(game.page, Page::Settings);
        assert_eq!(game.settings_list().len(), 7);
        let t = game.summary();
        game.update(&Input::default(), 1.0);
        assert_eq!(game.summary(), t, "the run stays paused");
        press(&mut game, Buttons::B);
        assert_eq!(game.page, Page::Main);
        assert!(matches!(game.state, State::Harvest(_)));
    }

    #[test]
    fn the_enemies_page_turns_kinds_off_but_never_the_last() {
        let mut game = Game::new(1);
        settings_row(&mut game, 8);
        press(&mut game, Buttons::A);
        assert_eq!(game.page, Page::Enemies);
        game.draw();
        press(&mut game, Buttons::A);
        assert_eq!(game.save().enemies_off, 0b001);
        press(&mut game, Buttons::DOWN);
        press(&mut game, Buttons::RIGHT);
        press(&mut game, Buttons::DOWN);
        press(&mut game, Buttons::A);
        assert_eq!(game.save().enemies_off, 0b011, "the last kind on stays on");
        assert!(game.take_sounds().contains(&Sfx::Miss));
        let off = Save::parse(&game.save().to_text()).enemies_off;
        assert_eq!(off, 0b011, "and the save keeps them by id");
        press(&mut game, Buttons::B);
        assert_eq!(game.page, Page::Settings);
        press(&mut game, Buttons::B);
        for _ in 0..3 {
            press(&mut game, Buttons::UP);
        }
        press(&mut game, Buttons::A);
        let State::Harvest(h) = &game.state else { panic!("a run starts") };
        assert_eq!(h.enemies_off(), 0b011, "the run leaves them out");
    }

    #[test]
    fn game_folder_shows_only_where_the_platform_offers_one_and_asks_it_to_choose() {
        let mut game = Game::new(1);
        assert!(!game.settings_list().contains(&menu::Setting::Folder), "no folder to choose by default");
        game.set_folder("");
        settings_row(&mut game, 9);
        assert_eq!(game.settings_list()[game.setting], menu::Setting::Folder);
        game.draw();
        press(&mut game, Buttons::A);
        assert!(game.take_folder_request());
        assert!(!game.take_folder_request(), "one press, one picker");
        game.set_folder("Documents/Last Harvest");
        game.draw();
        press(&mut game, Buttons::B);
        for _ in 0..3 {
            press(&mut game, Buttons::UP);
        }
        press(&mut game, Buttons::A);
        press(&mut game, Buttons::START);
        press(&mut game, Buttons::DOWN);
        press(&mut game, Buttons::A);
        assert!(!game.settings_list().contains(&menu::Setting::Folder), "nor over a run");
    }

    #[test]
    fn the_player_row_keeps_the_farmer_when_there_are_no_player_cards() {
        let mut game = Game::new(1);
        settings_row(&mut game, 7);
        assert_eq!(game.settings_list()[game.setting], menu::Setting::Player);
        game.draw();
        press(&mut game, Buttons::RIGHT);
        assert_eq!(game.save().player.as_str(), "", "one look to choose from");
        game.load(Save { player: save::LookId::new("nobody"), how_to_seen: true, ..game.save() });
        press(&mut game, Buttons::B);
        for _ in 0..3 {
            press(&mut game, Buttons::UP);
        }
        press(&mut game, Buttons::A);
        assert!(matches!(game.state, State::Harvest(_)), "a run with a look that's gone plays as the farmer");
        game.draw();
    }

    #[test]
    fn the_title_walks_each_kind_turned_on_between_the_common_one() {
        let mut game = Game::new(1);
        let names = |g: &Game| g.title_walkers().iter().map(|w| w[0]).collect::<Vec<_>>();
        assert_eq!(names(&game), [art::SKINNY_SIDE[0], art::KID_SIDE[0], art::SKINNY_SIDE[0], art::BIG_SIDE[0], art::SKINNY_SIDE[0]]);
        game.load(Save { enemies_off: 0b010, ..game.save() });
        assert_eq!(names(&game), [art::SKINNY_SIDE[0], art::BIG_SIDE[0], art::SKINNY_SIDE[0]], "a kind turned off stays off the title");
        game.load(Save { enemies_off: 0b100, ..game.save() });
        assert_eq!(names(&game), [art::KID_SIDE[0], art::BIG_SIDE[0], art::KID_SIDE[0]], "with the common kind off, the last one on fills in");
    }

    #[test]
    fn the_map_row_stays_on_the_random_farm_with_no_maps() {
        let mut game = Game::new(1);
        press(&mut game, Buttons::DOWN);
        for button in [Buttons::A, Buttons::RIGHT, Buttons::LEFT] {
            press(&mut game, button);
            assert_eq!((game.page, game.save.map.as_str()), (Page::Main, ""), "it picks, it doesn't open a page");
        }
        game.draw();
        assert!(Scene::from_name("bench-100x80") == Some(Scene::Bench([100, 80])) && Scene::from_name("bench-1x").is_none());
        assert!(Scene::from_name("bench-corn") == Some(Scene::BenchCorn(harvest::MAP_SIZE)) && Scene::from_name("bench-corn-100x80") == Some(Scene::BenchCorn([100, 80])));
        assert!(Scene::from_name("bench-corn-1x").is_none());
    }

    #[test]
    fn a_bench_reports_its_flow_field_once_timed_by_the_caller() {
        let mut game = Game::new(1);
        assert!(game.bench_report(|_| 0.0).is_none(), "only a bench reports");
        game.open(Scene::Bench([20, 16]));
        let mut ran = false;
        let line = game.bench_report(|work| {
            work();
            ran = true;
            0.01
        });
        assert!(ran);
        let line = line.expect("a bench reports");
        assert!(line.starts_with("bench: 20x16 tiles, ") && line.ends_with(" props, flow field 0.50 ms"), "{line}");
        assert!(game.bench_report(|_| 0.0).is_none(), "and only once");
    }

    fn texts(canvas: &Canvas) -> Vec<String> {
        canvas.shapes.iter().filter_map(|s| if let Shape::Text { text, .. } = s { Some(text.clone()) } else { None }).collect()
    }

    #[test]
    fn the_first_play_shows_how_to_play_then_plays_and_never_again() {
        let mut game = Game::new(1);
        game.load(Save::default());
        press(&mut game, Buttons::A);
        assert!(matches!(game.state, State::Title) && game.page == Page::HowTo, "a new save's first Play explains first");
        assert!(texts(&game.draw().bottom).contains(&"A: Play".to_string()));
        press(&mut game, Buttons::A);
        assert!(matches!(game.state, State::Harvest(_)), "then plays");
        assert!(game.save().how_to_seen);
        press(&mut game, Buttons::BACK);
        press(&mut game, Buttons::A);
        assert!(matches!(game.state, State::Harvest(_)), "not again this session");
        let mut next = Game::new(2);
        next.load(Save::parse(&game.save().to_text()));
        press(&mut next, Buttons::A);
        assert!(matches!(next.state, State::Harvest(_)), "nor from the saved file");
        let mut old = Game::new(3);
        old.load(Save::parse("nights=3\n"));
        press(&mut old, Buttons::A);
        assert!(matches!(old.state, State::Harvest(_)), "nor for a save from before it that has played");
    }

    #[test]
    fn b_leaves_the_first_how_to_play_for_the_menu_and_it_counts_as_seen() {
        let mut game = Game::new(1);
        game.load(Save::default());
        press(&mut game, Buttons::A);
        press(&mut game, Buttons::B);
        assert!(matches!(game.state, State::Title) && game.page == Page::Main && game.save().how_to_seen);
        press(&mut game, Buttons::A);
        assert!(matches!(game.state, State::Harvest(_)));
    }

    #[test]
    fn how_to_play_opens_from_the_title_and_any_button_or_a_tap_closes_it() {
        for close in [Some(Buttons::A), Some(Buttons::B), Some(Buttons::START), None] {
            let mut game = Game::new(1);
            game.load(Save { how_to_seen: true, ..Save::default() });
            press(&mut game, Buttons::DOWN);
            press(&mut game, Buttons::DOWN);
            press(&mut game, Buttons::A);
            assert_eq!(game.page, Page::HowTo);
            let shown = texts(&game.draw().bottom);
            assert!(shown.contains(&"How to play".to_string()) && shown.contains(&"B: Back".to_string()), "{shown:?}");
            match close {
                Some(b) => press(&mut game, b),
                None => game.update(&Input { touch: Some([512.0, 300.0]), touch_started: true, ..Default::default() }, 0.016),
            }
            assert!(matches!(game.state, State::Title) && game.page == Page::Main, "{close:?} goes back to the menu, not into a run");
        }
    }

    #[test]
    fn how_to_play_fits_every_layout_and_names_its_keys() {
        let layouts: [(&str, Option<[f32; 2]>, bool, bool); 5] = [
            ("two screens", None, false, true),
            ("one screen", None, true, true),
            ("phone, touch", Some([2400.0, 1080.0]), false, true),
            ("phone, pad grenades", Some([2400.0, 1080.0]), false, false),
            ("muOS", Some([640.0, 480.0]), false, true),
        ];
        for (what, phone, one_screen, touch_grenades) in layouts {
            let mut game = Game::new(1);
            game.set_phone(phone);
            game.set_touch(what != "muOS");
            game.load(Save { one_screen, touch_grenades, how_to_seen: true, ..Save::default() });
            game.page = Page::HowTo;
            let lines: Vec<&str> = game.how_to_lines().iter().map(|l| l.1).collect();
            let all = lines.join(" ");
            for line in &lines {
                let wrapped = menu::wrapped(line, menu::HOW_TO_SIZE, menu::HOW_TO_ROOM, 2);
                assert!(wrapped.iter().all(|l| !menu::fitted(l, menu::HOW_TO_SIZE, menu::HOW_TO_ROOM, menu::HOW_TO_SIZE).0.ends_with("...")), "{what}: {wrapped:?}");
            }
            assert!(all.contains("hearts") && all.contains("Harvester") && all.contains("farmhouse") && all.contains("last seen"), "{what}: {all}");
            let (pad, l2) = (all.contains("D-pad"), all.contains("L2"));
            match what {
                "two screens" => assert!(pad && !l2 && all.contains("Tap the map"), "{all}"),
                "one screen" | "muOS" => assert!(pad && l2 && all.contains("L1"), "{what}: {all}"),
                _ => assert!(!pad && all.contains("Drag") && !all.contains("R1"), "{what}: {all}"),
            }
            let f = game.draw();
            let shown: Vec<_> = f.phone.map(|p| [p.top, p.bottom].into_iter().flatten().count()).into_iter().collect();
            assert!(shown.iter().all(|&n| n >= 1), "{what}: the page shows");
            assert!(texts(&f.bottom).contains(&"How to play".to_string()), "{what}");
        }
    }

    #[test]
    fn stats_and_credits_open_and_close() {
        let mut game = Game::new(1);
        for (downs, page) in [(4, Page::Stats), (1, Page::Credits)] {
            for _ in 0..downs {
                press(&mut game, Buttons::DOWN);
            }
            press(&mut game, Buttons::A);
            assert_eq!(game.page, page);
            game.draw();
            press(&mut game, Buttons::B);
            assert_eq!(game.page, Page::Main);
        }
    }

    #[test]
    fn stats_opens_the_run_history_newest_first_and_b_comes_back() {
        let mut game = Game::new(1);
        game.save.record(harvest::Run { secs: 125, level: 4, difficulty: Difficulty::HARD, dps: 37, ..Default::default() });
        game.save.record(harvest::Run { secs: 600, level: 9, won: true, dps: 81, ..Default::default() });
        for _ in 0..4 {
            press(&mut game, Buttons::DOWN);
        }
        press(&mut game, Buttons::A);
        let text = |game: &Game| game.draw().bottom.shapes.iter().filter_map(|s| if let Shape::Text { text, .. } = s { Some(text.clone()) } else { None }).collect::<Vec<_>>();
        assert!(text(&game).contains(&"81".to_string()), "Stats shows the best damage a second");
        press(&mut game, Buttons::A);
        assert_eq!(game.page, Page::History);
        let shown = text(&game);
        let at = |s: &str| shown.iter().position(|t| t == s).unwrap_or_else(|| panic!("{s} missing: {shown:?}"));
        assert!(at("Home at dawn") < at("Turned"), "newest first");
        for cell in ["Hard", "2:05", "4", "37", "Easy", "10:00", "9", "81"] {
            at(cell);
        }
        press(&mut game, Buttons::B);
        assert_eq!(game.page, Page::Stats);
        press(&mut game, Buttons::B);
        assert_eq!(game.page, Page::Main);
    }

    #[test]
    fn history_shows_each_runs_powerups_five_runs_a_page() {
        let mut game = Game::new(1);
        let pistol = harvest::powerup_index("pistol").unwrap();
        let crows = harvest::powerup_index("crows").unwrap();
        let mut kit = [None; harvest::RUN_KIT];
        kit[0] = Some((pistol as u16, 4));
        kit[1] = Some((crows as u16, 2));
        game.save.record(harvest::Run { secs: 61, kit, ..Default::default() });
        for k in 0..6 {
            game.save.record(harvest::Run { secs: 100 + k, ..Default::default() });
        }
        game.page = Page::Stats;
        press(&mut game, Buttons::A);
        let shown = |game: &Game| {
            let f = game.draw();
            let arts: Vec<crate::art::Art> = f.bottom.shapes.iter().filter_map(|s| if let Shape::Art { art, .. } = s { Some(*art) } else { None }).collect();
            let text: Vec<String> = f.bottom.shapes.iter().filter_map(|s| if let Shape::Text { text, .. } = s { Some(text.clone()) } else { None }).collect();
            (arts, text)
        };
        let (arts, text) = shown(&game);
        assert!(text.contains(&"Page 1 of 2".to_string()) && !text.contains(&"1:01".to_string()), "the oldest run is on page two");
        assert!(!arts.contains(&harvest::powerup_icon(pistol)));
        press(&mut game, Buttons::RIGHT);
        let (arts, text) = shown(&game);
        assert!(text.contains(&"1:01".to_string()) && text.contains(&"Page 2 of 2".to_string()));
        assert!(arts.contains(&harvest::powerup_icon(pistol)) && arts.contains(&harvest::powerup_icon(crows)), "its powerups as icons");
        assert!(text.contains(&"4".to_string()) && text.contains(&"2".to_string()), "at their levels");
        press(&mut game, Buttons::RIGHT);
        assert!(shown(&game).1.contains(&"Page 1 of 2".to_string()), "and round again");
    }

    #[test]
        fn r2_takes_one_screenshot_and_flashes_after_it() {
        let mut game = Game::new(1);
        press(&mut game, Buttons::R2);
        assert_eq!(game.take_screenshot(), None, "a platform that saves no screenshots takes none");
        game.set_captures(true);
        press(&mut game, Buttons::R2);
        assert_eq!(game.take_screenshot(), Some(Capture::Screens));
        assert_eq!(game.take_screenshot(), None, "one press, one shot");
        assert!(game.shot_flash > 0.0);
        press(&mut game, Buttons::R1);
        assert_eq!(game.take_screenshot(), None, "R1 no longer takes one");
        game.open(Scene::Hardware);
        press(&mut game, Buttons::R2);
        assert_eq!(game.take_screenshot(), None, "the hardware check keeps R2 for the motor");
    }

    #[test]
    fn in_a_run_on_either_layout_r2_screenshots_r1_quick_throws_and_no_button_pauses_on_a_map() {
        for one in [false, true] {
            let mut game = Game::new(1);
            game.set_captures(true);
            game.open(Scene::Harvest);
            let State::Harvest(h) = &mut game.state else { panic!() };
            h.set_one_screen(one);
            press(&mut game, Buttons::R2);
            assert_eq!(game.take_screenshot(), Some(Capture::Screens), "one screen: {one}");
            game.update(&Input { held: Buttons::R1, pressed: Buttons::R1, ..Default::default() }, 0.016);
            game.update(&Input::default(), 0.016);
            assert!(game.take_screenshot().is_none(), "one screen: {one}");
            assert!(matches!(&game.state, State::Harvest(h) if h.grenades_in_the_air() == 1), "one screen: {one}: R1 threw");
            let night = |g: &Game| match &g.state {
                State::Harvest(h) => h.night_t(),
                _ => f32::NAN,
            };
            for held in [Buttons::L1, Buttons::R2] {
                let before = night(&game);
                for _ in 0..90 {
                    game.update(&Input { held, ..Default::default() }, 1.0 / 60.0);
                }
                assert!(night(&game) > before + 1.0, "one screen: {one}: holding {held:?} never pauses the night");
            }
        }
    }

    fn in_photo_mode(captures: bool) -> Game {
        let mut game = Game::new(1);
        game.set_captures(captures);
        game.open(Scene::Harvest);
        for b in [Buttons::START, Buttons::DOWN, Buttons::DOWN, Buttons::A] {
            press(&mut game, b);
        }
        assert!(game.in_photo());
        game
    }

    #[test]
    fn a_photo_capture_frame_hides_the_guides_and_every_overlay() {
        let mut game = in_photo_mode(true);
        game.save.fps = true;
        press(&mut game, Buttons::VOL_UP);
        assert!(!game.draw().top.shapes.is_empty(), "guides, volume and frame rate show while setting up");
        press(&mut game, Buttons::R1);
        let frame = game.draw();
        assert!(frame.top.cards.is_some() && frame.top.shapes.is_empty(), "the capture is the scene alone");
        assert_eq!(game.take_screenshot(), Some(Capture::Photo));
        assert!(!game.draw().top.shapes.is_empty(), "and the guides come back after it");
    }

    #[test]
    fn on_a_phone_photo_mode_has_a_named_key_for_each_of_its_buttons_and_only_back_leaves() {
        let size = [2400.0, 1080.0];
        let mut game = in_photo_mode(true);
        game.set_phone(Some(size));
        let (run, photo) = (touch::layout(size, true, true, false), touch::layout(size, true, true, true));
        let finger = |at: [f32; 2]| {
            let mut touches = [None; MAX_TOUCHES];
            touches[0] = Some(at);
            Input { touches, ..Default::default() }
        };
        let middle = |[x, y, w, h]: touch::Rect| [x + w / 2.0, y + h / 2.0];
        let tap = |game: &mut Game, at: [f32; 2]| {
            game.update(&finger(at), 0.016);
            game.update(&Input::default(), 0.016);
        };
        let help = |game: &Game| game.draw().top.shapes.iter().filter_map(|s| if let Shape::Text { text, .. } = s { Some(text.clone()) } else { None }).collect::<Vec<_>>().join("\n");
        tap(&mut game, middle(run.key(Buttons::B).unwrap()));
        assert!(game.in_photo(), "where Turn around was leaves photo mode no more");
        assert!(help(&game).contains("Setting: choose") && !help(&game).contains("L2"), "the guides name the keys: {}", help(&game));
        tap(&mut game, middle(photo.key(Buttons::L2).unwrap()));
        tap(&mut game, middle(photo.key(Buttons::R2).unwrap()));
        assert!(help(&game).contains("[Fog: Thick]"), "{}", help(&game));
        let eye = |game: &Game| game.draw().top.cards.unwrap().eye.y;
        let low = eye(&game);
        let height = middle(photo.key(Buttons::L1).unwrap());
        let stick = photo.stick_at();
        let mut both = finger(height);
        both.touches[1] = Some(stick);
        game.update(&both, 0.016);
        both.touches[1] = Some([stick[0], stick[1] - size[1] * 0.2]);
        for _ in 0..30 {
            game.update(&both, 0.016);
        }
        game.update(&Input::default(), 0.016);
        assert!(eye(&game) > low, "Height and the stick raise the camera");
        for _ in 0..4 {
            tap(&mut game, middle(photo.key(Buttons::L2).unwrap()));
        }
        assert!(help(&game).contains("[Place: ahead]"), "{}", help(&game));
        game.update(&finger(stick), 0.016);
        for _ in 0..30 {
            game.update(&finger([stick[0] + size[1] * 0.2, stick[1]]), 0.016);
        }
        game.update(&Input::default(), 0.016);
        assert!(!help(&game).contains("[Place: ahead]"), "the stick moves the cross");
        tap(&mut game, middle(photo.key(Buttons::X).unwrap()));
        assert!(help(&game).contains("[Place: ahead]"), "Reset puts it back: {}", help(&game));
        assert!(game.in_photo() && game.take_screenshot().is_none(), "and none of them takes a photo");
        tap(&mut game, middle(photo.key(Buttons::B).unwrap()));
        assert!(!game.in_photo(), "Back leaves");
    }

    #[test]
    fn without_saving_a_photo_r1_hides_the_guides_for_the_players_own_screenshot() {
        let mut game = in_photo_mode(false);
        press(&mut game, Buttons::R1);
        assert_eq!(game.take_screenshot(), None);
        assert!(game.draw().top.shapes.is_empty());
        press(&mut game, Buttons::R1);
        assert!(!game.draw().top.shapes.is_empty());
    }

    #[test]
    fn the_title_sets_the_difficulty_and_the_run_uses_it() {
        let mut game = Game::new(1);
        assert_eq!(game.save().difficulty, Difficulty::EASY, "a first run starts on Easy");
        press(&mut game, Buttons::LEFT);
        press(&mut game, Buttons::LEFT);
        assert_eq!(game.save().difficulty, Difficulty::STROLL, "it stops at the end");
        let tap = |game: &mut Game, x: f32| game.update(&Input { touch: Some([x, DIFFICULTY_Y + 40.0]), touch_started: true, ..Default::default() }, 0.016);
        for _ in 0..3 {
            tap(&mut game, SCREEN_W / 2.0 + 250.0);
        }
        assert_eq!(game.save().difficulty, Difficulty::HARD);
        assert!(matches!(game.state, State::Title), "an arrow tap doesn't start a run");
        press(&mut game, Buttons::A);
        assert!(game.summary().starts_with("harvest hard "), "{}", game.summary());
    }
}
