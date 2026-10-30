//! The powerups the level-up cards offer, read from `powerups.txt`.

use std::sync::OnceLock;

use crate::Difficulty;
use crate::art::{self, Art};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Weapon,
    Passive,
    Filler,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Ahead,
    Around,
    Behind,
    Map,
}

impl Zone {
    pub fn name(self) -> &'static str {
        match self {
            Zone::Ahead => "AHEAD",
            Zone::Around => "AROUND",
            Zone::Behind => "BEHIND",
            Zone::Map => "MAP",
        }
    }
}

/// What a weapon does when its cooldown runs out; each is written in `weapons.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Does {
    Pistol,
    Shotgun,
    Rifle,
    Lantern,
    Burner,
    Pitchfork,
    Knife,
    Scythe,
    Crows,
    Zapper,
    Dog,
    Tacks,
    Gas,
    Holster,
    Scarecrow,
    Molotov,
    Flare,
    /// A gun shaped by its section's `aim` and `pattern`.
    Gun,
}

/// Where a gun looks for something to shoot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aim {
    /// The nearest zombie in range, whichever way.
    Nearest,
    /// The nearest in the cone the player faces.
    Ahead,
    /// The nearest in the cone behind.
    Behind,
    /// Wherever the player faces, once anything is in range.
    Around,
}

/// How a gun's shots spread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pattern {
    /// Evenly across its spread, centred on the target.
    Fan,
    /// Evenly all the way round.
    Ring,
    /// At random across its spread.
    Spray,
}

impl Does {
    fn from_key(key: &str) -> Option<Does> {
        Some(match key {
            "pistol" => Does::Pistol,
            "shotgun" => Does::Shotgun,
            "rifle" => Does::Rifle,
            "lantern" => Does::Lantern,
            "burner" => Does::Burner,
            "pitchfork" => Does::Pitchfork,
            "knife" => Does::Knife,
            "scythe" => Does::Scythe,
            "crows" => Does::Crows,
            "zapper" => Does::Zapper,
            "dog" => Does::Dog,
            "tacks" => Does::Tacks,
            "gas" => Does::Gas,
            "holster" => Does::Holster,
            "scarecrow" => Does::Scarecrow,
            "molotov" => Does::Molotov,
            "flare" => Does::Flare,
            "gun" => Does::Gun,
            _ => return None,
        })
    }
}

/// When a powerup can first be found on a card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unlock {
    Start,
    /// Zombies killed over every run.
    Kills(u32),
    /// Seconds survived in one night.
    Night(u32),
    /// Level reached in one run.
    Level(u32),
    /// Nights played.
    Nights(u32),
    /// Nights survived to dawn.
    Dawns(u32),
}

/// What the player has done over every run, and the powerups they have found.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub kills: u32,
    pub best: u32,
    pub best_level: u32,
    pub nights: u32,
    pub dawns: u32,
    /// Powerups taken from a find card, one bit each by their place in the book.
    pub found: u64,
    /// Everything unlocked regardless, for trying powerups out.
    pub everything: bool,
}

impl Progress {
    /// This progress with one more run added.
    pub fn after(self, run: Run) -> Progress {
        Progress {
            kills: self.kills.saturating_add(run.kills),
            best: self.best.max(run.secs),
            best_level: self.best_level.max(run.level),
            nights: self.nights + 1,
            dawns: self.dawns + run.won as u32,
            ..self
        }
    }

    /// Whether the cards can offer powerup `i` as an ordinary level.
    pub fn owns(&self, i: usize) -> bool {
        self.everything || book().defs[i].unlock == Unlock::Start || self.found & (1 << i) != 0
    }

    /// Whether powerup `i` has met its condition but not been found yet.
    pub fn findable(&self, i: usize) -> bool {
        !self.owns(i) && book().defs[i].kind != Kind::Filler && book().defs[i].unlock.met(self)
    }
}

