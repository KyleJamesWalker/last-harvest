//! How the player looks: the game's own farmer, or a player card's figure.

use std::sync::OnceLock;

use super::Look;
use super::enemies::{draw_cell, place};
use super::powerups::{Kind, book};
use crate::Difficulty;
use crate::art::Art;
use crate::data::Card;

/// The most a player card's id can be, so a save keeps it inline.
pub const ID_MAX: usize = 24;

pub struct PlayerLook {
    pub id: String,
    pub name: String,
    pub(crate) look: Look,
    /// The weapons a run starts with at each difficulty, by id; None keeps the game's. Ids, since the powerup
    /// table can't be read while the cards install.
    pub(crate) start: [Option<Vec<String>>; 3],
}

/// What a player card's text says: its id and name, and the weapons it starts with at each difficulty, by id;
/// None for a difficulty keeps the game's own.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerFields {
    pub id: String,
    pub name: String,
    pub start: [Option<Vec<String>>; 3],
}

/// Every weapon a player card can start with, by id and name.
pub fn weapons() -> Vec<(String, String)> {
    book().defs.iter().filter(|d| d.kind == Kind::Weapon).map(|d| (d.id.clone(), d.name.clone())).collect()
}

/// How many weapons a run can carry, and so a player card can start with.
pub fn weapon_slots() -> usize {
    book().weapon_slots
}

/// The weapons the game starts a run with at `difficulty`, by id.
pub fn default_start(difficulty: Difficulty) -> Vec<String> {
    book().start[difficulty as usize].iter().map(|&i| book().defs[i].id.clone()).collect()
}

/// A player card's text with its `start` line for `difficulty` set to `weapons`, or removed when None.
pub fn set_player_start(text: &str, difficulty: Difficulty, weapons: Option<&[String]>) -> String {
    let mine = |line: &str| {
        let words: Vec<&str> = line.split('#').next().unwrap_or("").split_whitespace().collect();
        words.first() == Some(&"start") && words.get(1) == Some(&difficulty.key())
    };
    let line = weapons.map(|w| format!("start       {:<6} {}", difficulty.key(), w.join(" ")).trim_end().to_string());
    let mut out: Vec<String> = text.lines().filter(|l| !mine(l)).map(str::to_string).collect();
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    out.extend(line);
    out.join("\n") + "\n"
}

static LOOKS: OnceLock<Vec<PlayerLook>> = OnceLock::new();

fn farmer() -> PlayerLook {
    PlayerLook { id: String::new(), name: "Farmer".into(), look: super::player_look(), start: Default::default() }
}

/// The looks to choose from: the farmer first, then the player cards.
pub fn looks() -> &'static [PlayerLook] {
    LOOKS.get_or_init(|| vec![farmer()])
}

/// The look with `id`, or the farmer's when there's none.
pub fn look(id: &str) -> &'static PlayerLook {
    looks().iter().find(|l| l.id == id).unwrap_or(&looks()[0])
}

impl PlayerLook {
    /// The weapons this look starts a run with at `difficulty`, by powerup, when it has its own: the weapons it
    /// names, as many as a run carries.
    pub(crate) fn start_defs(&self, difficulty: Difficulty) -> Option<Vec<usize>> {
        let ids = self.start[difficulty as usize].as_ref()?;
        let mut defs: Vec<usize> = Vec::new();
        for def in ids.iter().filter_map(|w| book().index(w)).filter(|&i| book().defs[i].kind == Kind::Weapon) {
            // A weapon named twice would sit in two slots, and the second could never level.
            if !defs.contains(&def) && defs.len() < book().weapon_slots {
                defs.push(def);
            }
        }
        Some(defs)
    }

    /// The names of the weapons this look starts a run with at `difficulty`: its own, else the game's.
    pub fn start_names(&self, difficulty: Difficulty) -> Vec<String> {
        let defs = self.start_defs(difficulty).unwrap_or_else(|| book().start[difficulty as usize].clone());
        defs.iter().map(|&i| book().defs[i].name.clone()).collect()
    }
}

