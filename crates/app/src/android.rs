//! The game on an Android phone or handheld: a NativeActivity whose one window shows the game as `Game::set_phone`
//! lays it out, with the on-screen controls, which hide while a built-in or attached gamepad is in use. The pack is
//! the APK's `game_data.pak` asset, the save sits in the app's internal storage, and the log goes to logcat under
//! `last-harvest`. Input comes straight from `android-activity`: winit drops a gamepad's hat axes, which is where
//! handhelds such as the Retroid Pocket send their d-pad. On a dual-screen handheld, `MainActivity.java` hands over
//! the second screen's surface and touches, and the game plays on two screens as on the RG DS; it also runs the
//! vibrator, which the NDK can't reach.

use std::io::Cursor;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use android_activity::input::{Axis, InputEvent, KeyAction, Keycode, MotionAction, MotionEvent};
use android_activity::{AndroidApp, InputStatus, MainEvent, PollEvent};
use glutin::config::{Api, Config, ConfigTemplateBuilder, GlConfig};
use glutin::context::{ContextApi, ContextAttributesBuilder, NotCurrentGlContext, PossiblyCurrentContext, PossiblyCurrentGlContext, Version};
use glutin::display::{Display, DisplayApiPreference, GlDisplay};
use glutin::surface::{GlSurface, Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use last_harvest_game::{Battery, Buttons, Canvas, Game, InputTracker, MAX_TOUCHES, RawInput, SCREEN_H, SCREEN_W, Save, System};
use last_harvest_pack::Pak;
use last_harvest_render::{Profile, Renderer};
use raw_window_handle::{AndroidDisplayHandle, AndroidNdkWindowHandle, RawDisplayHandle, RawWindowHandle};

use crate::{audio, log, pak, samples};

/// Seconds a changed save waits for more changes before it is written.
const SETTLE: f32 = 1.0;
const STICK_DEAD: f32 = 0.15;
/// `AINPUT_SOURCE_CLASS_JOYSTICK`: a gamepad's sticks and hat, as opposed to a finger.
const JOYSTICK: u32 = 0x10;

/// A gamepad's button, named by position as on an Xbox pad, as the web build maps them. Back backs out as B does.
fn pad_button(key: Keycode) -> Option<Buttons> {
    Some(match key {
        Keycode::DpadUp => Buttons::UP,
        Keycode::DpadDown => Buttons::DOWN,
        Keycode::DpadLeft => Buttons::LEFT,
        Keycode::DpadRight => Buttons::RIGHT,
        Keycode::ButtonA => Buttons::A,
        Keycode::ButtonB | Keycode::Back => Buttons::B,
        Keycode::ButtonX => Buttons::X,
        Keycode::ButtonY => Buttons::Y,
        Keycode::ButtonL1 => Buttons::L1,
        Keycode::ButtonR1 => Buttons::R1,
        Keycode::ButtonL2 => Buttons::L2,
        Keycode::ButtonR2 => Buttons::R2,
        Keycode::ButtonStart => Buttons::START,
        Keycode::ButtonSelect => Buttons::SELECT,
        _ => return None,
    })
}

/// The frame-rate overlay's numbers, gathered over a second, and the thermal zones it reads then.
struct Stats {
    frames: u32,
    total_ms: f32,
    worst_ms: f32,
    since: Instant,
    cpu: Vec<PathBuf>,
    gpu: Vec<PathBuf>,
    system: System,
}

/// The hottest of `zones`, in Celsius, or None when Android won't let the app read any.
fn hottest(zones: &[PathBuf]) -> Option<f32> {
    zones.iter().filter_map(|z| std::fs::read_to_string(z.join("temp")).ok()?.trim().parse::<f32>().ok()).map(|m| m / 1000.0).reduce(f32::max)
}

impl Stats {
    fn new() -> Self {
        let mut s = Self { frames: 0, total_ms: 0.0, worst_ms: 0.0, since: Instant::now(), cpu: Vec::new(), gpu: Vec::new(), system: System::default() };
        for zone in std::fs::read_dir("/sys/class/thermal").into_iter().flatten().flatten().map(|e| e.path()) {
            let kind = std::fs::read_to_string(zone.join("type")).unwrap_or_default();
            let kind = kind.trim();
            if kind.ends_with("-usr") && kind.starts_with("cpu-") {
                s.cpu.push(zone);
            } else if kind.ends_with("-usr") && kind.starts_with("gpu") {
                s.gpu.push(zone);
            }
        }
        if hottest(&s.cpu).is_none() {
            log!("stats: no readable CPU temperature ({} zones)", s.cpu.len());
        }
        s
    }

    fn frame(&mut self, ms: f32) -> System {
        self.frames += 1;
        self.total_ms += ms;
        self.worst_ms = self.worst_ms.max(ms);
        let secs = self.since.elapsed().as_secs_f32();
        if secs >= 1.0 {
            let battery = || {
                let read = |f: &str| std::fs::read_to_string(format!("/sys/class/power_supply/battery/{f}")).ok();
                let percent = read("capacity")?.trim().parse().ok()?;
                Some(Battery { percent, charging: read("status").is_some_and(|s| s.trim() == "Charging") })
            };
            self.system = System { fps: self.frames as f32 / secs, frame_ms: self.total_ms / self.frames as f32, worst_ms: self.worst_ms, soc_c: hottest(&self.cpu), gpu_c: hottest(&self.gpu), battery: battery(), ..self.system };
            (self.frames, self.total_ms, self.worst_ms, self.since) = (0, 0.0, 0.0, Instant::now());
        }
        self.system
    }
}

/// A finger by slot, which it keeps until it lifts.
#[derive(Clone, Copy)]
struct Finger {
    id: i32,
    at: [f32; 2],
    /// A frame has seen it; a tap can lift before the next frame, and must still count.
    seen: bool,
    lifted: bool,
}

#[derive(Default)]
struct Fingers([Option<Finger>; MAX_TOUCHES]);

impl Fingers {
    fn down(&mut self, id: i32, at: [f32; 2]) {
        if let Some(slot) = self.0.iter_mut().find(|f| f.is_none()) {
            *slot = Some(Finger { id, at, seen: false, lifted: false });
        }
    }

    fn moved(&mut self, id: i32, at: [f32; 2]) {
        if let Some(f) = self.0.iter_mut().flatten().find(|f| f.id == id && !f.lifted) {
            f.at = at;
        }
    }

    fn up(&mut self, id: i32) {
        for slot in &mut self.0 {
            if let Some(f) = slot
                && f.id == id
            {
                f.lifted = true;
                if f.seen {
                    *slot = None;
                }
            }
        }
    }

    /// Each slot's finger for this frame, mapped by `to`, dropping the ones that have lifted once a frame saw them.
    fn frame(&mut self, to: impl Fn([f32; 2]) -> [f32; 2]) -> [Option<[f32; 2]>; MAX_TOUCHES] {
        let mut out = [None; MAX_TOUCHES];
        for (o, slot) in out.iter_mut().zip(&mut self.0) {
            if let Some(f) = slot {
                *o = Some(to(f.at));
                f.seen = true;
                if f.lifted {
                    *slot = None;
                }
            }
        }
        out
    }
}

/// What `MainActivity` hands over from its UI thread: the second screen's window, and touches on it as
/// (phase, id, x, y) with x and y from 0 to 1.
#[derive(Default)]
struct Second {
    window: Option<ndk::native_window::NativeWindow>,
    /// A new window waits to be drawn on.
    fresh: bool,
    /// The window is going away: the frame loop drops its surface, then clears this, which lets Java go on.
    dropping: bool,
    touches: Vec<(i32, i32, f32, f32)>,
}

static SECOND: Mutex<Option<Second>> = Mutex::new(None);
static DROPPED: Condvar = Condvar::new();

fn second<R>(f: impl FnOnce(&mut Second) -> R) -> R {
    let mut guard = SECOND.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Second::default))
}

