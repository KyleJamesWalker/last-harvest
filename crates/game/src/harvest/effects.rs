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
    pub fade: Fade,
    /// A bullet that flies as this effect is a streak of particles in this style, not its sprite.
    pub shot: Option<Shot>,
}

/// A shot's look: the tracer a bullet flies as, the flash and sparks at its muzzle, and the sparks where it hits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    pub color: [f32; 3],
    /// The tracer's length and its width at the head, in map pixels; sparks are about as wide.
    pub length: f32,
    pub size: f32,
    /// Sparks thrown from the muzzle with each shot, and where each bullet hits.
    pub sparks: u32,
    pub impact: u32,
    /// The white-hot flare out of the muzzle, in map pixels.
    pub core: f32,
    /// The sprites it draws in place of plain colored shapes, by `SPRITES`; None draws the shape.
    pub sprites: [Option<&'static [Art]>; SPRITES.len()],
    /// How its smoke puffs fade over their life: its section's alpha and fades.
    pub fade: Fade,
}

/// The sprites a shot style can name, in the order `Shot::sprites` holds them: the tracer, stretched along the
/// bullet's path with its head to the right; a spark and a smoke puff, their frames played over their life; and the
/// muzzle flare, pointing right.
pub const SPRITES: [&str; 4] = ["tracer", "spark", "smoke", "flare"];
pub const TRACER: usize = 0;
pub const SPARK: usize = 1;
pub const SMOKE: usize = 2;
pub const FLARE: usize = 3;

pub const MAX_LENGTH: f32 = 64.0;
pub const MAX_SIZE: f32 = 8.0;
pub const MAX_SPARKS: u32 = 32;
pub const MAX_CORE: f32 = 16.0;

impl Shot {
    /// Where the tracer of a bullet at `pos` flying at `vel`, `age` seconds out, ends behind it: `length` back,
    /// but never past where it left the gun.
    pub fn tail(&self, pos: [f32; 2], vel: [f32; 2], age: f32) -> [f32; 2] {
        let speed = (vel[0] * vel[0] + vel[1] * vel[1]).sqrt();
        if speed <= 0.0 {
            return pos;
        }
        let back = self.length.min(speed * age.max(0.0)) / speed;
        [pos[0] - vel[0] * back, pos[1] - vel[1] * back]
    }

    /// The tracer's width, as a fraction of `size`, and its brightness, `k` of the way from its head to its tail.
    pub fn taper(k: f32) -> (f32, f32) {
        let k = k.clamp(0.0, 1.0);
        (1.0 - 0.85 * k, 1.0 - 0.75 * k)
    }
}

/// How opaque an effect draws over its play: `alpha` through the middle, from clear over the first `fade_in` of its
/// play and back to clear over the last `fade_out`, each a fraction from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fade {
    pub alpha: f32,
    pub fade_in: f32,
    pub fade_out: f32,
}

impl Fade {
    pub const OPAQUE: Fade = Fade { alpha: 1.0, fade_in: 0.0, fade_out: 0.0 };

    /// The alpha `k` of the way through its play, 0 to 1.
    pub fn at(&self, k: f32) -> f32 {
        let k = k.clamp(0.0, 1.0);
        let rise = if self.fade_in > 0.0 { (k / self.fade_in).min(1.0) } else { 1.0 };
        let fall = if self.fade_out > 0.0 { ((1.0 - k) / self.fade_out).min(1.0) } else { 1.0 };
        self.alpha * rise * fall
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
    pub fade: Fade,
    pub color: Option<[u8; 3]>,
    pub length: Option<f32>,
    pub size: Option<f32>,
    pub sparks: Option<u32>,
    pub impact: Option<u32>,
    pub core: Option<f32>,
    pub sprites: [Option<String>; SPRITES.len()],
}

impl Written {
    /// Whether it sets any of a shot's style, and so flies as particles.
    pub fn is_shot(&self) -> bool {
        self.color.is_some() || self.length.is_some() || self.size.is_some() || self.sparks.is_some() || self.impact.is_some() || self.core.is_some() || self.sprites.iter().any(Option::is_some)
    }

