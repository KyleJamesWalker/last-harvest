//! The weapon range: the player holds one weapon at a level the range steps, in an open field, against a ring of
//! zombies of one picked kind that rise again as they die, with its damage per second over the last few seconds.

use std::collections::VecDeque;
use std::f32::consts::TAU;

use super::map::{Map, TILE};
use super::powerups::{self, Kind as Card, Stat, book};
use super::weapons::Kit;
use super::{GOLD, Harvest, Kind, Phase, Tell, WHITE, effects, len, norm, roster, sub, Fx};
use crate::rng::Rng;
use crate::{Align, Buttons, Canvas, Difficulty, Input, SCREEN_W, Sfx};

/// Zombies standing in the ring.
pub(super) const RING: usize = 8;
const FIELD: [usize; 2] = [40, 30];
/// Seconds of damage the damage per second is taken over.
const WINDOW: f32 = 5.0;
/// The ring stands at this share of the weapon's range, within these many tiles.
const RING_SHARE: f32 = 0.4;
const RING_TILES: [f32; 2] = [2.5, 10.0];
/// The clock goes back this many seconds before it reaches a minute, so no minute call or dawn comes.
const CLOCK_LOOP: f32 = 50.0;
/// Radians the range's camera swings per pixel dragged, and tiles it backs off per wheel step, within `ORBIT_BACK`.
const ORBIT_TURN: f32 = 0.006;
const ORBIT_STEP: f32 = 0.5;
const ORBIT_BACK: [f32; 2] = [-1.5, 8.0];
/// How far down the top screen the buttons go when the run plays on it alone.
const TOP_KEYS: f32 = 76.0;
/// The bottom screen's buttons, as x, y, w, h, in the order of `Key`.
const KEYS: [[f32; 4]; 6] = [
    [24.0, 96.0, 150.0, 70.0],
    [184.0, 96.0, 150.0, 70.0],
    [364.0, 96.0, 150.0, 70.0],
    [524.0, 96.0, 150.0, 70.0],
    [704.0, 96.0, 140.0, 70.0],
    [854.0, 96.0, 146.0, 70.0],
];

/// What the range tries: a weapon by id at a level, and its `powerups.txt` section when that isn't the game's own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RangeSetup {
    pub weapon: String,
    pub level: u32,
    pub section: Option<String>,
    /// `effects.txt` sections to lay over the game's, such as the shot style its bullets fly as.
    pub effects: Option<String>,
}

/// The line before a setup's `effects.txt` sections.
const EFFECTS: &str = "effects.txt";

fn heading(line: &str) -> Option<&str> {
    line.split('#').next()?.trim().strip_prefix('[')?.strip_suffix(']')
}

impl RangeSetup {
    /// Reads what `LAST_HARVEST_RANGE` or an editor's Try it hands over: `id` or `id:level`, else a weapon's
    /// `powerups.txt` section after an optional `level N` line. An error says why, as the game would.
    pub fn read(text: &str) -> Result<Self, String> {
        let (text, effects) = match text.lines().position(|l| l.trim() == EFFECTS) {
            Some(at) => {
                let lines: Vec<&str> = text.lines().collect();
                let tail: String = lines[at + 1..].iter().map(|l| format!("{l}\n")).collect();
                (lines[..at].join("\n"), tail.lines().any(|l| heading(l).is_some()).then_some(tail))
            }
            None => (text.to_string(), None),
        };
        if let Some(e) = &effects {
            effects_with(super::built_in_table("effects.txt").unwrap_or_default(), e)?;
        }
        let setup = Self::read_weapon(&text)?;
        Ok(Self { effects, ..setup })
    }

