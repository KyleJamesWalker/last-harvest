//! The data page: one section of one of the game's data files, such as `tuning.txt`'s [player], a line at a time.
//! Each section lists as its own sprite name, `DATA_TUNING_PLAYER`, under a heading for its file.

use crate::table::{self, Table};

use super::weapons::{self, Weapon};
use super::*;

/// Lines the page shows at once.
const ROWS: usize = 14;
const ROW_TOP: f32 = CANVAS.y + 48.0;
const ROW_H: f32 = 34.0;
/// Characters of a value that fit before the buttons.
const VALUE_CHARS: usize = 24;
/// Files whose sections are ids a modder can add to, by copying one; a ground file's are one-character codes.
const COPYABLE: [&str; 4] = ["powerups.txt", "enemies.txt", "effects.txt", "ground.txt"];

/// The kind of data file `file` is, a farm's by what it replaces: `farm.txt` or `ground.txt`.
fn kind(file: &str) -> &str {
    last_harvest_game::table_kind(file).unwrap_or(file)
}

pub(super) struct DataEdit {
    pub file: String,
    pub section: String,
    pub table: Table,
    pub dirty: bool,
    /// The first line shown.
    pub first: usize,
    /// The line whose value is being typed, by its place in the section.
    pub typing: Option<usize>,
    /// The file's text and the section shown, before each change.
    pub undo: Vec<(String, String)>,
    pub redo: Vec<(String, String)>,
    /// For a powerups.txt weapon, what its weapon page shows.
    pub weapon: Option<Weapon>,
}

impl DataEdit {
    /// Makes one change to the table as a step to undo; true if it changed anything.
    pub(super) fn change(&mut self, f: impl FnOnce(&mut Table, &str) -> bool) -> bool {
        let before = self.table.text.clone();
        if !f(&mut self.table, &self.section) || self.table.text == before {
            return false;
        }
        self.undo.push((before, self.section.clone()));
        if self.undo.len() > UNDO_DEPTH {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
        true
    }
}

/// Whether a line is a shot style's color, which steps through the palette.
fn is_color(d: &DataEdit, key: &str) -> bool {
    d.file == "effects.txt" && key == "color"
}

/// Whether a name in the list is a sprite that can be drawn in the game: the catalog's, or a player's prop.
fn is_paintable(name: &str) -> bool {
    ![sprite::DATA_PREFIX, sprite::MAP_PREFIX, sprite::CARD_PREFIX, sprite::PLAYER_CARD_PREFIX].iter().any(|p| name.starts_with(p))
}

/// Whether a line names one of a shot style's sprites, which step through the sprites.
fn is_sprite(d: &DataEdit, key: &str) -> bool {
    d.file == "effects.txt" && last_harvest_game::SHOT_SPRITES.contains(&key)
}

/// The list name for a section of a data file: `DATA_POWERUPS_PISTOL`, or `DATA_POWERUPS` for the lines before
/// its first heading.
pub(super) fn data_name(file: &str, section: &str) -> String {
    let stem: String = file.trim_end_matches(".txt").chars().map(|c| if c == '/' || c == '.' { '_' } else { c.to_ascii_uppercase() }).collect();
    // A ground code, `.` or `g`, is spelled as its hex: names are capitals, and `g` and `G` are two codes.
    let hex = |c: char| format!("_{:02X}", c as u32);
    let section: String = if kind(file) == "ground.txt" { section.chars().map(hex).collect() } else { section.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c.to_ascii_uppercase().to_string() } else { hex(c) }).collect() };
    if section.is_empty() { format!("{}{stem}", sprite::DATA_PREFIX) } else { format!("{}{stem}_{section}", sprite::DATA_PREFIX) }
}

impl Editor {
    /// The data files kept here: the game's, then each farm's, its rules before its ground.
    pub(super) fn table_files(&self) -> Vec<String> {
        let farms = self.store.farm_ids().into_iter().flat_map(|id| [last_harvest_game::farm_file(&id, false), last_harvest_game::farm_file(&id, true)]);
        last_harvest_game::TABLES.into_iter().map(str::to_string).chain(farms).collect()
    }