    /// Its shot style, the rest taken from `base`, with its sprites' frames as `frames` finds them.
    fn shot(&self, base: Option<Shot>, frames: &dyn Fn(&str) -> Option<&'static [Art]>) -> Option<Shot> {
        let base = base.unwrap_or(Shot { color: [1.0; 3], length: 10.0, size: 1.5, sparks: 6, impact: 4, core: 5.0, sprites: [None; SPRITES.len()], fade: Fade::OPAQUE });
        self.is_shot().then(|| Shot {
            sprites: std::array::from_fn(|k| match &self.sprites[k] {
                Some(name) => frames(name),
                None => base.sprites[k],
            }),
            color: self.color.map_or(base.color, |c| c.map(|v| v as f32 / 255.0)),
            length: self.length.unwrap_or(base.length),
            size: self.size.unwrap_or(base.size),
            sparks: self.sparks.unwrap_or(base.sparks),
            impact: self.impact.unwrap_or(base.impact),
            core: self.core.unwrap_or(base.core),
            fade: if self.fade == Fade::OPAQUE { base.fade } else { self.fade },
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
            let fresh = Written { art: String::new(), fps: 10.0, life: 0.2, reach: 1.0, color: None, length: None, size: None, sparks: None, impact: None, core: None, sprites: Default::default(), fade: Fade::OPAQUE };
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
            "alpha" => e.fade.alpha = value.parse().ok().filter(|v| (crate::alpha::MIN..=1.0).contains(v)).ok_or_else(|| format!("{n}: alpha is a number from {} to 1", crate::alpha::MIN))?,
            "fade_in" | "fade_out" => {
                let v = value.parse().ok().filter(|v| (0.0..=1.0).contains(v)).ok_or_else(|| format!("{n}: {key} is a fraction of its play from 0 to 1"))?;
                *if key == "fade_in" { &mut e.fade.fade_in } else { &mut e.fade.fade_out } = v;
            }
            "length" => e.length = Some(number()?.min(MAX_LENGTH)),
            "size" => e.size = Some(number()?.min(MAX_SIZE)),
            "sparks" | "impact" => {
                let v = value.parse::<u32>().map_err(|_| format!("{n}: {key} is a whole number, 0 or more"))?.min(MAX_SPARKS);
                *if key == "sparks" { &mut e.sparks } else { &mut e.impact } = Some(v);
            }
            "core" => e.core = Some(number()?.min(MAX_CORE)),
            k if SPRITES.contains(&k) && !value.is_empty() && value.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') => {
                e.sprites[SPRITES.iter().position(|s| *s == k).unwrap_or(0)] = Some(value.to_string());
            }
            k if SPRITES.contains(&k) => return Err(format!("{n}: {key} is a sprite's name, such as SHOT_SPARK")),
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

/// The shot style a bullet named `name` flies as, with `text` as effects.txt; no name, or one the file lacks, fires
/// [bullet]. None for a section that sets no style, and so flies as its sprite.
/// Its sprites are the catalog's alone, since a page reads it without the game's data.
pub fn shot_named(text: &str, name: Option<&str>) -> Option<Shot> {
    let catalog = |n: &str| art::ALL.iter().find(|(k, _)| *k == n).map(|(_, f)| *f);
    let (own, extra) = parse(text).ok()?;
    let at = NAMES.iter().position(|n| *n == "bullet")?;
    let base = parse(BUILT_IN).ok()?.0[at].shot(None, &catalog);
    let bullet = own[at].shot(base, &catalog).or(base);
    match section(&own, &extra, name) {
        Some(w) => w.shot(bullet, &catalog),
        None => bullet,
    }
}

/// The names of the sprites a bullet named `name` draws, as `shot_named` finds its style; None draws the shape.
pub fn shot_sprite_names(text: &str, name: Option<&str>) -> [Option<String>; SPRITES.len()] {
    let Ok((own, extra)) = parse(text) else { return Default::default() };
    let at = NAMES.iter().position(|n| *n == "bullet").unwrap_or(0);
    let game = parse(BUILT_IN).map(|(g, _)| g[at].sprites.clone()).unwrap_or_default();
    let bullet: [Option<String>; SPRITES.len()] = std::array::from_fn(|k| own[at].sprites[k].clone().or_else(|| game[k].clone()));
    match section(&own, &extra, name) {
        Some(w) if w.is_shot() => std::array::from_fn(|k| w.sprites[k].clone().or_else(|| bullet[k].clone())),
        Some(_) => Default::default(),
        None => bullet,
    }
}

/// The section a bullet named `name` flies as, other than [bullet]; None for [bullet] or a name the file lacks.
fn section<'a>(own: &'a [Written], extra: &'a Extra, name: Option<&str>) -> Option<&'a Written> {
    match name.unwrap_or("bullet") {
        "bullet" => None,
        n => NAMES.iter().position(|k| *k == n).map(|k| &own[k]).or_else(|| extra.iter().find(|(k, _)| k == n).map(|(_, w)| w)),
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
        let sprite = |name: &str| {
            let found = frames_of(name);
            if found.is_none() {
                crate::data::note(format!("effects.txt: a shot draws {name}, which isn't a sprite, so it draws a plain shape"));
            }
            found
        };
        let game = own[at].shot(None, &sprite);
        let shot = mine[at].shot(game, &sprite).or(game);
        let extra = extra
            .into_iter()
            .filter_map(|(name, w)| match frames_of(&w.art) {
                Some(frames) => Some((name, Effect { frames, fps: w.fps, life: w.life, reach: w.reach, shot: w.shot(shot, &sprite), fade: w.fade })),
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
                let own_shot = if k == at { shot } else { w.shot(shot, &sprite) };
                Effect { frames, fps: w.fps, life: w.life, reach: w.reach, shot: own_shot, fade: w.fade }
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
        assert_eq!(e.money.once(10.0), art::ZOMBIE_HEART[5]);
        let own = include_str!("../../effects.txt");
        assert_eq!(parse(&own.replace("art SLASH\n", "")).unwrap_err(), "[slash] needs an art line");
        assert!(parse("[Sparkle]\nart X").unwrap_err().starts_with("1: [Sparkle]"));
        assert!(parse("[slash]\nart SLASH").unwrap_err().starts_with("no [muzzle_pistol]"));
        let older = own.replace("[spit]\nart BLOOD_SPLAT\nfps 10\n", "").replace("[mound]\nart MOUND\nfps 6\n", "").replace("[mound_breaks]\nart DIRT_PUFF\nfps 9\n", "");
        assert_eq!(older.matches('[').count(), own.matches('[').count() - 3);
        let arts: Vec<_> = parse(&older).unwrap().0.iter().rev().take(3).map(|w| w.art.clone()).collect();
        assert_eq!(arts, ["DIRT_PUFF", "MOUND", "BLOOD_SPLAT"], "a mod's file from before [spit] and [mound] plays the game's");
        let (_, extra) = parse(&format!("{own}\n[red_slash]\nart SCYTHE_SWING\nfps 9")).unwrap();
        assert_eq!((extra.last().unwrap().0.as_str(), extra.last().unwrap().1.fps), ("red_slash", 9.0));
        assert_eq!(e.named("slash"), Some(e.slash));
        assert_eq!(e.named("red_slash"), None);
    }

    #[test]
    fn an_effect_reads_its_alpha_and_fades_and_one_without_them_is_opaque() {
        let own = include_str!("../../effects.txt");
        let (_, extra) = parse(&format!("{own}\n[ghost]\nart SMOKE\nalpha 0.5\nfade_in 0.2\nfade_out 0.4\n[plain]\nart SMOKE")).unwrap();
        assert_eq!(extra[extra.len() - 2].1.fade, Fade { alpha: 0.5, fade_in: 0.2, fade_out: 0.4 });
        assert_eq!(extra[extra.len() - 1].1.fade, Fade::OPAQUE, "a section from before alpha draws opaque");
        let f = extra[extra.len() - 2].1.fade;
        assert_eq!([0.0, 0.1, 0.4, 0.8, 1.0].map(|k| (f.at(k) * 100.0).round() / 100.0), [0.0, 0.25, 0.5, 0.25, 0.0]);
        for bad in ["alpha 0", "alpha 1.2", "alpha x", "fade_in -0.1", "fade_out 2", "fade_out nan"] {
            assert!(parse(&format!("{own}\n[ghost]\nart SMOKE\n{bad}")).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_bullet_flies_as_the_shot_style_its_section_sets() {
        let own = include_str!("../../effects.txt");
        let shot = effects().bullet.shot.expect("the game's bullet is a streak of particles");
        assert!(shot.length > 0.0 && shot.size > 0.0 && shot.sparks > 0);
        let (_, extra) = parse(&format!("{own}\n[tracer]\ncolor 7AA062\nlength 20\nsize 2\nsparks 3\nimpact 2\ncore 6")).unwrap();
        let w = &extra.last().unwrap().1;
        assert_eq!((w.art.as_str(), w.color, w.length, w.size, w.sparks), ("", Some([0x7A, 0xA0, 0x62]), Some(20.0), Some(2.0), Some(3)), "a style needs no art");
        assert_eq!((w.impact, w.core), (Some(2), Some(6.0)));
        assert_eq!(parse(&format!("{own}\n[plain]\nfps 3")).unwrap_err(), "[plain] needs an art line");
        let (_, extra) = parse(&format!("{own}\n[odd]\nart SLASH\nlength 1e9\nsize 99\nsparks 5000\nimpact 999\ncore 1e30")).unwrap();
        let w = &extra.last().unwrap().1;
        assert_eq!((w.length, w.size, w.sparks, w.impact, w.core), (Some(MAX_LENGTH), Some(MAX_SIZE), Some(MAX_SPARKS), Some(MAX_SPARKS), Some(MAX_CORE)), "a mod's numbers are capped");
        for bad in ["length NaN", "length inf", "length -2", "size 0", "sparks 1.5", "sparks -1", "impact x", "core NaN", "core 0", "color 7AA06", "color GGGGGG", "color 0.5 0.5 0.5"] {
            assert!(parse(&format!("{own}\n[odd]\nart SLASH\n{bad}")).is_err(), "{bad}");
        }
        let guns = own.find("[pistol_shot]").unwrap()..own.find("# The colors of particles").unwrap();
        let older: String = own.replace(&own[guns], "").lines().filter(|l| !["color", "length", "size", "sparks", "impact", "core"].iter().any(|k| l.starts_with(k))).map(|l| format!("{l}\n")).collect();
        let (mine, _) = parse(&older).unwrap();
        assert!(mine.iter().all(|w| w.color.is_none()), "a mod's file from before shot styles still reads");
    }

    #[test]
    fn a_page_reads_the_shot_a_weapon_fires_as_the_game_does() {
        let own = include_str!("../../effects.txt");
        assert_eq!(shot_named(own, None), effects().bullet.shot);
        assert_eq!(shot_named(own, Some("slash")), None, "a sprite's section is no style");
        assert_eq!(shot_named(own, Some("nope")), shot_named(own, None), "a name the file lacks fires [bullet], as the game does");
        let text = format!("{own}\n[tracer]\nlength 30");
        let tracer = shot_named(&text, Some("tracer")).unwrap();
        assert_eq!((tracer.length, tracer.sparks), (30.0, effects().bullet.shot.unwrap().sparks), "the rest from [bullet]");
    }

    #[test]
    fn a_shot_draws_the_sprites_it_names_and_a_plain_shape_for_any_it_cant_find() {
        let own = include_str!("../../effects.txt");
        let pistol = effects().named("pistol_shot").and_then(|e| e.shot).unwrap();
        assert_eq!(pistol.sprites, [Some(&[art::SHOT_TRACER][..]), Some(&art::SHOT_SPARK[..]), Some(&art::SHOT_SMOKE[..]), Some(&art::SHOT_FLARE[..])]);
        let names = |name| shot_sprite_names(own, name).map(|n| n.unwrap_or_default());
        assert_eq!(names(Some("shotgun_pellet")), ["SHOT_PELLET", "SHOT_SPARK", "SHOT_SMOKE", "SHOT_FLARE"]);
        assert_eq!(names(Some("nope")), names(None), "a name the file lacks draws [bullet]'s");
        assert_eq!(shot_sprite_names(own, Some("slash")), [None, None, None, None], "a sprite's section is no style");
        let text = format!("{own}\n[mine]\nlength 30\ntracer SHOT_PELLET\nspark PROP_NOPE");
        let mine = shot_named(&text, Some("mine")).unwrap();
        assert_eq!(mine.sprites[TRACER], Some(&[art::SHOT_PELLET][..]));
        assert_eq!(mine.sprites[SPARK], None, "a sprite that isn't there draws the plain spark");
        assert_eq!(mine.sprites[SMOKE], Some(&art::SHOT_SMOKE[..]), "the rest from [bullet]");
        let older: String = own.lines().filter(|l| !SPRITES.iter().any(|k| l.starts_with(&format!("{k} ")))).map(|l| format!("{l}\n")).collect();
        assert_eq!(shot_named(&older, None).unwrap().sprites[FLARE], Some(&art::SHOT_FLARE[..]), "a mod's [bullet] without sprites keeps the game's");
        for bad in ["tracer", "spark shot_spark", "smoke Shot", "flare A B"] {
            assert!(parse(&format!("{own}\n[odd]\nart SLASH\n{bad}")).is_err(), "{bad}");
        }
    }

    #[test]
    fn blasts_and_eruptions_fade_out_as_they_end() {
        let e = effects();
        for (name, fx) in [("explosion", e.explosion), ("explosion_smoke", e.explosion_smoke), ("zombie_rises", e.zombie_rises), ("mound_breaks", e.mound_breaks)] {
            let a: Vec<f32> = [0.5, 0.7, 0.9, 1.0].iter().map(|&k| fx.fade.at(k)).collect();
            assert!(a.windows(2).all(|w| w[1] < w[0]) && a[3] == 0.0, "{name} fades to clear: {a:?}");
        }
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
    fn a_tracer_trails_behind_its_bullet_and_never_behind_the_gun() {
        let shot = Shot { color: [1.0; 3], length: 12.0, size: 1.5, sparks: 4, impact: 3, core: 4.0, sprites: [None; SPRITES.len()], fade: Fade::OPAQUE };
        assert_eq!(shot.tail([100.0, 0.0], [200.0, 0.0], 1.0), [88.0, 0.0]);
        let fresh = shot.tail([100.0, 0.0], [200.0, 0.0], 0.01);
        assert!((fresh[0] - 98.0).abs() < 1e-3, "a bullet two pixels out trails two pixels");
        assert_eq!(shot.tail([5.0, 5.0], [0.0, 0.0], 1.0), [5.0, 5.0]);
        let (head, tail) = (Shot::taper(0.0), Shot::taper(1.0));
        assert!(head.0 > tail.0 && head.1 > tail.1 && tail.0 > 0.0, "it thins and dims toward the tail");
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
