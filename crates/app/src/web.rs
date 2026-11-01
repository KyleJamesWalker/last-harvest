//! The game in a web page: both screens on one WebGL2 canvas, top above bottom as on the device, the keyboard and
//! gamepads for its buttons, the pointer on the bottom half for touch, and the save in the browser's storage.

use std::cell::RefCell;
use std::rc::Rc;

use last_harvest_game::{Buttons, Game, InputTracker, MAX_TOUCHES, RawInput, SCREEN_H, SCREEN_W, Save, System};
use last_harvest_pack::Pak;
use last_harvest_render::{Profile, Renderer};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{AddEventListenerOptions, Document, Event, EventTarget, Gamepad, GamepadButton, HtmlCanvasElement, HtmlElement, KeyboardEvent, PointerEvent, Storage, WebGl2RenderingContext};

use crate::{audio, pak, samples};

const SAVE_KEY: &str = "last-harvest-save";
const STATS_MS: f64 = 1000.0;
const STICK_DEAD: f32 = 0.15;
/// The standard gamepad layout's buttons, by index, named as on an Xbox pad.
const PAD: [(u32, Buttons); 14] = [
    (0, Buttons::A),
    (1, Buttons::B),
    (2, Buttons::X),
    (3, Buttons::Y),
    (4, Buttons::L1),
    (5, Buttons::R1),
    (6, Buttons::L2),
    (7, Buttons::R2),
    (8, Buttons::SELECT),
    (9, Buttons::START),
    (12, Buttons::UP),
    (13, Buttons::DOWN),
    (14, Buttons::LEFT),
    (15, Buttons::RIGHT),
];

/// A pointer down on the bottom screen, in its pixels.
struct Touch {
    id: i32,
    at: [f32; 2],
    /// A frame has seen it; a click can lift before the next frame, and must still count as a tap.
    seen: bool,
    lifted: bool,
}

struct Web {
    game: Game,
    tracker: InputTracker,
    gl: glow::Context,
    renderer: Renderer,
    canvas: HtmlCanvasElement,
    document: Document,
    overlay: HtmlElement,
    /// Held until the first click or key, when the browser lets the page play sound.
    sounds: Option<samples::Files>,
    audio: Option<audio::Audio>,
    keys: Buttons,
    /// Keys pressed since the last frame, which holds each for at least one frame however quickly it lifts.
    struck: Buttons,
    lid_key: bool,
    touches: Vec<Touch>,
    storage: Option<Storage>,
    saved: Save,
    /// The last write was refused and has been warned of.
    refused: bool,
    /// The browser has taken the GL context away.
    lost: bool,
    /// A gamepad without the standard layout has been logged.
    odd_pad: bool,
    last: f64,
    stats: (u32, f64, f64),
    system: System,
}

fn now() -> f64 {
    web_sys::window().and_then(|w| w.performance()).map_or(0.0, |p| p.now())
}

fn key_button(code: &str) -> Option<Buttons> {
    Some(match code {
        "ArrowUp" => Buttons::UP,
        "ArrowDown" => Buttons::DOWN,
        "ArrowLeft" => Buttons::LEFT,
        "ArrowRight" => Buttons::RIGHT,
        "KeyX" => Buttons::A,
        "KeyZ" => Buttons::B,
        "KeyS" => Buttons::X,
        "KeyA" => Buttons::Y,
        "KeyQ" => Buttons::L1,
        "KeyW" => Buttons::R1,
        "Enter" | "NumpadEnter" => Buttons::START,
        "Backspace" => Buttons::SELECT,
        "Tab" => Buttons::BACK,
        "Digit1" => Buttons::L2,
        "Digit2" => Buttons::R2,
        "Minus" => Buttons::VOL_DOWN,
        "Equal" => Buttons::VOL_UP,
        _ => return None,
    })
}

