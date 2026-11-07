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
    /// A bullet that flies as this effect is a streak of particles in this style, not its sprite.
    pub shot: Option<Shot>,
}

/// A shot's look: the streak a bullet flies as and the sparks its gun throws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    pub color: [f32; 3],
    /// The streak's length and its specks' width, in map pixels.
    pub length: f32,
    pub size: f32,
    /// Sparks thrown from the muzzle with each shot.
    pub sparks: u32,
}

pub const MAX_LENGTH: f32 = 64.0;
pub const MAX_SIZE: f32 = 8.0;
pub const MAX_SPARKS: u32 = 32;
/// The most specks one streak draws.
pub const MAX_SPECKS: usize = 6;

impl Shot {
    /// The specks of a bullet at `pos` flying at `vel`, `age` seconds out: each one's place, width and brightness
    /// from 0 to 1, head first. The tail never reaches back past where the bullet left the gun.
    pub fn streak(&self, pos: [f32; 2], vel: [f32; 2], age: f32) -> impl Iterator<Item = ([f32; 2], f32, f32)> {
        let speed = (vel[0] * vel[0] + vel[1] * vel[1]).sqrt();
        let dir = if speed > 0.0 { [vel[0] / speed, vel[1] / speed] } else { [0.0; 2] };
        let n = ((self.length / self.size).ceil() as usize).clamp(2, MAX_SPECKS);
        let gap = self.length.min(speed * age.max(0.0)) / (n - 1) as f32;
        let size = self.size;
        (0..n).map(move |i| {
            let k = i as f32 / n as f32;
            ([pos[0] - dir[0] * gap * i as f32, pos[1] - dir[1] * gap * i as f32], size * (1.0 - 0.6 * k), 1.0 - 0.7 * k)
        })
    }
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
    spit,
    mound,
    mound_breaks,
}

/// An effect as written: its sprite's name and its numbers.
#[derive(Clone, Debug, PartialEq)]
pub struct Written {
    pub art: String,
    pub fps: f32,
    pub life: f32,
    pub reach: f32,
    pub color: Option<[u8; 3]>,
    pub length: Option<f32>,
    pub size: Option<f32>,
    pub sparks: Option<u32>,
}

impl Written {
    /// Whether it sets any of a shot's style, and so flies as particles.
    pub fn is_shot(&self) -> bool {
        self.color.is_some() || self.length.is_some() || self.size.is_some() || self.sparks.is_some()
    }

    /// Its shot style, the rest taken from `base`.
    fn shot(&self, base: Option<Shot>) -> Option<Shot> {
        let base = base.unwrap_or(Shot { color: [1.0; 3], length: 10.0, size: 1.5, sparks: 6 });
        self.is_shot().then(|| Shot {
            color: self.color.map_or(base.color, |c| c.map(|v| v as f32 / 255.0)),
            length: self.length.unwrap_or(base.length),
            size: self.size.unwrap_or(base.size),
            sparks: self.sparks.unwrap_or(base.sparks),
        })
    }
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
        let v: Vec<f32> = value.split_whitespace().map(|w| w.parse().ok().filter(|v: &f32| (0.0..=1.0).contains(v))).collect::<Option<_>>().unwrap_or_default();
        let [r, g, b] = v[..] else { return Err(format!("{n}: {key} takes red, green and blue from 0 to 1, such as 0.62 0.08 0.06")) };
        out[k] = [r, g, b];
        seen[k] = true;
    }
    match seen.iter().position(|s| !s) {
        Some(k) => Err(format!("[colors] doesn't set {}", COLORS[k])),
        None => Ok(out),
    }
}

/// The colors in the game's own effects.txt, without reading the game's data.
pub fn built_in_colors() -> Palette {
    parse_colors(include_str!("../../effects.txt"), None).unwrap_or_else(|e| panic!("effects.txt:{e}"))
}

/// The particle colors: the game's own, with any `effects.txt` sets over them.
pub fn colors() -> &'static Palette {
    static C: OnceLock<Palette> = OnceLock::new();
    C.get_or_init(|| {
        parse_colors(crate::data::effects(), Some(&built_in_colors())).unwrap_or_else(|e| panic!("effects.txt:{e}"))
    })
}

