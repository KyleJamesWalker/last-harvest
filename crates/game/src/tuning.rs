//! The numbers the game plays by that no card, prop or powerup sets, read from `tuning.txt`.

use std::sync::OnceLock;

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
    crit = "player" "crit",
    pickup = "player" "pickup",
    easy_damage = "easy" "damage",
    easy_hurt_time = "easy" "hurt_time",
    easy_health_drop = "easy" "health_drop",
    easy_heal = "easy" "heal",
    normal_damage = "normal" "damage",
    normal_hurt_time = "normal" "hurt_time",
    normal_health_drop = "normal" "health_drop",
    normal_heal = "normal" "heal",
    hard_damage = "hard" "damage",
    hard_hurt_time = "hard" "hurt_time",
    hard_health_drop = "hard" "health_drop",
    hard_heal = "hard" "heal",
    max_zombies = "zombies" "max",
    rise = "zombies" "rise",
    stagger = "zombies" "stagger",
    growth = "zombies" "growth",
    leader_hp = "zombies" "leader_hp",
    fury = "zombies" "fury",
    harvester_hp = "harvester" "hp",
    harvester_speed = "harvester" "speed",
    harvester_bite = "harvester" "bite",
    harvester_rise = "harvester" "rise",
    grenade_recharge = "grenade" "recharge",
    grenade_flight = "grenade" "flight",
    grenade_radius = "grenade" "radius",
    grenade_damage = "grenade" "damage",
    bomblet_radius = "grenade" "bomblet_radius",
    bomblet_damage = "grenade" "bomblet_damage",
    barrel_hp = "barrel" "hp",
    barrel_radius = "barrel" "radius",
    barrel_damage = "barrel" "damage",
    blast_hurt = "blast" "hurt",
    blast_near = "blast" "near",
}

/// Reads a `tuning.txt` over `base`, the numbers it doesn't set; with no base, it must set every one. Errors
/// name the line.
pub fn parse(text: &str, base: Option<&Tuning>) -> Result<Tuning, String> {
    let mut values: Vec<Option<f32>> = match base {
        Some(b) => b.values().into_iter().map(Some).collect(),
        None => vec![None; KEYS.len()],
    };
    let mut section = String::new();
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(s) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            if !KEYS.iter().any(|(k, _)| *k == s) {
                return Err(format!("{n}: [{s}] isn't a section"));
            }
            section = s.to_string();
            continue;
        }
        let (key, value) = line.split_once(char::is_whitespace).map_or((line, ""), |(k, v)| (k, v.trim()));
        let at = KEYS.iter().position(|(s, k)| *s == section && *k == key).ok_or_else(|| format!("{n}: {key} isn't a number [{section}] sets"))?;
        let v = value.parse::<f32>().ok().filter(|v| v.is_finite() && *v >= 0.0).ok_or_else(|| format!("{n}: {key} is a number from 0 up"))?;
        values[at] = Some(v);
    }
    if let Some(i) = values.iter().position(Option::is_none) {
        let (s, k) = KEYS[i];
        return Err(format!("[{s}] doesn't set {k}"));
    }
    Ok(Tuning::from_values(&values.into_iter().flatten().collect::<Vec<_>>()))
}

/// The game's own numbers, as built in, which a pack's `tuning.txt` sets over.
pub fn built_in() -> Tuning {
    parse(include_str!("../tuning.txt"), None).unwrap_or_else(|e| panic!("tuning.txt:{e}"))
}

/// The numbers the game plays by, read the first time they're asked for.
pub fn tuning() -> &'static Tuning {
    static T: OnceLock<Tuning> = OnceLock::new();
    T.get_or_init(|| match crate::data::tuning() {
        Some(text) => parse(text, Some(&built_in())).unwrap_or_else(|e| panic!("tuning.txt:{e}")),
        None => built_in(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_numbers_read_and_a_mod_sets_only_what_it_says() {
        let base = built_in();
        assert_eq!((base.player_hp, base.harvester_hp, base.normal_health_drop), (100.0, 2500.0, 200.0));
        let modded = parse("[player]\nhp 150", Some(&base)).unwrap();
        assert_eq!((modded.player_hp, modded.player_speed), (150.0, base.player_speed));
        assert!(parse("[player]\nhp 150", None).unwrap_err().contains("doesn't set"));
        assert!(parse("[space]\n", Some(&base)).unwrap_err().starts_with("1: [space]"));
        assert!(parse("[player]\nwings 2", Some(&base)).unwrap_err().starts_with("2: wings"));
        assert!(parse("[player]\nhp -1", Some(&base)).unwrap_err().contains("from 0 up"));
    }
}
