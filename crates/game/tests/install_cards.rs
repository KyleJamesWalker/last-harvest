//! Installs cards the way the app does at start-up, in a process of its own: installing reads the game's data
//! once and for good, so nothing on the way may read it first.

use last_harvest_game::data::{self, Card, Data};
use last_harvest_game::{Difficulty, kill_sounds, player_looks};

/// A card of 16-pixel frames, opaque all over, `walk` frames across its walking rows.
fn card(name: &str, text: &str, walk: u32) -> Card {
    let cols = walk.max(3);
    Card { name: name.into(), text: text.into(), frame: [16, 16], sheet: vec![255; (16 * cols * 16 * 5 * 4) as usize] }
}

#[test]
fn enemy_and_player_cards_install_and_reach_the_game() {
    let cards = vec![card("ghoul.enemy.png", "[ghoul]\nname Ghoul\nabout Quick and quiet.\nsound kill-ghoul\nwalk 5\n", 5)];
    let players = vec![card("jo.player.png", "[jo]\nname Jo\nabout Up with the sun.\nstart normal crows knife crows\nstart hard\n", 3)];
    let notes = data::install(Data { cards, players, ..Data::default() }).expect("installing cards leaves the data unread until it's done");
    assert!(notes.is_empty(), "{notes:?}");
    assert!(kill_sounds().any(|s| s == "kill-ghoul"), "the enemy card joins the roster");
    let jo = player_looks().iter().find(|l| l.id == "jo").expect("the player card joins the looks");
    assert_eq!(jo.about, "Up with the sun.");
    assert_eq!(jo.start_names(Difficulty::Normal), ["Crows", "Knife"]);
    assert!(jo.start_names(Difficulty::Hard).is_empty());
    assert_eq!(jo.start_names(Difficulty::Easy), ["Pistol", "Knife"], "a difficulty it doesn't set keeps the game's");
}
