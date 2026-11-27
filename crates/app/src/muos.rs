//! The game on a muOS handheld, such as the H700 Anbernics: one screen through the device's own SDL2, which carries
//! its Mali GPU driver, and the built-in pad through muOS's SDL controller mapping. It plays as on a phone without
//! touch: menus show the bottom screen, a run the top, and grenades aim with L2. Hold Menu to quit. Rumble drives
//! the motor muOS's device config names, as muOS's own RUMBLE helper does.

#[allow(dead_code)]
mod persist;

use std::time::{Duration, Instant};

use last_harvest::controllers::{Motors, Pad, Seats};
use last_harvest::{VERSION, audio, clock, log, pak, shot};
use last_harvest_game::{Buttons, Capture, Game, InputTracker, RawInput, Scene, System, photo};
use last_harvest_render::{Profile, Renderer};
use sdl2::controller::{Axis, Button, GameController};
use sdl2::event::Event;
use sdl2::video::GLProfile;

/// Seconds Menu is held to quit.
const QUIT_HOLD: f32 = 1.0;
const STICK_DEAD: f32 = 0.15;

/// A pad button by position, as SDL's controller mapping names them: A is the bottom face button, as on the web.
fn button(b: Button) -> Option<Buttons> {
    Some(match b {
        Button::A => Buttons::A,
        Button::B => Buttons::B,
        Button::X => Buttons::X,
        Button::Y => Buttons::Y,
        Button::LeftShoulder => Buttons::L1,
        Button::RightShoulder => Buttons::R1,
        Button::Start => Buttons::START,
        Button::Back => Buttons::SELECT,
        Button::Guide => Buttons::HOME,
        Button::DPadUp => Buttons::UP,
        Button::DPadDown => Buttons::DOWN,
        Button::DPadLeft => Buttons::LEFT,
        Button::DPadRight => Buttons::RIGHT,
        _ => return None,
    })
}

/// The rumble motor's sysfs file and what turns it on and off, from muOS's device config.
struct Motor {
    path: String,
    on: &'static str,
    off: &'static str,
}

impl Motor {
    fn find() -> Option<Self> {
        let read = |key: &str| {
            std::fs::read_to_string(format!("/opt/muos/device/config/board/{key}"))
                .ok()
                .map(|s| s.trim().to_string())
        };
        let path = read("rumble").filter(|p| !p.is_empty())?;
        // muOS's rk boards stop the motor with 1000000; the others with 0.
        let off = if read("name").is_some_and(|n| n.starts_with("rk")) {
            "1000000"
        } else {
            "0"
        };
        log!("rumble: {path}");
        Some(Self { path, on: "1", off })
    }

    fn set(&self, on: bool) {
        if let Err(e) = std::fs::write(&self.path, if on { self.on } else { self.off }) {
            log!("rumble: {}: {e}", self.path);
        }
    }
}

/// One controller's state from SDL's events, by its instance id.
#[derive(Default)]
struct PadState {
    id: u32,
    keys: Buttons,
    struck: Buttons,
    stick: [f32; 2],
    look: f32,
    triggers: [bool; 2],
}

impl PadState {
    fn take(&mut self) -> Pad {
        let mut held = self.keys | std::mem::take(&mut self.struck);
        held.set(Buttons::L2, held.contains(Buttons::L2) || self.triggers[0]);
        held.set(Buttons::R2, held.contains(Buttons::R2) || self.triggers[1]);
        Pad {
            id: self.id as usize,
            held,
            stick: self.stick,
            look: self.look,
        }
    }
}

fn axis(v: i16) -> f32 {
    let v = v as f32 / 32767.0;
    if v.abs() < STICK_DEAD {
        0.0
    } else {
        v.clamp(-1.0, 1.0)
    }
}

