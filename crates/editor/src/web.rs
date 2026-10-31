//! The editor in a web page, for a tablet: the same panes drawn with WebGL2, fingers and a pen in place of the
//! mouse, a toolbar in place of the keyboard shortcuts, and `sprite-server` in place of the disk.

#[cfg(not(feature = "modding"))]
mod http;
#[cfg(feature = "modding")]
mod local;

use std::cell::RefCell;
use std::rc::Rc;

use last_harvest_game::SCREEN_W;
use last_harvest_render::{Profile, Renderer};
use sprite_editor::editor::Key;
use sprite_editor::store::Store;
use sprite_editor::studio::{Pane, Studio};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{AddEventListenerOptions, Document, Element, Event, EventTarget, HtmlCanvasElement, HtmlElement, HtmlInputElement, KeyboardEvent, PointerEvent, WebGl2RenderingContext, WheelEvent};

/// How far a finger may wander, in CSS pixels, and still tap; and how long it rests, in ms, before it draws.
const TAP: f32 = 8.0;
const HOLD: f64 = 120.0;
/// Room left under the panes for the toolbar, in CSS pixels.
const TOOLBAR: f32 = 180.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Gesture {
    Idle,
    /// A finger on the pixel canvas, not yet known to be a stroke or the first of a pinch.
    Waiting { id: i32, at: [f32; 2], since: f64 },
    /// A press handed to the studio.
    Held { id: i32 },
    /// A finger on the sprite list, scrolling it once it moves.
    List { id: i32, at: [f32; 2], moved: bool },
    /// Two fingers zooming and panning the pane they began in, until both lift.
    Pinch { pane: Pane },
}

struct Web {
    studio: Studio,
    gl: glow::Context,
    renderer: Renderer,
    canvas: HtmlCanvasElement,
    input: HtmlInputElement,
    document: Document,
    /// The panes' size in CSS pixels, and device pixels per CSS pixel.
    size: [f32; 2],
    dpr: f32,
    last: f64,
    gesture: Gesture,
    /// Fingers down, and where each is.
    touches: Vec<(i32, [f32; 2])>,
    /// The pinch's last center and spread.
    spread: ([f32; 2], f32),
    pointer: [f32; 2],
    /// The toolbar's Shift, standing in for the key until tapped again.
    shift: bool,
    /// Fingers on the corner Shift pads, by pad and pointer, which hold Shift down while they stay.
    holding: Vec<(usize, i32)>,
    /// A tap on a corner pad holds Shift for the next press, since iPadOS drops a pencil's touch while a finger is down.
    armed: bool,
    /// Whether the keyboard's Shift was down when the last pointer landed.
    key_shift: bool,
    i_held: bool,
    title: String,
}