    fn read_weapon(text: &str) -> Result<Self, String> {
        let text = text.trim();
        let level = |v: &str| v.trim().parse::<u32>().ok().filter(|l| *l > 0).ok_or_else(|| format!("the level is a whole number from 1, not {:?}", v.trim()));
        let Some(start) = text.lines().position(|l| heading(l).is_some()) else {
            let (id, at) = text.split_once(':').map_or((text, 1), |(id, l)| (id, level(l).unwrap_or(0)));
            if id.trim().is_empty() || at == 0 {
                return Err(format!("{text:?} isn't a weapon id, or id:level"));
            }
            return Ok(Self { weapon: id.trim().to_string(), level: at, section: None, effects: None });
        };
        let mut at = 1;
        for line in text.lines().take(start).map(|l| l.split('#').next().unwrap_or("").trim()).filter(|l| !l.is_empty()) {
            match line.strip_prefix("level ") {
                Some(v) => at = level(v)?,
                None => return Err(format!("before the section, only a level line, not {line:?}")),
            }
        }
        let lines: Vec<&str> = text.lines().skip(start).collect();
        let weapon = heading(lines[0]).unwrap_or_default().to_string();
        if lines[1..].iter().any(|l| heading(l).is_some()) {
            return Err(format!("only the one section [{weapon}]"));
        }
        let section = lines.join("\n") + "\n";
        let own = super::built_in_table("powerups.txt").unwrap_or_default();
        let book = powerups::parse(&powerups_with(own, &section)?).map_err(|e| format!("powerups.txt:{e}"))?;
        if book.defs.iter().find(|d| d.id == weapon).is_none_or(|d| d.kind != Card::Weapon) {
            return Err(format!("[{weapon}] isn't a weapon"));
        }
        Ok(Self { weapon, level: at, section: Some(section), effects: None })
    }

    /// The setup as `read` takes it back.
    pub fn text(&self) -> String {
        let weapon = match &self.section {
            Some(s) => format!("level {}\n{}", self.level.max(1), s),
            None => format!("{}:{}\n", self.weapon, self.level.max(1)),
        };
        match &self.effects {
            Some(e) => format!("{weapon}{EFFECTS}\n{e}"),
            None => weapon,
        }
    }
}

/// `base`, a whole `powerups.txt`, with `section` in place of the one with its heading, or added at the end when it
/// has none; an error names what the game wouldn't read.
pub fn powerups_with(base: &str, section: &str) -> Result<String, String> {
    let out = with_section(base, section)?;
    powerups::parse(&out).map_err(|e| format!("powerups.txt:{e}"))?;
    Ok(out)
}

/// `base`, a whole `effects.txt`, with each of `sections` in place of the one with its heading, or added at the end
/// when it has none; an error names what the game wouldn't read.
pub fn effects_with(base: &str, sections: &str) -> Result<String, String> {
    let mut out = base.to_string();
    let mut part = String::new();
    for line in sections.lines().chain(std::iter::once("[]")) {
        if heading(line).is_some() && !part.trim().is_empty() {
            out = with_section(&out, &part)?;
            part.clear();
        }
        part += line;
        part.push('\n');
    }
    super::check_table("effects.txt", &out)?;
    Ok(out)
}

/// `base` with `section` in place of the one with its heading, or added at the end when it has none.
fn with_section(base: &str, section: &str) -> Result<String, String> {
    let id = section.lines().find_map(heading).ok_or("the section has no [heading]")?;
    let mut out = String::new();
    let (mut skipping, mut placed) = (false, false);
    for line in base.lines() {
        if let Some(h) = heading(line) {
            skipping = h == id;
            if skipping && !placed {
                out += section.trim_end();
                out.push('\n');
                placed = true;
            }
        }
        if !skipping {
            out += line;
            out.push('\n');
        }
    }
    if !placed {
        out.push('\n');
        out += section.trim_end();
        out.push('\n');
    }
    Ok(out)
}

/// The range's buttons on the bottom screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
    LevelDown,
    LevelUp,
    KindBack,
    KindOn,
    Hand,
    Fire,
}

const ALL_KEYS: [Key; 6] = [Key::LevelDown, Key::LevelUp, Key::KindBack, Key::KindOn, Key::Hand, Key::Fire];

/// The button at a touch, with the buttons `down` further down the screen.
fn key_at([x, y]: [f32; 2], down: f32) -> Option<Key> {
    KEYS.iter().position(|&[kx, ky, w, h]| (kx..kx + w).contains(&x) && (ky + down..ky + down + h).contains(&y)).map(|i| ALL_KEYS[i])
}

