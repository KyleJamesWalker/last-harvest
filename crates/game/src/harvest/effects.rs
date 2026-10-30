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
        #[derive(Clone, Debug)]
        pub struct Effects {
            $(pub $field: Effect,)*
            /// Effects of a mod's own, by the name of their section.
            pub extra: Vec<(String, Effect)>,
        }

        /// Every effect `effects.txt` names, in the order `Effects` holds them.
        pub const NAMES: &[&str] = &[$(stringify!($field),)*];

        impl Effects {
            fn from_list(v: &[Effect], extra: Vec<(String, Effect)>) -> Self {
                let mut at = v.iter().copied();
                Effects { $($field: at.next().expect("one effect a name"),)* extra }
            }

            /// An effect by its section's name, the game's own or a mod's.
            pub fn named(&self, name: &str) -> Option<Effect> {
                match name {
                    $(stringify!($field) => Some(self.$field),)*
                    _ => self.extra.iter().find(|(n, _)| n == name).map(|(_, e)| *e),
                }
            }
        }
    };
}

effects! {
    muzzle_pistol,
    muzzle_rifle,
    shotgun_blast,
    slash,
    bullet,
    bullet_3d,
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

/// Effects of a mod's own as written, by the name of their section.
type Extra = Vec<(String, Written)>;

/// Reads `effects.txt`: the game's effects in the order of `NAMES`, which it must all name, then any of its own by
/// name, each with a sprite. Errors name the line.
pub fn parse(text: &str) -> Result<(Vec<Written>, Extra), String> {
    let mut out: Vec<Option<Written>> = vec![None; NAMES.len()];
    let mut extra: Vec<(String, Written)> = Vec::new();
    let mut at = None;
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let fresh = Written { art: String::new(), fps: 10.0, life: 0.2, reach: 1.0 };
            if !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') || name.is_empty() {
                return Err(format!("{n}: [{name}] isn't a name of lowercase letters, digits and _"));
            }
            if out.iter().zip(NAMES).any(|(o, k)| o.is_some() && *k == name) || extra.iter().any(|(k, _)| k == name) {
                return Err(format!("{n}: [{name}] appears twice"));
            }
            at = Some(match NAMES.iter().position(|e| *e == name) {
                Some(k) => {
                    out[k] = Some(fresh);
                    k
                }
                None => {
                    extra.push((name.to_string(), fresh));
                    NAMES.len() + extra.len() - 1
                }
            });
            continue;
        }
        let e = match at {
            Some(k) if k < NAMES.len() => out[k].as_mut(),
            Some(k) => extra.get_mut(k - NAMES.len()).map(|(_, w)| w),
            None => None,
        };
        let Some(e) = e else { return Err(format!("{n}: a line before any [effect]")) };
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
    let own = out
        .into_iter()
        .zip(NAMES)
        .map(|(e, name)| match e {
            Some(e) if e.art.is_empty() => Err(format!("[{name}] needs an art line")),
            Some(e) => Ok(e),
            None => Err(format!("no [{name}]")),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if let Some((name, _)) = extra.iter().find(|(_, w)| w.art.is_empty()) {
        return Err(format!("[{name}] needs an art line"));
    }
    Ok((own, extra))
}

/// A sprite's frames by name: the catalog's, else a player's prop's.
fn frames_of(name: &str) -> Option<&'static [Art]> {
    art::ALL.iter().find(|(n, _)| *n == name).map(|(_, f)| *f).or_else(|| super::props::prop(name).map(|p| p.frames))
}

/// The game's effects, read the first time they're asked for. A sprite that isn't there plays the game's own.
pub fn effects() -> &'static Effects {
    static E: OnceLock<Effects> = OnceLock::new();
    E.get_or_init(|| {
        let (own, _) = parse(include_str!("../../effects.txt")).unwrap_or_else(|e| panic!("effects.txt:{e}"));
        let (mine, extra) = parse(crate::data::effects()).unwrap_or_else(|e| panic!("effects.txt:{e}"));
        let extra = extra
            .into_iter()
            .filter_map(|(name, w)| match frames_of(&w.art) {
                Some(frames) => Some((name, Effect { frames, fps: w.fps, life: w.life, reach: w.reach })),
                None => {
                    crate::data::note(format!("effects.txt: [{name}] plays {}, which isn't a sprite", w.art));
                    None
                }
            })
            .collect();
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
        Effects::from_list(&list, extra)
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
        assert!(parse("[Sparkle]\nart X").unwrap_err().starts_with("1: [Sparkle]"));
        assert!(parse("[slash]\nart SLASH").unwrap_err().starts_with("no [muzzle_pistol]"));
        let (_, extra) = parse(&format!("{own}\n[red_slash]\nart SCYTHE_SWING\nfps 9")).unwrap();
        assert_eq!((extra[0].0.as_str(), extra[0].1.fps), ("red_slash", 9.0));
        assert_eq!(e.named("slash"), Some(e.slash));
        assert_eq!(e.named("red_slash"), None);
    }
}
