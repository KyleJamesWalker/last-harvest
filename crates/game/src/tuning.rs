//! The numbers the game plays by that no card, prop or powerup sets, read from `tuning.txt`.

use std::cell::Cell;
use std::sync::OnceLock;

use crate::difficulty::Tier;

macro_rules! tuning {
    ($($field:ident = $section:literal $key:literal,)*) => {
        /// Every number `tuning.txt` sets, by section and key.
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct Tuning {
            $(pub $field: f32,)*
        }

        /// Each number's section and key, in the order `Tuning` holds them.
        pub const KEYS: &[(&str, &str)] = &[$(($section, $key),)*];

        impl Tuning {
            fn from_values(v: &[f32]) -> Self {
                let mut at = v.iter().copied();
                Tuning { $($field: at.next().unwrap_or_default(),)* }
            }

            /// Each number in the order of `KEYS`.
            pub fn values(&self) -> Vec<f32> {
                vec![$(self.$field,)*]
            }
        }
    };
}

tuning! {
    player_hp = "player" "hp",
    player_speed = "player" "speed",
    player_magnet = "player" "magnet",
    merge_after = "player" "merge_after",
    merge_reach = "player" "merge_reach",
    merge_grow = "player" "merge_grow",
    crit = "player" "crit",
    pickup = "player" "pickup",
    player_width = "player" "width",
    player_depth = "player" "depth",
    turn_speed = "player" "turn",
    night = "night" "length",
    dusk_near = "night" "dusk_near",
    dusk_far = "night" "dusk_far",
    dawn_near = "night" "dawn_near",
    dawn_far = "night" "dawn_far",
    home = "night" "home",
    rare = "night" "rare",
    push = "night" "push",
    spawn_rate = "spawn" "rate",
    spawn_growth = "spawn" "growth",
    spawn_top = "spawn" "top",
    horde_every = "spawn" "horde_every",
    horde_size = "spawn" "horde",
    horde_growth = "spawn" "horde_growth",
    bank = "spawn" "bank",
    bank_release = "spawn" "release",
    straggler_reach = "spawn" "straggler",
    straggler_time = "spawn" "straggler_time",
    spawn_per_player = "spawn" "per_player",
    held_weight = "levelup" "held_weight",
    banish = "levelup" "banish",
    reroll = "levelup" "reroll",
    favor = "levelup" "favor",
    favor_weight = "levelup" "favor_weight",
    favor_level = "levelup" "favor_level",
    breaker = "levelup" "breaker",
    max_zombies = "zombies" "max",
    max_per_player = "zombies" "per_player",
    health_per_player = "zombies" "health_per_player",
    rise = "zombies" "rise",
    rise_bite = "zombies" "rise_bite",
    rise_bite_from = "zombies" "rise_bite_from",
    stagger = "zombies" "stagger",
    growth = "zombies" "growth",
    leader_hp = "zombies" "leader_hp",
    fury = "zombies" "fury",
    zombie_width = "zombies" "width",
    zombie_depth = "zombies" "depth",
    walk_fps = "zombies" "walk_fps",
    bite_reach = "zombies" "bite_reach",
    charge_reach = "zombies" "charge_reach",
    spit_speed = "zombies" "spit_speed",
    spit_every = "zombies" "spit_every",
    burrow_reach = "zombies" "burrow_reach",
    chew = "zombies" "chew",
    roam_speed = "roam" "speed",
    roam_turn = "roam" "turn",
    roam_after = "roam" "after",
    leader_pick = "leader" "respawn_pick",
    leader_min_tiles = "leader" "respawn_min_tiles",
    leader_swarm = "leader" "swarm",
    leader_speed = "leader" "speed",
    leader_call = "leader" "call",
    leader_chase = "leader" "chase",
    camp_after = "camp" "after",
    camp_radius = "camp" "radius",
    camp_every = "camp" "every",
    camp_wave = "camp" "wave",
    camp_grow = "camp" "grow",
    camp_ring = "camp" "ring",
    camp_over = "camp" "over",
    harvester_hp = "harvester" "hp",
    harvester_speed = "harvester" "speed",
    harvester_bite = "harvester" "bite",
    harvester_rise = "harvester" "rise",
    harvester_again = "harvester" "again",
    harvester_most = "harvester" "most",
    driver_after = "driver" "after",
    driver_every = "driver" "every",
    driver_reach = "driver" "reach",
    grenade_recharge = "grenade" "recharge",
    grenade_flight = "grenade" "flight",
    grenade_radius = "grenade" "radius",
    grenade_damage = "grenade" "damage",
    bomblet_radius = "grenade" "bomblet_radius",
    bomblet_damage = "grenade" "bomblet_damage",
    aim_window = "grenade" "aim_window",
    aim_slow = "grenade" "aim_slow",
    aim_speed = "grenade" "aim_speed",
    aim_memory = "grenade" "aim_memory",
    aim_ahead = "grenade" "aim_ahead",
    quick_near = "grenade" "quick_near",
    quick_far = "grenade" "quick_far",
    quick_grow = "grenade" "quick_grow",
    barrel_hp = "barrel" "hp",
    barrel_radius = "barrel" "radius",
    barrel_damage = "barrel" "damage",
    view_half = "weapons" "view_half",
    behind_half = "weapons" "behind_half",
    point_blank = "weapons" "point_blank",
    crow_reach = "weapons" "crow_reach",
    dog_reach = "weapons" "dog_reach",
    dog_stray = "weapons" "dog_stray",
    dog_trot = "weapons" "dog_trot",
    dog_leap = "weapons" "dog_leap",
    dog_leap_height = "weapons" "dog_leap_height",
    dog_leap_time = "weapons" "dog_leap_time",
    dog_horde = "weapons" "dog_horde",
    dog_horde_spread = "weapons" "dog_horde_spread",
    maul_shatter = "weapons" "maul_shatter",
    drone_hold = "weapons" "drone_hold",
    drone_reach = "weapons" "drone_reach",
    drone_bullet = "weapons" "drone_bullet",
    fire_burn = "weapons" "fire_burn",
    soak_spread = "weapons" "soak_spread",
    soak_blast = "weapons" "soak_blast",
    blast_hurt = "blast" "hurt",
    blast_near = "blast" "near",
    blast_clear = "blast" "clear",
    map_scale = "map" "scale",
    map_sight = "map" "sight",
    map_memory = "map" "memory",
    map_shade = "map" "shade",
    map_mini = "map" "mini",
    map_mini_reach = "map" "mini_reach",
    photo_walk = "photo" "walk",
}

