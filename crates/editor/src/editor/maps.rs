//! Hand-made maps: opened from the list as `MAP_<ID>`, drawn from above on the canvas by the game's own map
//! drawing, and painted with ground, props, fences, the spawn and home.

use last_harvest_game::art::Art;
use last_harvest_game::{MAP_GROUND, MAP_PROPS, MapSource, PROP_MAX_HP, PropFields, PropLine, SCREEN_H, SCREEN_W};

use super::*;

const TILE: f32 = 16.0;

/// Files by name, or by their path in a pack.
type Files = Vec<(String, Vec<u8>)>;

/// What the picker offers: players' props as packed for the previews, then the game's own, each with its sprite
/// name, walk-through rows and a frame to show.
fn pickable() -> Vec<(String, usize, Option<Art>)> {
    let own = MAP_PROPS.iter().map(|k| (k.name.to_string(), k.open, last_harvest_game::art::ALL.iter().find(|(a, _)| *a == k.name).map(|(_, f)| f[0])));
    let players = last_harvest_game::named_props().into_iter().map(|p| (p.name, p.open, p.frames.first().copied()));
    players.chain(own).collect()
}

/// What a press on the map does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapTool {
    Paint,
    Prop,
    Move,
    Erase,
    Fence,
    Spawn,
    Home,
}

const MAP_TOOLS: [(MapTool, &str); 7] = [
    (MapTool::Paint, "Ground"),
    (MapTool::Prop, "Prop"),
    (MapTool::Move, "Move"),
    (MapTool::Erase, "Erase"),
    (MapTool::Fence, "Fence"),
    (MapTool::Spawn, "Spawn"),
    (MapTool::Home, "Home"),
];
const MAP_SIZE: [usize; 2] = [72, 54];
/// The prop picker: thumbnails across and rows shown, and where it starts.
const PICK_ACROSS: usize = 4;
const PICK_ROWS: usize = 4;
const PICK_TOP: f32 = 194.0;
const HINT_Y: f32 = 174.0;
const PICK_CELL: f32 = 57.0;
const SIZE_Y: f32 = 470.0;
/// The most zoom, in screen pixels to a map pixel.
const MAX_SCALE: f32 = 4.0;
/// Where a prop's hit points and cover show: the selected one's under Move, and an open prop's sprite's under
/// the palette, which keeps this many rows for them.
const TOUGH_Y: f32 = PICK_TOP + 30.0;
pub(super) const PROP_PALETTE_ROWS: usize = 6;
pub(super) const PROP_HP_Y: f32 = 498.0;
const PROP_COVER_Y: f32 = 462.0;

pub(super) struct MapEdit {
    pub id: String,
    pub src: MapSource,
    tool: MapTool,
    ground: u8,
    prop: usize,
    picket: bool,
    pick_scroll: usize,
    /// The map pixel at the canvas's top-left, and screen pixels to a map pixel.
    cam: [f32; 2],
    scale: f32,
    stroke: bool,
    /// The last tile painted, which a shift-click draws a line from.
    last_tile: Option<[usize; 2]>,
    /// The prop being moved, by its place in the list, and the tile it was grabbed by, from its top-left.
    grab: Option<(usize, [usize; 2])>,
    /// The prop Move last picked up or that was last placed, by its place in the list, whose hit points and cover
    /// show.
    selected: Option<usize>,
    fence_from: Option<[usize; 2]>,
    pan_from: Option<[f32; 2]>,
    hover: Option<[usize; 2]>,
    undo: Vec<MapSource>,
    redo: Vec<MapSource>,
    pub dirty: bool,
    /// The previews need the map built again.
    pub changed: bool,
    /// The tile the previews look at.
    pub focus: [usize; 2],
    problem: Option<String>,
    /// How many farms Randomize has laid out, so each press gives a new one.
    rolls: u64,
}

impl MapEdit {
    fn new(id: &str, src: MapSource) -> Self {
        let focus = src.spawn;
        let mut m = Self {
            id: id.into(),
            src,
            tool: MapTool::Paint,
            ground: b'g',
            prop: 0,
            picket: false,
            pick_scroll: 0,
            cam: [0.0; 2],
            scale: 1.0,
            stroke: false,
            last_tile: None,
            grab: None,
            selected: None,
            fence_from: None,
            pan_from: None,
            hover: None,
            undo: Vec::new(),
            redo: Vec::new(),
            dirty: false,
            changed: true,
            focus,
            problem: None,
            rolls: 0,
        };
        m.fit();
        m.check();
        m
    }

    /// Zooms out to the whole map, centred on the canvas.
    fn fit(&mut self) {
        let [w, h] = self.src.size.map(|v| v as f32 * TILE);
        self.scale = (CANVAS.w / w).min(CANVAS.h / h);
        self.cam = [(w - CANVAS.w / self.scale) / 2.0, (h - CANVAS.h / self.scale) / 2.0];
    }

    /// The tile under a point on the pane, if it's on the map.
    fn tile_at(&self, p: [f32; 2]) -> Option<[usize; 2]> {
        let x = (p[0] - CANVAS.x) / self.scale + self.cam[0];
        let y = (p[1] - CANVAS.y) / self.scale + self.cam[1];
        let [w, h] = self.src.size;
        (x >= 0.0 && y >= 0.0 && (x / TILE) < w as f32 && (y / TILE) < h as f32).then(|| [(x / TILE) as usize, (y / TILE) as usize])
    }

    /// Where a tile's top-left lands on the pane.
    pub(super) fn screen(&self, [x, y]: [usize; 2]) -> [f32; 2] {
        [CANVAS.x + (x as f32 * TILE - self.cam[0]) * self.scale, CANVAS.y + (y as f32 * TILE - self.cam[1]) * self.scale]
    }