/// Found powerups as their ids, for the save file.
pub fn found_ids(found: u64) -> String {
    book().defs.iter().enumerate().filter(|(i, _)| found & (1 << i) != 0).map(|(_, d)| d.id.as_str()).collect::<Vec<_>>().join(",")
}

/// The bits for a list of ids; ones the book no longer has are dropped.
pub fn found_from_ids(ids: &str) -> u64 {
    ids.split(',').filter_map(|id| book().index(id.trim())).fold(0, |m, i| m | 1 << i)
}

/// How one run went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    pub secs: u32,
    pub kills: u32,
    pub level: u32,
    pub won: bool,
}

impl Unlock {
    pub fn met(self, p: &Progress) -> bool {
        p.everything
            || match self {
                Unlock::Start => true,
                Unlock::Kills(n) => p.kills >= n,
                Unlock::Night(s) => p.best >= s,
                Unlock::Level(n) => p.best_level >= n,
                Unlock::Nights(n) => p.nights >= n,
                Unlock::Dawns(n) => p.dawns >= n,
            }
    }

    /// Roughly how many seconds of play are left before it's met, to rank conditions of different kinds.
    pub fn effort(self, p: &Progress) -> f32 {
        let left = |have: u32, need: u32, secs_each: f32| need.saturating_sub(have) as f32 * secs_each;
        match self {
            Unlock::Start => 0.0,
            Unlock::Kills(n) => left(p.kills, n, 1.5),
            Unlock::Night(s) => left(p.best, s, 1.0),
            Unlock::Level(n) => left(p.best_level, n, 40.0),
            Unlock::Nights(n) => left(p.nights, n, 300.0),
            Unlock::Dawns(n) => left(p.dawns, n, 900.0),
        }
    }

    /// What it takes, for the title screen.
    pub fn text(self) -> String {
        match self {
            Unlock::Start => "Ready from the start".into(),
            Unlock::Kills(n) => format!("Kill {n} zombies in all"),
            Unlock::Night(s) => format!("Survive {}:{:02} in one night", s / 60, s % 60),
            Unlock::Level(n) => format!("Reach level {n} in one night"),
            Unlock::Nights(n) => format!("Play {n} nights"),
            Unlock::Dawns(1) => "Survive until dawn".into(),
            Unlock::Dawns(n) => format!("Survive until dawn {n} times"),
        }
    }
}

/// A number a powerup's level sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stat {
    Cooldown,
    Damage,
    Count,
    Range,
    Area,
    Pierce,
    Speed,
    Duration,
    Spread,
    Knockback,
    Slow,
    MaxHp,
    Heal,
    Magnet,
    Armor,
    Regen,
    Crit,
    Reach,
    Luck,
    Greed,
    Revive,
    View,
    Grenades,
    Recharge,
    Cluster,
    Drums,
    Blast,
    Refill,
    Money,
}

/// How a stat reads on a card.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shown {
    Number,
    Seconds,
    Percent,
    Chance,
}

impl Stat {
    const ALL: [Stat; 29] = [
        Stat::Cooldown,
        Stat::Damage,
        Stat::Count,
        Stat::Range,
        Stat::Area,
        Stat::Pierce,
        Stat::Speed,
        Stat::Duration,
        Stat::Spread,
        Stat::Knockback,
        Stat::Slow,
        Stat::MaxHp,
        Stat::Heal,
        Stat::Magnet,
        Stat::Armor,
        Stat::Regen,
        Stat::Crit,
        Stat::Reach,
        Stat::Luck,
        Stat::Greed,
        Stat::Revive,
        Stat::View,
        Stat::Grenades,
        Stat::Recharge,
        Stat::Cluster,
        Stat::Drums,
        Stat::Blast,
        Stat::Refill,
        Stat::Money,
    ];

