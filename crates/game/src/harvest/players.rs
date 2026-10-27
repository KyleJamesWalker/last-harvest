//! How the player looks: the game's own farmer, or a player card's figure.

use std::sync::OnceLock;

use super::Look;
use super::enemies::{draw_cell, place};
use crate::art::Art;
use crate::data::Card;

/// The most a player card's id can be, so a save keeps it inline.
pub const ID_MAX: usize = 24;

pub struct PlayerLook {
    pub id: String,
    pub name: String,
    pub(crate) look: Look,
}

static LOOKS: OnceLock<Vec<PlayerLook>> = OnceLock::new();

fn farmer() -> PlayerLook {
    PlayerLook { id: String::new(), name: "Farmer".into(), look: super::player_look() }
}

/// The looks to choose from: the farmer first, then the player cards.
pub fn looks() -> &'static [PlayerLook] {
    LOOKS.get_or_init(|| vec![farmer()])
}

/// The look with `id`, or the farmer's when there's none.
pub fn look(id: &str) -> &'static PlayerLook {
    looks().iter().find(|l| l.id == id).unwrap_or(&looks()[0])
}

pub(super) fn install(cards: Vec<PlayerLook>) {
    LOOKS.set(std::iter::once(farmer()).chain(cards).collect()).ok();
}

/// A player card's id and name, from its text: an `[id]` line and a `name` line.
pub fn player_fields(text: &str) -> Result<(String, String), String> {
    let (mut id, mut name) = (None, None);
    for (i, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        match (line.strip_prefix('[').and_then(|l| l.strip_suffix(']')), line.split_once(char::is_whitespace)) {
            (Some(v), _) if id.is_none() => id = Some(v.to_string()),
            (None, Some(("name", v))) if id.is_some() => name = Some(v.trim().to_string()),
            _ => return Err(format!("line {}: a player card has its [id] and a name, nothing else", i + 1)),
        }
    }
    let id = id.ok_or("a player card starts with its [id]")?;
    if id.is_empty() || id.len() > ID_MAX || !id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
        return Err(format!("[{id}]: a player card's id is 1 to {ID_MAX} of a-z, 0-9 and _"));
    }
    let name = name.unwrap_or_else(|| id.clone());
    Ok((id, name))
}

/// The player cards' looks with their frames drawn into `atlas` from `cursor` on, numbered from `first`, the rects
/// drawn, and a line for each card left out.
pub(super) fn pack(cards: &[Card], atlas: &mut [u8], cursor: &mut [u16; 3], first: usize) -> (Vec<String>, Vec<[u16; 4]>, Vec<PlayerLook>) {
    let (mut notes, mut rects, mut looks): (Vec<String>, Vec<[u16; 4]>, Vec<PlayerLook>) = (Vec::new(), Vec::new(), Vec::new());
    for card in cards {
        let (id, name) = match player_fields(&card.text) {
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
        looks.push(PlayerLook { id, name, look });
    }
    (notes, rects, looks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::art::{ATLAS_H, ATLAS_W};

    #[test]
    fn a_player_card_reads_its_id_and_name_and_nothing_else() {
        assert_eq!(player_fields("[jo]\nname Jo the Farmer\n"), Ok(("jo".into(), "Jo the Farmer".into())));
        assert_eq!(player_fields("# mine\n[jo]"), Ok(("jo".into(), "jo".into())));
        assert!(player_fields("[jo]\nhp 5").is_err());
        assert!(player_fields("[Jo!]").is_err());
        assert!(player_fields("name Jo").is_err());
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
