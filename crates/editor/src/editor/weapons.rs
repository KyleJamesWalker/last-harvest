//! The weapon page: a `powerups.txt` weapon's section as buttons for its behaviour, effect, bullet and sound, its
//! numbers at each level, and its effect and bullet playing. Every change rewrites the section's lines in place.

use crate::table::{self, Table};

use super::*;

/// Levels shown side by side; more scroll with the picked level.
const COLS: usize = 10;
const COL_X: f32 = CANVAS.x + 212.0;
const COL_W: f32 = 34.0;
const HEAD_Y: f32 = CANVAS.y + 44.0;
const STAT_Y: f32 = CANVAS.y + 80.0;
const STAT_H: f32 = 27.0;
const PICK_Y: f32 = CANVAS.y + 422.0;
const DOES_Y: f32 = CANVAS.y + 28.0;
const DOES_W: f32 = 114.0;
const PREVIEW: Rect = Rect { x: SIDE_X, y: CANVAS.y + 340.0, w: SIDE_W, h: 220.0 };
const STYLE: Rect = Rect { x: SIDE_X + SIDE_W / 2.0 + 8.0, y: PREVIEW.y + PREVIEW.h + 6.0, w: SIDE_W / 2.0 - 16.0, h: 28.0 };
/// The lines the pickers set, in order.
const PICKS: [&str; 4] = ["effect", "bullet", "sound", "pattern"];

/// What the weapon page shows of a section, beside the lines it edits.
pub(super) struct Weapon {
    /// The data page's lines show instead.
    pub lines: bool,
    /// The level the -/+ buttons step, from 1.
    pub level: usize,
    /// The effect and bullet playing: the effects.txt name, its frames and their pace.
    pub preview: [(Option<String>, Vec<Frame>, f32); 2],
    /// The shot style its bullets fly as, when they fly as particles.
    pub shot: Option<last_harvest_game::Shot>,
    /// The frames of the sprites the shot draws, by `SHOT_SPRITES`; none draws the plain shape.
    pub shot_art: [Vec<Frame>; 4],
}

impl Weapon {
    pub fn new() -> Self {
        Self { lines: false, level: 1, preview: Default::default(), shot: None, shot_art: Default::default() }
    }
}

pub(super) fn is_weapon(t: &Table, section: &str) -> bool {
    t.file == "powerups.txt" && t.entries(section).iter().any(|e| e.key == "kind" && e.value == "weapon")
}

fn value(t: &Table, section: &str, key: &str) -> Option<String> {
    t.entries(section).into_iter().find(|e| e.key == key).map(|e| e.value)
}

/// Sets the section's `key` line, adding it at the end when it has none; None removes it.
fn set_key(t: &mut Table, section: &str, key: &str, value: Option<&str>) -> bool {
    match (t.entries(section).iter().position(|e| e.key == key), value) {
        (Some(row), Some(v)) => t.set(section, row, v),
        (Some(row), None) => t.remove(section, row),
        (None, Some(v)) => t.add(section, key, v),
        (None, None) => false,
    }
}

/// A stat's values with only `level`'s stepped: a short list first grows by its last value until the level after
/// is its own, so it keeps what it was.
fn step_level(values: &str, level: usize, max: usize, by: i32, shift: bool) -> Option<String> {
    let mut words: Vec<String> = values.split_whitespace().map(str::to_string).collect();
    let last = words.last()?.clone();
    let level = level.clamp(1, max.max(1));
    while words.len() < (level + 1).min(max).max(level) {
        words.push(last.clone());
    }
    words[level - 1] = table::step(&words[level - 1], by, shift)?;
    Some(words.join(" "))
}

/// The highest level as the game reads the section.
fn max_level(t: &Table, section: &str) -> usize {
    value(t, section, "max").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).max(1)
}

/// The levels shown, scrolled so the picked one is among them.
fn shown(level: usize, max: usize) -> std::ops::RangeInclusive<usize> {
    let first = level.saturating_sub(COLS - 1).max(1);
    first..=(first + COLS - 1).min(max)
}

fn number(v: f32) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// What a picker can name: effects.txt's sections, the weapon sounds, or the patterns of a gun's shots.
fn choices(store: &dyn Store, pick: usize) -> Vec<String> {
    match PICKS[pick] {
        "sound" => return last_harvest_game::weapon_sound_names().map(str::to_string).to_vec(),
        "pattern" => return last_harvest_game::weapon_patterns().map(str::to_string).to_vec(),
        _ => {}
    }
    let effects = Table::new("effects.txt", &store.read_table("effects.txt").unwrap_or_default());
    effects.sections().into_iter().filter(|s| s != table::TOP && s != "colors").collect()
}