    fn remember(&mut self) {
        self.undo.push(self.src.clone());
        if self.undo.len() > UNDO_DEPTH {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn edited(&mut self) {
        self.dirty = true;
        self.changed = true;
        self.check();
    }

    /// What would keep the game from loading the map as it stands.
    fn check(&mut self) {
        let file = format!("{}{}", self.id, last_harvest_game::MAP_EXTENSION);
        self.problem = last_harvest_game::check_map(&file, self.src.to_text()).err().map(|e| e.trim_start_matches(&format!("{file}: ")).to_string());
    }

    /// Paints the tile, and for a road the tiles either side of it across the road, so a stroke lays all three lanes.
    fn paint(&mut self, [x, y]: [usize; 2]) {
        let [w, h] = self.src.size;
        let across: &[[i32; 2]] = match self.ground {
            b'v' => &[[-1, 0], [0, 0], [1, 0]],
            b'h' => &[[0, -1], [0, 0], [0, 1]],
            _ => &[[0, 0]],
        };
        let mut changed = false;
        for [dx, dy] in across {
            let (tx, ty) = (x as i32 + dx, y as i32 + dy);
            if (0..w as i32).contains(&tx) && (0..h as i32).contains(&ty) {
                let i = ty as usize * w + tx as usize;
                changed |= self.src.ground[i] != self.ground;
                self.src.ground[i] = self.ground;
            }
        }
        self.last_tile = Some([x, y]);
        if changed {
            self.edited();
        }
    }

    /// Paints every tile on the straight line from `a` to `b`.
    fn paint_line(&mut self, a: [usize; 2], b: [usize; 2]) {
        let (dx, dy) = (b[0] as i32 - a[0] as i32, b[1] as i32 - a[1] as i32);
        let steps = dx.abs().max(dy.abs()).max(1);
        for k in 0..=steps {
            let t = k as f32 / steps as f32;
            let at = [(a[0] as f32 + dx as f32 * t).round() as usize, (a[1] as f32 + dy as f32 * t).round() as usize];
            self.paint(at);
        }
    }

    /// The last prop covering the tile, by its place in the list.
    fn prop_at(&self, [x, y]: [usize; 2]) -> Option<usize> {
        self.src.props.iter().rposition(|(_, p)| {
            let [px, py, pw, ph] = p.rect();
            (px..px + pw).contains(&x) && (py..py + ph).contains(&y)
        })
    }

    /// Moves a prop so its top-left sits on `to`.
    fn move_prop(&mut self, i: usize, to: [usize; 2]) {
        let Some((_, p)) = self.src.props.get_mut(i) else { return };
        let at = match p {
            PropLine::Sprite { at, .. } => at,
            PropLine::Fence { rect, .. } => {
                let [x, y, ..] = rect;
                if [*x, *y] != to {
                    (rect[0], rect[1]) = (to[0], to[1]);
                    self.edited();
                }
                return;
            }
        };
        if *at != to {
            *at = to;
            self.edited();
        }
    }

    /// Takes off the last prop covering the tile; true if one was there.
    fn erase(&mut self, at: [usize; 2]) -> bool {
        self.prop_at(at).map(|i| self.src.props.remove(i)).is_some()
    }

    /// The selected prop's line, if it's still there.
    fn selected_line(&self) -> Option<&PropLine> {
        self.selected.and_then(|i| self.src.props.get(i)).map(|(_, p)| p)
    }

    /// Changes the selected prop's line, as one step to undo.
    fn retough(&mut self, change: impl FnOnce(&mut PropLine)) {
        let Some(i) = self.selected.filter(|&i| i < self.src.props.len()) else { return };
        self.remember();
        change(&mut self.src.props[i].1);
        self.edited();
    }

    fn fence_rect(&self, to: [usize; 2]) -> Option<[usize; 4]> {
        let from = self.fence_from?;
        let (x0, x1) = (from[0].min(to[0]), from[0].max(to[0]));
        let (y0, y1) = (from[1].min(to[1]), from[1].max(to[1]));
        Some([x0, y0, x1 - x0 + 1, y1 - y0 + 1])
    }
}

impl Editor {
    pub(super) fn is_prop(&self) -> bool {
        crate::store::props::id(&self.sprite.name).is_some()
    }

    /// The open prop's text, read.
    pub(super) fn open_prop_fields(&self) -> Option<PropFields> {
        self.sprite.card.as_deref().filter(|_| self.is_prop()).and_then(|t| last_harvest_game::prop_fields(t).ok())
    }

    /// Changes the open prop's text, its frame count kept to its frames so a broken frame always has one.
    pub(super) fn edit_prop(&mut self, change: impl FnOnce(&mut PropFields)) {
        let Some(mut f) = self.open_prop_fields() else { return };
        change(&mut f);
        f.frames = (self.sprite.frames.len() as u32).clamp(1, last_harvest_game::PROP_MAX_FRAMES);
        f.broken &= f.frames > 1;
        self.sprite.card = Some(last_harvest_game::prop_text(&f));
        self.dirty = true;
        self.changed = true;
    }

    /// An open prop's Cover, Broken frame and hit point buttons, under the palette.
    pub(super) fn prop_buttons(&self, push: &mut impl FnMut(f32, f32, f32, f32, Action, &str, bool)) {
        let Some(f) = self.open_prop_fields() else { return };
        push(SIDE_X, PROP_COVER_Y, 114.0, 30.0, Action::Cover, "Cover", f.cover);
        push(SIDE_X + 118.0, PROP_COVER_Y, 114.0, 30.0, Action::Broken, "Broken frame", f.broken);
        push(SIDE_X + 130.0, PROP_HP_Y, 46.0, 30.0, Action::Hp(-1), "-", false);
        push(SIDE_X + 182.0, PROP_HP_Y, 46.0, 30.0, Action::Hp(1), "+", false);
    }

    /// Every player's prop the store keeps, to pack for the previews while a map is open: their frames, in turn,
    /// and each one's fields and frame count.
    pub(super) fn prop_surfaces(&self) -> (Vec<Frame>, Vec<(PropFields, usize)>) {
        let (mut frames, mut props) = (Vec::new(), Vec::new());
        for (name, _) in self.names.iter().filter(|(n, _)| n.starts_with(last_harvest_game::PROP_PREFIX)) {
            let Some(s) = self.store.load(name) else { continue };
            let Some(f) = s.card.as_deref().and_then(|t| last_harvest_game::prop_fields(t).ok()) else { continue };
            props.push((f, s.frames.len()));
            frames.extend(s.frames);
        }
        (frames, props)
    }

    /// Tells the game which packed arts are which player's prop, so maps place them by name, and checks the open
    /// map again now they can.
    pub(super) fn register_props(&mut self) {
        let mut at = 0;
        let mut named = Vec::new();
        for (f, n) in &self.map_props {
            let Some(arts) = self.live.get(at..at + n) else { break };
            named.push((f.clone(), arts.to_vec()));
            at += n;
        }
        last_harvest_game::set_props(named);
        if let Some(m) = &mut self.map {
            m.check();
            m.changed = true;
        }
    }

    /// The open map's file and every player's prop it places, as saved, for Export; None with a word why.
    pub fn export_map(&mut self) -> Option<(String, Files)> {
        let m = self.map.as_ref()?;
        if m.dirty {
            self.say("Save the map first: Export gives the saved copy");
            return None;
        }
        let id = m.id.clone();
        let text = self.store.read_map(&id)?;
        let ext = last_harvest_game::MAP_EXTENSION;
        let mut files = vec![(format!("{}{id}{ext}", last_harvest_pack::MAPS), text.into_bytes())];
        let mut missing = Vec::new();
        for (_, p) in &m.src.props {
            let PropLine::Sprite { name, .. } = p else { continue };
            let Some(pid) = crate::store::props::id(name) else { continue };
            let file = format!("{}{pid}{}", last_harvest_pack::PROPS, last_harvest_pack::prop::EXTENSION);
            if files.iter().any(|(f, _)| *f == file) {
                continue;
            }
            match self.store.prop_bytes(&pid) {
                Some(bytes) => files.push((file, bytes)),
                None => missing.push(name.clone()),
            }
        }
        let props = files.len() - 1;
        let with = if props == 0 { String::new() } else { format!(" with {props} prop{}", if props == 1 { "" } else { "s" }) };
        let note = if missing.is_empty() { String::new() } else { format!("; not kept here: {}", missing.join(", ")) };
        self.say(format!("Exported {id}{with} as {id}.pak: put it in the game's data/mods{note}"));
        Some((format!("{id}.pak"), files))
    }

    /// The open prop's file and PNG as saved, for Export; None with a word why.
    pub fn export_prop(&mut self) -> Option<(String, Vec<u8>)> {
        let id = crate::store::props::id(&self.sprite.name)?;
        if self.unsaved() {
            self.say("Save the prop first: Export gives the saved copy");
            return None;
        }
        let bytes = self.store.prop_bytes(&id)?;
        let file = format!("{id}{}", last_harvest_pack::prop::EXTENSION);
        self.say(format!("Exported {file}"));
        Some((file, bytes))
    }

    /// Every saved map and player's prop by its path in a pack, for Export all.
    pub fn all_maps_and_props(&self) -> Files {
        let maps = self.store.map_ids().into_iter().filter_map(|id| {
            let text = self.store.read_map(&id)?;
            Some((format!("{}{id}{}", last_harvest_pack::MAPS, last_harvest_game::MAP_EXTENSION), text.into_bytes()))
        });
        let props = self.names.iter().filter_map(|(n, _)| crate::store::props::id(n)).filter_map(|id| {
            let bytes = self.store.prop_bytes(&id)?;
            Some((format!("{}{id}{}", last_harvest_pack::PROPS, last_harvest_pack::prop::EXTENSION), bytes))
        });
        maps.chain(props).collect()
    }

    /// Takes in props, then maps, by file name, as Import does from a `.map.txt`, a `.prop.png` or a pack's
    /// `maps/` and `props/`; opens the first map, else the first prop, and says what came in and what didn't.
    pub fn import_maps(&mut self, source: &str, maps: Files, props: Files) {
        if self.unsaved() {
            return self.say("Save or revert first: Import opens what it brings in");
        }
        let (mut took, mut problems, mut first) = (Vec::new(), Vec::new(), None);
        for (file, bytes) in &props {
            let read = last_harvest_pack::prop::read(file, bytes).and_then(|p| last_harvest_game::prop_fields(&p.text).map_err(|e| format!("{file}: {e}")));
            match read.and_then(|f| self.store.put_prop(&f.id, bytes).map(|()| f.id)) {
                Ok(id) => {
                    took.push(crate::store::props::name(&id));
                    first.get_or_insert(crate::store::props::name(&id));
                }
                Err(e) => problems.push(e),
            }
        }
        let mut first_map = None;
        for (file, bytes) in &maps {
            let id = file.strip_suffix(last_harvest_game::MAP_EXTENSION).unwrap_or(file);
            let read = std::str::from_utf8(bytes).map_err(|_| format!("{file}: not text")).and_then(|t| MapSource::parse(t).map(|s| s.to_text()).map_err(|e| format!("{file}: {e}")));
            match read.and_then(|text| if last_harvest_game::valid_map_id(id) { self.store.save_map(id, &text) } else { Err(format!("{file}: a map's file name is lowercase letters, digits and _")) }) {
                Ok(()) => {
                    let name = format!("{}{}", sprite::MAP_PREFIX, id.to_ascii_uppercase());
                    took.push(name.clone());
                    first_map.get_or_insert(name);
                }
                Err(e) => problems.push(e),
            }
        }
        self.names = self.all_names();
        if let Some(name) = first_map.or(first) {
            self.open(&name);
            self.reveal();
        }
        let took = if took.is_empty() { format!("Nothing came in from {source}") } else { format!("Imported {} from {source}", took.join(", ")) };
        let note = problems.first().map_or(String::new(), |p| format!("; left out {}: {p}", problems.len()));
        self.say(format!("{took}{note}"));
    }

    pub(super) fn map_id_of(name: &str) -> Option<String> {
        let id = name.strip_prefix(sprite::MAP_PREFIX)?.to_ascii_lowercase();
        last_harvest_game::valid_map_id(&id).then_some(id)
    }

    /// Every name for the list: the store's sprites, then its maps.
    pub(super) fn all_names(&self) -> Vec<(String, bool)> {
        let mut names = self.store.names();
        names.extend(self.store.map_ids().into_iter().map(|id| (format!("{}{}", sprite::MAP_PREFIX, id.to_ascii_uppercase()), true)));
        names
    }

    /// Opens a map from the store; true if `name` is one.
    pub(super) fn open_map(&mut self, name: &str) -> bool {
        let Some(id) = Self::map_id_of(name) else { return false };
        let Some(text) = self.store.read_map(&id) else { return false };
        match MapSource::parse(&text) {
            Ok(src) => self.start_map(name, &id, src),
            Err(e) => self.say(format!("{name} can't be opened: {e}")),
        }
        true
    }

    fn start_map(&mut self, name: &str, id: &str, src: MapSource) {
        self.map = Some(MapEdit::new(id, src));
        self.sprite = Sprite { name: name.into(), frames: vec![Frame::blank(16, 16)], views: None, footprint: None, top: None, card: None };
        self.frame = 0;
        self.view = None;
        self.stats = false;
        self.selection = None;
        self.undo.clear();
        self.redo.clear();
        self.dirty = false;
        self.stand_touched = false;
        self.changed = true;
    }

    /// Makes a new map of dirt for `name` and saves it, so it lists.
    pub(super) fn make_map(&mut self, name: &str) {
        let Some(id) = Self::map_id_of(name) else {
            return self.say(format!("A map's name is MAP_ then up to {} letters, digits and _", last_harvest_game::PLAYER_ID_MAX));
        };
        if self.store.read_map(&id).is_some() {
            return self.say(format!("{name} already exists"));
        }
        let title: String = id.split('_').filter(|w| !w.is_empty()).map(|w| w[..1].to_ascii_uppercase() + &w[1..]).collect::<Vec<_>>().join(" ");
        let src = MapSource::new(&title, MAP_SIZE);
        if let Err(e) = self.store.save_map(&id, &src.to_text()) {
            return self.say(format!("Couldn't make {name}: {e}"));
        }
        self.ask = None;
        self.names = self.all_names();
        self.start_map(name, &id, src);
        self.reveal();
        self.say(format!("Made {name}: paint ground, place props, and set the spawn and home"));
    }

    pub(super) fn save_map(&mut self) {
        let Some(m) = &mut self.map else { return };
        let text = m.src.to_text();
        match self.store.save_map(&m.id, &text) {
            Ok(()) => {
                m.dirty = false;
                let note = match &m.problem {
                    Some(p) => format!("Saved {}, but the game will leave it out: {p}", self.sprite.name),
                    None => format!("Saved {}. Restart the game to play it.", self.sprite.name),
                };
                self.say(note);
            }
            Err(e) => self.say(format!("Save failed: {e}")),
        }
    }

    /// The map's view for the canvas: the map pixel at the pane's top-left and the scale, for drawing it there.
    pub fn map_view(&self) -> Option<([f32; 2], f32)> {
        let m = self.map.as_ref()?;
        Some(([m.cam[0] - CANVAS.x / m.scale, m.cam[1] - CANVAS.y / m.scale], m.scale))
    }

    /// The open map, for the previews to build.
    pub fn map_source(&self) -> Option<&MapSource> {
        self.map.as_ref().map(|m| &m.src)
    }

    /// Whether the previews need the map built again since last asked, and the tile they look at.
    pub fn take_map_change(&mut self) -> Option<[usize; 2]> {
        let m = self.map.as_mut()?;
        let changed = std::mem::take(&mut m.changed);
        changed.then_some(m.focus)
    }

    pub fn map_focus(&self) -> Option<[usize; 2]> {
        self.map.as_ref().map(|m| m.focus)
    }

    pub(super) fn map_buttons(&self, m: &MapEdit, push: &mut impl FnMut(f32, f32, f32, f32, Action, &str, bool)) {
        for (i, (a, l)) in [(Action::Ask(Ask::New), "New"), (Action::Revert, "Revert"), (Action::Save, "Save")].into_iter().enumerate() {
            push(614.0 + i as f32 * 102.0, 8.0, 94.0, 34.0, a, l, a == Action::Save && m.dirty);
        }
        for (i, (t, l)) in MAP_TOOLS.into_iter().enumerate() {
            push(SIDE_X + (i % 3) as f32 * 79.0, 52.0 + (i / 3) as f32 * 38.0, 74.0, 32.0, Action::MapTool(t), l, m.tool == t);
        }
        match m.tool {
            MapTool::Paint => {
                for (i, (code, l)) in MAP_GROUND.into_iter().enumerate() {
                    push(SIDE_X + (i % 3) as f32 * 79.0, PICK_TOP + (i / 3) as f32 * 34.0, 74.0, 30.0, Action::Ground(code), l, m.ground == code);
                }
            }
            MapTool::Prop => {
                let count = pickable().len();
                for k in 0..PICK_ACROSS * PICK_ROWS {
                    let i = m.pick_scroll * PICK_ACROSS + k;
                    if i < count {
                        let (x, y) = (SIDE_X + (k % PICK_ACROSS) as f32 * PICK_CELL, PICK_TOP + (k / PICK_ACROSS) as f32 * PICK_CELL);
                        push(x, y, PICK_CELL - 4.0, PICK_CELL - 4.0, Action::PickProp(i), "", m.prop == i);
                    }
                }
                if count > PICK_ACROSS * PICK_ROWS {
                    let y = PICK_TOP + PICK_ROWS as f32 * PICK_CELL + 2.0;
                    push(SIDE_X, y, 114.0, 30.0, Action::PickPage(-1), "<", false);
                    push(SIDE_X + 118.0, y, 114.0, 30.0, Action::PickPage(1), ">", false);
                }
            }
            MapTool::Fence => {
                push(SIDE_X, PICK_TOP, 114.0, 30.0, Action::Picket(false), "Rail", !m.picket);
                push(SIDE_X + 118.0, PICK_TOP, 114.0, 30.0, Action::Picket(true), "Picket", m.picket);
            }
            MapTool::Move => {
                if let Some(p) = m.selected_line() {
                    let (_, cover) = p.hp_and_cover();
                    push(SIDE_X + 130.0, TOUGH_Y, 46.0, 30.0, Action::Hp(-1), "-", false);
                    push(SIDE_X + 182.0, TOUGH_Y, 46.0, 30.0, Action::Hp(1), "+", false);
                    push(SIDE_X, TOUGH_Y + 36.0, 114.0, 30.0, Action::Cover, "Cover", cover);
                    push(SIDE_X + 118.0, TOUGH_Y + 36.0, 114.0, 30.0, Action::PropDefaults, "Defaults", false);
                }
            }
            _ => {}
        }
        push(SIDE_X + 130.0, SIZE_Y, 46.0, 30.0, Action::MapSize(-4, 0), "-", false);
        push(SIDE_X + 182.0, SIZE_Y, 46.0, 30.0, Action::MapSize(4, 0), "+", false);
        push(SIDE_X + 130.0, SIZE_Y + 36.0, 46.0, 30.0, Action::MapSize(0, -4), "-", false);
        push(SIDE_X + 182.0, SIZE_Y + 36.0, 46.0, 30.0, Action::MapSize(0, 4), "+", false);
        push(SIDE_X, SIZE_Y + 76.0, 114.0, 30.0, Action::Randomize, "Randomize", false);
        push(SIDE_X + 118.0, SIZE_Y + 76.0, 114.0, 30.0, Action::MapFit, "Fit view", false);
    }

    /// Carries out a map action; false if it isn't one.
    pub(super) fn map_act(&mut self, a: Action) -> bool {
        let Some(m) = &mut self.map else { return false };
        match a {
            Action::MapTool(t) => m.tool = t,
            Action::Ground(code) => m.ground = code,
            Action::PickProp(i) => m.prop = i,
            Action::PickPage(d) => m.scroll_picker(d * PICK_ROWS as i32),
            Action::Picket(on) => m.picket = on,
            Action::MapFit => m.fit(),
            Action::Hp(d) => m.retough(|p| {
                let hp = p.hp_and_cover().0;
                p.tough_mut().hp = Some((hp + d as f32 * 10.0).clamp(0.0, PROP_MAX_HP));
            }),
            Action::Cover => m.retough(|p| {
                let cover = p.hp_and_cover().1;
                p.tough_mut().cover = Some(!cover);
            }),
            Action::PropDefaults => m.retough(|p| *p.tough_mut() = Default::default()),
            Action::Randomize => {
                m.remember();
                m.rolls += 1;
                let seed = m.id.bytes().fold(m.rolls, |h, b| h.wrapping_mul(31).wrapping_add(b as u64));
                let before = m.src.size;
                m.src = MapSource::random(&m.src.name, m.src.size, seed);
                if m.src.size != before {
                    m.fit();
                }
                m.focus = m.src.spawn;
                m.edited();
                let [w, h] = m.src.size;
                let grown = if m.src.size != before { format!(", grown to {w}x{h} to fit one") } else { String::new() };
                self.say(format!("A random farm{grown}. Press again for another, or Undo to go back"));
            }
            Action::MapSize(dw, dh) => {
                m.remember();
                m.src = m.src.resized([dw, dh]);
                m.fit();
                m.edited();
                let [w, h] = m.src.size;
                self.say(format!("{w}x{h} tiles"));
            }
            Action::Save => self.save_map(),
            Action::Revert => {
                let name = self.sprite.name.clone();
                self.open_map(&name);
                self.say("Reverted to the last save");
            }
            _ => return false,
        }
        true
    }

    pub(super) fn map_down(&mut self, p: [f32; 2], right: bool, shift: bool) {
        let Some(m) = &mut self.map else { return };
        if right {
            m.pan_from = Some(p);
            return;
        }
        let Some(at) = m.tile_at(p) else { return };
        m.focus = at;
        m.changed = true;
        m.remember();
        match m.tool {
            MapTool::Paint => {
                m.stroke = true;
                match m.last_tile.filter(|_| shift) {
                    Some(from) => m.paint_line(from, at),
                    None => m.paint(at),
                }
            }
            MapTool::Move => {
                m.selected = m.prop_at(at);
                match m.selected {
                    Some(i) => {
                        let [x, y, ..] = m.src.props[i].1.rect();
                        m.grab = Some((i, [at[0] - x, at[1] - y]));
                    }
                    None => {
                        m.undo.pop();
                    }
                }
            }
            MapTool::Prop => {
                if let Some((name, open, _)) = pickable().into_iter().nth(m.prop) {
                    m.src.props.push((0, PropLine::Sprite { name, at, frame: None, fps: 0.0, open, tough: Default::default() }));
                    m.selected = Some(m.src.props.len() - 1);
                    m.edited();
                }
            }
            MapTool::Erase => {
                m.stroke = true;
                if m.erase(at) {
                    m.edited();
                }
            }
            MapTool::Fence => m.fence_from = Some(at),
            MapTool::Spawn => {
                m.src.spawn = at;
                m.edited();
            }
            MapTool::Home => {
                m.src.home = at;
                m.edited();
            }
        }
    }

    pub(super) fn map_drag(&mut self, p: [f32; 2]) {
        let Some(m) = &mut self.map else { return };
        if let Some(from) = m.pan_from {
            m.cam = [m.cam[0] - (p[0] - from[0]) / m.scale, m.cam[1] - (p[1] - from[1]) / m.scale];
            m.pan_from = Some(p);
            return;
        }
        m.hover = m.tile_at(p);
        if let (Some((i, by)), Some(at)) = (m.grab, m.hover) {
            m.move_prop(i, [at[0].saturating_sub(by[0]), at[1].saturating_sub(by[1])]);
            return;
        }
        let Some(at) = m.hover.filter(|_| m.stroke) else { return };
        match m.tool {
            MapTool::Paint => match m.last_tile {
                Some(from) if from != at => m.paint_line(from, at),
                _ => m.paint(at),
            },
            MapTool::Erase => {
                if m.erase(at) {
                    m.edited();
                }
            }
            _ => {}
        }
    }

    pub(super) fn map_up(&mut self) {
        let Some(m) = &mut self.map else { return };
        if let Some(rect) = m.hover.and_then(|to| m.fence_rect(to)) {
            let [_, _, w, h] = rect;
            if w >= 4 && h >= 3 {
                let gate = Some(((w - 2) / 2).clamp(1, w - 3));
                m.src.props.push((0, PropLine::Fence { picket: m.picket, rect, gate, tough: Default::default() }));
                m.selected = Some(m.src.props.len() - 1);
                m.edited();
            } else {
                m.undo.pop();
                self.say("Drag a fence at least 4 tiles wide and 3 tall");
            }
        }
        let Some(m) = &mut self.map else { return };
        m.stroke = false;
        m.grab = None;
        m.fence_from = None;
        m.pan_from = None;
    }

    /// Scrolls the prop picker over it, else zooms the map about the point.
    pub(super) fn map_wheel(&mut self, p: [f32; 2], dy: f32) {
        let Some(m) = &mut self.map else { return };
        if m.tool == MapTool::Prop && p[0] >= SIDE_X && (PICK_TOP..PICK_TOP + PICK_ROWS as f32 * PICK_CELL).contains(&p[1]) {
            m.scroll_picker(dy.signum() as i32);
        } else if CANVAS.contains(p) {
            m.zoom(p, if dy < 0.0 { 1.25 } else { 0.8 });
        }
    }

    /// Two fingers over the map: zoom about `center` and pan by `pan`.
    pub(super) fn map_pinch(&mut self, center: [f32; 2], factor: f32, pan: [f32; 2]) {
        let Some(m) = &mut self.map else { return };
        m.zoom(center, factor);
        m.cam = [m.cam[0] - pan[0] / m.scale, m.cam[1] - pan[1] / m.scale];
    }

    pub(super) fn map_key(&mut self, key: Key) -> bool {
        let Some(m) = &mut self.map else { return false };
        match key {
            Key::Undo | Key::Redo => {
                let (from, to) = if key == Key::Undo { (&mut m.undo, &mut m.redo) } else { (&mut m.redo, &mut m.undo) };
                if let Some(prev) = from.pop() {
                    to.push(std::mem::replace(&mut m.src, prev));
                    m.edited();
                }
            }
            Key::Save => self.save_map(),
            Key::Left | Key::Right | Key::Up | Key::Down => {
                let step = 4.0 * TILE;
                let d = match key {
                    Key::Left => [-step, 0.0],
                    Key::Right => [step, 0.0],
                    Key::Up => [0.0, -step],
                    _ => [0.0, step],
                };
                m.cam = [m.cam[0] + d[0], m.cam[1] + d[1]];
            }
            _ => return false,
        }
        true
    }

    /// The map pane: masks where the map spills past the canvas, then its marks, the list and the tools.
    pub(super) fn draw_map(&self, c: &mut Canvas, m: &MapEdit) {
        let bg = c.clear;
        c.rect(0.0, 0.0, SCREEN_W, CANVAS.y, bg);
        c.rect(0.0, 0.0, CANVAS.x, SCREEN_H, bg);
        c.rect(CANVAS.x + CANVAS.w, 0.0, SCREEN_W, SCREEN_H, bg);
        c.rect(0.0, CANVAS.y + CANVAS.h, SCREEN_W, SCREEN_H, bg);
        let tile = TILE * m.scale;
        let mark = |c: &mut Canvas, at: [usize; 2], label: &str, color: Color| {
            let [x, y] = m.screen(at);
            if CANVAS.contains([x + tile / 2.0, y + tile / 2.0]) {
                c.rect(x, y, tile, tile, [color[0], color[1], color[2], 0.45]);
                c.label(label, x + tile / 2.0, y + tile / 2.0 - 9.0, 18.0, TEXT, Align::Center);
            }
        };
        mark(c, m.src.spawn, "S", [0.3, 0.6, 1.0, 1.0]);
        mark(c, m.src.home, "H", [0.3, 0.9, 0.4, 1.0]);
        if let Some([x, y, w, h]) = m.selected_line().filter(|_| m.tool == MapTool::Move).map(PropLine::rect) {
            let ([sx, sy], (w, h)) = (m.screen([x, y]), (w as f32 * tile, h as f32 * tile));
            let pick = [0.45, 0.8, 1.0, 1.0];
            for [rx, ry, rw, rh] in [[sx, sy, w, 2.0], [sx, sy + h - 2.0, w, 2.0], [sx, sy, 2.0, h], [sx + w - 2.0, sy, 2.0, h]] {
                c.rect(rx, ry, rw, rh, pick);
            }
        }
        if let Some(at) = m.hover {
            let [x, y] = m.screen(at);
            let (w, h) = match (m.tool, m.fence_rect(at)) {
                (MapTool::Fence, Some([fx, fy, fw, fh])) => {
                    let [sx, sy] = m.screen([fx, fy]);
                    c.rect(sx, sy, fw as f32 * tile, fh as f32 * tile, [1.0, 0.85, 0.3, 0.25]);
                    (0.0, 0.0)
                }
                (MapTool::Prop, _) => {
                    let name = pickable().into_iter().nth(m.prop).map_or_else(String::new, |(n, _, _)| n);
                    let probe = PropLine::Sprite { name, at, frame: None, fps: 0.0, open: 0, tough: Default::default() };
                    let [_, _, pw, ph] = probe.rect();
                    (pw as f32 * tile, ph as f32 * tile)
                }
                _ => (tile, tile),
            };
            if w > 0.0 && CANVAS.contains([x + 1.0, y + 1.0]) {
                c.rect(x, y, w, 2.0, HOT);
                c.rect(x, y + h - 2.0, w, 2.0, HOT);
                c.rect(x, y, 2.0, h, HOT);
                c.rect(x + w - 2.0, y, 2.0, h, HOT);
            }
        }
        c.rect(LIST.x, LIST.y, LIST.w, LIST.h, PANEL);
        self.draw_list(c);
        c.label(format!("{}{}", self.sprite.name, if m.dirty { "  (unsaved)" } else { "" }), 12.0, 12.0, 26.0, TEXT, Align::Left);
        let picks = pickable();
        for b in self.buttons() {
            let r = b.rect;
            c.rect(r.x, r.y, r.w, r.h, if b.on { HOT } else { BUTTON });
            if let Action::PickProp(i) = b.action {
                if let Some(a) = picks.get(i).and_then(|(_, _, a)| *a) {
                    let s = ((r.w - 8.0) / a.w().max(a.h())).min(3.0);
                    c.rect(r.x + 2.0, r.y + 2.0, r.w - 4.0, r.h - 4.0, [0.22, 0.22, 0.25, 1.0]);
                    c.art(a, r.x + (r.w - a.w() * s) / 2.0, r.y + (r.h - a.h() * s) / 2.0, s, false, [1.0; 4]);
                }
                continue;
            }
            c.label(b.label, r.x + r.w / 2.0, r.y + (r.h - 18.0) / 2.0, 18.0, if b.on { [0.1, 0.1, 0.1, 1.0] } else { TEXT }, Align::Center);
        }
        let what = match m.tool {
            MapTool::Paint => "Drag to paint; Shift-click: a line".to_string(),
            MapTool::Move => "Drag a prop or fence to move it; set its HP and cover".to_string(),
            MapTool::Prop => format!("{}: click to place", picks.get(m.prop).map_or("", |(n, _, _)| n.as_str())),
            MapTool::Erase => "Click a prop to take it off".to_string(),
            MapTool::Fence => "Drag a rectangle; the gate goes mid-bottom".to_string(),
            MapTool::Spawn => "Click where the player starts".to_string(),
            MapTool::Home => "Click the tile they run back to at dawn".to_string(),
        };
        c.label(what, SIDE_X, HINT_Y, 15.0, DIM, Align::Left);
        if let Some(p) = m.selected_line().filter(|_| m.tool == MapTool::Move) {
            let [x, y, ..] = p.rect();
            let what = match p {
                PropLine::Sprite { name, .. } => name.clone(),
                PropLine::Fence { picket, .. } => if *picket { "Picket fence" } else { "Rail fence" }.to_string(),
            };
            c.label(format!("{what} at {x} {y}"), SIDE_X, PICK_TOP + 4.0, 16.0, TEXT, Align::Left);
            let (hp, _) = p.hp_and_cover();
            c.label(if hp > 0.0 { format!("HP {hp}") } else { "Won't break".into() }, SIDE_X, TOUGH_Y + 5.0, 18.0, TEXT, Align::Left);
            let own = p.tough() == Default::default();
            c.label(if own { "As its sprite says" } else { "Set on this map" }, SIDE_X, TOUGH_Y + 74.0, 15.0, DIM, Align::Left);
        }
        let [w, h] = m.src.size;
        c.label(format!("Width {w}"), SIDE_X, SIZE_Y + 6.0, 17.0, TEXT, Align::Left);
        c.label(format!("Height {h}"), SIDE_X, SIZE_Y + 42.0, 17.0, TEXT, Align::Left);
        let at = m.hover.map_or(String::new(), |[x, y]| format!("Tile {x} {y}   "));
        c.label(format!("{at}Right-drag or arrows: move   Scroll or pinch: zoom"), CANVAS.x, CANVAS.y + CANVAS.h + 12.0, 16.0, DIM, Align::Left);
        let (line, color) = match &m.problem {
            Some(p) => (format!("The game would leave it out: {p}"), [1.0, 0.5, 0.45, 1.0]),
            None => ("Ready to play".to_string(), [0.5, 0.9, 0.55, 1.0]),
        };
        let mut y = CANVAS.y + CANVAS.h + 36.0;
        for part in wrap(&line, 70).into_iter().take(2) {
            c.label(part, CANVAS.x, y, 16.0, color, Align::Left);
            y += 20.0;
        }
        if let Some((text, _)) = &self.status {
            for part in wrap(text, 70).into_iter().take(2) {
                c.label(part, CANVAS.x, y + 4.0, 16.0, TEXT, Align::Left);
                y += 20.0;
            }
        }
    }
}

/// Breaks `text` into lines of at most `width` characters, between words.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = vec![String::new()];
    for word in text.split(' ') {
        let line = lines.last_mut().unwrap();
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(word.to_string());
        } else {
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
    }
    lines
}

impl MapEdit {
    /// Moves the prop picker `rows` rows on, or back for fewer than none, within what it holds.
    fn scroll_picker(&mut self, rows: i32) {
        let last = pickable().len().div_ceil(PICK_ACROSS).saturating_sub(PICK_ROWS);
        self.pick_scroll = (self.pick_scroll as i32 + rows).clamp(0, last as i32) as usize;
    }