/// Reads `effects.txt`: the game's effects in the order of `NAMES`, which it must all name, then any of its own by
/// name, each with a sprite. Errors name the line.
/// The game's own effects.txt.
const BUILT_IN: &str = include_str!("../../effects.txt");
/// Sections added after mods could replace effects.txt, which a mod's file may leave out.
const ADDED_LATER: [&str; 3] = ["spit", "mound", "mound_breaks"];

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
            let fresh = Written { art: String::new(), fps: 10.0, life: 0.2, reach: 1.0, color: None, length: None, size: None, sparks: None };
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
            "length" => e.length = Some(number()?.min(MAX_LENGTH)),
            "size" => e.size = Some(number()?.min(MAX_SIZE)),
            "sparks" => e.sparks = Some(value.parse::<u32>().map_err(|_| format!("{n}: sparks is a whole number, 0 or more"))?.min(MAX_SPARKS)),
            "color" => e.color = Some(crate::paint::hex(value).filter(|_| value.bytes().all(|b| b.is_ascii_hexdigit())).ok_or_else(|| format!("{n}: color is RRGGBB as palette.txt writes one, such as E8D27A"))?),
            _ => return Err(format!("{n}: {key} isn't something an effect says")),
        }
    }
    // A mod's file from before a section was added plays the game's own for it.
    let game = || parse(BUILT_IN).ok().map(|(own, _)| own);
    let own = out
        .into_iter()
        .zip(NAMES)
        .enumerate()
        .map(|(k, (e, name))| match e {
            Some(e) if e.art.is_empty() && !e.is_shot() => Err(format!("[{name}] needs an art line")),
            Some(e) => Ok(e),
            None if ADDED_LATER.contains(name) && text != BUILT_IN => game().map(|g| g[k].clone()).ok_or_else(|| format!("no [{name}]")),
            None => Err(format!("no [{name}]")),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if let Some((name, _)) = extra.iter().find(|(_, w)| w.art.is_empty() && !w.is_shot()) {
        return Err(format!("[{name}] needs an art line"));
    }
    Ok((own, extra))
}

/// The shot style a bullet named `name` flies as, with `text` as effects.txt; no name fires [bullet]. None for a
/// section that sets no style, and so flies as its sprite, or one the file lacks.
pub fn shot_named(text: &str, name: Option<&str>) -> Option<Shot> {
    let (own, extra) = parse(text).ok()?;
    let at = NAMES.iter().position(|n| *n == "bullet")?;
    let base = parse(BUILT_IN).ok()?.0[at].shot(None);
    let bullet = own[at].shot(base).or(base);
    match name.unwrap_or("bullet") {
        "bullet" => bullet,
        n => NAMES.iter().position(|k| *k == n).map(|k| &own[k]).or_else(|| extra.iter().find(|(k, _)| k == n).map(|(_, w)| w))?.shot(bullet),
    }
}

/// A sprite's frames by name: the catalog's, else a player's prop's.
fn frames_of(name: &str) -> Option<&'static [Art]> {
    if name.is_empty() {
        return frames_of("BULLET");
    }
    art::ALL.iter().find(|(n, _)| *n == name).map(|(_, f)| *f).or_else(|| super::props::prop(name).map(|p| p.frames))
}