impl Editor {
    fn weapon_page(&self) -> Option<(&DataEdit, &Weapon)> {
        let d = self.data.as_ref()?;
        d.weapon.as_ref().filter(|w| !w.lines).map(|w| (d, w))
    }

    /// Loads the frames of the effect and bullet the section names, where they changed.
    pub(super) fn refresh_weapon(&mut self) {
        let Some(d) = &self.data else { return };
        let text = self.store.read_table("effects.txt").unwrap_or_default();
        let effects = Table::new("effects.txt", &text);
        let names = ["effect", "bullet"].map(|k| value(&d.table, &d.section, k));
        let shot = last_harvest_game::shot_named(&text, names[1].as_deref());
        let shot_art = last_harvest_game::shot_sprite_names(&text, names[1].as_deref()).map(|n| n.and_then(|n| self.store.load(&n)).map(|s| s.frames).unwrap_or_default());
        let mut loaded = Vec::new();
        for (i, name) in names.into_iter().enumerate() {
            if d.weapon.as_ref().is_some_and(|w| w.preview[i].0 == name) {
                continue;
            }
            let art = name.as_ref().and_then(|n| value(&effects, n, "art"));
            let fps = name.as_ref().and_then(|n| value(&effects, n, "fps")).and_then(|f| f.parse().ok()).unwrap_or(10.0);
            let frames = art.and_then(|a| self.store.load(&a)).map(|s| s.frames).unwrap_or_default();
            loaded.push((i, (name, frames, fps)));
        }
        if let Some(w) = self.data.as_mut().and_then(|d| d.weapon.as_mut()) {
            (w.shot, w.shot_art) = (shot, shot_art);
            for (i, p) in loaded {
                w.preview[i] = p;
            }
        }
    }

    pub(super) fn weapon_buttons(&self, d: &DataEdit, w: &Weapon, push: &mut impl FnMut(f32, f32, f32, f32, Action, &str, bool)) {
        let max = max_level(&d.table, &d.section);
        push(CANVAS.x + 100.0, HEAD_Y, 28.0, 26.0, Action::WeaponMax(-1), "-", false);
        push(CANVAS.x + 130.0, HEAD_Y, 28.0, 26.0, Action::WeaponMax(1), "+", false);
        for (i, l) in shown(w.level, max).enumerate() {
            push(COL_X + i as f32 * COL_W, HEAD_Y, COL_W - 2.0, 26.0, Action::WeaponLevel(l), &l.to_string(), l == w.level.min(max));
        }
        for i in 0..last_harvest_game::weapon_stats().len() {
            let y = STAT_Y + i as f32 * STAT_H;
            push(CANVAS.x + 100.0, y, 28.0, 26.0, Action::WeaponStep(i, -1), "-", false);
            push(CANVAS.x + 130.0, y, 28.0, 26.0, Action::WeaponStep(i, 1), "+", false);
            push(CANVAS.x + 160.0, y, 44.0, 26.0, Action::WeaponType(i), "Edit", false);
        }
        for i in 0..PICKS.len() {
            let y = PICK_Y + i as f32 * 32.0;
            push(CANVAS.x + 392.0, y, 36.0, 28.0, Action::WeaponPick(i, -1), "<", false);
            push(CANVAS.x + 432.0, y, 36.0, 28.0, Action::WeaponPick(i, 1), ">", false);
            push(CANVAS.x + 472.0, y, 76.0, 28.0, Action::WeaponPick(i, 0), "Own", false);
        }
        push(STYLE.x, STYLE.y, STYLE.w, STYLE.h, Action::WeaponStyle, "Style", false);
        let does = value(&d.table, &d.section, "does").unwrap_or_else(|| d.section.clone());
        for (i, name) in last_harvest_game::weapon_behaviours().into_iter().enumerate() {
            let (x, y) = (SIDE_X + (i % 2) as f32 * (DOES_W + 4.0), DOES_Y + (i / 2) as f32 * 30.0);
            push(x, y, DOES_W, 26.0, Action::WeaponDoes(i), name, name == does);
        }
    }