pub(super) struct Range {
    /// The weapon, by its place in the book.
    def: usize,
    /// The zombie kind in the ring, by its place in the roster.
    kind: usize,
    /// The weapon fires only when asked, rather than whenever its cooldown runs out.
    by_hand: bool,
    /// The dog or drone the camera follows, by its place among the dogs then the drones.
    follow: Option<usize>,
    /// The next ring spot a zombie rises at.
    slot: usize,
    /// Real seconds the range has run, and the damage dealt by each of the last `WINDOW` seconds' frames.
    clock: f32,
    dealt: VecDeque<(f32, f32)>,
}

impl Range {
    /// Where a zombie at `pos` walks back to on the ring, and how fast, so knockback shows and the ring holds.
    pub(super) fn hold(&self, pos: [f32; 2], player: [f32; 2], radius: f32, speed: f32) -> ([f32; 2], f32) {
        let out = norm(sub(pos, player));
        let spot = [player[0] + out[0] * radius, player[1] + out[1] * radius];
        let back = sub(spot, pos);
        if len(back) > 2.0 { (norm(back), speed) } else { (norm(sub(player, pos)), 0.0) }
    }
}

impl Harvest {
    /// The weapon range for `setup`: its weapon, or the book's first when it names none, held alone at its level.
    pub fn range(seed: u64, setup: &RangeSetup) -> Self {
        let mut rng = Rng::new(seed);
        let map = Map::open_field(&mut rng, FIELD[0], FIELD[1]);
        let mut h = Self { calm: true, ..Self::with_map(seed, rng, map) };
        let b = book();
        let weapon = |i: &usize| b.defs[*i].kind == Card::Weapon;
        let def = b.index(&setup.weapon).filter(weapon).or_else(|| (0..b.defs.len()).find(weapon)).unwrap_or(0);
        h.kit = Kit::with_weapons(&[def], Difficulty::Normal);
        h.kit.weapons[0].level = setup.level.clamp(1, top_level(def));
        h.hp = h.kit.max_hp();
        h.grenades = h.kit.grenades();
        h.next_xp = u32::MAX;
        h.range = Some(Range { def, kind: roster().common(), by_hand: false, follow: None, slot: 0, clock: 0.0, dealt: VecDeque::new() });
        h
    }

    /// The ring's distance from the player, from the weapon's range at its level.
    pub(super) fn ring_radius(&self) -> f32 {
        let Some(r) = &self.range else { return 0.0 };
        let level = self.kit.weapons.first().map_or(1, |a| a.level);
        (book().defs[r.def].at(Stat::Range, level) * RING_SHARE).clamp(RING_TILES[0] * TILE, RING_TILES[1] * TILE)
    }

    /// The range's line for logs: the weapon by id, its level and the damage per second.
    pub(super) fn range_summary(&self) -> Option<String> {
        let r = self.range.as_ref()?;
        let level = self.kit.weapons.first().map_or(1, |a| a.level);
        Some(format!("range {} {level} dps={:.0} kills={}", book().defs[r.def].id, self.range_dps(), self.kills))
    }

    /// Damage per second over the last few seconds.
    pub(super) fn range_dps(&self) -> f32 {
        let Some(r) = &self.range else { return 0.0 };
        match (r.dealt.front(), r.dealt.back()) {
            (Some(a), Some(b)) if b.0 - a.0 > 0.5 => (b.1 - a.1) / (b.0 - a.0),
            _ => 0.0,
        }
    }

