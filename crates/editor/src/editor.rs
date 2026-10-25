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
const STANDS: [&str; 5] = ["Prop", "Slab", "Figure", "Ground", "Carve"];
const CARVE: u8 = 4;
const VIEW_NAMES: [&str; 3] = ["South view", "East view", "Footprint"];
const VIEW_HINTS: [&str; 3] = ["the sprite seen from the south: west on the left", "the sprite seen from the east: south on the left", "the outline from above: only opaque or clear counts"];
const FOOTPRINT: usize = 2;
const DIALOG: Rect = Rect { x: 212.0, y: 250.0, w: 600.0, h: 230.0 };
/// The unsaved-changes dialog's choices: save then switch, discard then switch, or stay.
const CHOICES: [(&str, Choice); 3] = [("Save and switch  S", Choice::Save), ("Discard  D", Choice::Discard), ("Stay  Esc", Choice::Stay)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    Save,
    Discard,
    Stay,
}

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
    Select,
}

/// A rectangle of pixels picked out with the select tool. Once moved or pasted, its pixels float above the
/// surface until placed, and only their opaque pixels land.
#[derive(Clone, Debug, PartialEq)]
struct Selection {
    x: i32,
    y: i32,
    w: usize,
    h: usize,
    float: Option<Frame>,
}

impl Selection {
    fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w as i32 && y < self.y + self.h as i32
    }
}

/// What the select tool is doing while the mouse is down.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Grab {
    /// Drawing a new rectangle from this corner.
    From(i32, i32),
    /// Moving the selection, held this far from its top-left.
    Move(i32, i32),
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
    View(usize),
    AddFootprint,
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
    Up,
    Down,
    Copy,
    Cut,
    Paste,
}

/// Everything an undo puts back: the frames, the carving views and the footprint.
type Snapshot = (Vec<Frame>, Option<[Frame; 2]>, Option<Frame>);

pub struct Editor {
    dir: PathBuf,
    catalog: PathBuf,
    pub sprite: Sprite,
    names: Vec<(String, bool)>,
    scroll: f32,
    frame: usize,
    tool: Tool,
    mirror: bool,
    color: Rgba,
    palette: Vec<Rgba>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// Edits not yet saved.
    dirty: bool,
    /// The previews need repacking.
    pub changed: bool,
    playing: bool,
    clock: f32,
    /// An index into `STANDS`.
    stand: u8,
    /// Which carving view is on the canvas instead of a frame.
    view: Option<usize>,
    depth: f32,
    height: f32,
    /// Whether the stand has been changed since it was loaded or saved.
    stand_touched: bool,
    pub fps: f32,
    ask: Option<(Ask, String)>,
    /// A sprite to switch to once the current one's unsaved changes are saved or dropped.
    pending: Option<String>,
    selection: Option<Selection>,
    grab: Option<Grab>,
    /// Rows of the south and east views cut off by lowering the carving's height, highest first.
    trimmed: [Vec<Vec<Rgba>>; 2],
    /// Pixels an arrow key moves a selection: 8 with Shift held.
    pub nudge: i32,
    /// Copied pixels and where they were copied from.
    clipboard: Option<(Frame, i32, i32)>,
    status: Option<(String, f32)>,
    /// The last pixel a stroke painted, so a fast drag joins up.
    stroke: Option<(i32, i32)>,
    channel: Option<usize>,
    hover: Option<[f32; 2]>,
    /// The frames, then the carving views, as packed into the scratch area, when they fit.
    live: Vec<Art>,
}

impl Editor {
    pub fn new(dir: PathBuf, name: &str) -> Self {
        let sprite = Sprite::load(&dir, name).unwrap_or(Sprite { name: name.into(), frames: vec![Frame::blank(16, 16)], views: None, footprint: None });
        let names = sprite::catalog(&dir);
        let mut e = Self {
            dir,
            catalog: sprite::catalog_file(),
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
            view: None,
            depth: 12.0,
            height: 10.0,
            stand_touched: false,
            fps: 6.0,
            ask: None,
            pending: None,
            selection: None,
            grab: None,
            trimmed: Default::default(),
            nudge: 1,
            clipboard: None,
            status: None,
            stroke: None,
            channel: None,
            hover: None,
            live: Vec::new(),
        };
        e.load_stand();
        e
    }

    /// How the sprite should stand in the previews.
    pub fn stand(&self) -> Stand {
        let n = self.sprite.frames.len();
        match self.stand {
            0 => Stand::Prop { depth: self.depth },
            2 => Stand::Figure,
            3 => Stand::Ground,
            CARVE if self.live.len() == n + 2 + self.sprite.footprint.is_some() as usize => {
                Stand::Carve { footprint: self.live.get(n + 2).copied(), south: self.live[n], east: self.live[n + 1] }
            }
            _ => Stand::Slab { height: self.height },
        }
    }

    /// The stand as the catalog and `stand.txt` write it.
    fn stand_text(&self) -> String {
        if self.stand == CARVE { "carve".into() } else { sprite::stand_text(self.stand()) }
    }

    /// Everything to pack for the previews: the frames, then the carving views when carving, with any
    /// floating pixels shown where they would land.
    pub fn surfaces(&self) -> Vec<Frame> {
        let carving = self.stand == CARVE && self.sprite.views.is_some();
        let views = self.sprite.views.iter().flatten().chain(&self.sprite.footprint).filter(|_| carving);
        let mut out: Vec<Frame> = self.sprite.frames.iter().chain(views).cloned().collect();
        if let Some(Selection { x, y, float: Some(f), .. }) = &self.selection {
            let i = self.view.map_or(self.frame, |v| self.sprite.frames.len() + v);
            if let Some(target) = out.get_mut(i) {
                stamp(target, f, *x, *y);
            }
        }
        out
    }