    pub(super) fn weapon_act(&mut self, a: Action, shift: bool) -> bool {
        let store = &*self.store;
        let Some(d) = &mut self.data else { return false };
        let Some(level) = d.weapon.as_ref().map(|w| w.level) else { return false };
        let max = max_level(&d.table, &d.section);
        match a {
            Action::WeaponLines => {
                d.weapon.as_mut().unwrap().lines ^= true;
                d.typing = None;
                self.naming = None;
            }
            Action::WeaponLevel(l) => d.weapon.as_mut().unwrap().level = l.clamp(1, max),
            Action::WeaponTry => {
                let level = level.min(max) as u32;
                let effects = Table::new("effects.txt", &store.read_table("effects.txt").unwrap_or_default());
                let mut styles = vec!["bullet".to_string()];
                styles.extend(value(&d.table, &d.section, "bullet").filter(|b| b != "bullet"));
                let effects: String = styles.iter().filter_map(|s| effects.section_text(s)).collect();
                let effects = (!effects.is_empty()).then_some(effects);
                let setup = last_harvest_game::RangeSetup { weapon: d.section.clone(), level, section: d.table.section_text(&d.section), effects };
                let said = match last_harvest_game::RangeSetup::read(&setup.text()) {
                    Ok(s) => {
                        self.tried = Some(s.text());
                        format!("Trying {} at level {level} in the game's range", s.weapon)
                    }
                    Err(e) => format!("The game won't try it: {e}"),
                };
                self.say(said);
                return true;
            }
            Action::WeaponMax(by) => {
                if let Some(v) = table::step(&max.to_string(), by, shift).filter(|v| v != "0") {
                    d.change(|t, s| set_key(t, s, "max", Some(&v)));
                }
            }
            Action::WeaponDoes(i) => {
                let name = last_harvest_game::weapon_behaviours()[i];
                if value(&d.table, &d.section, "does").unwrap_or_else(|| d.section.clone()) != name {
                    d.change(|t, s| set_key(t, s, "does", Some(name)));
                }
            }
            Action::WeaponStep(i, by) => {
                let (key, own) = last_harvest_game::weapon_stats()[i];
                let now = value(&d.table, &d.section, key).unwrap_or_else(|| number(own));
                if let Some(v) = step_level(&now, level, max, by, shift) {
                    d.change(|t, s| set_key(t, s, key, Some(&v)));
                }
            }
            Action::WeaponType(i) => {
                let (key, own) = last_harvest_game::weapon_stats()[i];
                let entries = d.table.entries(&d.section);
                match entries.iter().position(|e| e.key == key) {
                    Some(row) => {
                        d.typing = Some(row);
                        self.naming = Some((Line::Value, entries[row].value.clone()));
                    }
                    None => {
                        d.typing = None;
                        self.naming = Some((Line::Added, format!("{key} {}", number(own))));
                    }
                }
            }
            Action::WeaponPick(i, by) => {
                let key = PICKS[i];
                let all = choices(store, i);
                let now = value(&d.table, &d.section, key);
                let at = now.as_ref().and_then(|n| all.iter().position(|c| c == n)).map_or(0, |p| p as i32 + 1);
                let next = if by == 0 { 0 } else { (at + by).rem_euclid(all.len() as i32 + 1) as usize };
                let pick = next.checked_sub(1).map(|n| all[n].as_str());
                d.change(|t, s| set_key(t, s, key, pick));
            }
            Action::WeaponStyle => {
                let name = value(&d.table, &d.section, "bullet").unwrap_or_else(|| "bullet".into());
                self.switch_to(super::tables::data_name("effects.txt", &name));
                return true;
            }
            _ => return false,
        }
        self.refresh_weapon();
        true
    }

