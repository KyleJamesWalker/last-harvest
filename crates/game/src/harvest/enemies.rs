//! The kinds of zombie, read from `enemies.txt` and from enemy cards.

use std::sync::OnceLock;

use super::Look;
use crate::art::{self, ATLAS_H, ATLAS_W, Art};
use crate::data::Card;

#[derive(Clone)]
pub struct Enemy {
    pub id: String,
    pub name: String,
    pub look: Look,
    pub hp: f32,
    pub speed: f32,
    pub damage: f32,
    pub xp: u32,
    /// Divides knockback.
    pub mass: f32,
    pub gibs: u32,
    pub sound: String,
    pub after: f32,
    pub chance: f32,
    pub per_minute: f32,
    pub heavy: bool,
    /// It came from an enemy card rather than the game's own file.
    pub from_card: bool,
}

impl Enemy {
    /// Whether it can arrive `minutes` into the night, given a roll from 0 to 1.
    pub fn arrives(&self, minutes: f32, roll: f32) -> bool {
        minutes >= self.after && roll < self.chance + self.per_minute * minutes
    }
}

pub struct Roster {
    pub enemies: Vec<Enemy>,
    pub leader: usize,
    pub harvester: usize,
}

/// The most kinds a roster holds, so a save can keep the ones turned off as bits.
const MAX_KINDS: usize = 64;
/// The most frames a walk cycle can have.
pub const MAX_WALK: usize = 8;
const MAX_FRAME: u32 = 64;
const PAD: u16 = 1;

impl Roster {
    pub fn index(&self, id: &str) -> Option<usize> {
        self.enemies.iter().position(|e| e.id == id)
    }

    /// The kind that arrives `minutes` into the night for a roll from 0 to 1: the first whose chance is above it,
    /// skipping kinds in the `off` mask, else the last kind left on. With every kind off, none are skipped.
    pub fn pick(&self, minutes: f32, roll: f32, off: u64) -> usize {
        let off = if self.all_off(off) { 0 } else { off };
        let on = |i: &usize| off & 1 << i == 0;
        let all = 0..self.enemies.len();
        all.clone().filter(on).find(|&i| self.enemies[i].arrives(minutes, roll)).or_else(|| all.rev().find(on)).unwrap_or(self.common())
    }

    pub fn all_off(&self, off: u64) -> bool {
        (0..self.enemies.len()).all(|i| off & 1 << i != 0)
    }

    /// The share of spawns each kind takes `minutes` into the night, by `pick`'s rule: each kind takes the rolls
    /// between the highest chance above it and its own, and the last kind takes every roll the rest miss.
    pub fn shares(&self, minutes: f32) -> Vec<f32> {
        let mut out = vec![0.0; self.enemies.len()];
        let mut covered = 0.0f32;
        for (i, e) in self.enemies.iter().enumerate() {
            let chance = if minutes >= e.after { (e.chance + e.per_minute * minutes).clamp(0.0, 1.0) } else { 0.0 };
            if chance > covered {
                out[i] = chance - covered;
                covered = chance;
            }
        }
        out[self.common()] += 1.0 - covered;
        out
    }

    /// The last kind, which the list ends on for every roll the others miss.
    pub fn common(&self) -> usize {
        self.enemies.len() - 1
    }

    /// Adds the cards' kinds: one sharing an id takes that kind's place, and a new one goes just before the last
    /// kind, so the list still ends on the file's own.
    fn with(mut self, cards: &[Entry]) -> Self {
        let mut leader = self.enemies[self.leader].id.clone();
        let mut harvester = self.enemies[self.harvester].id.clone();
        for card in cards {
            let e = Enemy { from_card: true, ..card.enemy.clone() };
            if card.leader {
                leader = e.id.clone();
            }
            if card.harvester {
                harvester = e.id.clone();
            }
            match self.index(&e.id) {
                Some(i) => self.enemies[i] = e,
                None => self.enemies.insert(self.common(), e),
            }
        }
        self.leader = self.index(&leader).unwrap_or(0);
        self.harvester = self.index(&harvester).unwrap_or(0);
        self
    }
}

/// The kinds a save turns off, read from comma-separated ids as a mask by place in the roster.
pub fn off_from_ids(text: &str) -> u64 {
    text.split(',').filter_map(|id| roster().index(id.trim())).fold(0, |m, i| m | 1 << i)
}