    /// The frames to show in the previews: all of them while playing, else the one being edited.
    pub fn shown(&self) -> Vec<Art> {
        if self.playing { self.live.clone() } else { self.live.get(self.frame).copied().into_iter().collect() }
    }

    pub fn set_live(&mut self, live: Vec<Art>) {
        self.live = live;
    }

    pub fn title(&self) -> String {
        format!("{}{} - LastHarvest sprite editor", self.sprite.name, if self.unsaved() { " *" } else { "" })
    }

    fn unsaved(&self) -> bool {
        self.dirty || self.stand_touched
    }

    /// What the canvas shows: a frame, or a carving view.
    fn current(&self) -> &Frame {
        match (self.view, &self.sprite.views, &self.sprite.footprint) {
            (Some(FOOTPRINT), _, Some(f)) => f,
            (Some(v), Some(views), _) if v < 2 => &views[v],
            _ => &self.sprite.frames[self.frame],
        }
    }

    fn current_mut(&mut self) -> &mut Frame {
        match (self.view, &mut self.sprite.views, &mut self.sprite.footprint) {
            (Some(FOOTPRINT), _, Some(f)) => f,
            (Some(v), Some(views), _) if v < 2 => &mut views[v],
            _ => &mut self.sprite.frames[self.frame],
        }
    }

    fn snapshot(&self) -> Snapshot {
        (self.sprite.frames.clone(), self.sprite.views.clone(), self.sprite.footprint.clone())
    }

    /// The carving height, which is how tall the views are.
    fn view_height(&self) -> usize {
        self.sprite.views.as_ref().map_or(self.height as usize, |v| v[0].h)
    }

    /// The stand as saved, or a guess from the name and size when none was.
    fn load_stand(&mut self) {
        let saved = sprite::read_stand(&self.dir, &self.catalog, &self.sprite.name);
        self.stand_touched = false;
        self.view = None;
        self.depth = (self.current().w as f32 * 0.6).clamp(2.0, 48.0).round();
        self.height = 10.0;
        let guess = || {
            let n = &self.sprite.name;
            if ["PLAYER", "SKINNY", "KID", "BIG", "CROW"].iter().any(|k| n.starts_with(k)) {
                Stand::Figure
            } else if ["DIRT", "GRASS", "PATH", "ROAD", "SOIL", "SPROUTS", "BLOOD", "CORPSE"].iter().any(|k| n.starts_with(k)) {
                Stand::Ground
            } else {
                Stand::Prop { depth: self.depth }
            }
        };
        if saved.as_deref() == Some("carve") && self.sprite.views.is_some() {
            self.stand = CARVE;
            return;
        }
        let s = saved.as_deref().and_then(sprite::parse_stand).unwrap_or_else(guess);
        self.stand = match s {
            Stand::Prop { depth } => {
                self.depth = depth;
                0
            }
            Stand::Slab { height } => {
                self.height = height;
                1
            }
            Stand::Figure => 2,
            Stand::Ground => 3,
            Stand::Carve { .. } => CARVE,
        };
    }

    fn say(&mut self, text: impl Into<String>) {
        self.status = Some((text.into(), 0.0));
    }