#[unsafe(no_mangle)]
extern "system" fn Java_com_kylejameswalker_lastharvest_MainActivity_secondSurface(env: *mut jni::sys::JNIEnv, _class: jni::sys::jclass, surface: jni::sys::jobject) {
    if surface.is_null() {
        let mut guard = SECOND.lock().unwrap_or_else(|e| e.into_inner());
        let s = guard.get_or_insert_with(Second::default);
        s.window = None;
        s.fresh = false;
        s.dropping = true;
        // Android frees the surface once this returns, so wait for the frame loop to stop drawing on it.
        let _ = DROPPED.wait_timeout_while(guard, Duration::from_secs(2), |s| s.as_ref().is_some_and(|s| s.dropping));
        return;
    }
    let window = unsafe { ndk::native_window::NativeWindow::from_surface(env.cast(), surface) };
    second(|s| {
        s.window = window;
        s.fresh = true;
    });
}

#[unsafe(no_mangle)]
extern "system" fn Java_com_kylejameswalker_lastharvest_MainActivity_secondTouch(_env: *mut jni::sys::JNIEnv, _class: jni::sys::jclass, phase: i32, id: i32, x: f32, y: f32) {
    second(|s| s.touches.push((phase, id, x, y)));
}

/// The largest 4:3 rect centered in a `w` by `h` window, as x, y, width and height in GL's bottom-up pixels.
fn fit(w: u32, h: u32) -> [i32; 4] {
    let k = (w as f32 / SCREEN_W).min(h as f32 / SCREEN_H);
    let (fw, fh) = ((SCREEN_W * k).round() as i32, (SCREEN_H * k).round() as i32);
    [(w as i32 - fw) / 2, (h as i32 - fh) / 2, fw, fh]
}