/// A difficulty: a `[section]` of `tuning.txt` that isn't one of the base sections. It names itself, says whose
/// starting kit and grenades it plays with, and sets how bites and health go and any base number it changes.
#[derive(Clone, Debug, PartialEq)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub blurb: String,
    pub kit: Tier,
    /// Multiplies the damage a zombie deals.
    pub damage: f32,
    /// Seconds a hit leaves the player safe from the next.
    pub hurt_time: f32,
    /// One kill in this many drops health.
    pub health_drop: f32,
    pub heal: f32,
    /// The base numbers it sets, by their place in `KEYS`.
    pub overrides: Vec<(usize, f32)>,
    /// The base numbers with its own laid over them.
    pub tuning: Tuning,
}

/// Most difficulties the game and its mods may have together.
pub const PRESETS_MAX: usize = 16;
/// Most characters in a difficulty's id.
pub const PRESET_ID_MAX: usize = 24;
const PRESET_NUMBERS: [&str; 4] = ["damage", "hurt_time", "health_drop", "heal"];

/// `tuning.txt` as read: the base numbers and the difficulties, in order.
#[derive(Clone, Debug, PartialEq)]
pub struct Rules {
    pub base: Tuning,
    pub presets: Vec<Preset>,
}

impl std::ops::Deref for Rules {
    type Target = Tuning;
    fn deref(&self) -> &Tuning {
        &self.base
    }
}

/// Whether `id` can name a difficulty.
pub fn valid_preset_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= PRESET_ID_MAX
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// Whether `section` is one of the base sections, not a difficulty.
pub fn is_base_section(section: &str) -> bool {
    KEYS.iter().any(|(s, _)| *s == section)
}

fn number(n: usize, section: &str, key: &str, value: &str) -> Result<f32, String> {
    // These divide, end the run, flood it or empty the level-up draw at 0.
    let above = matches!(
        (section, key),
        ("night", "length")
            | ("player", "hp")
            | ("grenade", "flight" | "quick_grow")
            | ("map", "scale")
            | ("harvester", "again")
            | ("spawn", "horde_every")
            | ("driver", "every")
            | ("levelup", "held_weight" | "favor_weight")
    );
    let ok = |v: &f32| v.is_finite() && if above { *v > 0.0 } else { *v >= 0.0 };
    value.parse::<f32>().ok().filter(ok).ok_or_else(|| {
        format!(
            "{n}: {key} is a number from {}",
            if above { "above 0" } else { "0 up" }
        )
    })
}