pub(super) fn install(cards: Vec<PlayerLook>) {
    LOOKS.set(std::iter::once(farmer()).chain(cards).collect()).ok();
}

/// A player card's settings, from its text: an `[id]` line, a `name` line, and a `start DIFFICULTY WEAPON...` line
/// for any difficulty whose starting weapons it sets.
pub fn player_fields(text: &str) -> Result<PlayerFields, String> {
    parse(text, true)
}

/// Reads a player card's text, checking its weapons against the powerup table when `check` says to; installing
/// cards can't, since that comes before the table can be read.
fn parse(text: &str, check: bool) -> Result<PlayerFields, String> {
    let (mut id, mut name, mut start): (Option<String>, Option<String>, [Option<Vec<String>>; 3]) = (None, None, Default::default());
    for (i, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let n = i + 1;
        let words: Vec<&str> = line.split_whitespace().collect();
        match (line.strip_prefix('[').and_then(|l| l.strip_suffix(']')), &words[..]) {
            (Some(v), _) if id.is_none() => id = Some(v.to_string()),
            (None, ["name", ..]) if id.is_some() => name = Some(line["name".len()..].trim().to_string()),
            (None, ["start", d, weapons @ ..]) if id.is_some() => {
                let d = Difficulty::from_key(d).ok_or_else(|| format!("line {n}: start names easy, normal or hard first, not {d:?}"))?;
                if check {
                    if let Some(w) = weapons.iter().find(|w| book().index(w).is_none_or(|i| book().defs[i].kind != Kind::Weapon)) {
                        return Err(format!("line {n}: {w:?} isn't a weapon"));
                    }
                    if weapons.len() > book().weapon_slots {
                        return Err(format!("line {n}: a run carries at most {} weapons", book().weapon_slots));
                    }
                }
                start[d as usize] = Some(weapons.iter().map(|w| w.to_string()).collect());
            }
            _ => return Err(format!("line {n}: a player card has its [id], a name and start lines, nothing else")),
        }
    }
    let id = id.ok_or("a player card starts with its [id]")?;
    if id.is_empty() || id.len() > ID_MAX || !id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
        return Err(format!("[{id}]: a player card's id is 1 to {ID_MAX} of a-z, 0-9 and _"));
    }
    let name = name.unwrap_or_else(|| id.clone());
    Ok(PlayerFields { id, name, start })
}