    fn remember(&mut self) {
        self.undo.push(self.snapshot());
        if self.undo.len() > UNDO_DEPTH {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
        self.changed = true;
    }

    fn open(&mut self, name: &str) {
        self.selection = None;
        self.trimmed = Default::default();
        if let Some(s) = Sprite::load(&self.dir, name) {
            self.sprite = s;
            self.frame = 0;
            self.undo.clear();
            self.redo.clear();
            self.dirty = false;
            self.changed = true;
            self.load_stand();
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
            push(616.0 + i as f32 * 102.0, 8.0, 94.0, 34.0, a, l, a == Action::Save && self.unsaved());
        }
        let tools = [(Tool::Pencil, "Pencil B"), (Tool::Eraser, "Eraser E"), (Tool::Fill, "Fill G"), (Tool::Picker, "Pick I"), (Tool::Select, "Select S")];
        for (i, (t, l)) in tools.into_iter().enumerate() {
            push(SIDE_X + (i % 2) as f32 * 118.0, 52.0 + (i / 2) as f32 * 38.0, 114.0, 32.0, Action::Tool(t), l, self.tool == t);
        }
        push(SIDE_X + 118.0, 128.0, 114.0, 32.0, Action::Mirror, "Mirror M", self.mirror);
        let cols = (SIDE_W / SWATCH) as usize;
        for (i, &c) in self.palette.iter().enumerate().take(cols * 9) {
            let (x, y) = (SIDE_X + (i % cols) as f32 * SWATCH, 316.0 + (i / cols) as f32 * SWATCH);
            push(x, y, SWATCH - 2.0, SWATCH - 2.0, Action::Color(c), "", self.color == c);
        }
        for (i, l) in STANDS.into_iter().enumerate() {
            push(SIDE_X + (i % 3) as f32 * 78.0, 566.0 + (i / 3) as f32 * 34.0, 74.0, 30.0, Action::Stand(i as u8), l, self.stand == i as u8);
        }
        if self.stand < 2 || self.stand == CARVE {
            push(SIDE_X + 130.0, 640.0, 46.0, 30.0, Action::Depth(-1), "-", false);
            push(SIDE_X + 182.0, 640.0, 46.0, 30.0, Action::Depth(1), "+", false);
        }
        push(SIDE_X + 130.0, 678.0, 46.0, 30.0, Action::Fps(-1), "-", false);
        push(SIDE_X + 182.0, 678.0, 46.0, 30.0, Action::Fps(1), "+", false);
        let carving = self.stand == CARVE && self.sprite.views.is_some();
        let room = if carving { 4 } else { 7 };
        let shown = self.sprite.frames.len().min(room);
        let first = self.frame.saturating_sub(room - 1).min(self.sprite.frames.len() - shown);
        for i in first..first + shown {
            push(CANVAS.x + (i - first) as f32 * (THUMB + 6.0), 622.0, THUMB, THUMB, Action::Frame(i), "", i == self.frame && self.view.is_none());
        }
        let after = CANVAS.x + shown as f32 * (THUMB + 6.0);
        push(after, 622.0, 44.0, THUMB, Action::AddFrame, "+", false);
        if carving {
            for v in 0..2 {
                push(after + 56.0 + v as f32 * (THUMB + 6.0), 622.0, THUMB, THUMB, Action::View(v), "", self.view == Some(v));
            }
            let x = after + 56.0 + 2.0 * (THUMB + 6.0);
            if self.sprite.footprint.is_some() {
                push(x, 622.0, THUMB, THUMB, Action::View(FOOTPRINT), "", self.view == Some(FOOTPRINT));
            } else {
                push(x, 622.0, THUMB, THUMB, Action::AddFootprint, "", false);
            }
        }
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
        if self.pending.is_some() {
            if let Some(&(_, choice)) = CHOICES.iter().enumerate().find(|(i, _)| choice_rect(*i).contains(p)).map(|(_, c)| c) {
                self.decide(choice);
            }
            return;
        }
        if self.ask.is_some() {
            return;
        }
        if LIST.contains(p) {
            let i = ((p[1] - LIST.y + self.scroll) / LIST_ROW) as usize;
            if let Some((name, _)) = self.names.get(i).cloned() {
                self.switch_to(name);
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
            if self.tool == Tool::Select && !right {
                self.grab = Some(match &self.selection {
                    Some(sel) if sel.contains(x, y) => {
                        self.lift();
                        let sel = self.selection.as_ref().unwrap();
                        Grab::Move(x - sel.x, y - sel.y)
                    }
                    _ => {
                        self.place();
                        self.selection = None;
                        Grab::From(x, y)
                    }
                });
                return;
            }
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
        if let Some(grab) = self.grab {
            let (x, y) = self.pixel_at(p);
            match grab {
                Grab::From(ax, ay) => {
                    let f = self.current();
                    let clamp = |v: i32, max: usize| v.clamp(0, max as i32 - 1);
                    let (x0, x1) = (clamp(ax.min(x), f.w), clamp(ax.max(x), f.w));
                    let (y0, y1) = (clamp(ay.min(y), f.h), clamp(ay.max(y), f.h));
                    self.selection = Some(Selection { x: x0, y: y0, w: (x1 - x0 + 1) as usize, h: (y1 - y0 + 1) as usize, float: None });
                }
                Grab::Move(gx, gy) => self.shift_selection(x - gx, y - gy),
            }
            return;
        }
        let Some((lx, ly)) = self.stroke else { return };
        if matches!(self.tool, Tool::Fill | Tool::Picker | Tool::Select) {
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
        if matches!(self.grab.take(), Some(Grab::From(..))) && self.selection.as_ref().is_some_and(|s| s.w * s.h == 1) {
            self.selection = None;
        }
    }

    /// Lifts the selected pixels off the surface so they can move, leaving clear pixels behind.
    fn lift(&mut self) {
        let Some(sel) = self.selection.clone().filter(|s| s.float.is_none()) else { return };
        self.remember();
        let mut float = Frame::blank(sel.w, sel.h);
        let f = self.current_mut();
        for y in 0..sel.h as i32 {
            for x in 0..sel.w as i32 {
                float.set(x, y, f.get(sel.x + x, sel.y + y).unwrap_or(CLEAR));
                f.set(sel.x + x, sel.y + y, CLEAR);
            }
        }
        self.selection = Some(Selection { float: Some(float), ..sel });
    }

    /// Puts floating pixels down where they are; the selection stays as a rectangle.
    fn place(&mut self) {
        let Some(Selection { x, y, float: Some(f), .. }) = self.selection.clone() else { return };
        stamp(self.current_mut(), &f, x, y);
        if let Some(sel) = &mut self.selection {
            sel.float = None;
        }
        self.changed = true;
    }

    /// Ends the selection, putting any floating pixels down first.
    fn deselect(&mut self) {
        self.place();
        self.selection = None;
        self.grab = None;
    }

    fn shift_selection(&mut self, x: i32, y: i32) {
        self.lift();
        if let Some(sel) = &mut self.selection {
            (sel.x, sel.y) = (x, y);
        }
        self.changed = true;
    }

    fn copy(&mut self) {
        let Some(sel) = &self.selection else {
            self.say("Select some pixels first: S, then drag");
            return;
        };
        let pixels = match &sel.float {
            Some(f) => f.clone(),
            None => {
                let mut out = Frame::blank(sel.w, sel.h);
                for y in 0..sel.h as i32 {
                    for x in 0..sel.w as i32 {
                        out.set(x, y, self.current().get(sel.x + x, sel.y + y).unwrap_or(CLEAR));
                    }
                }
                out
            }
        };
        let (w, h) = (pixels.w, pixels.h);
        self.clipboard = Some((pixels, sel.x, sel.y));
        self.say(format!("Copied {w} x {h}: pick a surface and paste with Cmd+V"));
    }

    /// Clears the selected pixels, or drops floating ones.
    fn clear_selection(&mut self) {
        let Some(sel) = self.selection.clone() else { return };
        if sel.float.is_none() {
            self.remember();
            let f = self.current_mut();
            for y in 0..sel.h as i32 {
                for x in 0..sel.w as i32 {
                    f.set(sel.x + x, sel.y + y, CLEAR);
                }
            }
        }
        self.selection = None;
        self.changed = true;
    }

    fn paste(&mut self) {
        let Some((pixels, x, y)) = self.clipboard.clone() else {
            self.say("Nothing copied yet");
            return;
        };
        self.deselect();
        self.remember();
        let f = self.current();
        // Where it was copied from, unless that would put it out of sight.
        let fits = |v: i32, size: usize, max: usize| if v >= 0 && v + size as i32 <= max as i32 { v } else { 0 };
        let (x, y) = (fits(x, pixels.w, f.w), fits(y, pixels.h, f.h));
        self.selection = Some(Selection { x, y, w: pixels.w, h: pixels.h, float: Some(pixels) });
        self.tool = Tool::Select;
        self.say("Pasted: drag or use the arrow keys to line it up, Enter to place");
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
        let (mirror, tool) = (self.mirror, self.tool);
        let f = self.current_mut();
        match tool {
            Tool::Fill => {
                f.fill(x, y, c);
                if mirror {
                    f.fill(w - 1 - x, y, c);
                }
            }
            _ => {
                f.set(x, y, c);
                if mirror {
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
            Action::Save => {
                self.deselect();
                let new = !self.names.iter().any(|(n, _)| *n == self.sprite.name);
                // A carving lives in the sprite's folder, so it saves the pixels too.
                let carving = self.stand == CARVE && self.stand_touched;
                let pixels = if self.dirty || new || carving { self.sprite.save(&self.dir) } else { Ok(()) };
                let stand = if self.stand_touched { sprite::save_stand(&self.dir, &self.catalog, &self.sprite.name, &self.stand_text()) } else { Ok(()) };
                match pixels.and(stand) {
                    Ok(()) => {
                        self.dirty = false;
                        self.stand_touched = false;
                        self.names = sprite::catalog(&self.dir);
                        self.say(format!("Saved {}. Rebuild the game to see it in play.", self.sprite.name));
                    }
                    Err(e) => self.say(format!("Save failed: {e}")),
                }
            }
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
            Action::Tool(t) => {
                if t != Tool::Select {
                    self.deselect();
                }
                self.tool = t;
            }
            Action::Mirror => self.mirror = !self.mirror,
            Action::AddFrame => {
                self.remember();
                let copy = self.current().clone();
                self.sprite.frames.insert(self.frame + 1, copy);
                self.frame += 1;
            }
            Action::DeleteFrame if self.view == Some(FOOTPRINT) => {
                self.remember();
                self.sprite.footprint = None;
                self.view = None;
                self.say("Footprint removed: the sprite's own outline shapes the solid again");
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
                self.deselect();
                self.frame = i;
                self.view = None;
                self.playing = false;
                self.changed = true;
            }
            Action::AddFootprint => {
                self.deselect();
                self.remember();
                self.sprite.footprint = Some(self.sprite.frames[self.frame].clone());
                self.view = Some(FOOTPRINT);
                self.say("Erase what isn't solid from above");
            }
            Action::View(v) => {
                self.deselect();
                self.view = Some(v);
                self.playing = false;
            }
            Action::Play => {
                self.playing = !self.playing;
                self.changed = true;
            }
            Action::Width(_) if self.view.is_some() => self.say("A view is as wide as the sprite: resize the frames instead"),
            Action::Height(_) if self.view == Some(FOOTPRINT) => self.say("The footprint is the sprite's size: resize the frames instead"),
            Action::Height(d) if self.view.is_some() => self.resize_views(self.view_height() as i32 + d * step),
            Action::Width(d) | Action::Height(d) => {
                let top = &self.sprite.frames[self.frame];
                let (w, h) = (top.w as i32, top.h as i32);
                let (w, h) = if matches!(a, Action::Width(_)) { (w + d * step, h) } else { (w, h + d * step) };
                let (w, h) = (w.clamp(1, MAX_SIZE as i32) as usize, h.clamp(1, MAX_SIZE as i32) as usize);
                self.remember();
                for f in &mut self.sprite.frames {
                    *f = f.resized(w, h);
                }
                // The south view spans the sprite's width and the east view its height.
                if let Some([south, east]) = &mut self.sprite.views {
                    *south = south.resized(w, south.h);
                    *east = east.resized(h, east.h);
                    self.trimmed = Default::default();
                }
                if let Some(f) = &mut self.sprite.footprint {
                    *f = f.resized(w, h);
                }
            }
            Action::Stand(s) => {
                self.stand = s;
                self.stand_touched = true;
                self.changed = true;
                if s == CARVE && self.sprite.views.is_none() {
                    self.remember();
                    self.sprite.views = Some(sprite::block_views(&self.sprite.frames[self.frame], self.height as usize));
                    self.say("Carve: erase the south and east views to shape the solid");
                }
                if s != CARVE {
                    self.view = None;
                }
            }
            Action::Depth(d) if self.stand == CARVE => self.resize_views(self.view_height() as i32 + d * step),
            Action::Depth(d) => {
                let size = if self.stand == 0 { &mut self.depth } else { &mut self.height };
                *size = (*size + (d * step) as f32).clamp(1.0, 96.0);
                self.stand_touched = true;
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

    /// Sets how tall the carving is, keeping the views' bottoms on the ground.
    /// Rows lowering the height cut off are kept, and come back as it rises; past them, the top row repeats.
    fn resize_views(&mut self, height: i32) {
        let h = height.clamp(1, MAX_SIZE as i32) as usize;
        let Some(views) = &self.sprite.views else { return };
        if views[0].h == h {
            return;
        }
        self.remember();
        let Some(views) = &mut self.sprite.views else { return };
        for (v, kept) in views.iter_mut().zip(&mut self.trimmed) {
            let rows: Vec<Vec<Rgba>> = v.px.chunks(v.w).map(<[Rgba]>::to_vec).collect();
            let rows = if h < rows.len() {
                // The cut rows sit just below any cut before, which were higher up.
                kept.extend(rows[..rows.len() - h].iter().cloned());
                rows[rows.len() - h..].to_vec()
            } else {
                let mut grown = Vec::with_capacity(h);
                let back = (h - rows.len()).min(kept.len());
                let top = rows.first().cloned().unwrap_or_default();
                grown.extend(std::iter::repeat_n(top, h - rows.len() - back));
                grown.extend(kept.drain(kept.len() - back..));
                grown.extend(rows);
                grown
            };
            *v = Frame { w: v.w, h, px: rows.concat() };
        }
        self.height = h as f32;
        self.stand_touched = true;
    }

    /// Opens another sprite, first asking what to do with unsaved changes to this one.
    fn switch_to(&mut self, name: String) {
        if name == self.sprite.name {
            return;
        }
        if self.unsaved() {
            self.pending = Some(name);
        } else {
            self.open(&name);
            self.reveal();
        }
    }

    fn decide(&mut self, choice: Choice) {
        let Some(name) = self.pending.take() else { return };
        match choice {
            Choice::Save => {
                self.act(Action::Save, false);
                if self.unsaved() {
                    return;
                }
                self.open(&name);
            }
            Choice::Discard => {
                self.dirty = false;
                self.stand_touched = false;
                self.open(&name);
            }
            Choice::Stay => return,
        }
        self.reveal();
    }

    /// Steps `by` places through the sprite list.
    fn step_sprite(&mut self, by: i32) {
        let n = self.names.len() as i32;
        let at = self.names.iter().position(|(m, _)| *m == self.sprite.name).map_or(0, |i| i as i32);
        let next = (at + by).rem_euclid(n.max(1)) as usize;
        if let Some((name, _)) = self.names.get(next).cloned() {
            self.switch_to(name);
        }
    }

    /// Scrolls the list so the open sprite shows.
    fn reveal(&mut self) {
        let Some(i) = self.names.iter().position(|(m, _)| *m == self.sprite.name) else { return };
        let y = i as f32 * LIST_ROW;
        if y < self.scroll {
            self.scroll = y;
        } else if y + LIST_ROW > self.scroll + LIST.h {
            self.scroll = y + LIST_ROW - LIST.h;
        }
    }

    fn step_frame(&mut self, by: i32) {
        let n = self.sprite.frames.len() as i32;
        self.act(Action::Frame((self.frame as i32 + by).rem_euclid(n) as usize), false);
    }

    pub fn key(&mut self, key: Key) {
        if self.pending.is_some() {
            match key {
                Key::Char('s') | Key::Enter => self.decide(Choice::Save),
                Key::Char('d') => self.decide(Choice::Discard),
                Key::Escape | Key::Char('c') => self.decide(Choice::Stay),
                _ => {}
            }
            return;
        }
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
                        let (frames, views, footprint) = if kind == Ask::Copy { self.snapshot() } else { (vec![Frame::blank(16, 16)], None, None) };
                        self.sprite = Sprite { name: name.clone(), frames, views, footprint };
                        self.view = None;
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
        if self.selection.is_some() {
            let moved = match key {
                Key::Left => Some((-1, 0)),
                Key::Right => Some((1, 0)),
                Key::Up => Some((0, -1)),
                Key::Down => Some((0, 1)),
                _ => None,
            };
            if let Some((dx, dy)) = moved {
                let sel = self.selection.as_ref().unwrap();
                let (x, y) = (sel.x + dx * self.nudge, sel.y + dy * self.nudge);
                self.shift_selection(x, y);
                return;
            }
            match key {
                Key::Enter | Key::Escape => return self.deselect(),
                Key::Backspace => return self.clear_selection(),
                _ => {}
            }
        }
        match key {
            Key::Copy => self.copy(),
            Key::Cut => {
                self.copy();
                self.clear_selection();
            }
            Key::Paste => self.paste(),
            Key::Char('b' | 'p') => self.act(Action::Tool(Tool::Pencil), false),
            Key::Char('e') => self.act(Action::Tool(Tool::Eraser), false),
            Key::Char('g' | 'f') => self.act(Action::Tool(Tool::Fill), false),
            Key::Char('i') => self.act(Action::Tool(Tool::Picker), false),
            Key::Char('s') => self.act(Action::Tool(Tool::Select), false),
            Key::Char('m') => self.mirror = !self.mirror,
            Key::Char(' ') => self.act(Action::Play, false),
            Key::Char('n') => self.act(Action::Ask(Ask::New), false),
            Key::Left | Key::Char('h') => self.step_frame(-1),
            Key::Right | Key::Char('l') => self.step_frame(1),
            Key::Char('j') => self.step_sprite(1),
            Key::Char('k') => self.step_sprite(-1),
            Key::Save => self.act(Action::Save, false),
            Key::Undo => {
                self.selection = None;
                self.trimmed = Default::default();
                if let Some(prev) = self.undo.pop() {
                    let now = self.snapshot();
                    (self.sprite.frames, self.sprite.views, self.sprite.footprint) = prev;
                    self.redo.push(now);
                    self.frame = self.frame.min(self.sprite.frames.len() - 1);
                    self.changed = true;
                    self.dirty = true;
                }
            }
            Key::Redo => {
                self.selection = None;
                self.trimmed = Default::default();
                if let Some(next) = self.redo.pop() {
                    let now = self.snapshot();
                    (self.sprite.frames, self.sprite.views, self.sprite.footprint) = next;
                    self.undo.push(now);
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
        c.label(format!("{}{}", self.sprite.name, if self.unsaved() { "  (unsaved)" } else { "" }), 12.0, 12.0, 26.0, TEXT, Align::Left);

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
            let thumb = match b.action {
                Action::Frame(i) => Some((i, None)),
                Action::View(v) => Some((self.sprite.frames.len() + v, Some(["South", "East", "Footprint"][v]))),
                _ => None,
            };
            if b.action == Action::AddFootprint {
                c.rect(r.x, r.y, r.w, r.h, BUTTON);
                c.label("+", r.x + r.w / 2.0, r.y + 6.0, 30.0, TEXT, Align::Center);
                c.label("Footprint", r.x + r.w / 2.0, r.y + r.h - 18.0, 13.0, DIM, Align::Center);
                continue;
            }
            if let Some((i, label)) = thumb {
                c.rect(r.x, r.y, r.w, r.h, if b.on { HOT } else { PANEL });
                c.rect(r.x + 2.0, r.y + 2.0, r.w - 4.0, r.h - 4.0, [0.22, 0.22, 0.25, 1.0]);
                if let Some(&a) = self.live.get(i) {
                    let s = ((r.w - 8.0) / a.w().max(a.h())).min(4.0);
                    c.art(a, r.x + (r.w - a.w() * s) / 2.0, r.y + (r.h - a.h() * s) / 2.0, s, false, [1.0; 4]);
                }
                if let Some(l) = label {
                    c.label(l, r.x + r.w / 2.0, r.y + r.h - 16.0, 14.0, TEXT, Align::Center);
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
        c.label("Stands in 3D as", SIDE_X, 546.0, 16.0, DIM, Align::Left);
        match self.stand {
            0 => c.label(format!("Depth {} px", self.depth), SIDE_X, 645.0, 18.0, TEXT, Align::Left),
            1 => c.label(format!("Height {} px", self.height), SIDE_X, 645.0, 18.0, TEXT, Align::Left),
            CARVE => c.label(format!("Height {} px", self.view_height()), SIDE_X, 645.0, 18.0, TEXT, Align::Left),
            _ => {}
        }
        if let Some(v) = self.view {
            c.label(format!("{}: {}", VIEW_NAMES[v], VIEW_HINTS[v]), CANVAS.x + 8.0, CANVAS.y + 6.0, 16.0, HOT, Align::Left);
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

        let help = "S: select, drag to move, arrows nudge   Cmd+C/X/V: copy, cut, paste   J/K: sprites   H/L: frames   Right-click: pick   Cmd+Z: undo   Cmd+S: save";
        c.label(help, 12.0, 748.0, 13.0, DIM, Align::Left);
        if let Some((text, age)) = &self.status {
            let a = (4.0 - age).clamp(0.0, 1.0);
            c.rect(CANVAS.x, CANVAS.y + CANVAS.h - 40.0, CANVAS.w, 40.0, [0.0, 0.0, 0.0, 0.7 * a]);
            c.label(text.clone(), CANVAS.x + CANVAS.w / 2.0, CANVAS.y + CANVAS.h - 30.0, 18.0, [1.0, 1.0, 1.0, a], Align::Center);
        }
        if let Some(name) = &self.pending {
            c.rect(0.0, 0.0, 1024.0, 768.0, [0.0, 0.0, 0.0, 0.65]);
            let d = DIALOG;
            c.rect(d.x - 3.0, d.y - 3.0, d.w + 6.0, d.h + 6.0, HOT);
            c.rect(d.x, d.y, d.w, d.h, PANEL);
            c.label(format!("{} has unsaved changes", self.sprite.name), d.x + d.w / 2.0, d.y + 24.0, 28.0, HOT, Align::Center);
            c.label(format!("Save them before opening {name}?"), d.x + d.w / 2.0, d.y + 70.0, 22.0, TEXT, Align::Center);
            for (i, (label, choice)) in CHOICES.iter().enumerate() {
                let r = choice_rect(i);
                c.rect(r.x, r.y, r.w, r.h, if *choice == Choice::Save { HOT } else { BUTTON });
                let ink = if *choice == Choice::Save { [0.1, 0.1, 0.1, 1.0] } else { TEXT };
                c.label(*label, r.x + r.w / 2.0, r.y + (r.h - 20.0) / 2.0, 20.0, ink, Align::Center);
            }
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
        if let Some(sel) = &self.selection {
            if let Some(f) = &sel.float {
                for y in 0..f.h {
                    for x in 0..f.w {
                        let p = f.px[y * f.w + x];
                        let (px, py) = (o[0] + (sel.x + x as i32) as f32 * zoom, o[1] + (sel.y + y as i32) as f32 * zoom);
                        if p[3] > 0 && CANVAS.contains([px, py]) {
                            c.rect(px, py, zoom, zoom, rgba(p));
                        }
                    }
                }
            }
            // Marching ants: the outline's dashes walk around it.
            let (x0, y0) = (o[0] + sel.x as f32 * zoom, o[1] + sel.y as f32 * zoom);
            let (sw, sh) = (sel.w as f32 * zoom, sel.h as f32 * zoom);
            let dash = 6.0;
            let phase = (self.clock * 12.0) % (dash * 2.0);
            let edges = [(x0, y0, sw, true), (x0, y0 + sh - 2.0, sw, true), (x0, y0, sh, false), (x0 + sw - 2.0, y0, sh, false)];
            for (ex, ey, len, across) in edges {
                let mut t = -phase;
                while t < len {
                    let (a, b) = (t.max(0.0), (t + dash).min(len));
                    if b > a {
                        let r = if across { [ex + a, ey, b - a, 2.0] } else { [ex, ey + a, 2.0, b - a] };
                        c.rect(r[0], r[1], r[2], r[3], HOT);
                    }
                    t += dash * 2.0;
                }
            }
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

/// Lays the opaque pixels of `top` over `f` with its top-left at (x, y).
fn stamp(f: &mut Frame, top: &Frame, x: i32, y: i32) {
    for ty in 0..top.h as i32 {
        for tx in 0..top.w as i32 {
            if let Some(c) = top.get(tx, ty).filter(|c| c[3] > 0) {
                f.set(x + tx, y + ty, c);
            }
        }
    }
}

fn choice_rect(i: usize) -> Rect {
    let widths = [220.0, 150.0, 150.0];
    let x = DIALOG.x + 24.0 + widths[..i].iter().map(|w| w + 14.0).sum::<f32>();
    Rect { x, y: DIALOG.y + DIALOG.h - 76.0, w: widths[i], h: 48.0 }
}

fn rgba(c: Rgba) -> Color {
    [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, if c[3] == 0 { 0.0 } else { 1.0 }]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> Editor {
        let dir = std::env::temp_dir().join(format!("lh-editor-{}-{:?}", std::process::id(), std::thread::current().id()));
        let mut e = Editor::new(dir.clone(), "BARREL");
        e.catalog = dir.join("art.txt");
        e
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

    #[test]
    fn carving_starts_from_a_block_and_paints_on_the_views() {
        let mut e = editor();
        e.act(Action::Stand(CARVE), false);
        let [south, east] = e.sprite.views.clone().unwrap();
        assert_eq!((south.w, east.w, south.h), (16, 16, 10));
        e.act(Action::View(1), false);
        e.tool = Tool::Eraser;
        click_pixel(&mut e, 0, 0);
        assert_eq!(e.sprite.views.as_ref().unwrap()[1].get(0, 0), Some(CLEAR));
        assert_eq!(e.surfaces().len(), e.sprite.frames.len() + 2);
        e.act(Action::Depth(2), false);
        assert_eq!(e.view_height(), 12);
        e.key(Key::Undo);
        e.key(Key::Undo);
        assert_eq!(e.sprite.views.as_ref().unwrap()[1].get(0, 0), east.get(0, 0));
        assert_eq!(e.stand_text(), "carve");
    }

    #[test]
    fn carving_buttons_fit_and_none_overlap() {
        let mut e = editor();
        e.act(Action::Stand(CARVE), false);
        for _ in 0..8 {
            e.act(Action::AddFrame, false);
        }
        let b = e.buttons();
        assert!(b.iter().any(|b| b.action == Action::View(1)));
        for x in &b {
            assert!(x.rect.x + x.rect.w <= 1024.0 && x.rect.y + x.rect.h <= 768.0, "{:?}", x.action);
            assert!(!matches!(x.action, Action::Frame(_)) || x.rect.x + x.rect.w <= CANVAS.x + CANVAS.w, "{:?}", x.action);
            for y in &b {
                let apart = x.rect.x + x.rect.w <= y.rect.x || y.rect.x + y.rect.w <= x.rect.x || x.rect.y + x.rect.h <= y.rect.y || y.rect.y + y.rect.h <= x.rect.y;
                assert!(std::ptr::eq(x, y) || apart, "{:?} overlaps {:?}", x.action, y.action);
            }
        }
    }

    #[test]
    fn j_and_k_walk_the_list_and_h_and_l_the_frames() {
        let mut e = editor();
        let at = |e: &Editor| e.names.iter().position(|(n, _)| *n == e.sprite.name).unwrap();
        let start = at(&e);
        e.key(Key::Char('j'));
        assert_eq!(at(&e), start + 1);
        e.key(Key::Char('k'));
        e.key(Key::Char('k'));
        assert_eq!(at(&e), start - 1);
        e.act(Action::AddFrame, false);
        e.key(Key::Char('h'));
        assert_eq!(e.frame, 0);
        e.key(Key::Char('l'));
        assert_eq!(e.frame, 1);
    }

    #[test]
    fn switching_away_from_changes_asks_first() {
        let mut e = editor();
        click_pixel(&mut e, 0, 0);
        let name = e.sprite.name.clone();
        e.key(Key::Char('j'));
        assert_eq!(e.sprite.name, name, "still here until a choice is made");
        assert!(e.pending.is_some());
        e.key(Key::Char('b'));
        assert!(e.pending.is_some(), "other keys wait");
        e.key(Key::Escape);
        assert!(e.pending.is_none() && e.dirty && e.sprite.name == name);
        e.key(Key::Char('j'));
        e.key(Key::Char('d'));
        assert_ne!(e.sprite.name, name);
        assert!(!e.dirty);
    }

    #[test]
    fn save_and_switch_writes_then_opens() {
        let mut e = editor();
        click_pixel(&mut e, 0, 0);
        e.key(Key::Char('j'));
        let target = e.pending.clone().unwrap();
        e.key(Key::Enter);
        assert_eq!(e.sprite.name, target);
        assert!(e.dir.join("BARREL/0.png").exists());
        let _ = std::fs::remove_dir_all(&e.dir);
    }

    fn at(e: &Editor, x: i32, y: i32) -> [f32; 2] {
        let (o, zoom) = e.grid();
        [o[0] + (x as f32 + 0.5) * zoom, o[1] + (y as f32 + 0.5) * zoom]
    }

    #[test]
    fn dragging_a_selection_moves_its_pixels_and_undo_puts_them_back() {
        let mut e = editor();
        let before = e.current().clone();
        let px = |e: &Editor, x, y| e.current().get(x, y).unwrap();
        let original = px(&e, 5, 8);
        e.key(Key::Char('s'));
        e.mouse_down(at(&e, 4, 7), false, false);
        e.drag(at(&e, 6, 9));
        e.mouse_up();
        assert_eq!(e.selection.as_ref().map(|s| (s.x, s.y, s.w, s.h)), Some((4, 7, 3, 3)));
        e.mouse_down(at(&e, 5, 8), false, false);
        e.drag(at(&e, 7, 8));
        e.mouse_up();
        e.key(Key::Enter);
        assert_eq!(px(&e, 7, 8), original);
        assert!(e.selection.is_none());
        e.key(Key::Undo);
        assert_eq!(*e.current(), before);
    }

    #[test]
    fn copy_then_paste_onto_a_view_and_nudge_into_place() {
        let mut e = editor();
        e.act(Action::Stand(CARVE), false);
        e.sprite.frames[0] = Frame::blank(16, 16);
        e.sprite.frames[0].set(1, 1, [200, 0, 0, 255]);
        e.key(Key::Char('s'));
        e.mouse_down(at(&e, 0, 0), false, false);
        e.drag(at(&e, 2, 2));
        e.mouse_up();
        e.key(Key::Copy);
        e.act(Action::View(0), false);
        let under = e.current().get(3, 1).unwrap();
        let beside = e.current().get(2, 0).unwrap();
        e.key(Key::Paste);
        e.nudge = 1;
        e.key(Key::Right);
        e.key(Key::Right);
        assert!(e.surfaces()[e.sprite.frames.len()].get(3, 1) == Some([200, 0, 0, 255]), "previews show it while it floats");
        e.key(Key::Enter);
        let v = &e.sprite.views.as_ref().unwrap()[0];
        assert_eq!(v.get(3, 1), Some([200, 0, 0, 255]));
        assert_eq!(v.get(2, 0), Some(beside), "clear pixels in a paste leave what was there");
        e.key(Key::Undo);
        assert_eq!(e.sprite.views.as_ref().unwrap()[0].get(3, 1), Some(under));
    }

    #[test]
    fn delete_clears_the_selection_and_cut_copies_first() {
        let mut e = editor();
        e.key(Key::Char('s'));
        e.mouse_down(at(&e, 0, 0), false, false);
        e.drag(at(&e, 15, 15));
        e.mouse_up();
        e.key(Key::Cut);
        assert!(e.current().px.iter().all(|p| p[3] == 0));
        e.key(Key::Paste);
        e.key(Key::Enter);
        assert!(e.current().px.iter().any(|p| p[3] > 0));
    }

    #[test]
    fn a_footprint_is_optional_and_carves_from_its_own_outline() {
        let mut e = editor();
        e.act(Action::Stand(CARVE), false);
        let n = e.sprite.frames.len();
        assert_eq!(e.surfaces().len(), n + 2, "no footprint until one is added");
        assert!(e.buttons().iter().any(|b| b.action == Action::AddFootprint));
        e.act(Action::AddFootprint, false);
        assert_eq!(e.view, Some(FOOTPRINT));
        assert_eq!(e.surfaces().len(), n + 3);
        e.tool = Tool::Eraser;
        click_pixel(&mut e, 3, 3);
        assert_eq!(e.sprite.footprint.as_ref().unwrap().get(3, 3), Some(CLEAR));
        assert_ne!(e.sprite.frames[0].get(3, 3), Some(CLEAR), "the sprite keeps its colors");
        e.set_live((0..n + 3).map(|i| Art(i as u16)).collect());
        assert!(matches!(e.stand(), Stand::Carve { footprint: Some(_), .. }));
        e.act(Action::Width(2), false);
        assert_eq!(e.sprite.footprint.as_ref().unwrap().w, e.sprite.frames[0].w);
        e.act(Action::View(FOOTPRINT), false);
        e.act(Action::DeleteFrame, false);
        assert!(e.sprite.footprint.is_none() && e.view.is_none());
        e.key(Key::Undo);
        assert!(e.sprite.footprint.is_some());
    }

    #[test]
    fn a_footprint_saves_and_loads_beside_the_views() {
        let mut e = editor();
        e.act(Action::Stand(CARVE), false);
        e.act(Action::AddFootprint, false);
        e.act(Action::Save, false);
        let back = Sprite::load(&e.dir, "BARREL").unwrap();
        assert_eq!(back.footprint, e.sprite.footprint);
        assert!(back.views.is_some());
        let _ = std::fs::remove_dir_all(&e.dir);
    }

    #[test]
    fn lowering_then_raising_the_carving_brings_its_rows_back() {
        let mut e = editor();
        e.act(Action::Stand(CARVE), false);
        let views = e.sprite.views.clone().unwrap();
        let h = views[0].h as i32;
        e.sprite.views.as_mut().unwrap()[0].set(2, 0, [9, 9, 9, 255]);
        let marked = e.sprite.views.clone().unwrap();
        e.resize_views(h - 3);
        e.resize_views(h - 5);
        assert_eq!(e.view_height(), (h - 5) as usize);
        e.resize_views(h);
        assert_eq!(e.sprite.views.clone().unwrap(), marked, "every cut row is back where it was");
        e.resize_views(h + 2);
        let grown = &e.sprite.views.as_ref().unwrap()[0];
        assert_eq!(grown.get(2, 0), Some([9, 9, 9, 255]), "past the cut rows, the top row repeats");
        assert_eq!(grown.get(2, 2), Some([9, 9, 9, 255]));
        assert_eq!(grown.get(3, 0), marked[0].get(3, 0));
    }
}
