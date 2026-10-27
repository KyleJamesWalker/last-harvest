//! The kinds of zombie, read from `enemies.txt`.

use std::sync::OnceLock;

use super::Look;
use crate::art::{self, Art};

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

impl Roster {
    #[cfg(test)]
    pub fn index(&self, id: &str) -> Option<usize> {
        self.enemies.iter().position(|e| e.id == id)
    }

    /// The kind that arrives `minutes` into the night for a roll from 0 to 1: the first whose chance is above it.
    pub fn pick(&self, minutes: f32, roll: f32) -> usize {
        self.enemies.iter().position(|e| e.arrives(minutes, roll)).unwrap_or(self.common())
    }

    /// The last kind, which the list ends on for every roll the others miss.
    pub fn common(&self) -> usize {
        self.enemies.len() - 1
    }
}

/// The zombies in the shipped file.
pub fn roster() -> &'static Roster {
    static ROSTER: OnceLock<Roster> = OnceLock::new();
    ROSTER.get_or_init(|| parse(crate::data::enemies()).unwrap_or_else(|e| panic!("enemies.txt:{e}")))
}

fn frames<const N: usize>(name: &str) -> Result<[Art; N], String> {
    let (_, frames) = art::ALL.iter().find(|(n, _)| *n == name).ok_or_else(|| format!(" no sprite called {name}"))?;
    <[Art; N]>::try_from(*frames).map_err(|_| format!(" {name} has {} frames, not {N}", frames.len()))
}

fn look(prefix: &str) -> Result<Look, String> {
    let walk = |facing: &str| frames::<3>(&format!("{prefix}_{facing}"));
    Ok(Look { walk: [walk("FRONT")?, walk("SIDE")?, walk("BACK")?], flash: frames(&format!("{prefix}_FLASH"))?, hurt: frames(&format!("{prefix}_HURT"))? })
}

/// Reads the enemy file; an error names the line.
pub fn parse(text: &str) -> Result<Roster, String> {
    let mut enemies: Vec<Enemy> = Vec::new();
    let mut looks: Vec<Option<(usize, String)>> = Vec::new();
    let (mut leader, mut harvester) = (None, None);
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(id) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            if enemies.iter().any(|e| e.id == id) {
                return Err(format!("{n}: [{id}] appears twice"));
            }
            let blank = Look { walk: [[art::WHITE; 3]; 3], flash: [art::WHITE; 3], hurt: [art::WHITE; 3] };
            enemies.push(Enemy {
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
            });
            looks.push(None);
            continue;
        }
        let Some(e) = enemies.last_mut() else { return Err(format!("{n}: expected an [enemy] first")) };
        let (key, rest) = line.split_once(char::is_whitespace).map(|(k, r)| (k, r.trim())).unwrap_or((line, ""));
        let number = || rest.parse::<f32>().ok().filter(|v| *v >= 0.0).ok_or_else(|| format!("{n}: {key} is a number, not {rest:?}"));
        match key {
            "name" => e.name = rest.to_string(),
            "look" => *looks.last_mut().unwrap() = Some((n, rest.to_string())),
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
            "leader" if rest.is_empty() => leader = Some(enemies.len() - 1),
            "harvester" if rest.is_empty() => harvester = Some(enemies.len() - 1),
            _ => return Err(format!("{n}: [{}] has no setting {key:?}", e.id)),
        }
    }
    if enemies.is_empty() {
        return Err(" no enemies".into());
    }
    for (e, at) in enemies.iter_mut().zip(looks) {
        let (n, prefix) = at.ok_or_else(|| format!(" [{}] has no look", e.id))?;
        e.look = look(&prefix).map_err(|err| format!("{n}:{err}"))?;
    }
    Ok(Roster { leader: leader.unwrap_or(0), harvester: harvester.unwrap_or(0), enemies })
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
        let id = |minutes: f32, roll: f32| r.enemies[r.pick(minutes, roll)].id.as_str();
        assert_eq!(id(0.5, 0.0), "skinny", "nothing but skinnies in the first minute");
        assert_eq!(id(1.5, 0.0), "kid");
        assert_eq!(id(1.5, 0.35), "skinny");
        assert_eq!(id(5.0, 0.10), "big", "bigs take rolls under 0.06 plus 0.01 a minute after two minutes");
        assert_eq!(id(5.0, 0.11), "kid");
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