pub fn off_ids(off: u64) -> String {
    let r = roster();
    (0..r.enemies.len()).filter(|i| off & 1 << i != 0).map(|i| r.enemies[i].id.as_str()).collect::<Vec<_>>().join(",")
}

static CARDS: OnceLock<Vec<Entry>> = OnceLock::new();

/// The zombies in the shipped file, with the installed cards.
pub fn roster() -> &'static Roster {
    static ROSTER: OnceLock<Roster> = OnceLock::new();
    ROSTER.get_or_init(|| {
        let base = parse(crate::data::enemies()).unwrap_or_else(|e| panic!("enemies.txt:{e}"));
        base.with(CARDS.get().map_or(&[], Vec::as_slice))
    })
}

fn frames<const N: usize>(name: &str) -> Result<[Art; N], String> {
    let (_, frames) = art::ALL.iter().find(|(n, _)| *n == name).ok_or_else(|| format!(" no sprite called {name}"))?;
    <[Art; N]>::try_from(*frames).map_err(|_| format!(" {name} has {} frames, not {N}", frames.len()))
}

fn look(prefix: &str) -> Result<Look, String> {
    let walk = |facing: &str| {
        let name = format!("{prefix}_{facing}");
        let (_, frames) = art::ALL.iter().find(|(n, _)| *n == name).ok_or_else(|| format!(" no sprite called {name}"))?;
        if !(1..=MAX_WALK).contains(&frames.len()) {
            return Err(format!(" {name} has {} frames, but a walk has 1 to {MAX_WALK}", frames.len()));
        }
        Ok(frames.to_vec())
    };
    Ok(Look { walk: [walk("FRONT")?, walk("SIDE")?, walk("BACK")?], flash: frames(&format!("{prefix}_FLASH"))?, hurt: frames(&format!("{prefix}_HURT"))? })
}

/// How many frames the editor should let a sprite have, when the game needs a set number or range: the player's
/// sprites and each zombie's flash and hurt hold one per facing, and a zombie's walk has 1 to `MAX_WALK`.
pub fn frame_limits(name: &str) -> Option<(usize, usize)> {
    let (prefix, part) = name.rsplit_once('_')?;
    let zombie = || entries(crate::data::enemies()).ok()?.into_iter().find(|e| e.look.as_ref().is_some_and(|(_, p)| p == prefix));
    match part {
        "FLASH" | "HURT" if prefix == "PLAYER" || zombie().is_some() => Some((3, 3)),
        "FRONT" | "SIDE" | "BACK" if prefix == "PLAYER" => Some((3, 3)),
        "FRONT" | "SIDE" | "BACK" if zombie().is_some() => Some((1, MAX_WALK)),
        _ => None,
    }
}

/// One `[id]` section: the kind, the line and name of its `look`, and whether it leads hordes or dresses the Harvester.
#[derive(Clone)]
struct Entry {
    enemy: Enemy,
    look: Option<(usize, String)>,
    leader: bool,
    harvester: bool,
    /// Walk frames per facing on a card's sheet.
    walk: usize,
}

fn entries(text: &str) -> Result<Vec<Entry>, String> {
    let mut entries: Vec<Entry> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(id) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            if entries.iter().any(|e| e.enemy.id == id) {
                return Err(format!("{n}: [{id}] appears twice"));
            }
            let blank = Look { walk: std::array::from_fn(|_| vec![art::WHITE; 3]), flash: [art::WHITE; 3], hurt: [art::WHITE; 3] };
            let enemy = Enemy {
                id: id.to_string(),
                name: id.to_string(),
                look: blank,
                hp: 10.0,
                speed: 20.0,
                damage: 10.0,
                xp: 1,
                mass: 1.0,
                gibs: 9,
                sound: format!("kill-{id}"),
                after: 0.0,
                chance: 1.0,
                per_minute: 0.0,
                heavy: false,
                from_card: false,
            };
            entries.push(Entry { enemy, look: None, leader: false, harvester: false, walk: 3 });
            continue;
        }
        let Some(entry) = entries.last_mut() else { return Err(format!("{n}: expected an [enemy] first")) };
        let e = &mut entry.enemy;
        let (key, rest) = line.split_once(char::is_whitespace).map(|(k, r)| (k, r.trim())).unwrap_or((line, ""));
        let number = || rest.parse::<f32>().ok().filter(|v| *v >= 0.0).ok_or_else(|| format!("{n}: {key} is a number, not {rest:?}"));
        match key {
            "name" => e.name = rest.to_string(),
            "look" => entry.look = Some((n, rest.to_string())),
            "hp" => e.hp = number()?,
            "speed" => e.speed = number()?,
            "damage" => e.damage = number()?,
            "xp" => e.xp = number()? as u32,
            "mass" => e.mass = number()?.max(0.1),
            "gibs" => e.gibs = number()? as u32,
            "sound" => e.sound = rest.to_string(),
            "after" => e.after = number()?,
            "chance" => e.chance = number()?,
            "per_minute" => e.per_minute = number()?,
            "walk" => {
                entry.walk = rest.parse().ok().filter(|n| (1..=MAX_WALK).contains(n)).ok_or_else(|| format!("{n}: walk is 1 to {MAX_WALK} frames, not {rest:?}"))?;
            }
            "heavy" if rest.is_empty() => e.heavy = true,
            "leader" if rest.is_empty() => entry.leader = true,
            "harvester" if rest.is_empty() => entry.harvester = true,
            _ => return Err(format!("{n}: [{}] has no setting {key:?}", e.id)),
        }
    }
    Ok(entries)
}

