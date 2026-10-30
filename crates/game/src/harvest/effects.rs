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
    tacks,
    flare,
    grenade,
    molotov,
    health,
    chest,
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

/// The particle colors `[colors]` sets, in order.
pub const COLORS: [&str; 7] = ["blood", "dark_blood", "dirt", "spark", "ember", "gold", "firefly"];

/// Particle colors by `COLORS`, red, green and blue from 0 to 1.
pub type Palette = [[f32; 3]; COLORS.len()];

/// Reads the `[colors]` section of `effects.txt` over `base`, or alone when there's no base, when it must set
/// every color. Errors name the line.
pub fn parse_colors(text: &str, base: Option<&Palette>) -> Result<Palette, String> {
    let mut out = base.copied().unwrap_or_default();
    let mut seen = [base.is_some(); COLORS.len()];
    let mut inside = false;
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.split('#').next().unwrap_or("").trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            inside = name == "colors";
            continue;
        }
        if !inside || line.is_empty() {
            continue;
        }
        let (key, value) = line.split_once(char::is_whitespace).map_or((line, ""), |(k, v)| (k, v.trim()));
        let k = COLORS.iter().position(|c| *c == key).ok_or_else(|| format!("{n}: {key} isn't a color; there's {}", COLORS.join(", ")))?;
        let v: Vec<f32> = value.split_whitespace().filter_map(|w| w.parse().ok()).filter(|v: &f32| (0.0..=1.0).contains(v)).collect();
        let [r, g, b] = v[..] else { return Err(format!("{n}: {key} takes red, green and blue from 0 to 1, such as 0.62 0.08 0.06")) };
        out[k] = [r, g, b];
        seen[k] = true;
    }
    match seen.iter().position(|s| !s) {
        Some(k) => Err(format!("[colors] doesn't set {}", COLORS[k])),
        None => Ok(out),
    }
}

/// The particle colors: the game's own, with any `effects.txt` sets over them.
pub fn colors() -> &'static Palette {
    static C: OnceLock<Palette> = OnceLock::new();
    C.get_or_init(|| {
        let own = parse_colors(include_str!("../../effects.txt"), None).unwrap_or_else(|e| panic!("effects.txt:{e}"));
        parse_colors(crate::data::effects(), Some(&own)).unwrap_or_else(|e| panic!("effects.txt:{e}"))
    })
}

/// Reads `effects.txt`: the game's effects in the order of `NAMES`, which it must all name, then any of its own by
/// name, each with a sprite. Errors name the line.
pub fn parse(text: &str) -> Result<(Vec<Written>, Extra), String> {
    let mut out: Vec<Option<Written>> = vec![None; NAMES.len()];
    let mut extra: Vec<(String, Written)> = Vec::new();
    let mut at = None;
    let mut colors = false;
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            colors = name == "colors";
            if colors {
                continue;
            }
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
        if colors {
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

    #[test]
    fn particle_colors_read_and_a_mod_sets_only_some() {
        assert_eq!(colors()[0], [0.62, 0.08, 0.06]);
        let mine = parse_colors("[colors]\ngold 0 0 1\n[slash]\nart SLASH", Some(colors())).unwrap();
        assert_eq!((mine[5], mine[0]), ([0.0, 0.0, 1.0], colors()[0]));
        assert_eq!(parse_colors("[colors]\ngold 0 0 1", None).unwrap_err(), "[colors] doesn't set blood");
        assert!(parse_colors("[colors]\nmauve 1 1 1", Some(colors())).unwrap_err().starts_with("2: mauve isn't a color"));
        assert!(parse_colors("[colors]\ngold 2 0 0", Some(colors())).unwrap_err().starts_with("2: gold takes"));
    }
}