/// Buttons held and the left stick across every connected gamepad with the standard layout; the first without it is
/// logged once, and `warned` remembers that.
fn gamepads(warned: &mut bool) -> (Buttons, [f32; 2]) {
    let (mut held, mut stick) = (Buttons::NONE, [0.0f32; 2]);
    let Some(pads) = web_sys::window().and_then(|w| w.navigator().get_gamepads().ok()) else { return (held, stick) };
    for pad in pads.iter().filter_map(|p| p.dyn_into::<Gamepad>().ok()) {
        if js_sys::Reflect::get(&pad, &"mapping".into()).ok().and_then(|m| m.as_string()).as_deref() != Some("standard") {
            if !std::mem::replace(warned, true) {
                web_sys::console::warn_1(&format!("gamepad: {} has no standard layout, so it is ignored", pad.id()).into());
            }
            continue;
        }
        let buttons = pad.buttons();
        for (i, b) in PAD {
            if buttons.get(i).dyn_into::<GamepadButton>().is_ok_and(|b| b.pressed()) {
                held |= b;
            }
        }
        let axes = pad.axes();
        for (i, s) in stick.iter_mut().enumerate() {
            let v = axes.get(i as u32).as_f64().unwrap_or(0.0) as f32;
            if v.abs() >= STICK_DEAD {
                *s = (*s + v).clamp(-1.0, 1.0);
            }
        }
    }
    (held, stick)
}

impl Web {
    /// Sizes the canvas's pixels to the space the page gives it, up to the screens' own 1024 across.
    fn fit(&mut self) {
        let dpr = web_sys::window().map_or(1.0, |w| w.device_pixel_ratio());
        let w = ((self.canvas.client_width() as f64 * dpr).round() as u32).clamp(4, SCREEN_W as u32) & !3;
        if self.canvas.width() != w {
            self.canvas.set_width(w);
            self.canvas.set_height(w * 3 / 2);
        }
    }

    /// Bottom-screen pixels for a pointer, or None on the top screen.
    fn to_screen(&self, e: &PointerEvent) -> Option<[f32; 2]> {
        let [w, h] = [self.canvas.client_width().max(1) as f32, self.canvas.client_height().max(2) as f32];
        let y = e.offset_y() as f32 / h * 2.0 - 1.0;
        (y >= 0.0).then(|| [(e.offset_x() as f32 / w).clamp(0.0, 1.0) * SCREEN_W, y.min(1.0) * SCREEN_H])
    }

    /// The first click or key: the page may now play sound.
    fn begin(&mut self) {
        let Some(sounds) = self.sounds.take() else { return };
        self.overlay.set_hidden(!self.lost);
        self.audio = audio::Audio::open(&sounds).inspect_err(|e| web_sys::console::warn_1(&format!("audio disabled: {e}").into())).ok();
    }

    /// The browser took the GL context: nothing draws until it gives it back, so the game waits behind a note.
    fn context_lost(&mut self, e: &Event) {
        e.prevent_default();
        self.lost = true;
        if let Ok(label) = element::<HtmlElement>(&self.document, "start-label") {
            label.set_inner_text("Reload to continue");
        }
        if let Some(p) = self.overlay.query_selector("p").ok().flatten().and_then(|p| p.dyn_into::<HtmlElement>().ok()) {
            p.set_inner_text("The browser took the graphics away from the game. Reload the page to carry on.");
        }
        self.overlay.set_hidden(false);
    }

    /// The context is back but empty, so everything the renderer uploaded goes up again.
    fn context_restored(&mut self) {
        match Renderer::new(&self.gl, Profile::Gles) {
            Ok(renderer) => {
                self.renderer = renderer;
                self.lost = false;
                self.overlay.set_hidden(true);
            }
            Err(e) => web_sys::console::error_1(&format!("graphics: {e}").into()),
        }
    }

    fn down(&mut self, e: &PointerEvent) {
        self.begin();
        // In an iframe, as on itch.io, the keys only come once the page has the focus.
        let _ = self.canvas.focus();
        if e.pointer_type() == "mouse" && e.button() != 0 {
            return;
        }
        if let Some(p) = self.to_screen(e)
            && self.touches.len() < MAX_TOUCHES
        {
            let _ = self.canvas.set_pointer_capture(e.pointer_id());
            self.touches.push(Touch { id: e.pointer_id(), at: p, seen: false, lifted: false });
            e.prevent_default();
        }
    }

    fn moved(&mut self, e: &PointerEvent) {
        let [w, h] = [self.canvas.client_width().max(1) as f32, self.canvas.client_height().max(2) as f32];
        let p = [(e.offset_x() as f32 / w).clamp(0.0, 1.0) * SCREEN_W, (e.offset_y() as f32 / h * 2.0 - 1.0).clamp(0.0, 1.0) * SCREEN_H];
        if let Some(t) = self.touches.iter_mut().find(|t| t.id == e.pointer_id() && !t.lifted) {
            t.at = p;
        }
    }

    fn up(&mut self, e: &PointerEvent) {
        self.touches.retain_mut(|t| {
            t.lifted |= t.id == e.pointer_id();
            !(t.lifted && t.seen)
        });
    }