    /// Every data file's sections, as names for the list, marked as saved where the file differs from the game's own.
    pub(super) fn data_names(&self) -> Vec<(String, bool)> {
        let mut out = Vec::new();
        for file in self.table_files() {
            let file = file.as_str();
            let Some(text) = self.store.read_table(file) else { continue };
            let changed = last_harvest_game::built_in_table(file) != Some(text.as_str());
            out.extend(Table::new(file, &text).sections().iter().map(|s| (data_name(file, s), changed)));
        }
        out
    }

    /// The data page section that says how a sprite plays: its props.txt section, else the first effects.txt
    /// section that plays it, else the ground.txt code whose crop it is part of.
    pub(super) fn data_link_for(&self, sprite: &str) -> Option<String> {
        let props = self.store.read_table("props.txt").map(|t| Table::new("props.txt", &t));
        if props.is_some_and(|p| p.sections().iter().any(|s| s == sprite)) {
            return Some(data_name("props.txt", sprite));
        }
        let effects = self.store.read_table("effects.txt").map(|t| Table::new("effects.txt", &t));
        let plays = |t: &Table, s: &String| t.entries(s).iter().any(|e| e.key == "art" && e.value == sprite);
        let effect = effects.and_then(|t| t.sections().iter().find(|s| plays(&t, s)).map(|s| data_name("effects.txt", s)));
        effect.or_else(|| self.store.read_table("ground.txt").and_then(|t| super::Crop::find(&t, sprite)).map(|c| data_name("ground.txt", &c.code)))
    }

    /// Hands the game the store's `ground.txt` for maps to paint and lay, where it isn't the game's own, or always
    /// with `saved`, just after it is saved.
    pub(super) fn install_ground(&mut self, saved: bool) {
        let Some(text) = self.store.read_table("ground.txt") else { return };
        if !saved && last_harvest_game::built_in_table("ground.txt") == Some(text.as_str()) {
            return;
        }
        if let Err(e) = last_harvest_game::set_map_ground(&text) {
            self.say(format!("ground.txt: {e}"));
        }
    }

    /// Opens a data file's section from the store; true if `name` is one.
    pub(super) fn open_table(&mut self, name: &str) -> bool {
        if !name.starts_with(sprite::DATA_PREFIX) {
            return false;
        }
        for file in self.table_files() {
            let file = file.as_str();
            let Some(text) = self.store.read_table(file) else { continue };
            let t = Table::new(file, &text);
            if let Some(section) = t.sections().into_iter().find(|s| data_name(file, s) == name) {
                (self.map, self.data_link) = (None, None);
                let weapon = weapons::is_weapon(&t, &section).then(Weapon::new);
                self.data = Some(DataEdit { file: file.to_string(), section, table: t, dirty: false, first: 0, typing: None, undo: Vec::new(), redo: Vec::new(), weapon });
                self.refresh_weapon();
                self.sprite = Sprite { detail: 1, alpha: Vec::new(), worn: Vec::new(), name: name.into(), frames: vec![Frame::blank(16, 16)], views: None, footprint: None, roof: None, north: None, west: None, top: None, card: None, clear_in_3d: Vec::new(), paint: Default::default() };
                (self.frame, self.view, self.stats, self.selection, self.naming) = (0, None, false, None, None);
                self.undo.clear();
                self.redo.clear();
                (self.dirty, self.stand_touched, self.changed) = (false, false, true);
                return true;
            }
        }
        false
    }

    /// Whether the page's section can be copied to a new id.
    pub(super) fn copyable(d: &DataEdit) -> bool {
        COPYABLE.contains(&kind(&d.file)) && d.section != table::TOP
    }

    /// Hands the game the store's `farm.txt` for Randomize to lay out, where it isn't the game's own, or always
    /// with `saved`, just after it is saved; then the map page's farm again, over it.
    pub(super) fn install_farm(&mut self, saved: bool) {
        if let Some(text) = self.store.read_table("farm.txt")
            && (saved || last_harvest_game::built_in_table("farm.txt") != Some(text.as_str()))
            && let Err(e) = last_harvest_game::set_farm(&text)
        {
            self.say(format!("farm.txt: {e}"));
        }
        self.use_map_farm();
    }