fn main() {
    if let Err(e) = run() {
        log!("last-harvest: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    log!("last-harvest {VERSION} on muOS");
    if let Some(home) = last_harvest::home() {
        last_harvest::metrics::install_crash_hook(home);
    }
    let range = pak::range_setup();
    let sounds = pak::load_without(&persist::mods_off())?;
    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    let attr = video.gl_attr();
    attr.set_context_profile(GLProfile::GLES);
    attr.set_context_version(3, 0);
    attr.set_depth_size(24);
    attr.set_double_buffer(true);
    let mode = video.current_display_mode(0)?;
    log!("display: {}x{} at {} Hz", mode.w, mode.h, mode.refresh_rate);
    let window = video
        .window("Last Harvest", mode.w as u32, mode.h as u32)
        .opengl()
        .fullscreen()
        .build()
        .map_err(|e| e.to_string())?;
    let _context = window.gl_create_context()?;
    video.gl_set_swap_interval(1).ok();
    let gl =
        unsafe { glow::Context::from_loader_function(|s| video.gl_get_proc_address(s).cast()) };
    let mut renderer = Renderer::new(&gl, Profile::Gles)?;
    let controllers = sdl.game_controller()?;
    let mut pads: Vec<GameController> = Vec::new();
    let mut events = sdl.event_pump()?;
    sdl.mouse().show_cursor(false);

    let (mut saver, save) = persist::Saver::load();
    let mut platform = last_harvest::platform::backend(last_harvest::home());
    let mut game = Game::new(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.as_nanos() as u64),
    );
    game.set_split(
        std::env::var("LAST_HARVEST_SPLIT")
            .ok()
            .and_then(|v| last_harvest::split_views(&v))
            .unwrap_or(1),
    );
    game.load(save);
    game.record_replays(last_harvest::replays::info());
    game.set_backlights(false);
    game.set_touch(false);
    game.set_quit(true);
    game.set_captures(true);
    last_harvest::online::set_up(&mut game);
    let mut online = last_harvest::online::Online::default();
    if let Ok(kit) = std::env::var("LAST_HARVEST_KIT") {
        game.start_with(&kit);
    }
    if let Some(secs) = std::env::var("LAST_HARVEST_CLOCK")
        .ok()
        .and_then(|s| s.parse().ok())
    {
        game.start_clock(secs);
    }
    game.set_range(range.unwrap_or_default());
    if let Some(scene) = std::env::var("LAST_HARVEST_START")
        .ok()
        .and_then(|s| Scene::from_name(&s))
    {
        game.open(scene);
    }
    let audio = audio::Audio::open(&sounds)
        .inspect_err(|e| log!("audio disabled: {e}"))
        .ok();
    let motor = Motor::find();
    let mut rumbling = false;

    let mut tracker = InputTracker::default();
    let mut ticker = clock::Ticker::default();
    let mut states: Vec<PadState> = Vec::new();
    let mut seats = Seats::default();
    let mut motors = Motors::default();
    let mut trackers = Vec::new();
    let mut menu_held = 0.0;
    let mut last = Instant::now();
    let mut stats = (0u32, 0.0f32, 0.0f32, Instant::now());
    let mut system = System::default();
    loop {
        for event in events.poll_iter() {
            match event {
                Event::Quit { .. } => {
                    return finish(&mut saver, &mut game, &mut platform, motor.as_ref());
                }
                Event::ControllerDeviceAdded { which, .. } => match controllers.open(which) {
                    Ok(pad) => {
                        log!("pad: {}", pad.name());
                        pads.push(pad);
                    }
                    Err(e) => log!("pad {which}: {e}"),
                },
                Event::ControllerDeviceRemoved { which, .. } => {
                    pads.retain(|p| p.instance_id() != which);
                    states.retain(|s| s.id != which);
                }
                Event::ControllerButtonDown {
                    button: b, which, ..
                } => {
                    if let Some(b) = button(b) {
                        let s = state(&mut states, which);
                        s.keys |= b;
                        s.struck |= b;
                    }
                }
                Event::ControllerButtonUp {
                    button: b, which, ..
                } => {
                    if let Some(b) = button(b) {
                        state(&mut states, which).keys.set(b, false);
                    }
                }
                Event::ControllerAxisMotion {
                    axis: a,
                    value,
                    which,
                    ..
                } => {
                    let s = state(&mut states, which);
                    match a {
                        Axis::LeftX => s.stick[0] = axis(value),
                        Axis::LeftY => s.stick[1] = axis(value),
                        Axis::RightX => s.look = axis(value),
                        Axis::TriggerLeft => s.triggers[0] = value > 16384,
                        Axis::TriggerRight => s.triggers[1] = value > 16384,
                        Axis::RightY => {}
                    }
                }
                _ => {}
            }
        }
        let now = Instant::now();
        let frame = now - last;
        let dt = frame.as_secs_f32().min(0.05);
        last = now;
        let ms = frame.as_secs_f32() * 1000.0;
        stats = (stats.0 + 1, stats.1 + ms, stats.2.max(ms), stats.3);
        if stats.3.elapsed() >= Duration::from_secs(1) {
            let secs = stats.3.elapsed().as_secs_f32();
            system = System {
                fps: stats.0 as f32 / secs,
                frame_ms: stats.1 / stats.0 as f32,
                worst_ms: stats.2,
                ..system
            };
            stats = (0, 0.0, 0.0, Instant::now());
            log!(
                "perf: fps={:.1} ms={:.1} worst={:.1} {}",
                system.fps,
                system.frame_ms,
                system.worst_ms,
                game.summary()
            );
        }
        let frame_pads: Vec<Pad> = states.iter_mut().map(PadState::take).collect();
        seats.update(&frame_pads, game.at_title());
        game.set_players(seats.players());
        let seated = seats.inputs(&frame_pads);
        let others = last_harvest::seated_others(&seated, &mut trackers);
        let (held, stick, look) = seated[0];
        menu_held = if frame_pads.iter().any(|p| p.held.contains(Buttons::HOME)) {
            menu_held + dt
        } else {
            0.0
        };
        if menu_held >= QUIT_HOLD || game.quit_asked() {
            return finish(&mut saver, &mut game, &mut platform, motor.as_ref());
        }
        let input = tracker.frame(RawInput {
            held,
            stick,
            look,
            ..Default::default()
        });
        let (w, h) = window.drawable_size();
        game.set_system(system);
        game.set_phone(Some([w as f32, h as f32]));
        online.pump(&mut game, dt);
        for inputs in ticker.frame_all(frame, std::iter::once(input).chain(others).collect()) {
            game.update_all(&inputs, clock::STEP);
        }
        last_harvest::platform::serve(&mut game, &mut platform, dt);
        persist::keep_replays(game.take_replays());
        if game.take_replays_request() {
            game.set_saved_replays(persist::saved_replays());
        }
        online.send(&mut game);
        let sounds = game.take_sounds();
        if let Some(audio) = &audio {
            audio.set_volume(game.volume());
            let outputs = game.outputs();
            audio.set_dread(outputs.dread);
            audio.set_beds(outputs.screens_on[0]);
            sounds.into_iter().for_each(|s| audio.play(s));
        }
        let rumble = game.outputs().rumble;
        if rumble != rumbling {
            rumbling = rumble;
            if let Some(m) = &motor {
                m.set(rumble);
            }
        }
        // Player 1 keeps the device's own motor; a joined pad buzzes through SDL.
        for (id, on) in motors.changes(&seats, &frame_pads, 1, |seat| game.rumble_for(seat)) {
            if let Some(pad) = pads.iter_mut().find(|p| p.instance_id() as usize == id) {
                let (k, ms) = if on { (0xC000, 2000) } else { (0, 0) };
                if let Err(e) = pad.set_rumble(k, k, ms) {
                    log!("rumble: {e}");
                }
            }
        }
        let n = last_harvest::lay_out_views(&mut game, true, [w, h]);
        let frame = game.draw_at(ticker.alpha());
        if n > 1 {
            last_harvest::draw_split(
                &mut renderer,
                &gl,
                &last_harvest::frame_views(&frame, n),
                [w as i32, h as i32],
            );
        } else if let Some(phone) = &frame.phone {
            last_harvest::draw_phone(&mut renderer, &gl, &frame, phone, [w as i32, h as i32]);
        }
        if let Some(capture) = game.take_screenshot() {
            let photo = capture == Capture::Photo;
            if let Some(path) = persist::next_screenshot(if photo { "photo" } else { "shot" }) {
                let mut img = shot::read_back(&gl, w, h);
                if photo && n > 1 {
                    img.top = last_harvest_render::split::split(n, [w, h])
                        .get(game.menu_view())
                        .copied()
                        .unwrap_or(img.top);
                }
                std::thread::spawn(move || save_capture(&path, photo, &img));
            }
        }
        window.gl_swap_window();
        saver.update(game.save(), false);
    }
}

/// A screenshot as `shot-NNNN.png`, or photo mode's top screen as `photo-NNNN.png` and its middle square beside it.
fn save_capture(path: &std::path::Path, photo: bool, img: &shot::Shot) {
    let write = |p: &std::path::Path, width: u32, rgba: &[u8]| match shot::png(width, rgba)
        .map_err(|e| e.to_string())
        .and_then(|b| std::fs::write(p, b).map_err(|e| e.to_string()))
    {
        Ok(()) => log!("screenshot: saved {}", p.display()),
        Err(e) => log!("screenshot: {}: {e}", p.display()),
    };
    if !photo {
        return write(path, img.width, &img.rgba);
    }
    let (top, full) = photo::crop(&img.rgba, img.width, img.top);
    write(path, top[2], &full);
    let (square, pixels) = photo::crop(&img.rgba, img.width, photo::square(top));
    let stem = path
        .file_stem()
        .map_or_else(Default::default, |s| s.to_string_lossy().into_owned());
    write(
        &path.with_file_name(format!("{stem}-square.png")),
        square[2],
        &pixels,
    );
}

/// The state of the controller with SDL instance id `which`, made on its first event.
fn state(states: &mut Vec<PadState>, which: u32) -> &mut PadState {
    let at = match states.iter().position(|s| s.id == which) {
        Some(at) => at,
        None => {
            states.push(PadState {
                id: which,
                ..Default::default()
            });
            states.len() - 1
        }
    };
    &mut states[at]
}

fn finish(
    saver: &mut persist::Saver,
    game: &mut Game,
    platform: &mut last_harvest::platform::Hooks,
    motor: Option<&Motor>,
) -> Result<(), String> {
    if let Some(m) = motor {
        m.set(false);
    }
    saver.update(game.save(), true);
    persist::keep_replays(game.end_replays());
    platform.exiting();
    log!("quit");
    Ok(())
}