    pub(super) fn draw_weapon(&self, c: &mut Canvas) {
        let Some((d, w)) = self.weapon_page() else { return };
        c.label(format!("{} [{}], a weapon", d.file, d.section), CANVAS.x + 12.0, CANVAS.y + 12.0, 20.0, HOT, Align::Left);
        let max = max_level(&d.table, &d.section);
        c.label("max", CANVAS.x + 12.0, HEAD_Y + 4.0, 17.0, TEXT, Align::Left);
        c.label(max.to_string(), CANVAS.x + 182.0, HEAD_Y + 4.0, 17.0, TEXT, Align::Center);
        let levels = last_harvest_game::weapon_levels(&d.table.text, &d.section);
        let level = w.level.min(max);
        for (i, (key, _)) in last_harvest_game::weapon_stats().into_iter().enumerate() {
            let y = STAT_Y + i as f32 * STAT_H;
            c.label(key, CANVAS.x + 12.0, y + 4.0, 17.0, TEXT, Align::Left);
            let Ok(levels) = &levels else { continue };
            let (_, given, values) = &levels.stats[i];
            for (col, l) in shown(w.level, max).enumerate() {
                let Some(v) = values.get(l - 1) else { continue };
                let color = if l == level { HOT } else if *given { TEXT } else { DIM };
                c.label(number(*v), COL_X + col as f32 * COL_W + (COL_W - 2.0) / 2.0, y + 6.0, 14.0, color, Align::Center);
            }
        }
        for (i, key) in PICKS.into_iter().enumerate() {
            let y = PICK_Y + i as f32 * 32.0;
            c.label(key, CANVAS.x + 12.0, y + 5.0, 17.0, TEXT, Align::Left);
            let now = value(&d.table, &d.section, key);
            let own = if key == "pattern" { "fan" } else { "its behaviour's own" };
            c.label(now.unwrap_or_else(|| own.into()), CANVAS.x + 100.0, y + 5.0, 17.0, TEXT, Align::Left);
        }
        let hint = match &self.naming {
            Some((Line::Value, t)) => format!("{}: {t}_", d.typing.and_then(|r| d.table.entries(&d.section).get(r).map(|e| e.key.clone())).unwrap_or_default()),
            Some((Line::Added, t)) => format!("New line: {t}_"),
            _ => match &levels {
                Err(e) => format!("The game won't read it:{}", e.chars().take(56).collect::<String>()),
                Ok(_) => "Dim numbers are the game's own; a short list repeats its last".into(),
            },
        };
        let typing = self.naming.is_some() || levels.is_err();
        c.label(hint, CANVAS.x + 12.0, CANVAS.y + CANVAS.h - 34.0, if typing { 18.0 } else { 15.0 }, if typing { HOT } else { DIM }, Align::Left);
        c.label("Does", SIDE_X, CANVAS.y + 4.0, 17.0, TEXT, Align::Left);
        c.rect(PREVIEW.x, PREVIEW.y, PREVIEW.w, PREVIEW.h, PANEL);
        for (i, (name, frames, fps)) in w.preview.iter().enumerate() {
            let x = PREVIEW.x + i as f32 * PREVIEW.w / 2.0;
            c.label(PICKS[i], x + PREVIEW.w / 4.0, PREVIEW.y + 6.0, 15.0, DIM, Align::Center);
            if let (1, Some(shot)) = (i, w.shot) {
                self.draw_shot(c, shot, &w.shot_art, x);
                continue;
            }
            let Some(f) = frames.get((self.clock * fps).max(0.0) as usize % frames.len().max(1)) else {
                let none = if name.is_some() { "no frames" } else { "none" };
                c.label(none, x + PREVIEW.w / 4.0, PREVIEW.y + PREVIEW.h / 2.0, 15.0, DIM, Align::Center);
                continue;
            };
            let zoom = ((PREVIEW.w / 2.0 - 12.0) / f.w as f32).min((PREVIEW.h - 40.0) / f.h as f32).floor().max(1.0);
            let o = [x + (PREVIEW.w / 2.0 - f.w as f32 * zoom) / 2.0, PREVIEW.y + 30.0 + (PREVIEW.h - 40.0 - f.h as f32 * zoom) / 2.0];
            for (p, px) in f.px.iter().enumerate().filter(|(_, px)| px[3] > 0) {
                c.rect(o[0] + (p % f.w) as f32 * zoom, o[1] + (p / f.w) as f32 * zoom, zoom, zoom, rgba(*px));
            }
        }
        for b in self.buttons() {
            let r = b.rect;
            c.rect(r.x, r.y, r.w, r.h, if b.on { HOT } else { BUTTON });
            c.label(b.label, r.x + r.w / 2.0, r.y + (r.h - 16.0) / 2.0, 16.0, if b.on { [0.1, 0.1, 0.1, 1.0] } else { TEXT }, Align::Center);
        }
        c.label("Levels: pick one; -/+ step its value, Shift by ten   Lines: every line", CANVAS.x, CANVAS.y + CANVAS.h + 12.0, 15.0, DIM, Align::Left);
        c.label("< > pick from effects.txt or the sounds; Own leaves the behaviour's.", CANVAS.x, CANVAS.y + CANVAS.h + 34.0, 15.0, DIM, Align::Left);
        self.draw_overlays(c);
    }

