//! The editing pane: sprite list, pixel canvas, tools, palette and frames, laid out on a 1024x768 canvas.

use last_harvest_game::art::Art;
use last_harvest_game::{Align, Canvas, Color, Stand};

use crate::history::Commit;
use crate::sprite::{self, CLEAR, Frame, Rgba, Sprite};
use crate::store::Store;

mod cards;
mod coats;
mod maps;
mod palette;
mod tables;

use cards::HeldImport;
use maps::{MapEdit, MapTool, PROP_HP_Y, PROP_OPEN_Y, PROP_PALETTE_ROWS};
use tables::DataEdit;

const UNDO_DEPTH: usize = 100;
const LIST: Rect = Rect { x: 8.0, y: 52.0, w: 200.0, h: 690.0 };
const LIST_ROW: f32 = 22.0;
const CANVAS: Rect = Rect { x: 216.0, y: 52.0, w: 560.0, h: 560.0 };
const SIDE_X: f32 = 784.0;
const SIDE_W: f32 = 232.0;
const SWATCH: f32 = 23.0;
const THUMB: f32 = 64.0;
/// How many of a sprite's family Compare lays out at once.
const FAMILY_MAX: usize = 6;
/// How opaque Onion draws the previous frame.
const ONION: f32 = 0.3;
const MAX_SIZE: usize = 128;
const STANDS: [&str; 6] = ["Prop", "Slab", "Figure", "Ground", "Carve", "Turned"];
const CARVE: u8 = 4;
const TURNED: u8 = 5;
const VIEW_NAMES: [&str; 7] = ["South view", "East view", "Footprint", "Top", "Top", "North view", "West view"];
/// Each view's name on its thumbnail.
const VIEW_LABELS: [&str; 7] = ["South", "East", "Footprint", "Top", "Top", "North", "West"];
const VIEW_HINTS: [&str; 7] = [
    "the sprite seen from the south: west on the left",
    "the sprite seen from the east: south on the left",
    "the outline from above: only opaque or clear counts",
    "each column's color on top in 3D; clear leaves its own",
    "the colors on top in 3D; clear leaves the sprite's",
    "the sprite seen from the north: east on the left; clear leaves the south view's",
    "the sprite seen from the west: north on the left; clear leaves the east view's",
];
const FOOTPRINT: usize = 2;
/// The view that is a figure's top strip.
const TOP: usize = 3;
/// A carving's top colors, and its north and west views.
const ROOF: usize = 4;
const NORTH: usize = 5;
const WEST: usize = 6;
/// The pictures a carving may have or not, in the order they pack after its south and east views.
const CARVE_EXTRAS: [usize; 4] = [FOOTPRINT, ROOF, NORTH, WEST];
/// Every picture a sprite may have or not.
const OPTIONAL: [usize; 5] = [FOOTPRINT, TOP, ROOF, NORTH, WEST];
/// How a sprite stands, by its button: prop, slab, figure, ground, carve, turned.
const FIGURE: u8 = 2;
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
    Line,
    Rect,
    Ellipse,
}

impl Tool {
    /// Whether it draws a shape by dragging from one corner to the other.
    fn shapes(self) -> bool {
        matches!(self, Tool::Line | Tool::Rect | Tool::Ellipse)
    }
}

/// A shape being dragged out: where it started, the surface as it was, and whether Shift was held at the start.
#[derive(Clone)]
struct Drawing {
    from: (i32, i32),
    base: Frame,
    shift: bool,
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
    /// A Lospec palette's name, to draw from its colors.
    Palette,
}

/// A line of a card's text typed on its stats page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Line {
    Name,
    About,
    /// A data file's line's value.
    Value,
    /// A new `key value` line for a data file's section.
    Added,
}

impl Line {
    fn key(self) -> &'static str {
        match self {
            Line::Name => "name",
            Line::About => "about",
            Line::Value | Line::Added => "",
        }
    }

    /// Whether `c` can be typed into this line; a `#` would start a comment.
    fn takes(self, c: char) -> bool {
        match self {
            Line::Name | Line::Value | Line::Added => !c.is_control() && c != '#',
            Line::About => (' '..='~').contains(&c) && c != '#',
        }
    }

    fn max(self) -> usize {
        match self {
            Line::Name => NAME_MAX,
            Line::About => last_harvest_game::ABOUT_MAX,
            Line::Value | Line::Added => 300,
        }
    }
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
    /// Adds one of a carving's extras, by its view.
    AddView(usize),
    AddTop,
    History,
    Restore,
    Back,
    Play,
    Width(i32),
    Height(i32),
    Stand(u8),
    Depth(i32),
    /// Floats a slab higher or lower.
    Float(i32),
    Fps(i32),
    Color(Rgba),
    Onion,
    Compare,
    /// Shows a card's stats on the canvas, or its pixels again.
    Stats,
    /// Steps one of `CARD_NUMBERS` by its step times the value.
    StatStep(usize, i32),
    Flag(usize),
    Sound(i32),
    DropSound,
    EditName,
    EditAbout,
    /// Turns a weapon, by its place in `weapons()`, on or off in a player card's start at a difficulty.
    StartWeapon(usize, usize),
    /// Gives a player card the game's own start at a difficulty.
    StartDefault(usize),
    MapTool(MapTool),
    Ground(u8),
    /// Picks a prop by its place in the map editor's picker.
    PickProp(usize),
    /// Turns the prop picker a page back or on.
    PickPage(i32),
    Picket(bool),
    /// Grows or shrinks the map by tiles across and down.
    MapSize(i32, i32),
    /// Lays a farm out as the random generator does, over the whole map.
    Randomize,
    MapFit,
    /// Hits the card's figure in the previews, killing it when true.
    Strike(bool),
    /// Turns whether a prop stops shots, the open prop's or the map's selected one.
    Cover,
    /// Steps a prop's hit points, the open prop's or the map's selected one, by 10 times the value.
    Hp(i32),
    /// Keeps the open prop's last frame for when it breaks, or not.
    Broken,
    /// Gives the map's selected prop its sprite's hit points and cover again.
    PropDefaults,
    /// Fills a blank flash, hurt or dead part with the game's own frames.
    Generate,
    /// Blanks a flash, hurt or dead part, so the game draws it.
    ClearPart,
    /// Clears every pixel of the paint color.
    ClearColor,
    /// Steps how many of the open prop's top rows a figure walks through, or the map's selected prop's.
    Open(i32),
    /// Opens the data page section that says how the open sprite plays.
    Data,
    /// Steps a player card's health, speed or magnet down or up.
    PlayerBase(usize, i32),
    /// Steps which frame the map's selected prop shows, through each one and back to all of them animating.
    LineFrame(i32),
    /// Steps how fast the map's selected prop animates; 0 keeps its sprite's own pace.
    LineFps(i32),
    /// Steps the data page's line, by its place in the section, down or up.
    DataStep(usize, i32),
    /// Starts typing the data page's line's value.
    DataEdit(usize),
    DataRemove(usize),
    DataAdd,
    DataPage(i32),
    /// Leaves the paint color out of the 3D view, or puts it back.
    ClearIn3d,
    /// Shows the Coats page on the canvas, or the pixels again.
    Coats,
    /// Marks the brush's color as paint, or not.
    PaintColor,
    /// Shows a coat in the previews, or the frames as drawn.
    Coat(Option<usize>),
    /// Gives a coat's shade of a paint color the brush's color.
    Shade(usize, usize),
    DropCoat(usize),
    AddCoat,
}

/// What each of a card's numbers is called on its stats page, and how far a press of - or + moves it.
const STAT_ROWS: [(&str, f32); 10] = [
    ("Health", 5.0),
    ("Speed", 1.0),
    ("Bite", 1.0),
    ("Money", 1.0),
    ("Weight", 0.1),
    ("Gibs", 1.0),
    ("Arrives after (minutes)", 0.5),
    ("Chance", 0.01),
    ("Chance per minute", 0.005),
    ("Charge (times speed near you)", 0.1),
];
const FLAG_NAMES: [&str; 3] = ["Heavy", "Leads hordes", "Harvester"];
const STAT_TOP: f32 = CANVAS.y + 100.0;
const STAT_ROW: f32 = 22.0;
const STAT_BUTTON: f32 = 20.0;
const NAME_MAX: usize = 24;
const DELETE_CONFIRM: f32 = 3.0;
/// A player card's stats: a block per difficulty with its weapon toggles, five across.
const START_TOP: f32 = CANVAS.y + 100.0;
const START_BLOCK: f32 = 126.0;
const START_ACROSS: usize = 5;
const START_ROW: f32 = 24.0;
/// A player card's health, speed and magnet, under its starting weapons.
const BASE_Y: f32 = START_TOP + 3.0 * START_BLOCK + 4.0;
/// How far a press of - or + moves a player card's health, speed and magnet.
const BASE_STEPS: [f32; 3] = [5.0, 2.0, 2.0];

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

/// Browsing a sprite's committed versions: the commits, the one on show, and what was open before.
struct Browse {
    commits: Vec<Commit>,
    at: usize,
    /// The sprite's files differ from its last commit.
    changed: bool,
    before: Stash,
}

/// The sprite as it was before browsing began, to go back to.
struct Stash {
    sprite: Sprite,
    stand: u8,
    depth: f32,
    height: f32,
    lift: f32,
    frame: usize,
    view: Option<usize>,
    dirty: bool,
    stand_touched: bool,
}

const HISTORY_ROW: f32 = 40.0;
/// How far each nesting level of the list steps in.
const INDENT: f32 = 12.0;
const HISTORY_TOP: f32 = LIST.y + 48.0;
const RESTORE: Rect = Rect { x: LIST.x + 8.0, y: LIST.y + LIST.h - 44.0, w: 88.0, h: 36.0 };
const BACK: Rect = Rect { x: LIST.x + 104.0, y: LIST.y + LIST.h - 44.0, w: 88.0, h: 36.0 };

/// Everything an undo puts back: the frames, the carving views, and the optional pictures in `OPTIONAL` order.
type Snapshot = (Vec<Frame>, Option<[Frame; 2]>, [Option<Frame>; 5]);

/// One of the pictures a sprite may have or not, by its view.
fn optional(s: &Sprite, view: usize) -> &Option<Frame> {
    match view {
        FOOTPRINT => &s.footprint,
        ROOF => &s.roof,
        NORTH => &s.north,
        WEST => &s.west,
        _ => &s.top,
    }
}

fn optional_mut(s: &mut Sprite, view: usize) -> &mut Option<Frame> {
    match view {
        FOOTPRINT => &mut s.footprint,
        ROOF => &mut s.roof,
        NORTH => &mut s.north,
        WEST => &mut s.west,
        _ => &mut s.top,
    }
}

fn snapshot_of(s: &Sprite) -> Snapshot {
    (s.frames.clone(), s.views.clone(), OPTIONAL.map(|v| optional(s, v).clone()))
}

fn restore(s: &mut Sprite, (frames, views, pictures): Snapshot) {
    (s.frames, s.views) = (frames, views);
    for (v, f) in OPTIONAL.into_iter().zip(pictures) {
        *optional_mut(s, v) = f;
    }
}