/// Calls one of `MainActivity`'s methods that take an int, or none.
fn call(android: &AndroidApp, name: &str, arg: Option<i32>) {
    let result = (|| -> jni::errors::Result<()> {
        let vm = unsafe { jni::JavaVM::from_raw(android.vm_as_ptr().cast()) }?;
        let mut env = vm.attach_current_thread()?;
        let activity = unsafe { jni::objects::JObject::from_raw(android.activity_as_ptr().cast()) };
        match arg {
            Some(v) => env.call_method(&activity, name, "(I)V", &[jni::objects::JValue::Int(v)])?,
            None => env.call_method(&activity, name, "()V", &[])?,
        };
        Ok(())
    })();
    if let Err(e) = result {
        log!("{name}: {e}");
    }
}

/// The second screen's window and the surface the bottom screen draws on.
struct Bottom {
    window: ndk::native_window::NativeWindow,
    surface: Surface<WindowSurface>,
}

/// What Android takes away while the app is in the background: the window, and with it the surface.
struct Gl {
    window: ndk::native_window::NativeWindow,
    surface: Surface<WindowSurface>,
    bottom: Option<Bottom>,
    context: PossiblyCurrentContext,
    gl: glow::Context,
    renderer: Renderer,
}

struct App {
    android: AndroidApp,
    game: Game,
    tracker: InputTracker,
    display: Option<Display>,
    config: Option<Config>,
    gl: Option<Gl>,
    sounds: samples::Files,
    audio: Option<audio::Audio>,
    fingers: Fingers,
    /// Fingers on the second screen, in its 0..1 coordinates.
    bottom_fingers: Fingers,
    /// The vibrator is running, as `Outputs::rumble` last said.
    rumbling: bool,
    keys: Buttons,
    /// Presses no frame has seen yet, held for one frame even when already let go.
    struck: Buttons,
    stick: [f32; 2],
    /// The right stick's sideways push, which turns.
    look: f32,
    hat: [f32; 2],
    last: Instant,
    save_path: Option<PathBuf>,
    written: Save,
    unsaved: f32,
    stats: Stats,
    quit: bool,
}

