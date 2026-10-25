//! The powerups the level-up cards offer, read from `powerups.txt`.

use std::sync::OnceLock;

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
            _ => return None,
        })
    }
}

/// When a powerup joins the cards.
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

/// What the player has done over every run, which unlocks powerups.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub kills: u32,
    pub best: u32,
    pub best_level: u32,
    pub nights: u32,
    pub dawns: u32,
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
            everything: self.everything,
        }
    }
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

    /// How close progress is to meeting it, from 0 to 1.
    pub fn nearness(self, p: &Progress) -> f32 {
        let k = |have: u32, need: u32| have as f32 / need.max(1) as f32;
        match self {
            Unlock::Start => 1.0,
            Unlock::Kills(n) => k(p.kills, n),
            Unlock::Night(s) => k(p.best, s),
            Unlock::Level(n) => k(p.best_level, n),
            Unlock::Nights(n) => k(p.nights, n),
            Unlock::Dawns(n) => k(p.dawns, n),
        }
        .min(1.0)
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
    stats: Vec<(Stat, Vec<f32>)>,
}

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
    BOOK.get_or_init(|| parse(include_str!("../../powerups.txt")).unwrap_or_else(|e| panic!("powerups.txt:{e}")))
}

fn sprite(name: &str) -> Option<Art> {
    art::ALL.iter().find(|(n, _)| *n == name).map(|(_, frames)| frames[0])
}

/// Reads the powerup file; an error names the line.
pub fn parse(text: &str) -> Result<Book, String> {
    let mut book = Book { weapon_slots: 6, passive_slots: 6, defs: Vec::new() };
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
                _ => return Err(format!("{n}: expected `slots weapons N`, `slots passives N` or a [powerup]")),
            }
            continue;
        };
        match key {
            "name" => def.name = rest.to_string(),
            "blurb" => def.blurb = rest.to_string(),
            "icon" => *icons.last_mut().unwrap() = Some(rest.to_string()),
            "max" => def.max = number(rest)?.max(1),
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
                def.stats.push((stat, values));
            }
        }
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
        assert_eq!(b.defs[0].id, "pistol", "the first weapon is the one every run starts with");
        assert!(b.defs.iter().filter(|d| d.kind == Kind::Weapon).all(|d| d.zone.is_some()));
    }

    #[test]
    fn levels_read_their_value_and_short_lists_repeat_the_last() {
        let b = parse("[gun]\nkind weapon\ndoes pistol\nicon WHITE\ndamage 1 2 3\n[pp]\nicon WHITE\ndamage +10% +25%\n").unwrap();
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
    fn mistakes_name_their_line() {
        for (text, line) in [
            ("[a]\nicon WHITE\ndamgae 3", "3:"),
            ("[a]\nkind gun", "2:"),
            ("slots weapons x", "1:"),
            ("[a]\nicon WHITE\n[a]", "3:"),
            ("[a]\nicon WHITE\nunlock kills", "3:"),
        ] {
            let e = parse(text).err().unwrap();
            assert!(e.starts_with(line), "{text:?}: {e}");
        }
        assert!(parse("[laser]\nkind weapon\nicon WHITE").unwrap_err().contains("does"));
        assert!(parse("[a]\nicon NOPE").unwrap_err().contains("NOPE"));
        assert!(parse("[a]\nkind weapon\ndoes knife\nicon WHITE\narmor 2").unwrap_err().contains("armor"));
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
    }
}