/// A difficulty as a section sets it, before what it leaves out is filled in.
#[derive(Default)]
struct Partial {
    id: String,
    line: usize,
    name: Option<String>,
    blurb: Option<String>,
    kit: Option<Tier>,
    numbers: [Option<f32>; 4],
    overrides: Vec<(usize, f32)>,
}

/// Reads a `tuning.txt` over `base`, the numbers and difficulties it doesn't set; with no base, it must set every
/// number and the easy, normal and hard difficulties. Errors name the line.
pub fn parse(text: &str, base: Option<&Rules>) -> Result<Rules, String> {
    let mut values: Vec<Option<f32>> = match base {
        Some(b) => b.values().into_iter().map(Some).collect(),
        None => vec![None; KEYS.len()],
    };
    let mut parts: Vec<Partial> = base.map_or_else(Vec::new, |b| {
        b.presets
            .iter()
            .map(|p| Partial {
                id: p.id.clone(),
                name: Some(p.name.clone()),
                blurb: Some(p.blurb.clone()),
                kit: Some(p.kit),
                numbers: [p.damage, p.hurt_time, p.health_drop, p.heal].map(Some),
                overrides: p.overrides.clone(),
                line: 0,
            })
            .collect()
    });
    let known = parts.len();
    let mut section = String::new();
    let mut preset: Option<usize> = None;
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(s) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = s.to_string();
            preset = None;
            if is_base_section(s) {
                continue;
            }
            if !valid_preset_id(s) {
                return Err(format!(
                    "{n}: [{s}] isn't a section, and a difficulty's id is 1 to {PRESET_ID_MAX} lowercase letters, digits and _"
                ));
            }
            preset = Some(parts.iter().position(|p| p.id == s).unwrap_or_else(|| {
                parts.push(Partial {
                    id: s.to_string(),
                    line: n,
                    ..Partial::default()
                });
                parts.len() - 1
            }));
            continue;
        }
        let (key, value) = line
            .split_once(char::is_whitespace)
            .map_or((line, ""), |(k, v)| (k, v.trim()));
        if let Some(p) = preset {
            let part = &mut parts[p];
            match key {
                "name" if !value.is_empty() => part.name = Some(value.to_string()),
                "name" => return Err(format!("{n}: name needs some words")),
                "blurb" => part.blurb = Some(value.to_string()),
                "kit" => {
                    part.kit = Some(
                        Tier::from_key(value)
                            .ok_or_else(|| format!("{n}: kit is easy, normal or hard"))?,
                    )
                }
                _ => {
                    if let Some(at) = PRESET_NUMBERS.iter().position(|k| *k == key) {
                        part.numbers[at] = Some(number(n, "", key, value)?);
                    } else {
                        let (s, k) = key.split_once('.').ok_or_else(|| format!("{n}: {key} isn't name, blurb, kit, damage, hurt_time, health_drop, heal or a section.key such as spawn.rate"))?;
                        let at = KEYS
                            .iter()
                            .position(|(ks, kk)| *ks == s && *kk == k)
                            .ok_or_else(|| format!("{n}: {key} isn't a number tuning.txt sets"))?;
                        let v = number(n, s, k, value)?;
                        part.overrides.retain(|o| o.0 != at);
                        part.overrides.push((at, v));
                    }
                }
            }
            continue;
        }
        let at = KEYS
            .iter()
            .position(|(s, k)| *s == section && *k == key)
            .ok_or_else(|| format!("{n}: {key} isn't a number [{section}] sets"))?;
        values[at] = Some(number(n, &section, key, value)?);
    }
    if let Some(i) = values.iter().position(Option::is_none) {
        let (s, k) = KEYS[i];
        return Err(format!("[{s}] doesn't set {k}"));
    }
    let base = Tuning::from_values(&values.into_iter().flatten().collect::<Vec<_>>());
    if parts.len() > PRESETS_MAX {
        return Err(format!(
            "{} difficulties; the game keeps at most {PRESETS_MAX}",
            parts.len()
        ));
    }
    for tier in Tier::ALL {
        if !parts.iter().any(|p| p.id == tier.key()) {
            return Err(format!("no [{}] difficulty", tier.key()));
        }
    }
    let tier_numbers = |parts: &[Partial], tier: Tier| {
        parts
            .iter()
            .find(|p| p.id == tier.key())
            .map_or([None; 4], |p| p.numbers)
    };
    let mut presets = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        let kit = p
            .kit
            .or_else(|| Tier::from_key(&p.id))
            .unwrap_or(Tier::Normal);
        let name = match &p.name {
            Some(name) => name.clone(),
            None if i < known || Tier::from_key(&p.id).is_some() => p.id.clone(),
            None => {
                return Err(format!(
                    "{}: [{}] isn't a section, and a new difficulty needs a name line",
                    p.line, p.id
                ));
            }
        };
        let fallback = tier_numbers(&parts, kit);
        let mut numbers = [0.0; 4];
        for (k, out) in numbers.iter_mut().enumerate() {
            *out = p.numbers[k]
                .or(fallback[k])
                .ok_or_else(|| format!("[{}] doesn't set {}", p.id, PRESET_NUMBERS[k]))?;
        }
        let mut v = base.values();
        for &(at, value) in &p.overrides {
            v[at] = value;
        }
        let [damage, hurt_time, health_drop, heal] = numbers;
        presets.push(Preset {
            id: p.id.clone(),
            name,
            blurb: p.blurb.clone().unwrap_or_default(),
            kit,
            damage,
            hurt_time,
            health_drop,
            heal,
            overrides: p.overrides.clone(),
            tuning: Tuning::from_values(&v),
        });
    }
    Ok(Rules { base, presets })
}

