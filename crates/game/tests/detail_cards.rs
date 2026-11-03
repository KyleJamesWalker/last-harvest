//! Installs detailed cards the way the app does at start-up, in a process of its own: installing reads the game's
//! data once and for good.

use last_harvest_game::art::{self, Art};
use last_harvest_game::data::{self, Card, Data};

/// A card of `side`-pixel frames at `detail`, opaque all over, three across and five down.
fn card(name: &str, text: &str, side: u32, detail: u8) -> Card {
    Card { detail, worn: String::new(), name: name.into(), text: text.into(), frame: [side, side], sheet: vec![255; (side * 3 * side * 5 * 4) as usize] }
}

#[test]
fn a_detailed_card_draws_its_full_pixels_at_a_plain_cards_size() {
    let cards = vec![card("ghost.enemy.png", "[ghost]\nname Ghost\n", 128, 2), card("big.enemy.png", "[big2]\nname Too big\n", 128, 1)];
    let players = vec![card("jo.player.png", "[jo]\nname Jo\n", 64, 4)];
    let notes = data::install(Data { cards, players, ..Data::default() }).unwrap();
    assert_eq!(notes.len(), 1, "128 pixels is past 64 game pixels at detail 1: {notes:?}");
    assert!(notes[0].starts_with("big.enemy.png"), "{notes:?}");
    // The player card's frames come last.
    let jo = Art((art::count() - 1) as u16);
    assert_eq!((jo.detail(), jo.rect()[2], jo.w(), jo.h()), (4, 64, 16.0, 16.0));
    let ghost = Art((art::count() - 16) as u16);
    assert_eq!((ghost.detail(), ghost.rect()[3], ghost.w()), (2, 128, 64.0));
}