    fn key(self) -> &'static str {
        match self {
            Stat::Cooldown => "cooldown",
            Stat::Damage => "damage",
            Stat::Count => "count",
            Stat::Range => "range",
            Stat::Area => "area",
            Stat::Pierce => "pierce",
            Stat::Speed => "speed",
            Stat::Duration => "duration",
            Stat::Spread => "spread",
            Stat::Knockback => "knockback",
            Stat::Slow => "slow",
            Stat::MaxHp => "max_hp",
            Stat::Heal => "heal",
            Stat::Magnet => "magnet",
            Stat::Armor => "armor",
            Stat::Regen => "regen",
            Stat::Crit => "crit",
            Stat::Reach => "reach",
            Stat::Luck => "luck",
            Stat::Greed => "greed",
            Stat::Revive => "revive",
            Stat::View => "view",
            Stat::Grenades => "grenades",
            Stat::Recharge => "recharge",
            Stat::Cluster => "cluster",
            Stat::Drums => "drums",
            Stat::Blast => "blast",
            Stat::Refill => "refill",
            Stat::Money => "money",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Stat::Cooldown => "Every",
            Stat::Damage => "Damage",
            Stat::Count => "Count",
            Stat::Range => "Range",
            Stat::Area => "Size",
            Stat::Pierce => "Pierce",
            Stat::Speed => "Speed",
            Stat::Duration => "Lasts",
            Stat::Spread => "Spread",
            Stat::Knockback => "Knockback",
            Stat::Slow => "Slow",
            Stat::MaxHp => "Max health",
            Stat::Heal => "Heal",
            Stat::Magnet => "Pickup range",
            Stat::Armor => "Armor",
            Stat::Regen => "Heal/sec",
            Stat::Crit => "Crit chance",
            Stat::Reach => "Reach",
            Stat::Luck => "Rare chance",
            Stat::Greed => "Money",
            Stat::Revive => "Revives",
            Stat::View => "View",
            Stat::Grenades => "Grenades",
            Stat::Recharge => "Recharge",
            Stat::Cluster => "Bomblets",
            Stat::Drums => "Barrel every",
            Stat::Blast => "Blast",
            Stat::Refill => "Refill",
            Stat::Money => "Money",
        }
    }

    /// Passive stats a percentage scales, which multiply together rather than add.
    pub fn scales(self) -> bool {
        matches!(
            self,
            Stat::Damage | Stat::Cooldown | Stat::Speed | Stat::Magnet | Stat::Area | Stat::Reach | Stat::Greed | Stat::Knockback | Stat::View | Stat::Recharge | Stat::Blast
        )
    }

    fn for_weapons(self) -> bool {
        matches!(
            self,
            Stat::Cooldown | Stat::Damage | Stat::Count | Stat::Range | Stat::Area | Stat::Pierce | Stat::Speed | Stat::Duration | Stat::Spread | Stat::Knockback | Stat::Slow
        )
    }

    fn for_passives(self) -> bool {
        !matches!(self, Stat::Count | Stat::Range | Stat::Duration | Stat::Spread | Stat::Slow | Stat::Refill | Stat::Money)
    }

    /// A weapon's value for a stat the file leaves out.
    fn weapon_default(self) -> f32 {
        match self {
            Stat::Cooldown | Stat::Count | Stat::Duration => 1.0,
            Stat::Range => 100.0,
            Stat::Area => 20.0,
            Stat::Speed => 200.0,
            Stat::Spread => 30.0,
            Stat::Knockback => 30.0,
            _ => 0.0,
        }
    }

    fn shown(self, kind: Kind) -> Shown {
        match self {
            _ if kind == Kind::Passive && self.scales() => Shown::Percent,
            Stat::Cooldown | Stat::Duration | Stat::Drums => Shown::Seconds,
            Stat::Crit | Stat::Luck | Stat::Slow => Shown::Chance,
            _ => Shown::Number,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Def {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub does: Option<Does>,
    pub zone: Option<Zone>,
    pub icon: Art,
    pub unlock: Unlock,
    pub blurb: String,
    pub max: u32,
    /// The chance, once its unlock is met, that a level-up offers it as a find.
    pub find: f32,
    /// Minutes into a night before it can turn up as a find.
    pub after: f32,
    /// Only found in the chest a horde's leader drops.
    pub chest: bool,
    /// For a weapon, the `effects.txt` effect it plays in place of its behaviour's own.
    pub effect: Option<String>,
    /// For a weapon that shoots, the effect its bullets fly as.
    pub bullet: Option<String>,
    /// For a weapon, the sound it makes in place of its behaviour's own.
    pub sound: Option<crate::Sfx>,
    /// For a gun, where it looks and how its shots spread.
    pub aim: Aim,
    pub pattern: Pattern,
    stats: Vec<(Stat, Vec<f32>)>,
}

/// The sounds a weapon can make, by the name `sound` gives them.
pub const WEAPON_SOUNDS: [(&str, crate::Sfx); 6] =
    [("shot", crate::Sfx::Shot), ("blast", crate::Sfx::Blast), ("slash", crate::Sfx::Slash), ("zap", crate::Sfx::Zap), ("bark", crate::Sfx::Bark), ("throw", crate::Sfx::Throw)];

impl Def {
    /// The value of `stat` at `level`, counting from 1.
    pub fn at(&self, stat: Stat, level: u32) -> f32 {
        match self.stats.iter().find(|(s, _)| *s == stat) {
            Some((_, v)) => v[(level.max(1) as usize - 1).min(v.len() - 1)],
            None if self.kind == Kind::Weapon => stat.weapon_default(),
            None if stat.scales() => 1.0,
            None => 0.0,
        }
    }

    /// The card's line for going from level `from` to `to`: the blurb when new, else what changes.
    pub fn change(&self, from: u32, to: u32) -> String {
        if from == 0 || self.kind == Kind::Filler {
            return self.blurb.clone();
        }
        let show = |s: Stat, v: f32| match s.shown(self.kind) {
            Shown::Percent => format!("{:+.0}%", (v - 1.0) * 100.0),
            Shown::Seconds => format!("{}s", trim(v)),
            Shown::Chance => format!("{:.0}%", v * 100.0),
            Shown::Number => trim(v),
        };
        let parts: Vec<String> = self
            .stats
            .iter()
            .filter(|(s, _)| *s != Stat::Heal)
            .filter_map(|&(s, _)| {
                let (a, b) = (self.at(s, from), self.at(s, to));
                (a != b).then(|| format!("{} {} > {}", s.label(), show(s, a), show(s, b)))
            })
            .take(3)
            .collect();
        if parts.is_empty() { self.blurb.clone() } else { parts.join("   ") }
    }
}

fn trim(v: f32) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[derive(Debug)]
pub struct Book {
    pub weapon_slots: usize,
    pub passive_slots: usize,
    /// What a run carries from the start at each difficulty, by place in the book.
    pub start: [Vec<usize>; 3],
    /// Grenades every run starts with, before a grenade belt.
    pub grenades: u32,
    /// The most powerups one run can find.
    pub finds: usize,
    /// Money for the first level-up.
    pub first_level: u32,
    pub defs: Vec<Def>,
}

impl Book {
    pub fn index(&self, id: &str) -> Option<usize> {
        self.defs.iter().position(|d| d.id == id)
    }
}

/// The powerups in the shipped file.
pub fn book() -> &'static Book {
    static BOOK: OnceLock<Book> = OnceLock::new();
    BOOK.get_or_init(|| parse(crate::data::powerups()).unwrap_or_else(|e| panic!("powerups.txt:{e}")))
}

fn sprite(name: &str) -> Option<Art> {
    art::ALL.iter().find(|(n, _)| *n == name).map(|(_, frames)| frames[0])
}

const DEFAULT_FIND: f32 = 0.25;

/// Reads the powerup file; an error names the line.
pub fn parse(text: &str) -> Result<Book, String> {
    let mut book = Book { weapon_slots: 6, passive_slots: 6, start: Default::default(), grenades: 3, finds: 2, first_level: 5, defs: Vec::new() };
    let mut start: [Option<(usize, Vec<String>)>; 3] = Default::default();
    let mut icons: Vec<Option<String>> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(id) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            if book.index(id).is_some() {
                return Err(format!("{n}: [{id}] appears twice"));
            }
            book.defs.push(Def {
                id: id.to_string(),
                name: id.to_string(),
                kind: Kind::Passive,
                does: None,
                zone: None,
                icon: art::WHITE,
                unlock: Unlock::Start,
                blurb: String::new(),
                max: 1,
                find: DEFAULT_FIND,
                after: 0.0,
                chest: false,
                effect: None,
                bullet: None,
                sound: None,
                aim: Aim::Nearest,
                pattern: Pattern::Fan,
                stats: Vec::new(),
            });
            icons.push(None);
            continue;
        }
        let (key, rest) = line.split_once(char::is_whitespace).map(|(k, r)| (k, r.trim())).unwrap_or((line, ""));
        let number = |w: &str| w.parse::<u32>().map_err(|_| format!("{n}: {w:?} isn't a whole number"));
        let Some(def) = book.defs.last_mut() else {
            let words: Vec<&str> = rest.split_whitespace().collect();
            match (key, &words[..]) {
                ("slots", ["weapons", v]) => book.weapon_slots = number(v)? as usize,
                ("slots", ["passives", v]) => book.passive_slots = number(v)? as usize,
                ("start", [d, ids @ ..]) => {
                    let d = Difficulty::from_key(d).ok_or_else(|| format!("{n}: start names easy, normal or hard first, not {d:?}"))?;
                    start[d as usize] = Some((n, ids.iter().map(|id| id.to_string()).collect()));
                }
                ("grenades", [v]) => book.grenades = number(v)?,
                ("finds", [v]) => book.finds = number(v)? as usize,
                ("first_level", [v]) => book.first_level = number(v)?.max(1),
                _ => return Err(format!("{n}: expected slots, start, grenades, finds, first_level or a [powerup]")),
            }
            continue;
        };
        match key {
            "name" => def.name = rest.to_string(),
            "blurb" => def.blurb = rest.to_string(),
            "icon" => *icons.last_mut().unwrap() = Some(rest.to_string()),
            "max" => def.max = number(rest)?.max(1),
            "find" => def.find = rest.parse::<f32>().ok().filter(|f| (0.0..=1.0).contains(f)).ok_or_else(|| format!("{n}: find is a chance from 0 to 1, not {rest:?}"))?,
            "after" => def.after = rest.parse::<f32>().ok().filter(|v| v.is_finite()).ok_or_else(|| format!("{n}: after is minutes into the night, not {rest:?}"))?,
            "chest" if rest.is_empty() => def.chest = true,
            "kind" => {
                def.kind = match rest {
                    "weapon" => Kind::Weapon,
                    "passive" => Kind::Passive,
                    "filler" => Kind::Filler,
                    _ => return Err(format!("{n}: kind is weapon, passive or filler, not {rest:?}")),
                }
            }
            "zone" => {
                def.zone = Some(match rest {
                    "ahead" => Zone::Ahead,
                    "around" => Zone::Around,
                    "behind" => Zone::Behind,
                    "map" => Zone::Map,
                    _ => return Err(format!("{n}: zone is ahead, around, behind or map, not {rest:?}")),
                })
            }
            "effect" | "bullet" if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') => {
                *if key == "effect" { &mut def.effect } else { &mut def.bullet } = Some(rest.to_string());
            }
            "effect" | "bullet" => return Err(format!("{n}: {key} names an effects.txt section, such as slash")),
            "sound" => {
                let names: Vec<&str> = WEAPON_SOUNDS.iter().map(|(k, _)| *k).collect();
                def.sound = Some(WEAPON_SOUNDS.iter().find(|(k, _)| *k == rest).map(|(_, s)| *s).ok_or_else(|| format!("{n}: sound is one of {}", names.join(", ")))?);
            }
            "aim" => {
                def.aim = match rest {
                    "nearest" => Aim::Nearest,
                    "ahead" => Aim::Ahead,
                    "behind" => Aim::Behind,
                    "around" => Aim::Around,
                    _ => return Err(format!("{n}: aim is nearest, ahead, behind or around, not {rest:?}")),
                }
            }
            "pattern" => {
                def.pattern = match rest {
                    "fan" => Pattern::Fan,
                    "ring" => Pattern::Ring,
                    "spray" => Pattern::Spray,
                    _ => return Err(format!("{n}: pattern is fan, ring or spray, not {rest:?}")),
                }
            }
            "does" => def.does = Some(Does::from_key(rest).ok_or_else(|| format!("{n}: no weapon behaviour called {rest:?}"))?),
            "unlock" => {
                let words: Vec<&str> = rest.split_whitespace().collect();
                def.unlock = match words[..] {
                    ["start"] => Unlock::Start,
                    ["kills", v] => Unlock::Kills(number(v)?),
                    ["night", v] => Unlock::Night(number(v)?),
                    ["level", v] => Unlock::Level(number(v)?),
                    ["nights", v] => Unlock::Nights(number(v)?),
                    ["dawns", v] => Unlock::Dawns(number(v)?),
                    _ => return Err(format!("{n}: unlock is start, kills N, night SECONDS, level N, nights N or dawns N")),
                }
            }
            _ => {
                let stat = Stat::ALL.iter().copied().find(|s| s.key() == key).ok_or_else(|| format!("{n}: no stat called {key:?}"))?;
                if def.stats.iter().any(|(s, _)| *s == stat) {
                    return Err(format!("{n}: {key} appears twice in [{}]", def.id));
                }
                let values = rest
                    .split_whitespace()
                    .map(|w| match w.strip_suffix('%') {
                        Some(p) => p.parse::<f32>().map(|p| 1.0 + p / 100.0),
                        None => w.parse::<f32>(),
                    })
                    .collect::<Result<Vec<f32>, _>>()
                    .map_err(|_| format!("{n}: {key} needs numbers, one per level"))?;
                if values.is_empty() {
                    return Err(format!("{n}: {key} needs at least one value"));
                }
                if values.iter().any(|v| !v.is_finite()) {
                    return Err(format!("{n}: {key} needs numbers, one per level"));
                }
                def.stats.push((stat, values));
            }
        }
    }
    for (kit, start) in book.start.iter_mut().zip(start) {
        match start {
            Some((n, ids)) => {
                for id in ids {
                    let i = book.defs.iter().position(|d| d.id == id).ok_or_else(|| format!("{n}: start names {id:?}, which has no [section]"))?;
                    if book.defs[i].kind != Kind::Weapon || book.defs[i].unlock != Unlock::Start {
                        return Err(format!("{n}: start takes weapons unlocked from the start, and {id} isn't one"));
                    }
                    kit.push(i);
                }
            }
            None => kit.extend(book.defs.iter().position(|d| d.kind == Kind::Weapon)),
        }
    }
    if book.defs.len() > 64 {
        return Err(" more than 64 powerups; the save keeps found ones as 64 bits".into());
    }
    for (def, icon) in book.defs.iter_mut().zip(icons) {
        let icon = icon.unwrap_or_else(|| format!("ICON_{}", def.id.to_ascii_uppercase()));
        def.icon = sprite(&icon).ok_or_else(|| format!(" [{}] has no sprite called {icon}; add it to art.txt", def.id))?;
        let wrong = def.stats.iter().find(|(s, _)| match def.kind {
            Kind::Weapon => !s.for_weapons(),
            Kind::Passive => !s.for_passives(),
            Kind::Filler => !matches!(s, Stat::Heal | Stat::Refill | Stat::Money),
        });
        if let Some((s, _)) = wrong {
            return Err(format!(" [{}] is a {:?} and can't have {}", def.id, def.kind, s.key()).to_lowercase());
        }
        if def.kind == Kind::Weapon {
            def.does = Some(def.does.or_else(|| Does::from_key(&def.id)).ok_or_else(|| format!(" [{}] needs `does` to say which weapon it fires", def.id))?);
        }
    }
    if !book.defs.iter().any(|d| d.kind == Kind::Filler) {
        return Err(" no filler; a level-up offers one once there's nothing else to take".into());
    }
    Ok(book)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_file_reads_and_has_enough_to_choose_from() {
        let b = book();
        let count = |k: Kind| b.defs.iter().filter(|d| d.kind == k).count();
        assert!(count(Kind::Weapon) + count(Kind::Passive) >= 30);
        assert!(count(Kind::Filler) >= 3, "fillers fill all three cards when nothing else is left");
        let start = b.defs.iter().filter(|d| d.kind == Kind::Weapon && d.unlock == Unlock::Start).count();
        assert!((2..=4).contains(&start), "a few weapons from the start, the rest unlocked over time");
        let ids = |d: Difficulty| b.start[d as usize].iter().map(|&i| b.defs[i].id.as_str()).collect::<Vec<_>>();
        assert_eq!(ids(Difficulty::Easy), ["pistol", "knife"]);
        assert_eq!(ids(Difficulty::Normal), ["knife"]);
        assert!(ids(Difficulty::Hard).is_empty(), "hard starts with only a grenade");
        assert_eq!(b.grenades, 1);
        assert!(b.defs.iter().filter(|d| d.kind == Kind::Weapon).all(|d| d.zone.is_some()));
    }

    #[test]
    fn levels_read_their_value_and_short_lists_repeat_the_last() {
        let b = parse("[gun]\nkind weapon\ndoes pistol\nicon WHITE\ndamage 1 2 3\n[pp]\nicon WHITE\ndamage +10% +25%\n[cash]\nkind filler\nicon WHITE\n").unwrap();
        let (gun, pp) = (&b.defs[0], &b.defs[1]);
        assert_eq!([1, 2, 3, 9].map(|l| gun.at(Stat::Damage, l)), [1.0, 2.0, 3.0, 3.0]);
        assert_eq!(gun.at(Stat::Range, 1), 100.0, "left out, a weapon stat has its default");
        assert!((pp.at(Stat::Damage, 2) - 1.25).abs() < 1e-6);
        assert_eq!(pp.at(Stat::Greed, 1), 1.0, "a percentage left out changes nothing");
        assert_eq!(pp.at(Stat::Armor, 1), 0.0);
        assert_eq!(pp.change(1, 2), "Damage +10% > +25%");
        assert_eq!(gun.change(0, 1), "", "a new card shows its blurb");
    }

    #[test]
    fn a_weapon_can_play_its_own_effect_bullet_and_sound() {
        let b = parse("[cleaver]\nkind weapon\ndoes knife\nicon WHITE\neffect red_slash\nbullet bullet_3d\nsound blast\n[cash]\nkind filler\nicon WHITE\n").unwrap();
        let d = &b.defs[0];
        assert_eq!((d.effect.as_deref(), d.bullet.as_deref(), d.sound), (Some("red_slash"), Some("bullet_3d"), Some(crate::Sfx::Blast)));
        assert!(parse("[a]\nicon WHITE\nsound moo").unwrap_err().starts_with("3: sound is one of shot"));
        assert!(parse("[a]\nicon WHITE\neffect Red").unwrap_err().starts_with("3: effect names"));
        let g = parse("[nova]\nkind weapon\ndoes gun\naim around\npattern ring\nicon WHITE\n[cash]\nkind filler\nicon WHITE\n").unwrap();
        assert_eq!((g.defs[0].does, g.defs[0].aim, g.defs[0].pattern), (Some(Does::Gun), Aim::Around, Pattern::Ring));
        assert!(parse("[a]\nicon WHITE\naim up").unwrap_err().starts_with("3: aim is"));
    }

    #[test]
    fn finds_read_their_chance_minute_and_chest() {
        let b = parse("[a]\nicon WHITE\n[b]\nicon WHITE\nfind 0.1\nafter 3\nchest\n[cash]\nkind filler\nicon WHITE\n").unwrap();
        assert_eq!((b.defs[0].find, b.defs[0].after, b.defs[0].chest), (DEFAULT_FIND, 0.0, false));
        assert_eq!((b.defs[1].find, b.defs[1].after, b.defs[1].chest), (0.1, 3.0, true));
        assert!(parse("[a]\nicon WHITE\nfind 2").unwrap_err().starts_with("3:"));
        assert!(parse("[a]\nicon WHITE\nchest yes").unwrap_err().starts_with("3:"));
        assert_eq!(found_from_ids(&found_ids(0b101)), 0b101);
        assert_eq!(found_from_ids("pistol, nope"), 1, "an id the book lost is dropped");
    }

    #[test]
    fn mistakes_name_their_line() {
        for (text, line) in [
            ("[a]\nicon WHITE\ndamgae 3", "3:"),
            ("[a]\nkind gun", "2:"),
            ("slots weapons x", "1:"),
            ("[a]\nicon WHITE\n[a]", "3:"),
            ("[a]\nicon WHITE\nunlock kills", "3:"),
            ("start nope\n[a]\nicon WHITE", "1:"),
            ("start a\n[a]\nicon WHITE", "1:"),
        ] {
            let e = parse(text).err().unwrap();
            assert!(e.starts_with(line), "{text:?}: {e}");
        }
        assert!(parse("[laser]\nkind weapon\nicon WHITE").unwrap_err().contains("does"));
        assert!(parse("[a]\nicon NOPE").unwrap_err().contains("NOPE"));
        assert!(parse("[a]\nkind weapon\ndoes knife\nicon WHITE\narmor 2").unwrap_err().contains("armor"));
        assert!(parse("[a]\nicon WHITE").unwrap_err().contains("no filler"));
        assert!(parse("# nothing").unwrap_err().contains("no filler"));
        assert!(parse("[a]\nicon WHITE\ndamage +10% inf").unwrap_err().starts_with("3: damage"));
        assert!(parse("[a]\nicon WHITE\nafter NaN").unwrap_err().starts_with("3: after"));
    }

    #[test]
    fn unlocks_follow_progress_over_runs() {
        let p = Progress::default().after(Run { secs: 130, kills: 120, level: 6, won: false });
        assert!(Unlock::Kills(100).met(&p) && !Unlock::Kills(250).met(&p));
        assert!(Unlock::Night(120).met(&p) && Unlock::Level(5).met(&p) && Unlock::Nights(1).met(&p));
        assert!(!Unlock::Dawns(1).met(&p));
        let p = p.after(Run { secs: 600, kills: 10, level: 3, won: true });
        assert_eq!((p.kills, p.best, p.best_level, p.nights, p.dawns), (130, 600, 6, 2, 1));
        assert!(Unlock::Dawns(1).met(&Progress { everything: true, ..Progress::default() }));
        let fresh = Progress::default();
        assert!(Unlock::Kills(100).effort(&fresh) < Unlock::Kills(500).effort(&fresh));
        assert!(Unlock::Night(120).effort(&fresh) < Unlock::Dawns(1).effort(&fresh));
    }
}