    fn key(&mut self, e: &KeyboardEvent, down: bool) {
        if e.ctrl_key() || e.meta_key() || e.alt_key() {
            return;
        }
        if down {
            self.begin();
        }
        let code = e.code();
        if code == "KeyC" {
            if down && !e.repeat() {
                self.lid_key = !self.lid_key;
            }
        } else if let Some(b) = key_button(&code) {
            self.keys.set(b, down);
            if down {
                self.struck |= b;
            }
        } else {
            return;
        }
        e.prevent_default();
    }

    fn measure(&mut self, frame_ms: f64) {
        let (frames, total, since) = &mut self.stats;
        *frames += 1;
        *total += frame_ms;
        let t = now();
        if t - *since >= STATS_MS {
            self.system.fps = *frames as f32 * 1000.0 / (t - *since) as f32;
            self.system.frame_ms = (*total / *frames as f64) as f32;
            self.stats = (0, 0.0, t);
        }
    }

    fn persist(&mut self) {
        let save = self.game.save();
        if save == self.saved {
            return;
        }
        let Some(s) = &self.storage else {
            self.saved = save;
            return;
        };
        if s.set_item(SAVE_KEY, &save.to_text()).is_ok() {
            self.saved = save;
            self.refused = false;
        } else if !std::mem::replace(&mut self.refused, true) {
            web_sys::console::warn_1(&"save: the browser's storage refused it; trying again each frame".into());
        }
    }

    fn frame(&mut self) {
        if self.lost {
            self.last = now();
            return;
        }
        let t = now();
        let frame_ms = t - self.last;
        let dt = (frame_ms / 1000.0).clamp(0.0, 0.05) as f32;
        self.last = t;
        self.measure(frame_ms);
        self.fit();
        let (pad, stick) = gamepads(&mut self.odd_pad);
        let mut raw = RawInput { held: self.keys | std::mem::take(&mut self.struck) | pad, stick, touch: self.touches.first().map(|t| t.at), lid_closed: self.lid_key, ..Default::default() };
        for (slot, t) in raw.touches.iter_mut().zip(&mut self.touches) {
            *slot = Some(t.at);
            t.seen = true;
        }
        self.touches.retain(|t| !t.lifted);
        let input = self.tracker.frame(raw);
        self.game.set_system(self.system);
        self.game.update(&input, dt);
        self.persist();
        let sounds = self.game.take_sounds();
        if let Some(audio) = &self.audio {
            audio.set_volume(self.game.volume());
            let outputs = self.game.outputs();
            audio.set_dread(outputs.dread);
            audio.set_beds(outputs.screens_on);
            sounds.into_iter().for_each(|s| audio.play(s));
        }
        let frame = self.game.draw();
        let (w, h) = (self.canvas.width() as i32, self.canvas.height() as i32 / 2);
        self.renderer.draw(&self.gl, &frame.top, [0, h, w, h]);
        self.renderer.draw(&self.gl, &frame.bottom, [0, 0, w, h]);
    }

    /// A hidden page draws no frames, so the music and ambience fade out until it shows again.
    fn hidden(&mut self) {
        if let Some(audio) = &self.audio
            && self.document.hidden()
        {
            audio.set_dread(0.0);
            audio.set_beds(false);
        }
    }
}

type Frame = Closure<dyn FnMut()>;

/// Calls `f` with the page's state on each `kind` event at `target`.
fn listen<E: JsCast + 'static>(target: &EventTarget, kind: &str, web: &Rc<RefCell<Web>>, f: impl Fn(&mut Web, &E) + 'static) {
    let web = web.clone();
    let cb = Closure::<dyn FnMut(Event)>::new(move |e: Event| {
        if let Ok(mut w) = web.try_borrow_mut() {
            f(&mut w, e.unchecked_ref());
        }
    });
    let opts = AddEventListenerOptions::new();
    opts.set_passive(false);
    target.add_event_listener_with_callback_and_add_event_listener_options(kind, cb.as_ref().unchecked_ref(), &opts).expect("listening");
    cb.forget();
}

fn element<T: JsCast>(document: &Document, id: &str) -> Result<T, String> {
    document.get_element_by_id(id).and_then(|e| e.dyn_into().ok()).ok_or(format!("#{id} missing from the page"))
}

