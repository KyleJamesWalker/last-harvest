//! What each effect, swing and animal plays, read from `effects.txt`.

use std::sync::OnceLock;

use crate::art::{self, Art};

/// One effect: its frames, and how fast they play, or for a swing how long it lasts and how far out it sweeps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Effect {
    pub frames: &'static [Art],
    pub fps: f32,
    pub life: f32,
    pub reach: f32,
}

impl Effect {
    /// The frame showing `t` seconds in, looping.
    pub fn looping(&self, t: f32) -> Art {
        self.frames[(t * self.fps).max(0.0) as usize % self.frames.len()]
    }

    /// The frame showing `t` seconds in, holding the last.
    pub fn once(&self, t: f32) -> Art {
        self.frames[((t * self.fps).max(0.0) as usize).min(self.frames.len() - 1)]
    }
}

macro_rules! effects {
    ($($field:ident,)*) => {
        #[derive(Clone, Copy, Debug)]
        pub struct Effects {
            $(pub $field: Effect,)*
        }

        /// Every effect `effects.txt` names, in the order `Effects` holds them.
        pub const NAMES: &[&str] = &[$(stringify!($field),)*];

        impl Effects {
            fn from_list(v: &[Effect]) -> Self {
                let mut at = v.iter().copied();
                Effects { $($field: at.next().expect("one effect a name"),)* }
            }
        }
    };
}

effects! {
    muzzle_pistol,
    muzzle_rifle,
    shotgun_blast,
    slash,
    pitchfork,
    scythe,
    explosion,
    explosion_smoke,
    blood,
    zombie_rises,
    player_rises,
    dawn_burns,
    prop_crumbles,
    drum_returns,
    fire,
    money,
    crow,
    dog,
}

/// An effect as written: its sprite's name and its numbers.
#[derive(Clone, Debug, PartialEq)]
pub struct Written {
    pub art: String,
    pub fps: f32,
    pub life: f32,
    pub reach: f32,
}

/// Reads `effects.txt`; it must name every effect, each with a sprite. Errors name the line.
pub fn parse(text: &str) -> Result<Vec<Written>, String> {
    let mut out: Vec<Option<Written>> = vec![None; NAMES.len()];
    let mut at = None;
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let k = NAMES.iter().position(|e| *e == name).ok_or_else(|| format!("{n}: [{name}] isn't an effect"))?;
            out[k] = Some(Written { art: String::new(), fps: 10.0, life: 0.2, reach: 1.0 });
            at = Some(k);
            continue;
        }
        let Some(e) = at.and_then(|k| out[k].as_mut()) else { return Err(format!("{n}: a line before any [effect]")) };
        let (key, value) = line.split_once(char::is_whitespace).map_or((line, ""), |(k, v)| (k, v.trim()));
        let number = || value.parse::<f32>().ok().filter(|v| v.is_finite() && *v > 0.0).ok_or_else(|| format!("{n}: {key} is a number above 0"));
        match key {
            "art" if !value.is_empty() && value.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') => e.art = value.to_string(),
            "art" => return Err(format!("{n}: art is a sprite's name, such as SLASH")),
            "fps" => e.fps = number()?.min(120.0),
            "life" => e.life = number()?,
            "reach" => e.reach = number()?,
            _ => return Err(format!("{n}: {key} isn't something an effect says")),
        }
    }
    out.into_iter()
        .zip(NAMES)
        .map(|(e, name)| match e {
            Some(e) if e.art.is_empty() => Err(format!("[{name}] needs an art line")),
            Some(e) => Ok(e),
            None => Err(format!("no [{name}]")),
        })
        .collect()
}

/// A sprite's frames by name: the catalog's, else a player's prop's.
fn frames_of(name: &str) -> Option<&'static [Art]> {
    art::ALL.iter().find(|(n, _)| *n == name).map(|(_, f)| *f).or_else(|| super::props::prop(name).map(|p| p.frames))
}

/// The game's effects, read the first time they're asked for. A sprite that isn't there plays the game's own.
pub fn effects() -> &'static Effects {
    static E: OnceLock<Effects> = OnceLock::new();
    E.get_or_init(|| {
        let own = parse(include_str!("../../effects.txt")).unwrap_or_else(|e| panic!("effects.txt:{e}"));
        let mine = parse(crate::data::effects()).unwrap_or_else(|e| panic!("effects.txt:{e}"));
        let list: Vec<Effect> = mine
            .iter()
            .zip(&own)
            .zip(NAMES)
            .map(|((w, o), name)| {
                let frames = frames_of(&w.art).unwrap_or_else(|| {
                    crate::data::note(format!("effects.txt: [{name}] plays {}, which isn't a sprite", w.art));
                    frames_of(&o.art).unwrap_or_else(|| panic!("effects.txt: [{name}] plays {}, which isn't a sprite", o.art))
                });
                Effect { frames, fps: w.fps, life: w.life, reach: w.reach }
            })
            .collect();
        Effects::from_list(&list)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_effect_plays_a_sprite() {
        let e = effects();
        assert_eq!((e.slash.frames, e.slash.fps), (&art::SLASH[..], 20.0));
        assert_eq!((e.scythe.life, e.scythe.reach), (0.3, 0.7));
        assert_eq!(e.money.once(10.0), art::MONEY[3]);
        let own = include_str!("../../effects.txt");
        assert_eq!(parse(&own.replace("art SLASH\n", "")).unwrap_err(), "[slash] needs an art line");
        assert!(parse("[sparkle]\nart X").unwrap_err().starts_with("1: [sparkle]"));
        assert!(parse("[slash]\nart SLASH").unwrap_err().starts_with("no [muzzle_pistol]"));
    }
}