fn now() -> f64 {
    web_sys::window().and_then(|w| w.performance()).map_or(0.0, |p| p.now())
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

impl Web {
    fn locate(&self, p: [f32; 2]) -> (Pane, [f32; 2]) {
        Studio::locate(self.size, p)
    }

    fn in_pane(&self, pane: Pane, p: [f32; 2]) -> [f32; 2] {
        Studio::in_pane(self.size, pane, p)
    }

    fn layout(&mut self) {
        let Some(window) = web_sys::window() else { return };
        let inner = |v: Result<JsValue, JsValue>| v.ok().and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let (iw, ih) = (inner(window.inner_width()), inner(window.inner_height()));
        let room = (ih - TOOLBAR).max(200.0);
        self.size = if ih > iw {
            let w = iw.min(room * 8.0 / 9.0).floor();
            [w, (w * 9.0 / 8.0).floor()]
        } else {
            let w = iw.min(room * 2.0).floor();
            [w, (w / 2.0).floor()]
        };
        self.dpr = window.device_pixel_ratio() as f32;
        let style = self.canvas.style();
        let _ = style.set_property("width", &format!("{}px", self.size[0]));
        let _ = style.set_property("height", &format!("{}px", self.size[1]));
        self.canvas.set_width((self.size[0] * self.dpr).round() as u32);
        self.canvas.set_height((self.size[1] * self.dpr).round() as u32);
    }

    fn frame(&mut self) {
        let t = now();
        let dt = ((t - self.last) / 1000.0).clamp(0.0, 0.05) as f32;
        self.last = t;
        if let Gesture::Waiting { id, at, since } = self.gesture
            && t - since > HOLD
        {
            self.begin(id, at);
        }
        if let Some(pixels) = self.studio.tick(dt) {
            self.renderer.set_art(&self.gl, pixels);
        }
        let px = |v: f32| (v * self.dpr).round() as i32;
        for (canvas, [x, y, w, h]) in self.studio.canvases().iter().zip(Studio::panes(self.size)) {
            self.renderer.draw(&self.gl, canvas, [px(x), px(self.size[1] - y - h), px(w), px(h)]);
        }
        let title = self.studio.editor.title();
        if title != self.title {
            self.document.set_title(&title);
            self.title = title;
        }
        self.sync_text(false);
    }

    /// Hands a press at `at` to the studio.
    fn begin(&mut self, id: i32, at: [f32; 2]) {
        let (pane, p) = self.locate(at);
        let shift = self.press_shift();
        self.studio.press(pane, p, false, shift);
        self.pointer = at;
        self.gesture = Gesture::Held { id };
    }

    fn motion(&mut self, p: [f32; 2]) {
        let (pane, at) = self.studio.locate_move(self.size, p);
        // Only the 2.5D view turns by how far the pointer moves, so it is measured across a preview.
        let dx = (p[0] - self.pointer[0]) / Studio::panes(self.size)[1][2] * SCREEN_W;
        self.studio.motion(pane, at, dx);
        self.pointer = p;
    }

    fn spread_now(&self) -> Option<([f32; 2], f32)> {
        let [(_, a), (_, b), ..] = self.touches[..] else { return None };
        Some(([(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0], distance(a, b).max(1.0)))
    }

    fn down(&mut self, e: &PointerEvent) {
        let (id, p) = (e.pointer_id(), [e.offset_x() as f32, e.offset_y() as f32]);
        let _ = self.canvas.set_pointer_capture(id);
        self.key_shift = e.shift_key();
        if e.pointer_type() != "touch" {
            // A pen or mouse takes over from any finger, such as a palm resting on the screen.
            self.cancel_touches();
            let (pane, at) = self.locate(p);
            let shift = self.press_shift();
            self.studio.press(pane, at, e.button() == 2, shift);
            self.pointer = p;
            self.gesture = Gesture::Held { id };
            return;
        }
        if matches!(self.gesture, Gesture::Held { id: held } if !self.touches.iter().any(|(t, _)| *t == held)) {
            return;
        }
        self.touches.push((id, p));
        match self.touches.len() {
            1 => {
                let (pane, at) = self.locate(p);
                let editing = pane == Pane::Edit;
                self.gesture = if editing && self.studio.editor.on_canvas(at) {
                    Gesture::Waiting { id, at: p, since: now() }
                } else if editing && self.studio.editor.on_list(at) {
                    self.pointer = p;
                    Gesture::List { id, at: p, moved: false }
                } else {
                    self.begin(id, p);
                    self.gesture
                };
            }
            2 => {
                if matches!(self.gesture, Gesture::Held { .. }) {
                    self.studio.release();
                }
                let pane = self.locate(self.touches[0].1).0;
                self.spread = self.spread_now().unwrap_or_default();
                self.gesture = Gesture::Pinch { pane };
            }
            _ => {}
        }
    }

    /// Drops what the fingers were doing, without the tap or stroke they would have made.
    fn cancel_touches(&mut self) {
        if !self.touches.is_empty() {
            if matches!(self.gesture, Gesture::Held { .. }) {
                self.studio.release();
            }
            self.touches.clear();
            self.gesture = Gesture::Idle;
        }
    }

    fn moved(&mut self, e: &PointerEvent) {
        let (id, p) = (e.pointer_id(), [e.offset_x() as f32, e.offset_y() as f32]);
        if let Some(t) = self.touches.iter_mut().find(|(t, _)| *t == id) {
            t.1 = p;
        }
        match self.gesture {
            Gesture::Waiting { id: w, at, since } if w == id => {
                if distance(p, at) > TAP || now() - since > HOLD {
                    self.begin(id, at);
                    self.motion(p);
                }
            }
            Gesture::Held { id: h } if h == id => self.motion(p),
            Gesture::List { id: l, at, moved } if l == id => {
                if moved || (p[1] - at[1]).abs() > TAP {
                    let dy = self.in_pane(Pane::Edit, p)[1] - self.in_pane(Pane::Edit, self.pointer)[1];
                    self.studio.editor.drag_list(dy);
                    self.gesture = Gesture::List { id, at, moved: true };
                }
                self.pointer = p;
            }
            Gesture::Pinch { pane } => {
                let Some((center, spread)) = self.spread_now() else { return };
                let (was, before) = self.spread;
                let [a, b] = [self.in_pane(pane, was), self.in_pane(pane, center)];
                self.studio.pinch(pane, b, spread / before, [b[0] - a[0], b[1] - a[1]]);
                self.spread = (center, spread);
            }
            Gesture::Idle if e.pointer_type() != "touch" => self.motion(p),
            _ => {}
        }
    }

    fn up(&mut self, e: &PointerEvent, cancelled: bool) {
        let id = e.pointer_id();
        self.touches.retain(|(t, _)| *t != id);
        match self.gesture {
            Gesture::Waiting { id: w, at, .. } if w == id => {
                if !cancelled {
                    self.begin(id, at);
                }
                self.studio.release();
                self.gesture = Gesture::Idle;
            }
            Gesture::List { id: l, at, moved } if l == id => {
                if !moved && !cancelled {
                    self.begin(id, at);
                    self.studio.release();
                }
                self.gesture = Gesture::Idle;
            }
            Gesture::Held { id: h } if h == id => {
                self.studio.release();
                self.gesture = Gesture::Idle;
            }
            Gesture::Pinch { .. } if self.touches.is_empty() => self.gesture = Gesture::Idle,
            _ => {}
        }
        if e.pointer_type() != "mouse" {
            self.studio.editor.unhover();
        }
        self.sync_text(true);
    }

    fn wheel(&mut self, e: &WheelEvent) {
        let p = [e.offset_x() as f32, e.offset_y() as f32];
        let (pane, at) = self.locate(p);
        let lines = if e.delta_mode() == 0 { e.delta_y() as f32 / 20.0 } else { e.delta_y() as f32 };
        if e.ctrl_key() {
            // A trackpad pinch arrives as a wheel with Ctrl held.
            self.studio.pinch(pane, at, (-lines * 0.2).exp(), [0.0; 2]);
        } else {
            self.studio.wheel(pane, at, -lines);
        }
    }

    fn press_key(&mut self, k: Key, shift: bool) {
        self.studio.editor.nudge = if shift || self.shifted() { 8 } else { 1 };
        self.studio.editor.key(k);
        self.sync_text(true);
    }

    /// A key from a hardware keyboard; whether the editor took it.
    fn key(&mut self, e: &KeyboardEvent, down: bool) -> bool {
        let key = e.key();
        if key.eq_ignore_ascii_case("i") && !self.studio.editor.wants_text() {
            if !e.repeat() {
                self.i_held = down;
            }
            self.studio.editor.peek(self.i_held || e.alt_key());
            return true;
        }
        self.studio.editor.peek(self.i_held || e.alt_key());
        if !down {
            return false;
        }
        let cmd = e.meta_key() || e.ctrl_key();
        let k = match key.as_str() {
            k if cmd => match k.to_lowercase().as_str() {
                "z" if e.shift_key() => Key::Redo,
                "z" => Key::Undo,
                "y" => Key::Redo,
                "s" => Key::Save,
                "c" => Key::Copy,
                "x" => Key::Cut,
                "v" => Key::Paste,
                _ => return false,
            },
            " " => Key::Char(' '),
            "Enter" => Key::Enter,
            "Escape" => Key::Escape,
            "Backspace" | "Delete" => Key::Backspace,
            "ArrowUp" => Key::Up,
            "ArrowDown" => Key::Down,
            "ArrowLeft" => Key::Left,
            "ArrowRight" => Key::Right,
            k if k.chars().count() == 1 => Key::Char(k.chars().next().unwrap_or(' ').to_ascii_lowercase()),
            _ => return false,
        };
        self.press_key(k, e.shift_key());
        true
    }

    /// Whether Shift is down: the toolbar's toggle, or a finger on a corner pad.
    fn shifted(&self) -> bool {
        self.shift || self.armed || !self.holding.is_empty()
    }

    /// Whether a press on the canvas is shifted, using up a pad's tap.
    fn press_shift(&mut self) -> bool {
        let shift = self.shifted() || self.key_shift;
        if self.armed {
            self.armed = false;
            self.light_pads();
        }
        shift
    }

    /// A finger landing on (`down`) or leaving corner pad `pad`.
    fn hold(&mut self, pad: usize, id: i32, down: bool) {
        self.holding.retain(|&(p, i)| !(p == pad && i == id));
        if down {
            self.holding.push((pad, id));
            self.armed = !self.armed;
        }
        self.light_pads();
    }

    fn light_pads(&self) {
        for (pad, name) in HOLD_PADS.iter().enumerate() {
            if let Some(el) = self.document.get_element_by_id(name) {
                let _ = el.class_list().toggle_with_force("on", self.armed || self.holding.iter().any(|&(p, _)| p == pad));
            }
        }
    }

    /// A toolbar button, by its `data-act`.
    fn act(&mut self, act: &str) {
        let k = match act {
            "undo" => Key::Undo,
            "redo" => Key::Redo,
            "copy" => Key::Copy,
            "cut" => Key::Cut,
            "paste" => Key::Paste,
            "delete" => Key::Backspace,
            "enter" => Key::Enter,
            "escape" => Key::Escape,
            "left" => Key::Left,
            "right" => Key::Right,
            "up" => Key::Up,
            "down" => Key::Down,
            "prev-sprite" => Key::Char('k'),
            "next-sprite" => Key::Char('j'),
            "prev-frame" => Key::Char('h'),
            "next-frame" => Key::Char('l'),
            "search" => Key::Char('/'),
            "history" => Key::Char('r'),
            "new" => Key::Char('n'),
            "play" => Key::Char(' '),
            "mirror" => Key::Char('m'),
            "save" => Key::Save,
            "fit" => return self.studio.editor.fit(),
            "palette" => return call("pick", &[".hex,.txt".into(), false.into(), "lastharvest-palette".into()]),
            #[cfg(feature = "modding")]
            "import" => return modding::call("pick", &[".png,.pak,.txt".into(), true.into(), "lastharvest-import".into()]),
            #[cfg(feature = "modding")]
            "export" => {
                let editor = &mut self.studio.editor;
                let out = if editor.map_source().is_some() {
                    editor.export_map().and_then(|(name, files)| match modding::pack_of(&name, files) {
                        Ok(bytes) => Some((name, bytes)),
                        Err(e) => {
                            editor.say(format!("Couldn't pack the map: {e}"));
                            None
                        }
                    })
                } else if sprite_editor::store::props::id(&editor.sprite.name).is_some() {
                    editor.export_prop()
                } else {
                    editor.export_card()
                };
                if let Some((name, bytes)) = out {
                    modding::call("save", &[name.into(), js_sys::Uint8Array::from(&bytes[..]).into()]);
                }
                return;
            }
            #[cfg(feature = "modding")]
            "pak" => return modding::call("choosePak", &[]),
            #[cfg(feature = "modding")]
            "delete-card" => return self.studio.editor.delete_card(),
            #[cfg(feature = "modding")]
            "sound" => return modding::call("pick", &[".ogg,.wav".into(), false.into(), "lastharvest-sound".into()]),
            #[cfg(feature = "modding")]
            "export-all" => {
                let extra = self.studio.editor.all_maps_and_props();
                let cards = self.studio.editor.all_cards();
                if !cards.is_empty() || !extra.is_empty() {
                    let sounds = self.studio.editor.all_sounds();
                    match modding::mod_pack(&cards, &sounds, &extra) {
                        Ok(bytes) => {
                            modding::call("save", &[modding::MOD_FILE.into(), js_sys::Uint8Array::from(&bytes[..]).into()]);
                            let with = if sounds.is_empty() { String::new() } else { format!(" and {} death sounds", sounds.len()) };
                            let maps = if extra.is_empty() { String::new() } else { format!(", {} maps, props and data files", extra.len()) };
                            self.studio.editor.say(format!("Exported {} cards{with}{maps} as {}: put it in the game's data/mods", cards.len(), modding::MOD_FILE));
                        }
                        Err(e) => self.studio.editor.say(format!("Couldn't pack the cards: {e}")),
                    }
                }
                return;
            }
            "shift" => {
                self.shift = !self.shift;
                if let Some(b) = self.document.query_selector("[data-act=shift]").ok().flatten() {
                    let _ = b.class_list().toggle_with_force("on", self.shift);
                }
                return;
            }
            _ => return,
        };
        self.press_key(k, false);
    }

    /// Shows the text field while a name or search is being typed, and hides it after. Focusing it brings up
    /// the system keyboard only while handling a tap, so `focus` says whether this is one.
    fn sync_text(&mut self, focus: bool) {
        let typing = self.document.active_element().is_some_and(|a| a == **self.input);
        match self.studio.editor.text().map(str::to_string) {
            Some(t) => {
                if self.input.hidden() {
                    let free = self.studio.editor.free_text();
                    let _ = self.input.set_attribute("autocapitalize", if free { "sentences" } else { "characters" });
                    let _ = self.input.style().set_property("text-transform", if free { "none" } else { "uppercase" });
                    self.input.set_hidden(false);
                    self.input.set_value(&t);
                } else if self.input.value() != t {
                    // Rename and About hand the open field from one line to the other.
                    self.input.set_value(&t);
                }
                if focus && !typing {
                    let _ = self.input.focus();
                }
            }
            None if !self.input.hidden() => {
                self.input.set_hidden(true);
                let _ = self.input.blur();
            }
            None => {}
        }
    }

    fn typed(&mut self) {
        self.studio.editor.set_text(&self.input.value());
        if let Some(t) = self.studio.editor.text() {
            self.input.set_value(t);
        }
    }
}

type Frame = Closure<dyn FnMut()>;

/// The corner pads a finger holds to keep Shift down while the pencil draws, for either hand.
const HOLD_PADS: [&str; 2] = ["hold-left", "hold-right"];

/// Calls `f` with the page's state on each `kind` event at `target`, which may cancel it.
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

fn start() -> Result<(), String> {
    // The renderer uploads the sprite atlas as it starts, so the mod editor's data goes in first.
    #[cfg(feature = "modding")]
    let note = modding::install_data()?;
    #[cfg(not(feature = "modding"))]
    let note: Option<String> = None;
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let canvas: HtmlCanvasElement = element(&document, "panes")?;
    let input: HtmlInputElement = element(&document, "text")?;
    let toolbar: Element = element(&document, "toolbar")?;
    let pads: Vec<Element> = HOLD_PADS.iter().map(|id| element(&document, id)).collect::<Result<_, _>>()?;
    let options = js_sys::Object::new();
    for (k, v) in [("antialias", true), ("alpha", false), ("depth", true)] {
        js_sys::Reflect::set(&options, &k.into(), &v.into()).map_err(|e| format!("{e:?}"))?;
    }
    let context = canvas.get_context_with_context_options("webgl2", &options).ok().flatten().ok_or("this browser has no WebGL2")?;
    let gl = glow::Context::from_webgl2_context(context.dyn_into::<WebGl2RenderingContext>().map_err(|_| "not a WebGL2 context")?);
    let renderer = Renderer::new(&gl, Profile::Gles)?;
    let search = window.location().search().unwrap_or_default();
    let open = search.trim_start_matches('?').split('&').find_map(|kv| kv.strip_prefix("open=")).unwrap_or("").to_string();
    #[cfg(feature = "modding")]
    let store: Box<dyn Store> = {
        modding::show_tools(&document);
        Box::new(local::LocalStore::open()?)
    };
    #[cfg(not(feature = "modding"))]
    let store: Box<dyn Store> = Box::new(http::HttpStore);
    let mut studio = Studio::new(store, &open);
    #[cfg(feature = "modding")]
    studio.editor.set_modding();
    if let Some(note) = note {
        studio.editor.say(note);
    }
    studio.hints = ["drag to turn, pinch to move closer", "pinch to zoom"];
    let web = Rc::new(RefCell::new(Web {
        studio,
        gl,
        renderer,
        canvas: canvas.clone(),
        input: input.clone(),
        document: document.clone(),
        size: [1.0; 2],
        dpr: 1.0,
        last: now(),
        gesture: Gesture::Idle,
        touches: Vec::new(),
        spread: ([0.0; 2], 1.0),
        pointer: [0.0; 2],
        shift: false,
        holding: Vec::new(),
        armed: false,
        key_shift: false,
        i_held: false,
        title: String::new(),
    }));
    web.borrow_mut().layout();

    listen(&canvas, "pointerdown", &web, |w, e: &PointerEvent| {
        e.prevent_default();
        w.down(e);
    });
    listen(&canvas, "pointermove", &web, |w, e: &PointerEvent| w.moved(e));
    listen(&canvas, "pointerup", &web, |w, e: &PointerEvent| w.up(e, false));
    listen(&canvas, "pointercancel", &web, |w, e: &PointerEvent| w.up(e, true));
    listen(&canvas, "contextmenu", &web, |_, e: &Event| e.prevent_default());
    listen(&canvas, "wheel", &web, |w, e: &WheelEvent| {
        e.prevent_default();
        w.wheel(e);
    });
    for kind in ["gesturestart", "gesturechange", "touchmove"] {
        listen(&document, kind, &web, |_, e: &Event| e.prevent_default());
    }
    listen(&window, "resize", &web, |w, _: &Event| w.layout());
    listen(&window, "keydown", &web, |w, e: &KeyboardEvent| {
        if e.target().and_then(|t| t.dyn_into::<HtmlInputElement>().ok()).is_none() && w.key(e, true) {
            e.prevent_default();
        }
    });
    listen(&window, "keyup", &web, |w, e: &KeyboardEvent| {
        w.key(e, false);
    });
    // Safari sends no click for a tap that moved while touchmove is cancelled, so buttons act on pointerup.
    for (pad, el) in pads.iter().enumerate() {
        let target = el.clone();
        listen(el, "pointerdown", &web, move |w, e: &PointerEvent| {
            e.prevent_default();
            let _ = target.set_pointer_capture(e.pointer_id());
            w.hold(pad, e.pointer_id(), true);
        });
        for kind in ["pointerup", "pointercancel"] {
            listen(el, kind, &web, move |w, e: &PointerEvent| w.hold(pad, e.pointer_id(), false));
        }
    }
    listen(&toolbar, "pointerup", &web, |w, e: &PointerEvent| {
        let act_at = |t: Option<Element>| t.and_then(|t| t.closest("[data-act]").ok().flatten()).and_then(|b| b.get_attribute("data-act"));
        let pressed = act_at(e.target().and_then(|t| t.dyn_into::<Element>().ok()));
        let lifted = act_at(w.document.element_from_point(e.client_x() as f32, e.client_y() as f32));
        if let Some(act) = pressed.filter(|a| e.button() == 0 && lifted.as_ref() == Some(a)) {
            w.act(&act);
        }
    });
    listen(&input, "input", &web, |w, _: &Event| w.typed());
    listen(&window, "lastharvest-palette", &web, |w, e: &web_sys::CustomEvent| {
        let detail = e.detail();
        let name = js_sys::Reflect::get(&detail, &"name".into()).ok().and_then(|v| v.as_string()).unwrap_or_default();
        let bytes = js_sys::Reflect::get(&detail, &"bytes".into()).map(|v| js_sys::Uint8Array::new(&v).to_vec()).unwrap_or_default();
        let stem = name.rsplit_once('.').map_or(name.as_str(), |(s, _)| s);
        w.studio.editor.use_palette(stem, &String::from_utf8_lossy(&bytes));
    });
    #[cfg(feature = "modding")]
    listen(&window, "lastharvest-import", &web, |w, e: &web_sys::CustomEvent| {
        let detail = e.detail();
        let name = js_sys::Reflect::get(&detail, &"name".into()).ok().and_then(|v| v.as_string()).unwrap_or_default();
        let bytes = js_sys::Reflect::get(&detail, &"bytes".into()).map(|v| js_sys::Uint8Array::new(&v).to_vec()).unwrap_or_default();
        if name.ends_with(".pak") {
            match modding::cards_in(&bytes) {
                Ok((cards, _, maps, props)) if cards.is_empty() && maps.is_empty() && props.is_empty() => w.studio.editor.say(format!("{name} holds no cards, maps, props or data")),
                Ok((cards, sounds, maps, props)) => {
                    if !maps.is_empty() || !props.is_empty() {
                        w.studio.editor.import_maps(&name, maps, props);
                    }
                    if !cards.is_empty() {
                        w.studio.editor.import_pack(&name, cards, sounds);
                    }
                }
                Err(e) => w.studio.editor.say(format!("{name}: {e}")),
            }
        } else if name.ends_with(last_harvest_game::MAP_EXTENSION) {
            w.studio.editor.import_maps(&name, vec![(name.clone(), bytes)], Vec::new());
        } else if name.ends_with(last_harvest_pack::prop::EXTENSION) {
            w.studio.editor.import_maps(&name, Vec::new(), vec![(name.clone(), bytes)]);
        } else {
            w.studio.editor.import_card(&name, &bytes);
        }
    });
    #[cfg(feature = "modding")]
    listen(&window, "lastharvest-sound", &web, |w, e: &web_sys::CustomEvent| {
        let detail = e.detail();
        let name = js_sys::Reflect::get(&detail, &"name".into()).ok().and_then(|v| v.as_string()).unwrap_or_default();
        let bytes = js_sys::Reflect::get(&detail, &"bytes".into()).map(|v| js_sys::Uint8Array::new(&v).to_vec()).unwrap_or_default();
        if let Some(kept) = w.studio.editor.attach_sound(&name, &bytes) {
            modding::call("play", &[kept.into(), js_sys::Uint8Array::from(&bytes[..]).into()]);
        }
    });
    listen(&input, "keydown", &web, |w, e: &KeyboardEvent| {
        let k = match e.key().as_str() {
            "Enter" => Key::Enter,
            "Escape" => Key::Escape,
            _ => return,
        };
        e.prevent_default();
        w.press_key(k, false);
    });

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

/// Calls one of the page's helpers on `window.lastHarvest`.
pub fn call(method: &str, args: &[JsValue]) {
    let Some(window) = web_sys::window() else { return };
    let Ok(helpers) = js_sys::Reflect::get(&window, &"lastHarvest".into()) else { return };
    if let Some(f) = js_sys::Reflect::get(&helpers, &method.into()).ok().and_then(|f| f.dyn_into::<js_sys::Function>().ok()) {
        let _ = f.apply(&helpers, &args.iter().collect());
    }
}

/// Shows a failure on the page, where a tablet's user can see it.
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

pub fn main() {
    std::panic::set_hook(Box::new(|info| fail(&info.to_string())));
    if let Err(e) = start() {
        fail(&e);
    }
}

/// The public mod editor's side of the page: its data, its toolbar group, and the page's helpers in `index.html`.
#[cfg(feature = "modding")]
mod modding {
    use last_harvest_game::art::{self, ATLAS_H, ATLAS_W};
    use last_harvest_game::data::{self, Data};
    use last_harvest_pack::{ATLAS, Pak};
    use wasm_bindgen::JsCast;
    use web_sys::{Document, HtmlElement};

    /// Installs the sprite atlas from the player's own game_data.pak, which the page keeps in IndexedDB, else the
    /// site's, either left on `window.lastHarvestPak`, or a blank one; returns a word for the player when their pack
    /// couldn't be used.
    pub fn install_data() -> Result<Option<String>, String> {
        let window = web_sys::window().ok_or("no window")?;
        let pak = js_sys::Reflect::get(&window, &"lastHarvestPak".into()).ok().filter(|v| !v.is_undefined() && !v.is_null());
        let shared = js_sys::Reflect::get(&window, &"lastHarvestPakShared".into()).is_ok_and(|v| v.is_truthy());
        let size = (ATLAS_W * ATLAS_H * 4) as usize;
        // A stored pack that can't be used must leave the page working, or Game data could never replace it.
        let (atlas, note) = match pak.map(|v| js_sys::Uint8Array::new(&v).to_vec()) {
            Some(bytes) => match Pak::read(std::io::Cursor::new(bytes)).map(|mut p| p.take(ATLAS)) {
                Ok(Some(atlas)) if atlas.len() == size => (Some(atlas), (!shared).then(|| "Previews use your game data".to_string())),
                Ok(_) => (None, Some("Your game data has no sprites this editor can use".to_string())),
                Err(e) => (None, Some(format!("Your game data isn't for this editor: {e}"))),
            },
            None => (None, None),
        };
        data::install(Data { atlas: Some(atlas.unwrap_or_else(plain_ground)), ..Data::default() })?;
        Ok(note)
    }

    /// An atlas with nothing of the tileset in it but flat colors where the ground tiles go, so a card in the
    /// previews walks on a plain floor rather than in the dark.
    fn plain_ground() -> Vec<u8> {
        const GROUND: [(&str, [u8; 3]); 7] =
            [("DIRT", [74, 60, 44]), ("DIRT_TUFTS", [74, 60, 44]), ("GRASS", [66, 84, 52]), ("PATH", [110, 96, 70]), ("ROAD_V", [52, 52, 58]), ("ROAD_H", [52, 52, 58]), ("SOIL", [88, 64, 44])];
        let mut atlas = vec![0; (ATLAS_W * ATLAS_H * 4) as usize];
        for (name, frames) in art::ALL {
            let Some((_, [r, g, b])) = GROUND.iter().find(|(n, _)| n == name) else { continue };
            for a in frames.iter() {
                let [x, y, w, h] = a.rect().map(usize::from);
                for row in y..y + h {
                    for px in atlas[(row * ATLAS_W as usize + x) * 4..(row * ATLAS_W as usize + x + w) * 4].chunks_exact_mut(4) {
                        px.copy_from_slice(&[*r, *g, *b, 255]);
                    }
                }
            }
        }
        atlas
    }

    pub const MOD_FILE: &str = "my-cards.pak";

    /// The cards in a mod pack, by file name, for Import.
    type Files = Vec<(String, Vec<u8>)>;

    /// The cards in a mod pack, the sounds beside them, its maps with any data files it changes, and its props, by
    /// file name, for Import.
    pub fn cards_in(bytes: &[u8]) -> Result<(Files, Files, Files, Files), String> {
        let mut pak = Pak::read(std::io::Cursor::new(bytes))?;
        let mut cards = pak.take_dir(last_harvest_pack::ENEMY_CARDS);
        cards.extend(pak.take_dir(last_harvest_pack::PLAYER_CARDS));
        let mut maps = pak.take_dir(last_harvest_pack::MAPS);
        maps.extend(last_harvest_game::TABLES.into_iter().filter_map(|file| Some((file.to_string(), pak.take(file)?))));
        Ok((cards, pak.take_dir(last_harvest_pack::SOUNDS), maps, pak.take_dir(last_harvest_pack::PROPS)))
    }

    /// A mod pack named after `name` holding `files` by their paths in it.
    pub fn pack_of(name: &str, mut files: Files) -> Result<Vec<u8>, String> {
        let id = name.strip_suffix(".pak").unwrap_or(name);
        files.insert(0, (last_harvest_pack::MANIFEST.to_string(), last_harvest_pack::mod_manifest(id).into_bytes()));
        let mut out = std::io::Cursor::new(Vec::new());
        last_harvest_pack::write(&mut out, &files).map_err(|e| e.to_string())?;
        Ok(out.into_inner())
    }

    /// Every card as one mod pack, enemy cards under `enemies/`, player cards under `players/` and their death
    /// sounds under `sounds/`, ready for the game's data/mods.
    pub fn mod_pack(cards: &[(String, Vec<u8>)], sounds: &[(String, Vec<u8>)], extra: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
        use last_harvest_pack::card::Kind;
        let mut files = vec![(last_harvest_pack::MANIFEST.to_string(), last_harvest_pack::mod_manifest("my-cards").into_bytes())];
        for (file, bytes) in cards {
            let dir = match Kind::of(file) {
                Some(Kind::Player) => last_harvest_pack::PLAYER_CARDS,
                _ => last_harvest_pack::ENEMY_CARDS,
            };
            files.push((format!("{dir}{file}"), bytes.clone()));
        }
        files.extend(sounds.iter().map(|(file, bytes)| (format!("{}{file}", last_harvest_pack::SOUNDS), bytes.clone())));
        files.extend(extra.iter().cloned());
        let mut out = std::io::Cursor::new(Vec::new());
        last_harvest_pack::write(&mut out, &files).map_err(|e| e.to_string())?;
        Ok(out.into_inner())
    }

    /// Shows the Cards group and hides what only the game's own editor has.
    pub fn show_tools(document: &Document) {
        if let Some(tools) = document.get_element_by_id("mod-tools").and_then(|e| e.dyn_into::<HtmlElement>().ok()) {
            tools.set_hidden(false);
        }
        if let Some(history) = document.query_selector("[data-act=history]").ok().flatten().and_then(|e| e.dyn_into::<HtmlElement>().ok()) {
            history.set_hidden(true);
        }
        call("offline", &[]);
    }

    pub use super::call;
}