    /// The range's own controls, before the run reads the rest: Up and Down step the level, Left and Right pick the
    /// kind, Select switches to firing by hand and back, X fires by hand, R1 steps the camera through the dogs and
    /// back to the player, and a mouse drag or wheel orbits it; none of them move the player.
    pub(super) fn range_input(&mut self, input: &Input, sounds: &mut Vec<Sfx>) -> Option<Input> {
        self.range.as_ref()?;
        if self.menu.is_some() || !matches!(self.phase, Phase::Playing) {
            return None;
        }
        let o = self.orbit.get_or_insert([0.0; 2]);
        o[0] += input.orbit[0] * ORBIT_TURN;
        o[1] = (o[1] - input.orbit[1] * ORBIT_STEP).clamp(ORBIT_BACK[0], ORBIT_BACK[1]);
        let down = if self.single { TOP_KEYS } else { 0.0 };
        let tap = input.touch.filter(|_| input.touch_started).and_then(|p| key_at(p, down));
        let asked = |b: Buttons, k: Key| input.pressed.contains(b) || tap == Some(k);
        if asked(Buttons::UP, Key::LevelUp) {
            self.step_level(1, sounds);
        }
        if asked(Buttons::DOWN, Key::LevelDown) {
            self.step_level(-1, sounds);
        }
        if asked(Buttons::RIGHT, Key::KindOn) {
            self.step_kind(1, sounds);
        }
        if asked(Buttons::LEFT, Key::KindBack) {
            self.step_kind(-1, sounds);
        }
        if input.pressed.contains(Buttons::R1) {
            self.step_follow(sounds);
        }
        let r = self.range.as_mut()?;
        if asked(Buttons::SELECT, Key::Hand) {
            r.by_hand ^= true;
            sounds.push(Sfx::Tick(if r.by_hand { 3 } else { 6 }));
            for a in &mut self.kit.weapons {
                a.cd = 0.0;
            }
        }
        let fire = asked(Buttons::X, Key::Fire);
        if fire && !r.by_hand {
            r.by_hand = true;
        }
        if r.by_hand {
            for a in &mut self.kit.weapons {
                a.cd = if fire { 0.0 } else { f32::MAX };
            }
        }
        let mine = Buttons::UP | Buttons::DOWN | Buttons::LEFT | Buttons::RIGHT | Buttons::SELECT | Buttons::X | Buttons::R1;
        let mut own = *input;
        own.held = input.held & !mine;
        own.pressed = input.pressed & !mine;
        own.stick = [0.0; 2];
        if tap.is_some() {
            own.touch = None;
            own.touch_started = false;
        }
        Some(own)
    }

    /// Points the camera at the next dog or drone, or back at the player after the last.
    fn step_follow(&mut self, sounds: &mut Vec<Sfx>) {
        let (dogs, drones) = (self.dogs.len(), self.drones.len());
        let Some(r) = &mut self.range else { return };
        r.follow = match r.follow {
            None if dogs + drones > 0 => Some(0),
            Some(i) if i + 1 < dogs + drones => Some(i + 1),
            _ => None,
        };
        sounds.push(Sfx::Tick(r.follow.map_or(6, |i| i as u8 % 12)));
        let one = |i: usize, n: usize, what: &str| if n > 1 { format!("Watching {what} {} of {n}", i + 1) } else { format!("Watching the {what}") };
        let banner = match r.follow {
            Some(i) if i < dogs => one(i, dogs, "dog"),
            Some(i) => one(i - dogs, drones, "drone"),
            None if dogs + drones == 0 => "Nothing to watch".to_string(),
            None => "Watching you".to_string(),
        };
        self.juice.banner(banner, WHITE, 1.2);
    }

    /// Where the range's camera centers: the dog or drone it follows, if any.
    pub(super) fn range_focus(&self) -> Option<[f32; 2]> {
        let i = self.range.as_ref()?.follow?;
        self.dogs.get(i).map(|d| d.pos).or_else(|| self.drones.get(i - self.dogs.len()).map(|d| d.pos))
    }

    fn step_level(&mut self, by: i32, sounds: &mut Vec<Sfx>) {
        let Some(r) = &mut self.range else { return };
        let Some(a) = self.kit.weapons.first_mut() else { return };
        let level = (a.level as i32 + by).clamp(1, top_level(r.def) as i32) as u32;
        if level == a.level {
            return;
        }
        a.level = level;
        r.dealt.clear();
        sounds.push(Sfx::Tick(level.min(12) as u8));
        let name = &book().defs[r.def].name;
        self.juice.banner(format!("{name}  level {level}"), WHITE, 1.2);
    }

    /// Picks the next or last kind, and sends the ring back under to rise as it.
    fn step_kind(&mut self, by: i32, sounds: &mut Vec<Sfx>) {
        let Some(r) = &mut self.range else { return };
        let kinds = roster().enemies.len();
        r.kind = (r.kind as i32 + by).rem_euclid(kinds as i32) as usize;
        r.dealt.clear();
        sounds.push(Sfx::Tick(r.kind as u8 % 12));
        for z in self.zombies.drain(..).chain(self.burrowed.drain(..)) {
            self.fx.push(Fx::play(effects::effects().mound_breaks, [z.pos[0], z.pos[1] - 3.0], 0.0));
        }
        self.tells.clear();
        self.juice.banner(roster().enemies[r.kind].name.clone(), WHITE, 1.2);
    }