/// The game's effects, read the first time they're asked for. A sprite that isn't there plays the game's own.
pub fn effects() -> &'static Effects {
    static E: OnceLock<Effects> = OnceLock::new();
    E.get_or_init(|| {
        let (own, _) = parse(include_str!("../../effects.txt")).unwrap_or_else(|e| panic!("effects.txt:{e}"));
        let (mine, extra) = parse(crate::data::effects()).unwrap_or_else(|e| panic!("effects.txt:{e}"));
        let at = NAMES.iter().position(|n| *n == "bullet").expect("a bullet effect");
        let shot = mine[at].shot(own[at].shot(None)).or(own[at].shot(None));
        let extra = extra
            .into_iter()
            .filter_map(|(name, w)| match frames_of(&w.art) {
                Some(frames) => Some((name, Effect { frames, fps: w.fps, life: w.life, reach: w.reach, shot: w.shot(shot) })),
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
            .enumerate()
            .map(|(k, ((w, o), name))| {
                let frames = frames_of(&w.art).unwrap_or_else(|| {
                    crate::data::note(format!("effects.txt: [{name}] plays {}, which isn't a sprite", w.art));
                    frames_of(&o.art).unwrap_or_else(|| panic!("effects.txt: [{name}] plays {}, which isn't a sprite", o.art))
                });
                let own_shot = if k == at { shot } else { w.shot(shot) };
                Effect { frames, fps: w.fps, life: w.life, reach: w.reach, shot: own_shot }
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
        let older = own.replace("[spit]\nart BLOOD_SPLAT\nfps 10\n", "").replace("[mound]\nart MOUND\nfps 6\n", "").replace("[mound_breaks]\nart DIRT_PUFF\nfps 9\n", "");
        assert_eq!(older.matches('[').count(), own.matches('[').count() - 3);
        let arts: Vec<_> = parse(&older).unwrap().0.iter().rev().take(3).map(|w| w.art.clone()).collect();
        assert_eq!(arts, ["DIRT_PUFF", "MOUND", "BLOOD_SPLAT"], "a mod's file from before [spit] and [mound] plays the game's");
        let (_, extra) = parse(&format!("{own}\n[red_slash]\nart SCYTHE_SWING\nfps 9")).unwrap();
        assert_eq!((extra[0].0.as_str(), extra[0].1.fps), ("red_slash", 9.0));
        assert_eq!(e.named("slash"), Some(e.slash));
        assert_eq!(e.named("red_slash"), None);
    }

    #[test]
    fn a_bullet_flies_as_the_shot_style_its_section_sets() {
        let own = include_str!("../../effects.txt");
        let shot = effects().bullet.shot.expect("the game's bullet is a streak of particles");
        assert!(shot.length > 0.0 && shot.size > 0.0 && shot.sparks > 0);
        let (_, extra) = parse(&format!("{own}\n[tracer]\ncolor 7AA062\nlength 20\nsize 2\nsparks 3")).unwrap();
        let w = &extra[0].1;
        assert_eq!((w.art.as_str(), w.color, w.length, w.size, w.sparks), ("", Some([0x7A, 0xA0, 0x62]), Some(20.0), Some(2.0), Some(3)), "a style needs no art");
        assert_eq!(parse(&format!("{own}\n[plain]\nfps 3")).unwrap_err(), "[plain] needs an art line");
        let (_, extra) = parse(&format!("{own}\n[odd]\nart SLASH\nlength 1e9\nsize 99\nsparks 5000")).unwrap();
        assert_eq!((extra[0].1.length, extra[0].1.size, extra[0].1.sparks), (Some(MAX_LENGTH), Some(MAX_SIZE), Some(MAX_SPARKS)), "a mod's numbers are capped");
        for bad in ["length NaN", "length inf", "length -2", "size 0", "sparks 1.5", "sparks -1", "color 7AA06", "color GGGGGG", "color 0.5 0.5 0.5"] {
            assert!(parse(&format!("{own}\n[odd]\nart SLASH\n{bad}")).is_err(), "{bad}");
        }
        let older: String = own.lines().filter(|l| !["color", "length", "size", "sparks"].iter().any(|k| l.starts_with(k))).map(|l| format!("{l}\n")).collect();
        let (mine, _) = parse(&older).unwrap();
        assert!(mine.iter().all(|w| w.color.is_none()), "a mod's file from before shot styles still reads");
    }

    #[test]
    fn a_page_reads_the_shot_a_weapon_fires_as_the_game_does() {
        let own = include_str!("../../effects.txt");
        assert_eq!(shot_named(own, None), effects().bullet.shot);
        assert_eq!(shot_named(own, Some("slash")), None, "a sprite's section is no style");
        assert_eq!(shot_named(own, Some("nope")), None);
        let text = format!("{own}\n[tracer]\nlength 30");
        let tracer = shot_named(&text, Some("tracer")).unwrap();
        assert_eq!((tracer.length, tracer.sparks), (30.0, effects().bullet.shot.unwrap().sparks), "the rest from [bullet]");
    }

    #[test]
    fn the_games_own_shot_colors_come_from_its_palette() {
        let (own, extra) = parse(include_str!("../../effects.txt")).unwrap();
        let palette = crate::paint::palette();
        for c in own.iter().chain(extra.iter().map(|(_, w)| w)).filter_map(|w| w.color) {
            assert!(palette.contains(&c), "{c:02X?} isn't in palette.txt");
        }
    }

    #[test]
    fn a_streak_trails_behind_its_bullet_and_never_behind_the_gun() {
        let shot = Shot { color: [1.0; 3], length: 12.0, size: 1.5, sparks: 4 };
        let specks: Vec<_> = shot.streak([100.0, 0.0], [200.0, 0.0], 1.0).collect();
        assert!((2..=MAX_SPECKS).contains(&specks.len()));
        assert_eq!(specks[0].0, [100.0, 0.0], "the head is the bullet");
        assert!(specks.windows(2).all(|w| w[1].0[0] < w[0].0[0] && w[1].1 <= w[0].1), "the tail trails back and thins");
        assert!((specks.last().unwrap().0[0] - 88.0).abs() < 1e-3);
        let fresh: Vec<_> = shot.streak([100.0, 0.0], [200.0, 0.0], 0.01).collect();
        assert!(fresh.iter().all(|s| s.0[0] >= 98.0 - 1e-3), "a bullet two pixels out trails two pixels");
    }

    #[test]
    fn particle_colors_read_and_a_mod_sets_only_some() {
        assert_eq!(colors()[0], [0.62, 0.08, 0.06]);
        let mine = parse_colors("[colors]\ngold 0 0 1\n[slash]\nart SLASH", Some(colors())).unwrap();
        assert_eq!((mine[5], mine[0]), ([0.0, 0.0, 1.0], colors()[0]));
        assert_eq!(parse_colors("[colors]\ngold 0 0 1", None).unwrap_err(), "[colors] doesn't set blood");
        assert!(parse_colors("[colors]\nmauve 1 1 1", Some(colors())).unwrap_err().starts_with("2: mauve isn't a color"));
        assert!(parse_colors("[colors]\ngold 2 0 0", Some(colors())).unwrap_err().starts_with("2: gold takes"));
        assert!(parse_colors("[colors]\ngold 1 2 0 0", Some(colors())).unwrap_err().starts_with("2: gold takes"), "a number out of range isn't dropped");
    }
}