    /// The bullet's streak flying across the preview's half at `x`, with the numbers of its style under it.
    fn draw_shot(&self, c: &mut Canvas, shot: last_harvest_game::Shot, art: &[Vec<Frame>; 4], x: f32) {
        let (w, mid) = (PREVIEW.w / 2.0, PREVIEW.y + PREVIEW.h / 2.0 - 16.0);
        let zoom = ((w - 16.0) / shot.length).clamp(1.0, 4.0);
        let (from, run) = (x + 8.0, (w - 16.0) / zoom + shot.length);
        let speed = 90.0;
        let age = (self.clock * speed / run).fract() * run / speed;
        let head = speed * age;
        let tail = shot.tail([head, 0.0], [speed, 0.0], age)[0];
        let [r, g, b] = shot.color;
        let in_panel = |px: f32| (from..x + w - 8.0).contains(&px);
        let flare_reach = shot.core * 2.4 * zoom;
        match art[3].first() {
            Some(f) if head * zoom < flare_reach => stretch(c, f, [from, mid], flare_reach, flare_reach * f.h as f32 / f.w as f32, &in_panel),
            None if head * zoom < shot.core => {
                let s = shot.core * zoom;
                c.rect(from - s / 2.0, mid - s / 2.0, s, s, [1.0; 4]);
            }
            _ => {}
        }
        if let Some(f) = art[0].get((age * 12.0) as usize % art[0].len().max(1)) {
            stretch(c, f, [from + tail * zoom, mid], (head - tail) * zoom, shot.size * 1.5 * zoom, &in_panel);
        }
        let mut px = 0.0;
        while art[0].is_empty() && px <= (head - tail) * zoom {
            let k = px / ((head - tail) * zoom).max(1.0);
            let (thick, bright) = last_harvest_game::Shot::taper(k);
            let at = from + head * zoom - px;
            let h = (shot.size * thick * zoom).max(1.0);
            if in_panel(at) {
                let hot = if k < 0.45 { 0.5 } else { 0.0 };
                c.rect(at, mid - h / 2.0, 1.0, h, [(r * bright + hot).min(1.0), (g * bright + hot).min(1.0), (b * bright + hot).min(1.0), 1.0]);
            }
            px += 1.0;
        }
        let hex = format!("{:02X}{:02X}{:02X}", (r * 255.0).round() as u8, (g * 255.0).round() as u8, (b * 255.0).round() as u8);
        c.label(hex, x + w / 2.0, mid + 30.0, 14.0, TEXT, Align::Center);
        c.label(format!("length {} size {}", number(shot.length), number(shot.size)), x + w / 2.0, mid + 48.0, 13.0, DIM, Align::Center);
        c.label(format!("{} sparks, core {}", shot.sparks, number(shot.core)), x + w / 2.0, mid + 64.0, 13.0, DIM, Align::Center);
        c.label(format!("{} on a hit", shot.impact), x + w / 2.0, mid + 80.0, 13.0, DIM, Align::Center);
        for (k, frames) in art[1..3].iter().enumerate() {
            for (i, f) in frames.iter().enumerate() {
                let s = 3.0;
                let o = [x + 8.0 + (k * 4 + i) as f32 * (f.w as f32 * s + 4.0), mid + 96.0];
                for (p, px) in f.px.iter().enumerate().filter(|(_, px)| px[3] > 0) {
                    c.rect(o[0] + (p % f.w) as f32 * s, o[1] + (p / f.w) as f32 * s, s, s, rgba(*px));
                }
            }
        }
    }
}

/// Draws a frame `w` by `h` with its left edge's middle at `at`, its pixels clipped to what `shown` allows.
fn stretch(c: &mut Canvas, f: &Frame, at: [f32; 2], w: f32, h: f32, shown: &dyn Fn(f32) -> bool) {
    let (sx, sy) = (w / f.w as f32, h / f.h as f32);
    for (p, px) in f.px.iter().enumerate().filter(|(_, px)| px[3] > 0) {
        let x = at[0] + (p % f.w) as f32 * sx;
        if shown(x) {
            c.rect(x, at[1] - h / 2.0 + (p / f.w) as f32 * sy, sx.max(1.0), sy.max(1.0), rgba(*px));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_changes_only_the_picked_level() {
        assert_eq!(step_level("150", 1, 8, 1, false).as_deref(), Some("151 150"));
        assert_eq!(step_level("150", 3, 8, -1, false).as_deref(), Some("150 150 149 150"));
        assert_eq!(step_level("150", 8, 8, 1, true).as_deref(), Some("150 150 150 150 150 150 150 160"));
        assert_eq!(step_level("0.6 0.55", 2, 2, 1, false).as_deref(), Some("0.6 0.56"));
        assert_eq!(step_level("4", 1, 1, 1, false).as_deref(), Some("5"));
        assert_eq!(step_level("", 1, 8, 1, false), None);
    }

    #[test]
    fn the_levels_shown_follow_the_picked_one() {
        assert_eq!(shown(1, 8), 1..=8);
        assert_eq!(shown(12, 20), 3..=12);
    }
}