/// Reads the enemy file; an error names the line.
pub fn parse(text: &str) -> Result<Roster, String> {
    let entries = entries(text)?;
    if entries.is_empty() || entries.len() > MAX_KINDS {
        return Err(format!(" {} enemies, but there can be 1 to {MAX_KINDS}", entries.len()));
    }
    let leader = entries.iter().position(|e| e.leader).unwrap_or(0);
    let harvester = entries.iter().position(|e| e.harvester).unwrap_or(0);
    let mut enemies = Vec::new();
    for Entry { mut enemy, look: at, .. } in entries {
        let (n, prefix) = at.ok_or_else(|| format!(" [{}] has no look", enemy.id))?;
        enemy.look = look(&prefix).map_err(|err| format!("{n}:{err}"))?;
        enemies.push(enemy);
    }
    Ok(Roster { leader, harvester, enemies })
}

/// One card's kind, from its text: a single `enemies.txt` section whose sprites come from the card's sheet.
fn card_entry(text: &str) -> Result<Entry, String> {
    let mut entries = entries(text).map_err(|e| format!("line {e}"))?;
    match (entries.len(), entries.first().and_then(|e| e.look.as_ref())) {
        (1, None) => Ok(entries.remove(0)),
        (1, Some(_)) => Err("a card's sprites come from its sheet, so it has no look".into()),
        (n, _) => Err(format!("a card holds one enemy, not {n}")),
    }
}

/// How a night's spawns split between the kinds: the file's own with these cards added.
pub struct SpawnMix {
    /// Each kind's id and name, in roster order.
    pub kinds: Vec<(String, String)>,
    /// The kinds in the file that a card replaces, by id.
    pub replaced: Vec<String>,
    /// Each kind's share of spawns at each of the minutes asked for, minute by minute.
    pub shares: Vec<Vec<f32>>,
}

/// The spawn mix through the night with `cards` added to the enemy file, at each of `minutes`. Horde leaders
/// and kinds a player turns off aren't counted.
pub fn spawn_mix(cards: &[&str], minutes: &[f32]) -> Result<SpawnMix, String> {
    let base = parse(crate::data::enemies()).map_err(|e| format!("enemies.txt:{e}"))?;
    let entries = cards.iter().map(|t| card_entry(t)).collect::<Result<Vec<_>, _>>()?;
    let replaced = entries.iter().map(|e| e.enemy.id.clone()).filter(|id| base.index(id).is_some()).collect();
    let r = base.with(&entries);
    Ok(SpawnMix {
        kinds: r.enemies.iter().map(|e| (e.id.clone(), e.name.clone())).collect(),
        replaced,
        shares: minutes.iter().map(|&m| r.shares(m)).collect(),
    })
}