    fn zoom(&mut self, about: [f32; 2], factor: f32) {
        let fit = (CANVAS.w / (self.src.size[0] as f32 * TILE)).min(CANVAS.h / (self.src.size[1] as f32 * TILE));
        let scale = (self.scale * factor).clamp(fit.min(1.0) * 0.8, MAX_SCALE);
        let at = [(about[0] - CANVAS.x) / self.scale + self.cam[0], (about[1] - CANVAS.y) / self.scale + self.cam[1]];
        self.cam = [at[0] - (about[0] - CANVAS.x) / scale, at[1] - (about[1] - CANVAS.y) / scale];
        self.scale = scale;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::FsStore;

    fn store(dir: &std::path::Path) -> Box<FsStore> {
        Box::new(FsStore { dir: dir.join("sprites"), catalog: dir.join("art.txt"), cards: dir.join("enemies") })
    }

    fn click(e: &mut Editor, at: [usize; 2]) {
        let p = e.map.as_ref().unwrap().screen(at);
        let half = TILE * e.map.as_ref().unwrap().scale / 2.0;
        e.mouse_down([p[0] + half, p[1] + half], false, false);
        e.mouse_up();
    }

    #[test]
    fn a_new_map_paints_places_fences_and_saves() {
        let dir = std::env::temp_dir().join(format!("lh-maps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut e = Editor::new(store(&dir), "BARREL");
        e.act(Action::Ask(Ask::New), false);
        "map old farm".chars().for_each(|ch| e.key(Key::Char(ch)));
        e.key(Key::Enter);
        assert_eq!(e.sprite.name, "MAP_OLD_FARM");
        assert!(dir.join("maps/old_farm.map.txt").exists(), "New writes the map so it lists");
        assert!(e.names.iter().any(|(n, _)| n == "MAP_OLD_FARM") && sprite::section("MAP_OLD_FARM") == sprite::MAP_SECTION);
        assert!(e.surfaces().is_empty(), "no sprite to pack while a map is open");

        e.act(Action::MapTool(MapTool::Paint), false);
        e.act(Action::Ground(b'g'), false);
        click(&mut e, [10, 10]);
        assert_eq!(e.map.as_ref().unwrap().src.ground[10 * 72 + 10], b'g');
        e.act(Action::MapTool(MapTool::Prop), false);
        e.act(Action::MapTool(MapTool::Prop), false);
        e.act(Action::PickPage(1), false);
        assert_eq!(e.map.as_ref().unwrap().pick_scroll, PICK_ROWS, "> turns a page");
        assert!(e.buttons().iter().any(|b| b.action == Action::PickPage(-1)), "and < is there to turn back");
        e.act(Action::PickPage(-5), false);
        assert_eq!(e.map.as_ref().unwrap().pick_scroll, 0, "no further back than the start");
        let house = pickable().iter().position(|(n, _, _)| n == "HOUSE").unwrap();
        e.act(Action::PickProp(house), false);
        click(&mut e, [5, 5]);
        e.act(Action::MapTool(MapTool::Spawn), false);
        click(&mut e, [20, 20]);
        e.act(Action::MapTool(MapTool::Fence), false);
        let m = e.map.as_ref().unwrap();
        let (from, to) = (m.screen([30, 30]), m.screen([36, 34]));
        e.mouse_down([from[0] + 1.0, from[1] + 1.0], false, false);
        e.drag([to[0] + 1.0, to[1] + 1.0]);
        e.mouse_up();
        let m = e.map.as_ref().unwrap();
        assert_eq!(m.src.props.last().map(|(_, p)| p.rect()), Some([30, 30, 7, 5]));
        assert!(m.dirty && m.problem.is_none(), "{:?}", m.problem);
        e.key(Key::Undo);
        assert_eq!(e.map.as_ref().unwrap().src.props.len(), 1, "undo takes the fence back off");
        e.key(Key::Redo);

        e.act(Action::MapTool(MapTool::Paint), false);
        e.act(Action::Ground(b'v'), false);
        click(&mut e, [50, 5]);
        let p = e.map.as_ref().unwrap().screen([50, 15]);
        e.mouse_down([p[0] + 1.0, p[1] + 1.0], false, true);
        e.mouse_up();
        let g = &e.map.as_ref().unwrap().src.ground;
        assert!((5..=15).all(|y| (49..=51).all(|x| g[y * 72 + x] == b'v')), "a shift-click road runs all the way, three lanes wide");
        assert_eq!(g[16 * 72 + 50], b'.');
        e.act(Action::Ground(b'.'), false);
        click(&mut e, [60, 40]);
        let p = e.map.as_ref().unwrap().screen([60, 44]);
        e.mouse_down([p[0] + 1.0, p[1] + 1.0], false, true);
        e.mouse_up();

        e.act(Action::MapTool(MapTool::Move), false);
        let m = e.map.as_ref().unwrap();
        let (from, to) = (m.screen([6, 6]), m.screen([12, 9]));
        e.mouse_down([from[0] + 1.0, from[1] + 1.0], false, false);
        e.drag([to[0] + 1.0, to[1] + 1.0]);
        e.mouse_up();
        assert_eq!(e.map.as_ref().unwrap().src.props[0].1.rect()[..2], [11, 8], "the house moves by where it was grabbed");
        e.key(Key::Undo);
        assert_eq!(e.map.as_ref().unwrap().src.props[0].1.rect()[..2], [5, 5], "one undo puts it back");
        e.key(Key::Redo);
        e.key(Key::Undo);

        e.act(Action::Save, false);
        let saved = MapSource::parse(&std::fs::read_to_string(dir.join("maps/old_farm.map.txt")).unwrap()).unwrap();
        assert_eq!((saved.spawn, saved.props.len(), saved.ground[10 * 72 + 10], saved.name.as_str()), ([20, 20], 2, b'g', "Old Farm"));
        assert!(!e.unsaved());

        e.act(Action::MapSize(-4, 4), false);
        assert_eq!(e.map.as_ref().unwrap().src.size, [68, 58]);
        e.act(Action::MapTool(MapTool::Home), false);
        let walled = e.map.as_ref().unwrap().src.size[0] - 1;
        click(&mut e, [walled, 5]);
        assert!(e.map.as_ref().unwrap().problem.as_deref().is_some_and(|p| p.contains("home")), "a home on the edge is a problem");
        let before = e.map.as_ref().unwrap().src.clone();
        e.act(Action::Randomize, false);
        let m = e.map.as_ref().unwrap();
        assert!(m.src.props.len() > 20 && m.problem.is_none() && m.src.size == [68, 58], "a whole farm that plays, at the map's size");
        e.act(Action::Randomize, false);
        assert_ne!(e.map.as_ref().unwrap().src, before, "another press, another farm");
        e.key(Key::Undo);
        e.key(Key::Undo);
        assert_eq!(e.map.as_ref().unwrap().src, before, "undo goes back to the hand-made map");

        let again = Editor::new(store(&dir), "MAP_OLD_FARM");
        assert_eq!(again.map.as_ref().map(|m| m.src.props.len()), Some(2), "opens straight onto a map");
        e.switch_to("BARREL".into());
        e.decide(Choice::Discard);
        assert!(e.map.is_none() && e.sprite.name == "BARREL", "a sprite opens in place of the map");
    }
}

#[cfg(test)]
mod tough_tests {
    use super::*;
    use crate::store::FsStore;

    #[test]
    fn move_picks_a_prop_whose_hit_points_and_cover_it_sets() {
        let dir = std::env::temp_dir().join(format!("lh-tough-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut e = Editor::new(Box::new(FsStore { dir: dir.join("sprites"), catalog: dir.join("art.txt"), cards: dir.join("enemies") }), "BARREL");
        e.act(Action::Ask(Ask::New), false);
        "map pen".chars().for_each(|ch| e.key(Key::Char(ch)));
        e.key(Key::Enter);
        let m = e.map.as_mut().unwrap();
        m.src.props.push((0, PropLine::Fence { picket: false, rect: [30, 30, 7, 5], gate: Some(2), tough: Default::default() }));
        e.act(Action::MapTool(MapTool::Move), false);
        let line = |e: &Editor| e.map.as_ref().unwrap().src.props[0].1.clone();
        e.act(Action::Hp(1), false);
        assert_eq!(line(&e).tough(), Default::default(), "nothing picked, nothing set");
        let p = e.map.as_ref().unwrap().screen([30, 32]);
        e.mouse_down([p[0] + 1.0, p[1] + 1.0], false, false);
        e.mouse_up();
        assert!(e.buttons().iter().any(|b| b.action == Action::PropDefaults), "the picked fence's settings show");
        e.act(Action::Hp(1), false);
        e.act(Action::Cover, false);
        assert_eq!(line(&e).hp_and_cover(), (50.0, true));
        e.act(Action::Save, false);
        let saved = std::fs::read_to_string(dir.join("maps/pen.map.txt")).unwrap();
        assert!(saved.contains("fence rail 30 30 7 5 gate 2 hp 50 cover yes"), "{saved}");
        e.key(Key::Undo);
        assert_eq!(line(&e).hp_and_cover(), (50.0, false), "each press undoes on its own");
        e.act(Action::PropDefaults, false);
        assert_eq!(line(&e).tough(), Default::default());
    }
}

#[cfg(test)]
mod prop_tests {
    use super::*;
    use crate::store::FsStore;

    fn store(dir: &std::path::Path) -> Box<FsStore> {
        Box::new(FsStore { dir: dir.join("sprites"), catalog: dir.join("art.txt"), cards: dir.join("enemies") })
    }

    fn new(e: &mut Editor, name: &str) {
        e.act(Action::Ask(Ask::New), false);
        name.chars().for_each(|ch| e.key(Key::Char(ch)));
        e.key(Key::Enter);
    }

    #[test]
    fn a_map_with_a_players_prop_exports_and_imports_whole() {
        let dir = std::env::temp_dir().join(format!("lh-props-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut e = Editor::new(store(&dir), "BARREL");
        new(&mut e, "prop beacon");
        assert_eq!((e.sprite.name.as_str(), sprite::section("PROP_BEACON")), ("PROP_BEACON", sprite::PROP_SECTION));
        e.sprite.frames = vec![Frame { w: 16, h: 24, px: vec![[200, 40, 40, 255]; 16 * 24] }; 2];
        e.act(Action::Fps(1), false);
        e.dirty = true;
        e.act(Action::Save, false);
        let saved = last_harvest_pack::prop::read("b", &std::fs::read(dir.join("props/beacon.prop.png")).unwrap()).unwrap();
        assert_eq!(saved.frame, [16, 24]);
        assert!(saved.text.contains("frames 2") && saved.text.contains("fps 7"), "{}", saved.text);

        new(&mut e, "map beacon farm");
        let frames = e.surfaces();
        assert_eq!(frames.len(), 2, "the prop's frames pack for the previews");
        e.set_live(vec![Art(1), Art(2)]);
        assert!(e.map.as_ref().unwrap().problem.is_none(), "registering the props checks the map again");
        let picks = pickable();
        let at = picks.iter().position(|(n, _, _)| n == "PROP_BEACON").expect("the picker offers it");
        e.act(Action::MapTool(MapTool::Prop), false);
        e.act(Action::PickProp(at), false);
        let p = e.map.as_ref().unwrap().screen([10, 10]);
        e.mouse_down([p[0] + 1.0, p[1] + 1.0], false, false);
        e.mouse_up();
        assert!(e.map.as_ref().unwrap().problem.is_none(), "{:?}", e.map.as_ref().unwrap().problem);
        e.act(Action::Save, false);
        let (pak, files) = e.export_map().unwrap();
        assert_eq!(pak, "beacon_farm.pak");
        let paths: Vec<&str> = files.iter().map(|(f, _)| f.as_str()).collect();
        assert_eq!(paths, ["maps/beacon_farm.map.txt", "props/beacon.prop.png"]);

        let other = dir.join("elsewhere");
        let mut fresh = Editor::new(store(&other), "BARREL");
        let take = |dir: &str| files.iter().filter(|(f, _)| f.starts_with(dir)).map(|(f, b)| (f[dir.len()..].to_string(), b.clone())).collect::<Vec<_>>();
        fresh.import_maps("beacon_farm.pak", take("maps/"), take("props/"));
        assert_eq!(fresh.sprite.name, "MAP_BEACON_FARM", "the map opens");
        assert!(other.join("props/beacon.prop.png").exists() && other.join("maps/beacon_farm.map.txt").exists());
        assert!(fresh.names.iter().any(|(n, _)| n == "PROP_BEACON"));
    }
}