    /// Makes the farm `id` from the open farm rules, as written, and opens it.
    pub(super) fn new_farm(&mut self, id: &str) {
        let Some(d) = &self.data else { return };
        if !last_harvest_game::valid_farm_id(id) {
            return self.say(format!("A farm's id is 1 to {} lowercase letters, digits and _", last_harvest_game::FARM_ID_MAX));
        }
        let file = last_harvest_game::farm_file(id, false);
        if self.store.read_table(&file).is_some() {
            return self.say(format!("There's a farm {id} already"));
        }
        let (text, section) = (d.table.text.clone(), d.section.clone());
        if let Err(e) = self.store.save_table(&file, &text) {
            return self.say(format!("Not made: {e}"));
        }
        self.ask = None;
        self.names = self.all_names();
        self.open_table(&data_name(&file, &section));
        self.say(format!("Made {file}: a player card plays on it once its Stats page picks it"));
    }

    /// Puts the open ground kind into the farm `id`'s ground, in place of its own of the same code, and saves it.
    pub(super) fn ground_to_farm(&mut self, id: &str) {
        let Some(d) = &self.data else { return };
        if !last_harvest_game::valid_farm_id(id) {
            return self.say(format!("A farm's id is 1 to {} lowercase letters, digits and _", last_harvest_game::FARM_ID_MAX));
        }
        let file = last_harvest_game::farm_file(id, true);
        if file == d.file {
            return self.say("That's this farm's own ground");
        }
        let mut to = Table::new(&file, &self.store.read_table(&file).unwrap_or_default());
        let section = d.section.clone();
        to.put_section(&d.table, &section);
        if let Err(e) = self.store.save_table(&file, &to.text) {
            return self.say(format!("Not saved: {e}"));
        }
        self.ask = None;
        self.names = self.all_names();
        self.say(format!("[{section}] is in {file}: the farm {id} paints it in place of the game's"));
    }

    /// Removes the open farm file on a second press within a few seconds, and leaves nothing open.
    pub(super) fn delete_farm_file(&mut self) {
        let Some(d) = &self.data else { return };
        let file = d.file.clone();
        if last_harvest_game::farm_of_file(&file).is_none() {
            return self.say("Only a farm's own files can be deleted");
        }
        if self.delete_armed <= 0.0 {
            self.delete_armed = DELETE_CONFIRM;
            return self.say(format!("Press Delete again to remove {file} for good"));
        }
        self.delete_armed = 0.0;
        if let Err(e) = self.store.delete_table(&file) {
            return self.say(format!("Couldn't delete {file}: {e}"));
        }
        self.data = None;
        self.sprite.name.clear();
        self.dirty = false;
        self.names = self.all_names();
        self.changed = true;
        self.use_map_farm();
        self.say(format!("Deleted {file}"));
    }