/// The game's own numbers, as built in, which a pack's `tuning.txt` sets over.
pub fn built_in() -> Rules {
    parse(include_str!("../tuning.txt"), None).unwrap_or_else(|e| panic!("tuning.txt:{e}"))
}

impl Tuning {
    /// Half the player's footprint for walls, across and down.
    pub fn player_half(&self) -> [f32; 2] {
        [self.player_width / 2.0, self.player_depth / 2.0]
    }

    pub fn zombie_half(&self) -> [f32; 2] {
        [self.zombie_width / 2.0, self.zombie_depth / 2.0]
    }
}

/// The base numbers and every difficulty, read the first time they're asked for.
pub fn rules() -> &'static Rules {
    static R: OnceLock<Rules> = OnceLock::new();
    R.get_or_init(|| match crate::data::tuning() {
        Some(text) => parse(text, Some(&built_in())).unwrap_or_else(|e| panic!("tuning.txt:{e}")),
        None => built_in(),
    })
}

thread_local! {
    static PLAYING: Cell<Option<usize>> = const { Cell::new(None) };
}

/// Makes `tuning` on this thread give difficulty `preset`'s numbers, or the base numbers for None.
pub fn play_at(preset: Option<usize>) {
    PLAYING.with(|p| p.set(preset));
}