    /// After the run's update: fills the ring again, keeps the night and the player where they were, and counts
    /// the damage dealt.
    pub(super) fn range_after(&mut self, dt: f32) {
        let radius = self.ring_radius();
        let Some(r) = &mut self.range else { return };
        let kind = Kind(r.kind as u8);
        while self.zombies.len() + self.tells.len() + self.burrowed.len() < RING {
            let a = r.slot as f32 * TAU / RING as f32 + (r.slot / RING) as f32 * 0.37;
            r.slot += 1;
            let pos = [self.pos[0] + a.cos() * radius, self.pos[1] + a.sin() * radius];
            self.tells.push(Tell { kind, pos, hp: kind.stats().hp, t: 0.0, leader: false });
        }
        if self.t > CLOCK_LOOP {
            self.t -= CLOCK_LOOP;
        }
        self.xp = 0;
        self.hp = self.kit.max_hp();
        r.clock += dt;
        r.dealt.push_back((r.clock, self.juice.dealt));
        while r.dealt.front().is_some_and(|&(t, _)| t < r.clock - WINDOW) {
            r.dealt.pop_front();
        }
    }

    /// A fresh range with the same weapon, level and kind.
    pub(super) fn restart_range(&mut self, seed: u64) {
        let Some(r) = &self.range else { return };
        let (def, kind, by_hand, follow) = (r.def, r.kind, r.by_hand, r.follow);
        let level = self.kit.weapons.first().map_or(1, |a| a.level);
        let setup = RangeSetup { weapon: book().defs[def].id.clone(), level, section: None, effects: None };
        let orbit = self.orbit;
        *self = Harvest::range(seed, &setup).single_screen(self.single);
        self.orbit = orbit;
        if let Some(r) = &mut self.range {
            (r.kind, r.by_hand, r.follow) = (kind, by_hand, follow);
        }
    }

    /// The weapon, its level, the kind and damage per second over the top screen; the buttons on the bottom one.
    pub(super) fn draw_range(&self, top: &mut Canvas, bottom: &mut Canvas) {
        let Some(r) = &self.range else { return };
        let def = &book().defs[r.def];
        let level = self.kit.weapons.first().map_or(1, |a| a.level);
        let how = if r.by_hand { "by hand" } else { "on cooldown" };
        let kind = &roster().enemies[r.kind].name;
        top.rect(16.0, 64.0, 620.0, 96.0, [0.0, 0.0, 0.0, 0.6]);
        let shown = if level > def.max { format!("level breaker {level}") } else { format!("level {level}/{}", def.max) };
        top.label(format!("{}  {shown}  {how}", def.name), 28.0, 72.0, 32.0, WHITE, Align::Left);
        top.label(format!("{:.0} damage a second   vs {kind}", self.range_dps()), 28.0, 114.0, 32.0, GOLD, Align::Left);
        let (keys, down) = if self.single { (top, TOP_KEYS) } else { (bottom, 0.0) };
        let camera = crate::pad::hint(match (self.dogs.is_empty(), self.drones.is_empty()) {
            (true, true) => "Right-drag, wheel: camera",
            (false, _) => "Right-drag, wheel: camera   {R1}: watch the dog",
            (true, false) => "Right-drag, wheel: camera   {R1}: watch the drone",
        });
        if self.single {
            keys.label(camera, 28.0, KEYS[0][1] + KEYS[0][3] + down + 8.0, 26.0, WHITE, Align::Left);
        } else {
            keys.rect(0.0, 0.0, SCREEN_W, 180.0, [0.0, 0.0, 0.0, 0.7]);
            keys.label(crate::pad::hint("Up/Down: level   Left/Right: kind   {Select}: by hand   {X}: fire"), SCREEN_W / 2.0, 20.0, 28.0, WHITE, Align::Center);
            keys.label(camera, SCREEN_W / 2.0, 56.0, 26.0, WHITE, Align::Center);
        }
        let labels = ["Level -", "Level +", "< Kind", "Kind >", if r.by_hand { "On cooldown" } else { "By hand" }, "Fire"];
        for (&[x, y, w, h], label) in KEYS.iter().zip(labels) {
            keys.rect(x, y + down, w, h, [0.0, 0.0, 0.0, 0.5]);
            keys.label(label, x + w / 2.0, y + down + h / 2.0 - 16.0, 30.0, WHITE, Align::Center);
        }
    }
}