/// A line of the sprite list.
enum Row<'a> {
    Heading { title: &'static str, depth: usize, count: usize, saved: usize, open: bool },
    Sprite(&'a (String, bool)),
}

pub struct Editor {
    store: Box<dyn Store>,
    pub sprite: Sprite,
    names: Vec<(String, bool)>,
    /// The list's headings that are unfolded; the open sprite's always is.
    unfolded: Vec<&'static str>,
    scroll: f32,
    frame: usize,
    tool: Tool,
    /// The tool to go back to once the picker has taken a color.
    before_pick: Tool,
    drawing: Option<Drawing>,
    mirror: bool,
    /// The previous frame shows faintly under the one being drawn.
    onion: bool,
    /// The canvas lays out the open sprite's family side by side, each with its frames.
    compare: bool,
    family: Vec<(String, Vec<Frame>)>,
    color: Rgba,
    palette: Vec<Rgba>,
    /// The Lospec palette drawn from, else the game's own.
    palette_name: Option<String>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// Edits not yet saved.
    dirty: bool,
    /// The previews need repacking.
    pub changed: bool,
    /// A Hurt (false) or Kill (true) for the previews to play.
    pub strike: Option<bool>,
    /// Where the last packing put a card's figure among the surfaces, and whether it has dead frames.
    figure_at: Option<(usize, bool)>,
    /// Where the last packing put the frames' 3D copies among the surfaces, and how many there are.
    solids_at: Option<(usize, usize)>,
    /// Where the last packing put the Coats page's rows among the surfaces.
    coats_at: Option<usize>,
    /// The open card's parts as saved, read when it opens or saves.
    card_parts: Vec<Vec<Frame>>,
    /// The hand-made map being edited, in place of a sprite.
    map: Option<MapEdit>,
    /// The data file section open on the data page, if one is.
    data: Option<DataEdit>,
    /// The data page section that says how the open sprite plays, found when it opens.
    data_link: Option<String>,
    /// Players' props packed for the previews while a map is open, in order: their fields and frame counts.
    map_props: Vec<(last_harvest_game::PropFields, usize, usize)>,
    playing: bool,
    clock: f32,
    /// An index into `STANDS`.
    stand: u8,
    /// Which carving view is on the canvas instead of a frame.
    view: Option<usize>,
    depth: f32,
    height: f32,
    /// How far a slab floats off the ground.
    lift: f32,
    /// Whether the stand has been changed since it was loaded or saved.
    stand_touched: bool,
    pub fps: f32,
    ask: Option<(Ask, String)>,
    /// A sprite to switch to once the current one's unsaved changes are saved or dropped.
    pending: Option<String>,
    /// An import over the open card, held with `pending` until its unsaved changes are saved or dropped.
    held_import: Option<HeldImport>,
    selection: Option<Selection>,
    grab: Option<Grab>,
    /// Rows of the south, east, north and west views cut off by lowering the carving's height, highest first.
    trimmed: [Vec<Vec<Rgba>>; 4],
    /// Narrows the sprite list to names containing it; `typing` while it's being typed.
    filter: String,
    typing: bool,
    /// The color picker stands in for the tool while a key is held.
    peek: bool,
    /// The last pixel drawn, which a Shift-click draws a line from.
    last_dot: Option<(i32, i32)>,
    browse: Option<Browse>,
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
    /// Magnification of the pixel canvas past fitting it, and how far it is moved off center.
    lens: f32,
    pan: [f32; 2],
    /// A card's stats are on the canvas in place of its pixels.
    stats: bool,
    /// The Coats page is on the canvas in place of the pixels.
    coats_page: bool,
    /// The coat the previews show, else the frames as drawn.
    coat: Option<usize>,
    /// The card's name or about as it's being typed.
    naming: Option<(Line, String)>,
    /// The other cards' texts, read when the stats page opens, for the spawn mix.
    other_cards: Vec<String>,
    /// The open card's own death sound, by file name, when it has one.
    own_sound: Option<String>,
    mix: Option<Result<last_harvest_game::SpawnMix, String>>,
    /// The public mod editor: every sprite is an enemy card, and cards come and go by Import and Export.
    modding: bool,
    /// Seconds left for a second press of Delete card to go ahead.
    delete_armed: f32,
}

impl Editor {
    /// An editor on the sprite `name`, or on none until one is picked when `name` is empty.
    pub fn new(store: Box<dyn Store>, name: &str) -> Self {
        let sprite = store.load(name).unwrap_or(Sprite { name: name.into(), frames: vec![Frame::blank(16, 16)], views: None, footprint: None, roof: None, north: None, west: None, top: None, card: None, clear_in_3d: Vec::new(), paint: Default::default() });
        let names = store.names();
        let mut e = Self {
            store,
            sprite,
            names,
            unfolded: Vec::new(),
            scroll: 0.0,
            frame: 0,
            tool: Tool::Pencil,
            before_pick: Tool::Pencil,
            drawing: None,
            mirror: false,
            onion: false,
            compare: false,
            family: Vec::new(),
            color: [0x55, 0x3c, 0x2a, 255],
            palette: sprite::palette(),
            palette_name: None,
            undo: Vec::new(),
            redo: Vec::new(),
            dirty: false,
            changed: true,
            strike: None,
            figure_at: None,
            solids_at: None,
            coats_at: None,
            card_parts: Vec::new(),
            map: None,
            data: None,
            data_link: None,
            map_props: Vec::new(),
            playing: false,
            clock: 0.0,
            stand: 0,
            view: None,
            depth: 12.0,
            height: 10.0,
            lift: 0.0,
            stand_touched: false,
            fps: 6.0,
            ask: None,
            pending: None,
            held_import: None,
            selection: None,
            grab: None,
            trimmed: Default::default(),
            filter: String::new(),
            typing: false,
            peek: false,
            last_dot: None,
            browse: None,
            nudge: 1,
            clipboard: None,
            status: None,
            stroke: None,
            channel: None,
            hover: None,
            live: Vec::new(),
            lens: 1.0,
            pan: [0.0; 2],
            stats: false,
            coats_page: false,
            coat: None,
            naming: None,
            other_cards: Vec::new(),
            own_sound: None,
            mix: None,
            modding: false,
            delete_armed: 0.0,
        };
        e.names = e.all_names();
        if !e.open_table(name) {
            e.open_map(name);
        }
        e.load_stand();
        e.load_card_parts();
        e.data_link = e.data_link_for(name);
        e.reveal();
        e
    }

    /// Makes this the mod editor: names typed for New and Copy are card ids, and there's no history to browse.
    pub fn set_modding(&mut self) {
        self.modding = true;
    }

    /// No sprite is open yet: the list waits for a pick, or New.
    fn none_open(&self) -> bool {
        self.sprite.name.is_empty()
    }

    /// How the sprite should stand in the previews.
    pub fn stand(&self) -> Stand {
        let n = self.sprite.frames.len();
        match self.stand {
            0 => Stand::Prop { depth: self.depth },
            2 => Stand::Figure,
            3 => Stand::Ground,
            CARVE if self.own_packed() == n + 2 + self.carve_extras().count() => {
                let at = |v: usize| optional(&self.sprite, v).as_ref().and_then(|_| self.live.get(self.surface_of(v)).copied());
                Stand::Carve { footprint: at(FOOTPRINT), roof: at(ROOF), south: self.live[n], east: self.live[n + 1], north: at(NORTH), west: at(WEST) }
            }
            TURNED => match self.live.get(n + 1).filter(|_| self.sprite.views.is_some()) {
                Some(&east) => Stand::Turned { east, depth: Some(self.depth) },
                None => Stand::Prop { depth: self.depth },
            },
            _ => Stand::Slab { height: self.height, lift: self.lift },
        }
    }

    /// How many of the packed pictures are the sprite's own frames and views, before any 3D copies or Coats rows.
    fn own_packed(&self) -> usize {
        self.solids_at.map(|(k, _)| k).or(self.coats_at).unwrap_or(self.live.len()).min(self.live.len())
    }

    /// The stand as the catalog and `stand.txt` write it.
    fn stand_text(&self) -> String {
        match self.stand {
            CARVE => "carve".into(),
            TURNED => format!("turned {}", self.depth),
            _ => sprite::stand_text(self.stand()),
        }
    }

    /// How many columns of the frame on show hold a pixel: how thick it is seen from above.
    fn outline_width(&self) -> f32 {
        let f = &self.sprite.frames[self.frame];
        ((0..f.w as i32).filter(|&x| (0..f.h as i32).any(|y| f.get(x, y).is_some_and(|p| p[3] > 0))).count() as f32).max(1.0)
    }

    /// Whether the sprite stands by its views, carved or turned, and has them.
    fn by_views(&self) -> bool {
        matches!(self.stand, CARVE | TURNED) && self.sprite.views.is_some()
    }

    /// Everything to pack for the previews: the frames, then the carving views when carving, with any
    /// floating pixels shown where they would land, then a card's figure.
    pub fn surfaces(&mut self) -> Vec<Frame> {
        if self.data.is_some() {
            self.figure_at = None;
            self.solids_at = None;
            return Vec::new();
        }
        if self.map.is_some() {
            self.figure_at = None;
            let (frames, props) = self.prop_surfaces();
            self.map_props = props;
            return frames;
        }
        let mut out = self.own_surfaces();
        self.figure_at = None;
        if let Some((figure, dead)) = self.figure_frames() {
            self.figure_at = Some((out.len(), dead));
            out.extend(figure);
        }
        self.solids_at = None;
        let clear = &self.sprite.clear_in_3d;
        if !clear.is_empty() && !self.none_open() {
            let n = self.sprite.frames.len();
            let solids: Vec<Frame> = out[..n]
                .iter()
                .zip(&self.sprite.frames)
                .map(|(f, drawn)| Frame { px: f.px.iter().zip(&drawn.px).map(|(p, d)| if clear.contains(d) { CLEAR } else { *p }).collect(), ..f.clone() })
                .collect();
            self.solids_at = Some((out.len(), n));
            out.extend(solids);
        }
        self.coats_at = None;
        if self.coats_page && !self.none_open() {
            self.coats_at = Some(out.len());
            out.extend(self.coat_rows());
        }
        out
    }

    /// The card's figure as packed for the previews, for Hurt and Kill.
    pub fn figure(&self) -> Option<last_harvest_game::Figure> {
        let (k, dead) = self.figure_at?;
        let a = self.live.get(k..k + cards::FIGURE_FRAMES)?;
        let three = |i: usize| [a[i], a[i + 1], a[i + 2]];
        let gibs = self.sprite.card.as_deref().and_then(|t| last_harvest_game::card_fields(t).ok());
        let gibs = gibs.and_then(|f| f.numbers.iter().find(|(k, _)| *k == "gibs").map(|(_, v)| *v as usize)).unwrap_or(9);
        Some(last_harvest_game::Figure { stand: three(0), flash: three(3), hurt: three(6), dead: dead.then(|| three(9)), gibs })
    }

    fn own_surfaces(&self) -> Vec<Frame> {
        if self.none_open() {
            return Vec::new();
        }
        let extras = self.carve_extras().map(|v| optional(&self.sprite, v).as_ref().unwrap()).filter(|_| self.stand == CARVE);
        let views = self.sprite.views.iter().flatten().chain(extras).filter(|_| self.by_views());
        let top = self.sprite.top.iter().filter(|_| self.stand == FIGURE);
        let mut out: Vec<Frame> = self.sprite.frames.iter().chain(views).chain(top).map(|f| self.in_coat(f)).collect();
        if let Some(Selection { x, y, float: Some(f), .. }) = &self.selection {
            let i = self.view.map_or(self.frame, |v| self.surface_of(v));
            if let Some(target) = out.get_mut(i) {
                stamp(target, f, *x, *y);
            }
        }
        out
    }

    /// Where a view's drawing sits among `surfaces`: the carving views follow the frames, then the carving's extras
    /// it has, and a figure's top strip follows the frames.
    fn surface_of(&self, view: usize) -> usize {
        let n = self.sprite.frames.len();
        match view {
            TOP => n,
            0 | 1 => n + view,
            _ => n + 2 + self.carve_extras().take_while(|&v| v != view).count(),
        }
    }

    /// The carving's extras the sprite has, in the order they pack.
    fn carve_extras(&self) -> impl Iterator<Item = usize> + '_ {
        CARVE_EXTRAS.into_iter().filter(|&v| optional(&self.sprite, v).is_some())
    }

    /// The frames to show in the previews: all of them while playing, else the one being edited.
    /// A still part's frames show as the game draws them: blank ones from the walk.
    pub fn shown(&self) -> Vec<Art> {
        if self.map.is_some() || self.data.is_some() {
            return Vec::new();
        }
        let n = self.sprite.frames.len().min(self.live.len());
        let still = self.still_row().and_then(|row| self.figure().map(|f| [f.flash, f.hurt, f.dead.unwrap_or(f.hurt)][row - 3]));
        let art = |i: usize| still.and_then(|s| s.get(i).copied()).unwrap_or(self.live[i]);
        if self.playing { (0..n).map(art).collect() } else { (self.frame < n).then(|| art(self.frame)).into_iter().collect() }
    }

    /// The top strip in the previews, for a figure that has one.
    pub fn top_art(&self) -> Option<Art> {
        self.sprite.top.as_ref().filter(|_| self.stand == FIGURE).and_then(|_| self.live.get(self.surface_of(TOP)).copied())
    }

    pub fn set_live(&mut self, live: Vec<Art>) {
        let solids = self.solids_at.map_or(Vec::new(), |(k, n)| (0..n).filter_map(|i| Some((*live.get(i)?, *live.get(k + i)?))).collect());
        last_harvest_game::art::set_live_solids(solids);
        self.live = live;
        if self.map.is_some() {
            self.register_props();
        }
    }

    pub fn title(&self) -> String {
        let app = if self.modding { "LastHarvest card editor" } else { "LastHarvest sprite editor" };
        if self.none_open() {
            return app.into();
        }
        format!("{}{} - {app}", self.sprite.name, if self.unsaved() { " *" } else { "" })
    }

    fn unsaved(&self) -> bool {
        self.dirty || self.stand_touched || self.map.as_ref().is_some_and(|m| m.dirty) || self.data.as_ref().is_some_and(|d| d.dirty)
    }

    /// What the canvas shows: a frame, or a carving view.
    fn current(&self) -> &Frame {
        match (self.view, &self.sprite.views) {
            (Some(v), Some(views)) if v < 2 => &views[v],
            (Some(v), _) if v >= 2 && optional(&self.sprite, v).is_some() => optional(&self.sprite, v).as_ref().unwrap(),
            _ => &self.sprite.frames[self.frame],
        }
    }

    fn current_mut(&mut self) -> &mut Frame {
        match self.view {
            Some(v) if v < 2 && self.sprite.views.is_some() => &mut self.sprite.views.as_mut().unwrap()[v],
            Some(v) if v >= 2 && optional(&self.sprite, v).is_some() => optional_mut(&mut self.sprite, v).as_mut().unwrap(),
            _ => &mut self.sprite.frames[self.frame],
        }
    }

    fn snapshot(&self) -> Snapshot {
        snapshot_of(&self.sprite)
    }

    /// The carving height, which is how tall the views are.
    fn view_height(&self) -> usize {
        self.sprite.views.as_ref().map_or(self.height as usize, |v| v[0].h)
    }

    /// The stand as saved, or a guess from the name and size when none was.
    fn load_stand(&mut self) {
        let saved = self.store.stand(&self.sprite.name);
        self.apply_stand(saved);
    }

    /// Sets the stand from how it was written, or a guess from the name and size when it wasn't.
    fn apply_stand(&mut self, saved: Option<String>) {
        self.stand_touched = false;
        self.view = None;
        let packed = last_harvest_game::art::ALL.iter().find(|(n, _)| *n == self.sprite.name).and_then(|(_, a)| a.first().copied());
        let open = self.open_prop_fields().map_or(0, |f| f.open);
        let f = self.current();
        self.depth = last_harvest_game::default_prop_depth(f.w as f32, f.h as f32, packed, open);
        self.height = 10.0;
        self.lift = 0.0;
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
        if let Some(depth) = saved.as_deref().and_then(last_harvest_game::turned_depth).filter(|_| self.sprite.views.is_some()) {
            self.stand = TURNED;
            self.depth = depth.unwrap_or_else(|| self.outline_width());
            return;
        }
        let s = saved.as_deref().and_then(sprite::parse_stand).unwrap_or_else(guess);
        self.stand = match s {
            Stand::Prop { depth } => {
                self.depth = depth;
                0
            }
            Stand::Slab { height, lift } => {
                (self.height, self.lift) = (height, lift);
                1
            }
            Stand::Figure => 2,
            Stand::Ground => 3,
            Stand::Carve { .. } => CARVE,
            Stand::Turned { .. } => TURNED,
        };
    }

    pub fn say(&mut self, text: impl Into<String>) {
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
        self.last_dot = None;
        self.names = self.all_names();
        self.data = None;
        self.data_link = None;
        if self.open_table(name) || self.open_map(name) {
            return;
        }
        self.data_link = self.data_link_for(name);
        if self.store.load(name).is_some() {
            self.map = None;
        }
        self.trimmed = Default::default();
        if let Some(s) = self.store.load(name) {
            self.sprite = s;
            self.frame = 0;
            self.undo.clear();
            self.redo.clear();
            self.dirty = false;
            self.changed = true;
            self.fit();
            self.load_stand();
            self.load_family();
            self.load_card_parts();
            if let Some(fps) = self.sprite.card.as_deref().filter(|_| crate::store::props::id(name).is_some()).and_then(|t| last_harvest_game::prop_fields(t).ok()).map(|f| f.fps).filter(|v| *v > 0.0) {
                self.fps = fps;
            }
            self.stats &= self.is_card();
            self.coats_page &= !self.is_card();
            self.coat = None;
            self.naming = None;
            self.delete_armed = 0.0;
            if self.stats {
                self.load_other_cards();
            }
        }
    }

    /// The sprites named like this one but for the last word, such as `KID_FRONT`, `KID_SIDE` and `KID_HURT`,
    /// in catalog order; at most `FAMILY_MAX`, around this one.
    fn family_names(&self) -> Vec<String> {
        let stem = |n: &str| n.rsplit_once('_').map(|(s, _)| s.to_string());
        let Some(mine) = stem(&self.sprite.name) else { return vec![self.sprite.name.clone()] };
        let all: Vec<String> = self.names.iter().map(|(n, _)| n.clone()).filter(|n| stem(n).as_ref() == Some(&mine)).collect();
        let at = all.iter().position(|n| *n == self.sprite.name).unwrap_or(0);
        let start = at.saturating_sub(FAMILY_MAX / 2).min(all.len().saturating_sub(FAMILY_MAX));
        all.into_iter().skip(start).take(FAMILY_MAX).collect()
    }

    /// Loads the rest of the family's frames for Compare; the open sprite draws from its live copy.
    fn load_family(&mut self) {
        self.family = if self.compare {
            self.family_names().into_iter().map(|n| {
                let frames = if n == self.sprite.name { Vec::new() } else { self.store.load(&n).map(|s| s.frames).unwrap_or_default() };
                (n, frames)
            }).collect()
        } else {
            Vec::new()
        };
    }

    /// Where Compare puts each of the family on the canvas, and the shared size of a pixel; None when it's off.
    fn compare_layout(&self) -> Option<(Vec<[f32; 2]>, f32)> {
        if !self.compare || self.view.is_some() || self.family.len() < 2 {
            return None;
        }
        let shown: Vec<&Frame> = self.family.iter().map(|(n, frames)| self.family_frame(n, frames)).collect();
        let gap = 16.0;
        let across: f32 = shown.iter().map(|f| f.w as f32).sum();
        let tall = shown.iter().map(|f| f.h).max().unwrap_or(1) as f32;
        let room = CANVAS.w - gap * (shown.len() + 1) as f32;
        let zoom = (room / across).min((CANVAS.h - 60.0) / tall).floor().max(1.0);
        let mut x = CANVAS.x + (CANVAS.w - (across * zoom + gap * (shown.len() - 1) as f32)) / 2.0;
        let origins = shown
            .iter()
            .map(|f| {
                let at = [x, CANVAS.y + (CANVAS.h - f.h as f32 * zoom) / 2.0];
                x += f.w as f32 * zoom + gap;
                at
            })
            .collect();
        Some((origins, zoom))
    }

    /// The frame Compare shows for one of the family: the same frame number as the open one, where it has it.
    fn family_frame<'a>(&'a self, name: &str, frames: &'a [Frame]) -> &'a Frame {
        if name == self.sprite.name { self.current() } else { frames.get(self.frame.min(frames.len().saturating_sub(1))).unwrap_or(self.current()) }
    }

    pub fn tick(&mut self, dt: f32) {
        self.clock += dt;
        self.delete_armed = (self.delete_armed - dt).max(0.0);
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
        if let Some((origins, zoom)) = self.compare_layout() {
            let at = self.family.iter().position(|(n, _)| *n == self.sprite.name).unwrap_or(0);
            return (origins[at], zoom);
        }
        let f = self.current();
        let fit = (CANVAS.w / f.w as f32).min(CANVAS.h / f.h as f32).floor().max(1.0);
        let zoom = (fit * self.lens).round().max(1.0);
        let (w, h) = (f.w as f32 * zoom, f.h as f32 * zoom);
        ([CANVAS.x + (CANVAS.w - w) / 2.0 + self.pan[0], CANVAS.y + (CANVAS.h - h) / 2.0 + self.pan[1]], zoom)
    }

    /// Magnifies the pixel canvas `factor` times about `center`, then moves it by `pan`, in pane units.
    pub fn zoom_canvas(&mut self, center: [f32; 2], factor: f32, pan: [f32; 2]) {
        if self.map.is_some() {
            return self.map_pinch(center, factor, pan);
        }
        let (o, zoom) = self.grid();
        let under = [(center[0] - o[0]) / zoom, (center[1] - o[1]) / zoom];
        self.lens = (self.lens * factor).clamp(1.0, 32.0);
        self.pan = [0.0; 2];
        let (base, zoom) = self.grid();
        let (w, h) = (self.current().w as f32 * zoom, self.current().h as f32 * zoom);
        // Some of the sprite always stays in sight.
        let reach = [(w + CANVAS.w) / 2.0 - 48.0, (h + CANVAS.h) / 2.0 - 48.0].map(|v| v.max(0.0));
        self.pan = [0, 1].map(|i| (center[i] - under[i] * zoom - base[i] + pan[i]).clamp(-reach[i], reach[i]));
    }

    /// Fits the whole surface on the canvas again.
    pub fn fit(&mut self) {
        (self.lens, self.pan) = (1.0, [0.0; 2]);
    }

    /// Whether `p` is on the pixel canvas with nothing in front of it.
    pub fn on_canvas(&self, p: [f32; 2]) -> bool {
        CANVAS.contains(p) && self.pending.is_none() && self.ask.is_none() && !self.stats && !self.coats_page && !self.none_open()
    }

    /// Whether `p` is on a row of the sprite list.
    pub fn on_list(&self, p: [f32; 2]) -> bool {
        LIST.contains(p) && p[1] >= self.list_top() && self.browse.is_none() && self.pending.is_none() && self.ask.is_none()
    }

    /// Scrolls the sprite list `dy` pane units, the way a finger drags it.
    pub fn drag_list(&mut self, dy: f32) {
        self.wheel([LIST.x + 1.0, LIST.y + LIST.h - 1.0], dy / (LIST_ROW * 3.0));
    }

    /// Forgets where the pointer is, once a finger or pen lifts.
    pub fn unhover(&mut self) {
        self.hover = None;
    }

    /// The name or search being typed, while one is.
    pub fn text(&self) -> Option<&str> {
        if let Some((_, name)) = &self.naming {
            return Some(name);
        }
        if self.typing { Some(&self.filter) } else { self.ask.as_ref().map(|(_, name)| name.as_str()) }
    }

    /// Whether the text being typed takes any case and spaces, as a card's name does, rather than a sprite name's capitals.
    pub fn free_text(&self) -> bool {
        self.naming.is_some()
    }

    /// Replaces the name or search being typed, as typing `text` key by key would leave it.
    pub fn set_text(&mut self, text: &str) {
        if let Some((line, name)) = &mut self.naming {
            *name = text.chars().filter(|&c| line.takes(c)).take(line.max()).collect();
            return;
        }
        if let Some((Ask::Palette, name)) = &mut self.ask {
            *name = text.chars().filter_map(lospec_char).take(64).collect();
            return;
        }
        let clean: String = text.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == ' ').map(|c| if c == ' ' { '_' } else { c.to_ascii_uppercase() }).collect();
        if self.typing {
            self.filter = clean;
            self.scroll = 0.0;
        } else if let Some((_, name)) = &mut self.ask {
            *name = clean;
        }
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
        if let Some(m) = &self.map {
            self.map_buttons(m, &mut push);
            return b;
        }
        if let Some(d) = &self.data {
            self.data_buttons(d, &mut push);
            return b;
        }
        if self.is_card() {
            push(410.0, 8.0, 94.0, 34.0, Action::Stats, "Stats T", self.stats);
        } else if self.built_in_still().is_some() {
            push(410.0, 8.0, 94.0, 34.0, Action::Generate, "Generate", false);
        } else if self.data_link.is_some() {
            push(410.0, 8.0, 94.0, 34.0, Action::Data, "Data", false);
        }
        if !self.is_card() && !self.none_open() {
            let x = if self.built_in_still().is_some() || self.data_link.is_some() { 312.0 } else { 410.0 };
            push(x, 8.0, 94.0, 34.0, Action::Coats, "Coats", self.coats_page);
        }
        if self.coats_page && !self.none_open() {
            self.coat_buttons(&mut push);
        }
        if self.stats && self.is_player_card() {
            self.line_buttons(&mut push);
            let starts = self.player_starts();
            for (d, start) in starts.iter().enumerate() {
                let top = START_TOP + d as f32 * START_BLOCK;
                push(CANVAS.x + 400.0, top, 140.0, 26.0, Action::StartDefault(d), "Default", start.is_none());
                let have = start.clone().unwrap_or_else(|| last_harvest_game::default_start(last_harvest_game::Difficulty::ALL[d]));
                for (w, (id, label)) in cards::weapon_labels().into_iter().enumerate() {
                    let (col, row) = (w % START_ACROSS, w / START_ACROSS);
                    let on = have.contains(&id);
                    push(CANVAS.x + 20.0 + col as f32 * 104.0, top + 30.0 + row as f32 * START_ROW, 100.0, 22.0, Action::StartWeapon(d, w), &label, on);
                }
            }
            for i in 0..3 {
                let x = CANVAS.x + 20.0 + i as f32 * 180.0;
                push(x + 100.0, BASE_Y, 34.0, 26.0, Action::PlayerBase(i, -1), "-", false);
                push(x + 138.0, BASE_Y, 34.0, 26.0, Action::PlayerBase(i, 1), "+", false);
            }
        } else if self.stats {
            self.line_buttons(&mut push);
            for i in 0..STAT_ROWS.len() {
                let y = STAT_TOP + i as f32 * STAT_ROW;
                push(CANVAS.x + 400.0, y, 66.0, STAT_BUTTON, Action::StatStep(i, -1), "-", false);
                push(CANVAS.x + 474.0, y, 66.0, STAT_BUTTON, Action::StatStep(i, 1), "+", false);
            }
            let y = STAT_TOP + STAT_ROWS.len() as f32 * STAT_ROW;
            push(CANVAS.x + 400.0, y, 66.0, STAT_BUTTON, Action::Sound(-1), "<", false);
            push(CANVAS.x + 474.0, y, 66.0, STAT_BUTTON, Action::Sound(1), ">", false);
            if self.own_sound.is_some() {
                push(CANVAS.x + 548.0, y, 110.0, STAT_BUTTON, Action::DropSound, "Drop own", false);
            }
            let fields = self.sprite.card.as_deref().and_then(|t| last_harvest_game::card_fields(t).ok());
            for (i, name) in FLAG_NAMES.into_iter().enumerate() {
                let on = fields.as_ref().is_some_and(|f| f.flags[i].1);
                push(CANVAS.x + 20.0 + i as f32 * 176.0, y + 32.0, 168.0, 28.0, Action::Flag(i), name, on);
            }
        }
        let header = [(Action::Ask(Ask::New), "New"), (Action::Ask(Ask::Copy), "Copy"), (Action::History, "History R"), (Action::Revert, "Revert"), (Action::Save, "Save")];
        for (i, (a, l)) in header.into_iter().enumerate().filter(|(_, (a, _))| !(self.modding && *a == Action::History)) {
            let on = (a == Action::Save && self.unsaved()) || (a == Action::History && self.browse.is_some());
            push(512.0 + i as f32 * 102.0, 8.0, 94.0, 34.0, a, l, on);
        }
        if self.browse.is_some() {
            push(RESTORE.x, RESTORE.y, RESTORE.w, RESTORE.h, Action::Restore, "Restore", true);
            push(BACK.x, BACK.y, BACK.w, BACK.h, Action::Back, "Back", false);
        }
        let tools = [(Tool::Pencil, "Pencil B"), (Tool::Eraser, "Eraser E"), (Tool::Fill, "Fill G"), (Tool::Picker, "Pick I"), (Tool::Select, "Select S")];
        let cell = |i: usize| (SIDE_X + (i % 3) as f32 * 79.0, 52.0 + (i / 3) as f32 * 38.0);
        for (i, (t, l)) in tools.into_iter().enumerate() {
            let (x, y) = cell(i);
            push(x, y, 74.0, 32.0, Action::Tool(t), l, self.tool_now() == t);
        }
        let (x, y) = cell(5);
        push(x, y, 74.0, 32.0, Action::Mirror, "Mirror M", self.mirror);
        push(SIDE_X + 128.0, 192.0, 50.0, 26.0, Action::ClearColor, "Erase", false);
        if !self.is_card() {
            push(SIDE_X + 182.0, 192.0, 50.0, 26.0, Action::ClearIn3d, "No 3D", self.sprite.clear_in_3d.contains(&self.color));
            push(SIDE_X, 192.0, 56.0, 26.0, Action::PaintColor, "Paint", self.sprite.paint.colors.contains(&[self.color[0], self.color[1], self.color[2]]));
        }
        for (i, (t, l)) in [(Tool::Line, "Line"), (Tool::Rect, "Box"), (Tool::Ellipse, "Oval")].into_iter().enumerate() {
            let (x, y) = cell(6 + i);
            push(x, y, 74.0, 32.0, Action::Tool(t), l, self.tool_now() == t);
        }
        push(SIDE_X + 132.0, 289.0, 100.0, 24.0, Action::Ask(Ask::Palette), "Lospec", self.palette_name.is_some());
        let cols = (SIDE_W / SWATCH) as usize;
        for (i, &c) in self.palette.iter().enumerate().take(cols * if self.is_prop() { PROP_PALETTE_ROWS } else { 9 }) {
            let (x, y) = (SIDE_X + (i % cols) as f32 * SWATCH, 316.0 + (i / cols) as f32 * SWATCH);
            push(x, y, SWATCH - 2.0, SWATCH - 2.0, Action::Color(c), "", self.color == c);
        }
        if self.none_open() {
            return b;
        }
        self.prop_buttons(&mut push);
        if self.is_card() {
            push(SIDE_X, 566.0, 74.0, 30.0, Action::Strike(false), "Hurt", false);
            push(SIDE_X + 78.0, 566.0, 74.0, 30.0, Action::Strike(true), "Kill", false);
            if self.still_row().is_some() {
                push(SIDE_X, 600.0, 114.0, 30.0, Action::Generate, "Generate", false);
                push(SIDE_X + 118.0, 600.0, 114.0, 30.0, Action::ClearPart, "Clear", false);
            }
        }
        for (i, l) in STANDS.into_iter().enumerate().filter(|_| !self.is_card()) {
            push(SIDE_X + (i % 3) as f32 * 78.0, 566.0 + (i / 3) as f32 * 34.0, 74.0, 30.0, Action::Stand(i as u8), l, self.stand == i as u8);
        }
        if (self.stand < 2 || matches!(self.stand, CARVE | TURNED)) && !self.is_card() {
            push(SIDE_X + 130.0, 640.0, 46.0, 30.0, Action::Depth(-1), "-", false);
            push(SIDE_X + 182.0, 640.0, 46.0, 30.0, Action::Depth(1), "+", false);
        }
        if self.stand == 1 && !self.is_card() {
            push(SIDE_X + 130.0, 672.0, 46.0, 28.0, Action::Float(-1), "-", false);
            push(SIDE_X + 182.0, 672.0, 46.0, 28.0, Action::Float(1), "+", false);
        }
        push(SIDE_X + 130.0, 704.0, 46.0, 28.0, Action::Fps(-1), "-", false);
        push(SIDE_X + 182.0, 704.0, 46.0, 28.0, Action::Fps(1), "+", false);
        push(SIDE_X, 736.0, 114.0, 28.0, Action::Onion, "Onion O", self.onion);
        push(SIDE_X + 118.0, 736.0, 114.0, 28.0, Action::Compare, "Compare C", self.compare);
        let carving = self.stand == CARVE && self.sprite.views.is_some();
        let room = if carving { 2 } else if self.by_views() { 6 } else { 7 };
        let shown = self.sprite.frames.len().min(room);
        let first = self.frame.saturating_sub(room - 1).min(self.sprite.frames.len() - shown);
        for i in first..first + shown {
            push(CANVAS.x + (i - first) as f32 * (THUMB + 6.0), 622.0, THUMB, THUMB, Action::Frame(i), "", i == self.frame && self.view.is_none());
        }
        let after = CANVAS.x + shown as f32 * (THUMB + 6.0);
        push(after, 622.0, 44.0, THUMB, Action::AddFrame, "+", false);
        if self.stand == FIGURE && !self.is_card() {
            let action = if self.sprite.top.is_some() { Action::View(TOP) } else { Action::AddTop };
            push(after + 56.0, 622.0, THUMB, THUMB, action, "", self.view == Some(TOP));
        }
        if carving {
            // Narrower than a frame's, so all six fit beside two frames.
            let slot = 52.0;
            for (i, v) in [0, 1].into_iter().chain(CARVE_EXTRAS).enumerate() {
                let action = if v < 2 || optional(&self.sprite, v).is_some() { Action::View(v) } else { Action::AddView(v) };
                push(after + 56.0 + i as f32 * (slot + 4.0), 622.0, slot, THUMB, action, "", self.view == Some(v));
            }
        } else if self.by_views() {
            push(after + 56.0, 622.0, THUMB, THUMB, Action::View(1), "", self.view == Some(1));
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
        if self.browse.is_some() {
            let allowed = |a: &Action| matches!(a, Action::History | Action::Restore | Action::Back | Action::Frame(_) | Action::View(_) | Action::Play);
            if let Some(b) = self.buttons().into_iter().find(|b| b.rect.contains(p) && allowed(&b.action)) {
                self.act(b.action, shift);
            } else if LIST.contains(p) && p[1] >= HISTORY_TOP {
                let i = ((p[1] - HISTORY_TOP) / HISTORY_ROW) as usize;
                if self.browse.as_ref().is_some_and(|b| i < b.commits.len()) {
                    self.show_version(i);
                }
            }
            return;
        }
        if LIST.contains(p) {
            if p[1] < self.list_top() {
                self.typing = true;
                return;
            }
            let i = ((p[1] - self.list_top() + self.scroll) / LIST_ROW) as usize;
            match self.rows().get(i) {
                Some(Row::Heading { title, .. }) => {
                    let title = *title;
                    match self.unfolded.iter().position(|t| *t == title) {
                        Some(at) => _ = self.unfolded.remove(at),
                        None => self.unfolded.push(title),
                    }
                    self.wheel([LIST.x + 1.0, LIST.y + 1.0], 0.0);
                }
                Some(Row::Sprite((name, _))) => {
                    let name = name.clone();
                    self.switch_to(name);
                }
                None => {}
            }
            return;
        }
        if self.data.is_some() {
            if let Some(b) = self.buttons().into_iter().find(|b| b.rect.contains(p)) {
                self.act(b.action, shift);
            }
            return;
        }
        if self.map.is_some() {
            match self.buttons().into_iter().find(|b| b.rect.contains(p)) {
                Some(b) => self.act(b.action, shift),
                None => self.map_down(p, right, shift),
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
        if let Some((origins, zoom)) = self.compare_layout().filter(|_| CANVAS.contains(p)) {
            let hit = self.family.iter().zip(&origins).find(|((n, frames), o)| {
                let f = self.family_frame(n, frames);
                *n != self.sprite.name && (o[0]..o[0] + f.w as f32 * zoom).contains(&p[0]) && (o[1]..o[1] + f.h as f32 * zoom).contains(&p[1])
            });
            if let Some(((name, _), _)) = hit {
                let name = name.clone();
                self.switch_to(name);
                return;
            }
        }
        if CANVAS.contains(p) && !self.stats && !self.coats_page && !self.none_open() {
            let (x, y) = self.pixel_at(p);
            if self.tool_now() == Tool::Select && !right {
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
            if self.tool_now().shapes() && !right {
                self.remember();
                self.drawing = Some(Drawing { from: (x, y), base: self.current().clone(), shift });
                self.draw_shape((x, y));
                return;
            }
            if right || self.tool_now() == Tool::Picker {
                if let Some(c) = self.current().get(x, y).filter(|c| c[3] > 0) {
                    self.color = c;
                    if !right && self.tool == Tool::Picker {
                        self.tool = self.before_pick;
                    }
                }
                return;
            }
            self.remember();
            match self.last_dot.filter(|_| shift && matches!(self.tool, Tool::Pencil | Tool::Eraser)) {
                Some(from) => self.line(from, (x, y)),
                None => self.paint(x, y),
            }
            self.stroke = Some((x, y));
        }
    }

    pub fn drag(&mut self, p: [f32; 2]) {
        self.hover = Some(p);
        if self.map.is_some() {
            return self.map_drag(p);
        }
        if self.data.is_some() {
            return;
        }
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
        if self.drawing.is_some() {
            let to = self.pixel_at(p);
            self.draw_shape(to);
            return;
        }
        let Some((lx, ly)) = self.stroke else { return };
        if matches!(self.tool_now(), Tool::Fill | Tool::Picker | Tool::Select) {
            return;
        }
        let (x, y) = self.pixel_at(p);
        // Every pixel on the line from the last one, so a quick drag leaves no gaps.
        self.line((lx, ly), (x, y));
        self.stroke = Some((x, y));
    }

    pub fn mouse_up(&mut self) {
        self.map_up();
        self.stroke = None;
        self.drawing = None;
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
        if self.none_open() {
            return;
        }
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

    /// Clears the paint color from every frame, or from the carving view on the canvas.
    fn clear_color(&mut self) {
        let c = self.color;
        let hex = sprite::hex(c);
        let in_view = self.view.is_some();
        let count = |f: &Frame| f.px.iter().filter(|&&p| p == c).count();
        let n = if in_view { count(self.current()) } else { self.sprite.frames.iter().map(count).sum() };
        if n == 0 {
            return self.say(format!("No {hex} pixels to clear"));
        }
        self.remember();
        let frames = if in_view { std::slice::from_mut(self.current_mut()) } else { &mut self.sprite.frames[..] };
        frames.iter_mut().flat_map(|f| f.px.iter_mut()).filter(|p| **p == c).for_each(|p| *p = CLEAR);
        self.say(format!("Cleared {n} {hex} pixels"));
    }

    /// Leaves the paint color out of the 3D view, keeping it in 2D, or puts it back.
    fn clear_in_3d(&mut self) {
        let (c, list) = (self.color, &mut self.sprite.clear_in_3d);
        let gone = match list.iter().position(|&k| k == c) {
            Some(i) => {
                list.remove(i);
                false
            }
            None => {
                list.push(c);
                true
            }
        };
        let n = list.len();
        self.dirty = true;
        self.changed = true;
        self.say(format!("{} {} the 3D view; {n} color{} cleared in 3D", sprite::hex(c), if gone { "left out of" } else { "back in" }, if n == 1 { "" } else { "s" }));
    }

    fn paste(&mut self) {
        if self.none_open() {
            return self.say("Pick a sprite from the list, or press N for a new one");
        }
        let Some((pixels, x, y)) = self.clipboard.clone() else {
            self.say("Nothing copied yet");
            return;
        };
        self.deselect();
        self.remember();
        let f = self.current();
        // Where it was copied from, pulled in to the nearest edge; one too big for the surface lines up with its far edge.
        let inside = |v: i32, size: usize, max: usize| {
            let room = max as i32 - size as i32;
            v.min(room).max(room.min(0))
        };
        let (x, y) = (inside(x, pixels.w, f.w), inside(y, pixels.h, f.h));
        self.selection = Some(Selection { x, y, w: pixels.w, h: pixels.h, float: Some(pixels) });
        self.tool = Tool::Select;
        self.say("Pasted: drag or use the arrow keys to line it up, Enter to place");
    }

    pub fn wheel(&mut self, p: [f32; 2], dy: f32) {
        if self.map.is_some() && !LIST.contains(p) {
            return self.map_wheel(p, dy);
        }
        if self.data.is_some() && !LIST.contains(p) {
            if dy != 0.0 {
                self.act(Action::DataPage(if dy > 0.0 { -1 } else { 1 }), false);
            }
            return;
        }
        if LIST.contains(p) {
            let max = (self.rows().len() as f32 * LIST_ROW - (LIST.h - (self.list_top() - LIST.y))).max(0.0);
            self.scroll = (self.scroll - dy * LIST_ROW * 3.0).clamp(0.0, max);
        }
    }

    /// Paints every pixel from `a` to `b`, leaving out `a` itself, which is already painted.
    fn line(&mut self, (ax, ay): (i32, i32), (bx, by): (i32, i32)) {
        let steps = (bx - ax).abs().max((by - ay).abs()).max(1);
        for k in 1..=steps {
            let t = k as f32 / steps as f32;
            self.paint(ax + ((bx - ax) as f32 * t).round() as i32, ay + ((by - ay) as f32 * t).round() as i32);
        }
    }

    /// Redraws the shape being dragged out, from where it started to `to`, over the surface as it was.
    fn draw_shape(&mut self, (bx, by): (i32, i32)) {
        let Some(d) = self.drawing.clone() else { return };
        *self.current_mut() = d.base;
        let (ax, ay) = d.from;
        match self.tool_now() {
            Tool::Line => {
                let (mut dx, mut dy) = (bx - ax, by - ay);
                if d.shift {
                    // Straight: across, down, or at 45 degrees, whichever the drag is nearest.
                    if dx.abs() > 2 * dy.abs() {
                        dy = 0;
                    } else if dy.abs() > 2 * dx.abs() {
                        dx = 0;
                    } else {
                        let n = dx.abs().max(dy.abs());
                        (dx, dy) = (n * dx.signum(), n * dy.signum());
                    }
                }
                self.paint(ax, ay);
                self.line((ax, ay), (ax + dx, ay + dy));
            }
            Tool::Rect | Tool::Ellipse => {
                let (x0, x1, y0, y1) = (ax.min(bx), ax.max(bx), ay.min(by), ay.max(by));
                let inside = |x: i32, y: i32| {
                    if self.tool_now() == Tool::Rect {
                        return (x0..=x1).contains(&x) && (y0..=y1).contains(&y);
                    }
                    let (rx, ry) = ((x1 - x0 + 1) as f32 / 2.0, (y1 - y0 + 1) as f32 / 2.0);
                    let (cx, cy) = (x0 as f32 + rx, y0 as f32 + ry);
                    let (u, v) = ((x as f32 + 0.5 - cx) / rx, (y as f32 + 0.5 - cy) / ry);
                    u * u + v * v <= 1.0
                };
                let edge = |x: i32, y: i32| [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(ox, oy)| !inside(x + ox, y + oy));
                let dots: Vec<(i32, i32)> = (y0..=y1).flat_map(|y| (x0..=x1).map(move |x| (x, y))).filter(|&(x, y)| inside(x, y) && (d.shift || edge(x, y))).collect();
                for (x, y) in dots {
                    self.paint(x, y);
                }
            }
            _ => {}
        }
        self.changed = true;
    }

    fn paint(&mut self, x: i32, y: i32) {
        self.last_dot = Some((x, y));
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
        let any_sprite = matches!(a, Action::Ask(Ask::New) | Action::Tool(_) | Action::Color(_) | Action::Mirror | Action::Onion | Action::Fps(_));
        if self.none_open() && !any_sprite {
            return self.say("Pick a sprite from the list, or press N for a new one");
        }
        if self.map.is_some() && self.map_act(a, shift) {
            return;
        }
        if self.data.is_some() && self.data_act(a, shift) {
            return;
        }
        match a {
            Action::MapTool(_) | Action::Ground(_) | Action::PickProp(_) | Action::PickPage(_) | Action::Picket(_) | Action::MapSize(..) | Action::Randomize | Action::MapFit | Action::PropDefaults => {}
            Action::DataStep(..) | Action::DataEdit(_) | Action::DataRemove(_) | Action::DataAdd | Action::DataPage(_) => {}
            Action::LineFrame(_) | Action::LineFps(_) => {}
            Action::PlayerBase(i, d) => {
                let t = last_harvest_game::tuning::tuning();
                let own = [t.player_hp, t.player_speed, t.player_magnet][i];
                let now = last_harvest_game::player_fields(self.sprite.card.as_deref().unwrap_or("")).ok().and_then(|f| f.base[i]).unwrap_or(own);
                let next = (now + BASE_STEPS[i] * (d * step) as f32).clamp(1.0, 1000.0);
                let value = (next != own).then(|| next.to_string());
                self.set_card(last_harvest_game::PLAYER_BASE_KEYS[i], value.as_deref());
            }
            Action::Data => {
                if let Some(name) = self.data_link.clone() {
                    self.switch_to(name);
                }
            }
            Action::Cover => self.edit_prop(|f| f.cover = !f.cover),
            Action::Open(d) => self.edit_prop(|f| f.open = (f.open as i32 + d * step).clamp(0, 64) as usize),
            Action::Hp(d) => self.edit_prop(|f| f.hp = (f.hp + (d * step * 10) as f32).clamp(0.0, last_harvest_game::PROP_MAX_HP)),
            Action::Broken if self.sprite.frames.len() < 2 => self.say("Add a frame first: the last one shows once the prop breaks"),
            Action::Broken => self.edit_prop(|f| f.broken = !f.broken),
            Action::Ask(k) => self.ask = Some((k, String::new())),
            Action::Save if self.browse.is_some() => self.say("Restore this version or go back first"),
            Action::Save => {
                self.deselect();
                let new = !self.names.iter().any(|(n, _)| *n == self.sprite.name);
                // A carving lives in the sprite's folder, so it saves the pixels too.
                let carving = matches!(self.stand, CARVE | TURNED) && self.stand_touched;
                let pixels = if self.dirty || new || carving { self.store.save(&self.sprite) } else { Ok(()) };
                let stand = if self.stand_touched { self.store.save_stand(&self.sprite.name, &self.stand_text()) } else { Ok(()) };
                match pixels.and(stand) {
                    Ok(()) => {
                        self.dirty = false;
                        self.stand_touched = false;
                        self.names = self.all_names();
                        self.load_card_parts();
                        self.changed = true;
                        self.reveal();
                        let next = if self.is_card() { "Restart the game to load it." } else { "Rebuild the game to see it in play." };
                        self.say(format!("Saved {}. {next}", self.sprite.name));
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
                if t == Tool::Picker && self.tool != Tool::Picker {
                    self.before_pick = self.tool;
                }
                self.tool = t;
                match t {
                    Tool::Line => self.say("Drag to draw a line; hold Shift as you start to keep it straight"),
                    Tool::Rect | Tool::Ellipse => self.say("Drag from corner to corner; hold Shift as you start to fill it"),
                    _ => {}
                }
            }
            Action::Mirror => self.mirror = !self.mirror,
            Action::Onion => self.onion = !self.onion,
            Action::Compare => {
                self.compare = !self.compare;
                self.load_family();
            }
            Action::AddFrame | Action::DeleteFrame | Action::MoveFrame(_) if self.view.is_none() && self.frames_refused(a) => {}
            Action::Stand(_) if self.is_card() => self.say("A card stands as a figure"),
            Action::Strike(kill) => self.strike = Some(kill),
            Action::Generate => self.generate_part(),
            Action::ClearPart => self.clear_part(),
            Action::ClearColor => self.clear_color(),
            Action::ClearIn3d if self.is_card() => self.say("A card stands as a figure, which keeps every color"),
            Action::ClearIn3d => self.clear_in_3d(),
            Action::Coats => {
                self.coats_page = !self.coats_page;
                self.changed = true;
            }
            Action::PaintColor => self.toggle_paint(),
            Action::Coat(k) => {
                self.coat = k;
                self.changed = true;
            }
            Action::Shade(k, j) => self.set_shade(k, j),
            Action::DropCoat(k) => self.drop_coat(k),
            Action::AddCoat => self.add_coat(),
            Action::Stats => {
                self.stats = !self.stats;
                self.naming = None;
                self.load_other_cards();
            }
            Action::StatStep(i, d) => {
                let Some(f) = self.sprite.card.as_deref().and_then(|t| last_harvest_game::card_fields(t).ok()) else { return };
                let (key, value) = f.numbers[i];
                let (_, step) = STAT_ROWS[i];
                let bigger = if shift { 10.0 } else { 1.0 };
                let lo = if matches!(key, "mass" | "charge") { 0.1 } else { 0.0 };
                let mut v = ((value + step * bigger * d as f32) / step).round() * step;
                v = v.max(lo);
                if key == "chance" {
                    v = v.min(1.0);
                }
                let text = format!("{}", (v * 1000.0).round() / 1000.0);
                self.set_card(key, Some(&text));
            }
            Action::Flag(i) => {
                let Some(f) = self.sprite.card.as_deref().and_then(|t| last_harvest_game::card_fields(t).ok()) else { return };
                let (key, on) = f.flags[i];
                self.set_card(key, if on { None } else { Some("") });
            }
            Action::Sound(d) => {
                let Some(f) = self.sprite.card.as_deref().and_then(|t| last_harvest_game::card_fields(t).ok()) else { return };
                let mut sounds: Vec<String> = last_harvest_game::kill_sounds().map(str::to_string).collect();
                sounds.dedup();
                if !sounds.contains(&f.sound) {
                    sounds.push(f.sound.clone());
                }
                let at = sounds.iter().position(|s| *s == f.sound).unwrap_or(0) as i32;
                let next = sounds[(at + d).rem_euclid(sounds.len() as i32) as usize].clone();
                self.set_card("sound", Some(&next));
            }
            Action::DropSound => self.drop_sound(),
            Action::StartWeapon(d, w) => {
                let difficulty = last_harvest_game::Difficulty::ALL[d];
                let mut have = self.player_starts()[d].clone().unwrap_or_else(|| last_harvest_game::default_start(difficulty));
                let Some((id, _)) = last_harvest_game::weapons().into_iter().nth(w) else { return };
                match have.iter().position(|h| *h == id) {
                    Some(at) => _ = have.remove(at),
                    None if have.len() >= last_harvest_game::weapon_slots() => return self.say(format!("A run carries at most {} weapons", last_harvest_game::weapon_slots())),
                    None => have.push(id),
                }
                self.set_start(difficulty, Some(&have));
            }
            Action::StartDefault(d) => self.set_start(last_harvest_game::Difficulty::ALL[d], None),
            Action::EditName => self.edit_line(Line::Name),
            Action::EditAbout => self.edit_line(Line::About),
            Action::AddFrame => {
                self.remember();
                let copy = self.current().clone();
                self.sprite.frames.insert(self.frame + 1, copy);
                self.frame += 1;
            }
            Action::DeleteFrame if self.view == Some(TOP) => {
                self.remember();
                self.sprite.top = None;
                self.view = None;
                self.say("Top removed: the 3D view picks the top colors itself again");
            }
            Action::DeleteFrame if self.view.is_some_and(|v| CARVE_EXTRAS.contains(&v)) => {
                let v = self.view.take().unwrap();
                self.remember();
                *optional_mut(&mut self.sprite, v) = None;
                self.say(match v {
                    FOOTPRINT => "Footprint removed: the sprite's own outline shapes the solid again",
                    ROOF => "Top removed: the sprite colors the top again",
                    NORTH => "North view removed: the south view colors the north side again",
                    _ => "West view removed: the east view colors the west side again",
                });
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
                self.last_dot = None;
                self.frame = i;
                self.view = None;
                self.playing = false;
                self.changed = true;
            }
            Action::History if self.browse.is_some() => self.leave_history(),
            Action::History => self.open_history(),
            Action::Restore => self.restore_version(),
            Action::Back => self.leave_history(),
            Action::AddTop => {
                self.deselect();
                self.remember();
                self.sprite.top = Some(sprite::top_strip(&self.sprite.frames[self.frame]));
                self.view = Some(TOP);
                self.say("Paint each column's top color for 3D");
            }
            Action::AddView(v) => {
                let Some([south, east]) = self.sprite.views.clone() else { return };
                self.deselect();
                self.remember();
                let frame = self.sprite.frames[self.frame].clone();
                let (start, hint) = match v {
                    FOOTPRINT => (frame, "Erase what isn't solid from above"),
                    ROOF => (frame, "Paint the top's colors; what's clear keeps the sprite's"),
                    NORTH => (south.mirrored(), "Paint the north side, east on the left; what's clear keeps the south view's"),
                    _ => (east.mirrored(), "Paint the west side, north on the left; what's clear keeps the east view's"),
                };
                *optional_mut(&mut self.sprite, v) = Some(start);
                self.view = Some(v);
                self.say(hint);
            }
            Action::View(v) => {
                self.deselect();
                self.last_dot = None;
                self.view = Some(v);
                self.playing = false;
            }
            Action::Play => {
                self.playing = !self.playing;
                self.changed = true;
            }
            Action::Width(_) if self.view.is_some() => self.say("A view is as wide as the sprite: resize the frames instead"),
            Action::Height(_) if self.view == Some(TOP) => self.say("The top is one row: resize the frames instead"),
            Action::Height(_) if self.view == Some(FOOTPRINT) => self.say("The footprint is the sprite's size: resize the frames instead"),
            Action::Height(_) if self.view == Some(ROOF) => self.say("The top is the sprite's size: resize the frames instead"),
            Action::Height(d) if self.view.is_some() => self.resize_views(self.view_height() as i32 + d * step),
            Action::Width(d) | Action::Height(d) => {
                let top = &self.sprite.frames[self.frame];
                let (w, h) = (top.w as i32, top.h as i32);
                let (w, h) = if matches!(a, Action::Width(_)) { (w + d * step, h) } else { (w, h + d * step) };
                let max = if self.is_card() { sprite::CARD_MAX_FRAME } else { MAX_SIZE } as i32;
                if (w > max || h > max) && self.is_card() {
                    self.say(format!("A card's frames are at most {max}x{max} pixels"));
                }
                let (w, h) = (w.clamp(1, max) as usize, h.clamp(1, max) as usize);
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
                for f in [&mut self.sprite.footprint, &mut self.sprite.roof].into_iter().flatten() {
                    *f = f.resized(w, h);
                }
                if let Some(f) = &mut self.sprite.north {
                    *f = f.resized(w, f.h);
                }
                if let Some(f) = &mut self.sprite.west {
                    *f = f.resized(h, f.h);
                }
                if let Some(t) = &mut self.sprite.top {
                    *t = t.resized(w, 1);
                }
            }
            Action::Stand(s) => {
                self.stand = s;
                self.stand_touched = true;
                self.changed = true;
                if matches!(s, CARVE | TURNED) && self.sprite.views.is_none() {
                    self.remember();
                    self.sprite.views = Some(sprite::block_views(&self.sprite.frames[self.frame], self.height as usize));
                }
                match s {
                    CARVE => self.say("Carve: erase the south and east views to shape the solid"),
                    TURNED => {
                        self.depth = self.outline_width();
                        self.view = Some(1);
                        self.say("Turned: draw it seen from the east, south on the left; it stands where the frame's outline is");
                    }
                    _ => self.view = None,
                }
            }
            Action::Depth(d) if self.stand == CARVE => self.resize_views(self.view_height() as i32 + d * step),
            Action::Depth(d) => {
                let size = if matches!(self.stand, 0 | TURNED) { &mut self.depth } else { &mut self.height };
                *size = (*size + (d * step) as f32).clamp(1.0, if self.stand == TURNED { 64.0 } else { 96.0 });
                self.stand_touched = true;
                self.changed = true;
            }
            Action::Float(d) => {
                self.lift = (self.lift + (d * step) as f32).clamp(0.0, 96.0);
                self.stand_touched = true;
                self.changed = true;
            }
            Action::Fps(d) => {
                self.fps = (self.fps + d as f32).clamp(1.0, 30.0);
                self.changed = true;
                if let Some(mut f) = self.sprite.card.as_deref().filter(|_| crate::store::props::id(&self.sprite.name).is_some()).and_then(|t| last_harvest_game::prop_fields(t).ok()) {
                    f.fps = self.fps;
                    self.sprite.card = Some(last_harvest_game::prop_text(&f));
                    self.dirty = true;
                }
            }
            Action::Color(c) => {
                self.color = c;
                if self.tool == Tool::Eraser {
                    self.tool = Tool::Pencil;
                }
            }
        }
    }

    fn stash(&self) -> Stash {
        Stash {
            sprite: self.sprite.clone(),
            stand: self.stand,
            depth: self.depth,
            height: self.height,
            lift: self.lift,
            frame: self.frame,
            view: self.view,
            dirty: self.dirty,
            stand_touched: self.stand_touched,
        }
    }

    fn unstash(&mut self, s: Stash) {
        (self.sprite, self.stand, self.depth, self.height, self.lift, self.frame, self.view) = (s.sprite, s.stand, s.depth, s.height, s.lift, s.frame, s.view);
        (self.dirty, self.stand_touched) = (s.dirty, s.stand_touched);
        self.changed = true;
    }

    /// Starts browsing the sprite's committed versions, showing the newest.
    fn open_history(&mut self) {
        let Some((commits, changed)) = self.store.history(&self.sprite.name) else {
            self.say(if self.modding { "The card editor keeps no history; the sprite editor reads it from git" } else { "Its folder isn't in a git repository" });
            return;
        };
        if commits.is_empty() {
            self.say(format!("{} has no commits yet", self.sprite.name));
            return;
        }
        self.deselect();
        self.playing = false;
        self.browse = Some(Browse { commits, at: 0, changed, before: self.stash() });
        self.show_version(0);
    }

    /// Puts the sprite as it was at the `i`th commit on show, read-only.
    fn show_version(&mut self, i: usize) {
        let Some(b) = &mut self.browse else { return };
        b.at = i.min(b.commits.len() - 1);
        let (id, name) = (b.commits[b.at].id.clone(), self.sprite.name.clone());
        let loaded = Sprite::load_from(&name, |file| self.store.version(&id, &name, file));
        let stand = self.store.version(&id, &name, "stand.txt").map(|t| String::from_utf8_lossy(&t).trim().to_string());
        if let Some(s) = loaded {
            self.sprite = s;
            self.frame = 0;
            self.view = None;
            let stand = stand.or_else(|| self.store.stand(&name));
            self.apply_stand(stand);
            self.changed = true;
        }
    }

    /// Keeps the version on show as the sprite, unsaved; undo goes back to what was open before.
    fn restore_version(&mut self) {
        let Some(b) = self.browse.take() else { return };
        let before = b.before.sprite;
        self.undo.push(snapshot_of(&before));
        self.redo.clear();
        self.dirty = true;
        self.stand_touched = true;
        let c = &b.commits[b.at];
        self.say(format!("Restored {} from {}: Save to keep it", c.id, c.date));
    }

    fn leave_history(&mut self) {
        if let Some(b) = self.browse.take() {
            self.unstash(b.before);
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
        let Some([south, east]) = &mut self.sprite.views else { return };
        let sides = [Some(south), Some(east), self.sprite.north.as_mut(), self.sprite.west.as_mut()];
        for (v, kept) in sides.into_iter().zip(&mut self.trimmed) {
            let Some(v) = v else { continue };
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
        let held = self.held_import.take();
        match choice {
            Choice::Save => {
                self.act(Action::Save, false);
                if self.unsaved() {
                    return;
                }
            }
            Choice::Discard => {
                self.dirty = false;
                self.stand_touched = false;
            }
            Choice::Stay => return,
        }
        match held {
            Some(HeldImport { source, cards, sounds }) => self.import_pack(&source, cards, sounds),
            None => {
                self.open(&name);
                self.reveal();
            }
        }
    }

    /// The sprites the list shows: all of them, or those matching the search.
    fn listed(&self) -> Vec<&(String, bool)> {
        let want = self.filter.to_ascii_uppercase();
        self.names.iter().filter(|(n, _)| n.contains(&want)).collect()
    }

    /// The list's rows: a heading over each group, nested by `/` in its title, then its sprites if every heading
    /// over them is unfolded. A search unfolds every group with a match.
    fn rows(&self) -> Vec<Row<'_>> {
        let searching = !self.filter.is_empty();
        let mut rows = Vec::new();
        let listed = self.listed();
        for (i, entry) in listed.iter().enumerate() {
            let title = sprite::section(&entry.0);
            let before = i.checked_sub(1).map(|b| sprite::section(&listed[b].0));
            let mut shown = true;
            for (depth, path) in sprite::section_path(title).enumerate() {
                let open = searching || self.unfolded.contains(&path);
                if !before.is_some_and(|b| sprite::within(b, path)) {
                    let group = listed[i..].iter().take_while(|e| sprite::within(sprite::section(&e.0), path));
                    let (count, saved) = group.fold((0, 0), |(n, s), e| (n + 1, s + e.1 as usize));
                    rows.push(Row::Heading { title: path, depth, count, saved, open });
                }
                if !open {
                    shown = false;
                    break;
                }
            }
            if shown {
                rows.push(Row::Sprite(entry));
            }
        }
        rows
    }

    /// Where the list's rows start: below the search box when there is one.
    fn list_top(&self) -> f32 {
        if self.typing || !self.filter.is_empty() { LIST.y + LIST_ROW + 6.0 } else { LIST.y }
    }

    /// Steps `by` places through the sprites the list shows.
    fn step_sprite(&mut self, by: i32) {
        let listed = self.listed();
        let n = listed.len() as i32;
        let at = listed.iter().position(|(m, _)| *m == self.sprite.name).map_or(if by > 0 { -1 } else { 0 }, |i| i as i32);
        let next = (at + by).rem_euclid(n.max(1)) as usize;
        if let Some((name, _)) = listed.get(next).map(|e| (*e).clone()) {
            self.switch_to(name);
        }
    }

    /// The tool that acts now: the picker while a key holds it, else the one chosen.
    fn tool_now(&self) -> Tool {
        if self.peek { Tool::Picker } else { self.tool }
    }

    /// Holds the color picker in place of the tool, until `on` goes false.
    pub fn peek(&mut self, on: bool) {
        self.peek = on;
    }

    /// Whether keys are typing a name or a search, so letters shouldn't act as shortcuts.
    pub fn wants_text(&self) -> bool {
        self.typing || self.ask.is_some() || self.naming.is_some()
    }

    /// Starts typing the card's `line`, or keeps what's typed when it's already being typed, as Enter does.
    fn edit_line(&mut self, line: Line) {
        match self.naming.take() {
            Some((was, text)) => {
                self.commit_line(was, text);
                if was != line {
                    self.naming = self.card_line(line).map(|t| (line, t));
                }
            }
            None => self.naming = self.card_line(line).map(|t| (line, t)),
        }
    }

    fn commit_line(&mut self, line: Line, text: String) {
        let text = text.trim();
        match line {
            Line::Value | Line::Added => self.commit_data(line, text),
            Line::Name if text.is_empty() => self.say("A card needs a name"),
            Line::About if text.is_empty() => self.set_card(line.key(), None),
            _ => self.set_card(line.key(), Some(text)),
        }
    }

    /// Unfolds the open sprite's group and scrolls the list so the sprite shows.
    fn reveal(&mut self) {
        if self.none_open() {
            return;
        }
        for title in sprite::section_path(sprite::section(&self.sprite.name)) {
            if !self.unfolded.contains(&title) {
                self.unfolded.push(title);
            }
        }
        let Some(i) = self.rows().iter().position(|r| matches!(r, Row::Sprite((m, _)) if *m == self.sprite.name)) else { return };
        let (y, room) = (i as f32 * LIST_ROW, LIST.h - (self.list_top() - LIST.y));
        if y < self.scroll {
            self.scroll = y;
        } else if y + LIST_ROW > self.scroll + room {
            self.scroll = y + LIST_ROW - room;
        }
    }

    fn step_frame(&mut self, by: i32) {
        let n = self.sprite.frames.len() as i32;
        self.act(Action::Frame((self.frame as i32 + by).rem_euclid(n) as usize), false);
    }

    pub fn key(&mut self, key: Key) {
        if let Some(b) = &self.browse {
            let (at, n) = (b.at, b.commits.len());
            match key {
                Key::Char('j') => self.show_version((at + 1).min(n - 1)),
                Key::Char('k') => self.show_version(at.saturating_sub(1)),
                Key::Enter => self.restore_version(),
                Key::Escape | Key::Char('r') => self.leave_history(),
                Key::Left | Key::Char('h') => self.step_frame(-1),
                Key::Right | Key::Char('l') => self.step_frame(1),
                Key::Char(' ') => self.act(Action::Play, false),
                _ => {}
            }
            return;
        }
        if self.pending.is_some() {
            match key {
                Key::Char('s') | Key::Enter => self.decide(Choice::Save),
                Key::Char('d') => self.decide(Choice::Discard),
                Key::Escape | Key::Char('c') => self.decide(Choice::Stay),
                _ => {}
            }
            return;
        }
        if let Some((line, name)) = &mut self.naming {
            match key {
                Key::Char(c) if line.takes(c) && name.chars().count() < line.max() => name.push(c),
                Key::Backspace => {
                    name.pop();
                }
                Key::Escape => self.naming = None,
                Key::Enter => {
                    if let Some((line, text)) = self.naming.take() {
                        self.commit_line(line, text);
                    }
                }
                _ => {}
            }
            return;
        }
        if self.typing {
            match key {
                Key::Char(c) if c.is_ascii_alphanumeric() || c == '_' || c == ' ' => {
                    self.filter.push(if c == ' ' { '_' } else { c.to_ascii_uppercase() });
                    self.scroll = 0.0;
                }
                Key::Backspace => {
                    self.filter.pop();
                }
                Key::Escape => {
                    self.typing = false;
                    self.filter.clear();
                    self.reveal();
                }
                Key::Enter => {
                    self.typing = false;
                    let open_one = !self.listed().iter().any(|(n, _)| *n == self.sprite.name);
                    if let Some((name, _)) = self.listed().first().map(|e| (*e).clone()).filter(|_| open_one) {
                        self.switch_to(name);
                    }
                    self.reveal();
                }
                _ => {}
            }
            return;
        }
        if let Some((Ask::Palette, name)) = &mut self.ask {
            match key {
                Key::Char(c) => name.extend(lospec_char(c).filter(|_| name.len() < 64)),
                Key::Backspace => _ = name.pop(),
                Key::Escape => self.ask = None,
                Key::Enter => {
                    let name = name.clone();
                    self.ask = None;
                    self.load_lospec(&name);
                }
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
                    let typed = || match name.strip_prefix("PLAYER_") {
                        Some(id) => (sprite::CardKind::Player, id.to_ascii_lowercase()),
                        None => (sprite::CardKind::Enemy, name.to_ascii_lowercase()),
                    };
                    let prop = crate::store::props::id(&name).is_some();
                    let wanted = sprite::card_wanted(&name).or_else(|| (self.modding && !name.is_empty() && !prop).then(typed));
                    if name.starts_with(sprite::MAP_PREFIX) && kind == Ask::New {
                        self.make_map(&name);
                    } else if let Some((card, id)) = wanted {
                        self.make_card(kind, card, &id);
                    } else if !sprite::valid_name(&name) {
                        self.say("Names start with a letter and use A-Z, 0-9 and _");
                    } else if self.names.iter().any(|(n, _)| *n == name) {
                        self.say(format!("{name} already exists"));
                    } else {
                        self.ask = None;
                        let pictures = if kind == Ask::Copy { self.snapshot() } else { (vec![Frame::blank(16, 16)], None, Default::default()) };
                        let card = crate::store::props::id(&name).map(|id| last_harvest_game::prop_text(&crate::store::props::blank(&id)));
                        let clear_in_3d = if kind == Ask::Copy { self.sprite.clear_in_3d.clone() } else { Vec::new() };
                        let paint = if kind == Ask::Copy { self.sprite.paint.clone() } else { Default::default() };
                        self.sprite = Sprite { name: name.clone(), frames: Vec::new(), views: None, footprint: None, roof: None, north: None, west: None, top: None, card, clear_in_3d, paint };
                        restore(&mut self.sprite, pictures);
                        self.fit();
                        self.view = None;
                        self.frame = 0;
                        self.undo.clear();
                        self.redo.clear();
                        self.dirty = true;
                        self.changed = true;
                        if !self.names.iter().any(|(n, _)| *n == name) {
                            self.names.push((name.clone(), false));
                        }
                        self.reveal();
                        self.say(format!("{name} is new: Save writes it to disk"));
                    }
                }
                _ => {}
            }
            return;
        }
        if self.map.is_some() && self.map_key(key) {
            return;
        }
        if self.data_key(key) {
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
            Key::Char('r') => self.open_history(),
            Key::Char('/') => {
                self.typing = true;
                self.filter.clear();
            }
            Key::Escape if !self.filter.is_empty() => {
                self.filter.clear();
                self.reveal();
            }
            Key::Char('s') => self.act(Action::Tool(Tool::Select), false),
            Key::Char('m') => self.mirror = !self.mirror,
            Key::Char('o') => self.act(Action::Onion, false),
            Key::Char('c') => self.act(Action::Compare, false),
            Key::Char('t') if self.is_card() => self.act(Action::Stats, false),
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
                    restore(&mut self.sprite, prev);
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
                    restore(&mut self.sprite, next);
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
        if let Some(m) = &self.map {
            return self.draw_map(c, m);
        }
        if let Some(d) = &self.data {
            return self.draw_data(c, d);
        }
        if self.none_open() {
            c.label("No sprite open", 12.0, 12.0, 26.0, DIM, Align::Left);
        } else {
            c.label(format!("{}{}", self.sprite.name, if self.unsaved() { "  (unsaved)" } else { "" }), 12.0, 12.0, 26.0, TEXT, Align::Left);
        }

        c.rect(LIST.x, LIST.y, LIST.w, LIST.h, PANEL);
        if let Some(b) = &self.browse {
            self.draw_history(c, b);
        } else {
            self.draw_list(c);
        }

        if self.none_open() {
            c.rect(CANVAS.x, CANVAS.y, CANVAS.w, CANVAS.h, PANEL);
            let (big, small) = if self.modding {
                ("Make a card", ["New starts one from a plain figure, PLAYER_ for a player card;", "Import opens a card or a .pak"])
            } else {
                ("Pick a sprite from the list", ["or press N for a new one;", "a name starting CARD_ makes an enemy card"])
            };
            c.label(big, CANVAS.x + CANVAS.w / 2.0, CANVAS.y + CANVAS.h / 2.0 - 40.0, 26.0, TEXT, Align::Center);
            for (i, line) in small.into_iter().enumerate() {
                c.label(line, CANVAS.x + CANVAS.w / 2.0, CANVAS.y + CANVAS.h / 2.0 + i as f32 * 24.0, 17.0, DIM, Align::Center);
            }
        } else if self.stats {
            self.draw_stats(c);
        } else if self.coats_page {
            self.draw_coats(c);
        } else {
            self.draw_canvas(c);
        }
        if let Some(b) = &self.browse {
            let commit = &b.commits[b.at];
            c.rect(CANVAS.x, CANVAS.y, CANVAS.w, 30.0, [0.0, 0.0, 0.0, 0.7]);
            let line: String = format!("Viewing {}  {}  {}", commit.id, commit.date, commit.subject).chars().take(62).collect();
            c.label(line, CANVAS.x + 8.0, CANVAS.y + 6.0, 16.0, HOT, Align::Left);
        }
        self.draw_controls(c);
    }

    fn draw_history(&self, c: &mut Canvas, b: &Browse) {
        c.label(format!("{} history", self.sprite.name), LIST.x + 8.0, LIST.y + 6.0, 17.0, HOT, Align::Left);
        let note = if b.changed { "Saved changes aren't committed" } else { "J/K: browse   Enter: restore" };
        c.label(note, LIST.x + 8.0, LIST.y + 28.0, 13.0, DIM, Align::Left);
        let rows = ((RESTORE.y - HISTORY_TOP - 8.0) / HISTORY_ROW) as usize;
        let first = b.at.saturating_sub(rows.saturating_sub(1));
        for (i, commit) in b.commits.iter().enumerate().skip(first).take(rows) {
            let y = HISTORY_TOP + (i - first) as f32 * HISTORY_ROW;
            if i == b.at {
                c.rect(LIST.x, y, LIST.w, HISTORY_ROW - 2.0, [0.35, 0.3, 0.18, 1.0]);
            }
            c.label(format!("{}  {}", commit.date, commit.id), LIST.x + 8.0, y + 3.0, 13.0, DIM, Align::Left);
            let subject: String = commit.subject.chars().take(26).collect();
            c.label(subject, LIST.x + 8.0, y + 19.0, 15.0, TEXT, Align::Left);
        }
    }

    fn draw_list(&self, c: &mut Canvas) {
        let top = self.list_top();
        if top > LIST.y {
            c.rect(LIST.x + 2.0, LIST.y + 2.0, LIST.w - 4.0, LIST_ROW + 2.0, if self.typing { [0.3, 0.27, 0.15, 1.0] } else { BUTTON });
            let caret = if self.typing && (self.clock * 2.0) as i32 % 2 == 0 { "_" } else { "" };
            let count = self.listed().len();
            c.label(format!("/{}{caret}", self.filter), LIST.x + 8.0, LIST.y + 4.0, 17.0, HOT, Align::Left);
            c.label(format!("{count}"), LIST.x + LIST.w - 8.0, LIST.y + 4.0, 15.0, DIM, Align::Right);
        }
        // Only whole rows, so a list scrolled part of a row doesn't spill past its panel.
        let first = (self.scroll / LIST_ROW).ceil() as usize;
        let rows = ((LIST.h - (top - LIST.y) - (first as f32 * LIST_ROW - self.scroll)) / LIST_ROW) as usize;
        for (i, row) in self.rows().into_iter().enumerate().skip(first).take(rows) {
            let y = top + i as f32 * LIST_ROW - self.scroll;
            match row {
                Row::Heading { title, depth, count, saved, open } => {
                    c.rect(LIST.x, y + 1.0, LIST.w, LIST_ROW - 2.0, BUTTON);
                    let title: String = title.rsplit('/').next().unwrap_or(title).chars().take(20 - 2 * depth).collect();
                    c.label(format!("{} {title}", if open { "-" } else { "+" }), LIST.x + 6.0 + INDENT * depth as f32, y + 3.0, 15.0, TEXT, Align::Left);
                    let tally = if saved > 0 { format!("{saved}*  {count}") } else { count.to_string() };
                    c.label(tally, LIST.x + LIST.w - 6.0, y + 4.0, 13.0, if saved > 0 { HOT } else { DIM }, Align::Right);
                }
                Row::Sprite((name, saved)) => {
                    if *name == self.sprite.name {
                        c.rect(LIST.x, y, LIST.w, LIST_ROW, [0.35, 0.3, 0.18, 1.0]);
                    }
                    let mark = if *saved { "  *" } else { "" };
                    let depth = sprite::section(name).matches('/').count() as f32;
                    c.label(format!("{}{mark}", list_label(name)), LIST.x + 14.0 + INDENT * depth, y + 2.0, 17.0, if *saved { HOT } else { TEXT }, Align::Left);
                }
            }
        }
    }

    fn draw_controls(&self, c: &mut Canvas) {

        for b in self.buttons() {
            let r = b.rect;
            if let Action::Color(col) = b.action {
                if b.on {
                    c.rect(r.x - 2.0, r.y - 2.0, r.w + 4.0, r.h + 4.0, TEXT);
                }
                c.rect(r.x, r.y, r.w, r.h, rgba(col));
                // A color cleared in 3D wears a corner mark, and paint another.
                if self.sprite.clear_in_3d.contains(&col) {
                    c.rect(r.x + r.w - 8.0, r.y, 8.0, 8.0, [0.0, 0.0, 0.0, 1.0]);
                    c.rect(r.x + r.w - 6.0, r.y + 2.0, 4.0, 4.0, [1.0; 4]);
                }
                if self.sprite.paint.colors.contains(&[col[0], col[1], col[2]]) {
                    c.rect(r.x, r.y + r.h - 8.0, 8.0, 8.0, [0.0, 0.0, 0.0, 1.0]);
                    c.rect(r.x + 2.0, r.y + r.h - 6.0, 4.0, 4.0, HOT);
                }
                continue;
            }
            if let Action::Shade(k, j) = b.action {
                c.rect(r.x - 1.0, r.y - 1.0, r.w + 2.0, r.h + 2.0, TEXT);
                c.rect(r.x, r.y, r.w, r.h, rgba(self.shade_color(k, j).unwrap_or(CLEAR)));
                continue;
            }
            let thumb = match b.action {
                Action::Frame(i) => Some((i, None)),
                Action::Coat(k) => self.coats_at.map(|at| (at + k.map_or(0, |k| k + 1), None)),
                Action::View(v) => Some((self.surface_of(v), Some(VIEW_LABELS[v]))),
                _ => None,
            };
            let added = match b.action {
                Action::AddView(v) => Some(v),
                Action::AddTop => Some(TOP),
                _ => None,
            };
            if let Some(v) = added {
                c.rect(r.x, r.y, r.w, r.h, BUTTON);
                c.label("+", r.x + r.w / 2.0, r.y + 6.0, 30.0, TEXT, Align::Center);
                let label = VIEW_LABELS[v];
                c.label(label, r.x + r.w / 2.0, r.y + r.h - 18.0, 13.0, DIM, Align::Center);
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
        // A color the palette lacks shows in the warning color: the game's own art keeps to its palette.
        let off = !self.palette.contains(&self.color);
        c.label(format!("#{:02X}{:02X}{:02X}", self.color[0], self.color[1], self.color[2]), SIDE_X + 130.0, 168.0, 18.0, if off { HOT } else { TEXT }, Align::Left);
        for (i, bar) in self.bars().iter().enumerate() {
            c.label(["R", "G", "B"][i], SIDE_X + 4.0, bar.y - 2.0, 18.0, DIM, Align::Left);
            for k in 0..16 {
                let mut col = self.color;
                col[i] = (k * 17) as u8;
                c.rect(bar.x + k as f32 * bar.w / 16.0, bar.y, bar.w / 16.0 + 0.5, bar.h, rgba(col));
            }
            c.rect(bar.x + self.color[i] as f32 / 255.0 * bar.w - 2.0, bar.y - 3.0, 4.0, bar.h + 6.0, TEXT);
        }
        c.label(self.palette_label(), SIDE_X, 294.0, 16.0, DIM, Align::Left);
        if !self.is_card() && !self.none_open() {
            c.label("Stands in 3D as", SIDE_X, 546.0, 16.0, DIM, Align::Left);
        }
        match self.stand {
            _ if self.none_open() => {}
            0 => c.label(format!("Depth {} px", self.depth), SIDE_X, 645.0, 18.0, TEXT, Align::Left),
            1 => {
                c.label(format!("Height {} px", self.height), SIDE_X, 645.0, 18.0, TEXT, Align::Left);
                c.label(format!("Float {} px", self.lift), SIDE_X, 676.0, 18.0, TEXT, Align::Left);
            }
            CARVE => c.label(format!("Height {} px", self.view_height()), SIDE_X, 645.0, 18.0, TEXT, Align::Left),
            TURNED => c.label(format!("Depth {} px", self.depth), SIDE_X, 645.0, 18.0, TEXT, Align::Left),
            _ => {}
        }
        if let Some(v) = self.view {
            c.label(format!("{}: {}", VIEW_NAMES[v], VIEW_HINTS[v]), CANVAS.x + 8.0, CANVAS.y + 6.0, 16.0, HOT, Align::Left);
        }
        if let Some(f) = self.open_prop_fields() {
            let open = match f.open {
                0 => "Blocks every row".to_string(),
                1 => "Top row open".to_string(),
                n => format!("Top {n} rows open"),
            };
            c.label(open, SIDE_X, PROP_OPEN_Y + 6.0, 16.0, TEXT, Align::Left);
            c.label(if f.hp > 0.0 { format!("HP {}", f.hp) } else { "Won't break".into() }, SIDE_X, PROP_HP_Y + 5.0, 18.0, TEXT, Align::Left);
        }
        let f = self.current();
        if !self.none_open() {
            c.label(format!("{} fps", self.fps), SIDE_X, 708.0, 18.0, TEXT, Align::Left);
            c.label(format!("{} / {}", self.frame + 1, self.sprite.frames.len()), CANVAS.x + 248.0, 704.0, 18.0, DIM, Align::Left);
            c.label(format!("W {}", f.w), CANVAS.x + 362.0, 704.0, 18.0, TEXT, Align::Right);
            c.label(format!("H {}", f.h), CANVAS.x + 484.0, 704.0, 18.0, TEXT, Align::Right);
        }
        if let Some((_, row)) = sprite::card_part(&self.sprite.name).filter(|_| self.view.is_none() && !self.stats) {
            c.label(sprite::card_frame_name(row, self.frame), CANVAS.x + 8.0, CANVAS.y + 6.0, 16.0, HOT, Align::Left);
        }
        if let Some(p) = self.hover.filter(|p| CANVAS.contains(*p)) {
            let (x, y) = self.pixel_at(p);
            if f.get(x, y).is_some() {
                c.label(format!("{x}, {y}"), CANVAS.x + CANVAS.w, 30.0, 16.0, DIM, Align::Right);
            }
        }

        let help = if self.modding {
            "/: search   J/K: parts   H/L: frames   T: stats   S: select   Cmd+C/X/V   hold I: pick   Shift-click: line   Cmd+S: save"
        } else {
            "/: search   J/K: sprites   H/L: frames   R: history   S: select   Cmd+C/X/V   hold I: pick   Shift-click: line   Cmd+S: save"
        };
        c.label(help, 12.0, 748.0, 13.0, DIM, Align::Left);
        self.draw_overlays(c);
    }

    /// The status line, and the unsaved-changes and naming dialogs over everything else.
    fn draw_overlays(&self, c: &mut Canvas) {
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
            let then = match &self.held_import {
                Some(held) => format!("importing {}", held.source),
                None => format!("opening {name}"),
            };
            c.label(format!("Save them before {then}?"), d.x + d.w / 2.0, d.y + 70.0, 22.0, TEXT, Align::Center);
            for (i, (label, choice)) in CHOICES.iter().enumerate() {
                let r = choice_rect(i);
                c.rect(r.x, r.y, r.w, r.h, if *choice == Choice::Save { HOT } else { BUTTON });
                let ink = if *choice == Choice::Save { [0.1, 0.1, 0.1, 1.0] } else { TEXT };
                let label = if *choice == Choice::Save && self.held_import.is_some() { "Save and import  S" } else { label };
                c.label(label, r.x + r.w / 2.0, r.y + (r.h - 20.0) / 2.0, 20.0, ink, Align::Center);
            }
        }
        if let Some((kind, name)) = &self.ask {
            c.rect(0.0, 0.0, 1024.0, 768.0, [0.0, 0.0, 0.0, 0.6]);
            c.rect(262.0, 280.0, 500.0, 170.0, PANEL);
            let what = match (*kind, self.modding) {
                (Ask::New, true) => "Name the new enemy card",
                (Ask::New, false) => "Name the new sprite",
                (Ask::Copy, _) => "Name the copy",
                (Ask::Palette, _) => "Draw from a Lospec palette",
            };
            c.label(what, 512.0, 300.0, 24.0, TEXT, Align::Center);
            let hint = if *kind == Ask::Palette {
                "Its name from lospec.com/palette-list, such as endesga-32; blank for the game's".to_string()
            } else if self.modding {
                "Its id: letters, digits and _. PLAYER_ makes a player card, PROP_ a prop, MAP_ a map".to_string()
            } else {
                format!("Start with {} for an enemy card, {} for a player card", sprite::CARD_PREFIX, sprite::PLAYER_CARD_PREFIX)
            };
            c.label(hint, 512.0, 328.0, 15.0, DIM, Align::Center);
            let caret = if (self.clock * 2.0) as i32 % 2 == 0 { "_" } else { " " };
            c.label(format!("{name}{caret}"), 512.0, 350.0, 30.0, HOT, Align::Center);
            c.label(if *kind == Ask::Palette { "Enter to load, Esc to cancel" } else { "Enter to create, Esc to cancel" }, 512.0, 405.0, 16.0, DIM, Align::Center);
        }
    }

    /// With Onion on, the frame before the one being drawn, wrapping round from the first.
    fn onion_frame(&self) -> Option<&Frame> {
        let n = self.sprite.frames.len();
        (self.onion && self.view.is_none() && n > 1).then(|| &self.sprite.frames[(self.frame + n - 1) % n])
    }

    /// Compare's other sprites, read-only beside the open one, each named underneath.
    fn draw_family(&self, c: &mut Canvas, zoom: f32) {
        let Some((origins, _)) = self.compare_layout() else { return };
        for ((name, frames), o) in self.family.iter().zip(&origins) {
            let f = self.family_frame(name, frames);
            let (w, h) = (f.w as f32 * zoom, f.h as f32 * zoom);
            let mine = *name == self.sprite.name;
            clip(c, o[0] - 3.0, o[1] - 3.0, w + 6.0, h + 6.0, if mine { HOT } else { [0.25, 0.25, 0.29, 1.0] });
            clip(c, o[0], o[1], w, h, [0.3, 0.3, 0.33, 1.0]);
            if !mine {
                for y in 0..f.h {
                    for x in 0..f.w {
                        let p = f.px[y * f.w + x];
                        if p[3] > 0 {
                            clip(c, o[0] + x as f32 * zoom, o[1] + y as f32 * zoom, zoom, zoom, rgba(p));
                        }
                    }
                }
            }
            let short = name.rsplit_once('_').map_or(name.as_str(), |(_, s)| s);
            c.label(short, o[0] + w / 2.0, o[1] + h + 8.0, 15.0, if mine { HOT } else { DIM }, Align::Center);
        }
    }

    fn draw_canvas(&self, c: &mut Canvas) {
        c.rect(CANVAS.x, CANVAS.y, CANVAS.w, CANVAS.h, PANEL);
        let f = self.current();
        let (o, zoom) = self.grid();
        self.draw_family(c, zoom);
        let (w, h) = (f.w as f32 * zoom, f.h as f32 * zoom);
        // A checkerboard under clear pixels.
        let cell = 16.0;
        // The cells `size` big from `at` that fall within `from..from + extent`, of `n`.
        let seen = |from: f32, extent: f32, at: f32, size: f32, n: usize| {
            let first = ((from - at) / size).floor().max(0.0) as usize;
            (first.min(n), (((from + extent - at) / size).ceil().max(0.0) as usize).min(n))
        };
        let (gx0, gx1) = seen(CANVAS.x, CANVAS.w, o[0], cell, (w / cell).ceil() as usize);
        let (gy0, gy1) = seen(CANVAS.y, CANVAS.h, o[1], cell, (h / cell).ceil() as usize);
        for gy in gy0..gy1 {
            for gx in gx0..gx1 {
                let (x, y) = (gx as f32 * cell, gy as f32 * cell);
                let shade = if (gx + gy) % 2 == 0 { [0.3, 0.3, 0.33, 1.0] } else { [0.36, 0.36, 0.4, 1.0] };
                clip(c, o[0] + x, o[1] + y, cell.min(w - x), cell.min(h - y), shade);
            }
        }
        if let Some(prev) = self.onion_frame() {
            for y in 0..prev.h.min(f.h) {
                for x in 0..prev.w.min(f.w) {
                    let p = prev.px[y * prev.w + x];
                    if p[3] > 0 {
                        let [r, g, b, _] = rgba(p);
                        clip(c, o[0] + x as f32 * zoom, o[1] + y as f32 * zoom, zoom, zoom, [r, g, b, ONION]);
                    }
                }
            }
        }
        let (x0, x1) = seen(CANVAS.x, CANVAS.w, o[0], zoom, f.w);
        let (y0, y1) = seen(CANVAS.y, CANVAS.h, o[1], zoom, f.h);
        for y in y0..y1 {
            for x in x0..x1 {
                let p = f.px[y * f.w + x];
                if p[3] > 0 {
                    clip(c, o[0] + x as f32 * zoom, o[1] + y as f32 * zoom, zoom, zoom, rgba(p));
                }
            }
        }
        if zoom >= 8.0 {
            let line = [1.0, 1.0, 1.0, 0.07];
            for x in x0..=x1 {
                clip(c, o[0] + x as f32 * zoom, o[1], 1.0, h, line);
            }
            for y in y0..=y1 {
                clip(c, o[0], o[1] + y as f32 * zoom, w, 1.0, line);
            }
        }
        if self.mirror {
            clip(c, o[0] + w / 2.0 - 1.0, o[1], 2.0, h, [0.4, 0.8, 1.0, 0.5]);
        }
        if let Some(sel) = &self.selection {
            if let Some(f) = &sel.float {
                for y in 0..f.h {
                    for x in 0..f.w {
                        let p = f.px[y * f.w + x];
                        let (px, py) = (o[0] + (sel.x + x as i32) as f32 * zoom, o[1] + (sel.y + y as i32) as f32 * zoom);
                        if p[3] > 0 {
                            clip(c, px, py, zoom, zoom, rgba(p));
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
                        clip(c, r[0], r[1], r[2], r[3], HOT);
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
                clip(c, px, py, zoom, 1.0, edge);
                clip(c, px, py + zoom - 1.0, zoom, 1.0, edge);
                clip(c, px, py, 1.0, zoom, edge);
                clip(c, px + zoom - 1.0, py, 1.0, zoom, edge);
            }
        }
    }
}

/// Draws the part of a rectangle that falls on the pixel canvas.
fn clip(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, color: Color) {
    let (x0, y0) = (x.max(CANVAS.x), y.max(CANVAS.y));
    let (x1, y1) = ((x + w).min(CANVAS.x + CANVAS.w), (y + h).min(CANVAS.y + CANVAS.h));
    if x1 > x0 && y1 > y0 {
        c.rect(x0, y0, x1 - x0, y1 - y0, color);
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

/// How the list shows a sprite: a card's part without the prefix its heading already gives, its id cut short to fit.
fn list_label(name: &str) -> String {
    const ROOM: usize = 16;
    // Under its file's heading, a data section shows by its own name; a ground code by its character.
    if let Some(rest) = name.strip_prefix(sprite::DATA_PREFIX) {
        return match rest.split_once('_').map_or("", |(_, s)| s) {
            "" => "(top of the file)".to_string(),
            s if s.len() == 3 && s.starts_with('_') => u8::from_str_radix(&s[1..], 16).map_or(s.to_string(), |c| format!("[{}]", c as char)),
            s => s.to_string(),
        };
    }
    let Some((id, row)) = sprite::card_part(name) else { return name.to_string() };
    let (id, part) = (id.to_ascii_uppercase(), sprite::CARD_PARTS[row]);
    if id.len() + 1 + part.len() <= ROOM {
        return format!("{id}_{part}");
    }
    let keep = ROOM.saturating_sub(part.len() + 2).max(1);
    format!("{}~_{part}", id[..keep.min(id.len())].trim_end_matches('_'))
}

fn choice_rect(i: usize) -> Rect {
    let widths = [220.0, 150.0, 150.0];
    let x = DIALOG.x + 24.0 + widths[..i].iter().map(|w| w + 14.0).sum::<f32>();
    Rect { x, y: DIALOG.y + DIALOG.h - 76.0, w: widths[i], h: 48.0 }
}

fn rgba(c: Rgba) -> Color {
    [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, if c[3] == 0 { 0.0 } else { 1.0 }]
}

/// A character typed into a Lospec palette's name, as its address writes it.
fn lospec_char(c: char) -> Option<char> {
    match c {
        ' ' | '_' | '-' => Some('-'),
        c if c.is_ascii_alphanumeric() => Some(c.to_ascii_lowercase()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::store::FsStore;

    /// Where this test's editor saves.
    fn tmp() -> PathBuf {
        std::env::temp_dir().join(format!("lh-editor-{}-{:?}", std::process::id(), std::thread::current().id()))
    }

    fn editor_in(dir: &Path, catalog: PathBuf) -> Editor {
        Editor::new(Box::new(FsStore { dir: dir.to_path_buf(), catalog, cards: dir.join("enemies") }), "BARREL")
    }

    fn editor() -> Editor {
        editor_in(&tmp(), tmp().join("art.txt"))
    }

    fn editor_on(name: &str) -> Editor {
        let dir = tmp();
        Editor::new(Box::new(FsStore { dir: dir.clone(), catalog: dir.join("art.txt"), cards: dir.join("enemies") }), name)
    }

    fn click_pixel(e: &mut Editor, x: i32, y: i32) {
        let (o, zoom) = e.grid();
        let p = [o[0] + (x as f32 + 0.5) * zoom, o[1] + (y as f32 + 0.5) * zoom];
        e.mouse_down(p, false, false);
        e.mouse_up();
    }

    #[test]
    fn a_cards_still_parts_start_blank_and_generate_from_the_walk() {
        let dir = tmp().join("still-parts");
        let _ = std::fs::remove_dir_all(&dir);
        let store = || FsStore { dir: dir.join("sprites"), catalog: dir.join("art.txt"), cards: dir.join("enemies") };
        let mut e = Editor::new(Box::new(store()), "BARREL");
        e.act(Action::Ask(Ask::New), false);
        type_in(&mut e, "card ghoul");
        e.key(Key::Enter);
        let stand = e.sprite.frames[0].clone();
        e.switch_to("CARD_GHOUL_HURT".into());
        assert!(e.sprite.frames.iter().all(|f| f.px.iter().all(|p| p[3] == 0)), "a new card leaves hurt to the game");
        let (figure, dead) = e.figure_frames().unwrap();
        assert_eq!((figure.len(), dead), (cards::FIGURE_FRAMES, false));
        let derived: Vec<Rgba> = stand.px.iter().map(|&p| last_harvest_game::hurt_pixel(p)).collect();
        assert_eq!(figure[6].px, derived, "the preview shows the hurt frame the game will draw");
        e.act(Action::Generate, false);
        assert_eq!(e.sprite.frames[0].px, derived);
        e.act(Action::ClearPart, false);
        assert!(e.sprite.frames.iter().all(|f| f.px.iter().all(|p| p[3] == 0)));
        e.switch_to("CARD_GHOUL_DEAD".into());
        e.decide(Choice::Discard);
        assert_eq!(e.sprite.name, "CARD_GHOUL_DEAD");
        click_pixel(&mut e, 0, 0);
        e.act(Action::Save, false);
        let bytes = std::fs::read(dir.join("enemies/ghoul.enemy.png")).unwrap();
        let card = last_harvest_pack::card::read("ghoul.enemy.png", &bytes).unwrap();
        assert_eq!(card.sheet.len(), (16 * 3 * 16 * 6 * 4) as usize, "a drawn dead part saves a sixth row");
        assert!(e.figure_frames().unwrap().1);
    }

    #[test]
    fn a_game_figures_still_sprites_generate_from_its_walk() {
        let mut e = editor_on("SKINNY_HURT");
        assert_eq!(e.built_in_still(), Some(4));
        let front = e.store.load("SKINNY_FRONT").unwrap().frames[0].clone();
        e.act(Action::Generate, false);
        assert_eq!(e.sprite.frames.len(), 3, "one per facing");
        assert_eq!(e.sprite.frames[0].px, front.px.iter().map(|&p| last_harvest_game::hurt_pixel(p)).collect::<Vec<_>>());
        e.key(Key::Undo);
        assert_ne!(e.sprite.frames[0].px, front.px.iter().map(|&p| last_harvest_game::hurt_pixel(p)).collect::<Vec<_>>(), "Undo puts the drawn ones back");
        assert_eq!(editor_on("BARREL_FLASH").built_in_still(), None, "a barrel has no walk to draw from");
        let mut dead = editor_on("SKINNY_HURT");
        dead.sprite.name = "SKINNY_DEAD".into();
        dead.set_live(vec![Art(1), Art(2), Art(3)]);
        dead.frame = 1;
        assert_eq!(dead.lying_figure().and_then(|f| f.dead), Some([Art(2); 3]), "the frame being edited lies down");
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

    /// Drags a shape from one pixel to another, with Shift held at the start or not, and lists the painted pixels.
    fn drag_shape(tool: Tool, from: (i32, i32), to: (i32, i32), shift: bool) -> (Editor, Vec<(i32, i32)>) {
        let mut e = editor();
        e.sprite.frames[0] = Frame::blank(16, 16);
        e.act(Action::Tool(tool), false);
        e.color = [5, 6, 7, 255];
        let px = |e: &Editor, x: i32, y: i32| {
            let (o, zoom) = e.grid();
            [o[0] + (x as f32 + 0.5) * zoom, o[1] + (y as f32 + 0.5) * zoom]
        };
        e.mouse_down(px(&e, from.0, from.1), false, shift);
        e.drag(px(&e, 15, 15));
        e.drag(px(&e, to.0, to.1));
        e.mouse_up();
        let painted = (0..16).flat_map(|y| (0..16).map(move |x| (x, y))).filter(|&(x, y)| e.current().get(x, y) == Some([5, 6, 7, 255])).collect();
        (e, painted)
    }

    #[test]
    fn a_box_outlines_or_fills_and_leaves_no_trail_of_its_drag() {
        let (_, outline) = drag_shape(Tool::Rect, (2, 2), (5, 4), false);
        assert_eq!(outline.len(), 10, "the 4x3 border, and nothing of the larger box the drag passed through: {outline:?}");
        assert!(!outline.contains(&(3, 3)) && outline.contains(&(5, 4)));
        let (_, filled) = drag_shape(Tool::Rect, (5, 4), (2, 2), true);
        assert_eq!(filled.len(), 12, "Shift fills, whichever corner it starts from");
    }

    #[test]
    fn an_oval_is_round_and_symmetric() {
        let (_, ring) = drag_shape(Tool::Ellipse, (0, 0), (8, 8), false);
        let (_, disc) = drag_shape(Tool::Ellipse, (0, 0), (8, 8), true);
        assert!(ring.contains(&(4, 0)) && ring.contains(&(0, 4)) && ring.contains(&(8, 4)) && ring.contains(&(4, 8)));
        assert!(!ring.contains(&(0, 0)) && !ring.contains(&(4, 4)), "no corners, and hollow");
        assert!(disc.contains(&(4, 4)) && disc.len() > ring.len());
        assert!(ring.iter().all(|&(x, y)| ring.contains(&(8 - x, y)) && ring.contains(&(x, 8 - y))));
    }

    #[test]
    fn a_line_snaps_straight_with_shift_and_undoes_in_one_step() {
        let (_, slant) = drag_shape(Tool::Line, (0, 0), (6, 2), false);
        assert!(slant.contains(&(0, 0)) && slant.contains(&(6, 2)) && slant.len() == 7);
        let (mut e, flat) = drag_shape(Tool::Line, (0, 5), (6, 6), true);
        assert!(flat.iter().all(|&(_, y)| y == 5) && flat.len() == 7, "{flat:?}");
        e.key(Key::Undo);
        assert!((0..16).all(|x| e.current().get(x, 5) != Some([5, 6, 7, 255])), "one undo takes the whole line");
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
    fn clear_color_clears_it_in_every_frame_in_one_undo() {
        let mut e = editor();
        e.color = [9, 9, 9, 255];
        click_pixel(&mut e, 1, 3);
        click_pixel(&mut e, 5, 7);
        e.color = [4, 4, 4, 255];
        click_pixel(&mut e, 2, 2);
        e.sprite.frames.push(e.sprite.frames[0].clone());
        e.color = [9, 9, 9, 255];
        e.act(Action::ClearColor, false);
        for f in &e.sprite.frames {
            assert_eq!((f.get(1, 3), f.get(5, 7)), (Some(CLEAR), Some(CLEAR)));
            assert_eq!(f.get(2, 2), Some([4, 4, 4, 255]), "other colors stay");
        }
        e.key(Key::Undo);
        assert!(e.sprite.frames.iter().all(|f| f.get(1, 3) == Some([9, 9, 9, 255])));
    }

    #[test]
    fn a_lospec_palette_replaces_the_games_until_cleared() {
        let mut e = editor();
        assert_eq!(e.palette, sprite::palette(), "the game's palette to start");
        e.act(Action::Ask(Ask::Palette), false);
        e.set_text("Endesga 32!");
        assert_eq!(e.ask.as_ref().map(|(_, n)| n.as_str()), Some("endesga-32"), "typed as Lospec writes its names");
        e.use_palette("tiny", "be4a2f\nd77643\n");
        assert_eq!((e.palette.len(), e.palette_label()), (2, "tiny".to_string()));
        e.use_palette("broken", "nope");
        assert_eq!(e.palette.len(), 2, "a file that isn't a palette leaves the one in use");
        e.ask = Some((Ask::Palette, String::new()));
        e.key(Key::Enter);
        assert_eq!((e.palette.clone(), e.palette_label()), (sprite::palette(), "Game palette".to_string()), "blank goes back to the game's");
    }

    #[test]
    fn a_new_coats_darker_shades_keep_to_the_palette() {
        let mut e = editor();
        e.sprite.paint = last_harvest_game::paint::parse("paint 917662 726650 4E4637").unwrap();
        e.color = [0xAD, 0x64, 0x4B, 255];
        e.act(Action::AddCoat, false);
        let coat = &e.sprite.paint.coats[0];
        assert_eq!(coat.len(), 3);
        assert!(coat.iter().all(|c| e.palette.contains(&[c[0], c[1], c[2], 255])), "{coat:?}");
    }

    #[test]
    fn coats_repaint_the_previews_and_save() {
        let mut e = editor();
        e.color = [200, 100, 50, 255];
        click_pixel(&mut e, 1, 3);
        e.act(Action::AddCoat, false);
        assert!(e.sprite.paint.coats.is_empty(), "no coat before a paint color");
        e.act(Action::PaintColor, false);
        e.color = [20, 40, 160, 255];
        e.act(Action::Coats, false);
        e.act(Action::AddCoat, false);
        assert!(!e.on_canvas([CANVAS.x + 100.0, CANVAS.y + 100.0]), "the page covers the pixels");
        assert_eq!(e.current().get(1, 3), Some([200, 100, 50, 255]), "the drawing keeps its colors");
        let surfaces = e.surfaces();
        let at = e.coats_at.unwrap();
        assert_eq!((surfaces[0].get(1, 3), surfaces[at].get(1, 3), surfaces[at + 1].get(1, 3)), (Some([20, 40, 160, 255]), Some([200, 100, 50, 255]), Some([20, 40, 160, 255])), "the previews show the coat picked; the page's rows each one");
        e.color = [0, 255, 0, 255];
        e.act(Action::Shade(0, 0), false);
        assert_eq!(e.sprite.paint.coats[0], [[0, 255, 0]]);
        for _ in 1..last_harvest_game::paint::MAX_COATS {
            e.act(Action::AddCoat, false);
        }
        for b in e.buttons() {
            assert!(CANVAS.contains([b.rect.x, b.rect.y + b.rect.h - 1.0]) || !matches!(b.action, Action::Coat(_) | Action::Shade(..) | Action::DropCoat(_)), "{:?}", b.action);
        }
        let back = Sprite::from_files(&e.sprite.name, &e.sprite.files()).unwrap();
        assert_eq!(back.paint, e.sprite.paint);
        e.act(Action::Coat(None), false);
        assert_eq!(e.surfaces()[0].get(1, 3), Some([200, 100, 50, 255]), "As drawn shows the drawing");
        e.color = [200, 100, 50, 255];
        e.act(Action::PaintColor, false);
        assert!(e.sprite.paint.colors.is_empty() && e.sprite.paint.text().is_empty(), "no paint color, no coats saved");
    }

    #[test]
    fn a_color_cleared_in_3d_stays_in_2d_and_saves() {
        let mut e = editor();
        e.color = [9, 9, 9, 255];
        click_pixel(&mut e, 1, 3);
        e.act(Action::ClearIn3d, false);
        assert_eq!(e.current().get(1, 3), Some([9, 9, 9, 255]), "the 2D frame keeps it");
        let surfaces = e.surfaces();
        let (k, n) = e.solids_at.unwrap();
        assert_eq!((surfaces.len(), surfaces[k].get(1, 3)), (k + n, Some(CLEAR)), "the 3D copy leaves it out");
        let files = e.sprite.files();
        let back = Sprite::from_files(&e.sprite.name, &files).unwrap();
        assert_eq!(back.clear_in_3d, vec![[9, 9, 9, 255]]);
        e.act(Action::ClearIn3d, false);
        assert!(e.sprite.clear_in_3d.is_empty() && e.surfaces().len() == n, "pressing again puts it back");
    }

    fn type_in(e: &mut Editor, text: &str) {
        text.chars().for_each(|ch| e.key(Key::Char(ch)));
    }

    /// Fails if any two of the editor's buttons overlap or one leaves the pane.
    fn buttons_fit(e: &Editor) {
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
    fn a_new_card_lists_as_six_parts_that_save_into_one_png() {
        let dir = tmp().join("cards");
        let _ = std::fs::remove_dir_all(&dir);
        let store = || FsStore { dir: dir.join("sprites"), catalog: dir.join("art.txt"), cards: dir.join("enemies") };
        let mut e = Editor::new(Box::new(store()), "BARREL");
        e.act(Action::Ask(Ask::New), false);
        type_in(&mut e, "card ghoul");
        e.key(Key::Enter);
        assert!(dir.join("enemies/ghoul.enemy.png").exists(), "New writes the card from the template");
        assert_eq!(e.sprite.name, "CARD_GHOUL_FRONT");
        let parts: Vec<&str> = e.names.iter().map(|(n, _)| n.as_str()).filter(|n| n.starts_with("CARD_")).collect();
        assert_eq!(parts, ["CARD_GHOUL_FRONT", "CARD_GHOUL_SIDE", "CARD_GHOUL_BACK", "CARD_GHOUL_FLASH", "CARD_GHOUL_HURT", "CARD_GHOUL_DEAD"]);
        assert_eq!(e.sprite.frames.len(), 3);
        assert!(e.sprite.frames.iter().all(|f| (f.w, f.h) == (16, 16) && f.px.iter().any(|p| p[3] > 0)), "it starts from a figure");
        assert_ne!(e.sprite.frames[0].px, e.sprite.frames[1].px, "whose legs take turns");
        assert_eq!(e.stand(), Stand::Figure);
        e.act(Action::AddFrame, false);
        assert_eq!(e.sprite.frames.len(), 4, "a walking part takes another frame");
        e.act(Action::Save, false);
        e.act(Action::Compare, false);
        assert_eq!(e.family.len(), 6, "Compare lays out the card's parts like a built-in zombie's");
        e.act(Action::Compare, false);
        e.switch_to("CARD_GHOUL_SIDE".into());
        click_pixel(&mut e, 0, 0);
        e.act(Action::Stats, false);
        buttons_fit(&e);
        let kinds = |e: &Editor| e.mix.as_ref().unwrap().as_ref().unwrap().kinds.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>();
        assert_eq!(kinds(&e), ["big", "kid", "ghoul", "skinny"]);
        let skinny_total = |e: &Editor| e.mix.as_ref().unwrap().as_ref().unwrap().shares.iter().map(|s| s[3]).sum::<f32>();
        assert_eq!(skinny_total(&e), 0.0, "a new card's chance of 1 leaves skinnies none");
        for _ in 0..70 {
            e.act(Action::StatStep(7, -1), false);
        }
        assert!(skinny_total(&e) > 0.0, "and lowering it gives them some back, unsaved");
        e.draw(&mut Canvas::default());
        e.act(Action::StatStep(0, 1), false);
        e.act(Action::StatStep(1, 1), true);
        e.act(Action::StatStep(9, 1), false);
        assert_eq!(last_harvest_game::card_fields(e.sprite.card.as_deref().unwrap()).unwrap().numbers[9], ("charge", 1.1));
        e.act(Action::Flag(0), false);
        e.act(Action::EditName, false);
        for _ in 0..5 {
            e.key(Key::Backspace);
        }
        assert!(e.wants_text() && e.free_text());
        type_in(&mut e, "Bog Ghoul");
        e.act(Action::EditAbout, false);
        assert_eq!(e.text(), Some(""), "About keeps the name typed so far and starts on an empty about");
        type_in(&mut e, "Wet, slow and #hungry");
        assert_eq!(e.text(), Some("Wet, slow and hungry"), "a # would start a comment");
        e.set_text(&format!("Wet\u{2014}slow {}", "z".repeat(100)));
        assert!(e.text().is_some_and(|t| t.starts_with("Wetslow") && t.len() == last_harvest_game::ABOUT_MAX), "only the font's characters, up to the limit");
        e.set_text("Wet and slow, but it never gives up.");
        e.draw(&mut Canvas::default());
        e.key(Key::Enter);
        e.act(Action::Save, false);
        assert!(!e.unsaved(), "{:?}", e.status);
        let saved = last_harvest_pack::card::read("ghoul.enemy.png", &std::fs::read(dir.join("enemies/ghoul.enemy.png")).unwrap()).unwrap();
        let f = last_harvest_game::card_fields(&saved.text).unwrap();
        assert_eq!((f.id.as_str(), f.name.as_str(), f.about.as_str()), ("ghoul", "Bog Ghoul", "Wet and slow, but it never gives up."));
        assert_eq!((f.numbers[0].1, f.numbers[1].1, f.flags[0].1), (15.0, 30.0, true), "health 10 + 5, speed 20 + 10, heavy");
        assert_eq!(saved.frame, [16, 16]);
        let side = Editor::new(Box::new(store()), "CARD_GHOUL_SIDE");
        assert_eq!(side.sprite.frames[0].get(0, 0), Some(e.color), "the pixel landed in the side row");
        let front = Editor::new(Box::new(store()), "CARD_GHOUL_FRONT");
        assert_eq!(front.sprite.frames[0].get(0, 0), Some([0, 0, 0, 0]), "and nowhere else");
        assert_eq!(sprite::section("CARD_GHOUL_HURT"), sprite::CARD_SECTION);
    }

    #[test]
    fn a_card_walks_its_parts_in_step_and_holds_its_stills_to_three() {
        let dir = tmp().join("walk");
        let _ = std::fs::remove_dir_all(&dir);
        let store = || FsStore { dir: dir.join("sprites"), catalog: dir.join("art.txt"), cards: dir.join("enemies") };
        let mut e = Editor::new(Box::new(store()), "");
        e.act(Action::Ask(Ask::New), false);
        type_in(&mut e, "card long");
        e.key(Key::Enter);
        e.act(Action::AddFrame, false);
        e.act(Action::AddFrame, false);
        e.act(Action::Save, false);
        let side = Editor::new(Box::new(store()), "CARD_LONG_SIDE");
        assert_eq!(side.sprite.frames.len(), 5, "the other walking parts follow");
        assert!(side.sprite.card.as_deref().unwrap().contains("walk"), "and the text says how long the walk is");
        let saved = last_harvest_pack::card::read("x", &std::fs::read(dir.join("enemies/long.enemy.png")).unwrap()).unwrap();
        assert_eq!(saved.frame, [16, 16], "a five-wide sheet still has 16-pixel frames");
        let mut hurt = Editor::new(Box::new(store()), "CARD_LONG_HURT");
        hurt.act(Action::AddFrame, false);
        hurt.act(Action::DeleteFrame, false);
        assert_eq!(hurt.sprite.frames.len(), 3, "a still part keeps one frame per facing");
        let mut front = Editor::new(Box::new(store()), "CARD_LONG_FRONT");
        for _ in 0..10 {
            front.act(Action::DeleteFrame, false);
        }
        assert_eq!(front.sprite.frames.len(), 1, "a walk keeps at least one frame");
    }

    #[test]
    fn the_player_and_zombie_stills_keep_their_frame_counts() {
        let mut e = editor_on("PLAYER_FRONT");
        e.act(Action::AddFrame, false);
        assert_eq!(e.sprite.frames.len(), 3);
        let mut e = editor_on("KID_FLASH");
        e.act(Action::DeleteFrame, false);
        assert_eq!(e.sprite.frames.len(), 3);
        let mut e = editor_on("KID_FRONT");
        e.act(Action::AddFrame, false);
        assert_eq!(e.sprite.frames.len(), 4, "a zombie's walk can grow");
        let mut e = editor_on("BARREL");
        e.act(Action::AddFrame, false);
        assert_eq!(e.sprite.frames.len(), 2, "other sprites are free");
    }

    #[test]
    fn a_player_card_keeps_three_frames_and_sets_its_starting_weapons() {
        let dir = tmp().join("player");
        let _ = std::fs::remove_dir_all(&dir);
        let store = || FsStore { dir: dir.join("sprites"), catalog: dir.join("art.txt"), cards: dir.join("enemies") };
        let mut e = Editor::new(Box::new(store()), "");
        e.act(Action::Ask(Ask::New), false);
        type_in(&mut e, "player card jo");
        e.key(Key::Enter);
        assert_eq!(e.sprite.name, "PLAYER_CARD_JO_FRONT");
        assert_eq!(sprite::section(&e.sprite.name), sprite::PLAYER_SECTION);
        e.act(Action::AddFrame, false);
        assert_eq!(e.sprite.frames.len(), 3, "the game walks the player with three frames");
        e.act(Action::Stats, false);
        buttons_fit(&e);
        assert!(e.mix.is_none(), "no spawn mix for a player");
        let crows = last_harvest_game::weapons().iter().position(|(id, _)| id == "crows").unwrap();
        e.act(Action::StartWeapon(1, crows), false);
        e.act(Action::StartWeapon(2, crows), false);
        e.act(Action::StartDefault(2), false);
        e.act(Action::EditAbout, false);
        type_in(&mut e, "Up with the sun.");
        e.act(Action::EditAbout, false);
        e.act(Action::EditAbout, false);
        assert_eq!(e.text(), Some("Up with the sun."), "About again types over what's saved");
        e.set_text("  ");
        e.key(Key::Enter);
        assert_eq!(last_harvest_game::player_fields(e.sprite.card.as_deref().unwrap()).unwrap().about, "", "an empty about takes the line out");
        e.act(Action::EditAbout, false);
        type_in(&mut e, "Up with the sun.");
        e.key(Key::Enter);
        e.act(Action::PlayerBase(0, 1), false);
        e.act(Action::PlayerBase(1, 1), false);
        e.act(Action::PlayerBase(1, -1), false);
        e.draw(&mut Canvas::default());
        e.act(Action::Save, false);
        let path = dir.join("players/jo.player.png");
        let saved = last_harvest_pack::card::read_as(sprite::CardKind::Player, "jo", &std::fs::read(path).unwrap()).unwrap();
        let f = last_harvest_game::player_fields(&saved.text).unwrap();
        assert_eq!(f.about, "Up with the sun.");
        assert_eq!(f.base, [Some(last_harvest_game::tuning::tuning().player_hp + 5.0), None, None], "back at the game's own, a number leaves the card");
        assert_eq!(f.start, [None, Some(vec!["knife".into(), "crows".into()]), None], "Normal adds crows to the game's knife; Hard went back to the default");
        let e = Editor::new(Box::new(store()), "");
        assert!(e.names.iter().any(|(n, _)| n == "PLAYER_CARD_JO_HURT"));
    }

    #[test]
    fn the_mod_editor_makes_a_player_card_from_a_player_name() {
        let dir = tmp().join("modplayer");
        let _ = std::fs::remove_dir_all(&dir);
        let mut e = Editor::new(Box::new(FsStore { dir: dir.join("sprites"), catalog: dir.join("art.txt"), cards: dir.join("enemies") }), "");
        e.set_modding();
        e.act(Action::Ask(Ask::New), false);
        type_in(&mut e, "player sam");
        e.key(Key::Enter);
        assert_eq!(e.sprite.name, "PLAYER_CARD_SAM_FRONT");
        e.act(Action::Ask(Ask::New), false);
        type_in(&mut e, "bat");
        e.key(Key::Enter);
        assert_eq!(e.sprite.name, "CARD_BAT_FRONT");
    }

    #[test]
    fn resizing_one_part_resizes_the_whole_card() {
        let dir = tmp().join("resize");
        let _ = std::fs::remove_dir_all(&dir);
        let store = || FsStore { dir: dir.join("sprites"), catalog: dir.join("art.txt"), cards: dir.join("enemies") };
        let mut e = Editor::new(Box::new(store()), "");
        e.act(Action::Ask(Ask::New), false);
        type_in(&mut e, "card tall");
        e.key(Key::Enter);
        e.act(Action::Height(4), false);
        e.act(Action::Save, false);
        let hurt = Editor::new(Box::new(store()), "CARD_TALL_HURT");
        assert_eq!((hurt.sprite.frames[0].w, hurt.sprite.frames[0].h), (16, 20));
    }

    #[test]
    fn a_card_stops_at_the_games_frame_size_and_never_saves_over_an_unreadable_card() {
        let dir = tmp().join("bigcard");
        let _ = std::fs::remove_dir_all(&dir);
        let store = || FsStore { dir: dir.join("sprites"), catalog: dir.join("art.txt"), cards: dir.join("enemies") };
        let mut e = Editor::new(Box::new(store()), "");
        e.act(Action::Ask(Ask::New), false);
        type_in(&mut e, "card big");
        e.key(Key::Enter);
        e.act(Action::Height(60), false);
        assert_eq!(e.current().h, sprite::CARD_MAX_FRAME);
        let path = dir.join("enemies/big.enemy.png");
        std::fs::write(&path, b"not a png").unwrap();
        click_pixel(&mut e, 0, 0);
        e.act(Action::Save, false);
        assert!(e.unsaved(), "{:?}", e.status);
        assert_eq!(std::fs::read(&path).unwrap(), b"not a png", "the file is left for the player to sort out");
    }

    #[test]
    fn importing_over_the_open_card_waits_for_its_changes_then_shows_the_import() {
        let dir = tmp().join("reimport");
        let _ = std::fs::remove_dir_all(&dir);
        let store = || FsStore { dir: dir.join("sprites"), catalog: dir.join("art.txt"), cards: dir.join("enemies") };
        let mut e = Editor::new(Box::new(store()), "");
        e.act(Action::Ask(Ask::New), false);
        type_in(&mut e, "card twin");
        e.key(Key::Enter);
        let shared = std::fs::read(dir.join("enemies/twin.enemy.png")).unwrap();
        click_pixel(&mut e, 0, 0);
        e.import_card("twin.enemy.png", &shared);
        assert!(e.unsaved() && e.pending.is_some(), "the import waits on the unsaved changes");
        e.key(Key::Escape);
        assert!(e.unsaved() && e.held_import.is_none(), "Stay keeps the changes and drops the import");
        e.import_card("twin.enemy.png", &shared);
        e.key(Key::Char('d'));
        assert!(!e.unsaved() && e.status.as_ref().is_some_and(|(t, _)| t.starts_with("Imported twin.enemy.png")), "{:?}", e.status);
        e.color = [1, 2, 3, 255];
        click_pixel(&mut e, 0, 0);
        e.act(Action::Save, false);
        e.import_card("twin.enemy.png", &shared);
        assert_ne!(e.sprite.frames[0].get(0, 0), Some([1, 2, 3, 255]), "the imported card shows, not the one it replaced");
    }

    #[test]
    fn importing_says_which_saved_cards_it_replaces() {
        let dir = tmp().join("replace");
        let _ = std::fs::remove_dir_all(&dir);
        let mut e = Editor::new(Box::new(FsStore { dir: dir.join("sprites"), catalog: dir.join("art.txt"), cards: dir.join("enemies") }), "");
        for id in ["one", "two"] {
            e.act(Action::Ask(Ask::New), false);
            type_in(&mut e, &format!("card {id}"));
            e.key(Key::Enter);
        }
        let read = |id: &str| (format!("{id}.enemy.png"), std::fs::read(dir.join(format!("enemies/{id}.enemy.png"))).unwrap());
        let (one, two) = (read("one"), read("two"));
        std::fs::remove_file(dir.join("enemies/two.enemy.png")).unwrap();
        e.names = e.store.names();
        e.import_pack("my-cards.pak", vec![one, two, ("x.enemy.png".into(), b"no".to_vec())], Vec::new());
        let said = e.status.as_ref().map(|(t, _)| t.clone()).unwrap_or_default();
        assert!(said.starts_with("Imported 2 cards from my-cards.pak, replacing your one; 1 couldn't: x.enemy.png"), "{said}");
    }

    #[test]
    fn the_list_shows_a_cards_parts_without_their_prefix_and_cut_to_fit() {
        assert_eq!(list_label("PLAYER_CARD_JO_FRONT"), "JO_FRONT");
        assert_eq!(list_label("CARD_GHOUL_HURT"), "GHOUL_HURT");
        assert_eq!(list_label("CARD_NORTHERN_FIELDS_GHOUL_FLASH"), "NORTHERN~_FLASH");
        assert_eq!(list_label("BARREL"), "BARREL");
    }

    #[test]
    fn a_death_sound_needs_an_enemy_card_and_an_ogg_or_wav() {
        let dir = tmp().join("sound");
        let _ = std::fs::remove_dir_all(&dir);
        let mut e = Editor::new(Box::new(FsStore { dir: dir.join("sprites"), catalog: dir.join("art.txt"), cards: dir.join("enemies") }), "");
        let said = |e: &Editor| e.status.as_ref().map(|(t, _)| t.clone()).unwrap_or_default();
        assert!(e.attach_sound("boo.ogg", b"OggS....").is_none() && said(&e).contains("enemy card"));
        e.act(Action::Ask(Ask::New), false);
        type_in(&mut e, "card ghoul");
        e.key(Key::Enter);
        assert!(e.attach_sound("boo.mp3", b"ID3....").is_none() && said(&e).contains("isn't an Ogg or WAV"));
        assert!(e.attach_sound("big.ogg", &[b"OggS".as_slice(), &vec![0; 600 * 1024]].concat()).is_none() && said(&e).contains("at most 512 KB"));
        assert!(e.attach_sound("boo.ogg", b"OggS....").is_none() && said(&e).contains("sounds folder"), "the Mac's store keeps none: {}", said(&e));
        e.stats = true;
        let drop = |e: &Editor| e.buttons().iter().any(|b| b.action == Action::DropSound);
        assert!(!drop(&e), "Drop own shows only for a card with its own sound");
        e.own_sound = Some("kill-ghoul.ogg".into());
        assert!(drop(&e));
        e.act(Action::DropSound, false);
        assert!(said(&e).starts_with("Couldn't drop the sound") && e.own_sound.is_some(), "{}", said(&e));
    }

    #[test]
    fn a_card_copies_to_a_card_with_its_own_id() {
        let dir = tmp().join("copy");
        let _ = std::fs::remove_dir_all(&dir);
        let mut e = Editor::new(Box::new(FsStore { dir: dir.join("sprites"), catalog: dir.join("art.txt"), cards: dir.join("enemies") }), "BARREL");
        e.act(Action::Ask(Ask::Copy), false);
        type_in(&mut e, "card nope");
        e.key(Key::Enter);
        assert!(e.ask.is_some(), "a plain sprite doesn't copy to a card");
        e.ask = None;
        e.act(Action::Ask(Ask::New), false);
        type_in(&mut e, "card a");
        e.key(Key::Enter);
        e.act(Action::StatStep(0, 1), false);
        e.act(Action::Ask(Ask::Copy), false);
        type_in(&mut e, "card b");
        e.key(Key::Enter);
        assert_eq!(e.sprite.name, "CARD_B_FRONT");
        let f = last_harvest_game::card_fields(e.sprite.card.as_deref().unwrap()).unwrap();
        assert_eq!((f.id.as_str(), f.numbers[0].1), ("b", 15.0), "the copy keeps unsaved stats");
        assert!(dir.join("enemies/b.enemy.png").exists() && dir.join("enemies/a.enemy.png").exists());
    }

    #[test]
    fn with_nothing_open_the_editor_waits_for_a_pick() {
        let mut e = editor_on("");
        assert!(e.none_open() && e.surfaces().is_empty());
        e.draw(&mut Canvas::default());
        assert!(!e.on_canvas([CANVAS.x + 10.0, CANVAS.y + 10.0]));
        e.act(Action::Save, false);
        e.act(Action::AddFrame, false);
        assert!(e.status.as_ref().is_some_and(|(t, _)| t.contains("Pick a sprite")));
        assert!(!e.unsaved() && e.names.iter().all(|(_, saved)| !saved), "nothing was written");
        e.key(Key::Char('j'));
        assert!(!e.none_open(), "J opens the first sprite in the list");
    }

    #[test]
    fn delete_card_takes_two_presses_and_leaves_nothing_open() {
        let dir = tmp().join("delete");
        let _ = std::fs::remove_dir_all(&dir);
        let mut e = Editor::new(Box::new(FsStore { dir: dir.join("sprites"), catalog: dir.join("art.txt"), cards: dir.join("enemies") }), "");
        e.act(Action::Ask(Ask::New), false);
        type_in(&mut e, "card gone");
        e.key(Key::Enter);
        e.delete_card();
        assert!(dir.join("enemies/gone.enemy.png").exists(), "one press only asks");
        e.tick(DELETE_CONFIRM + 0.1);
        e.delete_card();
        assert!(dir.join("enemies/gone.enemy.png").exists(), "the question times out");
        e.delete_card();
        assert!(!dir.join("enemies/gone.enemy.png").exists());
        assert!(e.none_open() && !e.names.iter().any(|(n, _)| n.starts_with("CARD_GONE")));
        assert!(!e.buttons().iter().any(|b| matches!(b.action, Action::Frame(_) | Action::AddFrame | Action::Width(_) | Action::Play)), "nothing open shows no frame controls");
        e.draw(&mut Canvas::default());
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
    fn a_sprite_with_no_stand_shows_the_depth_the_game_gives_it() {
        let mut e = editor();
        e.switch_to("BUSH".into());
        e.decide(Choice::Discard);
        let game = last_harvest_game::default_prop_depth(16.0, 16.0, Some(last_harvest_game::art::BUSH), 0);
        assert_eq!((e.sprite.name.as_str(), e.stand()), ("BUSH", Stand::Prop { depth: game }));
        e.switch_to("MAILBOX".into());
        e.decide(Choice::Discard);
        assert_eq!(e.depth, last_harvest_game::default_prop_depth(16.0, 16.0, Some(last_harvest_game::art::MAILBOX), 0));
    }

    #[test]
    fn the_data_page_steps_types_adds_and_saves_only_what_the_game_reads() {
        let dir = tmp().join("data");
        std::fs::create_dir_all(&dir).unwrap();
        let mut e = editor_in(&dir, dir.join("art.txt"));
        assert!(e.names.iter().any(|(n, _)| n == "DATA_TUNING_PLAYER") && e.names.iter().any(|(n, _)| n == "DATA_POWERUPS_PISTOL"));
        assert_eq!(sprite::section("DATA_TUNING_PLAYER"), "Data/tuning");
        assert_eq!((list_label("DATA_GROUND__67"), list_label("DATA_GROUND__47"), list_label("DATA_TUNING_PLAYER")), ("[g]".into(), "[G]".into(), "PLAYER".into()));
        assert!(["DATA_GROUND__2E", "DATA_GROUND__3D", "DATA_GROUND__67"].iter().all(|n| e.names.iter().any(|(m, _)| m == n)), "ground codes list apart");
        e.switch_to("DATA_TUNING_PLAYER".into());
        e.act(Action::DataStep(0, 1), false);
        assert!(e.unsaved());
        buttons_fit(&e);
        e.act(Action::DataEdit(1), false);
        e.set_text("52");
        e.key(Key::Enter);
        e.act(Action::DataAdd, false);
        e.set_text("wings 2");
        e.key(Key::Enter);
        e.act(Action::Save, false);
        assert!(e.unsaved(), "the game has no wings to read");
        let last = e.data.as_ref().map_or(0, |d| d.table.entries(&d.section).len() - 1);
        e.act(Action::DataRemove(last), false);
        e.key(Key::Save);
        assert!(!e.unsaved());
        let saved = std::fs::read_to_string(dir.join("tuning.txt")).unwrap();
        assert!(saved.contains("hp 101          # health") && saved.contains("speed 52"), "{saved}");
        assert_eq!(last_harvest_game::tuning::parse(&saved, None).unwrap().player_speed, 52.0);
    }

    #[test]
    fn the_mod_editor_makes_a_prop_from_its_name() {
        let dir = tmp().join("mod-prop");
        let mut e = editor_in(&dir, dir.join("art.txt"));
        e.set_modding();
        e.act(Action::Ask(Ask::New), false);
        e.set_text("PROP_POST");
        e.key(Key::Enter);
        assert_eq!(e.sprite.name, "PROP_POST");
        assert!(e.is_prop() && e.open_prop_fields().is_some(), "a prop, not a card");
        e.act(Action::Save, false);
        assert!(dir.join("props/post.prop.png").exists());
    }

    #[test]
    fn a_sprite_links_to_the_data_that_says_how_it_plays() {
        let mut e = editor_on("HOUSE");
        assert_eq!(e.data_link.as_deref(), Some("DATA_PROPS_HOUSE"));
        e.act(Action::Data, false);
        assert_eq!(e.sprite.name, "DATA_PROPS_HOUSE");
        e.switch_to("SLASH".into());
        assert_eq!(e.data_link.as_deref(), Some("DATA_EFFECTS_SLASH"));
        e.switch_to("PLAYER_FRONT".into());
        assert_eq!(e.data_link, None);
    }

    #[test]
    fn a_slab_floats_as_set() {
        let mut e = editor();
        e.act(Action::Stand(1), false);
        e.act(Action::Float(1), true);
        buttons_fit(&e);
        assert_eq!(e.stand(), Stand::Slab { height: e.height, lift: 8.0 });
        assert_eq!(e.stand_text(), format!("slab {} float 8", e.height));
        assert_eq!(sprite::parse_stand(&e.stand_text()), Some(e.stand()));
    }

    #[test]
    fn turned_stands_up_the_east_view_alone() {
        let mut e = editor();
        e.act(Action::Stand(TURNED), false);
        let outline = e.outline_width();
        assert_eq!(e.depth, outline, "it starts as thick as it is seen from above");
        e.act(Action::Depth(1), false);
        assert_eq!((e.view, e.stand_text()), (Some(1), format!("turned {}", outline + 1.0)), "it opens the east view to draw, its depth as set");
        for _ in 0..8 {
            e.act(Action::AddFrame, false);
        }
        buttons_fit(&e);
        let views: Vec<Action> = e.buttons().iter().map(|b| b.action).filter(|a| matches!(a, Action::View(_) | Action::AddView(FOOTPRINT))).collect();
        assert_eq!(views, [Action::View(1)], "only the east view shows");
        let n = e.surfaces().len();
        e.set_live((0..n).map(|i| Art(i as u16)).collect());
        assert_eq!(e.stand(), Stand::Turned { east: Art(e.sprite.frames.len() as u16 + 1), depth: Some(outline + 1.0) });
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
        assert!(tmp().join("BARREL/0.png").exists());
        let _ = std::fs::remove_dir_all(tmp());
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
    fn a_paste_from_low_on_the_sprite_lands_at_the_bottom_of_a_shorter_view() {
        let mut e = editor();
        e.sprite.frames[0] = Frame::blank(48, 32);
        e.act(Action::Stand(CARVE), false);
        e.key(Key::Char('s'));
        e.mouse_down(at(&e, 2, 18), false, false);
        e.drag(at(&e, 45, 31));
        e.mouse_up();
        e.key(Key::Copy);
        e.act(Action::View(0), false);
        e.key(Key::Paste);
        let sel = e.selection.as_ref().unwrap();
        assert_eq!((sel.x, sel.y + sel.h as i32), (2, e.current().h as i32), "taller than the view, so its bottom row is the view's");
        e.key(Key::Escape);
        e.key(Key::Char('s'));
        e.mouse_down(at(&e, 0, 0), false, false);
        e.drag(at(&e, 3, 3));
        e.mouse_up();
        e.key(Key::Copy);
        e.act(Action::Frame(0), false);
        e.key(Key::Paste);
        let sel = e.selection.as_ref().unwrap();
        assert_eq!((sel.x, sel.y), (0, 0), "a paste that fits stays where it was copied from");
    }

    #[test]
    fn the_picker_hands_back_the_tool_it_took_over_from() {
        let mut e = editor();
        e.sprite.frames[0] = Frame::blank(16, 16);
        e.sprite.frames[0].set(4, 4, [10, 200, 30, 255]);
        e.act(Action::Tool(Tool::Eraser), false);
        e.act(Action::Tool(Tool::Picker), false);
        click_pixel(&mut e, 0, 0);
        assert_eq!(e.tool, Tool::Picker, "a clear pixel gives no color, so it stays");
        click_pixel(&mut e, 4, 4);
        assert_eq!((e.color, e.tool), ([10, 200, 30, 255], Tool::Eraser));
    }

    #[test]
    fn compare_lays_out_the_family_and_a_tap_opens_another() {
        let mut e = editor_on("KID_SIDE");
        assert!(e.compare_layout().is_none());
        e.key(Key::Char('c'));
        let names: Vec<&str> = e.family.iter().map(|(n, _)| n.as_str()).collect();
        assert!(["KID_FRONT", "KID_SIDE", "KID_BACK"].iter().all(|n| names.contains(n)), "{names:?}");
        let (origins, zoom) = e.compare_layout().unwrap();
        let back = e.family.iter().position(|(n, _)| n == "KID_BACK").unwrap();
        let mine = e.family.iter().position(|(n, _)| n == "KID_SIDE").unwrap();
        assert_eq!(e.grid(), (origins[mine], zoom), "drawing lands on the open one");
        e.mouse_down([origins[back][0] + zoom, origins[back][1] + zoom], false, false);
        assert_eq!(e.sprite.name, "KID_BACK");
        assert!(e.compare && e.family.iter().any(|(n, _)| n == "KID_SIDE"), "still comparing, from the new one");
    }

    #[test]
    fn onion_shows_the_frame_before_wrapping_round() {
        let mut e = editor();
        e.sprite.frames = vec![Frame::blank(16, 16), Frame::blank(16, 16), Frame::blank(16, 16)];
        assert!(e.onion_frame().is_none());
        e.key(Key::Char('o'));
        e.act(Action::Frame(0), false);
        assert!(std::ptr::eq(e.onion_frame().unwrap(), &e.sprite.frames[2]));
        e.act(Action::Frame(2), false);
        assert!(std::ptr::eq(e.onion_frame().unwrap(), &e.sprite.frames[1]));
    }

    #[test]
    fn a_figure_gets_a_top_strip_that_starts_as_the_3d_view_would_color_it() {
        let mut e = editor();
        let mut f = Frame::blank(3, 4);
        for (x, y, p) in [(0, 0, [10, 10, 10, 255]), (0, 1, [250, 250, 250, 255]), (1, 1, [10, 10, 10, 255]), (1, 2, [10, 10, 10, 255])] {
            f.set(x, y, p);
        }
        e.sprite.frames = vec![f];
        e.act(Action::Stand(FIGURE), false);
        assert!(e.buttons().iter().any(|b| b.action == Action::AddTop));
        e.act(Action::AddTop, false);
        e.draw(&mut Canvas::default());
        let top = e.sprite.top.clone().unwrap();
        assert_eq!((top.w, top.h, e.view), (3, 1, Some(TOP)));
        assert_eq!(top.get(0, 0), Some([250, 250, 250, 255]), "past a one-pixel outline");
        assert_eq!(top.get(1, 0), Some([10, 10, 10, 255]), "dark all the way down stays dark");
        assert_eq!(top.get(2, 0), Some(CLEAR), "an empty column leaves the 3D view's own");
        e.act(Action::Width(1), false);
        assert_eq!(e.view, Some(TOP), "a view can't be resized");
        e.act(Action::Frame(0), false);
        e.act(Action::Width(1), false);
        assert_eq!(e.sprite.top.as_ref().unwrap().w, 4, "it follows the frames' width");
        e.act(Action::View(TOP), false);
        e.act(Action::DeleteFrame, false);
        assert!(e.sprite.top.is_none() && e.view.is_none());
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
        assert!(e.buttons().iter().any(|b| b.action == Action::AddView(FOOTPRINT)));
        e.act(Action::AddView(FOOTPRINT), false);
        assert_eq!(e.view, Some(FOOTPRINT));
        assert_eq!(e.surfaces().len(), n + 3);
        e.tool = Tool::Eraser;
        click_pixel(&mut e, 3, 3);
        assert_eq!(e.sprite.footprint.as_ref().unwrap().get(3, 3), Some(CLEAR));
        assert_ne!(e.sprite.frames[0].get(3, 3), Some(CLEAR), "the sprite keeps its colors");
        e.set_live((0..n + 3).map(|i| Art(i as u16)).collect());
        assert!(matches!(e.stand(), Stand::Carve { footprint: Some(_), .. }));
        e.color = e.sprite.frames[0].px.iter().copied().find(|p| p[3] > 0).unwrap();
        e.act(Action::ClearIn3d, false);
        e.act(Action::PaintColor, false);
        e.act(Action::Coats, false);
        e.act(Action::AddCoat, false);
        let packed = e.surfaces().len();
        e.set_live((0..packed).map(|i| Art(i as u16)).collect());
        assert!(matches!(e.stand(), Stand::Carve { footprint: Some(_), .. }), "3D copies and Coats rows packed after it keep it carved");
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
        e.act(Action::AddView(FOOTPRINT), false);
        e.act(Action::Save, false);
        let back = Sprite::load(&tmp(), "BARREL").unwrap();
        assert_eq!(back.footprint, e.sprite.footprint);
        assert!(back.views.is_some());
        let _ = std::fs::remove_dir_all(tmp());
    }

    #[test]
    fn a_carving_takes_a_top_and_north_and_west_views_of_its_own() {
        let mut e = editor();
        e.act(Action::Stand(CARVE), false);
        let n = e.sprite.frames.len();
        for v in [WEST, ROOF, NORTH] {
            assert!(e.buttons().iter().any(|b| b.action == Action::AddView(v)));
            e.act(Action::AddView(v), false);
            assert_eq!(e.view, Some(v));
        }
        let [south, east] = e.sprite.views.clone().unwrap();
        assert_eq!((e.sprite.north.as_ref(), e.sprite.west.as_ref()), (Some(&south.mirrored()), Some(&east.mirrored())), "each starts as its opposite seen from behind");
        assert_eq!((e.surface_of(ROOF), e.surface_of(NORTH), e.surface_of(WEST)), (n + 2, n + 3, n + 4), "packed in a fixed order, whatever order they were added in");
        assert_eq!(e.surfaces().len(), n + 5);
        e.set_live((0..n + 5).map(|i| Art(i as u16)).collect());
        let at = |k: usize| Some(Art(k as u16));
        assert_eq!(e.stand(), Stand::Carve { footprint: None, roof: at(n + 2), south: Art(n as u16), east: Art(n as u16 + 1), north: at(n + 3), west: at(n + 4) });
        e.act(Action::View(NORTH), false);
        e.act(Action::Height(1), false);
        assert_eq!(e.sprite.west.as_ref().unwrap().h, e.sprite.views.as_ref().unwrap()[0].h, "the sides keep the carving's height");
        e.act(Action::Width(1), false);
        e.act(Action::Frame(0), false);
        e.act(Action::Width(2), false);
        let f = &e.sprite.frames[0];
        assert_eq!((e.sprite.roof.as_ref().unwrap().w, e.sprite.north.as_ref().unwrap().w, e.sprite.west.as_ref().unwrap().w), (f.w, f.w, f.h));
        e.act(Action::Save, false);
        let back = Sprite::load(&tmp(), "BARREL").unwrap();
        assert_eq!((&back.roof, &back.north, &back.west), (&e.sprite.roof, &e.sprite.north, &e.sprite.west));
        e.act(Action::View(ROOF), false);
        e.act(Action::DeleteFrame, false);
        assert!(e.sprite.roof.is_none() && e.view.is_none());
        e.key(Key::Undo);
        assert!(e.sprite.roof.is_some());
        let _ = std::fs::remove_dir_all(tmp());
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

    fn type_keys(e: &mut Editor, text: &str) {
        text.chars().for_each(|c| e.key(Key::Char(c)));
    }

    #[test]
    fn slash_searches_the_list_and_j_steps_through_the_matches() {
        let mut e = editor();
        e.key(Key::Char('/'));
        assert!(e.wants_text());
        type_keys(&mut e, "car_");
        // Saved sprites with CAR_ in their names can join the two in the catalog.
        let found: Vec<String> = e.listed().iter().map(|(n, _)| n.clone()).collect();
        assert_eq!(found[..2], ["CAR_H", "CAR_V"]);
        assert!(found.iter().all(|n| n.contains("CAR_")));
        e.key(Key::Enter);
        assert_eq!(e.sprite.name, "CAR_H");
        assert!(!e.wants_text());
        e.key(Key::Char('j'));
        assert_eq!(e.sprite.name, "CAR_V");
        for _ in 1..found.len() {
            e.key(Key::Char('j'));
        }
        assert_eq!(e.sprite.name, "CAR_H", "it wraps within the matches");
        e.key(Key::Escape);
        assert!(e.filter.is_empty() && e.listed().len() == e.names.len());
    }

    #[test]
    fn a_new_copy_shows_in_the_list_before_it_is_saved() {
        let mut e = editor();
        e.act(Action::Ask(Ask::Copy), false);
        type_keys(&mut e, "barrel two");
        e.key(Key::Enter);
        assert!(e.names.iter().any(|(n, saved)| n == "BARREL_TWO" && !saved));
        e.key(Key::Char('/'));
        type_keys(&mut e, "two");
        assert_eq!(e.listed().len(), 1);
    }

    #[test]
    fn holding_the_picker_picks_without_painting_and_lets_go_to_the_tool() {
        let mut e = editor();
        let before = e.current().clone();
        let c = before.px.iter().copied().find(|p| p[3] > 0).unwrap();
        let i = before.px.iter().position(|p| *p == c).unwrap() as i32;
        e.peek(true);
        click_pixel(&mut e, i % 16, i / 16);
        assert_eq!(e.color, c);
        assert_eq!(*e.current(), before, "a held pick paints nothing");
        e.peek(false);
        assert_eq!(e.tool_now(), Tool::Pencil);
    }

    #[test]
    fn shift_click_draws_a_line_from_the_last_pixel_in_one_undo() {
        let mut e = editor();
        e.sprite.frames[0] = Frame::blank(16, 16);
        e.color = [5, 5, 5, 255];
        click_pixel(&mut e, 1, 1);
        e.mouse_down(at(&e, 7, 1), false, true);
        e.mouse_up();
        assert!((1..=7).all(|x| e.current().get(x, 1) == Some([5, 5, 5, 255])));
        e.key(Key::Undo);
        assert_eq!(e.current().get(4, 1), Some(CLEAR));
        assert_eq!(e.current().get(1, 1), Some([5, 5, 5, 255]), "the first dot was its own step");
    }

    /// An editor on a sprite with two commits: BARREL with pixel (0, 0) red, then green; its files now say blue.
    fn with_history(tag: &str) -> (Editor, PathBuf) {
        use crate::history::tests::{commit, repo};
        let root = repo(tag);
        let dir = root.join("sprites");
        let mut s = Sprite::load(&dir, "BARREL").unwrap();
        for (c, message) in [([255, 0, 0, 255], "red"), ([0, 255, 0, 255], "green")] {
            s.frames[0].set(0, 0, c);
            s.save(&dir).unwrap();
            commit(&root, message);
        }
        s.frames[0].set(0, 0, [0, 0, 255, 255]);
        s.save(&dir).unwrap();
        (editor_in(&dir, root.join("art.txt")), root)
    }

    #[test]
    fn a_cards_part_shows_the_history_of_its_card() {
        use crate::history::tests::{commit, repo};
        let root = repo("card");
        let dir = root.join("sprites");
        let mut e = Editor::new(Box::new(FsStore { dir: dir.clone(), catalog: root.join("art.txt"), cards: root.join("enemies") }), "BARREL");
        e.act(Action::Ask(Ask::New), false);
        type_in(&mut e, "card ghoul");
        e.key(Key::Enter);
        let part = e.sprite.name.clone();
        for (c, message) in [([255, 0, 0, 255], "red"), ([0, 255, 0, 255], "green"), ([0, 0, 255, 255], "")] {
            e.sprite.frames[0].set(0, 0, c);
            e.dirty = true;
            e.act(Action::Save, false);
            if !message.is_empty() {
                commit(&root, message);
            }
        }
        e.key(Key::Char('r'));
        let b = e.browse.as_ref().unwrap_or_else(|| panic!("{:?}", e.status));
        assert_eq!(b.commits.iter().map(|c| c.subject.as_str()).collect::<Vec<_>>(), ["green", "red"]);
        assert!(b.changed, "the blue on disk isn't committed");
        assert_eq!((e.sprite.name.as_str(), e.current().get(0, 0)), (part.as_str(), Some([0, 255, 0, 255])));
        e.key(Key::Char('j'));
        assert_eq!(e.current().get(0, 0), Some([255, 0, 0, 255]));
        assert!(e.sprite.card.is_some(), "the version keeps its card's text");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn history_shows_each_commit_and_back_returns_to_the_work() {
        let (mut e, root) = with_history("browse");
        click_pixel(&mut e, 1, 1);
        let work = e.sprite.frames.clone();
        e.key(Key::Char('r'));
        let b = e.browse.as_ref().unwrap();
        assert_eq!(b.commits.iter().map(|c| c.subject.as_str()).collect::<Vec<_>>(), ["green", "red"]);
        assert!(b.changed, "the blue on disk isn't committed");
        assert_eq!(e.current().get(0, 0), Some([0, 255, 0, 255]));
        e.key(Key::Char('j'));
        assert_eq!(e.current().get(0, 0), Some([255, 0, 0, 255]));
        let before = e.current().clone();
        click_pixel(&mut e, 2, 2);
        assert_eq!(*e.current(), before, "a version on show can't be painted");
        e.act(Action::Save, false);
        assert!(e.browse.is_some(), "nor saved");
        e.key(Key::Escape);
        assert!(e.browse.is_none());
        assert_eq!(e.sprite.frames, work);
        assert!(e.dirty);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn restoring_a_commit_is_one_undo_and_saving_writes_it() {
        let (mut e, root) = with_history("restore");
        e.key(Key::Char('r'));
        e.key(Key::Char('j'));
        e.key(Key::Enter);
        assert!(e.browse.is_none() && e.unsaved());
        assert_eq!(e.current().get(0, 0), Some([255, 0, 0, 255]));
        e.key(Key::Undo);
        assert_eq!(e.current().get(0, 0), Some([0, 0, 255, 255]));
        e.key(Key::Redo);
        e.act(Action::Save, false);
        let saved = Sprite::load(&root.join("sprites"), "BARREL").unwrap();
        assert_eq!(saved.frames[0].get(0, 0), Some([255, 0, 0, 255]));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn pinching_magnifies_about_the_fingers_and_opening_a_sprite_fits_it() {
        let mut e = editor();
        let p = at(&e, 3, 4);
        e.zoom_canvas(p, 3.0, [0.0; 2]);
        assert_eq!(e.pixel_at(p), (3, 4), "the pixel under the fingers stays there");
        assert!(e.grid().1 > 30.0);
        e.zoom_canvas(p, 1.0, [-40.0, 0.0]);
        assert_eq!(e.pixel_at([p[0] - 40.0, p[1]]), (3, 4));
        e.zoom_canvas(p, 1.0, [1e6, 1e6]);
        let (o, zoom) = e.grid();
        assert!(o[0] < CANVAS.x + CANVAS.w && o[1] + 16.0 * zoom > CANVAS.y, "some of it stays in sight");
        let mut big = Canvas::default();
        e.draw(&mut big);
        assert!(big.shapes.iter().all(|s| !matches!(s, last_harvest_game::Shape::Rect { x, w, .. } if *x < CANVAS.x && *x + *w > CANVAS.x + 1.0 && *w < CANVAS.w)), "pixels stay inside the canvas");
        e.key(Key::Char('j'));
        assert_eq!((e.lens, e.pan), (1.0, [0.0; 2]));
    }

    #[test]
    fn the_list_folds_into_headings_and_opens_where_you_are() {
        let mut e = editor();
        let rows = |e: &Editor| e.rows().iter().map(|r| match r {
            Row::Heading { title, .. } => format!("# {title}"),
            Row::Sprite((n, _)) => n.clone(),
        }).collect::<Vec<_>>();
        let here = sprite::section(&e.sprite.name);
        let shown = rows(&e);
        assert!(shown.iter().filter(|r| !r.starts_with('#')).all(|n| sprite::section(n) == here), "only the open sprite's group is unfolded");
        assert!(shown.contains(&"# Ground".to_string()) && shown.contains(&"# Characters".to_string()));
        let ground = shown.iter().position(|r| r == "# Ground").unwrap();
        e.mouse_down([LIST.x + 10.0, e.list_top() + (ground as f32 + 0.5) * LIST_ROW - e.scroll], false, false);
        assert!(rows(&e).contains(&"DIRT".to_string()), "tapping a heading unfolds it");
        let tap = |e: &mut Editor, heading: &str| {
            let at = rows(e).iter().position(|r| r == heading).unwrap();
            e.scroll = at as f32 * LIST_ROW;
            e.mouse_down([LIST.x + 10.0, e.list_top() + (at as f32 + 0.5) * LIST_ROW - e.scroll], false, false);
        };
        let data = |e: &Editor| rows(e).into_iter().filter(|r| r.starts_with("# Data")).collect::<Vec<_>>();
        assert_eq!(data(&e), ["# Data"], "the data files fold under one heading");
        tap(&mut e, "# Data");
        assert_eq!(data(&e).len(), 1 + last_harvest_game::TABLES.len(), "a folder per data file");
        assert!(!rows(&e).contains(&"DATA_TUNING_PLAYER".to_string()));
        tap(&mut e, "# Data/tuning");
        assert!(rows(&e).contains(&"DATA_TUNING_PLAYER".to_string()), "a data file's folder unfolds its sections");
        e.key(Key::Char('/'));
        e.set_text("player_b");
        assert!(rows(&e).contains(&"PLAYER_BACK".to_string()), "a search shows matches in folded groups");
    }

    #[test]
    fn typed_text_arrives_whole_and_dragging_scrolls_the_list() {
        let mut e = editor();
        e.key(Key::Char('/'));
        e.set_text("car h!");
        assert_eq!(e.text(), Some("CAR_H"));
        e.key(Key::Escape);
        assert_eq!(e.text(), None);
        e.unfolded = e.names.iter().flat_map(|(n, _)| sprite::section_path(sprite::section(n))).collect();
        e.scroll = 0.0;
        e.drag_list(-200.0);
        assert_eq!(e.scroll, 200.0);
        e.drag_list(500.0);
        assert_eq!(e.scroll, 0.0);
        assert!(e.on_list([LIST.x + 10.0, LIST.y + 10.0]));
        e.act(Action::Ask(Ask::New), false);
        assert!(!e.on_list([LIST.x + 10.0, LIST.y + 10.0]) && !e.on_canvas([CANVAS.x + 10.0, CANVAS.y + 10.0]));
        e.set_text("big rock");
        e.key(Key::Enter);
        assert_eq!(e.sprite.name, "BIG_ROCK");
    }

    #[test]
    fn a_props_buttons_set_how_it_stands_in_the_way() {
        let dir = tmp().join("tough");
        let _ = std::fs::remove_dir_all(&dir);
        let mut e = Editor::new(Box::new(FsStore { dir: dir.join("sprites"), catalog: dir.join("art.txt"), cards: dir.join("enemies") }), "BARREL");
        e.act(Action::Ask(Ask::New), false);
        type_in(&mut e, "prop crate");
        e.key(Key::Enter);
        assert!(!e.is_card() && e.buttons().iter().any(|b| b.action == Action::Stand(0)), "a prop isn't a card: it stands as a sprite does");
        buttons_fit(&e);
        e.act(Action::Broken, false);
        assert!(!e.open_prop_fields().unwrap().broken, "one frame has none to spare");
        e.act(Action::AddFrame, false);
        e.act(Action::Broken, false);
        e.act(Action::Cover, false);
        e.act(Action::Hp(1), false);
        e.act(Action::Hp(1), true);
        e.act(Action::Open(1), false);
        e.act(Action::Open(1), false);
        e.act(Action::Save, false);
        let saved = last_harvest_pack::prop::read("c", &std::fs::read(dir.join("props/crate.prop.png")).unwrap()).unwrap();
        let f = last_harvest_game::prop_fields(&saved.text).unwrap();
        assert_eq!((f.frames, f.broken, f.cover, f.hp, f.open), (2, true, false, 90.0, 2), "{}", saved.text);
        e.act(Action::Hp(-1), true);
        e.act(Action::Hp(-1), true);
        assert_eq!(e.open_prop_fields().unwrap().hp, 0.0, "no lower than none");
    }

    #[test]
    fn history_explains_when_there_is_none() {
        let mut e = editor();
        e.key(Key::Char('r'));
        assert!(e.browse.is_none());
        assert!(e.status.is_some());
    }
}