fn start(pak: Pak) -> Result<(), String> {
    // The renderer uploads the sprite atlas as it starts, so the data goes in first.
    let sounds = pak::install(Some(pak), None)?;
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let canvas: HtmlCanvasElement = element(&document, "screens")?;
    let overlay: HtmlElement = element(&document, "start")?;
    let options = js_sys::Object::new();
    for (k, v) in [("antialias", true), ("alpha", false), ("depth", true)] {
        js_sys::Reflect::set(&options, &k.into(), &v.into()).map_err(|e| format!("{e:?}"))?;
    }
    let context = canvas.get_context_with_context_options("webgl2", &options).ok().flatten().ok_or("this browser has no WebGL2")?;
    let gl = glow::Context::from_webgl2_context(context.dyn_into::<WebGl2RenderingContext>().map_err(|_| "not a WebGL2 context")?);
    let renderer = Renderer::new(&gl, Profile::Gles)?;
    let storage = window.local_storage().ok().flatten();
    if storage.is_none() {
        web_sys::console::warn_1(&"save: the browser gives this page no storage, so progress won't be kept".into());
    }
    let saved = storage.as_ref().and_then(|s| s.get_item(SAVE_KEY).ok().flatten()).map(|t| Save::parse(&t)).unwrap_or_default();
    let mut game = Game::new(js_sys::Date::now() as u64);
    game.load(saved);
    let web = Rc::new(RefCell::new(Web {
        game,
        tracker: InputTracker::default(),
        gl,
        renderer,
        canvas: canvas.clone(),
        document: document.clone(),
        overlay: overlay.clone(),
        sounds: Some(sounds),
        audio: None,
        keys: Buttons::NONE,
        struck: Buttons::NONE,
        lid_key: false,
        touches: Vec::new(),
        storage,
        saved,
        refused: false,
        lost: false,
        odd_pad: false,
        last: now(),
        stats: (0, 0.0, now()),
        system: System::default(),
    }));

    listen(&overlay, "pointerdown", &web, |w, e: &PointerEvent| {
        e.prevent_default();
        w.begin();
        let _ = w.canvas.focus();
    });
    listen(&canvas, "pointerdown", &web, |w, e: &PointerEvent| w.down(e));
    listen(&canvas, "pointermove", &web, |w, e: &PointerEvent| w.moved(e));
    for kind in ["pointerup", "pointercancel"] {
        listen(&canvas, kind, &web, |w, e: &PointerEvent| w.up(e));
    }
    listen(&canvas, "contextmenu", &web, |_, e: &Event| e.prevent_default());
    listen(&canvas, "webglcontextlost", &web, |w, e: &Event| w.context_lost(e));
    listen(&canvas, "webglcontextrestored", &web, |w, _: &Event| w.context_restored());
    listen(&window, "keydown", &web, |w, e: &KeyboardEvent| w.key(e, true));
    listen(&window, "keyup", &web, |w, e: &KeyboardEvent| w.key(e, false));
    // A key let go while the page had no focus would otherwise stay down.
    listen(&window, "blur", &web, |w, _: &Event| w.keys = Buttons::NONE);
    listen(&document, "visibilitychange", &web, |w, _: &Event| w.hidden());

    let tick: Rc<RefCell<Option<Frame>>> = Rc::new(RefCell::new(None));
    let next = tick.clone();
    *tick.borrow_mut() = Some(Closure::new(move || {
        if let Ok(mut w) = web.try_borrow_mut() {
            w.frame();
        }
        if let (Some(window), Some(cb)) = (web_sys::window(), next.borrow().as_ref()) {
            let _ = window.request_animation_frame(cb.as_ref().unchecked_ref());
        }
    }));
    let _ = window.request_animation_frame(tick.borrow().as_ref().ok_or("no frame callback")?.as_ref().unchecked_ref());
    Ok(())
}

/// Shows a failure on the page, where a player can see it.
fn fail(text: &str) {
    web_sys::console::error_1(&text.into());
    if let Some(body) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.body()) {
        let note = body.owner_document().and_then(|d| d.create_element("pre").ok());
        if let Some(note) = note.and_then(|n| n.dyn_into::<HtmlElement>().ok()) {
            note.set_class_name("failure");
            note.set_inner_text(text);
            let _ = body.prepend_with_node_1(&note);
        }
    }
}

/// Starts the game on the page's canvas with the bytes of `game_data.pak`, which the page fetches.
#[wasm_bindgen]
pub fn run(pak: Vec<u8>) -> Result<(), String> {
    std::panic::set_hook(Box::new(|info| fail(&info.to_string())));
    start(Pak::read(std::io::Cursor::new(pak)).map_err(|e| format!("game_data.pak: {e}"))?)
}
