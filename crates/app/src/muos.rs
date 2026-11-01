//! The game on a muOS handheld, such as the H700 Anbernics: one screen through the device's own SDL2, which carries
//! its Mali GPU driver, and the built-in pad through muOS's SDL controller mapping. It plays as on a phone without
//! touch: menus show the bottom screen, a run the top, and grenades aim with L2. Hold Menu to quit.

#[allow(dead_code)]
mod persist;

use std::time::{Duration, Instant};

use last_harvest::{VERSION, audio, log, pak};
use last_harvest_game::{Buttons, Game, InputTracker, RawInput, System};
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

fn axis(v: i16) -> f32 {
    let v = v as f32 / 32767.0;
    if v.abs() < STICK_DEAD { 0.0 } else { v.clamp(-1.0, 1.0) }
}

fn main() {
    if let Err(e) = run() {
        log!("last-harvest: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    log!("last-harvest {VERSION} on muOS");
    let sounds = pak::load()?;
    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    let attr = video.gl_attr();
    attr.set_context_profile(GLProfile::GLES);
    attr.set_context_version(3, 0);
    attr.set_depth_size(24);
    attr.set_double_buffer(true);
    let mode = video.current_display_mode(0)?;
    log!("display: {}x{} at {} Hz", mode.w, mode.h, mode.refresh_rate);
    let window = video.window("Last Harvest", mode.w as u32, mode.h as u32).opengl().fullscreen().build().map_err(|e| e.to_string())?;
    let _context = window.gl_create_context()?;
    video.gl_set_swap_interval(1).ok();
    let gl = unsafe { glow::Context::from_loader_function(|s| video.gl_get_proc_address(s).cast()) };
    let mut renderer = Renderer::new(&gl, Profile::Gles)?;
    let controllers = sdl.game_controller()?;
    let mut pads: Vec<GameController> = Vec::new();
    let mut events = sdl.event_pump()?;
    sdl.mouse().show_cursor(false);

    let (mut saver, save) = persist::Saver::load();
    let mut game = Game::new(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64));
    game.load(save);
    game.set_backlights(false);
    game.set_touch(false);
    game.set_quit(true);
    let audio = audio::Audio::open(&sounds).inspect_err(|e| log!("audio disabled: {e}")).ok();

    let mut tracker = InputTracker::default();
    let (mut keys, mut struck) = (Buttons::NONE, Buttons::NONE);
    let (mut stick, mut look, mut triggers) = ([0.0f32; 2], 0.0f32, [false; 2]);
    let mut menu_held = 0.0;
    let mut last = Instant::now();
    let mut stats = (0u32, 0.0f32, 0.0f32, Instant::now());
    let mut system = System::default();
    loop {
        for event in events.poll_iter() {
            match event {
                Event::Quit { .. } => return finish(&mut saver, &game),
                Event::ControllerDeviceAdded { which, .. } => match controllers.open(which) {
                    Ok(pad) => {
                        log!("pad: {}", pad.name());
                        pads.push(pad);
                    }
                    Err(e) => log!("pad {which}: {e}"),
                },
                Event::ControllerDeviceRemoved { which, .. } => pads.retain(|p| p.instance_id() != which),
                Event::ControllerButtonDown { button: b, .. } => {
                    if let Some(b) = button(b) {
                        keys |= b;
                        struck |= b;
                    }
                }
                Event::ControllerButtonUp { button: b, .. } => {
                    if let Some(b) = button(b) {
                        keys.set(b, false);
                    }
                }
                Event::ControllerAxisMotion { axis: a, value, .. } => match a {
                    Axis::LeftX => stick[0] = axis(value),
                    Axis::LeftY => stick[1] = axis(value),
                    Axis::RightX => look = axis(value),
                    Axis::TriggerLeft => triggers[0] = value > 16384,
                    Axis::TriggerRight => triggers[1] = value > 16384,
                    Axis::RightY => {}
                },
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
            system = System { fps: stats.0 as f32 / secs, frame_ms: stats.1 / stats.0 as f32, worst_ms: stats.2, ..system };
            stats = (0, 0.0, 0.0, Instant::now());
            log!("perf: fps={:.1} ms={:.1} worst={:.1} {}", system.fps, system.frame_ms, system.worst_ms, game.summary());
        }
        let mut held = keys | std::mem::take(&mut struck);
        held.set(Buttons::L2, held.contains(Buttons::L2) || triggers[0]);
        held.set(Buttons::R2, held.contains(Buttons::R2) || triggers[1]);
        menu_held = if held.contains(Buttons::HOME) { menu_held + dt } else { 0.0 };
        if menu_held >= QUIT_HOLD || game.quit_asked() {
            return finish(&mut saver, &game);
        }
        let input = tracker.frame(RawInput { held, stick, look, ..Default::default() });
        let (w, h) = window.drawable_size();
        game.set_system(system);
        game.set_phone(Some([w as f32, h as f32]));
        game.update(&input, dt);
        let sounds = game.take_sounds();
        if let Some(audio) = &audio {
            audio.set_volume(game.volume());
            let outputs = game.outputs();
            audio.set_dread(outputs.dread);
            audio.set_beds(outputs.screens_on[0]);
            sounds.into_iter().for_each(|s| audio.play(s));
        }
        let frame = game.draw();
        if let Some(phone) = &frame.phone {
            last_harvest::draw_phone(&mut renderer, &gl, &frame, phone, [w as i32, h as i32]);
        }
        window.gl_swap_window();
        saver.update(game.save(), false);
    }
}

fn finish(saver: &mut persist::Saver, game: &Game) -> Result<(), String> {
    saver.update(game.save(), true);
    log!("quit");
    Ok(())
}
