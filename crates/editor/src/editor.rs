//! The editing pane: sprite list, pixel canvas, tools, palette and frames, laid out on a 1024x768 canvas.

use std::path::PathBuf;

use last_harvest_game::art::Art;
use last_harvest_game::{Align, Canvas, Color, Stand};

use crate::sprite::{self, CLEAR, Frame, Rgba, Sprite};

const UNDO_DEPTH: usize = 100;
const LIST: Rect = Rect { x: 8.0, y: 52.0, w: 200.0, h: 690.0 };
const LIST_ROW: f32 = 22.0;
const CANVAS: Rect = Rect { x: 216.0, y: 52.0, w: 560.0, h: 560.0 };
const SIDE_X: f32 = 784.0;
const SIDE_W: f32 = 232.0;
const SWATCH: f32 = 23.0;
const THUMB: f32 = 64.0;
const MAX_SIZE: usize = 128;

const TEXT: Color = [0.92, 0.92, 0.95, 1.0];
const DIM: Color = [0.6, 0.6, 0.66, 1.0];
const PANEL: Color = [0.16, 0.16, 0.19, 1.0];
const BUTTON: Color = [0.24, 0.24, 0.28, 1.0];
const HOT: Color = [0.95, 0.75, 0.3, 1.0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    fn contains(&self, [px, py]: [f32; 2]) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Pencil,
    Eraser,
    Fill,
    Picker,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ask {
    New,
    Copy,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Action {
    Ask(Ask),
    Save,
    Revert,
    Tool(Tool),
    Mirror,
    AddFrame,
    DeleteFrame,
    MoveFrame(i32),
    Frame(usize),
    Play,
    Width(i32),
    Height(i32),
    Stand(u8),
    Depth(i32),
    Fps(i32),
    Color(Rgba),
}

struct Button {
    rect: Rect,
    action: Action,
    label: String,
    on: bool,
}

/// Keys the editor responds to, after the window has made sense of them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Key {
    Char(char),
    Enter,
    Escape,
    Backspace,
    Left,
    Right,
    Undo,
    Redo,
    Save,
}

pub struct Editor {
    dir: PathBuf,
    pub sprite: Sprite,
    names: Vec<(String, bool)>,
    scroll: f32,
    frame: usize,
    tool: Tool,
    mirror: bool,
    color: Rgba,
    palette: Vec<Rgba>,
    undo: Vec<Vec<Frame>>,
    redo: Vec<Vec<Frame>>,
    /// Edits not yet saved.
    dirty: bool,
    /// The previews need repacking.
    pub changed: bool,
    playing: bool,
    clock: f32,
    stand: u8,
    depth: f32,
    pub fps: f32,
    ask: Option<(Ask, String)>,
    status: Option<(String, f32)>,
    /// The last pixel a stroke painted, so a fast drag joins up.
    stroke: Option<(i32, i32)>,
    channel: Option<usize>,
    hover: Option<[f32; 2]>,
    /// The frames as packed into the scratch area, when they fit.
    live: Vec<Art>,
}

impl Editor {
    pub fn new(dir: PathBuf, name: &str) -> Self {
        let sprite = Sprite::load(&dir, name).unwrap_or(Sprite { name: name.into(), frames: vec![Frame::blank(16, 16)] });
        let names = sprite::catalog(&dir);
        let mut e = Self {
            dir,
            sprite,
            names,
            scroll: 0.0,
            frame: 0,
            tool: Tool::Pencil,
            mirror: false,
            color: [0x55, 0x3c, 0x2a, 255],
            palette: sprite::palette(),
            undo: Vec::new(),
            redo: Vec::new(),
            dirty: false,
            changed: true,
            playing: false,
            clock: 0.0,
            stand: 0,
            depth: 12.0,
            fps: 6.0,
            ask: None,
            status: None,
            stroke: None,
            channel: None,
            hover: None,
            live: Vec::new(),
        };
        e.guess_stand();
        e
    }

    /// How the sprite should stand in the previews.
    pub fn stand(&self) -> Stand {
        match self.stand {
            0 => Stand::Prop { depth: self.depth },
            1 => Stand::Figure,
            _ => Stand::Ground,
        }
    }

    /// The frames to show in the previews: all of them while playing, else the one being edited.
    pub fn shown(&self) -> Vec<Art> {
        if self.playing { self.live.clone() } else { self.live.get(self.frame).copied().into_iter().collect() }
    }

    pub fn set_live(&mut self, live: Vec<Art>) {
        self.live = live;
    }

    pub fn title(&self) -> String {
        format!("{}{} - LastHarvest sprite editor", self.sprite.name, if self.dirty { " *" } else { "" })
    }

    fn current(&self) -> &Frame {
        &self.sprite.frames[self.frame]
    }

    fn guess_stand(&mut self) {
        let n = &self.sprite.name;
        let figure = ["PLAYER", "SKINNY", "KID", "BIG", "CROW"].iter().any(|k| n.starts_with(k));
        let ground = ["DIRT", "GRASS", "PATH", "ROAD", "SOIL", "SPROUTS", "BLOOD", "CORPSE"].iter().any(|k| n.starts_with(k));
        self.stand = if figure { 1 } else if ground { 2 } else { 0 };
        self.depth = (self.current().w as f32 * 0.6).clamp(2.0, 48.0).round();
    }

    fn say(&mut self, text: impl Into<String>) {
        self.status = Some((text.into(), 0.0));
    }

    fn remember(&mut self) {
        self.undo.push(self.sprite.frames.clone());
        if self.undo.len() > UNDO_DEPTH {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
        self.changed = true;
    }

    fn open(&mut self, name: &str) {
        if let Some(s) = Sprite::load(&self.dir, name) {
            self.sprite = s;
            self.frame = 0;
            self.undo.clear();
            self.redo.clear();
            self.dirty = false;
            self.changed = true;
            self.guess_stand();
        }
    }

    pub fn tick(&mut self, dt: f32) {
        self.clock += dt;
        if let Some((_, age)) = &mut self.status {
            *age += dt;
            if *age > 4.0 {
                self.status = None;
            }
        }
        if self.playing && !self.sprite.frames.is_empty() {
            self.frame = (self.clock * self.fps) as usize % self.sprite.frames.len();
        }
    }

    /// Canvas origin and size of one sprite pixel.
    fn grid(&self) -> ([f32; 2], f32) {
        let f = self.current();
        let zoom = (CANVAS.w / f.w as f32).min(CANVAS.h / f.h as f32).floor().max(1.0);
        let (w, h) = (f.w as f32 * zoom, f.h as f32 * zoom);
        ([CANVAS.x + (CANVAS.w - w) / 2.0, CANVAS.y + (CANVAS.h - h) / 2.0], zoom)
    }

    fn pixel_at(&self, p: [f32; 2]) -> (i32, i32) {
        let (o, zoom) = self.grid();
        (((p[0] - o[0]) / zoom).floor() as i32, ((p[1] - o[1]) / zoom).floor() as i32)
    }

    fn buttons(&self) -> Vec<Button> {
        let mut b = Vec::new();
        let mut push = |x: f32, y: f32, w: f32, h: f32, action: Action, label: &str, on: bool| {
            b.push(Button { rect: Rect { x, y, w, h }, action, label: label.into(), on });
        };
        for (i, (a, l)) in [(Action::Ask(Ask::New), "New"), (Action::Ask(Ask::Copy), "Copy"), (Action::Revert, "Revert"), (Action::Save, "Save")].into_iter().enumerate() {
            push(616.0 + i as f32 * 102.0, 8.0, 94.0, 34.0, a, l, a == Action::Save && self.dirty);
        }
        let tools = [(Tool::Pencil, "Pencil  B"), (Tool::Eraser, "Eraser  E"), (Tool::Fill, "Fill  G"), (Tool::Picker, "Pick  I")];
        for (i, (t, l)) in tools.into_iter().enumerate() {
            push(SIDE_X + (i % 2) as f32 * 118.0, 52.0 + (i / 2) as f32 * 38.0, 114.0, 32.0, Action::Tool(t), l, self.tool == t);
        }
        push(SIDE_X, 128.0, SIDE_W, 32.0, Action::Mirror, "Mirror left/right  M", self.mirror);
        let cols = (SIDE_W / SWATCH) as usize;
        for (i, &c) in self.palette.iter().enumerate().take(cols * 12) {
            let (x, y) = (SIDE_X + (i % cols) as f32 * SWATCH, 316.0 + (i / cols) as f32 * SWATCH);
            push(x, y, SWATCH - 2.0, SWATCH - 2.0, Action::Color(c), "", self.color == c);
        }
        for (i, l) in ["Prop", "Figure", "Ground"].into_iter().enumerate() {
            push(SIDE_X + i as f32 * 78.0, 602.0, 74.0, 30.0, Action::Stand(i as u8), l, self.stand == i as u8);
        }
        if self.stand == 0 {
            push(SIDE_X + 130.0, 640.0, 46.0, 30.0, Action::Depth(-1), "-", false);
            push(SIDE_X + 182.0, 640.0, 46.0, 30.0, Action::Depth(1), "+", false);
        }
        push(SIDE_X + 130.0, 678.0, 46.0, 30.0, Action::Fps(-1), "-", false);
        push(SIDE_X + 182.0, 678.0, 46.0, 30.0, Action::Fps(1), "+", false);
        let shown = self.sprite.frames.len().min(7);
        let first = self.frame.saturating_sub(6).min(self.sprite.frames.len() - shown);
        for i in first..first + shown {
            push(CANVAS.x + (i - first) as f32 * (THUMB + 6.0), 622.0, THUMB, THUMB, Action::Frame(i), "", i == self.frame);
        }
        push(CANVAS.x + shown as f32 * (THUMB + 6.0), 622.0, 44.0, THUMB, Action::AddFrame, "+", false);
        let row = 700.0;
        push(CANVAS.x, row, 40.0, 30.0, Action::MoveFrame(-1), "<", false);
        push(CANVAS.x + 46.0, row, 40.0, 30.0, Action::MoveFrame(1), ">", false);
        push(CANVAS.x + 92.0, row, 70.0, 30.0, Action::DeleteFrame, "Delete", false);
        push(CANVAS.x + 168.0, row, 70.0, 30.0, Action::Play, if self.playing { "Stop" } else { "Play" }, self.playing);
        push(CANVAS.x + 368.0, row, 34.0, 30.0, Action::Width(-1), "-", false);
        push(CANVAS.x + 404.0, row, 34.0, 30.0, Action::Width(1), "+", false);
        push(CANVAS.x + 490.0, row, 34.0, 30.0, Action::Height(-1), "-", false);
        push(CANVAS.x + 526.0, row, 34.0, 30.0, Action::Height(1), "+", false);
        b
    }

    pub fn mouse_down(&mut self, p: [f32; 2], right: bool, shift: bool) {
        if self.ask.is_some() {
            return;
        }
        if LIST.contains(p) {
            let i = ((p[1] - LIST.y + self.scroll) / LIST_ROW) as usize;
            if let Some((name, _)) = self.names.get(i).cloned() {
                if self.dirty {
                    self.say(format!("{} has unsaved changes: Save or Revert first", self.sprite.name));
                } else {
                    self.open(&name);
                }
            }
            return;
        }
        for (i, bar) in self.bars().iter().enumerate() {
            if bar.contains(p) {
                self.channel = Some(i);
                self.drag(p);
                return;
            }
        }
        if let Some(b) = self.buttons().into_iter().find(|b| b.rect.contains(p)) {
            self.act(b.action, shift);
            return;
        }
        if CANVAS.contains(p) {
            let (x, y) = self.pixel_at(p);
            if right || self.tool == Tool::Picker {
                if let Some(c) = self.current().get(x, y).filter(|c| c[3] > 0) {
                    self.color = c;
                }
                return;
            }
            self.remember();
            self.paint(x, y);
            self.stroke = Some((x, y));
        }
    }

    pub fn drag(&mut self, p: [f32; 2]) {
        self.hover = Some(p);
        if let Some(i) = self.channel {
            let bar = self.bars()[i];
            self.color[i] = (((p[0] - bar.x) / bar.w).clamp(0.0, 1.0) * 255.0).round() as u8;
            self.color[3] = 255;
            return;
        }
        let Some((lx, ly)) = self.stroke else { return };
        if matches!(self.tool, Tool::Fill | Tool::Picker) {
            return;
        }
        let (x, y) = self.pixel_at(p);
        // Every pixel on the line from the last one, so a quick drag leaves no gaps.
        let steps = (x - lx).abs().max((y - ly).abs()).max(1);
        for k in 1..=steps {
            let t = k as f32 / steps as f32;
            self.paint(lx + ((x - lx) as f32 * t).round() as i32, ly + ((y - ly) as f32 * t).round() as i32);
        }
        self.stroke = Some((x, y));
    }

    pub fn mouse_up(&mut self) {
        self.stroke = None;
        self.channel = None;
    }

    pub fn wheel(&mut self, p: [f32; 2], dy: f32) {
        if LIST.contains(p) {
            let max = (self.names.len() as f32 * LIST_ROW - LIST.h).max(0.0);
            self.scroll = (self.scroll - dy * LIST_ROW * 3.0).clamp(0.0, max);
        }
    }

    fn paint(&mut self, x: i32, y: i32) {
        let c = if self.tool == Tool::Eraser { CLEAR } else { self.color };
        let w = self.current().w as i32;
        let f = &mut self.sprite.frames[self.frame];
        match self.tool {
            Tool::Fill => {
                f.fill(x, y, c);
                if self.mirror {
                    f.fill(w - 1 - x, y, c);
                }
            }
            _ => {
                f.set(x, y, c);
                if self.mirror {
                    f.set(w - 1 - x, y, c);
                }
            }
        }
        self.changed = true;
    }

    fn bars(&self) -> [Rect; 3] {
        [0, 1, 2].map(|i| Rect { x: SIDE_X + 34.0, y: 226.0 + i as f32 * 22.0, w: SIDE_W - 34.0, h: 16.0 })
    }

    fn act(&mut self, a: Action, shift: bool) {
        let step = if shift { 8 } else { 1 };
        match a {
            Action::Ask(k) => self.ask = Some((k, String::new())),
            Action::Save => match self.sprite.save(&self.dir) {
                Ok(()) => {
                    self.dirty = false;
                    self.names = sprite::catalog(&self.dir);
                    let n = self.sprite.frames.len();
                    self.say(format!("Saved {} ({n} frame{}). Rebuild the game to see it in play.", self.sprite.name, if n == 1 { "" } else { "s" }));
                }
                Err(e) => self.say(format!("Save failed: {e}")),
            },
            Action::Revert => {
                let name = self.sprite.name.clone();
                if self.names.iter().any(|(n, _)| *n == name) {
                    self.open(&name);
                } else {
                    self.remember();
                    let (w, h) = (self.current().w, self.current().h);
                    self.sprite.frames = vec![Frame::blank(w, h)];
                    self.frame = 0;
                }
                self.say("Reverted to the last save");
            }
            Action::Tool(t) => self.tool = t,
            Action::Mirror => self.mirror = !self.mirror,
            Action::AddFrame => {
                self.remember();
                let copy = self.current().clone();
                self.sprite.frames.insert(self.frame + 1, copy);
                self.frame += 1;
            }
            Action::DeleteFrame if self.sprite.frames.len() > 1 => {
                self.remember();
                self.sprite.frames.remove(self.frame);
                self.frame = self.frame.min(self.sprite.frames.len() - 1);
            }
            Action::DeleteFrame => self.say("A sprite keeps at least one frame"),
            Action::MoveFrame(d) => {
                let to = self.frame as i32 + d;
                if to >= 0 && (to as usize) < self.sprite.frames.len() {
                    self.remember();
                    self.sprite.frames.swap(self.frame, to as usize);
                    self.frame = to as usize;
                }
            }
            Action::Frame(i) => {
                self.frame = i;
                self.playing = false;
                self.changed = true;
            }
            Action::Play => {
                self.playing = !self.playing;
                self.changed = true;
            }
            Action::Width(d) | Action::Height(d) => {
                let (w, h) = (self.current().w as i32, self.current().h as i32);
                let (w, h) = if matches!(a, Action::Width(_)) { (w + d * step, h) } else { (w, h + d * step) };
                let (w, h) = (w.clamp(1, MAX_SIZE as i32) as usize, h.clamp(1, MAX_SIZE as i32) as usize);
                self.remember();
                for f in &mut self.sprite.frames {
                    *f = f.resized(w, h);
                }
            }
            Action::Stand(s) => {
                self.stand = s;
                self.changed = true;
            }
            Action::Depth(d) => {
                self.depth = (self.depth + (d * step) as f32).clamp(1.0, 96.0);
                self.changed = true;
            }
            Action::Fps(d) => {
                self.fps = (self.fps + d as f32).clamp(1.0, 30.0);
                self.changed = true;
            }
            Action::Color(c) => {
                self.color = c;
                if self.tool == Tool::Eraser {
                    self.tool = Tool::Pencil;
                }
            }
        }
    }

    pub fn key(&mut self, key: Key) {
        if let Some((kind, name)) = &mut self.ask {
            match key {
                Key::Char(c) if c.is_ascii_alphanumeric() || c == '_' || c == ' ' => {
                    name.push(if c == ' ' { '_' } else { c.to_ascii_uppercase() });
                }
                Key::Backspace => {
                    name.pop();
                }
                Key::Escape => self.ask = None,
                Key::Enter => {
                    let (kind, name) = (*kind, name.clone());
                    if !sprite::valid_name(&name) {
                        self.say("Names start with a letter and use A-Z, 0-9 and _");
                    } else if self.names.iter().any(|(n, _)| *n == name) {
                        self.say(format!("{name} already exists"));
                    } else {
                        self.ask = None;
                        let frames = if kind == Ask::Copy { self.sprite.frames.clone() } else { vec![Frame::blank(16, 16)] };
                        self.sprite = Sprite { name: name.clone(), frames };
                        self.frame = 0;
                        self.undo.clear();
                        self.redo.clear();
                        self.dirty = true;
                        self.changed = true;
                        self.say(format!("{name} is new: Save writes it to disk"));
                    }
                }
                _ => {}
            }
            return;
        }
        match key {
            Key::Char('b' | 'p') => self.tool = Tool::Pencil,
            Key::Char('e') => self.tool = Tool::Eraser,
            Key::Char('g' | 'f') => self.tool = Tool::Fill,
            Key::Char('i') => self.tool = Tool::Picker,
            Key::Char('m') => self.mirror = !self.mirror,
            Key::Char(' ') => self.act(Action::Play, false),
            Key::Char('n') => self.act(Action::Ask(Ask::New), false),
            Key::Left => self.act(Action::Frame((self.frame + self.sprite.frames.len() - 1) % self.sprite.frames.len()), false),
            Key::Right => self.act(Action::Frame((self.frame + 1) % self.sprite.frames.len()), false),
            Key::Save => self.act(Action::Save, false),
            Key::Undo => {
                if let Some(prev) = self.undo.pop() {
                    self.redo.push(std::mem::replace(&mut self.sprite.frames, prev));
                    self.frame = self.frame.min(self.sprite.frames.len() - 1);
                    self.changed = true;
                    self.dirty = true;
                }
            }
            Key::Redo => {
                if let Some(next) = self.redo.pop() {
                    self.undo.push(std::mem::replace(&mut self.sprite.frames, next));
                    self.frame = self.frame.min(self.sprite.frames.len() - 1);
                    self.changed = true;
                    self.dirty = true;
                }
            }
            _ => {}
        }
    }

    pub fn draw(&self, c: &mut Canvas) {
        c.clear = [0.1, 0.1, 0.12, 1.0];
        c.label(format!("{}{}", self.sprite.name, if self.dirty { "  (unsaved)" } else { "" }), 12.0, 12.0, 26.0, TEXT, Align::Left);

        c.rect(LIST.x, LIST.y, LIST.w, LIST.h, PANEL);
        let first = (self.scroll / LIST_ROW) as usize;
        let rows = (LIST.h / LIST_ROW) as usize;
        for (i, (name, saved)) in self.names.iter().enumerate().skip(first).take(rows) {
            let y = LIST.y + i as f32 * LIST_ROW - self.scroll;
            if *name == self.sprite.name {
                c.rect(LIST.x, y, LIST.w, LIST_ROW, [0.35, 0.3, 0.18, 1.0]);
            }
            let mark = if *saved { "  *" } else { "" };
            c.label(format!("{name}{mark}"), LIST.x + 8.0, y + 2.0, 17.0, if *saved { HOT } else { TEXT }, Align::Left);
        }

        self.draw_canvas(c);

        for b in self.buttons() {
            let r = b.rect;
            if let Action::Color(col) = b.action {
                if b.on {
                    c.rect(r.x - 2.0, r.y - 2.0, r.w + 4.0, r.h + 4.0, TEXT);
                }
                c.rect(r.x, r.y, r.w, r.h, rgba(col));
                continue;
            }
            if let Action::Frame(i) = b.action {
                c.rect(r.x, r.y, r.w, r.h, if b.on { HOT } else { PANEL });
                c.rect(r.x + 2.0, r.y + 2.0, r.w - 4.0, r.h - 4.0, [0.22, 0.22, 0.25, 1.0]);
                if let Some(&a) = self.live.get(i) {
                    let s = ((r.w - 8.0) / a.w().max(a.h())).min(4.0);
                    c.art(a, r.x + (r.w - a.w() * s) / 2.0, r.y + (r.h - a.h() * s) / 2.0, s, false, [1.0; 4]);
                }
                continue;
            }
            c.rect(r.x, r.y, r.w, r.h, if b.on { HOT } else { BUTTON });
            c.label(b.label, r.x + r.w / 2.0, r.y + (r.h - 18.0) / 2.0, 18.0, if b.on { [0.1, 0.1, 0.1, 1.0] } else { TEXT }, Align::Center);
        }

        // Color: a swatch, then red, green and blue bars.
        c.label("Color", SIDE_X, 172.0, 18.0, DIM, Align::Left);
        c.rect(SIDE_X + 60.0, 168.0, 60.0, 48.0, rgba(self.color));
        c.label(format!("#{:02X}{:02X}{:02X}", self.color[0], self.color[1], self.color[2]), SIDE_X + 130.0, 184.0, 18.0, TEXT, Align::Left);
        for (i, bar) in self.bars().iter().enumerate() {
            c.label(["R", "G", "B"][i], SIDE_X + 4.0, bar.y - 2.0, 18.0, DIM, Align::Left);
            for k in 0..16 {
                let mut col = self.color;
                col[i] = (k * 17) as u8;
                c.rect(bar.x + k as f32 * bar.w / 16.0, bar.y, bar.w / 16.0 + 0.5, bar.h, rgba(col));
            }
            c.rect(bar.x + self.color[i] as f32 / 255.0 * bar.w - 2.0, bar.y - 3.0, 4.0, bar.h + 6.0, TEXT);
        }
        c.label("Tileset palette", SIDE_X, 294.0, 16.0, DIM, Align::Left);
        c.label("Stands as", SIDE_X, 582.0, 16.0, DIM, Align::Left);
        if self.stand == 0 {
            c.label(format!("Depth {} px", self.depth), SIDE_X, 645.0, 18.0, TEXT, Align::Left);
        }
        c.label(format!("{} fps", self.fps), SIDE_X, 683.0, 18.0, TEXT, Align::Left);

        let f = self.current();
        c.label(format!("{} / {}", self.frame + 1, self.sprite.frames.len()), CANVAS.x + 248.0, 704.0, 18.0, DIM, Align::Left);
        c.label(format!("W {}", f.w), CANVAS.x + 362.0, 704.0, 18.0, TEXT, Align::Right);
        c.label(format!("H {}", f.h), CANVAS.x + 484.0, 704.0, 18.0, TEXT, Align::Right);
        if let Some(p) = self.hover.filter(|p| CANVAS.contains(*p)) {
            let (x, y) = self.pixel_at(p);
            if f.get(x, y).is_some() {
                c.label(format!("{x}, {y}"), CANVAS.x + CANVAS.w, 30.0, 16.0, DIM, Align::Right);
            }
        }

        let help = "Left: draw   Right: pick color   Shift with - and +: steps of 8   Cmd+Z / Cmd+Shift+Z: undo / redo   Cmd+S: save";
        c.label(help, 12.0, 748.0, 13.0, DIM, Align::Left);
        if let Some((text, age)) = &self.status {
            let a = (4.0 - age).clamp(0.0, 1.0);
            c.rect(CANVAS.x, CANVAS.y + CANVAS.h - 40.0, CANVAS.w, 40.0, [0.0, 0.0, 0.0, 0.7 * a]);
            c.label(text.clone(), CANVAS.x + CANVAS.w / 2.0, CANVAS.y + CANVAS.h - 30.0, 18.0, [1.0, 1.0, 1.0, a], Align::Center);
        }
        if let Some((kind, name)) = &self.ask {
            c.rect(0.0, 0.0, 1024.0, 768.0, [0.0, 0.0, 0.0, 0.6]);
            c.rect(262.0, 280.0, 500.0, 170.0, PANEL);
            let what = if *kind == Ask::New { "Name the new sprite" } else { "Name the copy" };
            c.label(what, 512.0, 300.0, 24.0, TEXT, Align::Center);
            let caret = if (self.clock * 2.0) as i32 % 2 == 0 { "_" } else { " " };
            c.label(format!("{name}{caret}"), 512.0, 350.0, 30.0, HOT, Align::Center);
            c.label("Enter to create, Esc to cancel", 512.0, 405.0, 16.0, DIM, Align::Center);
        }
    }

    fn draw_canvas(&self, c: &mut Canvas) {
        c.rect(CANVAS.x, CANVAS.y, CANVAS.w, CANVAS.h, PANEL);
        let f = self.current();
        let (o, zoom) = self.grid();
        let (w, h) = (f.w as f32 * zoom, f.h as f32 * zoom);
        // A checkerboard under clear pixels.
        let cell = 16.0;
        for gy in 0..(h / cell).ceil() as i32 {
            for gx in 0..(w / cell).ceil() as i32 {
                let (x, y) = (gx as f32 * cell, gy as f32 * cell);
                let shade = if (gx + gy) % 2 == 0 { [0.3, 0.3, 0.33, 1.0] } else { [0.36, 0.36, 0.4, 1.0] };
                c.rect(o[0] + x, o[1] + y, cell.min(w - x), cell.min(h - y), shade);
            }
        }
        for y in 0..f.h {
            for x in 0..f.w {
                let p = f.px[y * f.w + x];
                if p[3] > 0 {
                    c.rect(o[0] + x as f32 * zoom, o[1] + y as f32 * zoom, zoom, zoom, rgba(p));
                }
            }
        }
        if zoom >= 8.0 {
            let line = [1.0, 1.0, 1.0, 0.07];
            for x in 0..=f.w {
                c.rect(o[0] + x as f32 * zoom, o[1], 1.0, h, line);
            }
            for y in 0..=f.h {
                c.rect(o[0], o[1] + y as f32 * zoom, w, 1.0, line);
            }
        }
        if self.mirror {
            c.rect(o[0] + w / 2.0 - 1.0, o[1], 2.0, h, [0.4, 0.8, 1.0, 0.5]);
        }
        if let Some(p) = self.hover.filter(|p| CANVAS.contains(*p)) {
            let (x, y) = self.pixel_at(p);
            if f.get(x, y).is_some() {
                let (px, py) = (o[0] + x as f32 * zoom, o[1] + y as f32 * zoom);
                let edge = [1.0, 1.0, 1.0, 0.8];
                c.rect(px, py, zoom, 1.0, edge);
                c.rect(px, py + zoom - 1.0, zoom, 1.0, edge);
                c.rect(px, py, 1.0, zoom, edge);
                c.rect(px + zoom - 1.0, py, 1.0, zoom, edge);
            }
        }
    }
}

fn rgba(c: Rgba) -> Color {
    [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, if c[3] == 0 { 0.0 } else { 1.0 }]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> Editor {
        let dir = std::env::temp_dir().join(format!("lh-editor-{}-{:?}", std::process::id(), std::thread::current().id()));
        Editor::new(dir, "BARREL")
    }

    fn click_pixel(e: &mut Editor, x: i32, y: i32) {
        let (o, zoom) = e.grid();
        let p = [o[0] + (x as f32 + 0.5) * zoom, o[1] + (y as f32 + 0.5) * zoom];
        e.mouse_down(p, false, false);
        e.mouse_up();
    }

    #[test]
    fn painting_undoes_and_redoes() {
        let mut e = editor();
        e.color = [1, 2, 3, 255];
        let before = e.current().clone();
        click_pixel(&mut e, 0, 0);
        assert_eq!(e.current().get(0, 0), Some([1, 2, 3, 255]));
        assert!(e.dirty);
        e.key(Key::Undo);
        assert_eq!(*e.current(), before);
        e.key(Key::Redo);
        assert_eq!(e.current().get(0, 0), Some([1, 2, 3, 255]));
    }

    #[test]
    fn mirror_paints_both_sides() {
        let mut e = editor();
        e.mirror = true;
        e.color = [9, 9, 9, 255];
        click_pixel(&mut e, 1, 3);
        assert_eq!(e.current().get(14, 3), Some([9, 9, 9, 255]));
    }

    #[test]
    fn a_new_sprite_needs_a_good_unused_name() {
        let mut e = editor();
        e.act(Action::Ask(Ask::New), false);
        for ch in "barrel".chars() {
            e.key(Key::Char(ch));
        }
        e.key(Key::Enter);
        assert!(e.ask.is_some(), "BARREL is taken");
        for _ in 0..6 {
            e.key(Key::Backspace);
        }
        for ch in "big rock".chars() {
            e.key(Key::Char(ch));
        }
        e.key(Key::Enter);
        assert!(e.ask.is_none());
        assert_eq!(e.sprite.name, "BIG_ROCK");
        assert_eq!((e.current().w, e.current().h), (16, 16));
    }

    #[test]
    fn frames_add_move_and_delete() {
        let mut e = editor();
        e.act(Action::AddFrame, false);
        assert_eq!((e.sprite.frames.len(), e.frame), (2, 1));
        e.sprite.frames[1].set(0, 0, [7, 7, 7, 255]);
        e.act(Action::MoveFrame(-1), false);
        assert_eq!(e.sprite.frames[0].get(0, 0), Some([7, 7, 7, 255]));
        e.act(Action::DeleteFrame, false);
        e.act(Action::DeleteFrame, false);
        assert_eq!(e.sprite.frames.len(), 1);
    }

    #[test]
    fn every_button_fits_the_pane_and_none_overlap() {
        let e = editor();
        let b = e.buttons();
        for x in &b {
            assert!(x.rect.x >= 0.0 && x.rect.y >= 0.0 && x.rect.x + x.rect.w <= 1024.0 && x.rect.y + x.rect.h <= 768.0, "{:?}", x.action);
            for y in &b {
                let apart = x.rect.x + x.rect.w <= y.rect.x || y.rect.x + y.rect.w <= x.rect.x || x.rect.y + x.rect.h <= y.rect.y || y.rect.y + y.rect.h <= x.rect.y;
                assert!(std::ptr::eq(x, y) || apart, "{:?} overlaps {:?}", x.action, y.action);
            }
        }
    }
}
