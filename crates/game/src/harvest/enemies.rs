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
const FRAMES: usize = 15;
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
            let e = card.enemy.clone();
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
    let walk = |facing: &str| frames::<3>(&format!("{prefix}_{facing}"));
    Ok(Look { walk: [walk("FRONT")?, walk("SIDE")?, walk("BACK")?], flash: frames(&format!("{prefix}_FLASH"))?, hurt: frames(&format!("{prefix}_HURT"))? })
}

/// One `[id]` section: the kind, the line and name of its `look`, and whether it leads hordes or dresses the Harvester.
#[derive(Clone)]
struct Entry {
    enemy: Enemy,
    look: Option<(usize, String)>,
    leader: bool,
    harvester: bool,
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
            let blank = Look { walk: [[art::WHITE; 3]; 3], flash: [art::WHITE; 3], hurt: [art::WHITE; 3] };
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
            };
            entries.push(Entry { enemy, look: None, leader: false, harvester: false });
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

/// Where a card's frames go in the texture: shelves below the packed sprites, carrying on from `cursor`.
fn place(cursor: &mut [u16; 3], [w, h]: [u16; 2]) -> Option<[u16; 2]> {
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

/// Draws each card's frames into the free rows of `atlas` and keeps its kind for the roster; `base` is the
/// enemy file the cards join. Returns a line for each card left out, and why.
pub(crate) fn install_cards(cards: &[Card], base: &str, atlas: &mut [u8]) -> Vec<String> {
    let (notes, rects, kept) = pack(cards, base, atlas);
    art::set_added(rects);
    CARDS.set(kept).ok();
    notes
}

/// The cards' kinds with their sprites drawn into `atlas`, the rects drawn, and a line for each card left out.
fn pack(cards: &[Card], base: &str, atlas: &mut [u8]) -> (Vec<String>, Vec<[u16; 4]>, Vec<Entry>) {
    let mut ids: Vec<String> = entries(base).map(|es| es.into_iter().map(|e| e.enemy.id).collect()).unwrap_or_default();
    let (mut notes, mut rects, mut kept) = (Vec::new(), Vec::new(), Vec::new());
    let mut cursor = [PAD, art::free_top() + PAD, 0];
    for card in cards {
        let mut entry = match card_entry(&card.text) {
            Ok(e) => e,
            Err(e) => {
                notes.push(format!("{}: {e}", card.name));
                continue;
            }
        };
        let [fw, fh] = card.frame;
        if !(1..=MAX_FRAME).contains(&fw) || !(1..=MAX_FRAME).contains(&fh) || card.sheet.len() != (fw * fh * FRAMES as u32 * 4) as usize {
            notes.push(format!("{}: frames are 1 to {MAX_FRAME} pixels, three across and five down", card.name));
            continue;
        }
        let new = !ids.contains(&entry.enemy.id);
        if new && ids.len() >= MAX_KINDS {
            notes.push(format!("{}: there can be only {MAX_KINDS} kinds", card.name));
            continue;
        }
        let mut try_cursor = cursor;
        let spots: Option<Vec<[u16; 2]>> = (0..FRAMES).map(|_| place(&mut try_cursor, [fw as u16, fh as u16])).collect();
        let Some(spots) = spots else {
            notes.push(format!("{}: no room left in the sprite texture", card.name));
            continue;
        };
        cursor = try_cursor;
        let first = art::RECTS.len() + rects.len();
        for (k, [x, y]) in spots.into_iter().enumerate() {
            let (col, row) = ((k % 3) as u32, (k / 3) as u32);
            for py in 0..fh {
                for px in 0..fw {
                    let s = (((row * fh + py) * fw * 3 + col * fw + px) * 4) as usize;
                    let d = (((y as u32 + py) * ATLAS_W + x as u32 + px) * 4) as usize;
                    let p = &card.sheet[s..s + 4];
                    // Nearly clear pixels would otherwise show as faint squares.
                    let solid = if p[3] >= 128 { [p[0], p[1], p[2], 255] } else { [0; 4] };
                    atlas[d..d + 4].copy_from_slice(&solid);
                }
            }
            rects.push([x, y, fw as u16, fh as u16]);
        }
        let at = |k: usize| Art((first + k) as u16);
        entry.enemy.look = Look { walk: [[at(0), at(1), at(2)], [at(3), at(4), at(5)], [at(6), at(7), at(8)]], flash: [at(9), at(10), at(11)], hurt: [at(12), at(13), at(14)] };
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
        assert_eq!(big.look.walk[0], art::BIG_FRONT);
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
        let (notes, rects, kept) = pack(&[card("[ghoul]\nname Ghoul\nhp 40\nleader", [12, 20])], base, &mut atlas);
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(rects.len(), FRAMES);
        assert!(rects.iter().all(|r| r[1] > art::free_top() && r[2..] == [12, 20]), "below the packed sprites");
        let [x, y, ..] = rects[0].map(u32::from);
        assert_eq!(atlas[((y * ATLAS_W + x) * 4 + 3) as usize], 255, "its pixels are drawn in");
        let r = parse(base).unwrap().with(&kept);
        assert_eq!(r.enemies.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["big", "kid", "ghoul", "skinny"]);
        assert_eq!(r.enemies[2].hp, 40.0);
        assert_eq!(r.enemies[2].look.walk[0][0], Art(art::RECTS.len() as u16));
        assert_eq!((r.leader, r.harvester), (2, 0), "a card can lead hordes");
        assert_eq!(r.enemies[r.common()].id, "skinny");
    }

    #[test]
    fn a_card_with_a_known_id_replaces_that_kind() {
        let base = include_str!("../../enemies.txt");
        let mut atlas = vec![0u8; (ATLAS_W * ATLAS_H * 4) as usize];
        let (_, _, kept) = pack(&[card("[kid]\nname Imp\nspeed 50", [16, 16])], base, &mut atlas);
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
        let (notes, rects, kept) = pack(&cards, base, &mut atlas);
        assert_eq!(kept.len(), 1, "{notes:?}");
        assert_eq!(rects.len(), FRAMES);
        for (note, why) in notes.iter().zip(["no look", "one enemy", "wings", "three across"]) {
            assert!(note.starts_with("test.png: ") && note.contains(why), "{note}");
        }
        let many: Vec<Card> = (0..40).map(|i| card(&format!("[c{i}]"), [64, 64])).collect();
        let (notes, ..) = pack(&many, base, &mut atlas);
        assert!(notes.last().unwrap().contains("no room"), "a full texture turns cards away");
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