fn text(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn size(window: &ndk::native_window::NativeWindow) -> Result<[NonZeroU32; 2], String> {
    let dim = |v: i32| NonZeroU32::new(v.max(0) as u32).ok_or("the window has no size");
    Ok([dim(window.width())?, dim(window.height())?])
}

impl App {
    fn start_gl(&mut self) -> Result<(), String> {
        let window = self.android.native_window().ok_or("no window")?;
        let raw = RawWindowHandle::AndroidNdk(AndroidNdkWindowHandle::new(window.ptr().cast()));
        let display = match &self.display {
            Some(d) => d.clone(),
            None => unsafe { Display::new(RawDisplayHandle::Android(AndroidDisplayHandle::new()), DisplayApiPreference::Egl) }.map_err(text)?,
        };
        let config = match &self.config {
            Some(c) => c.clone(),
            None => {
                let template = ConfigTemplateBuilder::new().with_depth_size(24).with_api(Api::GLES3).compatible_with_native_window(raw).build();
                unsafe { display.find_configs(template) }.map_err(text)?.max_by_key(|c| c.depth_size().min(24)).ok_or("no GL config")?
            }
        };
        let context_attrs = ContextAttributesBuilder::new().with_context_api(ContextApi::Gles(Some(Version::new(3, 0)))).build(Some(raw));
        let not_current = unsafe { display.create_context(&config, &context_attrs) }.map_err(text)?;
        let [w, h] = size(&window)?;
        let surface_attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(raw, w, h);
        let surface = unsafe { display.create_window_surface(&config, &surface_attrs) }.map_err(text)?;
        let context = not_current.make_current(&surface).map_err(text)?;
        surface.set_swap_interval(&context, SwapInterval::Wait(NonZeroU32::MIN)).ok();
        let gl = unsafe { glow::Context::from_loader_function_cstr(|s| display.get_proc_address(s)) };
        let renderer = Renderer::new(&gl, Profile::Gles)?;
        self.display = Some(display);
        self.config = Some(config);
        self.gl = Some(Gl { window, surface, bottom: None, context, gl, renderer });
        second(|s| s.fresh |= s.window.is_some());
        self.last = Instant::now();
        Ok(())
    }

    fn event(&mut self, event: PollEvent) {
        let PollEvent::Main(event) = event else { return };
        match event {
            MainEvent::InitWindow { .. } => {
                if let Err(e) = self.start_gl() {
                    log!("gl: {e}");
                }
            }
            MainEvent::TerminateWindow { .. } => {
                self.gl = None;
                self.persist(0.0, true);
            }
            MainEvent::Resume { .. } => {
                if self.audio.is_none() {
                    self.audio = audio::Audio::open(&self.sounds).inspect_err(|e| log!("audio disabled: {e}")).ok();
                }
            }
            MainEvent::Pause => {
                self.audio = None;
                self.fingers = Fingers::default();
                self.bottom_fingers = Fingers::default();
                self.keys = Buttons::NONE;
                if std::mem::take(&mut self.rumbling) {
                    call(&self.android, "stopVibrate", None);
                }
                self.stick = [0.0; 2];
                self.look = 0.0;
                self.hat = [0.0; 2];
                self.persist(0.0, true);
            }
            MainEvent::Destroy => {
                self.persist(0.0, true);
                self.quit = true;
            }
            _ => {}
        }
    }

    fn read_input(&mut self) {
        for (phase, id, x, y) in second(|s| std::mem::take(&mut s.touches)) {
            match phase {
                0 => self.bottom_fingers.down(id, [x, y]),
                1 => self.bottom_fingers.moved(id, [x, y]),
                _ => self.bottom_fingers.up(id),
            }
        }
        let android = self.android.clone();
        let Ok(mut events) = android.input_events_iter() else { return };
        while events.next(|e| self.input(e)) {}
    }

    fn input(&mut self, event: &InputEvent) -> InputStatus {
        match event {
            InputEvent::KeyEvent(k) => {
                let Some(b) = pad_button(k.key_code()) else {
                    if k.action() == KeyAction::Down && k.repeat_count() == 0 {
                        log!("key: {:?}", k.key_code());
                    }
                    return InputStatus::Unhandled;
                };
                match k.action() {
                    KeyAction::Down => {
                        self.keys |= b;
                        self.struck |= b;
                    }
                    KeyAction::Up => self.keys.set(b, false),
                    _ => {}
                }
                InputStatus::Handled
            }
            InputEvent::MotionEvent(m) if u32::from(m.source()) & JOYSTICK != 0 => {
                let p = m.pointer_at_index(0);
                self.stick = [p.axis_value(Axis::X), p.axis_value(Axis::Y)].map(|v| if v.abs() < STICK_DEAD { 0.0 } else { v });
                self.hat = [p.axis_value(Axis::HatX), p.axis_value(Axis::HatY)];
                let z = p.axis_value(Axis::Z);
                self.look = if z.abs() < STICK_DEAD { 0.0 } else { z };
                InputStatus::Handled
            }
            InputEvent::MotionEvent(m) => {
                self.touch(m);
                InputStatus::Handled
            }
            _ => InputStatus::Unhandled,
        }
    }

    fn touch(&mut self, m: &MotionEvent) {
        match m.action() {
            MotionAction::Down | MotionAction::PointerDown => {
                let p = m.pointer_at_index(m.pointer_index());
                self.fingers.down(p.pointer_id(), [p.x(), p.y()]);
            }
            MotionAction::Move => {
                for p in m.pointers() {
                    self.fingers.moved(p.pointer_id(), [p.x(), p.y()]);
                }
            }
            MotionAction::Up | MotionAction::PointerUp => self.fingers.up(m.pointer_at_index(m.pointer_index()).pointer_id()),
            MotionAction::Cancel => {
                for p in m.pointers() {
                    self.fingers.up(p.pointer_id());
                }
            }
            _ => {}
        }
    }

    /// Takes up a second screen Java handed over, or lets one go.
    fn follow_second(&mut self) {
        if second(|s| s.dropping) {
            if let Some(g) = &mut self.gl {
                g.bottom = None;
            }
            self.bottom_fingers = Fingers::default();
            second(|s| s.dropping = false);
            DROPPED.notify_all();
            log!("second screen: gone");
        }
        let Some(g) = &mut self.gl else { return };
        let Some(window) = second(|s| if std::mem::take(&mut s.fresh) { s.window.clone() } else { None }) else { return };
        let (Some(display), Some(config)) = (&self.display, &self.config) else { return };
        let raw = RawWindowHandle::AndroidNdk(AndroidNdkWindowHandle::new(window.ptr().cast()));
        let made = size(&window).and_then(|[w, h]| {
            let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(raw, w, h);
            unsafe { display.create_window_surface(config, &attrs) }.map_err(text)
        });
        match made {
            Ok(surface) => {
                log!("second screen: {}x{}", window.width(), window.height());
                // Vsync on the main screen paces the loop; waiting on both would halve the frame rate.
                surface.set_swap_interval(&g.context, SwapInterval::DontWait).ok();
                g.bottom = Some(Bottom { window, surface });
            }
            Err(e) => log!("second screen: {e}"),
        }
    }

    fn persist(&mut self, dt: f32, now: bool) {
        let save = self.game.save();
        if save == self.written {
            self.unsaved = 0.0;
            return;
        }
        self.unsaved += dt;
        if (now || self.unsaved >= SETTLE)
            && let Some(path) = &self.save_path
        {
            let tmp = path.with_extension("tmp");
            match std::fs::write(&tmp, save.to_text()).and_then(|()| std::fs::rename(&tmp, path)) {
                Ok(()) => self.written = save,
                Err(e) => log!("save: {}: {e}", path.display()),
            }
            self.unsaved = 0.0;
        }
    }

    fn tick(&mut self) {
        let Some(g) = &mut self.gl else { return };
        let now = Instant::now();
        let frame_ms = now.duration_since(self.last).as_secs_f32() * 1000.0;
        let dt = (frame_ms / 1000.0).min(0.05);
        self.last = now;
        let Ok([w, h]) = size(&g.window) else { return };
        if g.surface.width() != Some(w.get()) || g.surface.height() != Some(h.get()) {
            g.surface.resize(&g.context, w, h);
        }
        let mut held = self.keys | std::mem::take(&mut self.struck);
        let [hx, hy] = self.hat;
        for (on, b) in [(hx < -0.5, Buttons::LEFT), (hx > 0.5, Buttons::RIGHT), (hy < -0.5, Buttons::UP), (hy > 0.5, Buttons::DOWN)] {
            if on {
                held |= b;
            }
        }
        let touches = match &g.bottom {
            // Two screens: only the second takes touches, as the bottom screen's, through its letterbox.
            Some(b) => {
                let (bw, bh) = (b.window.width().max(1) as u32, b.window.height().max(1) as u32);
                let [x, y, fw, fh] = fit(bw, bh);
                let top = bh as i32 - y - fh;
                self.fingers.frame(|p| p);
                self.bottom_fingers.frame(|[nx, ny]| [(nx * bw as f32 - x as f32) / fw as f32 * SCREEN_W, (ny * bh as f32 - top as f32) / fh as f32 * SCREEN_H])
            }
            None => self.fingers.frame(|p| p),
        };
        let raw = RawInput { held, stick: self.stick, look: self.look, touch: touches.iter().flatten().next().copied(), touches, ..Default::default() };
        let input = self.tracker.frame(raw);
        self.game.set_system(self.stats.frame(frame_ms));
        self.game.set_phone(g.bottom.is_none().then_some([w.get() as f32, h.get() as f32]));
        self.game.update(&input, dt);
        let sounds = self.game.take_sounds();
        if let Some(audio) = &self.audio {
            audio.set_volume(self.game.volume());
            let outputs = self.game.outputs();
            audio.set_dread(outputs.dread);
            audio.set_beds(outputs.screens_on[0]);
            sounds.into_iter().for_each(|s| audio.play(s));
        }
        let rumble = self.game.outputs().rumble;
        if rumble != self.rumbling {
            self.rumbling = rumble;
            // Longer than any pulse the game asks for; stopVibrate cuts it off when the game's rumble ends.
            call(&self.android, if rumble { "vibrate" } else { "stopVibrate" }, rumble.then_some(2000));
        }
        let frame = self.game.draw();
        let black = Canvas { clear: [0.0, 0.0, 0.0, 1.0], ..Default::default() };
        if let Some(b) = &g.bottom {
            let (bw, bh) = (b.window.width().max(1) as u32, b.window.height().max(1) as u32);
            if b.surface.width() != Some(bw) || b.surface.height() != Some(bh) {
                b.surface.resize(&g.context, NonZeroU32::new(bw).unwrap_or(NonZeroU32::MIN), NonZeroU32::new(bh).unwrap_or(NonZeroU32::MIN));
            }
            if g.context.make_current(&b.surface).is_ok() {
                g.renderer.draw(&g.gl, &black, [0, 0, bw as i32, bh as i32]);
                g.renderer.draw(&g.gl, &frame.bottom, fit(bw, bh));
                if let Err(e) = b.surface.swap_buffers(&g.context) {
                    log!("swap bottom: {e}");
                }
            }
            if let Err(e) = g.context.make_current(&g.surface) {
                log!("gl: {e}");
            }
            g.renderer.draw(&g.gl, &black, [0, 0, w.get() as i32, h.get() as i32]);
            g.renderer.draw(&g.gl, &frame.top, fit(w.get(), h.get()));
        } else if let Some(phone) = &frame.phone {
            crate::draw_phone(&mut g.renderer, &g.gl, &frame, phone, [w.get() as i32, h.get() as i32]);
        }
        if let Err(e) = g.surface.swap_buffers(&g.context) {
            log!("swap: {e}");
        }
        self.persist(dt, false);
    }
}

fn run(android: AndroidApp) -> Result<(), String> {
    let mut asset = android.asset_manager().open(c"game_data.pak").ok_or("game_data.pak is missing from the APK")?;
    let bytes = asset.buffer().map_err(text)?.to_vec();
    let sounds = pak::install(Some(Pak::read(Cursor::new(bytes))?), None)?;
    let save_path = android.internal_data_path().map(|d| d.join("save.txt"));
    let saved = save_path.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| Save::parse(&t)).unwrap_or_default();
    let mut game = Game::new(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64));
    game.load(saved);
    game.set_backlights(false);
    let mut app = App {
        android: android.clone(),
        game,
        tracker: InputTracker::default(),
        display: None,
        config: None,
        gl: None,
        sounds,
        audio: None,
        fingers: Fingers::default(),
        bottom_fingers: Fingers::default(),
        rumbling: false,
        keys: Buttons::NONE,
        struck: Buttons::NONE,
        stick: [0.0; 2],
        look: 0.0,
        hat: [0.0; 2],
        last: Instant::now(),
        save_path,
        written: saved,
        unsaved: 0.0,
        stats: Stats::new(),
        quit: false,
    };
    while !app.quit {
        // With a window, vsync in swap_buffers paces the loop; without one, wait for Android to give it back.
        let timeout = if app.gl.is_some() { Some(Duration::ZERO) } else { None };
        android.poll_events(timeout, |event| app.event(event));
        app.read_input();
        app.follow_second();
        app.tick();
    }
    Ok(())
}

#[unsafe(no_mangle)]
fn android_main(android: AndroidApp) {
    if let Err(e) = run(android) {
        log!("last-harvest: {e}");
    }
}