    /// Adds a copy of the open section under `[id]` as one step to undo, and shows it.
    pub(super) fn copy_section(&mut self, id: &str) {
        let Some(d) = &mut self.data else { return };
        let ground = kind(&d.file) == "ground.txt";
        let why = if id.is_empty() {
            Some("Name the copy first".to_string())
        } else if ground && !(id.len() == 1 && id.bytes().all(|b| b.is_ascii_graphic() && b != b'#' && b != b'[' && b != b']')) {
            Some("A ground's code is one character, such as k".to_string())
        } else if ground {
            None
        } else if !id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
            Some("An id uses lowercase letters, digits and _".to_string())
        } else if d.table.sections().iter().any(|s| s == id) {
            Some(format!("{} already has [{id}]", d.file))
        } else {
            None
        };
        if let Some(why) = why {
            return self.say(why);
        }
        self.ask = None;
        // Lines that default to the id keep the original's value.
        let mut keep = Vec::new();
        if d.file == "powerups.txt" {
            let has = |k: &str| d.table.entries(&d.section).iter().any(|e| e.key == k);
            if !has("icon") {
                keep.push(("icon", format!("ICON_{}", d.section.to_ascii_uppercase())));
            }
            if d.weapon.is_some() && !has("does") {
                keep.push(("does", d.section.clone()));
            }
        }
        if d.change(|t, s| t.copy(s, id) && keep.iter().all(|(k, v)| t.add(id, k, v))) {
            d.section = id.to_string();
            let file = d.file.clone();
            self.show_section();
            self.say(format!("[{id}] is new: Save writes it to {file}"));
        }
    }

    /// Shows the data page's section after it changes: its name, and its weapon page if it's a weapon.
    fn show_section(&mut self) {
        let Some(d) = &mut self.data else { return };
        let name = data_name(&d.file, &d.section);
        if self.sprite.name == name {
            return;
        }
        (d.first, d.typing) = (0, None);
        d.weapon = weapons::is_weapon(&d.table, &d.section).then(Weapon::new);
        self.sprite.name = name;
        self.naming = None;
        self.changed = true;
        self.refresh_weapon();
    }

    pub(super) fn data_buttons(&self, d: &DataEdit, push: &mut impl FnMut(f32, f32, f32, f32, Action, &str, bool)) {
        for (i, (a, l)) in [(Action::Revert, "Revert"), (Action::Save, "Save")].into_iter().enumerate() {
            push(716.0 + i as f32 * 102.0, 8.0, 94.0, 34.0, a, l, a == Action::Save && d.dirty);
        }
        if Self::copyable(d) {
            push(if d.weapon.is_some() { 410.0 } else { 614.0 }, 8.0, 94.0, 34.0, Action::Ask(Ask::Copy), "Copy", false);
        }
        match kind(&d.file) {
            "farm.txt" => push(512.0, 8.0, 94.0, 34.0, Action::Ask(Ask::Farm), "New farm", false),
            "ground.txt" if d.section != table::TOP => push(512.0, 8.0, 94.0, 34.0, Action::Ask(Ask::FarmGround), "To farm", false),
            _ => {}
        }
        if last_harvest_game::farm_of_file(&d.file).is_some() {
            push(410.0, 8.0, 94.0, 34.0, Action::Delete, "Delete", self.delete_armed > 0.0);
        }
        if let Some(w) = &d.weapon {
            push(512.0, 8.0, 94.0, 34.0, Action::WeaponTry, "Try it", false);
            push(614.0, 8.0, 94.0, 34.0, Action::WeaponLines, if w.lines { "Weapon" } else { "Lines" }, false);
            if !w.lines {
                return self.weapon_buttons(d, w, push);
            }
        }
        let entries = d.table.entries(&d.section);
        for (row, e) in entries.iter().enumerate().skip(d.first).take(ROWS) {
            let y = ROW_TOP + (row - d.first) as f32 * ROW_H;
            if is_sprite(d, &e.key) {
                push(CANVAS.x + 318.0, y, 62.0, 30.0, Action::DataPaint(row), "Paint", false);
                push(CANVAS.x + 384.0, y, 40.0, 30.0, Action::DataSprite(row, -1), "<", false);
                push(CANVAS.x + 428.0, y, 40.0, 30.0, Action::DataSprite(row, 1), ">", false);
            } else if is_color(d, &e.key) {
                push(CANVAS.x + 384.0, y, 40.0, 30.0, Action::DataColor(row, -1), "<", false);
                push(CANVAS.x + 428.0, y, 40.0, 30.0, Action::DataColor(row, 1), ">", false);
            } else if table::step(&e.value, 1, false).is_some() {
                push(CANVAS.x + 384.0, y, 40.0, 30.0, Action::DataStep(row, -1), "-", false);
                push(CANVAS.x + 428.0, y, 40.0, 30.0, Action::DataStep(row, 1), "+", false);
            }
            push(CANVAS.x + 472.0, y, 50.0, 30.0, Action::DataEdit(row), "Edit", self.naming.is_some() && d.typing == Some(row));
            push(CANVAS.x + 526.0, y, 30.0, 30.0, Action::DataRemove(row), "x", false);
        }
        let bottom = CANVAS.y + CANVAS.h - 40.0;
        push(CANVAS.x, bottom, 120.0, 34.0, Action::DataAdd, "Add line", self.naming.as_ref().is_some_and(|(l, _)| *l == Line::Added));
        if entries.len() > ROWS {
            push(CANVAS.x + 440.0, bottom, 56.0, 34.0, Action::DataPage(-1), "Up", false);
            push(CANVAS.x + 500.0, bottom, 56.0, 34.0, Action::DataPage(1), "Down", false);
        }
    }

    pub(super) fn data_act(&mut self, a: Action, shift: bool) -> bool {
        if self.weapon_act(a, shift) {
            return true;
        }
        let Some(d) = &mut self.data else { return false };
        match a {
            Action::DataStep(row, by) => {
                let Some(e) = d.table.entries(&d.section).into_iter().nth(row) else { return true };
                if let Some(v) = table::step(&e.value, by, shift) {
                    d.change(|t, s| t.set(s, row, &v));
                }
            }
            Action::DataColor(row, by) => {
                let Some(e) = d.table.entries(&d.section).into_iter().nth(row) else { return true };
                let hex = |[r, g, b, _]: [u8; 4]| format!("{r:02X}{g:02X}{b:02X}");
                let colors: Vec<String> = sprite::palette().into_iter().map(hex).collect();
                let next = match colors.iter().position(|c| c.eq_ignore_ascii_case(&e.value)) {
                    Some(at) => (at as i32 + by).rem_euclid(colors.len() as i32) as usize,
                    None => 0,
                };
                d.change(|t, s| t.set(s, row, &colors[next]));
            }
            Action::DataSprite(row, by) => {
                let Some(e) = d.table.entries(&d.section).into_iter().nth(row) else { return true };
                let sprites: Vec<&String> = self.names.iter().map(|(n, _)| n).filter(|n| is_paintable(n)).collect();
                if sprites.is_empty() {
                    return true;
                }
                let next = match sprites.iter().position(|n| **n == e.value) {
                    Some(at) => (at as i32 + by).rem_euclid(sprites.len() as i32) as usize,
                    None => 0,
                };
                let name = sprites[next].clone();
                d.change(|t, s| t.set(s, row, &name));
            }
            Action::DataPaint(row) => {
                let Some(e) = d.table.entries(&d.section).into_iter().nth(row) else { return true };
                match self.names.iter().any(|(n, _)| *n == e.value) {
                    true => self.switch_to(e.value),
                    false => self.say(format!("No sprite called {}", e.value)),
                }
            }
            Action::DataEdit(row) => {
                let Some(e) = d.table.entries(&d.section).into_iter().nth(row) else { return true };
                d.typing = Some(row);
                self.naming = Some((Line::Value, e.value));
            }
            Action::DataRemove(row) => {
                d.change(|t, s| t.remove(s, row));
            }
            Action::DataAdd => {
                d.typing = None;
                self.naming = Some((Line::Added, String::new()));
            }
            Action::DataPage(by) => {
                let n = d.table.entries(&d.section).len();
                d.first = (d.first as i32 + by * ROWS as i32).clamp(0, n.saturating_sub(ROWS) as i32) as usize;
            }
            Action::Save => self.save_table(),
            Action::Delete => self.delete_farm_file(),
            Action::Revert => {
                let name = self.sprite.name.clone();
                self.open_table(&name);
                self.say("Reverted to the last save");
            }
            _ => return false,
        }
        true
    }

    /// Writes what was typed into the data page: a line's new value, or a whole new `key value` line.
    pub(super) fn commit_data(&mut self, line: Line, text: &str) {
        let Some(d) = &mut self.data else { return };
        match (line, d.typing.take()) {
            (Line::Value, Some(row)) => {
                d.change(|t, s| t.set(s, row, text));
            }
            (Line::Added, _) => {
                let (key, value) = text.split_once(char::is_whitespace).map_or((text, ""), |(k, v)| (k, v.trim()));
                if !key.is_empty() && d.change(|t, s| t.add(s, key, value)) {
                    d.first = d.table.entries(&d.section).len().saturating_sub(ROWS);
                }
            }
            _ => {}
        }
        self.refresh_weapon();
    }

    /// Saves the open data file, once the game would read it; the list follows any section it adds or drops.
    pub(super) fn save_table(&mut self) {
        let Some(d) = &mut self.data else { return };
        match self.store.save_table(&d.file, &d.table.text) {
            Ok(()) => {
                d.dirty = false;
                self.dirty = false;
                let file = d.file.clone();
                self.names = self.all_names();
                match file.as_str() {
                    "ground.txt" => self.install_ground(true),
                    "farm.txt" => self.install_farm(true),
                    _ => self.use_map_farm(),
                }
                self.say(format!("Saved {file}"));
            }
            Err(e) => self.say(format!("Not saved: {e}")),
        }
    }

    pub(super) fn data_key(&mut self, key: Key) -> bool {
        if self.data.is_none() {
            return false;
        }
        match key {
            Key::Save => self.save_table(),
            Key::Undo | Key::Redo => {
                let Some(d) = &mut self.data else { return true };
                let (from, to) = if key == Key::Undo { (&mut d.undo, &mut d.redo) } else { (&mut d.redo, &mut d.undo) };
                if let Some((text, section)) = from.pop() {
                    to.push((std::mem::replace(&mut d.table.text, text), std::mem::replace(&mut d.section, section)));
                    (d.dirty, d.typing) = (true, None);
                    d.first = d.first.min(d.table.entries(&d.section).len().saturating_sub(ROWS));
                }
                self.show_section();
                self.refresh_weapon();
            }
            // The page has no pixels for these to change.
            Key::Copy | Key::Cut | Key::Paste => {}
            Key::Up => {
                self.data_act(Action::DataPage(-1), false);
            }
            Key::Down => {
                self.data_act(Action::DataPage(1), false);
            }
            _ => return false,
        }
        true
    }

    pub(super) fn draw_data(&self, c: &mut Canvas, d: &DataEdit) {
        c.rect(LIST.x, LIST.y, LIST.w, LIST.h, PANEL);
        self.draw_list(c);
        c.label(format!("{}{}", self.sprite.name, if d.dirty { "  (unsaved)" } else { "" }), 12.0, 12.0, 26.0, TEXT, Align::Left);
        c.rect(CANVAS.x, CANVAS.y, CANVAS.w, CANVAS.h, PANEL);
        if d.weapon.as_ref().is_some_and(|w| !w.lines) {
            return self.draw_weapon(c);
        }
        let title = if d.section.is_empty() { format!("{}, before its first section", d.file) } else { format!("{} [{}]", d.file, d.section) };
        c.label(title, CANVAS.x + 12.0, CANVAS.y + 12.0, 20.0, HOT, Align::Left);
        let entries = d.table.entries(&d.section);
        for (row, e) in entries.iter().enumerate().skip(d.first).take(ROWS) {
            let y = ROW_TOP + (row - d.first) as f32 * ROW_H;
            c.label(e.key.clone(), CANVAS.x + 12.0, y + 2.0, 18.0, TEXT, Align::Left);
            let typed = self.naming.as_ref().filter(|(l, _)| *l == Line::Value && d.typing == Some(row));
            let value = match typed {
                Some((_, t)) => format!("{}{}", t.chars().rev().take(VALUE_CHARS - 1).collect::<Vec<_>>().into_iter().rev().collect::<String>(), "_"),
                None if e.value.chars().count() > VALUE_CHARS => format!("{}...", e.value.chars().take(VALUE_CHARS - 3).collect::<String>()),
                None => e.value.clone(),
            };
            c.label(value, CANVAS.x + 150.0, y + 2.0, 18.0, if typed.is_some() { HOT } else { TEXT }, Align::Left);
            if !e.comment.is_empty() {
                let note: String = e.comment.chars().take(38).collect();
                c.label(note, CANVAS.x + 150.0, y + 21.0, 12.0, DIM, Align::Left);
            }
        }
        if let Some((Line::Added, t)) = &self.naming {
            c.label(format!("New line: {t}_"), CANVAS.x + 130.0, CANVAS.y + CANVAS.h - 32.0, 18.0, HOT, Align::Left);
        } else {
            c.label("key value, as the file writes it", CANVAS.x + 130.0, CANVAS.y + CANVAS.h - 30.0, 15.0, DIM, Align::Left);
        }
        for b in self.buttons() {
            let r = b.rect;
            c.rect(r.x, r.y, r.w, r.h, if b.on { HOT } else { BUTTON });
            c.label(b.label, r.x + r.w / 2.0, r.y + (r.h - 18.0) / 2.0, 18.0, if b.on { [0.1, 0.1, 0.1, 1.0] } else { TEXT }, Align::Center);
        }
        let help = "Edit: type a value, Enter keeps it   -/+: step a number, Shift by ten   Up/Down: page   Cmd+S: save";
        c.label(help, CANVAS.x, CANVAS.y + CANVAS.h + 12.0, 15.0, DIM, Align::Left);
        c.label("The game checks the file as it would read it before it saves.", CANVAS.x, CANVAS.y + CANVAS.h + 34.0, 15.0, DIM, Align::Left);
        self.draw_overlays(c);
    }
}
