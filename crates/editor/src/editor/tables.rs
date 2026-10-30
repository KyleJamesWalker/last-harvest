//! The data page: one section of one of the game's data files, such as `tuning.txt`'s [player], a line at a time.
//! Each section lists as its own sprite name, `DATA_TUNING_PLAYER`, under a heading for its file.

use crate::table::{self, Table};

use super::*;

/// Lines the page shows at once.
const ROWS: usize = 14;
const ROW_TOP: f32 = CANVAS.y + 48.0;
const ROW_H: f32 = 34.0;
/// Characters of a value that fit before the buttons.
const VALUE_CHARS: usize = 24;

pub(super) struct DataEdit {
    pub file: String,
    pub section: String,
    pub table: Table,
    pub dirty: bool,
    /// The first line shown.
    pub first: usize,
    /// The line whose value is being typed, by its place in the section.
    pub typing: Option<usize>,
}

/// The list name for a section of a data file: `DATA_POWERUPS_PISTOL`, or `DATA_POWERUPS` for the lines before
/// its first heading.
pub(super) fn data_name(file: &str, section: &str) -> String {
    let stem = file.trim_end_matches(".txt").to_ascii_uppercase();
    let section: String = section.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' }).collect();
    if section.is_empty() { format!("{}{stem}", sprite::DATA_PREFIX) } else { format!("{}{stem}_{section}", sprite::DATA_PREFIX) }
}

impl Editor {
    /// Every data file's sections, as names for the list, marked as saved where the file differs from the game's own.
    pub(super) fn data_names(&self) -> Vec<(String, bool)> {
        let mut out = Vec::new();
        for file in last_harvest_game::TABLES {
            let Some(text) = self.store.read_table(file) else { continue };
            let changed = last_harvest_game::built_in_table(file) != Some(text.as_str());
            out.extend(Table::new(file, &text).sections().iter().map(|s| (data_name(file, s), changed)));
        }
        out
    }

    /// The data page section that says how a sprite plays: its props.txt section, else the first effects.txt
    /// section that plays it.
    pub(super) fn data_link_for(&self, sprite: &str) -> Option<String> {
        let props = Table::new("props.txt", &self.store.read_table("props.txt")?);
        if props.sections().iter().any(|s| s == sprite) {
            return Some(data_name("props.txt", sprite));
        }
        let effects = Table::new("effects.txt", &self.store.read_table("effects.txt")?);
        let plays = |s: &String| effects.entries(s).iter().any(|e| e.key == "art" && e.value == sprite);
        effects.sections().iter().find(|s| plays(s)).map(|s| data_name("effects.txt", s))
    }

    /// Opens a data file's section from the store; true if `name` is one.
    pub(super) fn open_table(&mut self, name: &str) -> bool {
        if !name.starts_with(sprite::DATA_PREFIX) {
            return false;
        }
        for file in last_harvest_game::TABLES {
            let Some(text) = self.store.read_table(file) else { continue };
            let t = Table::new(file, &text);
            if let Some(section) = t.sections().into_iter().find(|s| data_name(file, s) == name) {
                self.map = None;
                self.data = Some(DataEdit { file: file.to_string(), section, table: t, dirty: false, first: 0, typing: None });
                self.sprite = Sprite { name: name.into(), frames: vec![Frame::blank(16, 16)], views: None, footprint: None, top: None, card: None, clear_in_3d: Vec::new() };
                (self.frame, self.view, self.stats, self.selection, self.naming) = (0, None, false, None, None);
                self.undo.clear();
                self.redo.clear();
                (self.dirty, self.stand_touched, self.changed) = (false, false, true);
                return true;
            }
        }
        false
    }

    pub(super) fn data_buttons(&self, d: &DataEdit, push: &mut impl FnMut(f32, f32, f32, f32, Action, &str, bool)) {
        for (i, (a, l)) in [(Action::Revert, "Revert"), (Action::Save, "Save")].into_iter().enumerate() {
            push(716.0 + i as f32 * 102.0, 8.0, 94.0, 34.0, a, l, a == Action::Save && d.dirty);
        }
        let entries = d.table.entries(&d.section);
        for (row, e) in entries.iter().enumerate().skip(d.first).take(ROWS) {
            let y = ROW_TOP + (row - d.first) as f32 * ROW_H;
            if table::step(&e.value, 1, false).is_some() {
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
        let Some(d) = &mut self.data else { return false };
        match a {
            Action::DataStep(row, by) => {
                let Some(e) = d.table.entries(&d.section).into_iter().nth(row) else { return true };
                if let Some(v) = table::step(&e.value, by, shift) {
                    d.table.set(&d.section, row, &v);
                    d.dirty = true;
                }
            }
            Action::DataEdit(row) => {
                let Some(e) = d.table.entries(&d.section).into_iter().nth(row) else { return true };
                d.typing = Some(row);
                self.naming = Some((Line::Value, e.value));
            }
            Action::DataRemove(row) => {
                d.table.remove(&d.section, row);
                d.dirty = true;
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
                d.dirty |= d.table.set(&d.section, row, text);
            }
            (Line::Added, _) => {
                let (key, value) = text.split_once(char::is_whitespace).map_or((text, ""), |(k, v)| (k, v.trim()));
                if !key.is_empty() && d.table.add(&d.section, key, value) {
                    d.dirty = true;
                    d.first = d.table.entries(&d.section).len().saturating_sub(ROWS);
                }
            }
            _ => {}
        }
    }

    /// Saves the open data file, once the game would read it; the list follows any section it adds or drops.
    pub(super) fn save_table(&mut self) {
        let Some(d) = &mut self.data else { return };
        match self.store.save_table(&d.file, &d.table.text) {
            Ok(()) => {
                d.dirty = false;
                let file = d.file.clone();
                self.names = self.all_names();
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
                let note: String = e.comment.chars().take(52).collect();
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
        let help = "Edit or Enter: type a value   -/+: step a number, Shift by ten   Up/Down: page   Cmd+S: save";
        c.label(help, CANVAS.x, CANVAS.y + CANVAS.h + 12.0, 15.0, DIM, Align::Left);
        c.label("The game checks the file as it would read it before it saves.", CANVAS.x, CANVAS.y + CANVAS.h + 34.0, 15.0, DIM, Align::Left);
        self.draw_overlays(c);
    }
}