/// A card's settings as the game reads them, defaults filled in, for an editor to show.
pub struct CardFields {
    pub id: String,
    pub name: String,
    /// Every number a card sets, by key, in `NUMBERS` order.
    pub numbers: Vec<(&'static str, f32)>,
    pub sound: String,
    /// Every flag a card sets, by key, in `FLAGS` order.
    pub flags: Vec<(&'static str, bool)>,
    /// Walk frames per facing.
    pub walk: usize,
}

pub const NUMBERS: [&str; 9] = ["hp", "speed", "damage", "xp", "mass", "gibs", "after", "chance", "per_minute"];
pub const FLAGS: [&str; 3] = ["heavy", "leader", "harvester"];

/// Reads a card's text as the game would, or says why it can't.
pub fn card_fields(text: &str) -> Result<CardFields, String> {
    let entry = card_entry(text)?;
    let e = &entry.enemy;
    let numbers = [e.hp, e.speed, e.damage, e.xp as f32, e.mass, e.gibs as f32, e.after, e.chance, e.per_minute];
    Ok(CardFields {
        id: e.id.clone(),
        name: e.name.clone(),
        numbers: NUMBERS.into_iter().zip(numbers).collect(),
        sound: e.sound.clone(),
        flags: FLAGS.into_iter().zip([e.heavy, entry.leader, entry.harvester]).collect(),
        walk: entry.walk,
    })
}

/// A card's text with `key` set to `value`, or a flag set when `value` is empty, or the key removed when None.
/// Other lines, comments among them, stay as they were.
pub fn set_card_field(text: &str, key: &str, value: Option<&str>) -> String {
    let line = value.map(|v| if v.is_empty() { key.to_string() } else { format!("{key:<11} {v}") });
    let mut out: Vec<String> = Vec::new();
    let mut placed = false;
    for raw in text.lines() {
        let words = raw.split('#').next().unwrap_or("").trim();
        if words.split_whitespace().next() == Some(key) {
            if let Some(l) = line.as_ref().filter(|_| !placed) {
                out.push(l.clone());
            }
            placed = true;
        } else {
            out.push(raw.to_string());
        }
    }
    if let Some(l) = line.filter(|_| !placed) {
        while out.last().is_some_and(|l| l.trim().is_empty()) {
            out.pop();
        }
        out.push(l);
    }
    out.join("\n") + "\n"
}

/// Where a card's frames go in the texture: shelves below the packed sprites, carrying on from `cursor`.
pub(super) fn place(cursor: &mut [u16; 3], [w, h]: [u16; 2]) -> Option<[u16; 2]> {
    let [x, y, shelf] = cursor;
    if *x + w + PAD > ATLAS_W as u16 {
        (*x, *y, *shelf) = (PAD, *y + *shelf + PAD, 0);
    }
    if *y + h > ATLAS_H as u16 {
        return None;
    }
    let at = [*x, *y];
    *x += w + PAD;
    *shelf = (*shelf).max(h);
    Some(at)
}

/// Draws each enemy and player card's frames into the free rows of `atlas` and keeps its kind for the roster and
/// its look for the player; `base` is the enemy file the enemy cards join. Returns a line for each card left out.
pub(crate) fn install_cards(cards: &[Card], players: &[Card], base: &str, atlas: &mut [u8]) -> Vec<String> {
    let mut cursor = start();
    let (mut notes, mut rects, kept) = pack(cards, base, atlas, &mut cursor);
    let (more, player_rects, looks) = super::players::pack(players, atlas, &mut cursor, art::RECTS.len() + rects.len());
    notes.extend(more);
    rects.extend(player_rects);
    art::set_added(rects);
    CARDS.set(kept).ok();
    super::players::install(looks);
    notes
}

/// Where the first card's frames go: the start of the free rows below the packed sprites.
pub(super) fn start() -> [u16; 3] {
    [PAD, art::free_top() + PAD, 0]
}

/// Copies the cell at `col` and `row` of a sheet `cols` frames wide into the atlas at `at`, with nearly clear
/// pixels made clear so they don't show as faint squares.
pub(super) fn draw_cell(card: &Card, cols: u32, (col, row): (u32, u32), [x, y]: [u16; 2], atlas: &mut [u8]) {
    let [fw, fh] = card.frame;
    for py in 0..fh {
        for px in 0..fw {
            let s = (((row * fh + py) * fw * cols + col * fw + px) * 4) as usize;
            let d = (((y as u32 + py) * ATLAS_W + x as u32 + px) * 4) as usize;
            let p = &card.sheet[s..s + 4];
            let solid = if p[3] >= 128 { [p[0], p[1], p[2], 255] } else { [0; 4] };
            atlas[d..d + 4].copy_from_slice(&solid);
        }
    }
}

/// The cards' kinds with their sprites drawn into `atlas` from `cursor` on, the rects drawn, and a line for each
/// card left out.
fn pack(cards: &[Card], base: &str, atlas: &mut [u8], cursor: &mut [u16; 3]) -> (Vec<String>, Vec<[u16; 4]>, Vec<Entry>) {
    let mut ids: Vec<String> = entries(base).map(|es| es.into_iter().map(|e| e.enemy.id).collect()).unwrap_or_default();
    let (mut notes, mut rects, mut kept) = (Vec::new(), Vec::new(), Vec::new());
    for card in cards {
        let mut entry = match card_entry(&card.text) {
            Ok(e) => e,
            Err(e) => {
                notes.push(format!("{}: {e}", card.name));
                continue;
            }
        };
        let [fw, fh] = card.frame;
        let (walk, cols) = (entry.walk, entry.walk.max(3) as u32);
        if !(1..=MAX_FRAME).contains(&fw) || !(1..=MAX_FRAME).contains(&fh) || card.sheet.len() != (fw * cols * fh * 5 * 4) as usize {
            notes.push(format!("{}: frames are 1 to {MAX_FRAME} pixels, {cols} across and five down", card.name));
            continue;
        }
        // Each walking row's frames, then a flash and a hurt frame per facing, by column and row on the sheet.
        let cells: Vec<(u32, u32)> = (0..5u32).flat_map(|row| (0..if row < 3 { walk as u32 } else { 3 }).map(move |col| (col, row))).collect();
        let new = !ids.contains(&entry.enemy.id);
        if new && ids.len() >= MAX_KINDS {
            notes.push(format!("{}: there can be only {MAX_KINDS} kinds", card.name));
            continue;
        }
        let mut try_cursor = *cursor;
        let spots: Option<Vec<[u16; 2]>> = cells.iter().map(|_| place(&mut try_cursor, [fw as u16, fh as u16])).collect();
        let Some(spots) = spots else {
            notes.push(format!("{}: no room left in the sprite texture", card.name));
            continue;
        };
        *cursor = try_cursor;
        let first = art::RECTS.len() + rects.len();
        for (&cell, at) in cells.iter().zip(spots) {
            draw_cell(card, cols, cell, at, atlas);
            rects.push([at[0], at[1], fw as u16, fh as u16]);
        }
        let at = |k: usize| Art((first + k) as u16);
        let still = 3 * walk;
        entry.enemy.look = Look {
            walk: std::array::from_fn(|f| (f * walk..(f + 1) * walk).map(at).collect()),
            flash: [at(still), at(still + 1), at(still + 2)],
            hurt: [at(still + 3), at(still + 4), at(still + 5)],
        };
        if new {
            ids.push(entry.enemy.id.clone());
        }
        kept.push(entry);
    }
    (notes, rects, kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_file_parses() {
        let r = roster();
        assert_eq!(r.enemies.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["big", "kid", "skinny"]);
        let big = &r.enemies[r.index("big").unwrap()];
        assert!(big.heavy && r.leader == 0 && r.harvester == 0);
        assert_eq!(big.look.walk[0], art::BIG_FRONT.to_vec());
        assert_eq!(big.sound, "kill-big");
        assert_eq!(r.common(), r.index("skinny").unwrap());
    }

    #[test]
    fn spawn_rolls_match_the_old_odds() {
        let r = roster();
        let id = |minutes: f32, roll: f32| r.enemies[r.pick(minutes, roll, 0)].id.as_str();
        assert_eq!(id(0.5, 0.0), "skinny", "nothing but skinnies in the first minute");
        assert_eq!(id(1.5, 0.0), "kid");
        assert_eq!(id(1.5, 0.35), "skinny");
        assert_eq!(id(5.0, 0.10), "big", "bigs take rolls under 0.06 plus 0.01 a minute after two minutes");
        assert_eq!(id(5.0, 0.11), "kid");
    }

    #[test]
    fn kinds_turned_off_never_arrive() {
        let r = roster();
        let bit = |id: &str| 1u64 << r.index(id).unwrap();
        let id = |off: u64, roll: f32| r.enemies[r.pick(5.0, roll, off)].id.as_str();
        assert_eq!(id(bit("big"), 0.0), "kid", "the roll a big would take goes to the next kind");
        assert_eq!(id(bit("skinny"), 0.9), "kid", "a roll every chance misses goes to the last kind left on");
        assert_eq!(id(bit("big") | bit("kid") | bit("skinny"), 0.9), "skinny", "with every kind off, none are skipped");
        assert_eq!(off_from_ids("kid, nope,big"), bit("kid") | bit("big"));
        assert_eq!(off_ids(bit("kid") | bit("big")), "big,kid");
    }

    /// A card whose sheet is opaque all over.
    fn card(text: &str, frame: [u32; 2]) -> Card {
        let [w, h] = frame;
        let sheet = (0..w * 3 * h * 5).flat_map(|i| [(i % 251) as u8, 40, 90, 255]).collect();
        Card { name: "test.png".into(), text: text.into(), frame, sheet }
    }

    #[test]
    fn a_card_adds_a_kind_before_the_last_and_draws_its_frames() {
        let base = include_str!("../../enemies.txt");
        let mut atlas = vec![0u8; (ATLAS_W * ATLAS_H * 4) as usize];
        let (notes, rects, kept) = pack(&[card("[ghoul]\nname Ghoul\nhp 40\nleader", [12, 20])], base, &mut atlas, &mut start());
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(rects.len(), 15, "three walking frames a facing, then a flash and a hurt frame each");
        assert!(rects.iter().all(|r| r[1] > art::free_top() && r[2..] == [12, 20]), "below the packed sprites");
        let [x, y, ..] = rects[0].map(u32::from);
        assert_eq!(atlas[((y * ATLAS_W + x) * 4 + 3) as usize], 255, "its pixels are drawn in");
        let r = parse(base).unwrap().with(&kept);
        assert_eq!(r.enemies.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["big", "kid", "ghoul", "skinny"]);
        assert_eq!(r.enemies[2].hp, 40.0);
        assert!(r.enemies[2].from_card && !r.enemies[0].from_card);
        assert_eq!(r.enemies[2].look.walk[0][0], Art(art::RECTS.len() as u16));
        assert_eq!((r.leader, r.harvester), (2, 0), "a card can lead hordes");
        assert_eq!(r.enemies[r.common()].id, "skinny");
    }

    #[test]
    fn a_card_walks_as_many_frames_as_it_says() {
        let base = include_str!("../../enemies.txt");
        let mut atlas = vec![0u8; (ATLAS_W * ATLAS_H * 4) as usize];
        let [w, h] = [8u32, 8];
        let sheet = vec![255u8; (w * 5 * h * 5 * 4) as usize];
        let card = Card { name: "long.enemy.png".into(), text: "[long]\nwalk 5".into(), frame: [w, h], sheet };
        let (notes, rects, kept) = pack(&[card], base, &mut atlas, &mut start());
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(rects.len(), 5 * 3 + 6);
        let look = &kept[0].enemy.look;
        assert!(look.walk.iter().all(|w| w.len() == 5));
        assert_eq!(look.flash[0], Art((art::RECTS.len() + 15) as u16), "the stills follow the walking frames");
        assert!(parse(&format!("{base}\n[x]\nlook SKINNY\nwalk 9")).is_err(), "a walk is at most {MAX_WALK}");
    }

    #[test]
    fn frame_limits_hold_the_player_and_stills_to_three_and_walks_to_a_range() {
        assert_eq!(frame_limits("PLAYER_FRONT"), Some((3, 3)));
        assert_eq!(frame_limits("KID_HURT"), Some((3, 3)));
        assert_eq!(frame_limits("BIG_SIDE"), Some((1, MAX_WALK)));
        assert_eq!(frame_limits("BARREL"), None);
        assert_eq!(frame_limits("SKINNY_TOP"), None);
    }

    #[test]
    fn a_card_with_a_known_id_replaces_that_kind() {
        let base = include_str!("../../enemies.txt");
        let mut atlas = vec![0u8; (ATLAS_W * ATLAS_H * 4) as usize];
        let (_, _, kept) = pack(&[card("[kid]\nname Imp\nspeed 50", [16, 16])], base, &mut atlas, &mut start());
        let r = parse(base).unwrap().with(&kept);
        assert_eq!(r.enemies.len(), 3);
        assert_eq!((r.enemies[1].name.as_str(), r.enemies[1].speed), ("Imp", 50.0));
    }

    #[test]
    fn bad_cards_are_left_out_with_a_reason() {
        let base = include_str!("../../enemies.txt");
        let mut atlas = vec![0u8; (ATLAS_W * ATLAS_H * 4) as usize];
        let mut short = card("[a]", [16, 16]);
        short.sheet.truncate(100);
        let cards = [card("[a]\nlook SKINNY", [16, 16]), card("[a]\n[b]", [16, 16]), card("[a]\nwings 2", [16, 16]), short, card("[a]", [64, 64])];
        let (notes, rects, kept) = pack(&cards, base, &mut atlas, &mut start());
        assert_eq!(kept.len(), 1, "{notes:?}");
        assert_eq!(rects.len(), 15, "three walking frames a facing, then a flash and a hurt frame each");
        for (note, why) in notes.iter().zip(["no look", "one enemy", "wings", "3 across"]) {
            assert!(note.starts_with("test.png: ") && note.contains(why), "{note}");
        }
        let many: Vec<Card> = (0..40).map(|i| card(&format!("[c{i}]"), [64, 64])).collect();
        let (notes, ..) = pack(&many, base, &mut atlas, &mut start());
        assert!(notes.last().unwrap().contains("no room"), "a full texture turns cards away");
    }

    #[test]
    fn a_card_reads_with_defaults_and_rewrites_one_setting_at_a_time() {
        let text = "# my ghoul\n[ghoul]\nname Ghoul\nhp 40\nheavy\n";
        let f = card_fields(text).unwrap();
        assert_eq!((f.id.as_str(), f.name.as_str(), f.sound.as_str()), ("ghoul", "Ghoul", "kill-ghoul"));
        assert_eq!(f.numbers[0], ("hp", 40.0));
        assert_eq!(f.numbers[1], ("speed", 20.0), "unset numbers read as the game's defaults");
        assert_eq!(f.flags[0], ("heavy", true));
        let text = set_card_field(text, "hp", Some("55"));
        let text = set_card_field(&text, "speed", Some("30"));
        let text = set_card_field(&text, "heavy", None);
        let text = set_card_field(&text, "leader", Some(""));
        assert_eq!(text, "# my ghoul\n[ghoul]\nname Ghoul\nhp          55\nspeed       30\nleader\n");
        let f = card_fields(&text).unwrap();
        assert_eq!((f.numbers[0].1, f.numbers[1].1, f.flags[0].1, f.flags[1].1), (55.0, 30.0, false, true));
        assert!(card_fields("[a]\nlook SKINNY").is_err());
    }

    #[test]
    fn shares_follow_the_spawn_rule() {
        let r = roster();
        let at = |m: f32| r.shares(m);
        let close = |a: &[f32], b: &[f32]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5);
        assert!(close(&at(0.5), &[0.0, 0.0, 1.0]), "only skinnies in the first minute");
        assert!(close(&at(1.5), &[0.0, 0.35, 0.65]));
        assert!(close(&at(5.0), &[0.11, 0.24, 0.65]), "bigs take 0.06 + 0.01 a minute out of the kids' share");
        for m in [0.0, 3.0, 9.9] {
            assert!((at(m).iter().sum::<f32>() - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn a_card_with_chance_one_takes_the_last_kinds_share() {
        let mix = spawn_mix(&["[ghoul]\nname Ghoul"], &[0.0, 5.0]).unwrap();
        let ids: Vec<&str> = mix.kinds.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, ["big", "kid", "ghoul", "skinny"]);
        assert_eq!(mix.shares[0][3], 0.0, "skinny never spawns");
        assert!(mix.replaced.is_empty());
        let mix = spawn_mix(&["[kid]\nchance 0.1"], &[5.0]).unwrap();
        assert_eq!(mix.replaced, ["kid"]);
        assert!((mix.shares[0][1] - 0.0).abs() < 1e-5, "a kid below the bigs' chance is crowded out");
        assert!(spawn_mix(&["[a]\nwings 2"], &[0.0]).is_err());
    }

    #[test]
    fn errors_name_the_line() {
        let err = |text: &str| parse(text).err().expect("an error");
        assert!(err("hp 3").starts_with("1:"));
        assert!(err("[a]\nlook SKINNY\nhp lots").starts_with("3:"));
        assert!(err("[a]\nlook NOPE").contains("NOPE"));
        assert!(err("[a]\nhp 3").contains("no look"));
        assert!(err("[a]\nlook SKINNY\n[a]").contains("twice"));
        assert!(err("[a]\nlook SKINNY\nwings 2").contains("wings"));
    }
}