/// The numbers the game plays by: the difficulty this thread plays at, else the base numbers.
pub fn tuning() -> &'static Tuning {
    let r = rules();
    match PLAYING.with(Cell::get).and_then(|i| r.presets.get(i)) {
        Some(p) => &p.tuning,
        None => &r.base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_numbers_read_and_a_mod_sets_only_what_it_says() {
        let base = built_in();
        assert_eq!(
            (
                base.player_hp,
                base.harvester_hp,
                base.presets[2].health_drop
            ),
            (100.0, 2500.0, 200.0)
        );
        let modded = parse("[player]\nhp 150", Some(&base)).unwrap();
        assert_eq!(
            (modded.player_hp, modded.player_speed),
            (150.0, base.player_speed)
        );
        assert!(
            parse("[player]\nhp 150", None)
                .unwrap_err()
                .contains("doesn't set")
        );
        assert!(
            parse("[space]\n", Some(&base))
                .unwrap_err()
                .starts_with("1: [space]")
        );
        assert!(
            parse("[player]\nwings 2", Some(&base))
                .unwrap_err()
                .starts_with("2: wings")
        );
        assert!(
            parse("[player]\nhp -1", Some(&base))
                .unwrap_err()
                .contains("above 0")
        );
        assert!(
            parse("[night]\nlength 0", Some(&base))
                .unwrap_err()
                .contains("above 0")
        );
        assert!(parse("[player]\ncrit 0", Some(&base)).is_ok());
        assert!(
            parse("[grenade]\nquick_grow 0", Some(&base))
                .unwrap_err()
                .contains("above 0")
        );
        assert!(
            parse("[harvester]\nagain 0", Some(&base))
                .unwrap_err()
                .contains("above 0")
        );
        assert!(
            parse("[spawn]\nhorde_every 0", Some(&base))
                .unwrap_err()
                .contains("above 0")
        );
        assert_eq!(
            (
                base.spawn_rate,
                base.spawn_growth,
                base.spawn_top,
                base.horde_every,
                base.horde_size,
                base.horde_growth,
                base.bank
            ),
            (0.7, 0.9, 9.0, 90.0, 20.0, 4.0, 40.0)
        );
        assert!(
            parse("[levelup]\nheld_weight 0", Some(&base))
                .unwrap_err()
                .contains("above 0")
        );
        assert!(parse("[levelup]\nbanish 0\nreroll 0\nfavor 0", Some(&base)).is_ok());
        assert!(
            parse("[levelup]\nfavor_weight 0", Some(&base))
                .unwrap_err()
                .contains("above 0")
        );
        assert!(
            parse("[driver]\nevery 0", Some(&base))
                .unwrap_err()
                .contains("above 0")
        );
        assert_eq!(
            parse("[driver]\nafter 0", Some(&base))
                .unwrap()
                .driver_after,
            0.0,
            "after 0 turns collection day off"
        );
    }

    #[test]
    fn a_difficulty_lays_its_numbers_over_the_base_and_leaves_the_rest() {
        let base = built_in();
        let normal = &base.presets[2];
        assert_eq!(
            normal.tuning, base.base,
            "easy, normal and hard play the base numbers"
        );
        assert!(base.presets[2..4].iter().all(|p| p.tuning == base.base));
        assert_eq!(
            base.presets[1].tuning,
            Tuning {
                rise_bite: 0.0,
                ..base.base
            },
            "easy only spares bites while rising"
        );
        let brutal = base.presets.iter().find(|p| p.id == "brutal").unwrap();
        assert_eq!(
            (
                brutal.tuning.spawn_rate,
                brutal.tuning.player_hp,
                brutal.kit,
                brutal.damage
            ),
            (1.0, base.player_hp, Tier::Hard, 1.4),
            "its kit's bites, the base's health"
        );
        let modded = parse("[spawn]\nrate 2\n\n[brutal]\nleader.speed 60\n\n[nightmare]\nname Nightmare\nkit easy\nzombies.max 50\n", Some(&base)).unwrap();
        let brutal = modded.presets.iter().find(|p| p.id == "brutal").unwrap();
        assert_eq!(
            (
                brutal.tuning.spawn_rate,
                brutal.tuning.leader_speed,
                brutal.tuning.spawn_top
            ),
            (1.0, 60.0, 12.0),
            "a mod changes one line and keeps the rest"
        );
        let nightmare = modded.presets.last().unwrap();
        assert_eq!(
            (
                nightmare.id.as_str(),
                nightmare.name.as_str(),
                nightmare.damage
            ),
            ("nightmare", "Nightmare", 0.55),
            "a new one comes last, with its kit's bites"
        );
        assert_eq!(
            (nightmare.tuning.max_zombies, nightmare.tuning.spawn_rate),
            (50.0, 2.0),
            "and the mod's base numbers"
        );
        assert!(
            parse("[nightmare]\nkit easy\n", Some(&base))
                .unwrap_err()
                .starts_with("1: [nightmare]"),
            "a typo isn't a new difficulty"
        );
        assert!(
            parse("[brutal]\nspawn.wings 2\n", Some(&base))
                .unwrap_err()
                .starts_with("2: spawn.wings")
        );
        assert!(
            parse("[brutal]\nspawn.rate -2\n", Some(&base))
                .unwrap_err()
                .contains("0 up")
        );
        assert!(
            parse("[brutal]\nnight.length 0\n", Some(&base))
                .unwrap_err()
                .contains("above 0")
        );
        assert!(
            parse("[brutal]\nkit insane\n", Some(&base))
                .unwrap_err()
                .starts_with("2: kit")
        );
        assert!(
            parse("[Brutal!]\n", Some(&base))
                .unwrap_err()
                .starts_with("1: [Brutal!]")
        );
        let many: String = (0..PRESETS_MAX)
            .map(|i| format!("[extra{i}]\nname Extra\n"))
            .collect();
        assert!(parse(&many, Some(&base)).unwrap_err().contains("at most"));
    }
}