/// The player cards' looks with their frames drawn into `atlas` from `cursor` on, numbered from `first`, the rects
/// drawn, and a line for each card left out.
pub(super) fn pack(cards: &[Card], atlas: &mut [u8], cursor: &mut [u16; 3], first: usize) -> (Vec<String>, Vec<[u16; 4]>, Vec<PlayerLook>) {
    let (mut notes, mut rects, mut looks): (Vec<String>, Vec<[u16; 4]>, Vec<PlayerLook>) = (Vec::new(), Vec::new(), Vec::new());
    for card in cards {
        let PlayerFields { id, name, start } = match parse(&card.text, false) {
            Ok(f) => f,
            Err(e) => {
                notes.push(format!("{}: {e}", card.name));
                continue;
            }
        };
        let [fw, fh] = card.frame;
        if !(1..=64).contains(&fw) || !(1..=64).contains(&fh) || card.sheet.len() != (fw * 3 * fh * 5 * 4) as usize {
            notes.push(format!("{}: frames are 1 to 64 pixels, 3 across and five down", card.name));
            continue;
        }
        if looks.iter().any(|l| l.id == id) {
            notes.push(format!("{}: another player card is [{id}] too", card.name));
            continue;
        }
        let cells: Vec<(u32, u32)> = (0..5).flat_map(|row| (0..3).map(move |col| (col, row))).collect();
        let mut try_cursor = *cursor;
        let Some(spots) = cells.iter().map(|_| place(&mut try_cursor, [fw as u16, fh as u16])).collect::<Option<Vec<_>>>() else {
            notes.push(format!("{}: no room left in the sprite texture", card.name));
            continue;
        };
        *cursor = try_cursor;
        let at = first + rects.len();
        for (&cell, spot) in cells.iter().zip(spots) {
            draw_cell(card, 3, cell, spot, atlas);
            rects.push([spot[0], spot[1], fw as u16, fh as u16]);
        }
        let art = |k: usize| Art((at + k) as u16);
        let look = Look { walk: std::array::from_fn(|f| (f * 3..f * 3 + 3).map(art).collect()), flash: [art(9), art(10), art(11)], hurt: [art(12), art(13), art(14)] };
        looks.push(PlayerLook { id, name, look, start });
    }
    (notes, rects, looks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art::{ATLAS_H, ATLAS_W};

    #[test]
    fn a_player_card_reads_its_id_and_name_and_nothing_else() {
        let f = player_fields("[jo]\nname Jo the Farmer\nstart normal crows knife\nstart hard\n").unwrap();
        assert_eq!((f.id.as_str(), f.name.as_str()), ("jo", "Jo the Farmer"));
        assert_eq!(f.start, [None, Some(vec!["crows".into(), "knife".into()]), Some(vec![])]);
        assert_eq!(player_fields("# mine\n[jo]").unwrap().name, "jo");
        assert!(player_fields("[jo]\nstart normal heal").is_err(), "only weapons");
        assert!(player_fields("[jo]\nstart nightmare knife").is_err());
        assert!(player_fields("[jo]\nhp 5").is_err());
        assert!(player_fields("[Jo!]").is_err());
        assert!(player_fields("name Jo").is_err());
    }

    #[test]
    fn start_lines_are_set_and_cleared_one_difficulty_at_a_time() {
        let text = "[jo]\nname Jo\nstart easy pistol\n";
        let text = set_player_start(text, Difficulty::Hard, Some(&["crows".to_string(), "dog".to_string()]));
        let text = set_player_start(&text, Difficulty::Easy, None);
        assert_eq!(text, "[jo]\nname Jo\nstart       hard   crows dog\n");
        let text = set_player_start(&text, Difficulty::Normal, Some(&[]));
        assert_eq!(player_fields(&text).unwrap().start, [None, Some(vec![]), Some(vec!["crows".into(), "dog".into()])]);
        assert_eq!(default_start(Difficulty::Easy), ["pistol", "knife"]);
        assert!(weapons().iter().any(|(id, name)| id == "rifle" && name == "Hunting rifle"));
    }

    #[test]
    fn a_start_keeps_each_weapon_once_and_only_weapons() {
        let start = Some(vec!["pistol".into(), "pistol".into(), "heal".into(), "knife".into(), "nope".into()]);
        let jo = PlayerLook { id: "jo".into(), name: "Jo".into(), look: super::super::player_look(), start: [None, start, None] };
        assert_eq!(jo.start_names(Difficulty::Normal), ["Pistol", "Knife"]);
    }

    #[test]
    fn a_look_without_its_own_start_names_the_games() {
        assert_eq!(looks()[0].start_names(Difficulty::Easy), ["Pistol", "Knife"]);
        assert!(looks()[0].start_names(Difficulty::Hard).is_empty());
    }

    #[test]
    fn player_cards_pack_after_the_others_and_skip_repeats() {
        let mut atlas = vec![0u8; (ATLAS_W * ATLAS_H * 4) as usize];
        let card = |text: &str| Card { name: "p.player.png".into(), text: text.into(), frame: [8, 8], sheet: vec![255; 3 * 8 * 5 * 8 * 4] };
        let mut cursor = super::super::enemies::start();
        let (notes, rects, looks) = pack(&[card("[jo]\nname Jo"), card("[jo]"), card("[x]\nhp 1")], &mut atlas, &mut cursor, 500);
        assert_eq!(notes.len(), 2, "{notes:?}");
        assert_eq!((rects.len(), looks.len()), (15, 1));
        assert_eq!(looks[0].look.walk[1][0], Art(503));
        assert_eq!(looks[0].look.hurt[2], Art(514));
        assert_eq!(look("nobody").name, "Farmer", "an unknown id falls back to the farmer");
    }
}