/// The highest level the range steps a weapon to: one past its top while the level breaker is on.
fn top_level(def: usize) -> u32 {
    book().defs[def].max + (crate::tuning::tuning().breaker > 0.0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn press(b: Buttons) -> Input {
        Input { held: b, pressed: b, ..Input::default() }
    }

    fn run(h: &mut Harvest, secs: f32, input: Input) {
        let mut sounds = Vec::new();
        h.update(&input, DT, &mut sounds);
        for _ in 0..(secs / DT) as usize {
            h.update(&Input::default(), DT, &mut sounds);
        }
    }

    fn pistol(level: u32) -> Harvest {
        Harvest::range(3, &RangeSetup { weapon: "pistol".into(), level, section: None, effects: None })
    }

    #[test]
    fn a_setup_reads_an_id_and_level_or_a_section() {
        assert_eq!(RangeSetup::read("shotgun").unwrap(), RangeSetup { weapon: "shotgun".into(), level: 1, section: None, effects: None });
        assert_eq!(RangeSetup::read("shotgun:3").unwrap().level, 3);
        assert!(RangeSetup::read("shotgun:0").is_err());
        let s = RangeSetup::read("level 4\n[pea]\nkind weapon\ndoes pistol\nicon ICON_PISTOL\nmax 6\ndamage 3 4 5\n").unwrap();
        assert_eq!((s.weapon.as_str(), s.level), ("pea", 4));
        assert_eq!(RangeSetup::read(&s.text()).unwrap(), s, "what text writes, read takes back");
        assert!(RangeSetup::read("[boots]\nkind passive\nicon ICON_PISTOL\n").unwrap_err().contains("isn't a weapon"));
        assert!(RangeSetup::read("[pea]\nkind weapon\ndoes pistol\ndamage lots\n").unwrap_err().starts_with("powerups.txt:"));
        assert!(RangeSetup::read("hello\n[pea]\nkind weapon\n").is_err());
    }

    #[test]
    fn a_setup_carries_the_shot_style_its_bullet_fires_as() {
        let text = "level 2\n[pistol]\nkind weapon\nbullet pistol_shot\nicon ICON_PISTOL\nmax 8\neffects.txt\n[pistol_shot]\ncolor D98A3C\nlength 30\ntracer SHOT_ROUND\n";
        let s = RangeSetup::read(text).unwrap();
        assert_eq!(s.effects.as_deref(), Some("[pistol_shot]\ncolor D98A3C\nlength 30\ntracer SHOT_ROUND\n"));
        assert_eq!(RangeSetup::read(&s.text()).unwrap(), s, "what text writes, read takes back");
        let base = super::super::built_in_table("effects.txt").unwrap();
        let laid = effects_with(base, s.effects.as_deref().unwrap()).unwrap();
        assert_eq!(super::super::effects::shot_named(&laid, Some("pistol_shot")).map(|shot| shot.length), Some(30.0));
        assert!(RangeSetup::read(&text.replace("length 30", "length lots")).unwrap_err().starts_with("effects.txt:"));
        assert!(RangeSetup::read("level 2\n[pistol]\nkind weapon\nicon ICON_PISTOL\neffects.txt\n").unwrap().effects.is_none(), "no sections, nothing to lay over");
    }

    #[test]
    fn a_section_replaces_its_own_or_joins_the_end() {
        let base = super::super::built_in_table("powerups.txt").unwrap();
        let out = powerups_with(base, "[pistol]\nkind weapon\ndoes pistol\nicon ICON_PISTOL\nmax 3\n").unwrap();
        let pistol = powerups::parse(&out).unwrap().defs.into_iter().find(|d| d.id == "pistol").unwrap();
        assert_eq!(pistol.max, 3);
        assert_eq!(powerups::parse(&out).unwrap().defs.len(), powerups::parse(base).unwrap().defs.len(), "in place of its own");
        let out = powerups_with(base, "[pea]\nkind weapon\ndoes pistol\nicon ICON_PISTOL\n").unwrap();
        assert!(out.starts_with(base.trim_end()) && out.ends_with("[pea]\nkind weapon\ndoes pistol\nicon ICON_PISTOL\n"));
        assert!(powerups_with(base, "[pea]\nkind weapon\nmax none\n").is_err());
    }

    #[test]
    fn the_range_holds_only_its_weapon_at_its_level_against_a_ring() {
        let mut h = pistol(3);
        assert_eq!(h.kit.weapons.len(), 1);
        assert_eq!((book().defs[h.kit.weapons[0].def].id.as_str(), h.kit.weapons[0].level), ("pistol", 3));
        assert!(h.kit.passives.is_empty());
        run(&mut h, 0.1, Input::default());
        assert_eq!(h.tells.len() + h.zombies.len(), RING);
        let radius = h.ring_radius();
        assert!(h.tells.iter().all(|t| (len(sub(t.pos, h.pos)) - radius).abs() < 0.5));
    }

    #[test]
    fn on_cooldown_it_kills_and_the_ring_rises_again() {
        let mut h = pistol(8);
        let start = h.pos;
        run(&mut h, 12.0, Input::default());
        assert!(h.kills > 0, "the pistol killed something");
        assert!(h.range_dps() > 0.0);
        assert_eq!(h.zombies.len() + h.tells.len() + h.burrowed.len(), RING, "the dead rise again");
        assert_eq!(h.pos, start);
        assert_eq!(h.hp, h.kit.max_hp(), "the ring never bites");
        assert_eq!(h.level, 1, "no level-ups");
        assert!(h.zombies.iter().all(|z| len(sub(z.pos, h.pos)) > h.ring_radius() * 0.5), "the ring stands its ground");
    }

    #[test]
    fn by_hand_it_fires_only_when_asked() {
        let mut h = pistol(1);
        run(&mut h, 2.0, Input::default());
        run(&mut h, 1.0, press(Buttons::SELECT));
        let before = h.juice.dealt;
        run(&mut h, 3.0, Input::default());
        assert_eq!(h.juice.dealt, before, "nothing fired by itself");
        run(&mut h, 0.5, press(Buttons::X));
        assert!(h.juice.dealt > before, "X fired");
    }

    #[test]
    fn the_pad_steps_the_level_and_the_kind_in_place() {
        let mut h = pistol(1);
        let start = h.pos;
        run(&mut h, 0.0, press(Buttons::UP));
        run(&mut h, 0.0, press(Buttons::UP));
        assert_eq!(h.kit.weapons[0].level, 3);
        run(&mut h, 0.0, press(Buttons::DOWN));
        assert_eq!(h.kit.weapons[0].level, 2);
        for _ in 0..20 {
            run(&mut h, 0.0, press(Buttons::DOWN));
        }
        assert_eq!(h.kit.weapons[0].level, 1, "no lower than 1");
        let kind = h.range.as_ref().unwrap().kind;
        run(&mut h, 1.0, press(Buttons::RIGHT));
        let now = h.range.as_ref().unwrap().kind;
        assert_eq!(now, (kind + 1) % roster().enemies.len());
        assert!(h.tells.iter().map(|t| t.kind.0).chain(h.zombies.iter().map(|z| z.kind.0)).all(|k| k as usize == now));
        assert_eq!(h.pos, start, "the pad never walked");
    }

    #[test]
    fn a_tap_on_a_button_steps_and_throws_nothing() {
        let mut h = pistol(1);
        let [x, y, w, hh] = KEYS[1];
        let tap = Input { touch: Some([x + w / 2.0, y + hh / 2.0]), touch_started: true, ..Input::default() };
        run(&mut h, 0.0, tap);
        assert_eq!(h.kit.weapons[0].level, 2);
        assert!(h.thrown.is_empty());
        let mut one = pistol(1).single_screen(true);
        let [x, y, w, hh] = KEYS[5];
        run(&mut one, 1.0, Input { touch: Some([x + w / 2.0, y + TOP_KEYS + hh / 2.0]), touch_started: true, ..Input::default() });
        assert!(one.range.as_ref().unwrap().by_hand, "on one screen, the buttons sit on the top one");
    }

    #[test]
    fn a_drag_and_the_wheel_orbit_the_camera_within_its_reach() {
        let mut h = pistol(1);
        let start = h.pos;
        run(&mut h, 0.0, Input { orbit: [100.0, 0.0], ..Input::default() });
        let [turn, back] = h.orbit.unwrap();
        assert!((turn - 100.0 * ORBIT_TURN).abs() < 1e-5 && back == 0.0);
        run(&mut h, 0.0, Input { orbit: [0.0, -3.0], ..Input::default() });
        assert_eq!(h.orbit.unwrap()[1], 3.0 * ORBIT_STEP, "scrolling down backs the camera off");
        run(&mut h, 0.0, Input { orbit: [0.0, -1000.0], ..Input::default() });
        assert_eq!(h.orbit.unwrap()[1], ORBIT_BACK[1]);
        run(&mut h, 0.0, Input { orbit: [0.0, 1000.0], ..Input::default() });
        assert_eq!(h.orbit.unwrap()[1], ORBIT_BACK[0]);
        assert_eq!(h.pos, start, "the camera never walked the player");
        let eye = |h: &Harvest| super::super::view3d::scene(h).eye;
        let before = eye(&h);
        run(&mut h, 0.0, Input { orbit: [200.0, 0.0], ..Input::default() });
        assert_ne!(eye(&h), before, "the 3D view swung");
    }

    #[test]
    fn r1_steps_the_camera_through_the_dogs_and_back_to_the_player() {
        let mut h = pistol(1);
        run(&mut h, 0.0, press(Buttons::R1));
        assert_eq!(h.range_focus(), None, "no dog, nothing to watch");
        let mut h = Harvest::range(3, &RangeSetup { weapon: "dog".into(), level: 8, section: None, effects: None });
        run(&mut h, 3.0, Input::default());
        let dogs = h.dogs.len();
        assert!(dogs > 1, "level 8 runs more than one dog");
        let start = h.pos;
        for i in 0..dogs {
            run(&mut h, 0.0, press(Buttons::R1));
            assert_eq!(h.range_focus(), Some(h.dogs[i].pos), "dog {i}");
        }
        run(&mut h, 1.0, Input::default());
        assert_eq!(h.range_focus(), Some(h.dogs[dogs - 1].pos), "the camera stays on the running dog");
        let away = super::super::view3d::scene(&h).eye;
        run(&mut h, 0.0, press(Buttons::R1));
        assert_eq!(h.range_focus(), None, "after the last dog, back to the player");
        assert_ne!(super::super::view3d::scene(&h).eye, away);
        assert_eq!(h.pos, start, "R1 never walked the player");
    }

    #[test]
    fn the_level_steps_up_to_the_level_breaker_and_no_further() {
        let mut h = Harvest::range(3, &RangeSetup { weapon: "maul".into(), level: 99, section: None, effects: None });
        let max = book().defs[h.kit.weapons[0].def].max;
        assert_eq!(h.kit.weapons[0].level, max + 1);
        h.step_level(1, &mut Vec::new());
        assert_eq!(h.kit.weapons[0].level, max + 1);
        h.step_level(-1, &mut Vec::new());
        assert_eq!(h.kit.weapons[0].level, max);
    }

    #[test]
    fn r1_steps_the_camera_through_the_drones_too() {
        let mut h = Harvest::range(3, &RangeSetup { weapon: "drone".into(), level: 8, section: None, effects: None });
        run(&mut h, 3.0, Input::default());
        let drones = h.drones.len();
        assert!(drones > 1, "level 8 flies more than one drone");
        for i in 0..drones {
            run(&mut h, 0.0, press(Buttons::R1));
            assert_eq!(h.range_focus(), Some(h.drones[i].pos), "drone {i}");
        }
        run(&mut h, 0.0, press(Buttons::R1));
        assert_eq!(h.range_focus(), None);
    }
}
